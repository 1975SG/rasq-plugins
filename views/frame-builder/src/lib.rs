#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::host;
use bindings::iderm::plugin::types::{ProjectInfo, Severity};
use bindings::iderm::plugin::view::{TableCell, TableView, ViewPrimitive};
use frame_format::{build_frame, inspect_frame, parse_frame_format, DeclaredFrame, FrameSpec};

struct Component;

/// reserved live-refresh token -- see `app.rs`'s `LIVE_TICK_COMMAND`.
const LIVE_TICK_COMMAND: &str = "\0tick";

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// builds every declared frame from `spec` (the real "builder" half),
/// then inspects each one back (the "inspector" half) -- one declared
/// frame's trailer gets a single bit flipped each live tick, rotating
/// which one, same convention `checksum-inspector`
/// already established: the more useful thing to prove live is the
/// verification pipeline catching a *different* real corruption each
/// time, not just the table redrawing.
fn build_table(spec: &FrameSpec, frames: &[DeclaredFrame], phase: u64) -> ViewPrimitive {
    let corrupted_index = if frames.is_empty() { usize::MAX } else { (phase as usize) % frames.len() };

    let rows = frames
        .iter()
        .enumerate()
        .map(|(i, frame)| match build_frame(spec, &frame.values) {
            Ok(mut bytes) => {
                if i == corrupted_index {
                    *bytes.last_mut().unwrap() ^= 0x01;
                }
                let status = match inspect_frame(spec, &bytes) {
                    Ok(inspected) if inspected.checksum_ok => {
                        TableCell { text: "ok".to_string(), status: Some(Severity::Info) }
                    }
                    Ok(inspected) => TableCell {
                        text: format!(
                            "mismatch (expected {}, got {})",
                            hex_bytes(&inspected.expected_trailer),
                            hex_bytes(&inspected.actual_trailer)
                        ),
                        status: Some(Severity::Error),
                    },
                    Err(e) => TableCell { text: format!("inspect failed: {e}"), status: Some(Severity::Error) },
                };
                vec![
                    TableCell { text: frame.label.clone(), status: None },
                    TableCell { text: hex_bytes(&bytes), status: None },
                    status,
                ]
            }
            Err(e) => vec![
                TableCell { text: frame.label.clone(), status: None },
                TableCell { text: format!("build failed: {e}"), status: Some(Severity::Error) },
                TableCell { text: "-".to_string(), status: None },
            ],
        })
        .collect();

    ViewPrimitive::Table(TableView {
        headers: vec!["Frame".to_string(), "Bytes (hex)".to_string(), "Checksum".to_string()],
        rows,
    })
}

fn encode_state(phase: u64) -> Vec<u8> {
    phase.to_le_bytes().to_vec()
}

fn decode_state(state: &[u8]) -> Result<u64, String> {
    let bytes: [u8; 8] =
        state.try_into().map_err(|_| format!("corrupt view-state: expected 8 bytes, got {}", state.len()))?;
    Ok(u64::from_le_bytes(bytes))
}

fn render_at(phase: u64) -> Result<Vec<ViewPrimitive>, String> {
    let Some(path) = host::list_files("**/*.frame-format").unwrap_or_default().into_iter().next() else {
        return Ok(vec![ViewPrimitive::Table(TableView {
            headers: vec!["".to_string()],
            rows: vec![vec![TableCell {
                text: "no .frame-format file found in this project".to_string(),
                status: None,
            }]],
        })]);
    };
    let contents = host::read_file(&path)?;
    let (spec, frames) = parse_frame_format(&contents)?;
    if frames.is_empty() {
        return Ok(vec![ViewPrimitive::Table(TableView {
            headers: vec!["".to_string()],
            rows: vec![vec![TableCell { text: format!("{path}: no frames declared"), status: None }]],
        })]);
    }
    Ok(vec![build_table(&spec, &frames, phase)])
}

impl Guest for Component {
    fn view_id() -> String {
        "frame-builder".to_string()
    }

    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        host::log(&format!("frame-builder: init for project at {}", project.root_path));
        Ok(encode_state(0))
    }

    fn render(state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        render_at(decode_state(&state)?)
    }

    fn handle_command(state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        let phase = decode_state(&state)?;
        if command == LIVE_TICK_COMMAND {
            let next_phase = phase + 1;
            return Ok((encode_state(next_phase), render_at(next_phase)?));
        }
        Err(format!("unknown command: {command:?} (this view has no commands)"))
    }
}

bindings::export!(Component with_types_in bindings);
