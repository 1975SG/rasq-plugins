# frame-builder

I build every frame declared in a frame spec, then inspect each one
back — the builder half proves the spec is well-formed, the inspector
half shows me what actually came out, both from one file.

**Data source:** the first `*.frame-format` file I find in the
project.

**Primitives:** `table` (one row per built frame, with its resulting
byte layout).

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/frame_builder.wasm`.
Copy it into a project's `.iderm/views/` to use it.
