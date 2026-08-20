#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::types::{ProjectInfo, Severity};
use bindings::iderm::plugin::view::{
    AxisScale, ChartMarker, ChartPoint, ChartSeries, SeriesRole, TableCell, TableView, ViewPrimitive,
    XyChartView,
};
use deviation_analyzer::{analyze, Point};
use distribution_analysis::{empirical_cdf, histogram, linear_regression, predict};

struct Component;

const SAMPLE_COUNT: usize = 500;
const BIN_COUNT: usize = 20;
const SEED: u64 = 0x2026_0807;

/// reserved live-refresh token -- see `app.rs`'s `LIVE_TICK_COMMAND` on
/// the Core side. Each plugin keeps its own copy not sharing one
/// (not part of the WIT contract by design), same as every other
/// live-capable example plugin.
const LIVE_TICK_COMMAND: &str = "\0tick";

fn next_uniform(state: &mut u64) -> f64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    ((*state >> 11) as f64 * (1.0 / (1u64 << 53) as f64)).max(1e-12)
}

/// box-Muller over a plain xorshift64* stream -- real (approximately)
/// gaussian samples without a `rand` dependency, same technique
/// `example-doctor-distribution` uses. `seed` (not just `mean`) is
/// perturbed by `phase` on each live tick, so a fresh batch actually
/// gets redrawn not the same shape sliding under a moving mean.
fn gaussian_samples(seed: u64, n: usize, mean: f64, stddev: f64) -> Vec<f64> {
    let mut state = seed;
    (0..n)
        .map(|_| {
            let u1 = next_uniform(&mut state);
            let u2 = next_uniform(&mut state);
            let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
            mean + stddev * z
        })
        .collect()
}

fn median(samples: &[f64]) -> f64 {
    empirical_cdf(samples).iter().find(|p| p.cumulative_fraction >= 0.5).map(|p| p.x).unwrap()
}

fn encode_state(phase: f64) -> Vec<u8> {
    phase.to_le_bytes().to_vec()
}

fn decode_state(state: &[u8]) -> Result<f64, String> {
    let bytes: [u8; 8] =
        state.try_into().map_err(|_| format!("corrupt view-state: expected 8 bytes, got {}", state.len()))?;
    Ok(f64::from_le_bytes(bytes))
}

/// renders `distribution-analysis`'s histogram + empirical CDF through
/// one `xy-chart` (both series share the same `[0, ~1]`-ish y-range, a
/// standard combined PDF/CDF plot), and a regression trend fit -- via
/// `linear_regression`/`predict` feeding straight into
/// `deviation_analyzer::analyze`, this crate's own module doc's "regression/
/// fit overlay from the Deviation Analyzer" -- summarized in the table
/// not drawn as a second chart, since a trend fit's own axes
/// (draw index vs. value) don't share meaning with the histogram's
/// (sample value vs. density/fraction).
fn build_primitives(phase: f64) -> Vec<ViewPrimitive> {
    let seed = SEED.wrapping_add((phase * 1000.0) as u64);
    let samples = gaussian_samples(seed, SAMPLE_COUNT, 50.0, 8.0);
    let bins = histogram(&samples, BIN_COUNT);
    let cdf = empirical_cdf(&samples);
    let med = median(&samples);

    // two points per bin -- `(x_start, density)` then `(x_end, density)`
    // -- connecting consecutive bins draws a real histogram staircase,
    // same technique `emc`'s step-shaped `Limit` series
    // already uses for its own flat-then-jump shape.
    let mut pdf_points = Vec::with_capacity(bins.len() * 2);
    for b in &bins {
        pdf_points.push(ChartPoint { x: b.x_start, y: b.density });
        pdf_points.push(ChartPoint { x: b.x_end, y: b.density });
    }
    let pdf = ChartSeries { name: "PDF (histogram)".to_string(), role: SeriesRole::Other, points: pdf_points };

    let cdf_points: Vec<ChartPoint> = cdf.iter().map(|p| ChartPoint { x: p.x, y: p.cumulative_fraction }).collect();
    let cdf_series = ChartSeries { name: "CDF".to_string(), role: SeriesRole::Measured, points: cdf_points };

    let median_marker = ChartMarker { x: med, y: 0.5, status: Severity::Info };

    let chart = ViewPrimitive::XyChart(XyChartView {
        series: vec![pdf, cdf_series],
        x_scale: AxisScale::Linear,
        y_scale: AxisScale::Linear,
        markers: vec![median_marker],
        cursor: None,
        bands: vec![],
    });

    let trend_points: Vec<(f64, f64)> = samples.iter().enumerate().map(|(i, &s)| (i as f64, s)).collect();
    let fit = linear_regression(&trend_points);
    let dev_points: Vec<Point> = trend_points
        .iter()
        .map(|&(x, y)| Point { x, predicted: predict(&fit, x), measured: y })
        .collect();
    let trend = analyze(&dev_points);

    let table = ViewPrimitive::Table(TableView {
        headers: vec!["Metric".to_string(), "Value".to_string()],
        rows: vec![
            vec![
                TableCell { text: "Samples".to_string(), status: None },
                TableCell { text: SAMPLE_COUNT.to_string(), status: None },
            ],
            vec![
                TableCell { text: "Median".to_string(), status: None },
                TableCell { text: format!("{med:.2}"), status: None },
            ],
            vec![
                TableCell { text: "Trend fit (slope, R²)".to_string(), status: None },
                TableCell { text: format!("{:.4}, {:.3}", fit.slope, fit.r_squared), status: None },
            ],
            vec![
                TableCell { text: "Trend outliers".to_string(), status: None },
                TableCell {
                    text: trend.outliers.len().to_string(),
                    status: if trend.outliers.is_empty() { None } else { Some(Severity::Warning) },
                },
            ],
        ],
    });

    vec![chart, table]
}

impl Guest for Component {
    fn view_id() -> String {
        "distribution".to_string()
    }

    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        bindings::iderm::plugin::host::log(&format!(
            "distribution: init for project at {}",
            project.root_path
        ));
        Ok(encode_state(0.0))
    }

    fn render(state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        let phase = decode_state(&state)?;
        Ok(build_primitives(phase))
    }

    /// only the reserved live-refresh token is recognized -- this
    /// example has no other command vocabulary, matching every other
    /// live-capable example plugin.
    fn handle_command(state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        let phase = decode_state(&state)?;
        if command == LIVE_TICK_COMMAND {
            let next_phase = phase + 1.0;
            Ok((encode_state(next_phase), build_primitives(next_phase)))
        } else {
            Err(format!("unknown command: {command:?} (this view has no commands)"))
        }
    }
}

bindings::export!(Component with_types_in bindings);
