# emergence

I step a cellular-automaton pattern forward one generation per tick and
render its current state as a grid. This is the one view of mine where
live-refresh isn't cosmetic — each tick genuinely advances the
simulation, which is the whole point of watching a macroscopic pattern
emerge from a simple local rule.

**Data source:** the first `*.ca-pattern` file I find in the project
(the starting generation and rule).

**Primitives:** `heatmap` (the current grid state) and `table` (live
cell counts).

**Live refresh:** each tick is a real simulation step, not a re-read —
turn it on to actually watch the pattern evolve.

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/emergence.wasm`.
Copy it into a project's `.iderm/views/` to use it.
