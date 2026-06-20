# Generic bounds & `where` clauses

> Ladder: [`src/bin/generic_bounds.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/generic_bounds.rs) ·
> Run: `cargo run --bin generic_bounds` · Phase 2 · 9 rungs

## TL;DR

A generic parameter `T` arrives as a **black box**: the compiler knows nothing about it, so you
can't call anything on it. A **bound** (`T: Trait`) is a contract that does two things at once —
it **restricts the caller** ("you may only pass types that implement `Trait`") and **empowers the
body** ("therefore I'm allowed to use `Trait`'s methods on a `T`"). Every method you call on a
generic must be justified by a bound.

A `where` clause is the same bounds written *below* the signature instead of inline. It's not just
cosmetic: the inline `<T: Bound>` form can only bound a bare type parameter, so anything
structured — a projection like `I::Item`, or a bound on a derived type like `&'a C` or `Vec<T>` —
**must** live in a `where` clause. That's the dividing line.

## Why this exists (from first principles)

Rust monomorphizes generics: `min_item::<i32>` and `min_item::<char>` compile to separate machine
code. But type-*checking* happens **once, on the generic definition**, before any concrete type is
known — not separately per instantiation (that's C++ templates, where errors surface at the use
site as walls of noise).

So when the compiler sees this generic body, it must decide *right now* whether it's legal:

```rust
fn min_item<T>(items: &[T]) -> T {
    // is `a < b` allowed here? the compiler has NO idea what T is.
}
```

With nothing known about `T`, almost nothing is permitted — you can move it, drop it, take its
address, and little else. A bound is how you tell the checker what `T` is guaranteed to support, so
it can verify the body once and trust it for every future `T`:

```rust
fn min_item<T: PartialOrd>(items: &[T]) -> T {
    // now `a < b` typechecks: PartialOrd guarantees it for EVERY T a caller can pass.
}
```

This is the whole game. **Bounds are how you trade away "any type at all" for "the capabilities you
actually need."** Too few bounds and the body won't compile; too many and you needlessly reject
callers (the over-constraint footgun in rung 3).

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|---|---|---|
| 1 | foundations | `min_item<T: PartialOrd + Copy>` | A single bound turns the black box into something comparable. |
| 2 | foundations | `dedup_describe` w/ 3 bounds | Multiple bounds; `where` keeps a crowded signature readable. |
| 3 | mechanics | `Stack<T>` | Bound the **method**, not the struct — don't over-constrain. |
| 4 | mechanics | `Pair<T>::cmp_display` | A method that **exists only for some `T`** (conditional method). |
| 5 | footgun | `show<T: Display + ?Sized>` | The hidden `Sized` bound, and how `?Sized` relaxes it. |
| 6 | footgun | `join_display` / `sum_borrowed` | Bounds you can write **only** in a `where` clause. |
| 7 | real-world | `trait Summary` blanket impl | One impl gives *every* qualifying type a method; coherence is the cost. |
| 8 | real-world | `PartialEq/Clone for MyBox<T>` | Conditional **trait** impl — what `#[derive]` actually emits. |
| 9 | capstone | `trait IterExt` | Supertrait + blanket impl + per-method `where Self::Item:` bounds. |

## The ideas, built up

### 1. A bound is a contract in two directions

```rust
fn min_item<T>(items: &[T]) -> T
where
    T: PartialOrd + Copy,
{
    *items
        .iter()
        .min_by(|a, b| a.partial_cmp(b).expect("items are comparable"))
        .expect("items is non-empty")
}
```

Two bounds, two distinct reasons:

- **`PartialOrd`** lets the body compare elements (`a.partial_cmp(b)`). Without it, `<` and
  `partial_cmp` don't exist for `T`.
- **`Copy`** lets the function *return a `T` by value* out of a borrowed `&[T]`. You're handing back
  one of the borrowed elements; `Copy` says "duplicating it is a trivial bit-copy, the original
  stays put."

> **Why `PartialOrd`, not `Ord`?** The test passes `&[2.5, 0.5, 7.0]`. Floats are only `PartialOrd`,
> never `Ord`, because `NaN` makes them *not totally ordered* (`NaN < x`, `NaN > x`, and `NaN == x`
> are all false). Reaching for `PartialOrd` keeps `f64` callers in; demanding `Ord` would lock them
> out. Picking the **weakest bound that still compiles** is a real API-design instinct — see
> [Associated types vs generic params](assoc-vs-generic.md) for the same theme.

### 2. Multiple bounds, and where `where` earns its keep

```rust
fn dedup_describe<T>(items: &[T]) -> String
where
    T: PartialEq + Copy + Debug,
{
    let mut result = Vec::new();
    for item in items {
        if result.last() != Some(item) {   // PartialEq: compare neighbours
            result.push(*item);            // Copy: duplicate out of the borrow
        }
    }
    format!("{:?}", result)                // Debug: render with {:?}
}
```

Each bound again maps to one capability: `PartialEq` for the `!=`, `Copy` for `*item`, `Debug` for
`{:?}`. With three bounds, inline `<T: PartialEq + Copy + Debug>` already crowds the line; the
`where` form scales without pushing the return type off-screen. For *these* bounds it's pure style —
they'd work inline too. Rung 6 is where `where` stops being optional.

> A subtlety worth noting: `result.last()` is `Option<&T>` and `Some(item)` is `Option<&T>`, so the
> `!=` compares two `Option<&T>`. That works because `PartialEq` is *lifted* through `Option` and
> `&` — `Option<&T>: PartialEq` holds whenever `T: PartialEq`. The single bound on `T` quietly
> powers a comparison two layers up.

### 3. Bound the method, not the struct

The single most common beginner mistake:

```rust
// WRONG: the bound infects every use site.
struct Stack<T: Debug> { items: Vec<T> }
// Now `Stack<SomethingNotDebug>` won't even compile — you can't store a socket,
// a closure, or any non-Debug type, even if you never print it.

// OK: the struct holds ANYTHING; the capability lives on the impl that needs it.
struct Stack<T> { items: Vec<T> }

impl<T> Stack<T> {                 // unbounded: available for every T
    fn new() -> Self { Self { items: Vec::new() } }
    fn push(&mut self, value: T) { self.items.push(value); }
    fn len(&self) -> usize { self.items.len() }
}

impl<T: Debug> Stack<T> {          // bounded: only when T: Debug
    fn dump(&self) -> String { format!("{:?}", self.items) }
}
```

The ladder proves it: `Stack<NotDebug>` (a type with no `Debug` impl) still constructs, pushes, and
reports its length, because those methods live in the unbounded `impl<T>`. Only `dump` requires
`Debug`, and only `dump` is gated.

This is exactly how `Vec<T>` is built. `Vec<T>` stores any `T`; `.contains` appears only for
`T: PartialEq`, `.to_vec` only for `T: Clone`, `.sort` only for `T: Ord`. The capabilities are
sliced across many `impl` blocks so the container itself constrains nothing.

> **Rule of thumb:** put a bound at the **lowest** point that needs it. On a struct definition it's
> almost always wrong; on the impl block or the individual method is almost always right.

### 4. A method that exists only for some `T`

Push rung 3 one notch further: the bound can gate a *single method*, and a value whose `T` doesn't
satisfy it simply doesn't have that method.

```rust
struct Pair<T> { first: T, second: T }

impl<T> Pair<T> {
    fn new(first: T, second: T) -> Self { Self { first, second } }
}

impl<T: PartialOrd + std::fmt::Display> Pair<T> {
    fn cmp_display(&self) -> String {
        let largest = if self.first > self.second { &self.first } else { &self.second };
        format!("the largest is {}", largest)   // > from PartialOrd, {} from Display
    }
}
```

`Pair<NotDebug>` is a perfectly valid, constructible type — it just has a **smaller API surface**.
Try to call the gated method on it and you get:

```text
error[E0599]: the method `cmp_display` exists for struct `Pair<NotDebug>`,
              but its trait bounds were not satisfied
              `NotDebug: PartialOrd` is not satisfied
```

Note the wording: the method *exists*, but its bounds aren't met. Method availability is decided
**per concrete type**, at the call site. This is the literal mechanism behind the Rust Book's
`cmp_display` example, and behind every "why doesn't `.sum()` work on my `Vec<String>`" question.

### 5. The hidden `Sized` bound, and `?Sized`

Here is a bound you never wrote but is always there:

```rust
fn show<T>(x: &T) -> String { ... }
// really means:
fn show<T: Sized>(x: &T) -> String { ... }
```

Every generic parameter has an **implicit `T: Sized`** — Rust assumes types have a size known at
compile time, because that's what you need to put them on the stack, pass them by value, etc. The
consequence bites the moment you try to use a **DST** (dynamically sized type) like `str` or `[u8]`:

```rust
fn show<T: std::fmt::Display>(x: &T) -> String { format!("{}", x) }

show(&42);                  // ok: T = i32, Sized
show("hello str");          // ERROR before the fix
```

```text
error[E0277]: the size for values of type `str` cannot be known at compilation time
```

Why? The argument `"hello str"` is `&str`, which matches `&T` with **`T = str`**. But `str` is
unsized, and the implicit `Sized` rejects it. The fix is the one bound you *remove* rather than add:

```rust
fn show<T: std::fmt::Display + ?Sized>(x: &T) -> String { format!("{}", x) }
//                              ^^^^^^ opt out of the default Sized bound
```

`?Sized` means "T *might not* be sized." The price: you may only touch the value **behind a
pointer** (`&T`, `Box<T>`, `Rc<T>`), never by value — because by-value needs a size. That is the
deep reason you always see `&str` and never bare `str` in a signature, and why
`impl<T: Display + ?Sized> ToString for T` (the impl that gives `str` a `.to_string()`) needs that
`?Sized`.

### 6. Bounds you can write *only* in a `where` clause

This rung is the concrete answer to "when do I actually *need* `where`?" Inline `<T: Bound>` syntax
can only attach a bound to a **bare type parameter**. The moment your bound is about a *type
expression* — `T::Item`, `&T`, `Vec<T>` — it has nowhere to go but a `where` clause.

**6a — associated-type projection.** You can declare `<I: IntoIterator>` inline, but the bound that
its *items* are printable is a fact about `I::Item`, not `I`:

```rust
fn join_display<I>(iter: I) -> String
where
    I: IntoIterator,
    I::Item: std::fmt::Display,   // a projection — cannot go inline in <...>
{
    iter.into_iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ")
}
```

**6b — a higher-ranked bound on a *derived* type.** To sum a collection *by reference* (without
consuming it), the capability you need is "I can iterate `&C`", which is a bound on `&'a C`, not on
`C`:

```rust
fn sum_borrowed<'a, C>(collection: &'a C) -> i32
where
    &'a C: IntoIterator<Item = &'a i32>,   // bound on &'a C — impossible inline
{
    let mut sum = 0;
    for item in collection { sum += item; }   // uses the &C: IntoIterator impl
    sum
}
```

> The fully general version of 6b uses a **higher-ranked trait bound**:
> `where for<'a> &'a C: IntoIterator<Item = &'a i32>` — "for *any* lifetime, `&C` is iterable." See
> [HRTB — `for<'a>`](hrtb.md) for why `for<'a>` is needed and how it differs from a single named
> `'a`. Either form proves the same point: the bound is structurally a clause about `&C`, and only
> `where` accepts clauses about type expressions.

### 7. Blanket impls — implement a trait for *every* qualifying type

```rust
trait Summary {
    fn summary(&self) -> String;
}

impl<T: Debug> Summary for T {            // ONE impl covers infinitely many types
    fn summary(&self) -> String { format!("{:?}", self) }
}
```

After this, `42.summary()`, `vec![1, 2].summary()`, and `Point { x: 1, y: 2 }.summary()` all work
with **zero per-type code**. This is the mechanism behind `ToString` (`impl<T: Display + ?Sized>
ToString for T`) and `Into` (`impl<T, U: From<T>> Into<U> for T` — implement `From`, get `Into`
free).

The cost is **coherence**. Once a blanket impl covers a set of types, you cannot carve out a special
case:

```rust
// uncommenting this triggers:
// error[E0119]: conflicting implementations of trait `Summary` for type `i32`
impl Summary for i32 {
    fn summary(&self) -> String { format!("the int {}", self) }
}
```

`i32` is already covered by the blanket impl, and stable Rust has no specialization, so the second
impl is an illegal overlap. This trade-off — "implement for all `T: Bound`" versus "exactly one impl
per (trait, type)" — is the central tension of trait design. It's covered in depth in its own note:
[Blanket impls & coherence](blanket-coherence.md).

### 8. Conditional trait impls — what `#[derive]` really does

A wrapper should gain a capability *only when its contents have it*. That's a conditional **trait**
impl, and it's literally what `#[derive(PartialEq)]` and `#[derive(Clone)]` expand to:

```rust
struct MyBox<T>(T);   // no derives — hand-written below

impl<T: PartialEq> PartialEq for MyBox<T> {
    fn eq(&self, other: &Self) -> bool { self.0 == other.0 }
}

impl<T: Clone> Clone for MyBox<T> {
    fn clone(&self) -> Self { MyBox(self.0.clone()) }
}
```

The bound lives **on the impl block**, not on the struct. So `MyBox<T>` exists for any `T`; it only
*acquires* `==` when `T: PartialEq` and `.clone()` when `T: Clone`. Two consequences the ladder
checks:

- `MyBox<MyBox<i32>>` is comparable, because the requirement **recurses**: `MyBox<i32>: PartialEq`
  holds because `i32: PartialEq`, so `MyBox<MyBox<i32>>: PartialEq` holds in turn.
- A `MyBox` of a non-comparable type silently lacks `==` — no error until you try to use it.

> **The one place `#[derive]` is subtly wrong.** `#[derive(Clone)]` on `MyBox<T>` mechanically emits
> `impl<T: Clone> Clone for MyBox<T>`. But if the field were an `Rc<T>`, `MyBox` would be cloneable
> *even when `T` itself isn't* (cloning an `Rc` just bumps a refcount). Hand-writing the impl lets
> you choose a **tighter or looser** bound than derive's reflexive `T: Clone`. Crates like
> `derivative` exist precisely to fix this.

## Footguns

| Trap | What happens | Fix |
|---|---|---|
| Bound on the **struct** (`struct S<T: Debug>`) | Every `S<NonDebug>` fails to construct, even when the capability is never used. | Move the bound to the impl/method that needs it (rung 3). |
| Forgetting `T` is implicitly `Sized` | Passing a `str`/`[T]` gives E0277 "size cannot be known at compile time". | Add `?Sized` and take the value behind a reference (rung 5). |
| Trying to bound `T::Item` / `&T` inline | Syntax error — inline bounds only attach to a bare `T`. | Use a `where` clause (rung 6). |
| `self`-by-value method in a trait without `Self: Sized` | E0277 — the `self` parameter needs a known size. | Add `where Self: Sized` (seen in the capstone). |
| Demanding `Ord` / `Eq` when `PartialOrd` / `PartialEq` suffices | Locks out `f64` and other partially-ordered types. | Use the weakest bound the body actually needs (rung 1). |
| Special-casing one type under a blanket impl | E0119 conflicting implementations. | You can't, on stable — design around it (rung 7). |

## Real-world patterns

- **Capability slicing across impl blocks.** `Vec<T>`, `HashMap<K, V>`, `Option<T>` all keep the
  type definition unbounded and attach methods to bounded impls. Mimic this in your own containers.
- **Blanket extension traits.** `itertools::Itertools` and `tower::ServiceExt` declare a trait with
  default methods plus `impl<T: Bound> Ext for T {}`, instantly adding methods to every existing
  type. The capstone builds a miniature of this.
- **`?Sized` in generic APIs.** Functions that should accept `&str` *and* `&String` take
  `T: AsRef<str> + ?Sized` or `impl AsRef<str>`; `ToString`/`Borrow`/`Hash` impls thread `?Sized`
  through so DSTs participate.
- **Conditional impls = how `#[derive]` works.** Every derived `Clone`/`PartialEq`/`Debug` is a
  `impl<T: Trait> Trait for Wrapper<T>`. Reading derive output demystifies a huge amount of std.

## Capstone insight

The `IterExt` extension trait fuses every earlier rung into the exact pattern real iterator-adapter
crates use:

```rust
trait IterExt: Iterator {                       // supertrait: gives access to Self::Item + iteration
    fn min_max(self) -> Option<(Self::Item, Self::Item)>
    where
        Self: Sized,                            // self-by-value needs a known size
        Self::Item: Ord + Copy,                 // per-method capability bound
    { /* fold to running (min, max); `min.zip(max)` yields None if empty */ }

    fn counts(self) -> HashMap<Self::Item, usize>
    where
        Self: Sized,
        Self::Item: Eq + Hash,                  // HashMap key requirements
    { /* *map.entry(item).or_insert(0) += 1 */ }

    fn join_with(self, sep: &str) -> String
    where
        Self: Sized,
        Self::Item: std::fmt::Display,
    { /* map(to_string).collect::<Vec<_>>().join(sep) */ }
}

impl<I: Iterator> IterExt for I {}              // blanket impl: EVERY iterator gets all three
```

Three ideas snap together:

1. **Supertrait bound** (`: Iterator`) — every method body can use `Self::Item` and consume `self`
   by iterating.
2. **Blanket impl** (`impl<I: Iterator> IterExt for I {}`) — like rung 7, this hands the methods to
   every iterator in the program for free. The method *bodies* live as defaults in the trait; the
   impl is empty. This is the canonical Itertools shape.
3. **Per-method `where Self::Item:` bounds** — like rung 6, each adapter is callable **only** when
   the element type qualifies. `"abc".chars().min_max()` works (`char: Ord + Copy`); an iterator of
   a non-`Ord` type silently won't offer `min_max`.

The aha: this is **precisely how `std::iter::Iterator` itself is built**. `.sum()` needs
`Self::Item: Sum`, `.max()` needs `Ord`, `.collect::<String>()` needs the right `FromIterator`.
Bounds aren't bureaucracy bolted onto generics — they're the dials that let one trait expose a
different API to every element type, decided independently at each call.

## Explain it back

- Why can't you call any methods on a bare `T` with no bounds? What *can* you still do with it?
- A bound restricts the caller and empowers the body. Give one concrete example of each direction
  from `min_item`.
- Why does `min_item` use `PartialOrd` instead of `Ord`? Which caller would `Ord` exclude?
- Where should the bound go: on `struct Stack<T>` or on an `impl`? Why is the struct almost always
  wrong?
- What is the hidden default bound on every `<T>`? What exactly does `?Sized` change, and why must a
  `?Sized` value sit behind a reference?
- Name two bounds that can be written *only* in a `where` clause, and say why inline syntax can't
  express them.
- What does `#[derive(Clone)]` expand to for `struct MyBox<T>(T)`? When is that derived bound too
  strict?
- In `IterExt`, why does each method need both `where Self: Sized` and a `where Self::Item: ...`
  bound? What goes wrong without each?

## See also

- [Blanket impls & coherence](blanket-coherence.md) — the E0119/orphan-rule story behind rung 7, in depth.
- [Associated types vs generic params](assoc-vs-generic.md) — the *other* axis of generic API design.
- [HRTB — `for<'a>`](hrtb.md) — the `for<'a> &'a C: ...` bound from rung 6, fully unpacked.
- [Static vs dynamic dispatch](dispatch.md) — what bounds enable at monomorphization vs. `dyn Trait`.
- [Lifetimes in depth](lifetimes-depth.md) — `'a: 'b` outlives bounds are bounds too.
