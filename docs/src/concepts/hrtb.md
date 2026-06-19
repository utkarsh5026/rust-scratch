# HRTB — `for<'a>`

> Ladder: [`src/bin/hrtb.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/hrtb.rs) ·
> Run: `cargo run --bin hrtb` · Phase 1 · 9 rungs

## TL;DR

A higher-ranked trait bound moves the **quantifier** on a lifetime. The two bounds
look almost identical but mean opposite things:

```rust
fn f<'a, F: Fn(&'a str)>(g: F)      // the CALLER picks one 'a; bound holds for THAT one
fn f<F: for<'a> Fn(&'a str)>(g: F)  // bound holds for EVERY 'a; the CALLEE picks fresh per call
```

`for<'a>` reads literally as *"for all lifetimes `'a`"*. You need it whenever a
value — almost always a closure or a trait impl — must work on a borrow whose
lifetime **doesn't exist yet** at the point you write the bound: a reference you'll
create *inside* your function and hand off, where the caller could never name its
lifetime.

You have used HRTB for years without seeing it: `Fn(&str) -> &str` already
desugars to `for<'a> Fn(&'a str) -> &'a str`. Elision writes the quantifier for
you almost everywhere. This ladder is about the handful of places where it can't,
and where reading the explicit form is the only way to understand the error.

## Why this exists (from first principles)

Start from the lifetime contract. A normal generic lifetime is chosen **by the
caller, at the call site**:

```rust
fn longest<'a>(a: &'a str, b: &'a str) -> &'a str { /* ... */ }
```

When some code calls `longest(&x, &y)`, the compiler picks `'a` to be the overlap
of those two specific borrows. `'a` is one concrete region, fixed once, from the
outside.

Now flip the direction. Suppose *you* are the one who will create the reference,
deep inside your own function, and you want to accept a callback that operates on
it:

```rust
fn run_on_local<F>(f: F) -> usize
where
    F: Fn(&str) -> &str,   // what lifetime is this &str?
{
    let s = String::from("hello world");   // born HERE, inside the function
    f(&s).len()                            // f operates on a borrow the caller can't see
}
```

The borrow `&s` has a lifetime that lasts only until the closing brace. The caller
of `run_on_local` has no way to name it — it doesn't exist at the call site. So a
caller-chosen `<'a>` is fundamentally the wrong tool: there is no single `'a` the
caller could supply that would cover a string born after they called you.

The fix is to demand that `f` works for **all** lifetimes, so it certainly works
for the private one you'll mint internally. That demand is `for<'a>`:

```rust
where F: for<'a> Fn(&'a str) -> &'a str
```

> The mental model in one line: a plain `<'a>` is a lifetime the **caller** fills
> in; `for<'a>` is a lifetime the **callee** fills in, freshly, every time it uses
> the value.

This is not an exotic corner. It is why `Fn` traits are defined with it implicitly,
why `serde`'s `DeserializeOwned` exists, and why every parser-combinator library in
Rust can compose at all. The bound is the load-bearing wall; you just rarely see it
because elision plasters over it.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `apply_to_each` | `Fn(&str)` already *is* `for<'a> Fn(&'a str)` — feed it borrows of many lifetimes. |
| 2 | foundations | `apply_to_each_explicit` | Spell the quantifier out; the elided and explicit forms are identical. |
| 3 | mechanics | `measure_on_local` | Return a borrow of the closure's arg; the caller can't name that lifetime. |
| 4 | mechanics | `Slicer<'a>` / `run_slicer` | HRTB works on *your own* lifetime-generic trait, not just `Fn`. |
| 5 | footgun | `apply_str` | "implementation of `Fn` is not general enough": `let`-bound closures get one lifetime. |
| 6 | footgun | `sum_two_locals` | The named-lifetime trap: one `<'a>` is fixed by the caller and shared by every call. |
| 7 | real-world | `DecodeOwned` | `DeserializeOwned: for<'de> Deserialize<'de>` — owners qualify, borrowers don't. |
| 8 | real-world | `StrPipeline` | `Box<dyn for<'a> Fn(&'a str) -> &'a str>` keeps the trait object lifetime-free. |
| 9 | capstone | `Parser<T>` | Parser combinators stand entirely on `for<'i> Fn(&'i str) -> Option<(&'i str, T)>`. |

## The ideas, built up

### 1. The quantifier is already there (`apply_to_each`)

The first surprise is that you have been writing HRTB all along. This bound has no
named lifetime at all:

```rust
fn apply_to_each<F>(items: &[String], f: F)
where
    F: Fn(&str),          // implicitly: for<'a> Fn(&'a str)
{
    for item in items {
        f(item);          // each &str lives only for this iteration
    }
}
```

Inside the loop, each `item` is borrowed for the span of one iteration. The closure
must accept a `&str` of *whatever* lifetime each iteration produces — and it does,
because `Fn(&str)` secretly means `for<'a> Fn(&'a str)`: "works for every input
lifetime." The check feeds it borrows that only live one loop turn, and a closure
capturing a `RefCell` records their lengths. Nothing forces you to think about
lifetimes here precisely because the quantifier was inserted for you.

### 2. Spell it out (`apply_to_each_explicit`)

`for<'a>` is a real slot in the grammar — it sits immediately before the trait name
and introduces a lifetime scoped to that one bound:

```rust
fn apply_to_each_explicit<F>(items: &[String], f: F)
where
    F: for<'a> Fn(&'a str),   // identical to Fn(&str) above
{ /* same body */ }
```

The `'a` here is **not** a generic parameter of the function — notice it does not
appear in `<F>`. It is bound *by the trait bound itself*. That scoping is the whole
point: it is a lifetime the function body gets to instantiate, not one the caller
supplies. The elided and explicit forms compile to exactly the same thing.

### 3. Returning a borrow forces the issue (`measure_on_local`)

Rung 1 took a borrow; this one **returns** one, which is where the caller-vs-callee
distinction becomes load-bearing:

```rust
fn measure_on_local<F>(f: F) -> usize
where
    F: for<'a> Fn(&'a str) -> &'a str,   // same 'a in and out: output welded to input
{
    let s = String::from("hello world");
    let result = f(&s);   // &s lives only inside this function
    result.len()
}
```

Two things to read carefully:

- The `-> &'a str` reuses the **same** `'a` as the input. That is what makes
  returning a borrow sound: the output is allowed to borrow the input and nothing
  else, so it can't outlive it.
- `s` is born and dies inside `measure_on_local`. The lifetime of `&s` is private
  to this call. The caller cannot name it. So the bound *must* be higher-ranked —
  `f` has to promise it works for every lifetime, including this internal one.

The closures in the check pass **inline**, which matters (see rung 5):

```rust
let n = measure_on_local(|s: &str| s.split(' ').next().unwrap_or(""));  // first word
assert_eq!(n, 5);                                                        // "hello"
assert_eq!(measure_on_local(|s: &str| s), 11);                           // identity -> "hello world"
```

### 4. HRTB on your own trait (`Slicer<'a>`)

`for<'a>` is not special to `Fn`. It applies to **any** trait with a lifetime
parameter:

```rust
trait Slicer<'a> {
    fn slice(&self, input: &'a str) -> &'a str;
}

struct FirstWord;
impl<'a> Slicer<'a> for FirstWord {
    fn slice(&self, input: &'a str) -> &'a str {
        input.split(' ').next().unwrap_or("")
    }
}
```

The key reframe: **`Slicer<'a>` is not one trait, it's a family of traits — one per
lifetime.** Writing `impl<'a> Slicer<'a> for FirstWord` implements every member of
that family in a single stroke. And the bound that asks for the whole family is
exactly `for<'a> Slicer<'a>`:

```rust
fn run_slicer<S>(s: S) -> usize
where
    S: for<'a> Slicer<'a>,   // S implements Slicer<'a> for ALL 'a
{
    let word = String::from("green eggs");
    s.slice(&word).len()     // &word is local -> needs the "for all 'a" guarantee
}
```

The ladder scaffolds this with a deliberately wrong placeholder bound
(`S: Slicer<'static>`) that compiles only while the body is `todo!()`. The moment
you write `s.slice(&word)` on a local, the compiler rejects `'static` and forces
you to generalize to `for<'a> Slicer<'a>`. The error *is* the lesson.

### 5. "implementation of `Fn` is not general enough" (`apply_str`)

This is the single most-cursed HRTB error, and it has a precise cause. In rung 3
the closures worked because they were passed **inline**. Factor a
reference-returning closure into a `let` binding and it breaks:

```rust
fn apply_str<F>(f: F) -> usize
where
    F: for<'a> Fn(&'a str) -> &'a str,
{
    let s = String::from("scaffold");
    f(&s).len()
}

// WRONG: "implementation of `Fn` is not general enough"
let bad = |s: &str| s;
apply_str(bad);
```

Why does the identical closure fail when named? When a closure is bound to a `let`
without a guiding context, type inference picks **one concrete lifetime** for its
signature. A closure inferred as `Fn(&'0 str) -> &'0 str` for some specific `'0`
does **not** satisfy `for<'a>` — it is general over one lifetime, not all of them.
When passed inline, the expected higher-ranked type propagates into inference and
the closure is inferred higher-ranked from the start.

The fixes, and *why* they work:

```rust
// OK (i): fn-pointer coercion — fn pointers are inherently for<'a>
let good: fn(&str) -> &str = |s| s;
apply_str(good);

// OK (ii): a real fn item — fn items are inherently for<'a> too
fn id(s: &str) -> &str { s }
let good = id;
apply_str(good);
```

> The rule to remember: **only closures get a single inferred lifetime that can
> break HRTB.** Function pointers (`fn(&str) -> &str`) and named `fn` items are
> always higher-ranked. Passing a closure inline usually also works, because the
> expected type guides inference.

### 6. The named-lifetime trap (`sum_two_locals`)

Rung 5 was a closure that wasn't general enough. This is the dual: a **bound** you
wrote that isn't general enough, because you reached for a single named lifetime
where you needed a higher-ranked one.

```rust
// WRONG: one named 'a, chosen by the caller, shared across every use of f
fn sum_two_locals<'a, F>(f: F) -> usize
where
    F: Fn(&'a str) -> &'a str,
{
    let s1 = String::from("ab");
    let s2 = String::from("cdef");
    f(&s1).len() + f(&s2).len()   // ERROR: borrowed value does not live long enough
}
```

The `'a` in `<'a, F>` is a free parameter chosen by the caller, so it must outlive
the entire function body. But `s1` and `s2` are locals that die inside it. One
fixed `'a` cannot cover either — let alone two borrows in different inner scopes.

The fix is to make each call mint its own lifetime:

```rust
fn sum_two_locals<F>(f: F) -> usize
where
    F: for<'a> Fn(&'a str) -> &'a str,   // OK: callee picks a fresh, short 'a per call
{
    let s1 = String::from("ab");
    let s2 = String::from("cdef");
    f(&s1).len() + f(&s2).len()          // each call gets its own 'a
}
```

The distinction this rung adds over rung 3: a single `<'a>` isn't merely
"caller-chosen", it is **one** lifetime *shared by every call* to `f`. `for<'a>`
gives each call site its own. Two locals in two scopes makes that concrete — no
single `'a` fits both, but "for all `'a`" fits each.

### 7. `DecodeOwned = for<'de> Decode<'de>` (the serde pattern)

Now the payoff: a higher-ranked bound doing real work in the most-used crate in the
ecosystem. `serde` has two traits:

```rust
pub trait Deserialize<'de> { /* may BORROW from the input (zero-copy) */ }

pub trait DeserializeOwned: for<'de> Deserialize<'de> {}
impl<T> DeserializeOwned for T where T: for<'de> Deserialize<'de> {}
```

`DeserializeOwned` is *defined* as "can be deserialized from input of any lifetime."
The ladder builds the miniature, where `Decode<'a>` plays the role of
`Deserialize<'de>`:

```rust
trait Decode<'a>: Sized {
    fn decode(input: &'a str) -> Option<Self>;
}

trait DecodeOwned: for<'a> Decode<'a> {}
impl<T> DecodeOwned for T where T: for<'a> Decode<'a> {}
```

The two contrasting impls are the whole point:

```rust
// BORROWS from input -> Decode<'a> only for the ONE matching 'a
struct Borrowed<'a> { first: &'a str }
impl<'a> Decode<'a> for Borrowed<'a> {
    fn decode(input: &'a str) -> Option<Self> {
        input.split(',').next().map(|first| Borrowed { first })
    }
}

// OWNS its data -> Decode<'a> for EVERY 'a
struct Owned { sum: u32 }
impl<'a> Decode<'a> for Owned {
    fn decode(input: &'a str) -> Option<Self> { /* parse + sum the csv */ }
}
```

`Borrowed<'a>` ties `Self` to the input lifetime, so it implements `Decode<'a>` for
exactly one `'a` — it is **not** `for<'a> Decode<'a>`, therefore **not**
`DecodeOwned`. `Owned` keeps a `u32` that borrows nothing, so it implements
`Decode<'a>` for all `'a` and **is** `DecodeOwned`.

That distinction is enforced the moment you try to load from data you own
internally:

```rust
fn load<T: DecodeOwned>(source: String) -> Option<T> {
    T::decode(&source)   // &source is local -> only a DecodeOwned T can be loaded this way
}

let got: Owned = load("1,2,3,4".to_string()).unwrap();   // OK: sum == 10
// let _: Borrowed = load("a,b".to_string()).unwrap();   // ERROR: Borrowed: DecodeOwned not satisfied
```

This is precisely why `serde_json::from_reader` requires `DeserializeOwned` and
won't deserialize a struct holding `&str`: the bytes are owned by the reader and
dropped when it returns, so anything that *borrows* them would dangle. The `for<'a>`
bound is what mechanically excludes the borrowing types.

### 8. HRTB inside a trait object (`StrPipeline`)

Up to here the higher-ranked thing was a *generic* parameter `F`. You can also erase
it behind `dyn`. Putting `for<'a>` **inside** the box is what lets the surrounding
type carry no lifetime parameter:

```rust
struct StrPipeline {
    steps: Vec<Box<dyn for<'a> Fn(&'a str) -> &'a str>>,
}
```

Because each boxed step is higher-ranked, one `StrPipeline` value can be applied to
inputs of *any* lifetime — the struct itself stays lifetime-free:

```rust
impl StrPipeline {
    fn add<F>(mut self, f: F) -> Self
    where
        F: for<'a> Fn(&'a str) -> &'a str + 'static,
    {
        self.steps.push(Box::new(f));
        self
    }

    fn run<'a>(&self, input: &'a str) -> &'a str {
        let mut cur = input;
        for step in &self.steps {
            cur = step(cur);   // &'a str in, &'a str out — same 'a, every iteration
        }
        cur
    }
}
```

Watch the loop body type-check: `cur` is `&'a str`; each `step` is
`for<'a> Fn(&'a str) -> &'a str`, so `step(cur)` returns `&'a str` — the same `'a` —
and the reassignment holds across every iteration.

The contrast that explains the design: if the box were
`Box<dyn Fn(&'x str) -> &'x str>` for some fixed `'x`, the struct would need an
`<'x>` parameter and could only ever process borrows of that one lifetime. HRTB is
what keeps `StrPipeline` a plain, storable, lifetime-free type while one value still
serves a `'static` string literal and a short-lived local alike.

### 9. Capstone: a parser combinator (`Parser<T>`)

A parser is a function: given input, either fail, or return *(remaining input,
value)*. The remaining slice is a sub-borrow of the input, so a parser is
fundamentally:

```rust
for<'i> Fn(&'i str) -> Option<(&'i str, T)>
```

HRTB is the load-bearing wall of every parser-combinator library — nom, winnow,
chumsky. It's what lets `Parser<T>` be a lifetime-free type you can store, pass, and
compose, while each parser still runs on input of any lifetime, and one parser's
leftover slice feeds straight into the next:

```rust
struct Parser<T>(Box<dyn for<'i> Fn(&'i str) -> Option<(&'i str, T)>>);

impl<T: 'static> Parser<T> {
    fn new(f: impl for<'i> Fn(&'i str) -> Option<(&'i str, T)> + 'static) -> Self {
        Parser(Box::new(f))
    }
    fn parse<'i>(&self, input: &'i str) -> Option<(&'i str, T)> {
        (self.0)(input)
    }
}
```

Notice `Parser<T>` has **no lifetime parameter** — the `for<'i>` lives inside the
box, exactly as in rung 8. The base parser `tag` matches a literal prefix and yields
it; its value type is `&'static str`, which never borrows from the input lifetime:

```rust
fn tag(prefix: &'static str) -> Parser<&'static str> {
    Parser::new(move |input: &str| input.strip_prefix(prefix).map(|rest| (rest, prefix)))
}
```

The combinators build on it. `number` parses a leading digit run; `map` transforms a
parser's output; and `then` — the heart of the rung — runs one parser and feeds its
leftover into the next:

```rust
fn then<A: 'static, B: 'static>(a: Parser<A>, b: Parser<B>) -> Parser<(A, B)> {
    Parser::new(move |input| {
        a.parse(input)
            .and_then(|(rest, a)| b.parse(rest).map(|(rest, b)| (rest, (a, b))))
    })
}
```

The composition only type-checks because of `for<'i>`. `a.parse(input)` hands back
`rest: &'i str` — a sub-slice of `input`, lifetime `'i`. You then call
`b.parse(rest)`, and only because `b` is `for<'i>` can it accept that leftover slice
of the very same `'i`. A single-lifetime parser type could not chain to arbitrary
depth without threading an explicit lifetime through every combinator. HRTB makes
the lifetime *disappear from the type* while staying *correct in the body*:

```rust
let assignment = then(tag("x="), number());
let (rest, (key, value)) = assignment.parse("x=42;").unwrap();
assert_eq!((key, value, rest), ("x=", 42, ";"));

let incremented = map(then(tag("n:"), number()), |(_, n)| n + 1);
assert_eq!(incremented.parse("n:99").unwrap().1, 100);
```

## Footguns

- **Thinking `for<'a>` is just a fancy `<'a>`.** It is the *opposite* quantifier.
  `<'a>` = caller picks one; `for<'a>` = holds for all, callee picks per call.
- **`let`-bound reference-returning closures.** They infer one concrete lifetime and
  fail `for<'a>` with "implementation of `Fn` is not general enough." Pass inline,
  coerce to a `fn` pointer, or use a named `fn` item.
- **Reaching for a named `<'a>` to accept a callback over locals.** The `'a` becomes
  caller-fixed and must outlive the whole function; your locals can't satisfy it.
  Use `for<'a>` so each call mints its own.
- **Expecting a borrowing type to be `DeserializeOwned`/`DecodeOwned`.** A type whose
  `Self` borrows the input implements the trait for only one lifetime, so the
  `for<'de>` supertrait excludes it. Own your data, or thread the input lifetime.
- **Adding a lifetime parameter to a struct that stores a callback.** If the boxed
  `Fn` is higher-ranked (`Box<dyn for<'a> Fn(&'a str) -> &'a str>`), the struct
  needs no lifetime at all. Only a *fixed* inner lifetime forces one onto the struct.

## Real-world patterns

- **`Fn`/`FnMut`/`FnOnce` over references** are all implicitly higher-ranked.
  `Fn(&T) -> &U` is `for<'a> Fn(&'a T) -> &'a U`; you only type the quantifier when
  elision can't infer it.
- **`serde::de::DeserializeOwned`** is the canonical HRTB supertrait
  (`for<'de> Deserialize<'de>`) — and the reason `from_reader`/`from_slice`-into-owned
  reject borrowing types.
- **Parser combinators** (`nom`, `winnow`, `chumsky`) are built bottom-to-top on
  `for<'i> Fn(&'i str) -> ...`, keeping their `Parser` types lifetime-free.
- **Closure-accepting APIs that borrow internal state** — iterator adapters,
  visitor/callback registries, middleware chains — lean on the implicit `for<'a>` so
  one callback can be invoked on transient internal borrows.

## Capstone insight

HRTB is the language's answer to a quantifier-ordering problem. Ordinary generics
put the caller's choices *outside* the function: `<'a>` is decided at the call site.
But a callback or impl frequently has to operate on data the function makes *for
itself*, after the call has begun — data whose lifetime is logically *inside* the
function. `for<'a>` moves the lifetime's binder from the outside (caller-chosen,
fixed) to the inside (callee-chosen, fresh each use). Once you see it as "who gets to
fill in this lifetime, and when," every symptom follows: the let-bound closure that's
"not general enough" (it committed to one lifetime too early), the named-`<'a>` that
rejects your locals (it's fixed from outside), `DeserializeOwned` (owns its data, so
it qualifies for *every* lifetime), and the parser combinator that composes without a
lifetime in sight (the binder hides inside each `dyn`). HRTB is how you write "works
for any borrow I'll ever hand it" — and most of the time, elision writes it for you.

## Explain it back

- What is the difference in *who chooses the lifetime* between `<'a, F: Fn(&'a str)>`
  and `<F: for<'a> Fn(&'a str)>`?
- Why does `Fn(&str) -> &str` need no annotation, and what does it desugar to?
- In `measure_on_local`, why can't the bound be a plain caller-chosen `<'a>`?
- You get "implementation of `Fn` is not general enough" from a `let`-bound closure.
  What exactly went wrong, and what are three ways to fix it?
- Why does a single named `<'a>` reject two locals in `sum_two_locals`, when
  `for<'a>` accepts them?
- Why is `Owned` a `DecodeOwned` but `Borrowed<'a>` is not? Relate it to why
  `serde_json::from_reader` needs `DeserializeOwned`.
- Why does `Box<dyn for<'a> Fn(&'a str) -> &'a str>` let `StrPipeline` avoid a
  lifetime parameter, where a fixed `'x` would not?
- In `then`, which line relies on `b` being higher-ranked, and what would break
  without it?

## See also

- [Lifetimes in depth](lifetimes-depth.md) — caller-chosen `<'a>`, elision, and
  `'a: 'b` bounds; HRTB is the next quantifier up.
- [Conversion traits](conversions.md) — `From`/`TryFrom` and trait families, the
  same "one impl, many instantiations" shape as `Slicer<'a>`.
- [Cow — Clone-on-Write](cow.md) — the borrowed-vs-owned split that `DecodeOwned`
  turns into a trait bound.
