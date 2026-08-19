# wave-shoaling

I read a `depth,wave_height,wavelength` capture and chart how a wave
transforms as it moves into shallower water — the shoaling effect. I
recognize a capture by its actual column shape, not by filename, so
any script writing that shape feeds me correctly.

**Data source:** the first matching `*.analog-capture` file whose
header is `depth,wave_height,wavelength`.

**Primitives:** `xy-chart` (height and wavelength over depth) and
`table` (summary values).

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/example_view_wave_shoaling.wasm`.
Copy it into a project's `.iderm/views/` to use it.
