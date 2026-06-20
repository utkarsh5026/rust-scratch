# Rust Mastery Roadmap

A practice-first path from "comfortable with the basics" to **advanced Rust
engineer** — the kind who reads the Rustonomicon for fun, writes sound `unsafe`,
designs libraries other people depend on, and reasons about async/atomics/layout
without hand-waving.

This is **not** a beginner track. It assumes you already know syntax, ownership
basics, `Result`/`Option`, basic traits, and closures. (You've already done the
`Cow` ladder — that's the level we build up from.)

## How to follow this

Every item below is a **practice ladder**, not a reading assignment. For each
concept, start it with the practice skill:

> `practice <concept>`  →  builds `src/bin/<concept>.rs` with 7-9 rungs to mastery

So the ~~90 items here are really **~~700 hands-on problems**. Work a ladder, tick
the box, and add a row to the **Completed concepts** table in `CLAUDE.md`. Read
the linked resource *alongside* the ladder, not before — theory sticks faster
once your hands have hit the problem.

Rule of mastery for every topic: **you can re-implement the core mechanism from
scratch and explain why the compiler enforces what it does.** Each ladder ends in
a build-it capstone for exactly this reason.

Suggested pace: 1-2 ladders per week. Phases 0-8 are the core (~6-9 months serious
part-time); phase 9 is pick-your-specialization; phase 10 is synthesis. Don't rush
the middle (traits, async, unsafe) — that's where intermediate and advanced part ways.

**Legend:** `[ ]` todo · `[x]` done · 🔥 high-leverage (do these even if you skip neighbors) · 🌙 needs nightly

---

## Phase 0 — Tooling, project structure & testing

The unglamorous foundation that separates "writes Rust" from "ships Rust". Skip
nothing here; it pays off in every later phase.

- [ ] **Modules & visibility** — `mod`, `pub(crate)`/`pub(super)`, re-exports, the module tree
- [ ] **Crates & workspaces** — multi-crate workspaces, path/version deps, `[workspace]`
- [ ] 🔥 **Cargo features & `cfg`** — feature flags, `#[cfg(...)]`, conditional compilation, additive features
- [ ] 🔥 **Testing** — unit, integration (`tests/`), doctests, `#[should_panic]`, test organization
- [ ] **Property-based testing** — `proptest`/`quickcheck`, shrinking, invariants over examples
- [ ] **Fuzzing** — `cargo-fuzz`, finding panics/UB with coverage-guided input
- [ ] **Snapshot & golden testing** — `insta`, reviewing diffs
- [ ] **Docs as a deliverable** — `///` doc comments, doc examples that run, `#[doc(hidden)]`, `cargo doc`

*Mastery check:* you can set up a workspace with feature-gated modules and a test
suite that includes a property test and a doctest, and explain why features must be additive.
*Read:* The Cargo Book; "Rust API Guidelines"; *Rust for Rustaceans* ch. on testing & project structure.

---

## Phase 1 — Ownership, conversions & the type system, for real

Stop fighting the borrow checker and start predicting it. Understand *where data
lives*, *who may touch it*, and *how values convert*.

- [x] `**Cow`** — clone-on-write, borrow-vs-own decisions ✅ *(done — `src/bin/cow.rs`)*
- [x] `**Box` & the heap** — ownership of heap data, recursive types, `Box<dyn Trait>` ✅ *(done — `src/bin/box_heap.rs`)*
- [x] 🔥 `**Rc` / `Arc*`* — shared ownership, reference counting, cycles & `Weak` ✅ *(done — `src/bin/rc_arc.rs`)*
- [x] `**RefCell` / `Cell**` — interior mutability and the runtime borrow check
- [x] `**Rc<RefCell<T>>` patterns** — when shared-mutable is right, and its costs ✅ *(done — `src/bin/rc_refcell.rs`)*
- [ ] `**OnceCell` / `LazyLock` / `OnceLock`** — lazy & one-time initialization
- [x] 🔥 **Conversion traits** — `From`/`Into`, `TryFrom`/`TryInto`, `AsRef`/`AsMut` ✅ *(done — `src/bin/conversions.rs`)*
- [x] `**Borrow` / `ToOwned`** — the traits `Cow` and `HashMap` keys are built on (closes the `Cow` loop) ✅ *(done — `src/bin/borrow_toowned.rs`)*
- [x] 🔥 **Lifetimes in depth** — elision rules, `'a: 'b` bounds, lifetimes in structs & impls ✅ *(done — `src/bin/lifetimes_depth.rs`)*
- [x] **HRTB — `for<'a>`** — higher-ranked bounds, why closures over references need them ✅ *(done — `src/bin/hrtb.rs`)*
- [ ] 🔥 **Variance & subtyping** — covariance/contravariance, `&T` vs `&mut T`, `PhantomData`
- [ ] **The never type `!` & DSTs** — diverging functions, `!` coercion, `str`/`[T]`/`dyn` as unsized types
- [x] `**Drop` & ordering** — destructor order, drop flags, `ManuallyDrop`, `mem::forget`/`take`/`replace` ✅ *(done — `src/bin/drop_ordering.rs`)*

*Mastery check:* you can draw the memory layout of an `Rc<RefCell<Vec<T>>>`,
explain every pointer, predict a variance-related compile error, and explain why
`Cow` requires `B: ToOwned`.
*Read:* *Rust for Rustaceans* ch. 1-3; Rustonomicon "Subtyping and Variance" & "Exotically Sized Types".

---

## Phase 2 — Traits & generics like a library author

Design abstractions, not just consume them. The single biggest intermediate→advanced gap.

- [ ] 🔥 **Trait objects & object safety** — `dyn Trait`, vtables, what makes a trait object-safe
- [x] 🔥 **Static vs dynamic dispatch** — monomorphization, code size, when each wins ✅ *(done — `src/bin/dispatch.rs`)*
- [x] **Associated types vs generic params** — `Iterator::Item` style design choices ✅ *(done — `src/bin/assoc_vs_generic.rs`)*
- [x] **Generic bounds & `where` clauses** — multiple bounds, conditional impls, `T: ?Sized` ✅ *(done — `src/bin/generic_bounds.rs`)*
- [x] 🔥 **Blanket impls & coherence** — the orphan rule, why it exists, newtype workarounds ✅ *(done — `src/bin/blanket_coherence.rs`)*
- [ ] **Sealed traits** — restricting who can implement your trait, and why
- [ ] **Marker & auto traits** — `Send`, `Sync`, `Sized`, `Copy`; `?Sized`; negative reasoning
- [ ] **Operator overloading & `Deref`** — `Add`, `Index`, deref coercion, `Deref` abuse
- [ ] 🔥 `**impl Trait` & RPIT** — `impl Trait` in args/returns, `async fn` desugaring
- [ ] **GATs (generic associated types)** — lending iterators, the patterns they unlock
- [ ] **Const generics** — `[T; N]` generic over `N`, type-level numbers
- [ ] 🔥 **Closures & `Fn`/`FnMut`/`FnOnce`** — how closures capture, returning closures, fn pointers
- [ ] 🌙 **Specialization (nightly)** — what it is, why it's hard, how `min_specialization` is used

*Mastery check:* you can design a trait with the right associated-type-vs-generic
split, explain object safety from the vtable up, and write a sealed trait.
*Read:* *Rust for Rustaceans* ch. 2-3; "Sizedness in Rust"; the GATs stabilization blog post.

---

## Phase 3 — API & error design

Write code other people can use without reading the source.

- [x] 🔥 **Error handling architecture** — `thiserror` (libs) vs `anyhow` (apps), `?`, conversion
- [x] **Custom error types** — `std::error::Error`, source chains, backtraces, `Box<dyn Error>`
- [x] 🔥 **The typestate pattern** — encode state machines in types; invalid states = compile errors ✅ *(done — `src/bin/typestate.rs`)*
- [x] **Builder pattern** — ergonomic construction, `#[must_use]`, consuming vs mutating builders
- [x] **Newtype & zero-cost wrappers** — type safety with no runtime cost; the `Deref` tradeoff ✅ *(done — `src/bin/newtype.rs`)*
- [ ] 🔥 **API evolution & semver** — what's breaking, sealed traits, `#[non_exhaustive]`, future-proofing
- [ ] 🔥 **Iterators end-to-end** — implementing `Iterator`, adapters, `IntoIterator`, laziness, `collect` magic
- [ ] **Strings & text** — `str`/`String`/`OsStr`/`CStr`/`Path`, UTF-8 invariants, when each appears
- [ ] **Collections deep-dive** — `HashMap`/`BTreeMap`/`VecDeque`/`HashSet`, hashing, `Entry` API, choosing one

*Mastery check:* you can build a typestate API where misuse won't compile, and
classify a list of changes to a public API as breaking or not.
*Read:* "Rust API Guidelines"; *Rust for Rustaceans* ch. 4 (Error Handling) & API design; std collections docs.

---

## Phase 4 — Concurrency

Fearless concurrency, *understood* — not `Arc<Mutex<T>>` cargo-culting.

- [ ] **Threads & scoped threads** — `std::thread`, `thread::scope`, `JoinHandle`
- [ ] 🔥 `**Send` & `Sync` deeply** — what they really guarantee, why `Rc` is `!Send`
- [ ] `**Mutex` / `RwLock`** — poisoning, lock guards, deadlock avoidance, lock ordering
- [ ] **Channels** — `mpsc`, bounded vs unbounded, `crossbeam` channels, backpressure
- [ ] 🔥 **Atomics & memory ordering** — `Relaxed`/`Acquire`/`Release`/`SeqCst`, happens-before
- [ ] 🔥 **Lock-free basics** — a `SpinLock`, atomic counters/flags, the ABA problem
- [ ] **Data parallelism with `rayon`** — `par_iter`, work-stealing, when parallelism actually helps
- [ ] **Architecture: shared state vs message passing** — choosing, and combining, the two models

*Mastery check:* you can implement a correct `SpinLock` with the right orderings
and explain why `Relaxed` would be a bug there.
*Read:* **"Rust Atomics and Locks" by Mara Bos** (free online — *the* book for this phase).

---

## Phase 5 — Async internals

Understand async from the `Future` trait up, not just `.await` syntax.

- [ ] 🔥 `**Future` trait & `poll`** — what `.await` desugars to, the generated state machine
- [ ] 🔥 `**Pin` & `Unpin**` — self-referential futures, why `Pin` exists, `Box::pin`
- [ ] **Writing a future by hand** — implement `Future`, wakers, `Context`
- [ ] 🔥 **Building a tiny executor** — poll loop, task queue, waking — runtimes demystified
- [ ] `**pin-project`** — safe field projection through `Pin` without unsafe
- [ ] `**tokio` essentials** — tasks, `spawn`, `select!`, `join!`, `spawn_blocking`
- [ ] **Cancellation & structured concurrency** — drop = cancel, `CancellationToken`, `JoinSet`
- [ ] **Async traits & lifetimes** — `async fn` in traits, `async-trait`, the lifetime gotchas
- [ ] **Streams** — `Stream` trait, async iteration, `StreamExt`, backpressure
- [ ] **Shared async state** — `tokio::sync`; never hold a std lock across `.await`
- [ ] **The `tower` service model** — `Service` trait, layers/middleware, how servers compose

*Mastery check:* you can write a toy executor that drives a hand-written future to
completion, and explain exactly when `Pin` is load-bearing.
*Read:* "Asynchronous Programming in Rust"; withoutboats on Pin & async; Jon Gjengset's futures video.

---

## Phase 6 — Unsafe & the machine

Write `unsafe` that is actually *sound*. The gateway to truly advanced Rust.

- [ ] 🔥 **Raw pointers** — `*const`/`*mut`, `&` vs raw, pointer arithmetic, provenance
- [ ] **The 5 unsafe superpowers** — deref raw ptr, call unsafe fn, `static mut`, unions, impl unsafe trait
- [ ] 🔥 **Type layout** — `repr(C)`, `repr(transparent)`, alignment, padding, `size_of`/`align_of`
- [ ] `**MaybeUninit`** — uninitialized memory done right
- [ ] 🔥 **Aliasing & Stacked Borrows** — the rules `&mut` must obey, what Miri checks, what UB *is*
- [ ] 🔥 **Implementing a `Vec`** — the Rustonomicon capstone: grow, push, pop, `Drop`, from scratch
- [ ] **Sound unsafe abstractions** — upholding invariants, `// SAFETY:` discipline
- [ ] **Custom allocators** — `GlobalAlloc`, `#[global_allocator]`, arena/bump allocators
- [ ] **FFI** — calling C & being called from C, `extern "C"`, opaque types, callbacks, `bindgen`
- [ ] `**PhantomData` & drop check** — telling the compiler about ownership held via raw pointers

*Mastery check:* your hand-rolled `Vec` passes `cargo miri` with no UB, and you can
state the safety invariant your abstraction upholds.
*Read:* **The Rustonomicon** (esp. "Implementing Vec"); run everything under `cargo +nightly miri`.

---

## Phase 7 — Performance & low-level craft

Make it fast, and *know* it's fast.

- [ ] 🔥 **Benchmarking** — `criterion`, measurement traps, `black_box`
- [ ] 🔥 **Profiling** — `perf`/`samply`/flamegraphs, finding the real hot path
- [ ] **Allocation awareness** — stack vs heap, `SmallVec`, arenas, `Box::leak`
- [ ] **Cache & data layout** — SoA vs AoS, false sharing, padding for performance
- [ ] **Zero-cost abstractions, verified** — read the asm (`cargo-asm`), confirm iterators → loops
- [ ] **Compile-time computation** — `const fn`, const eval, const generics in anger
- [ ] **Build tuning** — release profiles, LTO, `codegen-units`, `target-cpu`, PGO
- [ ] **SIMD** — `std::simd` / `wide`, autovectorization, when to hand-vectorize

*Mastery check:* you take a naive routine, profile it, and produce a measured
speedup you can explain at the assembly/cache level.
*Read:* "The Rust Performance Book"; Algorithmica's "Algorithms for Modern Hardware".

---

## Phase 8 — Metaprogramming

Generate code. Where libraries get their ergonomics.

- [ ] 🔥 **Declarative macros** — `macro_rules!`, fragment specifiers, repetition, hygiene
- [ ] 🔥 **Procedural macros — derive** — `syn`/`quote`, parse a struct, generate an impl
- [ ] **Attribute & function-like proc macros** — custom attributes, mini-DSLs
- [ ] **Macro debugging & hygiene pitfalls** — `cargo expand`, span/hygiene gotchas
- [ ] **Build scripts & codegen** — `build.rs`, generating code at build time

*Mastery check:* you've written a working `#[derive(Builder)]` proc macro with
`syn`/`quote` and debugged it with `cargo expand`.
*Read:* "The Little Book of Rust Macros"; *Rust for Rustaceans* macros chapter; the `syn`/`quote` docs.

---

## Phase 9 — Specialization tracks (pick what you need)

You don't need all of these — pick the domain(s) you actually work in. Each is a
mini-roadmap of its own.

**Systems / embedded**

- [ ] `**no_std`** — no allocator, `core`/`alloc`, panic handlers
- [ ] **Embedded basics** — `embedded-hal`, memory-mapped IO, interrupts (concepts)

**Web / network services**

- [ ] **HTTP servers** — `axum`/`actix`, routing, extractors, state
- [ ] **Database & async IO** — `sqlx`, connection pools, transactions
- [ ] **Serialization at scale** — `serde` advanced, zero-copy, custom (de)serializers

**WASM**

- [ ] **Rust → WASM** — `wasm-bindgen`, the JS boundary, `wasm-pack`

**CLI / tooling**

- [ ] **Production CLIs** — `clap`, config, structured logging with `tracing`

---

## Phase 10 — Capstones (synthesis)

Stop drilling, start building. These force multiple phases together — make a
sub-folder or a fresh crate (they don't fit the single-file ladder format).

- [ ] **A `no_std` library** — no allocator, embedded-flavored constraints (Phase 6 + 9)
- [ ] **A small async runtime** — executor + reactor + timer (Phase 5 + 6)
- [ ] **A lock-free data structure** — Treiber stack or SPSC queue (Phase 4 + 6)
- [ ] **A parser/interpreter** — for a small language; lifetimes, traits, errors at scale
- [ ] **A library with a proc-macro** — publish-quality API, derive macro, docs & tests (Phase 2 + 3 + 8)
- [ ] **Contribute to a real crate** — read tokio/serde/ripgrep, fix a real issue

---

## Core reference shelf

Keep these open as you go (most are free online):


| Resource                                                             | Best for                                           |
| -------------------------------------------------------------------- | -------------------------------------------------- |
| **The Rustonomicon**                                                 | unsafe, variance, layout, implementing `Vec`       |
| **Rust Atomics and Locks** (Mara Bos)                                | Phase 4 — concurrency & atomics                    |
| **Rust for Rustaceans** (Jon Gjengset)                               | the whole advanced arc; traits, API, macros        |
| **Jon Gjengset's "Crust of Rust" videos**                            | live deep-dives: lifetimes, atomics, Pin, dispatch |
| **The Async Book** + **withoutboats' blog**                          | Phase 5 — futures, Pin, async design               |
| **The Rust Performance Book**                                        | Phase 7                                            |
| **The Little Book of Rust Macros**                                   | Phase 8                                            |
| **The Cargo Book** + **Rust API Guidelines**                         | Phase 0 & 3                                        |
| `**cargo miri`, `clippy`, `cargo-asm`, `cargo-expand`, `criterion`** | the tools you verify mastery with                  |


---

## Sources

Roadmap synthesized from: [roadmap.sh/rust](https://roadmap.sh/rust),
[Rustify — Learn Rust in 2026](https://rustify.rs/articles/learn-rust-in-2025),
[Rust for Rustaceans (No Starch)](https://nostarch.com/rust-rustaceans),
[The Rustonomicon](https://doc.rust-lang.org/nomicon/),
[microsoft/RustTraining](https://github.com/microsoft/RustTraining),
and the structure of *Rust Atomics and Locks* by Mara Bos.