# rust-scratch

My personal **learn-by-doing** workspace for mastering Rust. Not a product — a
practice gym. I name a concept, get a ladder of problems, and implement each one
myself with coaching (never handed the answer), until I actually understand it.

The whole thing is built around the **`rust-practice` skill** + a structured
mastery curriculum in [`ROADMAP.md`](./ROADMAP.md).

## The core loop

1. Name a concept → `practice <concept>` (or `/rust-practice <concept>`)
2. A ladder of **7-9 problems** (easy → mastery) is scaffolded into `src/bin/<concept>.rs`
3. Implement one rung at a time; run `cargo run --bin <concept>` to check
4. Get **staged hints** if stuck — the solution is revealed only if you ask
5. Each ladder ends in a **build-it-from-scratch capstone**

Every solved rung earns **XP** and feeds a gamified dashboard (`cargo run --bin
stats` or `/stats`): ranks 🥚→🦀→…→👑, achievements (Hint-Free, One-Shot,
Miri-Clean, Capstone, Phase Clear), and a daily **streak**.

```
practice Rc and RefCell      # start a new ladder
cargo run --bin rc_refcell   # run it — passes the solved rungs, stops at your next todo!
```

## Quick start

```bash
# run the scratch pad (quick throwaway code)
cargo run

# run a specific concept ladder
cargo run --bin cow

# start a fresh concept (in Claude Code)
#   "practice lifetimes"   or   /rust-practice lifetimes
```

## How a concept file works

One file per concept. Each problem is a function you implement + a `check_N()`
that asserts it. `main` runs them in order, so it replays everything you've
solved and stops at the first unfinished rung (its `todo!` panics).

```rust
fn solve_it(...) -> ... {
    todo!("your turn")          // <- you fill this in
}

fn check_3() { /* asserts solve_it works */ }

fn main() {
    check_1();
    check_2();
    check_3();   // <- run stops here until you implement rung 3
    // check_4();
}
```

## The curriculum

[`ROADMAP.md`](./ROADMAP.md) is a 9-phase path from comfortable-with-basics to
advanced Rust engineer. Each item is a ladder to run:

| Phase | Theme |
|-------|-------|
| 0 | Tooling, project structure & testing (features, property tests, fuzzing) |
| 1 | Ownership, conversions & the type system (smart pointers, lifetimes, variance, HRTB) |
| 2 | Traits & generics like a library author (object safety, GATs, const generics) |
| 3 | API & error design (typestate, semver, strings, collections) |
| 4 | Concurrency (atomics, memory ordering, lock-free, rayon) |
| 5 | Async internals (`Future`, `Pin`, build an executor, tower) |
| 6 | Unsafe & the machine (raw pointers, layout, Stacked Borrows, implement `Vec`) |
| 7 | Performance & low-level craft |
| 8 | Metaprogramming (declarative & proc macros) |
| 9 | Specialization tracks (no_std/embedded · web · WASM · CLI) |
| 10 | Capstones |

~90 ladders · ~700 hands-on problems.

Say **"what's next"** to pull the next unchecked item and start its ladder.

## Completed concepts

| Concept | File | Rungs |
|---------|------|-------|
| `Cow` (Clone-on-Write) | [`src/bin/cow.rs`](./src/bin/cow.rs) | 9 — basics → serde zero-copy → reimplement from scratch |

_(early standalone demos, not ladders: `src/bin/lifetimes.rs`, `src/bin/traits.rs`)_

## Layout

```
rust-scratch/
├── README.md       ← you are here
├── ROADMAP.md      ← the mastery curriculum
├── CLAUDE.md       ← how Claude Code should drive this project
├── Cargo.toml
├── .claude/
│   ├── skills/rust-practice/   ← the practice-ladder coach
│   └── commands/               ← /experiment, /run, /explain, /list
└── src/
    ├── main.rs                 ← scratch pad (cargo run)
    └── bin/                    ← one file per concept ladder
```

## Slash commands

| Command | Does |
|---------|------|
| `/rust-practice <concept>` | start a coached practice ladder |
| `/stats` | show the gamified dashboard — rank, XP, phases, badges, streak |
| `/list` | show ladders & per-concept progress |
| `/run <name>` | run an experiment and explain its output |
| `/explain <name>` | tutor-mode walkthrough of an existing file |
| `/experiment <concept>` | scaffold a one-off demo (not a ladder) |

## Preinstalled dependencies

So ladders never need setup: `tokio`, `futures`, `async-trait`, `anyhow`,
`thiserror`, `serde` + `serde_json`, `reqwest`, `rand`, `tracing`. Add more in
`Cargo.toml` as a concept needs them.
