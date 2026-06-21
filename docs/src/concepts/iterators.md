# Iterators end-to-end

> Ladder: [`src/bin/iterators.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/iterators.rs) ·
> Run: `cargo run --bin iterators` · Phase 3 · 9 rungs

## TL;DR

An iterator is a tiny state machine with **one** required method:

```rust
fn next(&mut self) -> Option<Self::Item>;
```

That's the whole engine. Everything else — `map`, `filter`, `zip`, `sum`, `collect` —
is built on top of `next`. Two facts unlock the entire topic:

1. **Adapters are lazy.** `map`/`filter`/`take` don't *do* anything; each one returns a
   new struct that *remembers* what to do. No work happens until a **consumer** starts
   pulling.
2. **Consumers drive the pull.** `for`, `collect`, `sum`, `count`, `next` are the verbs.
   They call `next()` in a loop, and *that* cascades the pull back through every adapter
   to the source — one item at a time.

`for x in thing` is sugar for `IntoIterator::into_iter(thing)` followed by a
`while let Some(x) = it.next()` loop. Master `next`, laziness, and `IntoIterator`, and
the rest is vocabulary.

## Why this exists (from first principles)

Imagine you didn't have iterators. To "sum the squares of the even numbers" you'd write:

```rust
let mut total = 0;
for &x in &nums {
    if x % 2 == 0 {
        total += x * x;
    }
}
```

This works, but it **fuses three independent ideas** — selecting, transforming,
accumulating — into one tangled loop with a mutable accumulator. You can't reuse the
"keep evens" step, you can't swap the accumulation, and the intent is buried in
mechanics.

The iterator abstraction separates these concerns into composable pieces:

```rust
fn sum_of_even_squares(nums: &[i32]) -> i32 {
    nums.iter().filter(|&x| x % 2 == 0).map(|x| x * x).sum()
}
```

Each verb does one thing. The catch: if every step eagerly built an intermediate `Vec`,
this would be slower than the hand-written loop and couldn't handle infinite sequences.
So Rust makes adapters **lazy** — they compile down to roughly the same machine code as
the hand-written loop (zero-cost), *and* they compose, *and* they work on endless
streams. That combination is why the abstraction is worth having.

> The compiler is enforcing one core protocol — the `Iterator` trait — and giving you
> ~70 default methods for free the moment you supply `next`.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `filter`/`map`/`sum` chain | replace the manual loop with composable verbs |
| 2 | foundations | `iter` / `iter_mut` / `into_iter` | the same data yields `&T`, `&mut T`, or `T` |
| 3 | mechanics | adapter zoo | `enumerate`, `zip`, `flat_map`, `filter_map`, `fold` |
| 4 | mechanics | laziness, proven | a closure that runs 0 times; an infinite source tamed by `take` |
| 5 | footgun | ownership & `collect` traps | the move trap (E0382), turbofish, `Result` short-circuit |
| 6 | footgun | `impl Iterator` for `Fib` | write `next()` once, inherit every adapter |
| 7 | real-world | `IntoIterator` + `DoubleEndedIterator` | how `for` works; `rev()`; `size_hint` |
| 8 | real-world | custom lazy adapter + extension trait | `.pairs()` on every iterator (the itertools pattern) |
| 9 | capstone | mini iterator engine from scratch | own trait + lazy adapters + a consumer; prove the pull-chain |

## The ideas, built up

### 1. A chain is three verbs, not one loop

```rust
nums.iter().filter(|&x| x % 2 == 0).map(|x| x * x).sum()
```

The subtlety hides in the filter closure. `nums.iter()` yields `&i32`, so `filter`'s
closure receives `&&i32` (filter borrows each item to inspect it without consuming).
The pattern `|&x|` strips one reference layer, so inside the closure `x: &i32`, and
`x % 2` auto-derefs the rest. This `|&x|` destructuring-in-the-binding is *the* idiomatic
way to deal with the double reference — cleaner than writing `**x`.

`sum()` is a **consumer**: it's the verb that finally calls `next()` repeatedly and
folds the results. Without it, nothing runs (see rung 4).

### 2. One collection, three iterators

A `Vec<T>` gives you three entry points, distinguished by the **item type** they yield:

| Call | Item type | Effect on the source |
|------|-----------|----------------------|
| `.iter()` | `&T` | borrows; source survives |
| `.iter_mut()` | `&mut T` | borrows mutably; mutate in place |
| `.into_iter()` | `T` | **consumes**; source is gone afterward |

```rust
fn count_long(words: &[String]) -> usize {
    words.iter().filter(|w| w.len() > 3).count()   // &T: caller keeps `words`
}

fn double_in_place(nums: &mut Vec<i32>) {
    nums.iter_mut().for_each(|n| *n *= 2);          // &mut T: write through the ref
}

fn join_owned(words: Vec<String>) -> String {
    words.into_iter().collect::<Vec<_>>().join(", ") // T: takes ownership, `words` consumed
}
```

The choice is forced by what you need to do: read-only (`iter`), mutate (`iter_mut`), or
take ownership of the values (`into_iter`). `for_each` here is itself a consumer — it's
the iterator-land equivalent of a `for` loop body.

### 3. The adapter zoo

Five workhorses you reach for daily:

```rust
// enumerate yields (index, &value); keep the index where the value is even
nums.iter().enumerate()
    .filter_map(|(i, &x)| if x % 2 == 0 { Some(i) } else { None })
    .collect()

// zip welds two iterators and STOPS at the shorter one
names.iter().zip(scores).map(|(n, s)| format!("{}={}", n, s)).collect()

// flat_map: each item produces an iterator; they're concatenated flat
words.iter().flat_map(|w| w.chars()).collect()

// filter_map: filter + map in one pass; .ok() turns Result -> Option, dropping Errs
strs.iter().filter_map(|s| s.parse().ok()).collect()

// fold: thread an accumulator; the closure must RETURN the (mutated) acc
s.chars().fold(HashMap::new(), |mut acc, c| {
    *acc.entry(c).or_insert(0) += 1;
    acc
})
```

Two facts worth burning in:

- **`zip` stops at the shorter input.** `["a","b","c"].zip([9])` yields just `("a", 9)`.
  No panic, no padding — it's how you safely walk two sequences of unknown relative length.
- **`filter_map` is filter + map fused.** Whenever you find yourself writing
  `.filter(...).map(...)` where the filter and map both inspect the same thing (especially
  `Option`/`Result`), `filter_map` does it in one pass.

### 4. Laziness, proven

This is the conceptual heart. Build a million-element chain but never consume it:

```rust
fn lazy_never_runs(log: &mut Vec<i32>) {
    let _lazy = (0..1_000_000).map(|x| log.push(x));
    // no consumer called -> the closure body runs ZERO times
}
// afterwards: log.len() == 0
```

The `map` closure pushes to `log` *every time it runs* — and it runs **zero** times,
because nobody pulled. The compiler even hints at this: `_lazy` triggers a `must_use` /
unused warning, which is literally "you built an iterator and never drove it."

Laziness is also what makes **infinite** iterators usable:

```rust
fn first_4_triple_squares() -> Vec<u64> {
    (0u64..)                                 // endless
        .filter(|n| n % 3 == 0 && *n != 0)   // note: 0 is divisible by 3 — exclude it
        .map(|n| n * n)
        .take(4)                             // stops the pull after 4 items
        .collect()
}
// -> [9, 36, 81, 144]   (from 3, 6, 9, 12)
```

If any adapter were eager, `(0u64..)` would hang your machine forever. `take(4)` bounds
the pulling. The mental model to lock in:

> **Adapters are nouns (a recipe). Consumers are verbs (they run it).**

### 5. Where iterators bite

**The move trap (E0382).** `into_iter()` *takes ownership* of the receiver:

```rust
// WRONG — does not compile
let total: i32 = v.into_iter().sum();
let n = v.len();   // error[E0382]: borrow of moved value: `v`
```

The fix isn't `.clone()` (the compiler suggests it, but that allocates a whole new Vec).
Either borrow instead of consume, or capture the length first:

```rust
// OK — borrow to sum; `v` is fully intact for .len()
fn sum_then_len(v: Vec<i32>) -> (i32, usize) {
    let total: i32 = v.iter().sum();
    let n = v.len();
    (total, n)
}
```

**`collect` needs a target type.** `collect` is generic over its return type via
`FromIterator`. With nothing telling it *what* to build, inference fails. Pin it with a
binding annotation or a turbofish:

```rust
let v: Vec<i32> = (0..5).map(|x| x * 2).collect();      // annotate the binding
(0..5).map(|x| x * 2).collect::<Vec<i32>>()             // or turbofish
```

When the function's return type already pins it, you need neither.

**`collect` into `Result` short-circuits.** The single most-loved `collect` trick:

```rust
fn parse_all_or_fail(strs: &[&str]) -> Result<Vec<i32>, std::num::ParseIntError> {
    strs.iter().map(|s| s.parse::<i32>()).collect()
}
```

`collect` *transposes* an iterator of `Result<T, E>` into a single `Result<Vec<T>, E>`:
`Ok(vec)` if every element parsed, or the **first** `Err` the moment one fails (and it
stops pulling). That's validate-all-or-bail in one line. (The same works for
`Option`: `Iterator<Item = Option<T>>` collects to `Option<Vec<T>>`.)

### 6. Implement `Iterator` yourself

The payoff rung. The entire trait is one required method; supply it and dozens of adapters
appear for free, because they're default methods riding on `next`:

```rust
struct Fib { curr: u64, next: u64 }

impl Iterator for Fib {
    type Item = u64;
    fn next(&mut self) -> Option<Self::Item> {
        let curr = std::mem::replace(&mut self.curr, self.next);
        self.next = curr + self.next;
        Some(curr)   // infinite: never None — bounding it is the caller's job
    }
}
```

`std::mem::replace(&mut self.curr, self.next)` does two jobs atomically: it returns the
old `curr` (the value to yield) *while* overwriting `self.curr` with `self.next`. That
sidesteps the classic stale-value bug where you overwrite a field before you've finished
reading it.

The architectural lesson: **three lines of `next()` bought you `take`, `filter`, `sum`,
`nth`, `collect`** and the rest:

```rust
Fib::new().take(10).collect::<Vec<_>>();                 // [0,1,1,2,3,5,8,13,21,34]
Fib::new().take(10).filter(|n| n % 2 == 0).sum::<u64>(); // 44
Fib::new().nth(7);                                       // Some(13)
```

### 7. How `for` actually works: `IntoIterator`

`for` is not compiler magic. `for x in thing { ... }` desugars to:

```rust
let mut it = IntoIterator::into_iter(thing);
while let Some(x) = it.next() { ... }
```

So to make your own type loopable, implement `IntoIterator`. Real collections implement
it **three times** so `for x in v`, `for x in &v`, and `for x in &mut v` each pick the
right item type (`T`, `&T`, `&mut T`). The `&v` impl not consuming `v` is exactly what
lets you loop over a collection you still need afterward.

The ladder builds the consuming (`T`) variant by delegating to the standard library's
`vec::IntoIter`:

```rust
struct MyVec<T> { items: Vec<T> }
struct MyVecIntoIter<T> { inner: std::vec::IntoIter<T> }

impl<T> IntoIterator for MyVec<T> {
    type Item = T;
    type IntoIter = MyVecIntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        MyVecIntoIter { inner: self.items.into_iter() }
    }
}

impl<T> Iterator for MyVecIntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> { self.inner.next() }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
```

Two extras that matter for real APIs:

- **`size_hint`** returns `(lower, Option<upper>)`. Consumers like `collect` use it to
  pre-allocate exactly the right capacity. Forwarding it (here `(4, Some(4))` for a
  4-element vec) avoids reallocation churn.
- **`DoubleEndedIterator`** adds `next_back()` — pull from the *other* end. That single
  method is all `rev()` needs:

```rust
impl<T> DoubleEndedIterator for MyVecIntoIter<T> {
    fn next_back(&mut self) -> Option<T> { self.inner.next_back() }
}
// now: coll.into_iter().rev().collect()  works
```

### 8. A custom lazy adapter — the itertools pattern

To add a *new* adapter that works on every iterator, you write two things: a stateful
struct that implements `Iterator`, and a blanket extension trait that hands out the
method. The ladder builds `.pairs()`, which turns `[1,2,3,4]` into overlapping windows
`(1,2), (2,3), (3,4)`:

```rust
struct Pairs<I: Iterator> {
    inner: I,
    prev: Option<I::Item>,
}

impl<I> Iterator for Pairs<I>
where
    I: Iterator,
    I::Item: Clone,   // we keep a copy of prev AND emit it
{
    type Item = (I::Item, I::Item);
    fn next(&mut self) -> Option<Self::Item> {
        if self.prev.is_none() {
            self.prev = self.inner.next();      // seed once, on the first call
        }
        let curr = self.inner.next()?;          // ? bails on exhausted/empty source
        let prev = self.prev.replace(curr.clone())?; // install new prev, hand back old
        Some((prev, curr))
    }
}
```

`self.prev.replace(curr.clone())` is the elegant move: it stores `curr` as the new
remembered value **and** returns the previous one to emit — slide and extract in a single
call. The critical invariant is that each `next()` pulls *at most one* new item from
`inner`; that's what keeps `.pairs()` lazy enough to run on an infinite source.

The extension trait grafts the method onto everything:

```rust
trait IterPairsExt: Iterator + Sized {
    fn pairs(self) -> Pairs<Self> {
        Pairs { inner: self, prev: None }
    }
}
impl<I: Iterator> IterPairsExt for I {}   // blanket impl: every Iterator now has .pairs()
```

This composes like any built-in adapter, including on infinite streams:

```rust
let diffs: Vec<u64> = (0u64..).map(|x| x * x).pairs().map(|(a, b)| b - a).take(4).collect();
// squares 0,1,4,9,16 -> consecutive diffs [1, 3, 5, 7]
```

This is precisely how the `itertools` crate delivers `.tuple_windows()`, `.dedup()`,
`.chunks()`, and friends.

## Footguns

| Trap | What bites | Fix |
|------|-----------|-----|
| `into_iter()` move | `v` is consumed; later `v.len()` is E0382 | use `.iter()` to borrow, or read `len()` first; don't `.clone()` |
| `collect` can't infer | "type annotations needed" | annotate the binding or turbofish `collect::<Vec<_>>()` |
| `zip` length mismatch | silently stops at the shorter side | intended — but know it won't error on ragged inputs |
| infinite source, eager step | hangs forever | bound with `take`/`take_while`; keep every adapter lazy |
| building but never consuming | a `must_use` warning; nothing happens | remember adapters are inert until a consumer pulls |
| `0` in divisibility filters | `0 % n == 0` for all `n` | guard `&& *x != 0` when you mean "positive multiples" |

## Real-world patterns

- **`collect::<Result<_, _>>()`** for "parse/validate everything or fail fast" — ubiquitous
  in config loading, deserialization, and request handling.
- **Returning `impl Iterator<Item = T>`** from functions to expose a lazy stream without
  committing to a concrete type or allocating a `Vec`.
- **Extension traits with blanket impls** (`itertools::Itertools`, `rayon`'s
  `ParallelIterator`) — the standard way third-party crates bolt new methods onto every
  iterator.
- **`size_hint` + `DoubleEndedIterator`** are why `Vec`/slice iteration pre-allocates
  perfectly and supports `rev()`, `rposition`, etc.

## Capstone insight

The build-it-from-scratch rung re-implements the core of `std::iter` with no help from it:
a `MyIterator` trait (one required `next`), default methods `map`/`filter`/`take` that
return lazy adapter structs, a `collect_vec` consumer, and a `Counter` source.

```rust
trait MyIterator: Sized {            // Sized so adapters can take `self` by value
    type Item;
    fn next(&mut self) -> Option<Self::Item>;

    fn map<B, F: FnMut(Self::Item) -> B>(self, f: F) -> MyMap<Self, F> {
        MyMap { iter: self, f }      // just builds a struct — no work yet
    }
    // filter, take similar...

    fn collect_vec(mut self) -> Vec<Self::Item> {   // THE consumer: where pulling happens
        let mut out = Vec::new();
        while let Some(x) = self.next() { out.push(x); }
        out
    }
}
```

Each adapter implements `MyIterator` by pulling from its inner iterator inside its own
`next`:

```rust
impl<I: MyIterator> MyIterator for MyTake<I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        if self.remaining == 0 { None }       // <- the brake that stops an infinite source
        else { self.remaining -= 1; self.iter.next() }
    }
}
```

The **aha**: when `collect_vec` calls `next()` on the outermost `MyTake`, it triggers a
**pull-chain** — `take` asks `filter`, `filter` asks `map`, `map` asks `Counter` — *one
item at a time, on demand*. `filter` may loop internally and skip arbitrarily many source
items before returning one (so `take` never "sees" the skips), and `take`'s counter is the
only thing keeping the infinite `Counter` from running forever. That cascade **is** how
every iterator in Rust works. Note also that calling a closure stored in a struct field
needs parens — `(self.f)(x)` — to disambiguate from a method call.

## Explain it back

- What is the *only* method you must implement for `Iterator`, and why does that give you
  `map`/`filter`/`sum` for free?
- What does `for x in thing` desugar to, exactly? Which trait does it call?
- Why does `(0u64..).filter(...).map(...).take(4).collect()` not hang, but
  `(0u64..).filter(...).collect()` would?
- What's the difference between an adapter and a consumer? Name three of each.
- Why does `let total = v.into_iter().sum(); v.len()` fail to compile, and what are two
  fixes that don't clone?
- How does `collect::<Result<Vec<_>, _>>()` decide between `Ok` and `Err`, and when does
  it stop pulling?
- In the `.pairs()` adapter, what guarantees laziness — i.e., why does each `next()` pull
  at most one new item?
- What single method unlocks `rev()`, and what does `size_hint` buy a consumer?

## See also

- [Associated types vs generic params](assoc-vs-generic.md) — why `Iterator::Item` is an
  associated type, not a generic parameter.
- [Blanket impls & coherence](blanket-coherence.md) — the mechanism behind the `.pairs()`
  extension trait.
- [Collections deep-dive](collections.md) — what you're usually iterating over.
- [Static vs dynamic dispatch](dispatch.md) — monomorphized iterator chains vs
  `Box<dyn Iterator>`.
