# languages

These are language manifests for iderm — first-party, but not active
by default in Core. I keep them here rather than bundled in, since
they cover niche languages most projects won't use. Copy the one you
want into a project's `.iderm/languages/` to activate it.

| File | Language | Detected by | LSP |
|---|---|---|---|
| `fortran.toml` | Fortran (fpm) | `fpm.toml` at root | `fortls` |
| `latex.toml` | LaTeX | `*.tex` | `texlab` |
| `murphi.toml` | Murphi | `*.m` | none — I verified no LSP exists for it |
| `promela.toml` | Promela | `*.pml` | none — I verified no standalone LSP exists for it |
| `tla.toml` | TLA+ | `*.tla` | none — I verified no standalone LSP exists for it |

Where a manifest has no `lsp_binary`, that's a checked fact, not an
oversight — I looked before leaving it out.

## Format

A manifest declares how to detect the language (`detection_file` for
an exact root filename, `detection_glob` for a non-recursive glob over
root files), a display `label`, an optional `lsp_binary`, and an
optional `tree_sitter_grammar`. See Core's own `docs/modules/Config.md`
for the full schema.
