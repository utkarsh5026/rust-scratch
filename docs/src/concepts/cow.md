# Cow — Clone-on-Write

> Ladder: [`src/bin/cow.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/cow.rs) ·
> Run: `cargo run --bin cow` · Phase 1 · 9 rungs

## TL;DR

`Cow<'a, B>` ("clone on write") is an enum with two states — **`Borrowed(&'a B)`**
or **`Owned(B::Owned)`** — that lets one value be either a cheap borrow or a
heap-owned thing, behind a single type. You hand back `Borrowed` when the data is
already fine, and pay for an `Owned` allocation **only when you actually have to
change something**. It `Deref`s to `B`, so callers use it like a plain `&str` /
`&[T]` and never care which variant it's holding.

> **Mental model:** `Cow` is a *maybe-allocation*. "Here's your string back. I
> only made a new one if I had to."

## Why this exists (from first principles)

Imagine a function `ensure_https(url) -> ???`. Most URLs already start with
`https://` — for those you'd love to just return the input untouched. But some
don't, and for those you must build a **new** string `"https://" + url`.

Now: what's the return type?

- **Return `&str` (a borrow)?** Impossible for the fix-up case — the new string
  is a local; you can't return a reference to data that dies at function end.
- **Return `String` (owned)?** Works, but forces an **allocation + copy on every
  call**, even for the 90% of inputs that were already correct. Wasteful.

You're stuck because the two cases want *different* types. `Cow` is the type that
says **"either of those, decided at runtime"**:

```rust
fn ensure_https(url: &str) -> Cow<'_, str> {
    if url.starts_with("https://") {
        Cow::Borrowed(url)                       // zero cost
    } else {
        Cow::Owned(format!("https://{}", url))   // allocate only here
    }
}
```

That's the whole reason `Cow` exists: **a function that usually borrows but
sometimes must own, without committing every caller to the cost of owning.**

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `ensure_https` | Borrow when correct, own only when you must build. |
| 2 | foundations | `sanitize` (spaces to `_`) | Decide *before* allocating — `.replace()` always allocates. |
| 3 | mechanics | `Cow` as a struct **field** | One `Config<'a>` type holds a literal **or** a runtime string. |
| 4 | mechanics | `clamp_negatives` + `.to_mut()` | The actual *clone-on-**write***: upgrade on first mutation. |
| 5 | footgun | `greeting` | You can only borrow inputs, never **locals**. |
| 6 | mechanics | `first_word` via `Deref` | A `Cow<str>` *is* usable as a `&str` — no variant matching. |
| 7 | real-world | `normalize` batch | Borrow the clean entries, allocate only the dirty ones. |
| 8 | real-world | serde `#[serde(borrow)]` | Zero-copy deserialize; own only when decoding escapes. |
| 9 | capstone | hand-rolled `MyCow` | Build the borrow/own/upgrade machine yourself. |

## The ideas, built up

### The defining discipline: inspect before you allocate

The naive way to replace spaces is `input.replace(' ', "_")` — but `replace`
**always returns a fresh `String`**, even if there were no spaces to replace. That
throws away the entire point of `Cow`. The fix is to *check first*:

```rust
fn sanitize(input: &str) -> Cow<'_, str> {
    if input.contains(' ') {
        Cow::Owned(input.replace(' ', "_"))  // allocate only on the dirty path
    } else {
        Cow::Borrowed(input)                 // clean input: zero allocation
    }
}
```

> **The pattern that repeats all ladder long:** ask "is any work actually
> needed?" *before* you reach for an allocation. `Cow` only pays off if the
> borrowed path is genuinely free.

### Cow as a field: one type, two origins

`Cow` isn't just a return type — as a **struct field** it lets one type absorb
both a borrowed literal and an owned runtime value:

```rust
struct Config<'a> { name: Cow<'a, str> }

Config { name: Cow::Borrowed("default") }      // from a &'static literal — no alloc
Config { name: Cow::Owned(format!("user-{id}")) } // from a runtime String
```

Both are the *same* `Config<'a>` type, and `name(&self) -> &str` reads either one
uniformly (via `.as_ref()`). The lifetime `'a` is the price: the struct can't
outlive whatever the borrowed variant points at.

### The heart: `.to_mut()` and lazy upgrade

This is where "clone-on-**write**" earns its name. `cow.to_mut()` returns a
`&mut` to the owned data — and here's the mechanism:

- If the cow is **`Owned`** already: hands back the ref, **no clone**.
- If it's **`Borrowed`**: **clones into `Owned` first**, swaps itself, *then*
  gives you the mutable ref.

So you call `to_mut()` *exactly at the moment you first need to mutate*, and the
allocation happens then — and only then:

```rust
fn clamp_negatives(input: &[i32]) -> Cow<'_, [i32]> {
    let mut cow: Cow<[i32]> = Cow::Borrowed(input);   // start free
    for i in 0..input.len() {
        if input[i] < 0 {
            cow.to_mut()[i] = 0;   // first negative upgrades Borrowed -> Owned
        }
    }
    cow   // all-positive input is returned still-Borrowed
}
```

An all-positive slice never calls `to_mut()`, so it's returned borrowed for free.
One negative anywhere triggers a single clone, and every later write reuses it.

Note this also shows `Cow` is **not string-only** — here `B = [i32]`, owned
form `Vec<i32>`. Anything that is `ToOwned` works.

### Ergonomics: Deref makes a Cow act like its target

`Cow<str>` implements `Deref<Target = str>`, so every `&str` method works on it
directly — **no `match`, no `.as_ref()`, no caring about the variant**:

```rust
fn first_word(c: &Cow<str>) -> &str {
    c.split_whitespace().next().unwrap_or("")  // str methods, called straight on the Cow
}
```

`c.len()`, `c.starts_with(..)`, `&**c == "hello"` — all Just Work. This is *why*
`Cow` is pleasant to consume, not just to produce.

## Footguns

### You cannot borrow a local (rung 5)

This is *the* defining `Cow` compile error. This does **not** compile:

```rust
fn broken(name: &str) -> Cow<'_, str> {
    let local = format!("hi {name}");
    Cow::Borrowed(&local)   // WRONG: cannot return value referencing local variable
}
```

`Cow::Borrowed` ties its lifetime to data that must **outlive the call**. A
`String` built *inside* the function dies at the closing brace, so you literally
cannot hand it back borrowed. The correct version owns what it builds:

```rust
fn greeting(name: &str) -> Cow<'_, str> {
    if name == "hi there" {
        Cow::Borrowed(name)                       // OK: borrowing an INPUT is fine
    } else {
        Cow::Owned(format!("hi {}", name))        // OK: built locally -> must be Owned
    }
}
```

> **Rule:** `Borrowed` = "I'm pointing at *someone else's* data that lives long
> enough" (inputs, `'static` literals). `Owned` = "I made this myself." You can
> never borrow a local.

### `.replace()` / `.to_lowercase()` always allocate

These produce a fresh `String` unconditionally. If you call them on the borrowed
path "just in case", you've silently defeated `Cow`. Gate them behind a
`.contains(..)` / `.any(..)` check (rungs 2 and 7).

## Signatures to know

```rust
// The enum itself — B is the borrowed form, B::Owned is the owned form
enum Cow<'a, B: ?Sized + ToOwned> {
    Borrowed(&'a B),
    Owned(<B as ToOwned>::Owned),
}

// Upgrade: clone into Owned on first write, then hand back &mut
fn to_mut(&mut self) -> &mut <B as ToOwned>::Owned

// Consume the Cow, producing an owned value either way
fn into_owned(self) -> <B as ToOwned>::Owned

// Transparent access: Cow<str> derefs to &str
impl<B: ?Sized + ToOwned> Deref for Cow<'_, B> {
    type Target = B;
}
```

## Real-world patterns

### Borrow most, own a few (rung 7)

Normalize a batch of words to lowercase, allocating **only** for the ones that
actually had uppercase:

```rust
fn normalize<'a>(words: &'a [&'a str]) -> Vec<Cow<'a, str>> {
    words.iter().map(|w| {
        if w.chars().any(|c| c.is_uppercase()) {
            Cow::Owned(w.to_lowercase())   // dirty -> allocate
        } else {
            Cow::Borrowed(*w)              // already clean -> free
        }
    }).collect()
}
```

A mostly-clean batch costs almost nothing — each clean word still points into the
original input.

### Zero-copy deserialization with serde (rung 8)

This is the marquee payoff. Give a serde struct a `Cow<'a, str>` field and tag it
`#[serde(borrow)]`:

```rust
#[derive(Deserialize)]
struct Msg<'a> {
    #[serde(borrow)]
    text: Cow<'a, str>,
}
```

Now when you deserialize:

- `{"text":"hello world"}` — **`Borrowed`**, pointing straight into the JSON
  input buffer. **Zero copy.**
- `{"text":"line1\nline2"}` — the `\n` escape must be *decoded*, so serde has no
  choice but to build a fresh `String` — **`Owned`**.

One field, both outcomes, decided by the data. Drop the `#[serde(borrow)]` and
serde defaults to *always* `Owned` — watch the borrowed assertion fail. That
contrast *is* the lesson.

## Capstone insight

Re-implementing `MyCow` from scratch makes the whole thing click. It's just two
variants plus three methods — and `to_mut` is the only interesting one, because it
performs the in-place **state transition** from borrowed to owned:

```rust
fn to_mut(&mut self) -> &mut String {
    match self {
        Self::Borrowed(s) => {
            *self = Self::Owned(s.to_string());  // clone + replace SELF
            match self {                         // now re-match to hand out the owned ref
                Self::Owned(s) => s,
                _ => unreachable!(),
            }
        }
        Self::Owned(s) => s,                     // already owned: no clone
    }
}
```

That `*self = ...; re-match` dance is exactly how std does it. Once you've written
it, "clone on write" stops being a slogan and becomes a concrete line of code: the
borrow becomes an allocation *right here*, and nowhere else.

## Explain it back

- Why can't `ensure_https` just return `&str`? Why not just `String`?
- What does `.to_mut()` do differently for a `Borrowed` vs an `Owned` cow?
- Why does `Cow::Borrowed(&local)` fail to compile, but `Cow::Borrowed(input)` is fine?
- What makes `c.split_whitespace()` work directly on a `Cow<str>`?
- In the serde rung, *why* does `"line1\nline2"` come back `Owned` but
  `"hello world"` comes back `Borrowed`?
- `Cow<'a, B>` requires `B: ToOwned` — why? (What couldn't it do without it?)

## See also

- [Borrow / ToOwned](borrow-toowned.md) — the two traits `Cow` is *built on*;
  `B: ToOwned` is what lets the `Owned` variant exist, and rung 8 there closes
  this exact loop.
- [Drop & Ordering](drop-ordering.md) — `mem::replace` (used by `to_mut`
  internally) is covered in depth there.
