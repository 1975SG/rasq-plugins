#[allow(warnings)]
mod bindings;

use analog_waveform::{parse, Capture};
use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::host;
use bindings::iderm::plugin::types::{ProjectInfo, Severity};
use bindings::iderm::plugin::view::{
    AxisScale, Band, ChartMarker, ChartPoint, ChartSeries, SeriesRole, TableCell, TableView, ViewPrimitive,
    XyChartView,
};

struct Component;

/// reserved live-refresh token -- see `app.rs`'s `LIVE_TICK_COMMAND`.
/// each plugin keeps its own copy, not part of the WIT contract.
const LIVE_TICK_COMMAND: &str = "\0tick";

/// recognizes a capture by its real channel shape (`depth,wave_height,
/// wavelength`, as `stream_wave.py` writes), not by filename -- any
/// project could name the file differently, and content is the honest
/// signal of "this is wave-shoaling data," not a naming convention.
fn is_wave_capture(capture: &Capture) -> bool {
    capture.channel_names.len() == 3
        && capture.channel_names[0] == "depth"
        && capture.channel_names[1] == "wave_height"
        && capture.channel_names[2] == "wavelength"
}

fn load_capture() -> Result<Option<Capture>, String> {
    for path in host::list_files("**/*.analog-capture").unwrap_or_default() {
        let contents = host::read_file(&path)?;
        let capture = parse(&contents)?;
        if is_wave_capture(&capture) {
            return Ok(Some(capture));
        }
    }
    Ok(None)
}

/// caps how many of the most recent rows `current_lap_start` scans and
/// `build_primitives` charts. `load_capture` re-reads and re-parses the
/// whole growing capture file every tick by design; without this cap,
/// the forward scan below, Core's per-frame axis-autoscale fold, and
/// the terminal Canvas's rasterization all grow with total session
/// length, at whatever the live-refresh tick rate is -- a genuine UI
/// freeze confirmed by real dogfooding, not just a theoretical
/// concern. 2000 rows comfortably covers many laps of any real profile
/// length while keeping worst-case per-tick cost bounded regardless of
/// how long the stream has been running.
const MAX_SCANNED_ROWS: usize = 2000;

/// profile is a fixed, deterministic sequence that decreases
/// (offshore -> nearshore) each lap, then jumps back up to repeat —
/// finds where the *current* (possibly still-filling) lap began,
/// not assuming any fixed per-lap sample count belonging to
/// one particular source script. A plain forward scan over the
/// (already `MAX_SCANNED_ROWS`-windowed) input is cheap enough for a
/// live demo session's real row counts.
fn current_lap_start(depths: &[f64]) -> usize {
    let mut start = 0;
    for i in 1..depths.len() {
        if depths[i] > depths[i - 1] {
            start = i;
        }
    }
    start
}

/// renders the seafloor as an actual descending-then-flattening line
/// (`y = -depth`, so it visually sits below a calm-sea-level
/// reference) and the wave itself as a `Band` (ADR-017) hugging that
/// reference -- narrow offshore, visibly widening as the real shoaling
/// coefficient grows toward shore, the same physical picture a real
/// cross-section diagram would show. `x` is distance-into-the-profile
/// (`max depth in this lap - current depth`), not raw depth, so the
/// chart reads left-to-right as "toward the beach" not
/// decreasing.
fn build_primitives(capture: &Capture) -> Vec<ViewPrimitive> {
    let scan_start = capture.channels[0].len().saturating_sub(MAX_SCANNED_ROWS);
    let depths = &capture.channels[0][scan_start..];
    let heights = &capture.channels[1][scan_start..];
    let wavelengths = &capture.channels[2][scan_start..];

    let start = current_lap_start(depths);
    let depths = &depths[start..];
    let heights = &heights[start..];
    let wavelengths = &wavelengths[start..];

    let max_depth = depths.iter().copied().filter(|v| v.is_finite()).fold(f64::MIN, f64::max);

    let seafloor: Vec<ChartPoint> =
        depths.iter().filter(|d| d.is_finite()).map(|&d| ChartPoint { x: max_depth - d, y: -d }).collect();

    let band_upper: Vec<ChartPoint> = depths
        .iter()
        .zip(heights)
        .filter(|(d, h)| d.is_finite() && h.is_finite())
        .map(|(&d, &h)| ChartPoint { x: max_depth - d, y: h / 2.0 })
        .collect();
    let band_lower: Vec<ChartPoint> = depths
        .iter()
        .zip(heights)
        .filter(|(d, h)| d.is_finite() && h.is_finite())
        .map(|(&d, &h)| ChartPoint { x: max_depth - d, y: -h / 2.0 })
        .collect();

    let marker = depths.last().copied().filter(|d| d.is_finite()).map(|d| ChartMarker {
        x: max_depth - d,
        y: 0.0,
        status: Severity::Info,
    });

    let chart = ViewPrimitive::XyChart(XyChartView {
        series: vec![ChartSeries { name: "seafloor".to_string(), role: SeriesRole::Other, points: seafloor }],
        x_scale: AxisScale::Linear,
        y_scale: AxisScale::Linear,
        markers: marker.into_iter().collect(),
        cursor: None,
        bands: vec![Band { upper: band_upper, lower: band_lower, label: "wave envelope".to_string(), status: None }],
    });

    let last_depth = depths.last().copied().unwrap_or(f64::NAN);
    let last_height = heights.last().copied().unwrap_or(f64::NAN);
    let last_wavelength = wavelengths.last().copied().unwrap_or(f64::NAN);

    let table = ViewPrimitive::Table(TableView {
        headers: vec!["Reading (current position)".to_string(), "Value".to_string()],
        rows: vec![
            vec![
                TableCell { text: "Depth".to_string(), status: None },
                TableCell { text: format!("{last_depth:.3} m"), status: None },
            ],
            vec![
                TableCell { text: "Wave height".to_string(), status: None },
                TableCell { text: format!("{last_height:.3} m"), status: None },
            ],
            vec![
                TableCell { text: "Wavelength".to_string(), status: None },
                TableCell { text: format!("{last_wavelength:.3} m"), status: None },
            ],
        ],
    });

    vec![chart, table]
}

fn load_primitives() -> Result<Vec<ViewPrimitive>, String> {
    match load_capture()? {
        Some(capture) => Ok(build_primitives(&capture)),
        None => Ok(vec![ViewPrimitive::Table(TableView {
            headers: vec!["".to_string()],
            rows: vec![vec![TableCell {
                text: "no wave-shoaling capture (depth,wave_height,wavelength) found in this project".to_string(),
                status: None,
            }]],
        })]),
    }
}

impl Guest for Component {
    fn view_id() -> String {
        "wave-shoaling".to_string()
    }

    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        host::log(&format!("wave-shoaling: init for project at {}", project.root_path));
        Ok(Vec::new())
    }

    fn render(_state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        load_primitives()
    }

    /// purely derived from the capture file on every call -- no session
    /// state of its own, unlike `analog-capture`'s manual
    /// points/stream pin. Only the reserved live-refresh token is
    /// recognized; this view has no other command vocabulary.
    fn handle_command(state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        if command == LIVE_TICK_COMMAND {
            Ok((state, load_primitives()?))
        } else {
            Err(format!("unknown command: {command:?} (this view has no commands, just the F5 tick)"))
        }
    }
}

bindings::export!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_primitives_bounds_seafloor_points_to_max_scanned_rows_regardless_of_capture_length() {
        // many short laps (10 points each), well past MAX_SCANNED_ROWS
        // in total -- a long-running session's real shape.
        let lap: Vec<f64> = (0..10).map(|i| 30.0 - i as f64 * 3.0).collect();
        let laps = (MAX_SCANNED_ROWS / lap.len()) + 5;
        let depths: Vec<f64> = (0..laps).flat_map(|_| lap.clone()).collect();
        let n = depths.len();
        assert!(n > MAX_SCANNED_ROWS, "test setup should exceed the scan window");
        let heights = vec![1.0; n];
        let wavelengths = vec![50.0; n];
        let capture = Capture {
            time_label: "time".to_string(),
            channel_names: vec!["depth".to_string(), "wave_height".to_string(), "wavelength".to_string()],
            times: (0..n).map(|i| i as f64).collect(),
            channels: vec![depths, heights, wavelengths],
        };
        let primitives = build_primitives(&capture);
        let ViewPrimitive::XyChart(chart) = &primitives[0] else { panic!("expected a chart primitive first") };
        // seafloor series can be at most one lap's worth of points
        // (bounded by the window, never by the capture's total length).
        assert!(chart.series[0].points.len() <= lap.len());
    }

    #[test]
    fn current_lap_start_finds_the_most_recent_restart() {
        // two laps: 30 -> 20 (partial), restart, 30 -> 10.
        let depths = vec![30.0, 25.0, 20.0, 30.0, 22.0, 15.0, 10.0];
        assert_eq!(current_lap_start(&depths), 3);
    }

    #[test]
    fn current_lap_start_is_zero_within_a_single_lap() {
        let depths = vec![30.0, 25.0, 20.0, 15.0, 10.0];
        assert_eq!(current_lap_start(&depths), 0);
    }

    #[test]
    fn current_lap_start_handles_a_single_sample() {
        assert_eq!(current_lap_start(&[30.0]), 0);
        assert_eq!(current_lap_start(&[]), 0);
    }
}
