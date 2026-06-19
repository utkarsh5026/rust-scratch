# Blanket impls & coherence

> Ladder: [`src/bin/blanket_coherence.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/blanket_coherence.rs) ·
> Run: `cargo run --bin blanket_coherence` · Phase 2 · 9 rungs

## TL;DR

An `impl` block is a **fact** you assert to the compiler: "this trait is implemented for this type."
**Coherence** is the rule that there is *exactly one* such fact for any given (trait, type) pair —
never zero-ambiguity, never two conflicting answers. A **blanket impl** (`impl<T> Trait for T`) asserts
a fact about *infinitely many* types in one block. The **orphan rule** is the guardrail that stops two
different crates from each asserting conflicting facts about types neither of them owns.

Every error in this topic — E0117, E0119, E0210 — is coherence defending that "exactly one" invariant
from a different angle.

## Why this exists (from first principles)

Method resolution has to be **deterministic and global**. When you write `x.into()`, the compiler must
find *the* impl — not "an" impl, and definitely not two. Now imagine impls were unrestricted:

- Crate A does `impl Display for Vec<i32>` to print `[1, 2, 3]`.
- Crate B does `impl Display for Vec<i32>` to print `1 2 3`.
- Your program depends on both. `vec![1,2,3].to_string()` now has **two** answers.

There is no sound way to pick. Worse, adding a dependency could silently change which impl wins, breaking
code far away. Rust forbids the situation from ever being *written*, rather than trying to resolve it
after the fact. That ban is **coherence**, and its crate-boundary half is the **orphan rule**:

> To `impl SomeTrait for SomeType`, at least one of `{the trait, the type}` must be **local** to your crate.

If both are foreign, you can't write the impl — which means no two crates can both reach in and define
conflicting impls for types they don't own. The guarantee buys you: *any (trait, type) pair resolves to
the same impl no matter what crates are linked.*

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|---|---|---|
| 1 | foundations | `impl<T> Named for T` | One unconditional blanket impl gives *every* type a method. |
| 2 | foundations | `impl<T: Display> Loud for T` | A bound narrows the blanket to a *subset* of types. |
| 3 | mechanics | `From` → `Into` | Reconstruct std's blanket: implement `MyFrom`, get `.my_into()` free. |
| 4 | mechanics | extension trait | `impl<I: Iterator> IterExt for I` — the itertools pattern. |
| 5 | footgun | orphan rule (E0117) | Foreign trait + foreign type is rejected; local on either side is legal. |
| 6 | footgun | overlap (E0119) | A blanket and a concrete impl that both match one type collide. |
| 7 | footgun | uncovered param (E0210) | A bare `T` in `Self` position before a local type is illegal. |
| 8 | real-world | newtype workaround | Wrap the foreign type locally, then impl the foreign trait; `Deref` for ergonomics. |
| 9 | capstone | sealed extension trait | A private `Sealed` blanket gates a public trait nobody downstream can implement. |

## The ideas, built up

### 1. A blanket impl is one fact about infinitely many types

```rust
trait Named {
    fn type_label(&self) -> &'static str;
}

impl<T> Named for T {
    fn type_label(&self) -> &'static str {
        "a value"
    }
}
```

After that single block, `42i32`, `String::from("hi")`, and your own `Widget` *all* have
`.type_label()`. You never wrote a per-type impl. The generic `T` ranges over every type that exists,
so the impl is a universally-quantified statement: "for all `T`, `T: Named`."

This is also the first hint at why the orphan rule must exist. If a *downstream* crate also wrote
`impl<T> Named for T`, then for `i32` there would be two impls — exactly the ambiguity coherence forbids.
Owning the trait `Named` is what lets *you* (and only you) make this universal claim.

### 2. Bounds narrow the blanket to a subset

Real blanket impls almost always carry a bound:

```rust
impl<T: Display> Loud for T {
    fn loud(&self) -> String {
        format!("{}!!!", self)
    }
}
```

Now `7i32.loud()` and `"hi".loud()` work (both are `Display`), but a non-`Display` type gets a compile
error if you call `.loud()` on it. The bound is doing real work: it restricts *which* types the universal
claim applies to. Mentally, `impl<T: Display> Loud for T` reads as "for all `T` *where `T: Display`*,
`T: Loud`."

> **Key consequence for rung 6:** `impl<T> Loud for T` and `impl<T: Display> Loud for T` could **not**
> coexist for the same trait — every `Display` type would match both, and the compiler has no tiebreaker.
> Two *different* traits (`Named` vs `Loud`) is fine, because each impl is a separate fact about a
> separate trait.

### 3. The `From` → `Into` trick (std's most famous blanket impl)

This is the pattern in the standard library:

```rust
// std (paraphrased):
impl<T, U> Into<U> for T where U: From<T> {
    fn into(self) -> U { U::from(self) }
}
```

You implement `From`, and `.into()` materializes for free, in the correct direction. The ladder rebuilds
it with `MyFrom` / `MyInto` so the machinery is visible:

```rust
impl<T, U> MyInto<U> for T
where
    U: MyFrom<T>,
{
    fn my_into(self) -> U {
        U::my_from(self)
    }
}

impl MyFrom<Celsius> for Fahrenheit {
    fn my_from(c: Celsius) -> Fahrenheit {
        Fahrenheit(c.0 * 9.0 / 5.0 + 32.0)
    }
}
```

You write **zero** direct impls of `MyInto` — the one blanket covers every convertible pair. Note the
shape: the impl is `for T` (the *source*), with `U` a free type parameter pinned down by the
where-clause.

> **The inference gotcha.** In `let f: Fahrenheit = c.my_into();`, what supplies `U`? Nothing in `c`
> says `Fahrenheit` — the **type annotation** does. If you'd also written `impl MyFrom<Celsius> for Kelvin`,
> then `c.my_into()` with no annotation is ambiguous (E0282/E0283). Coherence guarantees *at most one*
> impl per `(T, U)` pair; it does **not** pick `U` for you. That's why real `.into()` calls so often need
> `let x: Target =` or a turbofish.

### 4. The extension trait — adding methods to types you don't own

You can't add an inherent method to `Iterator` (you don't own it). But you *can* define your own trait
and blanket-impl it for everything that is an `Iterator`. This is exactly how `itertools` bolts
`.chunks()`, `.dedup()`, etc. onto every iterator:

```rust
trait IterExt: Iterator<Item = u64> {        // supertrait: Self IS the iterator
    fn sum_of_squares(self) -> u64
    where
        Self: Sized,
    {
        self.map(|n| n * n).sum()
    }
}

impl<I: Iterator<Item = u64>> IterExt for I {} // empty body: inherits the default
```

One blanket impl, and `vec![...].into_iter()`, `(1..=3)`, and `(0..10).filter(...)` all gain
`.sum_of_squares()` — because they're all `Iterator<Item = u64>`.

**Two design shapes, know both:**

```rust
// Supertrait form (idiomatic, std/itertools use this):
trait IterExt: Iterator<Item = u64> { ... }
impl<I: Iterator<Item = u64>> IterExt for I {}

// Type-parameter form (works, but threads an extra param everywhere):
trait IterExt<I: Iterator<Item = u64>> { ... }
impl<I: Iterator<Item = u64>> IterExt<I> for I { ... }
```

The supertrait form makes `Self` *be* the iterator — no extra parameter to name in bounds. The
type-parameter form parameterizes the trait, so every bound that mentions it (`fn f<T: IterExt<?>>`) has
to thread the `I`. Prefer the supertrait.

Why does this need a *separate trait*? The orphan rule (next): you can't blanket-impl a *foreign* trait
over all iterators, and you can't add methods to `Iterator` itself. Owning `IterExt` is what makes the
blanket legal.

## Footguns

### E0117 — the orphan rule (foreign trait + foreign type)

```rust
// WRONG: Display is foreign, Vec<i32> is foreign -> E0117
impl Display for Vec<i32> { ... }

// OK: you own Summary (local trait), so a foreign type is fine
impl Summary for Vec<i32> { ... }

// OK: you own Temperature (local type), so a foreign trait is fine
impl Display for Temperature { ... }
```

The rule in one line: **at least one of `{trait, type}` must be yours.** The first breaks it from both
sides; the other two each satisfy it from one side. This is also why a blanket impl of a *foreign* trait
like `impl<T> Display for T` is doubly forbidden — it's a foreign trait *and* it would monopolize
`Display`, locking every other crate out of implementing it for their own types.

### E0119 — overlapping impls (no specialization on stable)

```rust
trait Kind { fn kind(&self) -> &'static str; }

// Both legal individually, both in your crate...
impl<T> Kind for T   { fn kind(&self) -> &'static str { "generic" } } // (D)
impl Kind for i32    { fn kind(&self) -> &'static str { "integer" } } // (C)
// ...but i32 matches BOTH -> E0119 conflicting implementations
```

The instinct is "the compiler should just prefer the more specific `i32` impl." That preference *is*
**specialization** — and it is **nightly-only**. On stable Rust there is no tiebreaker, so two impls that
can both match one type is simply ambiguous and rejected. The fix without specialization is to **not
overlap**: drop the blanket and write concrete impls per type, so exactly one matches each.

> Contrast with rung 3: `impl<T, U> MyInto<U> for T` never conflicted because it was the *only* impl of
> `MyInto`. Overlap requires *two* impls of the *same* trait both covering one type.

### E0210 — the uncovered type parameter (the subtle one)

The orphan rule is not just "some type must be local" — it's about **order and coverage**. Scanning
`Self`, then the trait's type arguments left-to-right, a **local** type must appear before any **bare**
(uncovered) type parameter.

- A bare `T` is **uncovered**.
- A `T` wrapped in *your* local type, like `Wrapper<T>`, is **covered**.

```rust
use std::ops::Add;
struct Meters(f64);

// WRONG: Add is foreign; Self is a BARE T (uncovered), and the only local
// type `Meters` appears AFTER it as Rhs -> E0210
impl<T> Add<Meters> for T { ... }

// OK: local type is in the Self position, first
impl Add for Meters { type Output = Meters; ... }

// OK: From is foreign, but T is COVERED by your local Wrapped<T>
impl<T> From<T> for Wrapped<T> { ... }
```

Why the asymmetry? `impl<T> Add<Meters> for T` claims `Add<Meters>` for types you **don't own** — so the
crate that owns some `Foo` could legitimately add `impl Add<Meters> for Foo`, and now `Foo` has two impls
the compiler can't see across crates. `impl<T> From<T> for Wrapped<T>` only ever claims `From` for *your*
`Wrapped`, and the orphan rule stops anyone else from impl'ing `From<…> for Wrapped<…>`. The covered case
is collision-proof; the uncovered one is a future-collision waiting to happen, so it's banned.

## Real-world patterns

### The newtype workaround

Rung 5 showed `impl Display for Vec<i32>` is illegal. The standard escape hatch: **wrap the foreign type
in your own local newtype**, then impl the foreign trait for the newtype. Now one side is local — legal.

```rust
struct Wrapper(Vec<i32>);

impl std::fmt::Display for Wrapper {       // legal: Wrapper is local
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts: Vec<String> = self.0.iter().map(|n| n.to_string()).collect();
        write!(f, "[{}]", parts.join(", "))
    }
}

impl std::ops::Deref for Wrapper {         // restore the inner type's methods
    type Target = Vec<i32>;
    fn deref(&self) -> &Vec<i32> { &self.0 }
}
```

The cost of a newtype is that you lose the inner type's methods; `Deref` buys them back via deref
coercion, so `w.len()`, `w.iter()`, `w.first()` all work.

> **`Deref` is fine here, but don't abuse it.** A transparent wrapper that exposes everything is the
> right use. But if the newtype exists to *enforce an invariant* (a `SortedVec`, a `NonEmptyVec`), `Deref`
> leaks the inner type's *mutators* (`push`, `clear`) and lets callers break the invariant behind your
> back. `Deref` should mean "*is-a* smart pointer to," not "*has-a* field I'm exposing." For restricting
> wrappers, expose a curated API instead.

## Capstone insight: sealing a trait with a private blanket impl

The capstone ships a tiny stats library: a `StatsExt` extension trait that adds `.mean()` and
`.variance()` to any `Iterator<Item = f64>` via a blanket impl — and then **seals** it so downstream code
can *use* the methods but can never *implement* the trait.

```rust
mod sealed {
    pub trait Sealed {}
    impl<I: Iterator<Item = f64>> Sealed for I {}   // the ONLY impl of Sealed
}

trait StatsExt: Iterator<Item = f64> + sealed::Sealed {
    fn mean(self) -> f64;
    fn variance(self) -> f64;
}

impl<I: Iterator<Item = f64>> StatsExt for I {
    fn mean(self) -> f64 { /* collect to Vec<f64>, average; empty -> 0.0 */ }
    fn variance(self) -> f64 { /* mean, then average of (x - mean)^2 */ }
}
```

The "aha" is how **coherence makes the seal unbreakable**:

1. `Sealed` is `pub` *inside a private module* — outside code literally cannot name it.
2. The only impl of `Sealed` is your blanket impl. Coherence means no one else can add another.
3. `StatsExt` requires `Sealed` as a supertrait. So to write `impl StatsExt for MyType`, a downstream
   crate would also need `MyType: Sealed` — which they can neither name nor satisfy.

The result: a public trait that is fully usable but **closed to implementation**. This is the production
pattern std uses to keep traits like `Error`-adjacent helpers (and many crate APIs) extensible internally
while presenting a stable, non-overridable surface. The blanket impl of a *private* trait is the gate;
coherence is the lock.

## Explain it back

Future-you should be able to answer these cold:

1. Why does `vec![1,2,3].to_string()` having "two answers" *have* to be a compile error rather than a
   runtime choice?
2. State the orphan rule in one sentence. Which of `{trait, type}` is local in `impl Display for Wrapper`?
3. Why can't `impl<T> Kind for T` and `impl Kind for i32` coexist on stable Rust? What single nightly
   feature would make it work, and what would it do?
4. In `let f: Fahrenheit = c.into()`, what supplies the target type? When does omitting the annotation
   become a hard error?
5. Why is `impl<T> Add<Meters> for T` (E0210) a future-collision risk, but `impl<T> From<T> for Wrapped<T>`
   is not? Define "covered."
6. When is `Deref` on a newtype the right call, and when does it actively break your type's guarantees?
7. In the sealed-trait capstone, name the three things that together make `impl StatsExt for MyType`
   impossible downstream.

## See also

- [Associated types vs generic params](assoc-vs-generic.md) — the other half of "designing a trait":
  `type Item` vs `<T>`, and where E0119 also shows up.
- [Conversion traits](conversions.md) — `From`/`Into`, `TryFrom`, the orphan rule and reflexivity in the
  conversion setting.
- [HRTB — for<'a>](hrtb.md) — the `DecodeOwned: for<'de> Decode<'de>` pattern is another supertrait-based
  bound, like the sealed-trait supertrait here.
