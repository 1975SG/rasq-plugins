//! testbench emulator, not a plugin -- behaves like a real frame source
//! (a protocol sniffer, a device streaming status frames) continuously
//! appending newly-observed frames to an *existing* `.frame-format`
//! declaration. Meant to be declared as a `.iderm/tasks.toml`
//! continuous task (ADR-032), the same mechanism a real capture tool
//! would use -- this is a stand-in for one, not a replacement for
//! pointing the same mechanism at real hardware.
//!
//! different shape from `vcd-parser`/`analog-waveform`'s own
//! emulators: `example-view-frame-builder` has no separate streamed
//! capture-log format to write into (it builds frames from declared
//! values in the same file it renders, deliberately not live-
//! refreshed -- see that plugin's own doc comment). So this emulator
//! reads the target file's *existing* field/checksum declaration
//! (`field,...`/`checksum,...` lines a project already wrote by hand)
//! and appends fresh `frame,emulated-N` blocks with generated values --
//! observed frames arriving over time, not a changing format. An
//! occasional out-of-width value (~1-in-15) exercises the same real
//! authoring-mistake check `example-doctor-frame-builder` already
//! performs on hand-declared frames. Deterministic seeded xorshift64,
//! same hand-rolled PRNG convention every synthetic dataset in this
//! project already uses.

use frame_format::{FieldKind, FrameSpec};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::thread;
use std::time::Duration;

const TICK: Duration = Duration::from_millis(500);
const OVERFLOW_DENOMINATOR: u64 = 15;

fn next_u64(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn max_for_width(width_bytes: usize) -> u64 {
    match width_bytes {
        1 => u8::MAX as u64,
        2 => u16::MAX as u64,
        4 => u32::MAX as u64,
        _ => u64::MAX,
    }
}

/// text a `frame,label` block's own `field=value` line expects --
/// decimal for numeric fields, exactly `2 * width` hex characters for
/// `bytesN` fields (mirrors `parse_frame_format`'s own parsing rules,
/// see `frame-format/src/lib.rs`).
fn random_field_text(kind: FieldKind, rng_state: &mut u64, overflow: bool) -> String {
    match kind {
        FieldKind::Bytes(width) => {
            (0..width).map(|_| format!("{:02x}", next_u64(rng_state) % 256)).collect::<Vec<_>>().join("")
        }
        _ => {
            let width = kind.byte_width();
            let max = max_for_width(width);
            let value = next_u64(rng_state) % (max + 1);
            // deliberately over the field's own declared width -- the
            // real authoring mistake this emulator exists to
            // occasionally exercise.
            if overflow { (value.saturating_add(max).saturating_add(1)).to_string() } else { value.to_string() }
        }
    }
}

fn main() -> std::io::Result<()> {
    let path = env::args().nth(1).unwrap_or_else(|| "device.frame-format".to_string());
    let contents = fs::read_to_string(&path).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!(
                "{path}: {e} -- this emulator appends to an *existing* field/checksum declaration, it doesn't invent one (create the file with \"field,...\"/\"checksum,...\" lines first)"
            ),
        )
    })?;
    let (spec, _existing): (FrameSpec, _) =
        frame_format::parse_frame_format(&contents).map_err(|e| std::io::Error::other(format!("{path}: {e}")))?;

    let mut rng_state: u64 = 0x6672_616d_655f;
    let mut n: u64 = 0;

    let mut file = OpenOptions::new().append(true).open(&path)?;
    loop {
        thread::sleep(TICK);
        n += 1;
        let overflow_field = if next_u64(&mut rng_state).is_multiple_of(OVERFLOW_DENOMINATOR) {
            Some((next_u64(&mut rng_state) as usize) % spec.fields.len())
        } else {
            None
        };

        writeln!(file, "\nframe,emulated-{n}")?;
        for (i, field) in spec.fields.iter().enumerate() {
            let text = random_field_text(field.kind, &mut rng_state, overflow_field == Some(i));
            writeln!(file, "{}={text}", field.name)?;
        }
        file.flush()?;
    }
}
