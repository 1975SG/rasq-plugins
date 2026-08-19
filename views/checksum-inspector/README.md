# checksum-inspector

I show a small table of framed byte payloads and their CRC-32, one row
per frame, flagging any frame whose stored checksum doesn't match what
I recompute. Self-contained — I generate a deterministic synthetic set
of frames myself, no project file needed, so I always have something
real to show.

**Data source:** none — synthetic, generated internally.

**Primitives:** `table` (frame, bytes, CRC-32, pass/mismatch status).

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/example_view_checksum_inspector.wasm`.
Copy it into a project's `.iderm/views/` to use it.
