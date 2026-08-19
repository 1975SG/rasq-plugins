# distribution

I show a dataset three ways at once — histogram, empirical CDF, and a
linear-regression fit — the combination I use whenever I need to check
whether something actually looks like the distribution I expect it to,
not just eyeball a single chart. Self-contained.

**Data source:** none — synthetic, generated internally.

**Primitives:** `xy-chart` (histogram/CDF/fit) and `table` (fit
parameters, outlier count).

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/example_view_distribution.wasm`.
Copy it into a project's `.iderm/views/` to use it.
