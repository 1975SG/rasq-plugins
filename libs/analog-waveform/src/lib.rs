//! parses a simple CSV time-series capture -- Hardware/Instrumentation
//! diagnostic Workbench §2, Analog Waveform Viewer
//! (`docs/roadmap/Hardware_Diagnostic_Workbench.md`). A header row
//! (`time,ch1,ch2,...`) followed by numeric rows is close to the shape
//! most real oscilloscopes/DAQ tools already export, not an invented
//! format -- kept intentionally generic (RFC 4180-style CSV, no vendor
//! dialect assumed) not matching one specific instrument's
//! exact export quirks, per ADR-011.
//!
//! same v1 scope as `vcd-parser` (ADR-029): reads a capture file that
//! already exists. Spawning the capture itself is `.iderm/tasks.toml`'s
//! job (ADR-030), not this crate's.

#[derive(Debug, Clone)]
pub struct Capture {
    /// header row's first column verbatim (e.g. `"time"`,
    /// `"time_s"`) -- not parsed into a unit, since nothing here does
    /// unit-aware time math, only relative comparisons.
    pub time_label: String,
    pub channel_names: Vec<String>,
    pub times: Vec<f64>,
    /// `channels[i][j]` is channel `i`'s value at `times[j]` -- same
    /// length as `times` for every channel, enforced by `parse`.
    pub channels: Vec<Vec<f64>>,
}

impl Capture {
    pub fn duration(&self) -> f64 {
        self.times.last().copied().unwrap_or(0.0)
    }

    /// whether any sample anywhere is `NaN` -- a real dropped-sample/
    /// instrument-glitch signal in a genuine capture, not just a
    /// parsing curiosity. An empty or literal `nan`/`NaN` cell parses
    /// as `f64::NAN` not failing the whole file, so a capture
    /// with one bad sample can still be read and this is how that
    /// shows up.
    pub fn has_missing_samples(&self) -> bool {
        self.channels.iter().any(|c| c.iter().any(|v| v.is_nan()))
    }
}

pub fn parse(text: &str) -> Result<Capture, String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines.next().ok_or("empty file: no header row")?;
    let columns: Vec<&str> = header.split(',').map(str::trim).collect();
    if columns.len() < 2 {
        return Err(format!("header must have a time column plus at least one channel, got {:?}", columns));
    }
    let time_label = columns[0].to_string();
    let channel_names: Vec<String> = columns[1..].iter().map(|s| s.to_string()).collect();
    let channel_count = channel_names.len();

    let mut times = Vec::new();
    let mut channels: Vec<Vec<f64>> = vec![Vec::new(); channel_count];

    for (i, line) in lines.enumerate() {
        let line_no = i + 2; // header was line 1
        let cells: Vec<&str> = line.split(',').map(str::trim).collect();
        if cells.len() != columns.len() {
            return Err(format!(
                "line {line_no}: expected {} column(s), got {}: {line:?}",
                columns.len(),
                cells.len()
            ));
        }
        let time: f64 =
            cells[0].parse().map_err(|_| format!("line {line_no}: invalid time value {:?}", cells[0]))?;
        times.push(time);
        for (ch_idx, cell) in cells[1..].iter().enumerate() {
            let value = parse_sample(cell).ok_or_else(|| format!("line {line_no}: invalid sample {cell:?}"))?;
            channels[ch_idx].push(value);
        }
    }

    Ok(Capture { time_label, channel_names, times, channels })
}

/// empty cell or a literal `nan`/`NaN`/`NAN` means a dropped sample
/// -- parsed as `f64::NAN`, not rejected, since one missing sample in
/// an otherwise-real capture is exactly the kind of thing
/// `Capture::has_missing_samples` exists to surface, not a reason to
/// refuse the whole file.
fn parse_sample(cell: &str) -> Option<f64> {
    if cell.is_empty() || cell.eq_ignore_ascii_case("nan") {
        return Some(f64::NAN);
    }
    cell.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
time,ch1,ch2
0.0,0.0,1.0
0.5,0.71,0.71
1.0,1.0,0.0
1.5,0.71,-0.71
2.0,0.0,-1.0
";

    #[test]
    fn parses_header_and_channel_names() {
        let cap = parse(SAMPLE).unwrap();
        assert_eq!(cap.time_label, "time");
        assert_eq!(cap.channel_names, vec!["ch1", "ch2"]);
    }

    #[test]
    fn parses_all_rows_for_every_channel() {
        let cap = parse(SAMPLE).unwrap();
        assert_eq!(cap.times, vec![0.0, 0.5, 1.0, 1.5, 2.0]);
        assert_eq!(cap.channels[0], vec![0.0, 0.71, 1.0, 0.71, 0.0]);
        assert_eq!(cap.channels[1], vec![1.0, 0.71, 0.0, -0.71, -1.0]);
    }

    #[test]
    fn duration_is_the_last_time_value() {
        assert_eq!(parse(SAMPLE).unwrap().duration(), 2.0);
    }

    #[test]
    fn clean_capture_has_no_missing_samples() {
        assert!(!parse(SAMPLE).unwrap().has_missing_samples());
    }

    #[test]
    fn an_empty_cell_is_a_missing_sample_not_a_parse_error() {
        let text = "time,ch1\n0.0,1.0\n1.0,\n2.0,1.0\n";
        let cap = parse(text).unwrap();
        assert!(cap.has_missing_samples());
        assert!(cap.channels[0][1].is_nan());
    }

    #[test]
    fn a_literal_nan_cell_is_a_missing_sample() {
        let text = "time,ch1\n0.0,1.0\n1.0,NaN\n";
        assert!(parse(text).unwrap().has_missing_samples());
    }

    #[test]
    fn rejects_a_row_with_the_wrong_column_count() {
        let text = "time,ch1,ch2\n0.0,1.0\n";
        let err = parse(text).unwrap_err();
        assert!(err.contains("line 2"));
    }

    #[test]
    fn rejects_a_header_with_no_channels() {
        assert!(parse("time\n0.0\n").is_err());
    }

    #[test]
    fn rejects_an_empty_file() {
        assert!(parse("").is_err());
    }

    #[test]
    fn rejects_a_non_numeric_time_value() {
        let text = "time,ch1\nnot-a-number,1.0\n";
        let err = parse(text).unwrap_err();
        assert!(err.contains("invalid time value"));
    }
}
