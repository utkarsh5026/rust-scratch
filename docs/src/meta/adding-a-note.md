# Adding a new note

The workflow for capturing a concept the moment a ladder is done.

## Steps

1. **Copy the [template](template.md)** into `docs/src/concepts/<concept>.md`
   (kebab-case, e.g. `cell-refcell.md`).
2. **Fill every section** — TL;DR, why, the ladder table, signatures, footguns,
   "explain it back". Pull the rung list straight from the `// Ladder:` comment at
   the top of `src/bin/<concept>.rs`.
3. **Register it in [`SUMMARY.md`](https://github.com/utkarsh5026/rust-scratch/blob/master/docs/src/SUMMARY.md)**
   under the right Phase heading. *A page not listed in `SUMMARY.md` is not built.*
4. **Tick the row** in the completed-concepts table on the [Introduction](../intro.md).
5. Commit & push to `master` — the `Deploy docs` GitHub Action rebuilds and
   publishes the site automatically.

## Local preview

```bash
# one-time: install mdBook
cargo install mdbook        # or: brew install mdbook

# live-reloading preview at http://localhost:3000
mdbook serve docs --open

# one-off build into docs/book/ (gitignored)
mdbook build docs
```

## Just say the word

In Claude Code you can simply say *"write up the `<concept>` note"* after a ladder
is finished — it reads the source file and drafts the page in this format.

## One-time GitHub setup

For the deploy workflow to publish, set **Settings → Pages → Build and deployment
→ Source = GitHub Actions** once on the repo. After that, every push to `master`
that touches `docs/` redeploys to `https://utkarsh5026.github.io/rust-scratch/`.
