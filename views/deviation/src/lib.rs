#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::types::{ProjectInfo, Severity};
use bindings::iderm::plugin::view::{
    AxisScale, Band, ChartMarker, ChartPoint, ChartSeries, SeriesRole, TableCell, TableView,
    ViewPrimitive, XyChartView,
};
use deviation_analyzer::{analyze, Point};

struct Component;

const POINT_COUNT: usize = 40;

/// reserved live-refresh token -- see `app.rs`'s `LIVE_TICK_COMMAND` on
/// the Core side. Each plugin keeps its own copy not sharing one
/// (not part of the WIT contract by design), same as `example-view-emc`.
const LIVE_TICK_COMMAND: &str = "\0tick";

/// `phase` drifts the general noise the same way `example-view-emc`'s
/// does (cosmetic ripple on the clean points), but also sweeps the
/// planted deviation's own magnitude from 0 to 15 and back -- the more
/// useful thing to prove live for *this* plugin specifically is the
/// analysis pipeline itself reacting, not just the curve wiggling: at
/// the low end the point is indistinguishable from ordinary noise (not
/// an outlier at all), at the high end it clearly is, so the table's
/// outlier count, the band width, and the marker's presence all
/// genuinely flip over time not merely jittering. `* 2.0`
/// (not e.g. `* 0.3`) so a full 0→15→0 sweep completes in ~21 ticks —
/// visible within a normal live-watching window at the default 1.0s
/// tick rate, not only after minutes.
fn dataset(phase: f64) -> Vec<Point> {
    (0..POINT_COUNT)
        .map(|i| {
            let x = i as f64;
            let predicted = 10.0 + 2.0 * x;
            let noise = 0.6 * (i as f64 * 0.9 + phase).sin();
            let outlier_extra = 7.5 + 7.5 * (phase * 2.0).sin();
            let measured = if i == 25 { predicted + outlier_extra } else { predicted + noise };
            Point { x, predicted, measured }
        })
        .collect()
}

fn encode_state(phase: f64) -> Vec<u8> {
    phase.to_le_bytes().to_vec()
}

fn decode_state(state: &[u8]) -> Result<f64, String> {
    let bytes: [u8; 8] =
        state.try_into().map_err(|_| format!("corrupt view-state: expected 8 bytes, got {}", state.len()))?;
    Ok(f64::from_le_bytes(bytes))
}

/// renders the `deviation-analyzer` crate's output through `xy-chart`'s
/// `bands` field (ADR-017) -- the direct proof that field serves a real
/// generic use, not just the EMC-specific one it first shipped with.
fn build_primitives(phase: f64) -> Vec<ViewPrimitive> {
    let data = dataset(phase);
    let result = analyze(&data);

    let predicted = ChartSeries {
        name: "Predicted".to_string(),
        role: SeriesRole::Average,
        points: data.iter().map(|p| ChartPoint { x: p.x, y: p.predicted }).collect(),
    };
    let measured = ChartSeries {
        name: "Measured".to_string(),
        role: SeriesRole::Measured,
        points: data.iter().map(|p| ChartPoint { x: p.x, y: p.measured }).collect(),
    };
    let band = Band {
        upper: data.iter().zip(&result.band_upper).map(|(p, &u)| ChartPoint { x: p.x, y: u }).collect(),
        lower: data.iter().zip(&result.band_lower).map(|(p, &l)| ChartPoint { x: p.x, y: l }).collect(),
        label: "confidence band (2σ, heuristic)".to_string(),
        status: None,
    };
    let markers = result
        .outliers
        .iter()
        .map(|&i| ChartMarker { x: data[i].x, y: data[i].measured, status: Severity::Warning })
        .collect();

    let chart = ViewPrimitive::XyChart(XyChartView {
        series: vec![predicted, measured],
        x_scale: AxisScale::Linear,
        y_scale: AxisScale::Linear,
        markers,
        cursor: None,
        bands: vec![band],
    });

    let table = ViewPrimitive::Table(TableView {
        headers: vec!["Metric".to_string(), "Value".to_string()],
        rows: vec![
            vec![
                TableCell { text: "Correlation".to_string(), status: None },
                TableCell { text: format!("{:.3}", result.correlation), status: None },
            ],
            vec![
                TableCell { text: "Outliers".to_string(), status: None },
                TableCell {
                    text: result.outliers.len().to_string(),
                    status: if result.outliers.is_empty() { None } else { Some(Severity::Warning) },
                },
            ],
        ],
    });

    vec![chart, table]
}

impl Guest for Component {
    fn view_id() -> String {
        "example-deviation-analyzer".to_string()
    }

    /// deliberately calls back into `host::log`, same round-trip proof
    /// every other example view-plugin's `init` already establishes.
    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        bindings::iderm::plugin::host::log(&format!(
            "example-view-deviation: init for project at {}",
            project.root_path
        ));
        Ok(encode_state(0.0))
    }

    fn render(state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        let phase = decode_state(&state)?;
        Ok(build_primitives(phase))
    }

    /// only the reserved live-refresh token is recognized -- this
    /// example has no other mode/vocabulary to offer, matching how
    /// `example-view-tla`'s non-`graph` commands are rejected.
    fn handle_command(state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        let phase = decode_state(&state)?;
        if command == LIVE_TICK_COMMAND {
            let next_phase = phase + 0.15;
            Ok((encode_state(next_phase), build_primitives(next_phase)))
        } else {
            Err(format!("unknown command: {command:?} (this view has no commands)"))
        }
    }
}

bindings::export!(Component with_types_in bindings);
