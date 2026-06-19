# Lifetimes in depth

> Ladder: [`src/bin/lifetimes_depth.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/lifetimes_depth.rs) ·
> Run: `cargo run --bin lifetimes_depth` · Phase 1 · 9 rungs

## TL;DR

A lifetime label like `'a` **does not create, extend, or change** the life of
anything. It is a *name* you attach to references so the compiler can check one
single rule:

> A reference must never outlive the data it points to.

`<'a>` on a function is a generic parameter — just over a *lifetime* instead of a
type. Its whole job is to **describe how an output borrow is connected to the
input borrows**, so the compiler can prove the returned reference is still valid
at every call site. You are not telling the compiler how long things live; you
are telling it *which borrows share a fate*.

## Why this exists (from first principles)

Rust has no garbage collector. A reference (`&T`) is just a pointer — it does not
keep the pointed-to data alive. So the danger is the **dangling reference**: a
pointer to memory that has already been freed. C and C++ let you write this and
hand you undefined behavior at runtime. Rust refuses to compile it.

To refuse it, the borrow checker needs to reason about *spans of validity*. Most
of the time it figures these out on its own. But at **function and struct
boundaries** the information is lost: a signature is a contract, and the compiler
only sees the signature, not the body, when checking a *call*. Consider:

```rust
fn longest(a: &str, b: &str) -> &str { /* ... */ }   // WRONG: won't compile
```

When some other code calls `longest(&x, &y)`, how long is the returned `&str`
valid? It depends on whether the body returns `a` or `b` — but the caller can't
see the body, and the compiler refuses to peek (that would make signatures
meaningless and changes to a body could silently break callers). So the signature
*must* carry the answer. Lifetimes are that missing piece of the contract:

```rust
fn longest<'a>(a: &'a str, b: &'a str) -> &'a str {   // OK
    if a.len() > b.len() { a } else { b }
}
```

Read it as: "for some lifetime `'a`, both inputs are borrowed for at least `'a`,
and the result is valid for `'a`." At a call site the compiler picks `'a` to be
the *overlap* of the two input borrows, and guarantees the result doesn't escape
that overlap. No runtime cost — this is all erased before codegen.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `longest` | Annotate a fn returning one of two `&str`; tie inputs and output to one `'a`. |
| 2 | mechanics | `first_word` / `prefix_before` | The 3 elision rules — and where they run out. |
| 3 | mechanics | `Excerpt<'a>` | A struct that holds a reference must declare a lifetime. |
| 4 | mechanics | `impl<'a>` methods | `&self` elision (rule 3) and the gotcha when the return can come from a param. |
| 5 | footguns | `make_label` / `after_slash` | The dangling-return error: owned vs borrowed-from-input. |
| 6 | footguns | `or_default` / `store` | `'a: 'b` outlives bounds — when one lifetime must contain another. |
| 7 | patterns | `longest_with_announcement` / `leak_label` | Lifetimes + generics + trait bounds; `&'static` and `Box::leak`. |
| 8 | patterns | `Words<'a>` | A borrowing `Iterator`: why `Item = &'a str`, not tied to `&mut self`. |
| 9 | capstone | `StrSplit<'h, 'd>` | Zero-copy split with two independent lifetimes. |

## The ideas, built up

### 1. One lifetime ties inputs to output (`longest`)

The annotation `<'a>(a: &'a str, b: &'a str) -> &'a str` does **not** mean "`a`
and `b` live equally long." It means: the compiler will choose a single `'a` that
is *no longer than either input's actual borrow*, and promise the output lives
that long. The check that makes this sound:

```rust
let s1 = String::from("a long string");
let result;
{
    let s2 = String::from("short");
    result = longest(&s1, &s2);   // 'a = the shorter of the two borrows = s2's scope
    assert_eq!(result, "a long string");
}   // s2 dropped here; using `result` past this point would be rejected
```

`'a` collapses to the *smaller* region (here `s2`'s scope), and the borrow
checker ties `result` to it. That is the entire mechanism: **lifetimes are
constraints the compiler solves, not durations you assign.**

### 2. Elision — the rules that let you omit lifetimes (`first_word`)

You rarely write `'a` because the compiler applies three **elision rules**, in
order, to fill them in:

1. Each elided **input** reference gets its **own fresh** lifetime.
2. If there is **exactly one** input lifetime, it is assigned to **all** outputs.
3. If one input is `&self` / `&mut self`, **its** lifetime goes to all outputs.

If after these an output lifetime is still unknown, you get a hard error.

Rule 1 + rule 2 are why this needs no annotation — one input ref, so the output
is unambiguous:

```rust
fn first_word(s: &str) -> &str {            // elided to fn first_word<'a>(s: &'a str) -> &'a str
    let first_space = s.find(' ').unwrap_or(s.len());
    &s[..first_space]
}
```

Where elision **runs out**: two input refs, no `&self`. Rule 2 doesn't apply,
rule 3 doesn't apply, so the compiler cannot guess which input the output borrows
from:

```rust
fn prefix_before(text: &str, marker: &str) -> &str   // WRONG: missing lifetime specifier
```

The fix is the real lesson — **only annotate what flows to the output.** Only
`text` is returned, so only `text` gets the named lifetime. Leave `marker` with
its own elided one:

```rust
fn prefix_before<'a>(text: &'a str, marker: &str) -> &'a str {   // OK
    let first = text.find(marker).unwrap_or(text.len());
    &text[..first]
}
```

This is a habit worth keeping: tying `marker` to `'a` too would over-constrain
every caller for no reason. Give the output's source a name; let everything else
elide.

### 3. Structs that hold references (`Excerpt<'a>`)

A struct that stores a reference must declare a lifetime and tag the field:

```rust
struct Excerpt<'a> {
    part: &'a str,
}
```

The new rule this buys: **an `Excerpt` value may never outlive the `str` it
borrows from.** The lifetime parameter makes the struct *generic over* how long
its borrow is good for, and the borrow checker enforces that the whole struct
dies before the borrowed data does.

The constructor uses `'_`, the inferred lifetime:

```rust
fn first_sentence(text: &str) -> Excerpt<'_> {   // '_ = "infer it" -> tied to text by elision
    let dot = text.find('.').unwrap_or(text.len());
    let offset = if dot == text.len() { 0 } else { 1 };
    Excerpt { part: &text[..dot + offset] }
}
```

`Excerpt<'_>` reads as "an `Excerpt` borrowing for *some* lifetime the compiler
will infer" — here, by elision, the lifetime of `text`. Writing `Excerpt<'a>`
with an explicit `<'a>` on the fn would mean the same thing.

### 4. Methods, and the `&self` gotcha (`impl<'a>`)

To add methods, you **declare** the lifetime after `impl` and **use** it on the
type — exactly like `impl<T> Vec<T>`:

```rust
impl<'a> Excerpt<'a> {
    fn part(&self) -> &str { self.part }   // rule 3: return tied to &self, no annotation needed
}
```

The gotcha appears the moment the return can come from a **parameter** instead of
`self`. Elision rule 3 has already tied the elided return to `&self`, so a
returned parameter is rejected — its lifetime is unrelated to `&self`:

```rust
// WRONG: returning `candidate` fails — its lifetime isn't tied to &self
fn longer_of(&self, candidate: &str) -> &str {
    if self.part.len() > candidate.len() { self.part } else { candidate }
}
```

The fix is to give `self.part` (which is `&'a str` — actually `&'b self` here) and
`candidate` the **same** lifetime and return that:

```rust
fn longer_of<'b>(&'b self, candidate: &'b str) -> &'b str {   // OK
    if self.part.len() > candidate.len() { self.part } else { candidate }
}
```

Now both possible return sources provably share one lifetime, so either branch is
valid. The general principle: **when a method's return may come from a parameter,
elision's default (tie to `&self`) is wrong — name a lifetime and share it.**

### 5. The dangling-return footgun (`make_label` / `after_slash`)

This is *the* defining lifetime error. A function may only return a reference to
data that **outlives the call** — i.e. data the caller owns. It cannot return a
reference to a local, because the local is dropped at the closing brace and the
reference would dangle:

```rust
// WRONG (E0515: cannot return reference to local variable `label`)
fn make_label(id: u32) -> &str {
    let label = format!("item-{id}");
    &label                              // label dies here; &label can't escape
}
```

There is **no lifetime annotation that fixes this** — the data genuinely doesn't
live long enough. The honest fix is to return *owned* data:

```rust
fn make_label(id: u32) -> String {     // OK: hand back ownership
    format!("item-{id}")
}
```

The contrast that makes it click: returning a slice of a **parameter** is fine,
because the caller owns it and it outlives the call:

```rust
fn after_slash(haystack: &str) -> &str {   // OK: result borrows the caller's data
    match haystack.find('/') {
        Some(i) => &haystack[i + 1..],
        None => "",
    }
}
```

Rule of thumb: **you can borrow *out of* what was borrowed *in*; you cannot
borrow out of what you made locally.** If you made it, return it (move it out).

### 6. `'a: 'b` outlives bounds (`or_default` / `store`)

Syntax: `'a: 'b` is a bound meaning **"`'a` outlives `'b`"** — `'a` lasts at least
as long as `'b`. It sits in the same slot as a trait bound `T: Clone`, but for
lifetimes. It is what lets the compiler treat a longer-lived `&'a T` as a
shorter `&'b T` (covariance, made explicit).

When you return the `'a` reference where a `'b` is promised, you must assert the
relationship or the compiler refuses ("lifetime 'a may not live long enough"):

```rust
fn or_default<'a: 'b, 'b>(primary: &'a str, fallback: &'b str) -> &'b str {   // OK
    if !primary.is_empty() { primary } else { fallback }
}
```

Where the bound is **unavoidable** — you can't unify the two lifetimes because
they appear in different positions. `store` overwrites what a slot points at:

```rust
fn store<'a: 'b, 'b>(slot: &mut &'b str, value: &'a str) {   // OK
    *slot = value;   // dropping a &'a into a &'b slot requires 'a to last >= 'b
}
```

The slot holds a `&'b str`, so anything written into it must live at least as
long as `'b`. Without `'a: 'b` the assignment is rejected. The lesson: when one
reference is *stored where another lives*, you often need to spell out which one
contains the other.

### 7. Lifetimes + generics + `'static` (`longest_with_announcement` / `leak_label`)

Lifetime params and type params share **one** `<...>` list, lifetimes first. They
don't interfere — `'a` constrains borrows, `T` constrains a type:

```rust
fn longest_with_announcement<'a, T>(x: &'a str, y: &'a str, ann: T) -> &'a str
where
    T: Display,
{
    println!("Announcing: {}", ann);
    if x.len() > y.len() { x } else { y }
}
```

`'static` is the lifetime that lasts the whole program. A `&'static str` borrows
data that never goes away — string literals are baked into the binary. Rung 5
gave one escape from "can't borrow out a local" (return owned). Here is the
*other* one: deliberately **leak** the allocation so it lives forever, yielding a
genuine `&'static str`:

```rust
fn leak_label(id: u32) -> &'static str {
    let label = format!("id-{}", id);
    Box::leak(label.into_boxed_str())   // never freed -> valid for 'static
}
```

> Caution: `Box::leak` is a real, permanent memory leak. It's the right tool for
> values that truly live for the program's duration (config, interned strings),
> not a way to dodge the borrow checker in a loop.

Worth distinguishing: `&'static T` ("this reference is valid forever") vs
`T: 'static` ("this type contains no borrow shorter than the program" — which
includes all owned types like `String`). They are not the same constraint.

### 8. A borrowing iterator (`Words<'a>`)

`Words` walks a string and yields each whitespace-separated word as a `&str` that
**borrows the original string** — exactly how `str::split` and `slice::iter` hand
out references without cloning. The crux is the `Item` lifetime:

```rust
struct Words<'a> { remainder: &'a str }

impl<'a> Iterator for Words<'a> {
    type Item = &'a str;   // borrows the STRING (lifetime 'a), NOT &mut self

    fn next(&mut self) -> Option<Self::Item> {
        let trimmed = self.remainder.trim_start_matches(' ');
        if trimmed.is_empty() { return None; }
        let word = match trimmed.find(' ') {
            Some(i) => { self.remainder = &trimmed[i + 1..]; &trimmed[..i] }
            None    => { self.remainder = "";              trimmed       }
        };
        Some(word)
    }
}
```

Read the signature carefully. `&mut self` is borrowed only for the *duration of
one `next()` call*. But the `&str` we return points into the underlying string
(lifetime `'a`), which lives **much longer** than a single call. So `Item` must
be `&'a str`, tied to the string — **not** to `&mut self`.

That choice is what makes this work:

```rust
let mut it = Words::new(&text);
let first = it.next().unwrap();    // holds a word...
let second = it.next().unwrap();   // ...across another next() call — compiles!
assert_eq!((first, second), ("the", "quick"));
```

If `Item` were tied to `&mut self`, `first` would borrow the iterator and the
second `next()` (another `&mut`) would be a borrow-conflict — you could never
keep a yielded item past the next iteration, and `.collect::<Vec<&str>>()` would
be impossible. **Tie `Item` to the data, not the cursor.**

### 9. Capstone: `StrSplit` with two lifetimes

Build your own `"a,b,c".split(",")` that **never allocates** — every yielded piece
is a `&str` slice into the original haystack. The structural insight is **two
distinct lifetimes**:

```rust
struct StrSplit<'haystack, 'delimiter> {
    remainder: Option<&'haystack str>,
    delimiter: &'delimiter str,
}

impl<'haystack, 'delimiter> Iterator for StrSplit<'haystack, 'delimiter> {
    type Item = &'haystack str;   // items borrow the HAYSTACK, never the delimiter

    fn next(&mut self) -> Option<Self::Item> {
        let remainder = self.remainder.as_mut()?;   // None -> already exhausted
        match remainder.find(self.delimiter) {
            Some(i) => {
                let current = *remainder;
                let piece = &current[..i];
                *remainder = &current[i + self.delimiter.len()..];   // skip the whole delimiter
                Some(piece)
            }
            None => self.remainder.take(),   // last piece; mark exhausted in one move
        }
    }
}
```

**Why two lifetimes.** The yielded pieces borrow `'haystack`. They do *not* borrow
`'delimiter` — once we've located a delimiter we keep no reference to it in the
output. Keeping the lifetimes separate lets a caller pass a **short-lived
delimiter** (even a temporary that drops before the results are used) and still
keep the pieces:

```rust
let haystack = String::from("x-y-z");
let result: Vec<&str>;
{
    let delim = String::from("-");
    result = StrSplit::new(&haystack, &delim).collect();
}   // delim dropped here — result is still valid, because pieces borrow haystack
assert_eq!(result, vec!["x", "y", "z"]);
```

Collapsing both into one lifetime would over-constrain every caller — the same
lesson as rung 2b and rung 6, now at struct scale: **give each borrow its own
lifetime; only unify when the data flow actually demands it.**

**Why `remainder: Option<&str>` and not just `&str`.** To distinguish "more to
yield, possibly an empty final field" from "fully exhausted." Splitting `"a,"` on
`","` must yield `["a", ""]` — a trailing empty piece — and *then* stop. The
`Option` lets `next` hand out that last `""` once via `.take()` (which returns
`Some(last)` and sets the field to `None`), and return `None` forever after.

## Footguns

- **Thinking `'a` extends a life.** It never does. It only *names* a borrow so the
  compiler can relate borrows. The compiler picks `'a` to be the *overlap* of the
  inputs — the shortest region, not the longest.
- **Over-annotating.** Tying every reference to one `'a` (e.g. `marker` in
  `prefix_before`, or merging `'haystack`/`'delimiter`) compiles but needlessly
  restricts callers. Name only the borrow that flows to the output.
- **The `&self` return trap.** Elision rule 3 ties an elided return to `&self`. If
  the method can return a *parameter*, you must introduce a shared lifetime — the
  default will reject the parameter branch.
- **Returning a reference to a local (E0515).** No annotation fixes this; the data
  dies at the brace. Return owned data, or `Box::leak` for genuine `&'static`.
- **Tying an iterator's `Item` to `&mut self`.** You then can't hold an item across
  the next `next()` call, and `.collect()` breaks. Tie `Item` to the underlying
  data's lifetime instead.
- **`Box::leak` as a borrow-checker dodge.** It's a permanent leak. Fine for
  program-lifetime data; a bug inside a loop or per-request path.

## Real-world patterns

- **`str::split` / `slice::iter`** are exactly the `Words` / `StrSplit` shape:
  zero-copy iterators whose `Item` borrows the source, not the cursor.
- **Parsers and tokenizers** lean on multi-lifetime structs so tokens can borrow
  the input buffer while transient state (delimiters, config) lives shorter.
- **`Cow<'a, str>`** (see [Cow](cow.md)) carries a lifetime for its borrowed
  variant — the same `struct holds a reference` rule as `Excerpt<'a>`.
- **`'a: 'b` outlives bounds** show up whenever you store one reference into a
  place that already holds another (caches, slot updates, builder APIs).
- **`T: 'static` bounds** are everywhere in `std::thread::spawn` and trait objects
  (`Box<dyn Error + 'static>`) — "this value carries no borrow that could dangle."

## Capstone insight

A reference-holding iterator has **two clocks**: the borrow of `&mut self` during
one `next()` call, and the borrow of the *data* it yields. The whole design hinges
on tying `Item` to the data clock, not the call clock. Generalize that to
`StrSplit` and you see lifetimes are a *dependency graph between borrows*: give
each borrow its own lifetime parameter, then add only the edges (`'a: 'b`, or
shared names) the data flow forces. Under-constrain and it won't compile;
over-constrain and your callers suffer. Lifetimes are the language for drawing
exactly that graph — no more, no less.

## Explain it back

- Why does `longest` need `'a` but `first_word` doesn't? Which elision rules cover
  `first_word`?
- What does the compiler actually pick `'a` to be at a call site — the longer or
  the shorter input borrow, and why?
- Why does returning `candidate` from `longer_of(&self, candidate)` fail under
  elision, and what's the minimal fix?
- Why can `after_slash` return a `&str` but `make_label` cannot? What are the two
  escape hatches?
- What does `'a: 'b` mean, and give a case where it's unavoidable.
- In `Words`, why is `type Item = &'a str` and not tied to `&mut self`? What breaks
  if you get it wrong?
- In `StrSplit`, why two lifetimes instead of one, and why is `remainder` an
  `Option`?
- `&'static str` vs `T: 'static` — what's the difference?

## See also

- [Cow — Clone-on-Write](cow.md) — a lifetime-carrying enum over borrowed/owned.
- [Borrow / ToOwned](borrow-toowned.md) — borrowed-vs-owned views, the `Cow` loop.
- [Drop & Ordering](drop-ordering.md) — *when* the data a reference points at dies.
