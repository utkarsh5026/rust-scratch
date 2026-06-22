# `impl Trait` & RPIT

> Ladder: [`src/bin/impl_trait.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/impl_trait.rs) ·
> Run: `cargo run --bin impl_trait` · Phase 2 · 9 rungs

## TL;DR

`impl Trait` means "some single concrete type that implements this trait, chosen at
this position." The one question that decides everything is **who picks the type?**

| Position | Syntax | Who picks | Desugars to |
|----------|--------|-----------|-------------|
| Argument (APIT) | `fn f(x: impl Trait)` | the **caller** | an anonymous generic param `<T: Trait>` |
| Return (RPIT) | `fn f() -> impl Trait` | the **callee** | one hidden concrete type the compiler knows but you can't name |

Everything else — the turbofish footgun, "all branches must be one type," lifetime
capture, `async fn` desugaring, RPITIT — falls out of those two facts.

## Why this exists (from first principles)

Some types **cannot be written down**. A closure has an anonymous, compiler-generated
type. An iterator chain like `(0..n).filter(...).map(...)` has a type like
`Map<Filter<Range<u32>, {closure}>, {closure}>` where `{closure}` is unnameable. Before
`impl Trait`, the only way to return one of these was to **erase** it behind a
`Box<dyn Trait>` — heap allocation plus a vtable on every call.

`impl Trait` in return position fixes this: you promise the caller "this is *some*
`Iterator<Item=u32>`," the compiler fills in the real type behind the scenes, and you
get a by-value, monomorphized return with **zero overhead** — no box, no vtable.

In argument position it is pure ergonomics: `fn f(x: impl Display)` reads better than
`fn f<T: Display>(x: T)`, and it is *exactly* the same thing after desugaring — with one
consequence (you lose the turbofish).

The deepest payoff: **`async fn` is built entirely on RPIT.** `async fn f() -> T` is
sugar for `fn f() -> impl Future<Output = T>`. Understanding RPIT *is* understanding how
async functions return their state machines.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | APIT basics | `impl Display` arg = sugar for `<T: Display>`; caller picks |
| 2 | foundations | RPIT basics | return `impl Iterator`; the real type is unspellable |
| 3 | mechanics | turbofish footgun | APIT == generic, but `impl`-arg has no name to turbofish |
| 4 | mechanics | the killer app | return a closure & an adapter chain — no `Box`, no vtable |
| 5 | footgun | one hidden type | `if/else` two iterators won't compile (E0308); fix 3 ways |
| 6 | footgun | lifetime capture | edition-2024 auto-capture + `+ use<>` opt-out (E0597) |
| 7 | real-world | `async fn` IS RPIT | `async fn` ≡ `-> impl Future`; the `Send` question |
| 8 | real-world | RPITIT | `impl Trait` in trait returns; async-fn-in-trait; not dyn-safe |
| 9 | capstone | combinator toolkit | RPIT/APIT/RPITIT everywhere, `Box<dyn>` only where forced |

## The ideas, built up

### 1. Argument position: the caller picks (APIT)

```rust
fn describe(x: impl Display) -> String {
    format!("[{x}]")
}
```

This is identical, after desugaring, to:

```rust
fn describe<T: Display>(x: T) -> String { format!("[{x}]") }
```

The same function body serves `describe(42)`, `describe("hi")`, and `describe(3.5)` —
three different concrete types, each chosen by the **caller** at the call site. "APIT"
(argument-position impl Trait) is just an anonymous generic parameter.

### 2. Return position: the callee picks (RPIT)

```rust
fn evens_up_to(n: u32) -> impl Iterator<Item = u32> {
    (0..n).filter(|x| x % 2 == 0)
}
```

Now the direction flips. The function body decides the concrete type, and the caller
only knows the *interface* (`Iterator<Item = u32>`). The real type is something like
`Filter<Range<u32>, {closure}>` — **you literally cannot write it in the signature**,
because the closure type has no name. That impossibility is the entire reason RPIT
exists. The caller has to `.collect()` (or otherwise consume it) to get back to a type
it can name.

> Mental model: RPIT is an *existential* type — "there exists one type `T: Iterator` and
> I'm returning it, but I'm hiding which one." APIT is a *universal* type — "for all
> `T: Trait` the caller chooses."

### 3. The turbofish footgun

APIT and a named generic are the same desugaring — but only the named generic gives you
a *name* to fill with turbofish.

```rust
fn count_args(x: impl Display) -> usize { x.to_string().len() }   // no name to turbofish

fn default_string<T: Default + Display>() -> String {             // named param `T`
    T::default().to_string()
}
```

`default_string` takes **no value argument**, so there's nothing to infer `T` from — the
only way to call it is `default_string::<i32>()`. An `impl Trait` argument literally
cannot express this case, because there is no type parameter in the `<...>` list to
fill. That is the one real cost of the argument-position sugar.

> Note: `count_args` uses `.to_string().len()`, which counts **bytes**, not chars. It
> matches the ASCII test cases, but `count_args("héllo")` would be 6, not 5. Use
> `.chars().count()` for characters.

### 4. The killer app: returning closures and chains

```rust
fn adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| x + n            // `move` captures n by value — without it the closure
}                            // would borrow n, which is gone when adder returns

fn pipeline<'a>(words: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
    words.iter().filter(|w| w.len() > 3).map(|w| w.to_uppercase())
}
```

Both return values have types you could never spell by hand. Before `impl Trait` you
would have written `Box<dyn Fn(i32) -> i32>` and `Box<dyn Iterator<Item = String>>` —
heap + dynamic dispatch. RPIT returns them by value, monomorphized.

### 5. The defining footgun: one hidden type, all branches

RPIT promises **exactly one** concrete type. So this is rejected:

```rust
// WRONG — E0308: `if` and `else` have incompatible types
fn ranged(rev: bool, n: u32) -> impl Iterator<Item = u32> {
    if rev { (0..n).rev() } else { 0..n }   // Rev<Range<u32>> vs Range<u32>
}
```

Both arms implement `Iterator<Item = u32>`, but they are *different concrete types*, and
a single RPIT can only hide one. Three ways to collapse the branches into one type, each
with a different cost:

```rust
// (a) ERASE — both arms coerce to the same trait object. Cost: heap + vtable.
fn ranged_box(rev: bool, n: u32) -> Box<dyn Iterator<Item = u32>> {
    if rev { Box::new((0..n).rev()) } else { Box::new(0..n) }
}

// (b) UNIFY — collect each arm into a Vec; both arms become vec::IntoIter<u32>.
//     Cost: eager allocation, loses laziness.
fn ranged_vec(rev: bool, n: u32) -> impl Iterator<Item = u32> {
    if rev { (0..n).rev().collect::<Vec<_>>().into_iter() }
    else   { (0..n).collect::<Vec<_>>().into_iter() }
}

// (c) BRANCH-AS-DATA — one enum that is itself an Iterator. No heap, stays lazy.
enum Either<L, R> { Left(L), Right(R) }
impl<L, R> Iterator for Either<L, R>
where L: Iterator, R: Iterator<Item = L::Item> {
    type Item = L::Item;
    fn next(&mut self) -> Option<Self::Item> {
        match self { Either::Left(l) => l.next(), Either::Right(r) => r.next() }
    }
}
```

Option (c) is exactly what `itertools::Either` is. The cost spectrum — Box (heap+vtable)
→ Vec (eager alloc) → Either (stack + lazy) — is the practical takeaway.

### 6. Lifetime capture (and edition 2024 changes the rules)

An RPIT's hidden type may **borrow** from the function's inputs, so the question is:
which lifetimes/type-params does the hidden type "capture"?

- **Edition 2021:** RPIT captured *nothing* unless you spelled it. Borrowing an input
  gave `E0700` ("hidden type captures lifetime that does not appear in bounds"); you
  fixed it by adding `+ '_` / `+ 'a` to the return.
- **Edition 2024** (this crate): RPIT **auto-captures all in-scope generic params and
  lifetimes.** So a function that borrows its input "just works" with no annotation:

```rust
// On 2024 this needs NO `+ 'a`. On 2021 it is E0700 without `+ '_`.
fn lengths<'a>(words: &'a [&'a str]) -> impl Iterator<Item = usize> {
    words.iter().map(|w| w.len())   // borrows words internally, yields owned usize
}
```

The new skill is the *opposite* problem — opting **out** of an over-broad capture with
precise-capturing `+ use<...>`:

```rust
// WRONG on 2024: auto-captures 'a even though the result owns nothing, so the
// returned iterator is wrongly tied to the borrow — caller can't outlive it (E0597).
fn counter(_data: &[i32]) -> impl Iterator<Item = i32> { 0..3 }

// OK: `use<>` = capture NOTHING. The iterator owns everything and outlives the borrow.
fn counter(_data: &[i32]) -> impl Iterator<Item = i32> + use<> { 0..3 }
```

Model it as: **2024 captures everything in scope by default; `use<...>` narrows the
set.** `use<>` captures nothing; `use<'a, T>` captures exactly those. The compiler even
suggests `+ use<>` in the E0597 message.

### 7. `async fn` IS return-position `impl Trait`

The reveal that ties the ladder together. These two are the same thing:

```rust
async fn double_async(x: u32) -> u32 { x * 2 }

fn double_rpit(x: u32) -> impl Future<Output = u32> {
    async move { x * 2 }
}
```

`async fn` is sugar: the compiler turns the body into an anonymous state-machine type
that implements `Future`, and hands it back via RPIT. The `Output` is whatever followed
the original `->`. Every RPIT rule still applies:

- **Capture:** the future borrows whatever the async block borrows.
- **The `Send` question:** the state machine is `Send` only if everything held *across
  an `.await`* is `Send` — the same auto-trait reasoning as the
  [`Send`/`Sync`](send-sync.md) ladder. `double_rpit(5)` is `Send` because only a `u32`
  lives across awaits, which `assert_send(&fut)` confirms.

### 8. RPITIT — `impl Trait` in trait returns

Since Rust 1.75 you can put `impl Trait` in a **trait method's** return type ("RPITIT"),
and `async fn` in traits is just RPITIT under the hood:

```rust
trait Source {
    fn values(&self) -> impl Iterator<Item = u32>;     // RPITIT
}

trait Greeter {
    async fn greet(&self) -> String;                   // ≡ fn greet(&self) -> impl Future<Output = String>
}
```

**The catch: a trait with an RPITIT (or `async fn`) method is not dyn-compatible.**

```rust
// E0038: `Source` cannot be made into an object.
let _boxed: Box<dyn Source> = Box::new(Squares);
```

Why? A vtable needs **one fixed return type per method** to store as a function pointer.
But each impl of `values` returns a *different* hidden type (`Squares::values` → some
`Map<...>`, another impl → some `Filter<...>`). There is no single signature to put in
the vtable. So you consume RPITIT traits through **generics / static dispatch**:

```rust
fn sum_source(s: impl Source) -> u32 { s.values().sum() }   // OK: monomorphized
```

This is precisely why `async fn` in traits historically needed the `async-trait` crate —
it `Box`es the future to erase it back into one nameable type — and why `dyn` async
traits still need help today.

## Footguns

| Trap | Symptom | Fix |
|------|---------|-----|
| Turbofish on an `impl Trait` arg | "cannot provide explicit generic arguments" | use a named generic param `<T>` instead |
| `if/else` returns two iterator types | E0308 incompatible types | `Box<dyn>`, collect-to-Vec, or an `Either` enum |
| RPIT over-captures a lifetime (2024) | E0597 "does not live long enough" | add `+ use<>` (or `+ use<'a, T>`) to narrow the capture |
| Borrowing input on edition 2021 | E0700 captures lifetime | add `+ '_` / `+ 'a` to the return type |
| `dyn Trait` on an RPITIT/async-fn trait | E0038 not dyn-compatible | use generics; or `Box` the return manually / `async-trait` |
| `.len()` for "char count" | wrong for non-ASCII | `.chars().count()` |

## Real-world patterns

- **Returning iterators from library functions** without exposing the concrete adapter
  type — the single most common RPIT use. `std`'s own `Vec::iter`, `HashMap::keys`, etc.
  return named types, but most application code returns `impl Iterator`.
- **`itertools::Either`** is the rung-5 enum, productized: the lazy, no-heap way to
  return one of two iterator types from a branch.
- **`async fn` everywhere** is RPIT in disguise. When you need `Send` futures (e.g. to
  spawn on a multithreaded runtime), you reason about what crosses each `.await`.
- **`async fn` in traits (1.75+)** for static-dispatch async APIs; `#[trait_variant]` /
  `async-trait` when you need `dyn`.
- **Precise capturing `use<>`** for returning owned iterators/futures that must outlive
  the borrowed data they were built from.

## Capstone insight

The capstone builds a small lazy combinator toolkit where **every builder hands back
`impl Trait`** — `compose` (RPIT closure + APIT bounds), `naturals()` (an infinite
`impl Iterator`), `keep` (threads any generic iterator through a filter), a RPITIT
`Stage` trait — and assembles them into a pipeline that stays lazy until the final
`collect()`:

```text
naturals() -> keep(evens) -> MulStage(10).apply -> compose((x+1)*2) -> take(3)
   1,2,3..        2,4,6           20,40,60            42,82,122        [42,82,122]
```

The single exception is `op_fn`, where a runtime `match` selects one of three closures:

```rust
fn op_fn(op: Op) -> Box<dyn Fn(u64) -> u64> {
    match op {
        Op::Inc    => Box::new(|x| x + 1),
        Op::Double => Box::new(|x| x * 2),
        Op::Square => Box::new(|x| x * x),
    }
}
```

Three different closure types, one per arm — the one-hidden-type rule means RPIT *cannot*
express it, so you **must** erase to `Box<dyn Fn>`. The whole lesson of the ladder in one
function: `impl Trait` carries you all the way until runtime branching over distinct
types forces type erasure, and there — and only there — you reach for `dyn`.

## Explain it back

- What's the difference between `fn f(x: impl Trait)` and `fn f() -> impl Trait` in terms
  of *who chooses the type*?
- Why can't you turbofish a function whose parameter is written `impl Trait`?
- Why does `if cond { a } else { b }` fail when `a` and `b` are different iterator types
  behind one `impl Iterator` return — and what are three ways to fix it?
- On edition 2024, what does `+ use<>` mean, and what error does omitting it cause when an
  RPIT accidentally captures a lifetime it doesn't need?
- `async fn f() -> T` desugars to what signature? When is the resulting future `Send`?
- Why is a trait with an `async fn` / RPITIT method not `dyn`-compatible, and how do you
  consume it instead?

## See also

- [Static vs dynamic dispatch](dispatch.md) — the `impl Trait` vs `dyn Trait` vs enum
  trade-off, and object safety in depth.
- [Closures & Fn/FnMut/FnOnce](closures.md) — what `impl Fn` is actually returning.
- [Iterators end-to-end](iterators.md) — the adapter chains whose types RPIT hides.
- [`Send` & `Sync` deeply](send-sync.md) — the auto-trait reasoning behind `Send` futures.
- [HRTB — for<'a>](hrtb.md) — higher-ranked bounds, the other place lifetimes get subtle.
