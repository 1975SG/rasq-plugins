//! parses a generic, tool-agnostic invariant-report format -- Formal-
//! methods tooling's "Invariant checker panel"
//! (`docs/roadmap/Workbenches.md`), shared by a `doctor-rule` and a
//! `view-provider` example the same way `deviation-analyzer`/
//! `distribution-analysis` already are.
//!
//! no real model checker (TLC, Alloy Analyzer, ...) speaks this format
//! natively -- it's the same role `compile_commands.json` already plays
//! for build systems in this project (`scanner.rs` reads it without
//! caring which build system produced it): a small, stable,
//! tool-agnostic intermediate a project's own tooling translates a real
//! checker's native output into. Per ADR-011, described entirely by
//! capability, no specific consuming project or checker named.
//!
//! one invariant per line: `name|status|description`. `#`-prefixed and
//! blank lines are skipped. Plain text, not JSON/TOML -- no parser
//! dependency, matching every other shared crate built this session
//! (`checksums`, `distribution-analysis`, `deviation-analyzer`), and a
//! model checker's own translation script (however it's written) only
//! ever needs to emit plain lines, not a structured serializer.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Holds,
    Violated,
    /// real, honest third state -- bounded/incomplete model checking
    /// can finish without having explored enough of the state space to
    /// say either way. Not the same as "violated" (which claims a
    /// counterexample exists) or silently omitting the invariant
    /// (which would look like it was never even considered).
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Invariant {
    pub name: String,
    pub status: Status,
    pub description: String,
}

/// parses `text` into a list of invariants, in file order. Fails on the
/// first malformed line -- reporting a 1-indexed line number and the raw
/// line content, since a hand-maintained or freshly-scripted translation
/// file is exactly where a typo is likely, and "which line" matters more
/// here than "keep going and report every remaining structural problem
/// at once."
pub fn parse(text: &str) -> Result<Vec<Invariant>, String> {
    let mut invariants = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line_no = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = trimmed.splitn(3, '|').collect();
        let [name, status, description] = parts[..] else {
            return Err(format!("line {line_no}: expected \"name|status|description\", got {line:?}"));
        };
        let status = match status {
            "holds" => Status::Holds,
            "violated" => Status::Violated,
            "unknown" => Status::Unknown,
            other => {
                return Err(format!(
                    "line {line_no}: unknown status {other:?} (expected \"holds\", \"violated\", or \"unknown\")"
                ));
            }
        };
        if name.is_empty() {
            return Err(format!("line {line_no}: empty invariant name"));
        }
        invariants.push(Invariant { name: name.to_string(), status, description: description.to_string() });
    }
    Ok(invariants)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_report() {
        let text = "\
MutexExclusion|violated|At most one process may be in Critical at a time.
NoDeadlock|holds|Some process can always eventually make progress.
";
        let result = parse(text).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "MutexExclusion");
        assert_eq!(result[0].status, Status::Violated);
        assert_eq!(result[1].status, Status::Holds);
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        let text = "\
# a comment
MutexExclusion|violated|desc

NoDeadlock|holds|desc
";
        assert_eq!(parse(text).unwrap().len(), 2);
    }

    #[test]
    fn parses_unknown_status() {
        let result = parse("SomeInvariant|unknown|Not fully explored.").unwrap();
        assert_eq!(result[0].status, Status::Unknown);
    }

    #[test]
    fn rejects_an_unrecognized_status() {
        let err = parse("Foo|maybe|desc").unwrap_err();
        assert!(err.contains("line 1"));
        assert!(err.contains("maybe"));
    }

    #[test]
    fn rejects_a_line_missing_fields() {
        let err = parse("Foo|holds").unwrap_err();
        assert!(err.contains("line 1"));
    }

    #[test]
    fn rejects_an_empty_name() {
        let err = parse("|holds|desc").unwrap_err();
        assert!(err.contains("empty invariant name"));
    }

    #[test]
    fn description_may_itself_contain_pipe_characters() {
        // splitn(3, ..) means only the first two `|` are structural --
        // a description quoting a `x | y` expression shouldn't be cut
        // short.
        let result = parse("Foo|holds|a | b | c").unwrap();
        assert_eq!(result[0].description, "a | b | c");
    }

    #[test]
    fn empty_input_is_an_empty_list_not_an_error() {
        assert!(parse("").unwrap().is_empty());
        assert!(parse("# only a comment\n").unwrap().is_empty());
    }
}
