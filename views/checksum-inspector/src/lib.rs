#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::types::{ProjectInfo, Severity};
use bindings::iderm::plugin::view::{TableCell, TableView, ViewPrimitive};

struct Component;

const FRAME_COUNT: usize = 8;

/// reserved live-refresh token -- see `app.rs`'s `LIVE_TICK_COMMAND` on
/// the Core side. Each plugin keeps its own copy not sharing
/// one (not part of the WIT contract by design), same as every other
/// live-capable example plugin.
const LIVE_TICK_COMMAND: &str = "\0tick";

/// deterministic synthetic payload, no external data dependency, same
/// convention every other example plugin's own dataset already uses --
/// varying length per frame (8-12 bytes) for a more realistic-looking
/// capture than uniform-size frames would be.
fn synthetic_payload(i: usize) -> Vec<u8> {
    let len = 8 + (i % 5);
    (0..len).map(|j| ((i * 31 + j * 17 + 7) % 256) as u8).collect()
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// builds `FRAME_COUNT` synthetic frames, each payload followed by its
/// own real `checksums::crc32` trailer -- a stand-in for "bytes received
/// off the wire," the shape a real frame/packet inspector would
/// actually consume. `phase` rotates which single frame gets its
/// trailer corrupted (one flipped bit) each live tick, so which row
/// reads `[xx]` visibly changes over time not staying static --
/// the more useful thing to prove live is the verification pipeline
/// itself catching a *different* real corruption each time, not just
/// the table redrawing.
fn build_table(phase: u64) -> ViewPrimitive {
    let corrupted_index = (phase as usize) % FRAME_COUNT;

    let rows = (0..FRAME_COUNT)
        .map(|i| {
            let payload = synthetic_payload(i);
            let computed = checksums::crc32(&payload);
            let mut trailer = computed.to_be_bytes();
            if i == corrupted_index {
                trailer[3] ^= 0x01; // one flipped bit -- real transmission corruption
            }
            let received_crc = u32::from_be_bytes(trailer);
            let ok = received_crc == computed;

            vec![
                TableCell { text: format!("frame-{i:02}"), status: None },
                TableCell { text: hex_bytes(&payload), status: None },
                TableCell { text: format!("{computed:08x}"), status: None },
                TableCell {
                    text: if ok { "ok".to_string() } else { format!("mismatch (got {received_crc:08x})") },
                    status: Some(if ok { Severity::Info } else { Severity::Error }),
                },
            ]
        })
        .collect();

    ViewPrimitive::Table(TableView {
        headers: vec!["Frame".to_string(), "Bytes (hex)".to_string(), "CRC-32".to_string(), "Status".to_string()],
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

impl Guest for Component {
    fn view_id() -> String {
        "checksum-inspector".to_string()
    }

    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        bindings::iderm::plugin::host::log(&format!(
            "checksum-inspector: init for project at {}",
            project.root_path
        ));
        Ok(encode_state(0))
    }

    fn render(state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        let phase = decode_state(&state)?;
        Ok(vec![build_table(phase)])
    }

    /// only the reserved live-refresh token is recognized -- this
    /// example has no other command vocabulary, matching every other
    /// live-capable example plugin.
    fn handle_command(state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        let phase = decode_state(&state)?;
        if command == LIVE_TICK_COMMAND {
            let next_phase = phase + 1;
            Ok((encode_state(next_phase), vec![build_table(next_phase)]))
        } else {
            Err(format!("unknown command: {command:?} (this view has no commands)"))
        }
    }
}

bindings::export!(Component with_types_in bindings);
