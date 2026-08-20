# emc

I plot a synthetic EMC emissions sweep (30 MHz–1 GHz) against a
CISPR-style limit line, with a measurement-uncertainty band around the
trace — the shape of a real compliance report, not an invented demo
curve. This is the one view of mine that exercises all four chart-shaped
primitives from a single plugin, which makes it my own go-to reference
for "what can a view plugin actually do."

**Data source:** none — synthetic, generated internally.

**Primitives and commands:**
- default (`curve` mode): `xy-chart` (measured trace, limit line,
  uncertainty band, a pass/warn/fail marker at the cursor) + `table`
  (peak/average/RMS/margin).
- `heatmap` — toggles a 2D field view of the same underlying data.
- `composed` — toggles a composed view layering the field and markers
  together.
- `cursor <freq_mhz>` — moves the cursor to the nearest real data
  point at that frequency and recomputes the margin against it.

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/emc.wasm`. Copy it
into a project's `.iderm/views/` to use it.
