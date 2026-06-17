---
description: List all experiments with a one-line summary of each
allowed-tools: Read, Bash(ls:*), Glob
---

List every experiment in this scratch project.

For each file in `src/bin/` (and `src/main.rs`):
1. Read its top comment / contents.
2. Show: the run command (`cargo run --bin <name>`, or `cargo run` for main.rs)
   and a one-line summary of the concept it demonstrates.

Present as a compact table or list, sorted by name. Don't run anything.
