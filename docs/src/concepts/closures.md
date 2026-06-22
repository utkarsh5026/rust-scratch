# Closures & `Fn` / `FnMut` / `FnOnce`

> Ladder: [`src/bin/closures.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/closures.rs) ·
> Run: `cargo run --bin closures` · Phase 2 · 9 rungs

## TL;DR

A closure is **an anonymous struct the compiler generates for you**. Its fields are
the variables it captures from the surrounding scope. *How* it captures them — by
shared reference, by mutable reference, or by value — decides which of three traits
it implements:

| Trait | Receiver | Meaning | Callable |
|-------|----------|---------|----------|
| `Fn` | `&self` | only **reads** captures | many times, shareably |
| `FnMut` | `&mut self` | **mutates** captures | many times, exclusively |
| `FnOnce` | `self` | **consumes** captures | exactly once |

These nest: `Fn ⊂ FnMut ⊂ FnOnce`. Anything that's `Fn` is automatically `FnMut`
and `FnOnce` too. Once you internalize "closure = struct + a call method whose
`self`-type is the trait", every confusing closure error becomes decodable.

## Why this exists (from first principles)

A plain function can't remember anything between the place it's defined and the
place it's called — it only has its arguments. But constantly we want a "function
plus some context": *multiply by this `factor`*, *push into this `log`*, *validate
against this `min..=max`*. That context has to live *somewhere*.

The closure's answer: bundle the captured context into a hidden struct, and attach a
call method to it. So this:

```rust
let factor = 3;
let times = |x| x * factor;
```

is conceptually compiled to:

```rust
struct __Times { factor: i32 }      // captured env becomes fields
impl __Times {
    fn call(&self, x: i32) -> i32 { x * self.factor }
}
let times = __Times { factor: 3 };
```

Now the *only* remaining question is what kind of access the call method needs to
its captured fields — and that is exactly what `Fn`/`FnMut`/`FnOnce` encode. The
compiler is enforcing the same borrow rules it always does, just on hidden fields:

- If the body only *reads* a capture, `&self` suffices → `Fn`.
- If the body *writes* a capture, it needs `&mut self` → `FnMut`.
- If the body *moves a capture out* (consumes it), it needs `self` by value → `FnOnce`.

Everything in this concept is a consequence of that one design choice.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|-----------|
| 1 | foundations | capture env by `&` | a closure reads outer variables without being passed them |
| 2 | foundations | three capture modes | `&` / `&mut` / `move` — the compiler picks the least invasive |
| 3 | mechanics | the trait hierarchy | `Fn ⊂ FnMut ⊂ FnOnce`; strictest vs loosest bound |
| 4 | mechanics | desugar by hand | build the struct + `call`/`call_mut` the compiler generates |
| 5 | footgun | once & `mut` | `FnOnce` is callable once (E0382); `FnMut` needs a `mut` binding (E0596) |
| 6 | footgun | returning closures | `impl Fn` (one type) vs `Box<dyn Fn>` (branchy) |
| 7 | real-world | fn pointers | fn items & non-capturing closures coerce to `fn`; capturing ones can't |
| 8 | real-world | stdlib + factories | which Fn trait each adapter wants; closures that build closures |
| 9 | capstone | event dispatcher | `Vec<Box<dyn FnMut(&Event)>>` registry with `subscribe`/`emit` |

## The ideas, built up

### 1. Capture: a closure reaches into its environment

A closure can use a variable that is *neither* a parameter *nor* a local — it reads
it straight out of the enclosing scope:

```rust
fn make_and_use(factor: i32, nums: &[i32]) -> Vec<i32> {
    nums.iter().map(|x| x * factor).collect()
    //              ^^^^^^^^^^^^^^ closure captures `factor`; never passed in
}
```

`factor` is captured. `x` is a parameter. That distinction is the whole point: the
captured value becomes hidden state, the parameter stays an argument. (Side note:
`x` here is `&i32`, yet `x * factor` compiles because `std` implements `Mul` for
reference operands and auto-derefs.)

### 2. The three capture modes

The compiler captures **as weakly as the body allows**. It tries `&` first, then
`&mut`, then by-value — picking the first that makes the body type-check. You can
force by-value with the `move` keyword.

```rust
// (a) read-only  -> captured by &     -> the closure is `Fn`
fn borrow_capture(data: &Vec<i32>) -> i32 {
    data.iter().sum()            // only reads `data`; caller can still use it after
}

// (b) mutating   -> captured by &mut  -> the closure is `FnMut`
fn mut_capture() -> Vec<String> {
    let mut log: Vec<String> = Vec::new();
    let mut record = |s: String| log.push(s);   // note: the *binding* is `mut`
    record("event 0".to_string());
    record("event 1".to_string());
    record("event 2".to_string());
    log                          // borrow of `log` has ended; we can return it
}

// (c) move       -> captured by value -> owns the data; can outlive the scope
fn move_capture(owned: String) -> Box<dyn Fn() -> usize> {
    Box::new(move || owned.len())
}
```

Two subtleties this rung surfaces:

- In `(b)`, the closure holds a `&mut log`. You **cannot read `log` while that
  borrow is live** — you must finish all `record(...)` calls first, *then* return
  `log`. The mutable borrow is released when the closure is last used.
- In `(c)`, `move` forces `owned` into the closure by value, which is what lets the
  returned closure outlive `move_capture`'s stack frame. Note its bound is `Fn`, not
  `FnOnce`: reading `.len()` doesn't consume the `String`, so it's re-callable.

> **`move` changes *how* captures are taken, not *which* trait results.** A `move`
> closure that only reads its captures is still `Fn`. `move` answers "by value?",
> the body answers "read, write, or consume?".

### 3. The trait hierarchy: strictest vs loosest bound

The three traits form a subtrait chain:

```text
Fn : FnMut : FnOnce
```

Read it as: every `Fn` *is a* `FnMut`, every `FnMut` *is a* `FnOnce`. Three generic
helpers make the consequence concrete:

```rust
fn apply_fn<F: Fn() -> i32>(f: F) -> i32       { f() + f() }   // call via &self
fn apply_mut<F: FnMut() -> i32>(mut f: F) -> i32 { f() + f() } // call via &mut self
fn apply_once<F: FnOnce() -> i32>(f: F) -> i32 { f() }         // call via self, once
```

A pure read-only closure satisfies **all three** bounds:

```rust
let read = || 7;
apply_fn(read);   // ok
apply_mut(read);  // ok — Fn is also FnMut
apply_once(read); // ok — Fn is also FnOnce
```

But a closure that mutates a capture is *only* `FnMut` (and `FnOnce`), never `Fn`:

```rust
let mut n = 0;
let counter = move || { n += 1; n };
apply_mut(counter);          // ok
// apply_fn(counter);        // E0525: expected a closure implementing `Fn`, found `FnMut`
```

And one that consumes a capture is *only* `FnOnce`:

```rust
let s = String::from("rust");
let consume = move || s.len() as i32; // here just reads, but if it moved `s` out...
apply_once(consume);                  // ...only `apply_once` would accept it
```

This is the key API-design lesson:

> **`F: Fn` is the *strictest* bound (fewest closures qualify), `F: FnOnce` is the
> *loosest* (most closures qualify).** Demand the *least* power you actually need:
> if you call the closure once, take `FnOnce`; if you call it repeatedly without
> mutation, you can afford to demand `Fn`. The looser the bound, the more callers
> can hand you a closure.

### 4. Desugar by hand — the "aha" rung

The mental model stops being a metaphor when you build the struct yourself. The
real `Fn` traits are unstable to *implement* directly on stable Rust, so the ladder
mirrors them with inherent methods carrying the **same `self`-type**:

```rust
// `move |x: i32| x + offset`  desugars to:
struct AddOffset { offset: i32 }          // one captured field
impl AddOffset {
    fn call(&self, x: i32) -> i32 { x + self.offset }   // &self  -> mirrors Fn
}

// `move || { count += step; count }`  desugars to:
struct Counter { count: i32, step: i32 }  // two captured fields
impl Counter {
    fn call_mut(&mut self) -> i32 {        // &mut self -> mirrors FnMut
        self.count += self.step;
        self.count
    }
}
```

The check runs each hand-built struct *next to* the equivalent real closure and
asserts identical output:

```rust
let offset = 100;
let hand = AddOffset { offset };
let real = move |x: i32| x + offset;
for x in [-5, 0, 7, 42] {
    assert_eq!(hand.call(x), real(x));     // identical
}
```

The payoff: `AddOffset.call(x)` and `|x| x + offset` are *the same thing*. And the
receiver type on the call method **is** the trait — `&self` ⇒ `Fn`, `&mut self` ⇒
`FnMut`, `self` ⇒ `FnOnce`. Memorize this and you never have to guess a closure's
trait again: ask "what does the body do to its captures, and what `self` would the
hidden call method need?"

## Footguns

### `FnOnce` is callable exactly once (E0382)

A closure that **moves a captured value out of itself** can only run once — the
second call would touch a value that's already been moved away:

```rust
fn unwrap_factory(s: String) -> impl FnOnce() -> String {
    move || s            // hands `s` back by value -> consumes the capture
}

let f = unwrap_factory(String::from("payload"));
assert_eq!(f(), "payload");
// let again = f();      // E0382: use of moved value `f` — the call consumed it
```

`impl FnOnce` is the *only* valid return bound here. Try widening it to `impl Fn`
and the compiler refuses, because returning `s` by value can't be done through
`&self`.

### `FnMut` needs a `mut` binding (E0596)

Calling an `FnMut` goes through `&mut self`, so the variable (or parameter) holding
the closure must be mutable:

```rust
fn run_n_times<F: FnMut() -> i32>(mut f: F, n: usize) -> Vec<i32> {
    //                            ^^^ delete this and you get E0596:
    //                                "cannot borrow `f` as mutable"
    let mut results = Vec::new();
    for _ in 0..n { results.push(f()); }
    results
}
```

This is the same rule as rung 2's `let mut record = ...`. A `FnMut` call mutates
hidden fields, so it needs mutable access to the closure value.

### Every closure has a unique, unnameable type

Two closures with identical bodies are still *different types*. You literally cannot
write the type out — so `fn foo() -> { the closure type }` is impossible. That forces
the return-position decision in rung 6.

## Real-world patterns

### Returning closures: `impl Fn` vs `Box<dyn Fn>`

```rust
// ONE concrete hidden type -> impl Trait. Static dispatch, zero allocation.
fn make_adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| x + n
}

// Different branches build DIFFERENT closure types -> must erase behind a vtable.
fn pick_op(op: char) -> Box<dyn Fn(i32, i32) -> i32> {
    match op {
        '+' => Box::new(|a, b| a + b),
        '-' => Box::new(|a, b| a - b),
        '*' => Box::new(|a, b| a * b),
        _   => Box::new(|_, _| 0),
    }
}
```

`impl Fn` means "I return exactly one hidden type, you just don't get to name it."
The moment your `match` arms return *distinct* closure types, no single `impl Trait`
type can cover them — you box them into `Box<dyn Fn>`, a heap-allocated trait object
with dynamic dispatch. Trying `-> impl Fn` on `pick_op` yields *"if and else have
incompatible types"*.

### fn pointers vs closures

`fn(i32) -> i32` is the **function pointer** type: one pointer-sized value aimed at
code, with **no captured environment**. Two things coerce to it:

```rust
fn triple(x: i32) -> i32 { x * 3 }

fn transform_all(xs: &[i32], f: fn(i32) -> i32) -> Vec<i32> {
    xs.iter().map(|x| f(*x)).collect()
}

transform_all(&[1, 2, 3], triple);          // a function ITEM coerces
transform_all(&[1, 2, 3], |x| x + 100);     // a NON-capturing closure coerces

// fn pointers are Copy + Sized -> store them inline, no Box, no vtable:
let ops: Vec<fn(i32) -> i32> = vec![triple, |x| x + 1, |x| x * x];

// But a CAPTURING closure does NOT coerce:
let k = 10;
// transform_all(&[1], |x| x + k);  // E0308: expected fn pointer, found closure
```

The dividing line: the instant a closure captures anything, it becomes a distinct
type carrying data and is no longer a bare `fn`. Note `Vec<fn(..)>` (rung 7) needs no
`Box`, unlike `Vec<Box<dyn Fn>>` (rung 6) — because all fn pointers share one
`Copy`, `Sized` type, whereas capturing closures don't.

### The stdlib demands the loosest bound it can

```rust
nums.iter().filter(...).map(...)   // map / filter / fold take FnMut
rows.sort_by_key(|&(_, n)| n);     // sort_by_key takes FnMut
opt.unwrap_or_else(|| default());  // unwrap_or_else takes FnOnce (runs at most once)
```

Each method asks for exactly the power it uses. And the *closure factory* is the
everyday production pattern — a function that builds and returns a closure capturing
its arguments:

```rust
fn make_validator(min: i32, max: i32) -> impl Fn(i32) -> bool {
    move |x| x >= min && x <= max
}

let valid = make_validator(1, 10);
let kept: Vec<i32> = (0..15).filter(|&n| valid(n)).collect(); // reused many times
```

`make_validator` returns an `Fn` (it only reads `min`/`max`), so the resulting
closure is freely re-callable inside `filter`.

## Capstone insight

The event dispatcher fuses every thread of the ladder into one small machine:

```rust
struct Dispatcher {
    subscribers: Vec<Box<dyn FnMut(&Event)>>,
}

impl Dispatcher {
    fn new() -> Self { Self { subscribers: Vec::new() } }

    fn subscribe<F: FnMut(&Event) + 'static>(&mut self, f: F) {
        self.subscribers.push(Box::new(f));     // generic F -> erased trait object
    }

    fn emit(&mut self, event: &Event) {
        for subscriber in self.subscribers.iter_mut() {
            subscriber(event);                  // &mut access -> FnMut call
        }
    }
}
```

Every type choice here is forced by the ladder:

- **`Box<dyn FnMut(&Event)>`** — subscribers are closures of *different* types with
  *different* captures, so they can't share a generic `F`; they must be type-erased
  behind a trait object (rung 6).
- **`FnMut`, not `Fn`** — a real handler keeps internal mutable state (a counter, a
  buffer). Choosing `FnMut` lets handlers mutate their captures; choosing `Fn` would
  forbid the most useful handlers (rung 3's "demand the least power you need", here
  meaning the least that still allows mutation).
- **`emit(&mut self)` + `iter_mut()`** — calling an `FnMut` needs `&mut self`, which
  needs `&mut` access to each boxed handler (rung 5's E0596 in its natural habitat).
- **`F: FnMut(&Event) + 'static`** — `subscribe` accepts *any* matching closure
  (generic), and `'static` guarantees the boxed handler can outlive the call.

The handlers in the test capture an `Rc<RefCell<...>>` purely so the test can peek
at what they did — the dispatcher itself owns the closures outright. Handler A
mutates a captured `seen` counter, which is precisely what makes the whole registry
have to be `FnMut` rather than `Fn`.

## Explain it back

Future-you should be able to answer these cold:

1. What hidden data structure is a closure, and what are its fields?
2. Given a closure body, how do you predict whether it's `Fn`, `FnMut`, or `FnOnce`?
   (Hint: what `self`-type would the generated call method need?)
3. Why is `F: FnOnce` the *loosest* bound and `F: Fn` the *strictest*? Which should
   a callback API ask for, and why?
4. What does `move` change — and what does it *not* change about a closure's trait?
5. When must a returned closure be `Box<dyn Fn>` instead of `impl Fn`?
6. Why does a non-capturing closure coerce to `fn(..)` but a capturing one doesn't?
7. In the dispatcher, why must `subscribers` be `Vec<Box<dyn FnMut(_)>>` and `emit`
   take `&mut self`?

## See also

- [Static vs dynamic dispatch](dispatch.md) — `impl Trait` vs `Box<dyn Trait>`, the
  same monomorphization-vs-vtable trade-off the closure-return rung hits.
- [`Rc<RefCell<T>>` patterns](rc-refcell.md) — the shared mutable state the capstone's
  handlers use for observation.
- [HRTB — for<'a>](hrtb.md) — closures over references (`Fn(&T)`) and why their
  bounds need higher-ranked lifetimes.
- [Iterators end-to-end](iterators.md) — where `map`/`filter`/`fold` consume the
  closures built here.
