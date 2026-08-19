//! testbench emulator, not a plugin
//! acts like a live scope/DAQ: write `time,ch1,ch2` once, then keep appending samples
//! use: `.iderm/tasks.toml` continuous task, ADR-032
//! stand-in for real capture hardware. same pipeline, fake signal
//!
//! channels: noisy sine sensor + cosine reference. looks like two probes, not a flat line
//! blank cell ~1-in-20 samples = dropped sample test case for `example-doctor-analog-capture`
//! deterministic xorshift64. same synthetic-data convention, no `rand` dependency

use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::thread;
use std::time::Duration;

const TICK: Duration = Duration::from_millis(200);
const TIME_STEP: f64 = 0.1;
const NOISE_AMPLITUDE: f64 = 0.03;
const DROPOUT_DENOMINATOR: u64 = 20;

fn next_uniform(state: &mut u64) -> f64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

fn main() -> std::io::Result<()> {
    let path = env::args().nth(1).unwrap_or_else(|| "capture.analog-capture".to_string());
    // A real capture tool starting a fresh capture truncates whatever
    // was there before, rather than resuming mid-timeline.
    let mut file = File::create(&path)?;
    writeln!(file, "time,ch1_sensor,ch2_reference")?;
    file.flush()?;
    let mut file = OpenOptions::new().append(true).open(&path)?;

    let mut rng_state: u64 = 0x616e_616c_6f67;
    let mut t = 0.0f64;

    loop {
        thread::sleep(TICK);
        t += TIME_STEP;

        let noise1 = (next_uniform(&mut rng_state) - 0.5) * 2.0 * NOISE_AMPLITUDE;
        let noise2 = (next_uniform(&mut rng_state) - 0.5) * 2.0 * NOISE_AMPLITUDE;
        let ch1 = t.sin() + noise1;
        let ch2 = t.cos() + noise2;

        let dropout_roll = (next_uniform(&mut rng_state) * DROPOUT_DENOMINATOR as f64) as u64;
        let ch1_text = if dropout_roll == 0 { String::new() } else { format!("{ch1:.4}") };
        let ch2_text = if dropout_roll == 1 { String::new() } else { format!("{ch2:.4}") };

        writeln!(file, "{t:.2},{ch1_text},{ch2_text}")?;
        file.flush()?;
    }
}
