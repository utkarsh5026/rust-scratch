---
description: Explain an existing experiment line-by-line, like a tutor
argument-hint: <name>
allowed-tools: Read, Glob
---

Walk through an existing experiment and teach it.

Target: $ARGUMENTS

Steps:
1. Read `src/bin/$ARGUMENTS.rs` (or `src/main.rs` if no argument).
   If it doesn't exist, list available bins and ask.
2. Explain it from first principles, as if teaching the concept:
   - What Rust concept is being demonstrated and why it matters.
   - Walk through the key lines, including any lifetimes, traits, borrows, or
     async machinery in play.
   - Call out anything subtle, any compiler rules involved, and common mistakes.
3. Don't modify the file — this is read-and-teach only.
