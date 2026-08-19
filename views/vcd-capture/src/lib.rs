#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::host;
use bindings::iderm::plugin::types::ProjectInfo;
use bindings::iderm::plugin::view::{
    AxisScale, ChartPoint, ChartSeries, SeriesRole, TableCell, TableView, ViewPrimitive, XyChartView,
};
use vcd_parser::{parse, VcdFile};

struct Component;

/// reserved live-refresh token -- see `example-view-emc`'s own copy of
/// this constant for the full explanation. Was missing here entirely
/// until ADR-057, on the same now-corrected assumption `example-view-
/// analog-capture` had (ADR-055): a `.vcd` file was assumed to be a
/// completed capture by the time anything opens it, and "live capture
/// is a separate not-yet-designed mechanism" -- but that mechanism
/// (ADR-030, `.iderm/tasks.toml` continuous tasks) already landed, and
/// is exactly what a real logic analyzer (`sigrok-cli` writing VCD)
/// would use to feed this plugin live.
const LIVE_TICK_COMMAND: &str = "\0tick";

/// stacked-channel logic-analyzer convention: signal `i` occupies
/// `[offset(i), offset(i)+1]` on the y-axis, with a gap between
/// channels so adjacent waveforms don't visually touch.
const CHANNEL_SPACING: f64 = 2.0;
/// more than this many scalar channels on one stacked chart stops
/// being readable at typical terminal heights -- the rest still appear
/// in the summary `Table` below, just not drawn.
const MAX_CHARTED_SIGNALS: usize = 8;

/// one signal's timeline as a step-shaped waveform -- two points per
/// held level (`t_start` and the next change's own time, same value),
/// the identical technique `example-view-checksum-inspector`'s
/// histogram staircase and `example-view-emc`'s step-shaped `Limit`
/// series already use for a flat-then-jump shape. `x`/`z`/unknown
/// values are drawn at the low level -- `vcd_parser::VcdFile::
/// has_unknown_or_high_z` is what actually flags their presence, this
/// is only about where the line sits, not a claim they mean `0`.
fn step_points(timeline: &[(u64, &str)], duration: u64, offset: f64) -> Vec<ChartPoint> {
    let mut points = Vec::with_capacity(timeline.len() * 2);
    for (i, &(t, v)) in timeline.iter().enumerate() {
        let level = if v == "1" { 1.0 } else { 0.0 };
        let next_t = timeline.get(i + 1).map(|&(t2, _)| t2).unwrap_or(duration.max(t));
        points.push(ChartPoint { x: t as f64, y: offset + level });
        points.push(ChartPoint { x: next_t as f64, y: offset + level });
    }
    points
}

fn build_primitives(vcd: &VcdFile) -> Vec<ViewPrimitive> {
    let duration = vcd.duration();
    let scalar_signals: Vec<_> = vcd.signals.iter().filter(|s| s.width == 1).take(MAX_CHARTED_SIGNALS).collect();

    let series = scalar_signals
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let timeline = vcd.timeline(&s.id);
            ChartSeries {
                name: s.name.clone(),
                role: SeriesRole::Other,
                points: step_points(&timeline, duration, i as f64 * CHANNEL_SPACING),
            }
        })
        .collect();

    let chart = ViewPrimitive::XyChart(XyChartView {
        series,
        x_scale: AxisScale::Linear,
        y_scale: AxisScale::Linear,
        markers: vec![],
        cursor: None,
        bands: vec![],
    });

    let rows = vcd
        .signals
        .iter()
        .map(|s| {
            let changes = vcd.changes.iter().filter(|c| c.id == s.id).count();
            vec![
                TableCell { text: s.name.clone(), status: None },
                TableCell { text: format!("{} bit(s)", s.width), status: None },
                TableCell { text: changes.to_string(), status: None },
            ]
        })
        .collect();
    let table = ViewPrimitive::Table(TableView {
        headers: vec!["Signal".to_string(), "Width".to_string(), "Transitions".to_string()],
        rows,
    });

    vec![chart, table]
}

/// shared by `render` and the tick branch of `handle_command` -- both
/// need exactly the same "find the capture file, read it fresh,
/// re-parse it" round trip. Re-reading from scratch every call rather
/// than tracking a byte offset is deliberate, same trade-off
/// `example-view-analog-capture` already made: real VCD captures here
/// are small enough that honest-but-slightly-wasteful is the right
/// call for now.
fn load_primitives() -> Result<Vec<ViewPrimitive>, String> {
    let Some(path) = host::list_files("**/*.vcd").unwrap_or_default().into_iter().next() else {
        return Ok(vec![ViewPrimitive::Table(TableView {
            headers: vec!["".to_string()],
            rows: vec![vec![TableCell { text: "no .vcd capture file found in this project".to_string(), status: None }]],
        })]);
    };
    let contents = host::read_file(&path)?;
    let vcd = parse(&contents)?;
    Ok(build_primitives(&vcd))
}

impl Guest for Component {
    fn view_id() -> String {
        "example-vcd-capture".to_string()
    }

    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        host::log(&format!("example-view-vcd-capture: init for project at {}", project.root_path));
        Ok(Vec::new())
    }

    fn render(_state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        load_primitives()
    }

    /// live-refresh (ADR-057) re-reads and re-renders the capture file
    /// fresh on every tick -- real signal for a `.vcd` a still-running
    /// `.iderm/tasks.toml` continuous task (a real logic analyzer via
    /// `sigrok-cli`, or the `vcd-parser` emulator) can still be
    /// appending to. Any other command stays rejected: this plugin has
    /// no other vocabulary of its own, only the tick.
    fn handle_command(state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        if command == LIVE_TICK_COMMAND {
            return Ok((state, load_primitives()?));
        }
        Err(format!("unknown command: {command:?} (this view only supports live-refresh, F5)"))
    }
}

bindings::export!(Component with_types_in bindings);
