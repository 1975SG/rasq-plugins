//! Parses VCD (Value Change Dump, IEEE 1364/1800) - a real, standard,
//! tool-agnostic digital-logic capture format, not a vendor product
//! (per ADR-011, `docs/roadmap/Hardware_Diagnostic_Workbench.md`'s §1
//! Logic/Protocol Capture Viewer). Scoped to reading an already-
//! captured `.vcd` file, not spawning a capture tool - `doctor-rule`/
//! `view-provider` plugins have no subprocess or network capability
//! (`wit/iderm-plugin.wit`'s `host` interface is exactly `read-file`/
//! `list-files`/`log`), so live/on-demand capture needs a separate,
//! not-yet-designed Core-side mechanism (spawn a capture tool, refresh
//! its output somewhere a plugin can `read-file`) - this crate covers
//! the "read a file that already exists" half only, deliberately.
//!
//! Covers scalar (`0`/`1`/`x`/`z`) and vector (`b101...`) value
//! changes, `$var`/`$timescale`/`$enddefinitions`/`$dumpvars`-family
//! structure. Doesn't cover `$scope`/`$upscope` hierarchy (signal names
//! are taken flat, not qualified by their scope path) or real-number
//! (`r`/`R`) value changes beyond storing them as an opaque string -
//! neither is needed to render a raw digital waveform, the one thing
//! this crate's own consumers actually do with the result.

#[derive(Debug, Clone)]
pub struct Signal {
    /// VCD's own short identifier code (e.g. `"!"`, not necessarily a
    /// single ASCII byte - VCD allows multi-character codes once a
    /// design has more distinct signals than single printable-ASCII
    /// characters can name) - an internal handle, not shown to a user.
    pub id: String,
    pub name: String,
    pub width: u32,
}

#[derive(Debug, Clone)]
pub struct ValueChange {
    pub time: u64,
    pub id: String,
    /// Raw value text: `"0"`/`"1"`/`"x"`/`"z"` for a scalar signal,
    /// a binary digit string (no `b` prefix) for a vector one.
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct VcdFile {
    /// Display text from `$timescale` (e.g. `"1 ns"`) - not parsed
    /// into a numeric unit, since nothing here does unit-aware time
    /// math, only relative comparisons between change timestamps.
    pub timescale: String,
    pub signals: Vec<Signal>,
    pub changes: Vec<ValueChange>,
}

impl VcdFile {
    /// The last (largest) time seen across every value change - the
    /// capture's total duration in `timescale` units. `0` for a file
    /// with no value changes at all.
    pub fn duration(&self) -> u64 {
        self.changes.iter().map(|c| c.time).max().unwrap_or(0)
    }

    /// One signal's own value changes, in time order - VCD's change
    /// stream is already chronological in a well-formed file, so this
    /// is a filter, not a sort.
    pub fn timeline(&self, id: &str) -> Vec<(u64, &str)> {
        self.changes.iter().filter(|c| c.id == id).map(|c| (c.time, c.value.as_str())).collect()
    }

    /// Whether any recorded value is unknown (`x`/`X`) or high-impedance
    /// (`z`/`Z`) - a real capture-quality signal worth a project-health
    /// check flagging, not just a rendering concern. Checks for the
    /// character *anywhere* in the value, not just a whole-value exact
    /// match - a vector's unknown state can be `"xx"`, `"1x"`, `"x0"`,
    /// etc. (per-bit, not a single character the way a scalar's is),
    /// and an exact-match check would silently never catch one at all.
    pub fn has_unknown_or_high_z(&self) -> bool {
        self.changes.iter().any(|c| c.value.chars().any(|ch| matches!(ch, 'x' | 'X' | 'z' | 'Z')))
    }
}

pub fn parse(text: &str) -> Result<VcdFile, String> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut i = 0;
    let mut timescale = String::new();
    let mut signals = Vec::new();
    let mut changes = Vec::new();
    let mut current_time: u64 = 0;
    let mut in_header = true;

    while i < tokens.len() {
        let tok = tokens[i];

        if in_header {
            match tok {
                "$var" => {
                    // $var <type> <size> <id> <name> [bit-range] $end
                    // -- each field is checked against `$end` too, not
                    // just against running off the end of the token
                    // stream: a `$var` that ends early would otherwise
                    // silently consume the *next* command's own tokens
                    // as a garbage id/name instead of being rejected.
                    let size_tok = tokens.get(i + 2).ok_or("$var: truncated (missing size)")?;
                    let id_tok = tokens.get(i + 3).ok_or("$var: truncated (missing id)")?;
                    let name_tok = tokens.get(i + 4).ok_or("$var: truncated (missing name)")?;
                    if *id_tok == "$end" || *name_tok == "$end" {
                        return Err("$var: truncated (ended before id/name were given)".to_string());
                    }
                    let width: u32 = size_tok.parse().map_err(|_| format!("$var: invalid size {size_tok:?}"))?;
                    signals.push(Signal { id: id_tok.to_string(), name: name_tok.to_string(), width });
                    i += 5;
                    while i < tokens.len() && tokens[i] != "$end" {
                        i += 1;
                    }
                    i += 1; // consume "$end" itself
                }
                "$timescale" => {
                    i += 1;
                    let mut parts = Vec::new();
                    while i < tokens.len() && tokens[i] != "$end" {
                        parts.push(tokens[i]);
                        i += 1;
                    }
                    timescale = parts.join(" ");
                    i += 1;
                }
                "$enddefinitions" => {
                    i += 1; // consume "$enddefinitions"
                    if tokens.get(i) == Some(&"$end") {
                        i += 1;
                    }
                    in_header = false;
                }
                t if t.starts_with('$') => {
                    // $date/$version/$scope/$upscope/$comment/... --
                    // header commands this parser doesn't need the
                    // content of. Skip to the matching $end.
                    i += 1;
                    while i < tokens.len() && tokens[i] != "$end" {
                        i += 1;
                    }
                    i += 1;
                }
                other => return Err(format!("unexpected token in header: {other:?}")),
            }
            continue;
        }

        // Data section.
        if let Some(rest) = tok.strip_prefix('#') {
            current_time = rest.parse().map_err(|_| format!("invalid time marker {tok:?}"))?;
            i += 1;
        } else if tok == "$dumpvars" || tok == "$dumpon" || tok == "$dumpoff" || tok == "$dumpall" || tok == "$end" {
            i += 1;
        } else if let Some(bits) = tok.strip_prefix(['b', 'B']) {
            let id = tokens.get(i + 1).ok_or_else(|| format!("vector value {tok:?} missing its id"))?;
            changes.push(ValueChange { time: current_time, id: id.to_string(), value: bits.to_string() });
            i += 2;
        } else if let Some(value) = tok.strip_prefix(['r', 'R']) {
            let id = tokens.get(i + 1).ok_or_else(|| format!("real value {tok:?} missing its id"))?;
            changes.push(ValueChange { time: current_time, id: id.to_string(), value: value.to_string() });
            i += 2;
        } else {
            // A scalar change: one value character immediately
            // followed by the id, no space (e.g. "0!", "x#").
            let mut chars = tok.chars();
            let value = chars.next().ok_or("empty token in data section")?;
            let id: String = chars.collect();
            if id.is_empty() {
                return Err(format!("malformed scalar value change {tok:?}"));
            }
            changes.push(ValueChange { time: current_time, id, value: value.to_string() });
            i += 1;
        }
    }

    Ok(VcdFile { timescale, signals, changes })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real, spec-valid minimal VCD: a clock and a 2-bit counter,
    /// hand-written against the actual IEEE VCD grammar rather than an
    /// invented shape -- same "test against the real format" standard
    /// this session's other parsers (`checksums`' published check
    /// values) already held themselves to.
    const SAMPLE: &str = "\
$date
   2026-08-09
$end
$version
   iderm test fixture
$end
$timescale 1 ns $end
$scope module top $end
$var wire 1 ! clk $end
$var wire 2 \" count $end
$upscope $end
$enddefinitions $end
$dumpvars
0!
b00 \"
$end
#5
1!
#10
0!
b01 \"
#15
1!
#20
0!
b10 \"
#25
1!
#30
0!
b11 \"
";

    #[test]
    fn parses_the_declared_signals() {
        let vcd = parse(SAMPLE).unwrap();
        assert_eq!(vcd.signals.len(), 2);
        assert_eq!(vcd.signals[0].name, "clk");
        assert_eq!(vcd.signals[0].width, 1);
        assert_eq!(vcd.signals[1].name, "count");
        assert_eq!(vcd.signals[1].width, 2);
    }

    #[test]
    fn parses_the_timescale() {
        let vcd = parse(SAMPLE).unwrap();
        assert_eq!(vcd.timescale, "1 ns");
    }

    #[test]
    fn clock_timeline_alternates_correctly() {
        let vcd = parse(SAMPLE).unwrap();
        let clk = vcd.timeline("!");
        assert_eq!(clk, vec![(0, "0"), (5, "1"), (10, "0"), (15, "1"), (20, "0"), (25, "1"), (30, "0")]);
    }

    #[test]
    fn counter_timeline_shows_the_vector_values() {
        let vcd = parse(SAMPLE).unwrap();
        let count = vcd.timeline("\"");
        assert_eq!(count, vec![(0, "00"), (10, "01"), (20, "10"), (30, "11")]);
    }

    #[test]
    fn duration_is_the_last_time_marker() {
        assert_eq!(parse(SAMPLE).unwrap().duration(), 30);
    }

    #[test]
    fn clean_capture_has_no_unknown_or_high_z_values() {
        assert!(!parse(SAMPLE).unwrap().has_unknown_or_high_z());
    }

    #[test]
    fn detects_an_unknown_scalar_value() {
        let text = "$timescale 1 ns $end\n$var wire 1 ! sig $end\n$enddefinitions $end\n#0\nx!\n";
        assert!(parse(text).unwrap().has_unknown_or_high_z());
    }

    /// Regression test for a real bug caught during live verification:
    /// a vector's unknown state is per-bit (`"xx"`, `"1x"`, ...), not a
    /// single character the way a scalar's is -- an exact whole-value
    /// match against `"x"` never fires for a vector at all, silently.
    #[test]
    fn detects_an_unknown_value_within_a_vector() {
        let text = "$timescale 1 ns $end\n$var wire 2 # sig $end\n$enddefinitions $end\n#0\nbx0 #\n";
        assert!(parse(text).unwrap().has_unknown_or_high_z());
    }

    #[test]
    fn detects_a_high_z_value_within_a_vector() {
        let text = "$timescale 1 ns $end\n$var wire 2 # sig $end\n$enddefinitions $end\n#0\nb1z #\n";
        assert!(parse(text).unwrap().has_unknown_or_high_z());
    }

    #[test]
    fn duration_of_an_empty_capture_is_zero() {
        let text = "$timescale 1 ns $end\n$var wire 1 ! sig $end\n$enddefinitions $end\n";
        assert_eq!(parse(text).unwrap().duration(), 0);
    }

    #[test]
    fn rejects_a_var_missing_required_fields() {
        let text = "$var wire 1 $end\n$enddefinitions $end\n";
        assert!(parse(text).is_err());
    }
}
