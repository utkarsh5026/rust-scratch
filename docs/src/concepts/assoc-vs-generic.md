# Associated Types vs Generic Params

> Ladder: [`src/bin/assoc_vs_generic.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/assoc_vs_generic.rs) ·
> Run: `cargo run --bin assoc_vs_generic` · Phase 2 · 9 rungs

## TL;DR

A trait can carry "extra" types in two ways, and the choice is not stylistic — it
changes what the type system lets you do:

- **Generic parameter** (`trait Convert<T>`): the type is an **input**. The
  caller or the impl *picks* it, so one type can implement the trait **many
  times**, once per choice of `T`.
- **Associated type** (`trait Iterator { type Item; }`): the type is an
  **output**. The implementor *determines* it once, so there is exactly **one
  impl per type** and the compiler can *deduce* the output instead of asking you.

> Rule of thumb: **input → generic param, output → associated type.**

## Why this exists (from first principles)

Say you want a trait whose method returns "some related type". You need to tell
the trait what that type is. There are only two places it can come from:

1. The **caller** supplies it. Then it must be a parameter on the trait:
   `Convert<T>`. Different callers want different `T`, so the same type must be
   allowed to implement `Convert<i32>` *and* `Convert<String>`.
2. The **implementor** fixes it. Then it belongs *inside* the impl as an
   associated type: `type Output = i32;`. There is one right answer per type, so
   a second impl with a different answer would be a contradiction.

That single fork — "who chooses the type?" — drives everything else: how many
impls are allowed, whether the compiler can infer the result, whether you can put
it behind `dyn`, and how the whole iterator-adapter ecosystem resolves element
types at compile time.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|-----------|
| 1 | foundations | Two shapes | Same trait written with `type Item` vs `<T>`; feel the syntax |
| 2 | foundations | The defining rule | One impl per type (assoc) vs many (generic); `E0119` |
| 3 | mechanics | Equality bounds | `where I: Iterator<Item = u64>` + `I::Item` projection |
| 4 | mechanics | Your own iterator | `impl Iterator` with `type Item` for `Countdown` |
| 5 | footgun | Inference & turbofish | Generic `.into()` ambiguity (`E0283`) vs determined output |
| 6 | footgun | Trait objects | `dyn Iterator<Item=..>` must pin the assoc type (`E0191`) |
| 7 | real-world | `Add` uses both | `Rhs` generic param + `Output` associated, in one trait |
| 8 | real-world | Design the split | A `Graph` trait — decide what's assoc vs generic |
| 9 | capstone | `MyIterator` + `Map` | Thread an associated `Item` through a generic adapter |

## The ideas, built up

### 1. Two shapes for the same idea

The same "pop an item out" trait, written both ways:

```rust
// Shape A: associated type — implementor names the output once.
trait PopAssoc {
    type Item;
    fn pop_it(&mut self) -> Option<Self::Item>;
}

// Shape B: generic param — output is a parameter on the trait.
trait PopGeneric<T> {
    fn pop_it(&mut self) -> Option<T>;
}
```

The difference shows up at the impl site. With the associated type the chosen
type goes **inside** the impl body; with the generic param it goes in the **impl
header**:

```rust
impl PopAssoc for Stack {
    type Item = i32;                 // output: declared inside
    fn pop_it(&mut self) -> Option<Self::Item> { self.items.pop() }
}

impl PopGeneric<i32> for Stack {    // input: chosen in the header
    fn pop_it(&mut self) -> Option<i32> { self.items.pop() }
}
```

Because `Stack` now has two `pop_it` methods (one per trait), a bare
`s.pop_it()` is ambiguous — the ladder calls them with fully-qualified syntax
(`PopAssoc::pop_it(&mut s)`, `PopGeneric::<i32>::pop_it(&mut s)`). That ambiguity
is a first hint that generic params multiply impls.

### 2. The defining rule: one impl vs many

This is the whole concept in miniature. An associated type makes the trait a
**function** of `Self` — one input, one answer:

```rust
trait Producer { type Output; fn produce(&self) -> Self::Output; }

impl Producer for Counter { type Output = i32; /* ... */ }

// WRONG: a second impl, even with a different Output, is rejected.
// impl Producer for Counter { type Output = String; /* ... */ }
//   error[E0119]: conflicting implementations of trait `Producer`
//                 for type `Counter`
```

A generic param makes the trait a **relation** — many answers are fine:

```rust
trait Convert<T> { fn convert(&self) -> T; }

impl Convert<i32>    for Counter { /* ... */ }   // OK
impl Convert<String> for Counter { /* ... */ }   // OK — different T
```

The consequence you feel immediately: `produce()` needs no annotation (one
answer), but `convert()` does (the compiler must know *which* impl):

```rust
let p = c.produce();              // i32, deduced
let as_int: i32 = c.convert();    // must say which T
let as_str: String = c.convert();
```

### 3. Equality bounds and projection

Associated types unlock two things generic params make clumsy.

**Equality bounds** pin the output type *inside* a `where` clause, keeping the
iterator the only type parameter:

```rust
fn sum_items<I>(it: I) -> u64
where
    I: Iterator<Item = u64>,   // "any iterator whose Item is exactly u64"
{
    it.sum()
}
```

**Projection** lets you name the output as `I::Item` in your own signature — again
with no extra type parameter:

```rust
fn first<I>(mut it: I) -> Option<I::Item>
where
    I: Iterator,
{
    it.next()
}
```

Contrast the generic-trait version. With `trait Stream<T>` you would be forced to
introduce a separate `T` that leaks into every signature:

```rust
// What you'd be stuck writing with a generic-param iterator trait:
fn first_g<S, T>(s: S) -> Option<T> where S: Stream<T> { /* ... */ }
//        ^^^ extra param, and callers must disambiguate T because a type
//            could implement Stream<u64> AND Stream<String>.
```

Associated types turn that `T` from a parameter-you-must-supply into an
output-the-compiler-deduces.

### 4. Implementing the real `Iterator`

`Iterator` is *the* canonical associated-type trait:

```rust
trait Iterator { type Item; fn next(&mut self) -> Option<Self::Item>; }
```

Why is `Item` associated? Because a given iterator yields exactly one type of
value. If it were generic (`Iterator<T>`), a single type could "be an iterator"
of many `T`, and then `for x in it` wouldn't know what `x` is, and `.map`,
`.filter`, `.sum` would all be ambiguous. The ladder implements it for a
countdown:

```rust
impl Iterator for Countdown {
    type Item = u32;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current == 0 { None }
        else { let c = self.current; self.current -= 1; Some(c) }
    }
}
```

The payoff: because you implemented the *real* std trait, you get `.collect()`,
`.sum()`, `.map()`, and for-loops for free — all keyed off the single `Item`.

## Footguns

### Generic params owe you a disambiguation tax (`E0283`)

Because `Counter: Convert<T>` holds for more than one `T`, a function that returns
the generic output can't be called without help:

```rust
fn pull<T>(c: &Counter) -> T where Counter: Convert<T> { c.convert() }

// let oops = pull(&c);          // error[E0283]: type annotations needed
let via_annotation: i32 = pull(&c);   // fix 1: pin via the binding's type
let via_turbofish = pull::<String>(&c); // fix 2: pin at the call site
```

This is the same tax you already pay on `.into()`, `.parse()`, and
`.collect::<Vec<_>>()`. Associated outputs (`c.produce()`) never charge it, because
there is only one answer.

### `dyn` forces you to pin the associated type (`E0191`)

A trait object must be a concrete, fully-known type behind the pointer. So the
associated type has to be nailed down:

```rust
fn boxed_counter(n: u32) -> Box<dyn Iterator<Item = u32>> { /* ... */ }

// WRONG:
// fn bad(n: u32) -> Box<dyn Iterator> { /* ... */ }
//   error[E0191]: the value of the associated type `Item` must be specified
```

For a generic-param trait the analogue is simply choosing which object you mean:
`dyn Convert<i32>` and `dyn Convert<String>` are two unrelated trait-object types.
The associated type is *part of the object's identity*; the generic param *selects*
the object.

## Real-world patterns

### `Add` deliberately uses both

`std::ops::Add` is the masterclass — it carries a generic param **and** an
associated type, each chosen for the right reason:

```rust
pub trait Add<Rhs = Self> {
    type Output;
    fn add(self, rhs: Rhs) -> Self::Output;
}
```

- `Rhs` is a **generic param** (an input): you might add `Meters + Meters`, or
  `Meters + f64`, or `Meters + Vector`. Multiple right-hand sides → many impls per
  type. It even defaults to `Self`.
- `Output` is an **associated type** (an output): once you fix the *pair*
  `(Self, Rhs)`, the result type is determined. One answer per impl.

```rust
impl Add        for Meters { type Output = Meters; /* Meters + Meters */ }
impl Add<f64>   for Meters { type Output = Meters; /* Meters + f64    */ }

// The determined output can even be named with projection:
let r: <Meters as Add<f64>>::Output = Meters(3.0) + 1.0;
```

### Designing your own split

When you design a trait, sort each "extra type" into input or output. The ladder's
`Graph` trait makes both node-id and weight associated, because a graph has exactly
one of each — they are facts about the graph, not knobs a caller turns:

```rust
trait Graph {
    type NodeId: Copy + Eq;   // one id type per graph  → associated
    type Weight;              // one weight type per graph → associated
    fn neighbors(&self, n: Self::NodeId) -> Vec<(Self::NodeId, Self::Weight)>;
}

// A consumer stays clean — one type param, node type via projection:
fn neighbor_count<G: Graph>(g: &G, n: G::NodeId) -> usize { g.neighbors(n).len() }
```

Had `NodeId` been a generic `Graph<N>` param, a single graph type could claim to
be a graph of `u32` ids *and* `(i32,i32)` ids, and every consumer would need an
extra ambiguous type parameter.

## Capstone insight

The capstone rebuilds the iterator-adapter machinery and reveals the deepest move:
**an adapter's associated type is computed from its generic parameters.**

```rust
struct Map<I, F> { iter: I, f: F }   // generic over inner iter + closure

impl<I, F, B> MyIterator for Map<I, F>
where
    I: MyIterator,
    F: FnMut(I::Item) -> B,          // F maps inner items to some B
{
    type Item = B;                   // <-- the adapter's output IS the closure's output
    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some(x) => Some((self.f)(x)),
            None => None,
        }
    }
}
```

`type Item = B` is the whole trick. `B` is a generic parameter of the *impl*,
constrained by the closure's return type, and it becomes the *associated* type of
the resulting iterator. That is how a chain like

```rust
Upto { next: 1, end: 4 }
    .map_it(|x| x + 10)        // u32 -> u32
    .map_it(|x| x as usize * 2) // u32 -> usize
```

threads its element type `u32 → u32 → usize` entirely through associated-type
projection, resolved statically with zero annotations. Every std iterator chain
you have ever written works exactly this way.

## Explain it back

Answer these cold:

1. Why can a type implement `From<A>` and `From<B>` but not have two `Iterator`
   impls with different `Item`s?
2. Why does `let x: i32 = something.into()` need the annotation while
   `iter.next()` does not?
3. What does `where I: Iterator<Item = u64>` give you that
   `where I: Stream<u64>` (a generic-param trait) would not?
4. Why must you write `Box<dyn Iterator<Item = u32>>` and not `Box<dyn Iterator>`?
5. In `Add<Rhs = Self> { type Output; }`, why is `Rhs` generic but `Output`
   associated?
6. In the `Map<I, F>` adapter, where does `type Item` come from, and why is that
   the key to compile-time iterator chains?

## See also

- [Conversion traits](conversions.md) — `From`/`Into` are the archetypal
  generic-param traits (and the source of `.into()` ambiguity).
- [Borrow / ToOwned](borrow-toowned.md) — `ToOwned::Owned` is an associated type
  used exactly as an "output determined by the impl".
- [Lifetimes in depth](lifetimes-depth.md) — the `Iterator` `Item` lifetime rung
  is the lifetime-flavored version of projection.
