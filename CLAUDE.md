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
| Lifetimes in depth | `src/bin/lifetimes_depth.rs` | `cargo run --bin lifetimes_depth` | 9 — `longest` annotation → 3 elision rules → lifetimes in structs → `impl<'a>` & the &self elision gotcha → dangling/owned-vs-borrowed return → `'a: 'b` outlives bounds (variance seed) → lifetimes + generics + `'static` (`&'static` vs `T: 'static`) → borrowing `Iterator` (Item lifetime) → hand-rolled `StrSplit` (two lifetimes, zero-copy) |
| `Borrow` / `ToOwned` | `src/bin/borrow_toowned.rs` | `cargo run --bin borrow_toowned` | 9 — `ToOwned` as generalized Clone → `Borrow` views → `HashMap::get(&str)` & `K: Borrow<Q>` → `T::Owned` assoc type → Borrow-vs-AsRef contract (`CiString`) → needless-alloc footgun → Into-in/Borrow-out API split → closing the `Cow` loop → hand-rolled `MyBorrow`+`MyToOwned`+`MyCow` |
| `Drop` & ordering | `src/bin/drop_ordering.rs` | `cargo run --bin drop_ordering` | 9 — Drop at scope end → locals LIFO vs struct-fields declaration order → `mem::drop` early & `E0040` → drop flags (conditional move, no double-drop) → `mem::forget`/`replace`/`take` → RAII scope guard w/ `.cancel()` → `ManuallyDrop` custom field order (Miri-clean) → rollback-on-drop `Transaction` (commit disarms; panic still rolls back) |
| `Rc<RefCell<T>>` patterns | `src/bin/rc_refcell.rs` | `cargo run --bin rc_refcell` | 10 — shared-cell aha → owner structs share one cell → counts/ptr_eq/&Rc-vs-clone → double `borrow_mut` panic → borrow-across-call reentrancy → strong cycle leak (Drop never runs) → `Weak` fix + parent-pointer tree → observer/subject fan-out → hand-rolled doubly-linked list (`Rc` next / `Weak` prev) → iterative `Drop` (no stack overflow on long chains) |
| HRTB — `for<'a>` | `src/bin/hrtb.rs` | `cargo run --bin hrtb` | 9 — implicit `for<'a>` in `Fn(&T)` → spell it explicit → caller-picks vs callee-picks (return a borrow of a local) → HRTB on your own `Slicer<'a>` trait → "impl of Fn not general enough" (let-bound closure gets one lifetime; fn ptr/item are `for<'a>`) → named-lifetime trap (one `<'a>` fixed by caller) → `DecodeOwned: for<'de> Decode<'de>` (serde pattern; borrowers excluded) → `Box<dyn for<'a> Fn>` keeps trait object lifetime-free → parser-combinator capstone (`tag`/`number`/`map`/`then`) |
| Associated types vs generic params | `src/bin/assoc_vs_generic.rs` | `cargo run --bin assoc_vs_generic` | 9 — same trait `type Item` vs `<T>` → one-impl-per-type vs many (E0119) → `Iterator<Item=u64>` equality bound + `I::Item` projection → hand-rolled `Iterator` for `Countdown` → generic `.into()` ambiguity (E0283) vs determined assoc output → `dyn Iterator<Item=..>` must pin assoc (E0191) → `Add<Rhs=Self>{type Output}` uses both → design a `Graph` trait's split → capstone: `MyIterator` + generic `Map` adapter threading the associated `Item` |
| Blanket impls & coherence | `src/bin/blanket_coherence.rs` | `cargo run --bin blanket_coherence` | 9 — unconditional `impl<T> Trait for T` → conditional `impl<T: Display>` → reconstruct `From`→`Into` blanket (`.into()` needs target type) → extension trait over `Iterator` (itertools pattern) → orphan rule E0117 (≥1 of {trait,type} local) → overlap E0119 (blanket vs concrete, no stable specialization) → uncovered-param E0210 (local-as-Self order; `Wrapper<T>` covers `T`) → newtype workaround + `Deref` ergonomics → capstone: sealed extension trait (private `Sealed` blanket gates `StatsExt`) |
| Static vs dynamic dispatch | `src/bin/dispatch.rs` | `cargo run --bin dispatch` | 9 — `<T>` vs `&dyn` (monomorphization vs vtable) → `impl Trait` arg position (generic sugar) vs return position (one hidden type) → `type_name::<T>()` proves a copy stamped per type → return-branch needs `Box<dyn>` (`impl Trait` can't) → `Vec<T>` can't mix but `Vec<Box<dyn>>` can → `-> Self` ok under generics, forbidden behind `dyn` (object safety) → closures: generic `F: Fn` vs `Vec<Box<dyn Fn>>` registry → enum dispatch (closed set, inline, no vtable, no alloc) → capstone: same pipeline static/dynamic/enum, three strategies one result |
| Error handling architecture | `src/bin/error_arch.rs` | `cargo run --bin error_arch` | 9 — `?`+`Box<dyn Error>` quick app error → hand-rolled enum (Display+Error+From+source) → `thiserror` derive (`#[error]`/`#[from]`) → `anyhow` context/`bail!`/`anyhow!` → source-chain walk + `downcast_ref` → E0277 `?`-won't-convert + `String`-error anti-pattern → thiserror-lib/anyhow-app boundary (typed error survives under context) → classification `is_retryable`+`#[non_exhaustive]` driving a generic retry loop → capstone: mini-anyhow (blanket `From`, `WrapErr::context`, `ContextError` source chain) |


## Project layout

```
rust-scratch/
├── Cargo.toml
├── CLAUDE.md
├── ROADMAP.md                  # the 9-phase mastery curriculum (what to practice next)
├── .claude/
│   ├── skills/rust-practice/   # the practice-ladder coach (the heart of this project)
│   └── commands/               # /experiment, /run, /explain, /list, /document
├── docs/                       # mdBook knowledge base -> GitHub Pages
│   ├── book.toml
│   └── src/
│       ├── SUMMARY.md          # nav (a page MUST be listed here to be built)
│       ├── intro.md
│       ├── concepts/<concept>.md   # one distilled note per finished ladder
│       └── meta/{adding-a-note,template}.md
└── src/
    ├── main.rs                 # scratch pad for quick throwaway code (cargo run)
    └── bin/                    # one file per concept ladder
```

## Knowledge base (mdBook -> GitHub Pages)

Finished ladders get a **distilled notes page** in `docs/src/concepts/`, published
as a searchable site at `https://utkarsh5026.github.io/rust-scratch/` via the
`Deploy docs` GitHub Action (`.github/workflows/docs.yml`). Local preview:
`mdbook serve docs --open`. See `docs/src/meta/adding-a-note.md` for the workflow
and `docs/src/meta/template.md` for the page format.

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
- When a concept ladder is finished: (1) add a row to the **Completed concepts**
  table here, and (2) write its distilled notes page in `docs/src/concepts/`,
  register it in `docs/src/SUMMARY.md`, and tick its row in `docs/src/intro.md`
  (use `docs/src/meta/template.md`). The site auto-deploys on push to `master`.
- For pure deep teaching (not practice), defer to the `/rustacean` skill.
