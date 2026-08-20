# option-surface

I read a declared option-pricing scenario and render its Black-Scholes
value surface as a heatmap over strike and time-to-expiry.

**Data source:** the first `*.option-surface` file I find in the
project.

**Primitives:** `heatmap` (the value surface) and `table` (sampled
values, greeks).

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/option_surface.wasm`.
Copy it into a project's `.iderm/views/` to use it.
