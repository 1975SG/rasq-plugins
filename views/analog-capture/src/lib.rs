#[allow(warnings)]
mod bindings;

use analog_waveform::{parse, Capture};
use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::host;
use bindings::iderm::plugin::types::ProjectInfo;
use bindings::iderm::plugin::view::{
    AxisScale, ChartPoint, ChartSeries, SeriesRole, TableCell, TableView, ViewPrimitive, XyChartView,
};

struct Component;

/// more channels than this on one chart stops being readable at
/// typical terminal widths -- the rest still appear in the summary
/// `Table` below, same "chart what's readable, list the rest"
/// convention `vcd-capture` already uses for its own
/// channel cap.
const MAX_CHARTED_CHANNELS: usize = 4;

/// caps how many of a channel's most recent samples get charted per
/// tick. `load_capture` re-reads and re-parses the whole growing
/// capture file every call by design (see its own doc comment) -- fine
/// for the file itself, but without this cap the chart's own point
/// count, Core's per-frame axis-autoscale fold, and the terminal
/// canvas's rasterization all grow with total session length too, at
/// whatever the live-refresh tick rate is. Real dogfooding against a
/// long-running stream showed this as a genuine UI freeze, not just a
/// theoretical concern. 500 points is already past a typical terminal's
/// horizontal resolution, so this is invisible in practice.
const MAX_PLOTTED_POINTS: usize = 500;

/// reserved live-refresh token `App::tick_view` sends every
/// `tick_interval` while `F5` live mode is on -- see `emc`'s own copy
/// of this same constant for the full explanation.
/// was missing here entirely until ADR-055: this plugin used to
/// reject every command including this one, on the assumption a
/// `.analog-capture` file is a finished record by the time anything
/// opens it. Real hardware streamed into iderm live (a growing
/// capture written by a running `.iderm/tasks.toml` continuous task,
/// not a completed one) broke that assumption -- caught by actually
/// dogfooding it against a real USB audio interface, not found by
/// inspection.
const LIVE_TICK_COMMAND: &str = "\0tick";

/// manually-typed data point (ADR-056) -- `value: <channel> <x> <y>`,
/// entirely session-only. Lives in the plugin's own opaque `view-state`
/// the same way `emc`'s cursor/phase already does; never
/// written to disk. Rendered as its own distinct series/rows rather
/// than spliced into the real capture's aligned sample arrays, so a
/// manual annotation always reads as visibly separate from what the
/// running capture task actually measured.
struct ManualPoint {
    channel: String,
    x: f64,
    y: f64,
}

/// full session-only view-state: which capture file this pane instance
/// is pinned to (ADR-058 -- `stream: <filename>`), plus the manual
/// points from ADR-056. Was manual-points-only until multiple real
/// capture files started coexisting in one project (three simultaneous
/// hardware bridges -- eth/wifi/bt -- each writing their own
/// `.analog-capture` file): `load_capture`'s old "take whatever
/// `list_files` returns first" picked the same alphabetical winner
/// (`bt.analog-capture`) for every pane regardless of what was
/// actually wanted, so a second pane pointed at `eth` showed Bluetooth
/// data instead. `stream_filter: None` preserves that exact old
/// first-match behavior for anyone with only one capture file, which
/// is still the common case.
struct ViewState {
    stream_filter: Option<String>,
    manual: Vec<ManualPoint>,
}

fn read_u32(bytes: &[u8], pos: &mut usize) -> Result<u32, String> {
    let slice = bytes.get(*pos..*pos + 4).ok_or_else(|| "corrupt view-state: truncated".to_string())?;
    *pos += 4;
    Ok(u32::from_le_bytes(slice.try_into().unwrap()))
}

fn read_f64(bytes: &[u8], pos: &mut usize) -> Result<f64, String> {
    let slice = bytes.get(*pos..*pos + 8).ok_or_else(|| "corrupt view-state: truncated".to_string())?;
    *pos += 8;
    Ok(f64::from_le_bytes(slice.try_into().unwrap()))
}

fn read_string(bytes: &[u8], pos: &mut usize) -> Result<String, String> {
    let len = read_u32(bytes, pos)? as usize;
    let slice = bytes.get(*pos..*pos + len).ok_or_else(|| "corrupt view-state: truncated string".to_string())?;
    *pos += len;
    String::from_utf8(slice.to_vec()).map_err(|_| "corrupt view-state: invalid utf8".to_string())
}

fn write_string(bytes: &mut Vec<u8>, s: &str) {
    let s_bytes = s.as_bytes();
    bytes.extend_from_slice(&(s_bytes.len() as u32).to_le_bytes());
    bytes.extend_from_slice(s_bytes);
}

fn encode_state(state: &ViewState) -> Vec<u8> {
    let mut bytes = Vec::new();
    match &state.stream_filter {
        Some(name) => {
            bytes.push(1);
            write_string(&mut bytes, name);
        }
        None => bytes.push(0),
    }
    bytes.extend_from_slice(&(state.manual.len() as u32).to_le_bytes());
    for p in &state.manual {
        write_string(&mut bytes, &p.channel);
        bytes.extend_from_slice(&p.x.to_le_bytes());
        bytes.extend_from_slice(&p.y.to_le_bytes());
    }
    bytes
}

fn decode_state(state: &[u8]) -> Result<ViewState, String> {
    if state.is_empty() {
        return Ok(ViewState { stream_filter: None, manual: Vec::new() });
    }
    let mut pos = 0usize;
    let has_filter = *state.get(pos).ok_or_else(|| "corrupt view-state: truncated".to_string())?;
    pos += 1;
    let stream_filter = if has_filter == 1 { Some(read_string(state, &mut pos)?) } else { None };

    let count = read_u32(state, &mut pos)?;
    let mut manual = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let channel = read_string(state, &mut pos)?;
        let x = read_f64(state, &mut pos)?;
        let y = read_f64(state, &mut pos)?;
        manual.push(ManualPoint { channel, x, y });
    }
    Ok(ViewState { stream_filter, manual })
}

fn channel_stats(samples: &[f64]) -> (f64, f64, f64) {
    let finite: Vec<f64> = samples.iter().copied().filter(|v| !v.is_nan()).collect();
    if finite.is_empty() {
        return (f64::NAN, f64::NAN, f64::NAN);
    }
    let min = finite.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = finite.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mean = finite.iter().sum::<f64>() / finite.len() as f64;
    (min, max, mean)
}

fn build_primitives(capture: &Capture, manual: &[ManualPoint]) -> Vec<ViewPrimitive> {
    let mut series: Vec<ChartSeries> = capture
        .channel_names
        .iter()
        .zip(&capture.channels)
        .take(MAX_CHARTED_CHANNELS)
        .enumerate()
        .map(|(i, (name, samples))| {
            let skip = capture.times.len().saturating_sub(MAX_PLOTTED_POINTS);
            let points = capture
                .times
                .iter()
                .zip(samples)
                .skip(skip)
                .filter(|(_, v)| !v.is_nan())
                .map(|(&t, &v)| ChartPoint { x: t, y: v })
                .collect();
            // first channel gets the "measured" role (reads in the
            // direction-of-travel color since there's no Limit series
            // to compare against); the rest fall back to the muted
            // other color -- not amazingly distinct across many
            // channels, but reuses the existing role convention rather
            // than inventing a new per-channel coloring scheme.
            let role = if i == 0 { SeriesRole::Measured } else { SeriesRole::Other };
            ChartSeries { name: name.clone(), role, points }
        })
        .collect();

    // manual points (ADR-056) render as their own series per channel,
    // named distinctly, not spliced into the real capture's
    // own aligned sample arrays above -- a manual annotation should
    // always read as visibly separate from what the running capture
    // task actually measured, not silently blended into it.
    for channel_name in capture.channel_names.iter().take(MAX_CHARTED_CHANNELS) {
        let points: Vec<ChartPoint> = manual
            .iter()
            .filter(|p| &p.channel == channel_name)
            .map(|p| ChartPoint { x: p.x, y: p.y })
            .collect();
        if !points.is_empty() {
            series.push(ChartSeries { name: format!("{channel_name} (manual)"), role: SeriesRole::Other, points });
        }
    }

    let chart = ViewPrimitive::XyChart(XyChartView {
        series,
        x_scale: AxisScale::Linear,
        y_scale: AxisScale::Linear,
        markers: vec![],
        cursor: None,
        bands: vec![],
    });

    let mut rows: Vec<Vec<TableCell>> = capture
        .channel_names
        .iter()
        .zip(&capture.channels)
        .map(|(name, samples)| {
            let (min, max, mean) = channel_stats(samples);
            vec![
                TableCell { text: name.clone(), status: None },
                TableCell { text: format!("{min:.3}"), status: None },
                TableCell { text: format!("{max:.3}"), status: None },
                TableCell { text: format!("{mean:.3}"), status: None },
            ]
        })
        .collect();

    for channel_name in &capture.channel_names {
        let ys: Vec<f64> = manual.iter().filter(|p| &p.channel == channel_name).map(|p| p.y).collect();
        if !ys.is_empty() {
            let (min, max, mean) = channel_stats(&ys);
            rows.push(vec![
                TableCell { text: format!("{channel_name} (manual)"), status: None },
                TableCell { text: format!("{min:.3}"), status: None },
                TableCell { text: format!("{max:.3}"), status: None },
                TableCell { text: format!("{mean:.3}"), status: None },
            ]);
        }
    }

    let table = ViewPrimitive::Table(TableView {
        headers: vec!["Channel".to_string(), "Min".to_string(), "Max".to_string(), "Mean".to_string()],
        rows,
    });

    vec![chart, table]
}

/// shared by `render`/the tick branch/the `value:` branch -- all three
/// need the same "find the capture file, read it fresh, re-parse it"
/// round trip. Re-reading from scratch every call not caching
/// is deliberate: the whole point is picking up rows a still-running
/// capture task appended since the last read, and a file this small
/// (real capture files here are tens of KB, not GB) makes re-parsing
/// cheap enough that caching would be premature. `Ok(None)` means no
/// capture file exists yet -- a real, distinct state from "file exists
/// but is empty," not folded into the error path.
/// `stream_filter: Some(name)` picks that exact file out of everything
/// `list_files` returns not always taking the first
/// alphabetical match (ADR-058) -- lets separate panes each pin to a
/// specific real capture stream when several coexist in one project.
/// `Err` when the pin no longer matches anything real (file renamed or
/// deleted out from under a running pane) not silently falling
/// back to some other stream, which would show the wrong data with no
/// indication it had changed streams.
/// candidate `.analog-capture` path matches a `stream:` query under
/// any of: the full path as typed, a path ending in `/<query>` (a
/// directory prefix plus the exact filename), or -- the case that was
/// missing and is the one users actually reach for -- the bare file
/// stem alone, no directory and no `.analog-capture` extension. The
/// tasks panel already shows continuous tasks discovered by ADR-060
/// as `stream: <name>` with that same bare name, so typing exactly
/// what's on screen (`stream: primary`, not `stream: primary.analog-
/// capture`) has to work, not just the fully-qualified filename.
fn stream_query_matches(path: &str, query: &str) -> bool {
    if path == query || path.ends_with(&format!("/{query}")) {
        return true;
    }
    let stem = path.strip_suffix(".analog-capture").unwrap_or(path);
    let file_stem = stem.rsplit('/').next().unwrap_or(stem);
    file_stem == query
}

fn load_capture(stream_filter: Option<&str>) -> Result<Option<Capture>, String> {
    let matches = host::list_files("**/*.analog-capture").unwrap_or_default();
    let path = match stream_filter {
        Some(name) => match matches.iter().find(|p| stream_query_matches(p, name)) {
            Some(p) => p.clone(),
            None => {
                return Err(format!(
                    "stream {name:?} not found among current .analog-capture files: {matches:?}"
                ))
            }
        },
        None => match matches.into_iter().next() {
            Some(p) => p,
            None => return Ok(None),
        },
    };
    let contents = host::read_file(&path)?;
    Ok(Some(parse(&contents)?))
}

fn load_primitives(state: &ViewState) -> Result<Vec<ViewPrimitive>, String> {
    match load_capture(state.stream_filter.as_deref())? {
        Some(capture) => Ok(build_primitives(&capture, &state.manual)),
        None => {
            let text = match &state.stream_filter {
                Some(name) => format!("no .analog-capture file matching {name:?} found in this project"),
                None => "no .analog-capture file found in this project".to_string(),
            };
            Ok(vec![ViewPrimitive::Table(TableView {
                headers: vec!["".to_string()],
                rows: vec![vec![TableCell { text, status: None }]],
            })])
        }
    }
}

/// how far past the real data's own range a manually-typed value can
/// go before it's refused (ADR-056) -- a typo or a wildly wrong unit
/// (a whole-volts reading typed where the capture is in millivolts,
/// say) would otherwise silently blow out the chart's auto-scale and
/// make every real point invisible, with no compiler or runtime error
/// to catch it: `f64` parses `1e300` exactly as happily as `1.0`, and
/// a downstream renderer has no way to know which one is a mistake.
/// only enforced once real data actually exists to compare against --
/// there's no "reasonable range" to judge a first-ever observation
/// against, so a capture with no finite samples yet only gets the
/// finiteness check below, not this one.
const MAX_VALUE_MULTIPLE_OF_EXISTING_RANGE: f64 = 10.0;

fn validate_value(capture: &Capture, channel: &str, y: f64) -> Result<(), String> {
    if !capture.channel_names.iter().any(|n| n == channel) {
        return Err(format!("unknown channel {channel:?} — known channels: {:?}", capture.channel_names));
    }
    let existing_max_abs = capture
        .channels
        .iter()
        .flatten()
        .copied()
        .filter(|v| v.is_finite())
        .map(f64::abs)
        .fold(0.0_f64, f64::max);
    if existing_max_abs > 0.0 {
        let bound = existing_max_abs * MAX_VALUE_MULTIPLE_OF_EXISTING_RANGE;
        if y.abs() > bound {
            return Err(format!(
                "value {y} is more than {MAX_VALUE_MULTIPLE_OF_EXISTING_RANGE}x this channel's real data range \
                 (max seen so far: {existing_max_abs:.4}, bound: {bound:.4}) — refused; check for a typo or a unit mismatch"
            ));
        }
    }
    Ok(())
}

impl Guest for Component {
    fn view_id() -> String {
        "analog-capture".to_string()
    }

    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        host::log(&format!("analog-capture: init for project at {}", project.root_path));
        Ok(Vec::new())
    }

    fn render(state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        let view_state = decode_state(&state)?;
        load_primitives(&view_state)
    }

    /// live-refresh (ADR-055) re-reads and re-renders the capture file
    /// fresh on every tick. `value: <channel> <x> <y>` (ADR-056) is a
    /// manually-typed point for cases nothing is automating -- an
    /// analog-only instrument with no computer output, or just noting
    /// something outside the running capture entirely. Validated
    /// (finite, and bounded against the real capture's own range once
    /// one exists) before ever reaching the chart; never written to
    /// disk, lives only in this pane's session state, same as
    /// `emc`'s own cursor/phase. `stream: <filename>`
    /// (ADR-058) pins this pane to one specific `.analog-capture` file
    /// among however many currently match, so several panes can each
    /// watch a different real stream instead of all landing on
    /// whichever one sorts first alphabetically; `stream: -` (or any
    /// empty argument) clears the pin back to that default.
    fn handle_command(state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        let mut view_state = decode_state(&state)?;

        if command == LIVE_TICK_COMMAND {
            return Ok((state, load_primitives(&view_state)?));
        }

        if let Some(rest) = command.strip_prefix("stream:") {
            let name = rest.trim();
            view_state.stream_filter = if name.is_empty() || name == "-" { None } else { Some(name.to_string()) };
            let primitives = load_primitives(&view_state)?;
            let new_state = encode_state(&view_state);
            return Ok((new_state, primitives));
        }

        if let Some(rest) = command.strip_prefix("value:") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            let [channel, x_str, y_str] = parts[..] else {
                return Err("usage: value: <channel> <x> <y>".to_string());
            };
            let x: f64 = x_str.parse().map_err(|_| format!("invalid x {x_str:?} — must be a plain number"))?;
            let y: f64 = y_str.parse().map_err(|_| format!("invalid y {y_str:?} — must be a plain number"))?;
            if !x.is_finite() || !y.is_finite() {
                return Err(format!("value must be finite, got x={x} y={y}"));
            }
            let Some(capture) = load_capture(view_state.stream_filter.as_deref())? else {
                return Err("no .analog-capture file exists yet to validate the channel name against".to_string());
            };
            validate_value(&capture, channel, y)?;

            view_state.manual.push(ManualPoint { channel: channel.to_string(), x, y });
            let primitives = build_primitives(&capture, &view_state.manual);
            let new_state = encode_state(&view_state);
            return Ok((new_state, primitives));
        }

        Err(format!(
            "unknown command: {command:?} (try the tick, \"stream: <filename>\", or \"value: <channel> <x> <y>\")"
        ))
    }
}

bindings::export!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_primitives_caps_charted_points_to_the_most_recent_window() {
        let n = MAX_PLOTTED_POINTS * 3;
        let times: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let samples: Vec<f64> = times.clone();
        let capture = Capture {
            time_label: "time".to_string(),
            channel_names: vec!["ch1".to_string()],
            times,
            channels: vec![samples],
        };
        let primitives = build_primitives(&capture, &[]);
        let ViewPrimitive::XyChart(chart) = &primitives[0] else { panic!("expected a chart primitive first") };
        let series = &chart.series[0];
        assert_eq!(series.points.len(), MAX_PLOTTED_POINTS);
        // windowed to the tail, not the head -- the most recent samples,
        // in original chronological order.
        assert_eq!(series.points.first().unwrap().x, (n - MAX_PLOTTED_POINTS) as f64);
        assert_eq!(series.points.last().unwrap().x, (n - 1) as f64);
    }

    #[test]
    fn build_primitives_leaves_a_short_capture_untouched() {
        let n = 10;
        let times: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let samples: Vec<f64> = times.clone();
        let capture = Capture {
            time_label: "time".to_string(),
            channel_names: vec!["ch1".to_string()],
            times,
            channels: vec![samples],
        };
        let primitives = build_primitives(&capture, &[]);
        let ViewPrimitive::XyChart(chart) = &primitives[0] else { panic!("expected a chart primitive first") };
        assert_eq!(chart.series[0].points.len(), n);
    }

    #[test]
    fn stream_query_matches_the_bare_stem_the_tasks_panel_actually_shows() {
        assert!(stream_query_matches("primary.analog-capture", "primary"));
        assert!(stream_query_matches("some/dir/primary.analog-capture", "primary"));
    }

    #[test]
    fn stream_query_matches_the_full_filename_too() {
        assert!(stream_query_matches("primary.analog-capture", "primary.analog-capture"));
        assert!(stream_query_matches("some/dir/primary.analog-capture", "primary.analog-capture"));
    }

    #[test]
    fn stream_query_rejects_an_unrelated_or_partial_name() {
        assert!(!stream_query_matches("primary.analog-capture", "prim"));
        assert!(!stream_query_matches("primary.analog-capture", "vertex"));
        assert!(!stream_query_matches("primary.analog-capture", "primary.analog-captur"));
    }
}
