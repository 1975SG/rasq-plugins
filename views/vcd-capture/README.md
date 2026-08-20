# vcd-capture

I read a digital logic capture and draw each signal as a step-shaped
waveform, one row per channel — my counterpart to `analog-capture` for
digital/logic-analyzer data. An unknown or high-Z value (`x`/`z`) is
drawn at the low level, but only as a rendering choice — it's never
claimed to mean an actual `0`.

**Data source:** the first `*.vcd` file I find in the project
(IEEE 1364/1800 value-change dump syntax — scalar and vector changes,
scope hierarchy not retained).

**Primitives:** `xy-chart` (the step waveforms, up to 8 charted) and
`table` (signal width and transition count).

**Live refresh:** I re-read the file fresh on every tick, so a capture
a still-running logic analyzer task is appending to stays current.

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/vcd_capture.wasm`. Copy it into
a project's `.iderm/views/` to use it.
