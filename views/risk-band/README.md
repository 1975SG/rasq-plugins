# risk-band

I run a Monte Carlo simulation and chart the P10/P50/P90 band over
time — a fan chart, the shape I reach for whenever the question is
"how wide is the actual uncertainty," not just "what's the expected
value." Self-contained.

**Data source:** none — synthetic, generated internally.

**Primitives:** `xy-chart` (the P10/P50/P90 fan) and `table` (summary
percentiles).

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/example_view_risk_band.wasm`.
Copy it into a project's `.iderm/views/` to use it.
