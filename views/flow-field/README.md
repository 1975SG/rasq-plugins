# flow-field

I render a 2D potential-flow field — velocity or pressure over a grid
around an obstacle — as a heatmap. Useful whenever I want to look at a
field quantity spatially rather than as a single number over time.

**Data source:** the first `*.flow-field` file I find in the project.

**Primitives:** `heatmap` (the field) and `table` (sampled values).

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/flow_field.wasm`.
Copy it into a project's `.iderm/views/` to use it.
