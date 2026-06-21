# API evolution & semver

> Ladder: [`src/bin/semver.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/semver.rs) ·
> Run: `cargo run --bin semver` · Phase 3 · 9 rungs

## TL;DR

Semantic versioning is a promise: within a major version, an upgrade never breaks a
downstream build. A change is **breaking** if *any* valid downstream crate could stop
compiling, stop linking, or change behavior after a `cargo update`. Rust's
exhaustiveness checking, type inference, and **auto traits** make that set of breaking
changes much larger than "I deleted a function" — adding a public field, adding an enum
variant, or even swapping a *private* field's type can all break the world.

The defensive toolkit, all proven in this ladder:

- **Private fields + constructors** — kill the struct literal, so fields can grow freely.
- **`#[non_exhaustive]`** — keep public fields readable while forbidding literals and exhaustive matches downstream.
- **Default trait methods** and **sealed traits** — evolve a trait without breaking implementors (or forbid foreign implementors entirely).
- **Compile-time auto-trait guards** (`const _` + `assert_send`) — catch a silently-dropped `Send`/`Sync` in CI instead of in a user's bug report.
- **Minimal generic bounds** — every bound is a wall some future caller hits.

## Why this exists (from first principles)

A version number is a compatibility contract with people you'll never meet. `MAJOR.MINOR.PATCH`:

| Bump  | Meaning                                  | Promise to downstream            |
|-------|------------------------------------------|----------------------------------|
| PATCH | backwards-compatible bug fix             | recompiles, behaves the same     |
| MINOR | backwards-compatible **addition**        | recompiles, new API available    |
| MAJOR | **breaking** change                      | may fail to compile/link         |

The hard part isn't the numbering — it's knowing which bucket a change falls into. In a
dynamically-typed language "breaking" mostly means "removed something." In Rust the
compiler lets downstream code lean on your types in ways you never anticipated:

- it can **construct** your struct with a literal `Foo { a, b }`, which silently requires *every* field;
- it can **exhaustively match** your enum with no `_` arm, which silently requires *every* variant;
- it can **send** your type across threads, which silently requires every field be `Send`.

Each of those "silently requires" is a constraint you didn't write down but are now on
the hook for. SemVer in Rust is the discipline of *seeing those implicit constraints*
and deciding, per change, whether you just violated one.

The authoritative rules live in the [Cargo SemVer reference](https://doc.rust-lang.org/cargo/reference/semver.html);
this ladder makes you *feel* each one with the compiler rather than memorize a table.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|-----------|
| 1 | Foundations | `required_bump` | The canonical, unambiguous cases: bugfix→patch, add fn→minor, remove/rename→major |
| 2 | Foundations | struct-literal landmine | An all-`pub` struct can't grow a field; private fields + constructor defuse it |
| 3 | Mechanics | `#[non_exhaustive]` structs | Keep fields readable, forbid downstream literals, add fields later |
| 4 | Mechanics | enum variants | Adding a variant breaks exhaustive `match`; `#[non_exhaustive]` forces a `_` arm |
| 5 | Footgun | trait evolution | Required method = break (E0046); defaulted method / non-blanket impl = safe |
| 6 | Footgun | sealed traits | A private `Sealed` supertrait means no foreign impls — so the trait is free to evolve |
| 7 | Real-world | auto-trait leakage | A private field type change can silently drop `Send`/`Sync`; guard it at compile time |
| 8 | Real-world | generic bounds | Loosen = safe, tighten = break; new type params need defaults |
| 9 | Capstone | `classify` engine + future-proofed `mod lib` | One classifier for all rules; one library whose v1.1 is a clean minor |

## The ideas, built up

### 1. The baseline rules

Start with the cases nobody argues about. The ladder encodes them as a match:

```rust
fn required_bump(change: &Change) -> Bump {
    match change {
        Change::BugFixInternal          => Bump::Patch, // no public surface moved
        Change::PerfImprovement         => Bump::Patch, // same signature, same result
        Change::AddPublicFunction       => Bump::Minor, // pure addition
        Change::DeprecatePublicFunction => Bump::Minor, // #[deprecated] still compiles
        Change::RemovePublicFunction    => Bump::Major, // downstream calls vanish
        Change::RenamePublicFunction    => Bump::Major, // = remove + add
    }
}
```

Two things worth internalizing here. **Deprecation is minor**, not breaking: `#[deprecated]`
emits a warning, and warnings don't fail a build. **A rename is a remove plus an add** — the
"add" half is harmless, but the "remove" half is what bumps it to major. Renames are the
classic accidental break.

### 2. The struct-literal landmine

Here's the first change that *looks* harmless and isn't. Ship this:

```rust
pub struct RgbColor { pub r: u8, pub g: u8, pub b: u8 }
```

Downstream is now free to write a struct literal and an exhaustive destructure:

```rust
let c = RgbColor { r: 255, g: 0, b: 0 };
let RgbColor { r, g, b } = c;
```

Add `pub a: u8` in v1.1 and **both** of those break:

- the literal fails with **E0063** (missing field `a`);
- the destructure fails with **E0027** (pattern does not mention field `a`).

A one-field "feature" just forced a major bump. The root cause: a struct literal
implicitly requires *all* fields, and you can't add a field without invalidating every
existing literal.

The fix is to remove the capability that creates the obligation — make the fields private
and hand out a constructor and accessors:

```rust
pub struct RgbColor { r: u8, g: u8, b: u8 } // private

impl RgbColor {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self { Self { r, g, b, a: 255 } }
    pub fn channels(&self) -> (u8, u8, u8) { (self.r, self.g, self.b) }
}
```

With no public fields, downstream *cannot* write a literal or an exhaustive destructure, so
adding `a` later changes nothing for them. (In the ladder, the proof is literal: the file
adds the `a` field and `check_2` keeps compiling untouched.)

### 3. `#[non_exhaustive]` — readable fields without the obligation

Private fields cost ergonomics: every read becomes a getter call. `#[non_exhaustive]` is
the middle path. It lets downstream **read** public fields directly but forbids the two
fragile operations:

```rust
#[non_exhaustive]
pub struct ClientConfig {
    pub timeout_ms: u32,
    pub retries: u8,
}

impl ClientConfig {
    pub fn new() -> Self { ClientConfig { timeout_ms: 30_000, retries: 3 } }
    pub fn with_retries(mut self, retries: u8) -> Self { self.retries = retries; self }
}
```

From another crate:

```rust
let cfg = ClientConfig::new().with_retries(5); // OK: constructor is the only door
let t = cfg.timeout_ms;                         // OK: reading a pub field is fine
let bad = ClientConfig { timeout_ms: 1, retries: 1 }; // ERROR: literal forbidden
```

> **Subtlety that the single-file ladder can't show you directly:** `#[non_exhaustive]`
> only restricts *foreign* crates. Inside the defining crate the attribute is inert — you
> can still literal-construct and exhaustively match. That's why `ClientConfig::new` can
> use a struct literal: it lives in the same crate. The restriction (and the safety) is
> purely a cross-crate property.

### 4. Enum variants, and the one place the compiler *does* show you the pain

Adding a variant to a plain public enum breaks every downstream exhaustive `match` with
**E0004** (non-exhaustive patterns). So "add a variant" defaults to **major**. Mark the
enum `#[non_exhaustive]` from day one and downstream is *forced* to include a `_` arm — so
later variants are only **minor**.

The neat trick in this rung: you can feel the real cross-crate error inside a single bin,
because **`std::io::ErrorKind` is itself `#[non_exhaustive]`**, and `std` is a foreign
crate to you. Write a match over it without a `_` and the compiler rejects it:

```rust
fn describe_io_error(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::NotFound        => "missing",
        ErrorKind::PermissionDenied => "denied",
        _                          => "other", // mandatory — that IS the point
    }
}
```

Drop the `_` arm and you get the exact experience your downstream has when you add a
variant. The rule, as code:

```rust
fn add_variant_bump(enum_was_non_exhaustive: bool) -> Bump {
    if enum_was_non_exhaustive { Bump::Minor } else { Bump::Major }
}
```

### 5. Trait evolution: the default body is everything

A public trait is a contract with everyone who `impl`s it. Two opposite moves, opposite costs:

```rust
pub trait Plugin {
    fn name(&self) -> &str;
    // Adding `fn version(&self) -> u32;`  (no body)  -> BREAKING: every downstream
    //   `impl Plugin for X` fails with E0046, "not all trait items implemented".
    // Adding `fn version(&self) -> u32 { 1 }` (default) -> MINOR: existing impls
    //   inherit the body and never knew it appeared.
    fn version(&self) -> u32 { 1 }
}
```

That single `{ 1 }` is the entire difference between a quiet minor release and breaking
every implementor in the ecosystem. The impl-side rules round it out:

| Change | Bump | Why |
|--------|------|-----|
| Add required method (no default) | Major | every foreign impl is now incomplete (E0046) |
| Add defaulted method | Minor | impls inherit the default |
| Add non-blanket `impl Trait for Concrete` | Minor | can perturb inference, but treated as minor |
| Add blanket `impl<T> Trait for T` | Major | can collide with downstream impls (coherence, E0119) |

### 6. Sealed traits: make "add a method" a minor change

Rung 5 was grim: adding a required method breaks every implementor. But what if *no
foreign implementor can exist*? Then there's nothing to break, and you can add methods,
change defaults, even add supertraits — all as minor changes. That's a **sealed trait**.

The pattern is a marker trait in a **private** module, required as a supertrait:

```rust
mod sealed {
    pub trait Sealed {}        // module is private to the crate
}

pub trait Format: sealed::Sealed { // public, but gated by a private bound
    fn extension(&self) -> &str;
}

pub struct Json;
impl sealed::Sealed for Json {}    // only this crate can write this line
impl Format for Json { fn extension(&self) -> &str { "json" } }
```

A foreign crate that tries `impl Format for TheirType` gets **E0277**: `TheirType: Sealed`
is not satisfied — and they can't fix it, because they can't reach into your private
`sealed` module. The visibility dance is the crux: `Sealed` must be `pub` (so it can appear
in the public `Format` bound) yet live in a *private* `mod sealed` (so it's unnameable
outside the crate). This is how `serde`, `bytes`, and several std traits stay evolvable.

### 7. Auto-trait leakage: the break with no signature change

The sneakiest one. `Send` and `Sync` are **auto traits**: the compiler derives them
*structurally* from a type's fields. So a type's thread-safety is a function of its private
internals — and changing a private field can flip it without the public signature moving
one character.

```rust
pub struct Job { data: Vec<u8> }       // Vec<u8> is Send + Sync  -> Job is too
// later, "just an internal refactor":
pub struct Job { data: std::rc::Rc<u8> } // Rc is !Send + !Sync   -> Job is now neither
```

Every downstream `thread::spawn(move || ... job ...)` now fails with **E0277** ("`Rc<u8>`
cannot be sent between threads safely"). You shipped it as a patch; it was a major break.
(The same hazard hides behind `-> impl Trait` returns: the opaque type leaks the auto
traits of whatever you built it from.)

The professional defense is a **compile-time regression guard** — the trick the
`static_assertions` crate automates:

```rust
fn assert_send<T: Send>() {} // the bound IS the test
fn assert_sync<T: Sync>() {}

const _: () = {
    let _guard: fn() = || {
        assert_send::<Job>();
        assert_sync::<Job>();
    };
};
```

If `Job` ever loses `Send` or `Sync`, this block fails to compile — the leak becomes a
build error in *your* CI instead of a bug report from a user. `const _` runs the
type-check at compile time with zero runtime cost.

### 8. Generic bounds: the loosen/tighten asymmetry

Bounds have a direction:

- **Loosening** a bound (removing a requirement) is **non-breaking**. Anyone who satisfied
  the stricter bound still satisfies the looser one — you only let *more* callers in.
- **Tightening** a bound (adding a requirement) is **breaking**. Callers whose type doesn't
  satisfy the new requirement are locked out with E0277.

The practical rule that falls out: **ask for the minimum your body actually needs.** Every
extra bound is a wall some future caller will hit. The ladder demonstrates with a type that
is `Debug` but not `Clone`:

```rust
#[derive(Debug)] struct NoClone;

fn process<T: std::fmt::Debug>(items: &[T]) -> usize { // minimal bound
    let _ = items.iter().map(|x| format!("{x:?}")).collect::<Vec<_>>();
    items.len()
}

process(&[NoClone, NoClone]); // OK
```

Add `+ Clone` to the bound and that call dies — an over-tight bound is the break. For new
type parameters: adding one *without* a default changes arity and breaks turbofish/some
calls (major); adding one *with* a default keeps existing uses working (minor).

## Footguns

| Trap | What bites | Fix |
|------|------------|-----|
| All-`pub` struct | Adding a field breaks every literal (E0063) and exhaustive destructure (E0027) | Private fields + constructor, or `#[non_exhaustive]` |
| Plain public enum | Adding a variant breaks exhaustive `match` (E0004) | `#[non_exhaustive]` from day one |
| Required trait method | Adding one breaks every implementor (E0046) | Give it a default body, or seal the trait |
| Blanket impl | Adding `impl<T> Trait for T` collides with downstream impls (E0119) | Treat as major; prefer non-blanket impls |
| Auto-trait leakage | A private field type change silently drops `Send`/`Sync` (E0277 downstream) | `const _` + `assert_send`/`assert_sync` guard |
| Over-tight bound | A needless `+ Clone` locks out valid callers | Bound only what the body uses |
| In-crate blind spot | `#[non_exhaustive]` and sealing don't restrict the *defining* crate, so local tests won't reveal the protection | Reason about it cross-crate; test against a real downstream crate if it matters |

## Real-world patterns

- **`#[non_exhaustive]` everywhere in std and the ecosystem.** `std::io::ErrorKind`,
  most error enums in `thiserror`-based libraries, and config structs use it so they can
  grow without a major bump.
- **Sealed traits** in `serde` (`serde::de`/`ser` internals), `bytes::Buf`/`BufMut`, and
  `nom` — the public trait is callable but not implementable, so the maintainers can add
  methods freely.
- **`static_assertions::assert_impl_all!(Job: Send, Sync)`** is the packaged version of the
  rung-7 `const _` guard; `tokio` and `bytes` ship these to lock auto traits in place.
- **`cargo semver-checks`** automates much of `classify` — it diffs your public API against
  the last published version and tells you the required bump. Knowing the rules by hand is
  how you read its output.

## Capstone insight

Two halves, and the point is how they fit.

**Part A — the brain.** `classify(&ApiChange) -> Bump` collapses all eight rungs into one
match with guards on boolean fields. The *shape* of the data model is the lesson: the
breaking-ness of a change is rarely about the change alone — it's about a condition.
"Add a struct field" isn't major or minor; it's major *unless* the struct was already
sealed from literals. "Change internals" is a patch *unless* it drops an auto trait.

```rust
ApiChange::AddStructField { sealed_from_literals: true  } => Bump::Minor,
ApiChange::AddStructField { sealed_from_literals: false } => Bump::Major,
ApiChange::ChangeInternals { keeps_auto_traits: true  }   => Bump::Patch,
ApiChange::ChangeInternals { keeps_auto_traits: false }   => Bump::Major,
```

**Part B — the hands.** A `mod lib` engineered so its v1.1 is a clean minor, by *combining*
the techniques: a `#[non_exhaustive]` `Settings` built through `Settings::new()`, and a
**sealed** `Codec` trait. The downstream consumer is the proof:

```rust
fn use_library() -> String {
    let settings = lib::Settings::new();
    let codec = lib::Gzip;
    format!("{} @ level {}", codec.name(), settings.level)
}
```

The "aha": because `Settings` is non-exhaustive (constructed via `new`) and `Codec` is
sealed, a v1.1 that **adds a field to `Settings`** and **adds a defaulted method to
`Codec`** requires *zero* changes to `use_library`. Future-proofing isn't one trick — it's
choosing, up front, the construction and extension points that keep your future options
open. You decide where downstream is allowed to couple to you, and you make everywhere else
unreachable.

## Explain it back

- Why is adding a `pub` field to an existing public struct a *major* change, and what two
  distinct downstream operations does it break?
- What exactly does `#[non_exhaustive]` forbid downstream, and why does it have no effect
  inside the defining crate?
- You add a method to a public trait. When is that a minor change and when is it major?
- How can a sealed trait let you add a *required* method as a minor release?
- Your "internal refactor" swaps a `Vec` field for an `Rc`. The signature is identical. How
  can this break a downstream `cargo update`, and how would you have caught it in CI?
- Loosening vs tightening a generic bound — which direction is safe, and why?

## See also

- [The typestate pattern](typestate.md) — same `Sealed`-supertrait trick, used to gate state transitions.
- [Blanket impls & coherence](blanket-coherence.md) — why a blanket impl is a major change (E0119) and the sealed-extension-trait pattern.
- [`Send` & `Sync` deeply](send-sync.md) — the auto-trait mechanics behind rung 7's leak.
- [Newtype & zero-cost wrappers](newtype.md) — private-field smart constructors (parse-don't-validate) as an API-stability tool.
- [Generic bounds & where clauses](generic-bounds.md) — the bound mechanics rung 8 builds on.
