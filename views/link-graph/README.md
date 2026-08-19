# link-graph

I read a declared network topology and draw it as a node/edge graph,
with three path-finding modes over the same graph depending on what
question I'm actually asking about it.

**Data source:** the first `*.transport-graph` file I find in the
project.

**Primitives:** `state-graph` (the topology) and `table` (edge list).

**Commands:** `bandwidth`, `latency`, `reliability` — each recomputes
and highlights the best path through the graph by that specific
metric.

A declared topology is static in the common case I originally built
this for, but a live-changing mesh (nodes and links appearing and
dropping in real time) is a real future producer for this same file
format — worth keeping in mind if I ever wire a live source to this.

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/example_view_link_graph.wasm`.
Copy it into a project's `.iderm/views/` to use it.
