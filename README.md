# iderm-plugins

I'm the plugin ecosystem for [iderm](https://github.com/1975SG/iderm),
my terminal-first, project-aware IDE. Core ships a small set of
built-in checks and no bundled views at all — this repo is where I put
everything beyond that: view providers, repair recipes, and the
niche language manifests I don't embed by default.

Doctor rules have their own repo, `iderm-doctor-rules` — I keep that
one separate since it existed first and I saw no reason to fold real,
working history into a rename.

**Status: private, pre-OSS staging.** This repo exists ahead of
iderm's own public release, so I can get the structure, the ABI
convention, and the contribution shape right while nobody but me is
depending on any of it yet.

## Layout

```
views/<name>/            one crate per view plugin
repair-recipes/<name>/   one crate per repair recipe
languages/                niche language manifests (not WASM plugins)
libs/<name>/              shared Rust crates a handful of the plugins above depend on
```

Every plugin under `views/` and `repair-recipes/` is its own crate,
not a workspace member sharing one `Cargo.toml` — I want each one to
build and version on its own, the same way Core's own example plugins
do. A handful of the view plugins lean on a small shared library crate
under `libs/` (parsing a capture format, a stats routine, that kind of
thing) rather than reimplementing it per plugin — those are linked by
relative path, so cloning this repo whole is all a plugin needs.

Each plugin directory has its own `README.md` — what it shows, where
its data comes from (or that it's self-contained and synthetic), and
which primitives/commands it uses.

## ABI

See `ABI.md` for the shared WIT contract, the plugin kinds, and the
versioning policy every plugin here follows.

## Adding a plugin

1. Pick the closest existing plugin in the right category and copy its
   directory as a starting point.
2. Rename the crate (`Cargo.toml`'s `name` and
   `package.metadata.component.package`) and whatever ID string the
   plugin returns (`rule_id()` / `recipe_id()` / `view_id()`).
3. Implement it against the contract for its kind — see `ABI.md`.
4. `cargo component build --release` from inside the plugin's own
   directory.
5. Open a PR. See `CONTRIBUTING.md`.

## License

Dual-licensed under MIT or Apache-2.0, matching iderm itself — see
`LICENSE-MIT` / `LICENSE-APACHE`.
