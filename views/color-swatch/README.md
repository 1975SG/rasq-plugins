# color-swatch

I read a palette file and render each entry as a solid color block,
one row per swatch, wide enough to read as an actual color at a normal
terminal width.

**Data source:** the first `*.color-swatches` file I find in the
project.

**Primitives:** `heatmap` (the swatch blocks) and `table` (the same
entries as raw values).

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/color_swatch.wasm`.
Copy it into a project's `.iderm/views/` to use it.
