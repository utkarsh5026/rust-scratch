# Error handling architecture

> Ladder: [`src/bin/error_arch.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/error_arch.rs) ·
> Run: `cargo run --bin error_arch` · Phase 3 · 9 rungs

## TL;DR

Rust has no exceptions. An error is just a **value** of type `Result<T, E>`, and
`?` is sugar for "if `Err`, convert via `From::from` and return early." So the
entire architecture question collapses to one decision: **what is `E`, and who
chooses its shape?**

Two answers, used at *different layers*:

- **Libraries → `thiserror`**: a typed, exhaustive `enum` the caller can `match`
  on and recover from. You hand out structure.
- **Applications → `anyhow`**: one opaque `anyhow::Error` that swallows any error,
  carries `.context()` breadcrumbs, and bubbles to `main`. The caller wants a
  report, not a branch.

`?` + `From` is the weld between the two. And `anyhow` is not magic — its core is a
blanket `From` impl plus a `.context()` that chains the original error as a
`source()`. The capstone rebuilds it in ~30 lines.

## Why this exists (from first principles)

In a language with exceptions, the error path is invisible: any call *might* throw,
and the type signature doesn't say so. Rust makes the error path part of the type:
a fallible function returns `Result<T, E>`, and you cannot get the `T` out without
acknowledging the `E`. That's the whole safety story — no surprise unwinding, no
forgotten failure mode.

But that honesty has an ergonomic cost: every fallible call would need an explicit
`match` to propagate. `?` buys the ergonomics back:

```rust
let n = s.parse::<i32>()?;   // desugars roughly to:
// let n = match s.parse::<i32>() {
//     Ok(v) => v,
//     Err(e) => return Err(From::from(e)),
// };
```

The critical word is `From::from`. `?` will only compile if the function's error
type implements `From<the error at the call site>`. **Every design choice in this
topic is downstream of that one fact.** Pick `E = Box<dyn Error>` and the blanket
`From<E: Error>` impl makes everything work. Pick a custom enum and you owe a
`From` impl per source error (or a `#[from]` to generate it). Pick `anyhow::Error`
and its blanket `From` covers everything.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|-----------|
| 1 | foundations | `?` + `Box<dyn Error>` | heterogeneous errors collapse to one trait object |
| 2 | foundations | hand-rolled enum | `Display` + `Error` + `From` is the contract |
| 3 | mechanics | `thiserror` derive | `#[error]`/`#[from]` generate rung 2 verbatim |
| 4 | mechanics | `anyhow` | opaque error + `.context()` / `bail!` / `anyhow!` |
| 5 | footgun | source chains & downcasting | errors are a linked list; recover the type back |
| 6 | footgun | E0277 + `String` errors | `?` needs `From`; `String: !Error` is a trap |
| 7 | real-world | lib/app boundary | typed error survives *under* the anyhow context |
| 8 | real-world | classification | `is_retryable()` + `#[non_exhaustive]` drive a retry loop |
| 9 | capstone | mini-`anyhow` | blanket `From` + `Context` trait + `source()` chain |

## The ideas, built up

### 1. The quick app error: `Box<dyn Error>`

When you don't care about the *type* of the failure — you just want it to bubble —
return `Box<dyn Error>`. Different concrete errors unify into one trait object:

```rust
fn parse_and_double(s: &str) -> Result<i32, Box<dyn Error>> {
    let n = s.parse::<i32>()?;             // ParseIntError -> Box<dyn Error>
    if n == 13 {
        return Err("13 is unlucky".into()); // &str -> Box<dyn Error>
    }
    Ok(n * 2)
}
```

Two *different* errors (`ParseIntError` and a string) leave through the same return
type with zero ceremony. This works because of two `From` impls in std:
`impl<E: Error + ...> From<E> for Box<dyn Error>` (coerces the parse error) and
`impl From<&str> for Box<dyn Error>` (builds an error from a message). That's the
seed of the entire "erased error" idea that `anyhow` industrializes later.

### 2. The contract for being an error

A *library* should not force `Box<dyn Error>` on callers — it should hand out a
type they can `match`. To be a "real" error in Rust you implement two traits:

- `Display` — the human-readable message.
- `std::error::Error` — the marker trait (with `Debug + Display` as supertraits)
  that unlocks `?`-into-`Box<dyn Error>`, source chains, and downcasting.

```rust
#[derive(Debug)]
enum ConfigError { Missing(String), Parse(ParseIntError) }

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(k) => write!(f, "missing key: {k}"),
            ConfigError::Parse(e)   => write!(f, "invalid number: {e}"),
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConfigError::Parse(e) => Some(e),  // expose the underlying cause
            _ => None,
        }
    }
}

impl From<ParseIntError> for ConfigError {     // <- this is what makes `?` work
    fn from(e: ParseIntError) -> Self { ConfigError::Parse(e) }
}
```

That `From<ParseIntError>` impl is the *only* reason `?` can turn a parse failure
into a `ConfigError`. The `source()` override is optional now but pays off in rung
5 — it's the link that lets a caller walk from `ConfigError` down to the
`ParseIntError` that caused it.

### 3. `thiserror`: the boilerplate, derived

Everything in rung 2 — `Display`, `Error`, `From`, `source` — is mechanical. The
`thiserror` derive generates **byte-for-byte the same code** from attributes:

```rust
#[derive(Debug, thiserror::Error)]
enum LoadError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("bad number: {0}")]
    BadNumber(#[from] ParseIntError),
    #[error("input was empty")]
    Empty,
}
```

- `#[error("...")]` generates the `Display` impl. `{0}` interpolates the tuple
  field.
- `#[from]` on a field generates `From<that type>` **and** wires that field up as
  the `source()`. One attribute, both jobs.

It is a zero-runtime-cost macro — no boxing, no dynamic dispatch. That's why it's
the library-layer choice: the caller still gets a fully typed enum to `match`.

### 4. `anyhow`: the application's opaque error

An application's top layer rarely wants to `match` on variants. It wants: "did it
work? if not, give me a good report and bubble to `main`." `anyhow::Error` is one
opaque type that **any** `E: Error + Send + Sync + 'static` converts into via `?`,
and its superpower is *context*:

```rust
use anyhow::{Context, anyhow};

fn load_user(dir: &str, id: &str) -> anyhow::Result<u64> {
    if dir == "missing" {
        return Err(anyhow!("no such dir: {dir}"));   // ad-hoc error
    }
    let id = id.parse::<u32>()
        .with_context(|| format!("parsing user id {id:?}"))?;  // add a breadcrumb
    Ok(id as u64 * 2)
}
```

The key behavior: `.with_context(...)` makes the context message the **outer**
`Display`, while the original `ParseIntError` is **preserved underneath** as the
`source()`. anyhow never throws the real error away — it stacks a readable layer on
top. So `e.to_string()` is `parsing user id "xyz"` but `e.source()` is still
`Some(ParseIntError)`.

- `.context("literal")` — eager.
- `.with_context(|| ...)` — lazy; the closure only runs on the error path. Use it
  when building the message costs something.
- `anyhow!(...)` builds an ad-hoc error; `bail!(...)` is `return Err(anyhow!(...))`.

### 5. Errors are a linked list: walk it, then downcast back

Every error optionally points at the cause it wrapped via `.source()`. That makes
an error a singly-linked list, and `.context()` grows it. Walking it gives a full
report:

```rust
fn error_chain(err: &dyn Error) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = err;
    chain.push(current.to_string());
    while let Some(source) = current.source() {
        chain.push(source.to_string());
        current = source;
    }
    chain
}
// load_user("data","xyz") -> ["parsing user id \"xyz\"", "invalid digit found in string"]
```

The reverse trick is **downcasting**: recover a concrete type after it has been
erased into `anyhow::Error`. This is the escape hatch for "opaque by default, but
typed when the app *does* need to branch":

```rust
fn classify(err: &anyhow::Error) -> &'static str {
    if let Some(load_error) = err.downcast_ref::<LoadError>() {
        match load_error {
            LoadError::Empty => "empty",
            _ => "load",
        }
    } else {
        "other"
    }
}
```

`downcast_ref::<T>()` walks the chain for you — even when the `LoadError` was
wrapped in a `.context(...)`, anyhow can still reach down and hand back the concrete
`&LoadError`.

### 6. The two footguns `?` sets

**E0277 — "the trait bound `MyError: From<X>` is not satisfied".** This is the most
common real-world error message in the whole topic, and it is *not* mysterious: it
is `?` telling you the `From` impl it needs doesn't exist. The fix is to add it (or
`#[from]`):

```rust
#[derive(Debug, thiserror::Error)]
enum PipelineError {
    #[error("stage a failed: {0}")] StageA(#[from] ParseIntError),
    #[error("stage b failed: {0}")] StageB(#[from] TryFromIntError),
    #[error("legacy: {0}")]         Legacy(String),
}
```

**`Result<T, String>` is an anti-pattern.** `String` does **not** implement
`std::error::Error`, so a stringly-typed error has no `source()` chain, can't be
`downcast`, and can't be matched — you discarded all structure and kept a sentence.
Wrap it back into a real type *at the boundary* with `.map_err`:

```rust
fn adapt_legacy(ok: bool) -> Result<i32, PipelineError> {
    legacy_op(ok).map_err(PipelineError::Legacy)   // tuple variant as a fn value
}
```

Note the deliberate choice **not** to put `#[from]` on `Legacy(String)`: a blanket
`From<String>` would let `?` silently coerce *any* stray `String` into your error
type. Forcing an explicit `.map_err` keeps the wrapping intentional.

### 7. The boundary: `thiserror` library, `anyhow` application

This is the whole architecture in one screen. The library exposes a typed error;
the app wraps it in context and returns opaque `anyhow::Error` — but the typed error
*survives underneath* and is recoverable:

```rust
mod store { // LIBRARY
    #[derive(Debug, thiserror::Error)]
    pub enum StoreError {
        #[error("key not found: {key}")] NotFound { key: String },
        #[error("not a number: {0}")]    Parse(#[from] ParseIntError),
    }
    pub fn get_number(key: &str) -> Result<i64, StoreError> { /* typed */ }
}

fn load_setting(key: &str) -> anyhow::Result<i64> { // APPLICATION
    store::get_number(key).with_context(|| format!("loading setting {key:?}"))
}
```

The payoff, proven by the test:

```rust
let e = load_setting("missing").unwrap_err();
assert_eq!(e.to_string(), r#"loading setting "missing""#);     // anyhow context outside
let typed = e.downcast_ref::<store::StoreError>();              // ...typed error still inside
assert!(matches!(typed, Some(store::StoreError::NotFound { .. })));
```

"thiserror for libs, anyhow for apps" isn't a compromise — `downcast_ref` means you
get both: opaque convenience *and* typed recovery.

### 8. Classify, don't just propagate: `is_retryable` + `#[non_exhaustive]`

A mature error type lets callers decide *how to react*, not just *what failed*.
Put the policy **on the error type** as a method, and a fully generic consumer can
branch without knowing any variant:

```rust
impl ApiError {
    fn is_retryable(&self) -> bool {
        match self { // exhaustive: a NEW variant forces a compile error here
            ApiError::RateLimited { .. } | ApiError::Timeout
                | ApiError::ServiceUnavailable => true,
            ApiError::NotFound { .. } | ApiError::Unauthorized => false,
        }
    }
}

fn run_with_retry<T, F>(max_attempts: u32, mut op: F) -> Result<T, ApiError>
where F: FnMut() -> Result<T, ApiError> {
    for attempt in 0..max_attempts {
        match op() {
            Ok(v) => return Ok(v),
            Err(e) => {
                if !e.is_retryable() || attempt + 1 == max_attempts {
                    return Err(e);          // fatal, or out of attempts
                }
                // retryable: loop again (real code would back off here)
            }
        }
    }
    unreachable!()
}
```

`run_with_retry` knows *nothing* about specific variants — only `is_retryable()`.
Add a variant later and every retry site behaves correctly for free.

`#[non_exhaustive]` on the enum is the companion: it forces *downstream crates* to
include a `_` arm in their `match`, so you can add variants later without a breaking
change. Note the split — inside the defining crate the `match` stays **exhaustive**
(a forgotten new variant won't compile, which is a safety net); only foreign crates
are forced to the wildcard.

## Footguns

| Trap | What bites | Fix |
|------|-----------|-----|
| `?` won't compile (E0277) | no `From<source>` for your error type | add `From` / `#[from]`, or return `Box<dyn Error>` / `anyhow::Error` |
| `Result<T, String>` | `String: !Error` — no source, no downcast, no match | wrap at the boundary with `.map_err(MyError::Variant)` |
| `source()` not overridden | the cause chain stops short; reports lose the root | override `source()` (or use `#[from]`/`#[source]`) |
| `#[from]` on an ad-hoc `String` variant | `?` silently coerces *any* `String` into your error | drop `#[from]`, force explicit `.map_err` |
| implementing `Error` for an anyhow-like wrapper | collides with std's reflexive `From<T> for T` | don't impl `Error` on the wrapper (see capstone) |
| matching a `#[non_exhaustive]` foreign enum without `_` | won't compile downstream | always add a `_ =>` arm for others' error enums |

## Real-world patterns

- **Library crates** define one `#[derive(thiserror::Error)] #[non_exhaustive]` enum
  per module/crate; `#[from]` for wrapped sources; classification methods like
  `is_retryable()` / `kind()` for callers.
- **Binaries** use `fn main() -> anyhow::Result<()>`, sprinkle `.context(...)` at
  each layer, and let the error bubble; anyhow prints the whole `source()` chain.
- **`Box<dyn Error>`** is the std-only middle ground (no dependency) when you want
  erasure without anyhow's context/backtrace features.
- The std `?` + `From` mechanism is what makes all three interoperate: a library's
  typed `StoreError` flows into an app's `anyhow::Error` with no glue code.

## Capstone insight: `anyhow` is ~30 lines

The build-it rung strips the magic. `anyhow::Error` is essentially:

```rust
pub struct MyError(Box<dyn Error + Send + Sync + 'static>);

// (1) the single most important impl: this is what makes `?` erase any error.
impl<E: Error + Send + Sync + 'static> From<E> for MyError {
    fn from(e: E) -> Self { MyError(Box::new(e)) }
}
```

Two non-obvious truths fall out of this:

1. **`MyError` must NOT implement `Error`.** If it did, the blanket
   `From<E: Error>` above would overlap with std's reflexive `From<MyError> for
   MyError` — a coherence conflict. The real `anyhow::Error` makes the exact same
   choice (it implements `Display + Debug` but not `Error`). The thing you reach
   for to *erase* errors deliberately isn't one itself.

2. **`.context()` is just another error whose `source()` is the old one.** Stacking
   context is growing the linked list by one node:

```rust
struct ContextError { msg: String, source: Box<dyn Error + Send + Sync + 'static> }

impl Error for ContextError {
    fn source(&self) -> Option<&(dyn Error + 'static)> { Some(&*self.source) }
}

trait WrapErr<T> { fn context<C: Display>(self, ctx: C) -> Result<T, MyError>; }

impl<T, E: Error + Send + Sync + 'static> WrapErr<T> for Result<T, E> {
    fn context<C: Display>(self, ctx: C) -> Result<T, MyError> {
        self.map_err(|e| MyError(Box::new(ContextError {
            msg: ctx.to_string(),
            source: Box::new(e),
        })))
    }
}
```

Walk `.source()` on the result and you see the context message on top of the
original error — exactly anyhow's `{:#}` output. (`Some(&*self.source)` is the
deref-then-reborrow that turns the `Box` back into a `&dyn Error`.) Once you've
written this, anyhow stops being a black box: it's a blanket `From`, a boxed trait
object, and a context node that chains via `source()`.

## Explain it back

- Why does `?` require a `From` impl, and what exactly does it call?
- When do you reach for `thiserror` vs `anyhow` vs `Box<dyn Error>`? Why not anyhow
  in a library?
- What does `#[from]` generate, and why does it also set `source()`?
- An error has been erased into `anyhow::Error`. How do you (a) print the full cause
  chain and (b) recover a specific typed variant to branch on?
- Why is `Result<T, String>` an anti-pattern? What capability do you lose?
- Why can't an anyhow-style erased error type implement `std::error::Error` itself?
- How does `.context()` preserve the original error? What does `source()` return for
  a context node?
- What's the difference between an exhaustive `match` inside the defining crate and
  the `_` arm `#[non_exhaustive]` forces on downstream crates?

## See also

- [Conversion traits](conversions.md) — `From`/`Into` and how `?` rides on them.
- [Box & the Heap](box-heap.md) — `Box<dyn Trait>` erasure, the basis of
  `Box<dyn Error>`.
- [Associated types vs generic params](assoc-vs-generic.md) — trait design choices
  that show up in error-type APIs.
