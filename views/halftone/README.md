# halftone

I render each declared halftone screen as its own dot-pattern band,
wide enough per band to actually read as a pattern rather than noise.

**Data source:** the first `*.halftone-screens` file I find in the
project.

**Primitives:** `heatmap` (the dot-pattern bands) and `table` (screen
parameters — angle, frequency, per screen).

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/example_view_halftone.wasm`.
Copy it into a project's `.iderm/views/` to use it.
