---
description: Scaffold and implement a new Rust concept experiment as a runnable bin
argument-hint: <concept-name> [what to demonstrate]
allowed-tools: Write, Edit, Read, Bash(cargo:*)
---

Create a new self-contained experiment that demonstrates a Rust concept.

Concept / request: $ARGUMENTS

Steps:
1. Pick a short kebab-case file name from the concept (e.g. "Pin and async" → `pin-async`).
   If `$ARGUMENTS` is empty, ask what concept to explore.
2. Create `src/bin/<name>.rs`. It MUST:
   - Start with a top comment: the concept, a one-line explanation, and the run
     command `cargo run --bin <name>`.
   - Have its own `fn main()` (use `#[tokio::main] async fn main()` if async).
   - Be fully self-contained (no imports from other bins).
   - Use comments to explain *why*, not just *what* — this is learning code.
   - Show the concept concretely with small examples and `println!` output that
     makes the behavior visible.
3. Run `cargo run --bin <name>` and confirm it compiles and produces sensible output.
4. Report the run command and a 1-2 line summary of what the experiment shows.

Favor clarity that demonstrates the concept over production patterns. Don't
over-engineer. Don't touch existing experiments.
