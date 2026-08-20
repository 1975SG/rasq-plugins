#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::host;
use bindings::iderm::plugin::types::{ProjectInfo, Severity};
use bindings::iderm::plugin::view::{TableCell, TableView, ViewPrimitive};
use invariant_report::{parse, Status};

struct Component;

/// reads and parses every `*.invariants` file, building one `Table` row
/// per invariant. Deliberately not live-refreshed, unlike every other
/// example view built this session -- a model-checker report is the
/// result of a completed run, not a value that changes on its own
/// between ticks the way a live measurement or a Monte Carlo re-draw
/// would; faking liveliness here would be dishonest about what this
/// panel actually shows.
fn build_table() -> ViewPrimitive {
    let files = host::list_files("**/*.invariants").unwrap_or_default();
    let mut rows = Vec::new();

    for path in &files {
        let Ok(contents) = host::read_file(path) else {
            continue;
        };
        match parse(&contents) {
            Ok(invariants) => {
                for inv in invariants {
                    let (status_text, severity) = match inv.status {
                        Status::Holds => ("holds".to_string(), Severity::Info),
                        Status::Violated => ("violated".to_string(), Severity::Error),
                        Status::Unknown => ("unknown".to_string(), Severity::Warning),
                    };
                    rows.push(vec![
                        TableCell { text: path.clone(), status: None },
                        TableCell { text: inv.name, status: None },
                        TableCell { text: status_text, status: Some(severity) },
                        TableCell { text: inv.description, status: None },
                    ]);
                }
            }
            Err(e) => {
                rows.push(vec![
                    TableCell { text: path.clone(), status: None },
                    TableCell { text: "(parse error)".to_string(), status: None },
                    TableCell { text: "error".to_string(), status: Some(Severity::Error) },
                    TableCell { text: e, status: None },
                ]);
            }
        }
    }

    ViewPrimitive::Table(TableView {
        headers: vec![
            "File".to_string(),
            "Invariant".to_string(),
            "Status".to_string(),
            "Description".to_string(),
        ],
        rows,
    })
}

impl Guest for Component {
    fn view_id() -> String {
        "invariant-checker".to_string()
    }

    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        host::log(&format!("invariant-checker: init for project at {}", project.root_path));
        Ok(Vec::new())
    }

    fn render(_state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        Ok(vec![build_table()])
    }

    /// no command vocabulary at all, not even the reserved live-refresh
    /// token -- see `build_table`'s own doc for why this panel is
    /// deliberately static.
    fn handle_command(_state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        Err(format!("unknown command: {command:?} (this view is static, no commands)"))
    }
}

bindings::export!(Component with_types_in bindings);
