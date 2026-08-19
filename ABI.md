# ABI reference

Every plugin here targets `iderm:plugin@0.4.0`, the WIT package Core
defines. I vendor a copy of `wit/deps/iderm-plugin/iderm-plugin.wit`
into each plugin's own directory rather than sharing one file across
the repo — a plugin should build and version independently of every
other plugin here, the same way each one does inside Core's own
example set.

## Versioning

I follow the same MAJOR.MINOR.PATCH policy Core itself commits to:

- **MAJOR** — a breaking change to `types`/`host`/one of the plugin
  worlds. A plugin built against an older MAJOR won't load.
- **MINOR** — additive only (a new optional field, a new interface). A
  plugin built against an older MINOR still loads against a newer
  Core.
- **PATCH** — no interface change.

A plugin's `Cargo.toml` doesn't declare a compatible range of its own —
the vendored `.wit` file's `package` line *is* the declaration. Moving
a plugin to a newer Core ABI means re-copying that file from the Core
repo I'm building against and rebuilding.

## Plugin kinds

| Kind | World | Contract |
|---|---|---|
| Doctor rule | `doctor-plugin` | `check(project) -> list<finding>` — read-only, pure |
| Repair recipe | `repair-plugin` | `propose(project) -> list<file-change>` — new files only, Core owns the write |
| View provider | `view-plugin` | `render`/`handle-command` -> `list<view-primitive>` |

## View primitives

A view provider returns one or more of: `xy-chart`, `heatmap`,
`table`, `state-graph`, `trace`, `composed`. See each plugin's own
README for which it uses.

## Host capabilities

A plugin gets exactly three host imports: `read-file`, `list-files`,
`log`. No preopened directory, no environment, no argument, no network
access. File reads and glob results are constrained to the project
root — I never assume more access than that, and neither should
anything I build here.

## Building

Every plugin needs [`cargo-component`](https://github.com/bytecodealliance/cargo-component):

```sh
cargo install cargo-component
cd <category>/<plugin-name>
cargo component build --release
```

Each build produces one `.wasm` under `target/wasm32-wasip1/release/`.
