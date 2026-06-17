---
description: List practice ladders and experiments, with progress on each
allowed-tools: Read, Bash(ls:*), Bash(grep:*), Glob
---

Show what's been practiced in this project, combining the CLAUDE.md index with
the actual files.

1. Read the **Completed concepts** table in `CLAUDE.md` — that's the canonical
   list of finished/in-progress ladders.
2. For each concept file in `src/bin/`, read its top "Ladder:" comment and count
   how many rungs are marked done vs total (e.g. "Cow — 9/9 done"). For files
   without a ladder (plain demos like `lifetimes.rs`, `traits.rs`) and
   `src/main.rs`, just give a one-line summary.
3. Cross-check: flag any concept file missing from the CLAUDE.md table (suggest
   adding a row), or any table row whose file is gone.

Present as a compact table sorted by name: concept | run command | progress
(rungs done / total) | one-line summary. Don't run cargo — read only.
