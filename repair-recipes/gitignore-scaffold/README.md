# gitignore-scaffold

I propose a starter `.gitignore` for any project that doesn't have one
yet — a small, genuinely useful default: editor swap files, OS
metadata, a generic `target`/`build`/`dist` catch-all. Nothing
language-specific, since I have no reliable way to know what languages
a project actually uses beyond what it's already declared, and
guessing wrong would be worse than a short, safe default.

**Applies when:** no `.gitignore` exists at the project root.

**Proposes:** one new file, `.gitignore`, never an edit to an existing
one — repair recipes can only propose brand-new files, by design.

## Build

```sh
cargo component build --release
```

Produces `target/wasm32-wasip1/release/example_repair_recipe.wasm`.
Copy it into a project's `.iderm/repair-recipes/` to use it.
