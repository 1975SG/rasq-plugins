# tla

I carry a small, hand-authored mutual-exclusion state machine and one
real counterexample through it — a second process entering `Critical`
while the first is still there. Self-contained, no `.tla` file parsing;
this is meant to prove the `state-graph`/`trace` primitives against a
real, textbook violation, the same kind formal-methods tooling like the
TLA+ Toolbox shows.

**Data source:** none — synthetic, generated internally.

**Primitives and commands:**
- default: `trace` (the counterexample sequence, step by step).
- `graph` — toggles to `state-graph`, the full reachable-state graph
  the counterexample was drawn from.

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/tla.wasm`. Copy it into a
project's `.iderm/views/` to use it.
