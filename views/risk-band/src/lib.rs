#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::types::ProjectInfo;
use bindings::iderm::plugin::view::{
    AxisScale, Band, ChartPoint, ChartSeries, SeriesRole, TableCell, TableView, ViewPrimitive, XyChartView,
};
use distribution_analysis::percentile;

struct Component;

const STEPS: usize = 40;
const PATHS: usize = 300;
const START_VALUE: f64 = 100.0;
const DRIFT: f64 = 0.5;
const STEP_STDDEV: f64 = 3.0;
const BASE_SEED: u64 = 0x2026_0808;

/// reserved live-refresh token -- see `app.rs`'s `LIVE_TICK_COMMAND` on
/// the Core side. Each plugin keeps its own copy not sharing
/// one (not part of the WIT contract by design), same as every other
/// live-capable example plugin.
const LIVE_TICK_COMMAND: &str = "\0tick";

fn next_uniform(state: &mut u64) -> f64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    ((*state >> 11) as f64 * (1.0 / (1u64 << 53) as f64)).max(1e-12)
}

/// box-Muller over a plain xorshift64* stream -- same deterministic,
/// `rand`-free technique `example-view-distribution` already uses.
fn gaussian(state: &mut u64) -> f64 {
    let u1 = next_uniform(state);
    let u2 = next_uniform(state);
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// one simulated path across `STEPS` -- a plain Gaussian random walk
/// (`START_VALUE`, then `DRIFT + STEP_STDDEV`-scaled steps), a stand-in
/// for the "Price/ROI-style" Monte Carlo projection
/// `Workbenches.md`'s Stochastic/flow-simulation tooling entry names --
/// not a claim about real financial modeling (no volatility clustering,
/// no fat tails), same "engineering-sandbox framing, not a domain
/// claim" instinct the Emergence/discrete-simulation category already
/// states explicitly for its own visualizer.
fn simulate_path(seed: u64) -> Vec<f64> {
    let mut state = seed;
    let mut value = START_VALUE;
    let mut path = Vec::with_capacity(STEPS);
    for _ in 0..STEPS {
        value += DRIFT + STEP_STDDEV * gaussian(&mut state);
        path.push(value);
    }
    path
}

/// `PATHS` independent walks, one ensemble slice per time step --
/// `ensemble[t]` is every path's value at step `t`, exactly what
/// `percentile` needs to compute a P10/P50/P90 cross-section at that
/// step.
fn simulate_ensemble(phase: f64) -> Vec<Vec<f64>> {
    let seed_offset = (phase * 1000.0) as u64;
    let paths: Vec<Vec<f64>> =
        (0..PATHS).map(|p| simulate_path(BASE_SEED.wrapping_add(seed_offset).wrapping_add(p as u64))).collect();

    (0..STEPS).map(|t| paths.iter().map(|path| path[t]).collect()).collect()
}

/// renders a P10/P50/P90 risk-band fan chart -- `distribution-analysis`'s
/// `percentile` per time step, P50 as the central `xy-chart` series and
/// P10/P90 as a `Band` (ADR-017's `bands` field), the same combination
/// `example-view-deviation`'s confidence band already proved for a
/// different statistic (stddev-based, not percentile-based).
fn build_primitives(phase: f64) -> Vec<ViewPrimitive> {
    let ensemble = simulate_ensemble(phase);

    let p10: Vec<f64> = ensemble.iter().map(|s| percentile(s, 0.10)).collect();
    let p50: Vec<f64> = ensemble.iter().map(|s| percentile(s, 0.50)).collect();
    let p90: Vec<f64> = ensemble.iter().map(|s| percentile(s, 0.90)).collect();

    let median_series = ChartSeries {
        name: "P50 (median)".to_string(),
        role: SeriesRole::Average,
        points: p50.iter().enumerate().map(|(t, &v)| ChartPoint { x: t as f64, y: v }).collect(),
    };
    let band = Band {
        upper: p90.iter().enumerate().map(|(t, &v)| ChartPoint { x: t as f64, y: v }).collect(),
        lower: p10.iter().enumerate().map(|(t, &v)| ChartPoint { x: t as f64, y: v }).collect(),
        label: "P10-P90 risk band".to_string(),
        status: None,
    };

    let chart = ViewPrimitive::XyChart(XyChartView {
        series: vec![median_series],
        x_scale: AxisScale::Linear,
        y_scale: AxisScale::Linear,
        markers: vec![],
        cursor: None,
        bands: vec![band],
    });

    let last = STEPS - 1;
    let table = ViewPrimitive::Table(TableView {
        headers: vec!["Metric".to_string(), "Value".to_string()],
        rows: vec![
            vec![
                TableCell { text: "Paths".to_string(), status: None },
                TableCell { text: PATHS.to_string(), status: None },
            ],
            vec![
                TableCell { text: "Final P10".to_string(), status: None },
                TableCell { text: format!("{:.1}", p10[last]), status: None },
            ],
            vec![
                TableCell { text: "Final P50".to_string(), status: None },
                TableCell { text: format!("{:.1}", p50[last]), status: None },
            ],
            vec![
                TableCell { text: "Final P90".to_string(), status: None },
                TableCell { text: format!("{:.1}", p90[last]), status: None },
            ],
        ],
    });

    vec![chart, table]
}

fn encode_state(phase: f64) -> Vec<u8> {
    phase.to_le_bytes().to_vec()
}

fn decode_state(state: &[u8]) -> Result<f64, String> {
    let bytes: [u8; 8] =
        state.try_into().map_err(|_| format!("corrupt view-state: expected 8 bytes, got {}", state.len()))?;
    Ok(f64::from_le_bytes(bytes))
}

impl Guest for Component {
    fn view_id() -> String {
        "example-risk-band".to_string()
    }

    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        bindings::iderm::plugin::host::log(&format!(
            "example-view-risk-band: init for project at {}",
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
