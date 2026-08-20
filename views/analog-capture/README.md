# analog-capture

I read a generic multi-channel analog capture and chart it. This is the
plugin behind most of my own real-hardware demos — a USB audio mixer, a
bench multimeter, anything that can be reduced to "one row per tick,
one column per channel."

**Data source:** the first `*.analog-capture` file I find in the
project (`time,ch1,ch2,...` CSV; a missing or `nan` cell is a real gap,
not an error). If more than one exists, `stream: <filename>` pins me to
a specific one.

**Primitives:** `xy-chart` (one series per channel, up to 8 charted —
the rest still list in the table) and `table` (min/max/mean per
channel).

**Live refresh:** I re-read the file fresh on every tick, so a capture
a still-running task is appending to stays current.

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/analog_capture.wasm`.
Copy it into a project's `.iderm/views/` to use it.
