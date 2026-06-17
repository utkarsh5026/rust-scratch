# rust-scratch

A personal Rust playground for trying out concepts. Not a real project — no
product, no architecture, no stability guarantees. The goal is: when I hit a
tricky concept and want to *write code and check it*, I drop a file here and run
it immediately.

## How it's structured

A single Cargo crate. Experiments are independent binaries.

```
rust-scratch/
├── Cargo.toml
└── src/
    ├── main.rs          # scratch pad — quick throwaway code
    └── bin/             # one file per concept, each with its own fn main()
        ├── lifetimes.rs
        └── traits.rs
```

## Workflow

- **Quick throwaway** → edit `src/main.rs`, run `cargo run`
- **Keep a concept** → create `src/bin/<concept>.rs` with its own `fn main()`,
  run `cargo run --bin <concept>`
- **Async experiment** → add `#[tokio::main]` to `async fn main()`
- **Need a crate** → add it under `[dependencies]` in `Cargo.toml`

Each file in `src/bin/` is fully self-contained. They don't import each other.

## Available dependencies

Common crates are preinstalled so experiments don't need setup:

- **async**: `tokio` (full), `futures`, `async-trait`
- **errors**: `anyhow`, `thiserror`
- **serde**: `serde`, `serde_json`
- **http**: `reqwest`
- **utils**: `rand`, `tracing`, `tracing-subscriber`

## Guidance for Claude

- This is throwaway/learning code. Favor clarity that *demonstrates the concept*
  over production patterns. Comments explaining *why* are welcome.
- Don't over-engineer. No need for tests, error handling ceremony, or module
  splitting unless the concept being explored is exactly that.
- When adding a new experiment, put it in `src/bin/<name>.rs` and note the run
  command (`cargo run --bin <name>`) in a top comment.
- Don't delete or rewrite existing experiments — they're notes-to-self. Add new
  files instead.
