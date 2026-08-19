//! Testbench emulator, not a plugin - a small standalone binary that
//! behaves like a real, continuously-running logic analyzer: writes a
//! valid VCD header once, then appends real value-change records at a
//! plausible cadence for as long as it runs. Meant to be declared as a
//! `.iderm/tasks.toml` continuous task (ADR-032), the same mechanism
//! a real capture tool would use - this is a stand-in for one, not a
//! replacement for pointing the same mechanism at real hardware.
//!
//! Models a minimal 3-wire bus (clock/data/chip-select - the shape of
//! a real SPI-style link, not an invented waveform): `clk` toggles
//! every tick, `cs` mostly stays asserted with occasional deselect
//! windows, `data` follows a deterministic pseudo-random schedule with
//! an occasional injected `x` (unknown) - a real bus glitch, the exact
//! condition `example-doctor-vcd-capture`'s own unknown/high-Z check
//! exists to catch. Deterministic seeded xorshift64, same hand-rolled
//! PRNG convention every synthetic dataset in this project already
//! uses - no `rand` dependency.

use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::thread;
use std::time::Duration;

const TICK: Duration = Duration::from_millis(200);
const TIME_STEP: u64 = 5;
/// Roughly 1-in-20 ticks corrupts the data line to `x` - frequent
/// enough to show up in a short demo run, rare enough to still read as
/// an anomaly rather than the normal state.
const GLITCH_DENOMINATOR: u64 = 20;

fn next_u64(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn write_header(file: &mut File) -> std::io::Result<()> {
    writeln!(file, "$timescale 1 ns $end")?;
    writeln!(file, "$var wire 1 ! clk $end")?;
    writeln!(file, "$var wire 1 \" data $end")?;
    writeln!(file, "$var wire 1 # cs $end")?;
    writeln!(file, "$enddefinitions $end")?;
    writeln!(file, "$dumpvars")?;
    writeln!(file, "0!")?;
    writeln!(file, "0\"")?;
    writeln!(file, "1#")?;
    writeln!(file, "$end")?;
    file.flush()
}

fn main() -> std::io::Result<()> {
    let path = env::args().nth(1).unwrap_or_else(|| "capture.vcd".to_string());
    // A real capture tool starting a fresh capture truncates whatever
    // was there before -- this emulator does the same rather than
    // trying to resume mid-timeline from a previous run.
    let mut file = File::create(&path)?;
    write_header(&mut file)?;
    let mut file = OpenOptions::new().append(true).open(&path)?;

    let mut rng_state: u64 = 0x564d_5f76_6364;
    let mut time: u64 = 0;
    let mut clk = false;
    let mut cs = true;
    let mut data = false;

    loop {
        thread::sleep(TICK);
        time += TIME_STEP;
        clk = !clk;

        let roll = next_u64(&mut rng_state);
        let glitch = roll.is_multiple_of(GLITCH_DENOMINATOR);
        if roll.is_multiple_of(7) {
            cs = !cs;
        }
        if roll.is_multiple_of(3) {
            data = !data;
        }

        writeln!(file, "#{time}")?;
        writeln!(file, "{}!", u8::from(clk))?;
        writeln!(file, "{}#", u8::from(cs))?;
        if glitch {
            writeln!(file, "x\"")?;
        } else {
            writeln!(file, "{}\"", u8::from(data))?;
        }
        file.flush()?;
    }
}
