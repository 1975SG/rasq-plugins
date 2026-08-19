# deviation

I plot a measured series against a target value with a confidence band
around it — the shape I reach for whenever the question is "how far off
is this, and is that drift real or noise." Self-contained, so I always
have a real series to show.

**Data source:** none — synthetic, generated internally.

**Primitives:** `xy-chart` (measured series, target line, confidence
band) and `table` (summary statistics).

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/example_view_deviation.wasm`.
Copy it into a project's `.iderm/views/` to use it.
