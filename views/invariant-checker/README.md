# invariant-checker

I read every invariant report in the project and show one table row
per invariant, with its actual status — holds, violated, or not fully
explored. Deliberately not live-refreshed: a model-checker report is
the result of a completed run, not a value that changes on its own
between ticks, and pretending otherwise would misrepresent what this
panel is actually showing.

**Data source:** every `*.invariants` file in the project. Plain-text
format, one invariant per line: `name|status|description`
(`status` is `holds`, `violated`, or `unknown`).

**Primitives:** `table` (name, status, description, per invariant,
plus a per-file summary row).

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/example_view_invariant_checker.wasm`.
Copy it into a project's `.iderm/views/` to use it.
