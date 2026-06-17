---
description: Run an experiment bin and explain its output
argument-hint: <name> (omit to run the scratch pad / src/main.rs)
allowed-tools: Bash(cargo:*), Read, Glob
---

Run a Rust experiment and explain what happened.

Target: $ARGUMENTS

Steps:
1. If `$ARGUMENTS` is empty, run `cargo run` (the scratch pad).
   Otherwise run `cargo run --bin $ARGUMENTS`.
   - If the name doesn't match a file in `src/bin/`, list the available bins and ask.
2. If it fails to compile, read the file, explain the error in plain terms, and
   propose a fix (apply it only if it's an obvious typo; otherwise ask).
3. On success, show the output and explain what it demonstrates about the concept.
