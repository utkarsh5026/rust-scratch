# rust-scratch

A personal Rust **learn-by-doing** workspace. This is not a product — it's where
I drill Rust concepts until I actually understand them. The whole project is
built around one workflow: pick a concept, get a ladder of problems, and
implement each one myself with coaching (never handed the solution).

The engine for this is the **`rust-practice` skill** (`.claude/skills/rust-practice/`).

## The core loop

I name a concept → the skill builds a **ladder of 7-9 problems** (easy → mastery)
in a single file `src/bin/<concept>.rs` → I implement one rung at a time, running
`cargo run --bin <concept>` to check → I get staged hints if stuck, never the
answer up front.

Just say e.g. *"practice `Rc`/`RefCell`"*, *"drill iterators"*, or
`/rust-practice <concept>` to start a new ladder.

The full curriculum lives in **`ROADMAP.md`** — a 9-phase path from basics to
advanced Rust (ownership → traits → async internals → unsafe → perf → macros →
capstones). Each item there is a ladder to run with the practice skill. When I
ask "what's next" or "give me the next concept", pull the next unchecked item
from ROADMAP.md, build its ladder, and tick the box when I finish it.

### How a concept file is laid out

- One file per concept: `src/bin/<concept>.rs`.
- Each problem = a function I implement + a `check_N()` that asserts it.
- `main` runs `check_1(); check_2(); …` in order, so it replays every solved rung
  and stops at the first unfinished one (its `todo!` panics).
- A "Ladder:" comment at the top lists all rungs and marks which are done.
- The ladder goes all the way to mastery: foundations → mechanics → footguns &
  edge cases → real-world patterns → a build-it-from-scratch capstone.

## Completed concepts

| Concept | File | Run | Rungs |
|---------|------|-----|-------|
| `Cow` (Clone-on-Write) | `src/bin/cow.rs` | `cargo run --bin cow` | 9 — basics → serde zero-copy → reimplement from scratch |
| `Box` & the heap | `src/bin/box_heap.rs` | `cargo run --bin box_heap` | 9 — basics → recursive types → `dyn Trait`/`dyn Error`/`Box::leak` → hand-rolled linked list w/ iterative Drop |
| `Rc` / `Arc` | `src/bin/rc_arc.rs` | `cargo run --bin rc_arc` | 9 — shared ownership → `strong_count`/diamond DAG → `Rc<str>`/`make_mut` → cycle leak & `Weak` fix → `Arc<Mutex>` across threads → hand-rolled `MyRc<T>` |
| `Cell` / `RefCell` | `src/bin/cell_refcell.rs` | `cargo run --bin cell_refcell` | 9 — interior mutability → Cell/RefCell toolboxes → runtime borrow panic & re-entrancy → `Rc<RefCell>` graph → `Ref::map` → hand-rolled `MyRefCell` (`UnsafeCell` + flag + RAII guards) |
| Conversion traits | `src/bin/conversions.rs` | `cargo run --bin conversions` | 9 — `From`/`.into()` free → `impl Into` bounds → `From` powers `?` → `TryFrom` → orphan rule & reflexivity → `as` truncation vs `TryInto` → `AsRef<str>`/`AsRef<Path>`/`AsMut` → mini `serde_json::Value` (From in, TryFrom out, AsRef lookup) |

_(early standalone demos, not ladders: `src/bin/lifetimes.rs`, `src/bin/traits.rs`)_

## Project layout

```
rust-scratch/
├── Cargo.toml
├── CLAUDE.md
├── ROADMAP.md                  # the 9-phase mastery curriculum (what to practice next)
├── .claude/
│   ├── skills/rust-practice/   # the practice-ladder coach (the heart of this project)
│   └── commands/               # /experiment, /run, /explain, /list
└── src/
    ├── main.rs                 # scratch pad for quick throwaway code (cargo run)
    └── bin/                    # one file per concept ladder
```

## Available dependencies

Preinstalled so ladders never need setup:

- **async**: `tokio` (full), `futures`, `async-trait`
- **errors**: `anyhow`, `thiserror`
- **serde**: `serde`, `serde_json`
- **http**: `reqwest`
- **utils**: `rand`, `tracing`, `tracing-subscriber`

Add more under `[dependencies]` in `Cargo.toml` as a concept needs them.

## Guidance for Claude

- **Default to the `rust-practice` skill.** When I name a Rust concept I want to
  learn/practice/drill, build a ladder — don't just explain it or write a demo.
- **Coach, don't solve.** Never pre-fill a solution body. Use `todo!()` / `// TODO`
  and give hints in stages; reveal the answer only if I explicitly ask.
- **One rung at a time.** Show the full ladder up front, but scaffold and hand
  over only the current problem.
- **Reach mastery.** Take ladders all the way (edge cases, real-world patterns,
  a from-scratch capstone). When in doubt, add another rung.
- **Keep the file compiling.** A scaffolded `todo!` should panic at runtime, not
  break compilation — otherwise earlier solved rungs stop running.
- **Don't rewrite my solved rungs.** They're notes-to-self. Append new rungs/files.
- When a concept ladder is finished, add a row to the **Completed concepts** table.
- For pure deep teaching (not practice), defer to the `/rustacean` skill.
