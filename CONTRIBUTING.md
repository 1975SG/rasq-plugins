# Contributing

## What I expect from a plugin

1. One plugin, one job. A view that mixes two unrelated data shapes,
   or a rule that bundles two unrelated checks, should be two plugins
   instead.
2. Stay inside the sandbox contract. A doctor rule reads project state
   through `host` only and never writes anything — that's what keeps
   Doctor safe to run automatically. A repair recipe proposes new
   files only, never an edit to an existing one. A view returns
   primitives, never raw terminal control.
3. A short doc comment explaining *why* the plugin exists, not just
   what it does. A finding or a chart a reader can't act on without
   more context is weaker than one that names the actual problem.
4. Test it against a real case that exhibits both the pass and the
   fail path (for a doctor rule) or real representative data (for a
   view) before opening a PR — not just that it compiles.
5. State which `iderm:plugin` ABI version (from your vendored `.wit`
   file's `package` line) the plugin was built and tested against, in
   the PR description.

## License and the sign-off line

Everything here is dual-licensed under MIT or Apache-2.0, matching
Core (`LICENSE-MIT` / `LICENSE-APACHE`). By opening a PR you're
confirming you have the right to offer your contribution under that
same license — add a `Signed-off-by: Your Name <email>` line to each
commit (`git commit -s` adds it for you) as that confirmation. No
separate paperwork, just that one line.

## Before a plugin actually lands

I review each submission for: does it build clean against the current
ABI, does it stay inside its plugin kind's contract, and does it hold
up against a real test case, not just a synthetic happy path. A
plugin that fails any of those doesn't land yet, not never — tell me
what's blocking it and I'll help get it there.
