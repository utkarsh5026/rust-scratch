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

**Phase 0 — Tooling, project structure & testing**

| Concept | File | Rungs |
|---------|------|-------|
| Modules & visibility | [`src/bin/modules.rs`](./src/bin/modules.rs) | 9 — module tree & paths → `pub` opens a door → field privacy + smart constructor → `use` vs `pub use` re-export → leaking a private type (E0603/E0616) → `pub(crate)`/`pub(super)`/`pub(in path)` → facade pattern → sealed trait via private module → `inventory` mini-library capstone |

**Phase 1 — Ownership, conversions & the type system**

| Concept | File | Rungs |
|---------|------|-------|
| `Cow` (Clone-on-Write) | [`src/bin/cow.rs`](./src/bin/cow.rs) | 9 — basics → serde zero-copy → reimplement from scratch |
| `Box` & the heap | [`src/bin/box_heap.rs`](./src/bin/box_heap.rs) | 9 — recursive types → `dyn Trait`/`Box::leak` → hand-rolled linked list |
| `Rc` / `Arc` | [`src/bin/rc_arc.rs`](./src/bin/rc_arc.rs) | 9 — shared ownership → cycles & `Weak` → `Arc<Mutex>` → hand-rolled `MyRc` |
| `Cell` / `RefCell` | [`src/bin/cell_refcell.rs`](./src/bin/cell_refcell.rs) | 9 — interior mutability → borrow panics → hand-rolled `MyRefCell` |
| Conversion traits | [`src/bin/conversions.rs`](./src/bin/conversions.rs) | 9 — `From`/`Into` → `TryFrom` → `AsRef` → mini `serde_json::Value` |
| Lifetimes in depth | [`src/bin/lifetimes_depth.rs`](./src/bin/lifetimes_depth.rs) | 9 — elision → structs → outlives bounds → hand-rolled `StrSplit` |
| `Borrow` / `ToOwned` | [`src/bin/borrow_toowned.rs`](./src/bin/borrow_toowned.rs) | 9 — `HashMap::get(&str)` → contracts → hand-rolled `MyCow` |
| `Drop` & ordering | [`src/bin/drop_ordering.rs`](./src/bin/drop_ordering.rs) | 9 — LIFO/field order → drop flags → rollback-on-drop `Transaction` |
| `Rc<RefCell<T>>` patterns | [`src/bin/rc_refcell.rs`](./src/bin/rc_refcell.rs) | 10 — shared cell → cycle leak + `Weak` → doubly-linked list w/ iterative Drop |
| HRTB — `for<'a>` | [`src/bin/hrtb.rs`](./src/bin/hrtb.rs) | 9 — implicit `for<'a>` → `DecodeOwned` → parser-combinator capstone |

**Phase 2 — Traits & generics like a library author**

| Concept | File | Rungs |
|---------|------|-------|
| Generic bounds & `where` clauses | [`src/bin/generic_bounds.rs`](./src/bin/generic_bounds.rs) | 9 — bounds → `?Sized` → blanket impls → `IterExt` extension trait |
| Associated types vs generic params | [`src/bin/assoc_vs_generic.rs`](./src/bin/assoc_vs_generic.rs) | 9 — one-impl-per-type vs many → `dyn` assoc pinning → `MyIterator` + `Map` |
| Blanket impls & coherence | [`src/bin/blanket_coherence.rs`](./src/bin/blanket_coherence.rs) | 9 — `From`→`Into` → orphan rule → sealed extension trait |
| Static vs dynamic dispatch | [`src/bin/dispatch.rs`](./src/bin/dispatch.rs) | 9 — monomorphization vs vtable → object safety → static/dynamic/enum pipeline |
| Closures & `Fn`/`FnMut`/`FnOnce` | [`src/bin/closures.rs`](./src/bin/closures.rs) | 9 — capture modes → `Fn ⊂ FnMut ⊂ FnOnce` → desugar by hand → `impl Fn` vs `Box<dyn Fn>` → fn-pointer coercion → `Box<dyn FnMut>` event dispatcher |
| `impl Trait` & RPIT | [`src/bin/impl_trait.rs`](./src/bin/impl_trait.rs) | 9 — APIT (caller picks) vs RPIT (callee picks one hidden type) → turbofish footgun → return closures/chains → one-type rule (Box/Vec/`Either`) → 2024 lifetime auto-capture + `use<>` → `async fn` ≡ `-> impl Future` → RPITIT & async-fn-in-trait (not dyn-safe) → combinator toolkit |

**Phase 3 — API & error design**

| Concept | File | Rungs |
|---------|------|-------|
| Error handling architecture | [`src/bin/error_arch.rs`](./src/bin/error_arch.rs) | 9 — `Box<dyn Error>` → `thiserror`/`anyhow` → mini-anyhow |
| Custom error types | [`src/bin/custom_errors.rs`](./src/bin/custom_errors.rs) | 9 — Display+Error by hand → source chains → `Report` reporter |
| Newtype & zero-cost wrappers | [`src/bin/newtype.rs`](./src/bin/newtype.rs) | 9 — distinct identity → `repr(transparent)` → phantom-typed `Id<T>` |
| Builder pattern | [`src/bin/builder.rs`](./src/bin/builder.rs) | 8 — consuming/`&mut` builders → typestate builder → `ServerConfig` |
| The typestate pattern | [`src/bin/typestate.rs`](./src/bin/typestate.rs) | 9 — ZST markers → sealed states → TCP-like protocol |
| API evolution & semver | [`src/bin/semver.rs`](./src/bin/semver.rs) | 9 — what breaks → `#[non_exhaustive]` → sealed traits → `ApiChange→Bump` engine |
| Collections deep-dive | [`src/bin/collections.rs`](./src/bin/collections.rs) | 9 — `Entry`/`Borrow` lookup → custom `Hash`/`Eq` → open-addressing `MyHashMap` |
| Strings & text | [`src/bin/strings_text.rs`](./src/bin/strings_text.rs) | 9 — `str`/`String` & UTF-8 invariant → char-boundary slicing → `OsStr`/`Path`/`CStr` → `from_utf8` validation → hand-rolled UTF-8 decoder |

**Phase 4 — Concurrency**

| Concept | File | Rungs |
|---------|------|-------|
| Threads & scoped threads | [`src/bin/threads.rs`](./src/bin/threads.rs) | 9 — `spawn`/`join` → `thread::scope` → `parallel_map` (rayon-lite) |
| `Send` & `Sync` deeply | [`src/bin/send_sync.rs`](./src/bin/send_sync.rs) | 9 — auto-derivation → the four quadrants → hand-rolled `SpinLock` |
| `Mutex` / `RwLock` | [`src/bin/mutex_rwlock.rs`](./src/bin/mutex_rwlock.rs) | 9 — guard RAII → `Arc<Mutex>` counter → `RwLock` readers-xor-writer → poisoning & recovery → non-reentrancy → ABBA deadlock + lock ordering → `Condvar` queue → concurrent `Bank` |

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
