# Custom error types

> Ladder: [`src/bin/custom_errors.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/custom_errors.rs) ·
> Run: `cargo run --bin custom_errors` · Phase 3 · 9 rungs

## TL;DR

A "custom error type" is just a normal type that satisfies a two-method contract:
`impl Display` gives it a human message, and `impl std::error::Error` marks it as
an error and *optionally* points at the lower-level error underneath it via
`source()`. Everything else in the error ecosystem — `?`, `Box<dyn Error>`,
downcasting, multi-line `Caused by:` reports, `anyhow`, `thiserror` — is built on
top of those two impls plus `From`. The one idea that unlocks the whole topic is
the **source chain**: a linked list of errors you walk from "what failed" down to
"why", with each link reachable through `source()`.

This ladder builds all of it *by hand, no derive macros*, so you can see exactly
what `thiserror` generates and what `anyhow` does at runtime.

> Sibling page: [Error handling architecture](error-arch.md) covers the
> *architecture* choice (`thiserror` for libs vs `anyhow` for apps). This page is
> the machinery underneath that choice.

## Why it exists (from first principles)

In Rust, errors are **values**: a function that can fail returns `Result<T, E>`
and you choose `E`. The cheapest `E` is a `String` — but a string is a dead end.
The caller can `println!` it and nothing else: they can't match on *which* failure
happened, can't programmatically recover from one case but not another, and can't
inspect what caused it. A string has thrown away all the structure.

So the standard library defines a contract for "a real error":

```rust
pub trait Error: Debug + Display {
    fn source(&self) -> Option<&(dyn Error + 'static)> { None }
    // ... a few unstable methods (backtrace, provide)
}
```

Two things to notice immediately:

- **`Debug + Display` are supertraits.** You literally cannot `impl Error` for a
  type that doesn't already implement both. `Display` is the human message;
  `Debug` is the developer/`{:?}` view. This is why every error in this file
  starts with `#[derive(Debug)]` and a hand-written `Display`.
- **`source()` has a default of `None`.** A "leaf" error (one that originates a
  failure) inherits that default. An error that *wraps* another overrides it to
  hand back the cause. That single optional method is the entire source-chain
  mechanism.

Once a type implements `Error`, it gains superpowers it can't have otherwise: it
coerces into the universal `Box<dyn Error>`, it slots into `?`, and the `dyn Error`
trait object gives you `is::<T>()` / `downcast_ref::<T>()` to recover the concrete
type later. The trait *is* the membership card.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `TooLong` struct | `Display` + empty `impl Error` = a real error; unlocks `Box<dyn Error>` |
| 2 | foundations | `ValidationError` enum | one enum, many failure modes the caller can `match` on |
| 3 | mechanics | `ConfigError` + `source()` | wrap a cause; keep the cause OUT of `Display` |
| 4 | mechanics | `LoadError` + `From` | `?` calls `From::from` — this is what `#[from]` generates |
| 5 | footgun | `Box<dyn Error + Send + Sync>` | the bounds propagate into fields; `Rc` → `Arc` to cross threads |
| 6 | footgun | `describe_root` / downcast | walk `source()` to the root, `is::<T>()` to decide |
| 7 | real-world | `TracedError` + `Backtrace` | capture *where* it failed; `capture()` vs `force_capture()` |
| 8 | real-world | `AppError` 3-level chain | layered library error + anyhow `{:#}` one-line printer |
| 9 | capstone | `Chain` + `Report` | rebuild anyhow's iterator + `Caused by:` reporter from scratch |

## The ideas, built up

### 1. The contract: `Display` + `Error`

The minimum viable error is a `Debug` struct, a `Display` impl, and an *empty*
`Error` impl:

```rust
#[derive(Debug)]
struct TooLong { len: usize, max: usize }

impl fmt::Display for TooLong {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "username too long: {} chars (max {})", self.len, self.max)
    }
}

impl std::error::Error for TooLong {}   // empty body — but it does real work
```

The empty `impl Error for TooLong {}` looks like it does nothing. It's the whole
point. Without it, this line fails to compile:

```rust
let boxed: Box<dyn Error> = Box::new(err);  // needs TooLong: Error
```

The coercion from `Box<TooLong>` to `Box<dyn Error>` is only allowed once the
compiler can prove `TooLong: Error`. The marker impl is what makes the type a
*member* of `dyn Error`. (If you forget it, the error reads:
`the trait bound TooLong: std::error::Error is not satisfied ... required for
the cast from Box<TooLong> to Box<dyn Error>`.)

### 2. One enum, many failure modes

A struct models one failure. Real code fails several ways, and the idiomatic shape
is a single enum with a variant per mode, each carrying exactly the data it needs:

```rust
#[derive(Debug)]
enum ValidationError {
    TooShort { len: usize, min: usize },
    TooLong  { len: usize, max: usize },
    BadChar  { ch: char },
}
```

`Display` becomes a `match self` with one arm per variant. The payoff is on the
*caller's* side — they get one type they can match exhaustively:

```rust
match validate("ab", 3, 16) {
    Err(ValidationError::TooShort { len, min }) => /* tell the user the minimum */,
    Err(ValidationError::BadChar { ch })       => /* highlight the bad char */,
    // ... the compiler forces you to handle every case
}
```

That exhaustiveness is precisely what `Box<dyn Error>` (or a `String`) throws
away. Typed enum = the caller can branch; erased error = the caller can only print.

### 3. `source()`: the cause underneath

Most errors don't originate a failure — they *wrap* a lower-level one. "Failed to
load config" because "failed to parse integer". The `Error` trait models this with
the one optional method:

```rust
#[derive(Debug)]
enum ConfigError {
    Malformed { line: String },                      // leaf: no underlying cause
    BadPort   { source: std::num::ParseIntError },   // wraps the real cause
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::BadPort { source } => Some(source),  // &ParseIntError -> &dyn Error
            Self::Malformed { .. }   => None,          // == the default
        }
    }
}
```

Two things make this click:

- **`Some(source)` works because `&ParseIntError` coerces to `&dyn Error`.** Same
  unsizing coercion as rung 1, just behind a reference.
- **Display must NOT restate the source.** `BadPort`'s `Display` says only
  `"invalid port number"` — it does *not* paste in the `ParseIntError`'s text.

> **The separation rule.** `Display` answers *what failed at this layer*.
> `source()` answers *why*. Keep them disjoint. If you bake the cause's message
> into `Display`, every chain printer (rung 8, rung 9, `anyhow`) prints it twice.
> This is the single most important habit on this page.

### 4. `From` + `?`: how `#[from]` actually works

In rung 3 you wrapped the cause manually with `.map_err(|source| BadPort { source })`.
The `?` operator can do that conversion for you — but only if you teach it how.
The desugaring of `expr?` is roughly:

```rust
match expr {
    Ok(v)  => v,
    Err(e) => return Err(From::from(e)),   // <- the magic line
}
```

`?` calls `From::from` on the error before returning it. So if you implement
`From<TheLowLevelError> for YourError`, `?` will silently convert and propagate:

```rust
impl From<std::io::Error>          for LoadError { fn from(e: std::io::Error)          -> Self { Self::Io(e) } }
impl From<std::num::ParseIntError> for LoadError { fn from(e: std::num::ParseIntError) -> Self { Self::Parse(e) } }

fn load_count(raw: &str) -> Result<u64, LoadError> {
    if raw.is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "empty input"))?;
    }
    let n = raw.parse::<u64>()?;   // ParseIntError -> LoadError, no .map_err
    Ok(n)
}
```

`load_count` returns `Result<_, LoadError>` but `?`-es values whose error types are
`io::Error` and `ParseIntError`. It compiles *only* because the two `From` impls
exist. Delete one and that `?` stops compiling — that exact coupling is what
`thiserror`'s `#[from]` attribute generates for you.

### 5. The bounds hiding inside `Box<dyn Error>`

`Box<dyn Error>` is the lazy form. The type the wider ecosystem actually wants —
and what `fn main() -> Result<(), Box<dyn Error>>` and `anyhow::Error` use — is:

```rust
type BoxedSendSync = Box<dyn Error + Send + Sync + 'static>;
```

`Send` means the error can move to another thread; `Sync` means `&error` can be
shared across threads. An error that can't cross threads is useless to a threaded
server or an async runtime. The footgun: those bounds **propagate into every
field**. This struct can't become a `BoxedSendSync`:

```rust
#[derive(Debug)]
struct NotThreadSafe { detail: Rc<str> }   // Rc is !Send + !Sync
```

```text
error[E0277]: `Rc<str>` cannot be sent between threads safely
   = note: required for the cast from `Box<NotThreadSafe>`
           to `Box<dyn Error + Send + Sync + 'static>`
```

The fix isn't to the signature — it's to the *payload*. Swap `Rc<str>` for
`Arc<str>` (atomically reference-counted, and `Send + Sync`) and both the plain
and the send-sync boxing compile, and the boxed error survives `thread::spawn`.

> `+ Send + Sync` is not ceremony. It's a thread-mobility promise that the
> auto-traits force every field of your error to keep.

### 6. Downcasting: get the concrete type back

`Box<dyn Error>` erases the type. Sometimes you need it back — "if the root cause
was specifically a `ParseIntError`, retry; otherwise give up." `dyn Error` has two
inherent methods (built on `Any`) for this:

```rust
err.is::<T>()           -> bool        // is the concrete type T?
err.downcast_ref::<T>() -> Option<&T>  // borrow it as T if so
```

These work because `Error: 'static`, so every error carries a `TypeId`. Combine
downcasting with the source-chain walk to find and classify the root cause:

```rust
fn describe_root(top: &(dyn Error + 'static)) -> String {
    let mut cur = top;
    while let Some(next) = cur.source() { cur = next; }  // walk to the bottom
    cur.to_string()
}

fn root_is_parse_error(top: &(dyn Error + 'static)) -> bool {
    let mut cur = top;
    while let Some(next) = cur.source() { cur = next; }
    cur.is::<std::num::ParseIntError>()                   // decide on the concrete type
}
```

`source()` gives you the *next link*; loop it to reach the *root*; `is`/`downcast_ref`
recover the *concrete type* so you can branch. This is exactly how
`anyhow::Error::downcast_ref` and retry-on-specific-error logic work.

### 7. Backtraces: capture *where* it failed

A source chain is the *logical* why (X because Y because Z). A backtrace is the
*physical* where — the call stack at the instant the error was created. You attach
one with `std::backtrace::Backtrace`:

```rust
#[derive(Debug)]
struct TracedError { msg: String, backtrace: Backtrace }

impl TracedError {
    fn new(msg: impl Into<String>) -> Self {
        Self { msg: msg.into(), backtrace: Backtrace::force_capture() }
    }
    fn backtrace(&self) -> &Backtrace { &self.backtrace }  // inherent getter
}
```

Two APIs, and the difference matters:

| API | Behavior | When |
|-----|----------|------|
| `Backtrace::capture()` | Respects `RUST_BACKTRACE` / `RUST_LIB_BACKTRACE`; if unset, returns a cheap *disabled* backtrace (`status() == Disabled`) | Real libraries — zero cost unless the user opts in |
| `Backtrace::force_capture()` | Always walks the stack, ignoring env vars (expensive) | When you truly always want it (and for deterministic tests) |

Note the getter is an **inherent method**, not a trait override. `Error::backtrace`
exists but is still **unstable** on stable Rust, so real crates (and this rung)
expose their own `fn backtrace(&self) -> &Backtrace` instead. And `Display` writes
*only the message* — a backtrace is diagnostic data you render separately via
`format!("{}", e.backtrace())`, never baked into the human message.

### 8. The layered library error + a chain printer

This is the shape a real library ships: one public enum whose variants each wrap a
*different* lower-level error, a correct `source()` exposing every cause, and a way
to render the whole chain. The domain here is a three-level chain:

```text
AppError::Config  ->  ConfigError::BadPort  ->  ParseIntError
   (your enum)          (your enum)              (std)
```

```rust
#[derive(Debug)]
enum AppError {
    Read   { path: String, source: std::io::Error },
    Config { source: ConfigError },
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            AppError::Read { source, .. } => Some(source),
            AppError::Config { source }   => Some(source),
        }
    }
}
```

The `anyhow`-style `{:#}` printer flattens the chain into one line by walking
`source()` and joining each level's `Display` with `": "`:

```rust
fn format_chain(err: &dyn Error) -> String {
    let mut chain = err.to_string();
    let mut cur = err.source();
    while let Some(next) = cur {
        chain.push_str(&format!(": {next}"));
        cur = next.source();
    }
    chain
}
// "invalid configuration: invalid port number: number too large to fit in target type"
```

The payoff lands here: because *every* layer kept its `Display` high-level and
pushed detail down into `source()`, the printer renders the full three-level story
with **zero duplication**. The separation rule you adopted in rung 3 is what makes
this clean.

## Footguns

- **Forgetting `impl Error`.** `Display` alone is not an error. The empty
  `impl Error for T {}` is the marker that unlocks `Box<dyn Error>`, `?`, and
  downcasting. The compile error points at the `Box::new` coercion, not the impl.
- **`Error` needs `Debug`.** `trait Error: Debug + Display` — both supertraits are
  mandatory. Missing `#[derive(Debug)]` makes `impl Error` itself fail to compile.
- **Duplicating the cause in `Display`.** If `BadPort`'s `Display` says
  `"invalid port: {source}"`, every chain printer prints the parse error twice.
  Keep `Display` to *this* layer; let `source()` carry the rest. (Rung 4's
  `LoadError` deliberately violates this with `write!(f, "io error: {e}")` — fine
  in isolation, but it would double-print under a `format_chain`-style walk.)
- **`Rc` in a `Send + Sync` error.** The auto-trait bounds propagate into fields.
  An `Rc<_>` (or `RefCell<_>`, `*const _`, etc.) anywhere inside makes the whole
  error `!Send`/`!Sync` and uncoercible to `Box<dyn Error + Send + Sync>`. Reach
  for `Arc` / thread-safe payloads.
- **Lifetime on `dyn Error + 'static`.** A function returning a borrow of a
  `&(dyn Error + 'static)` has *two* lifetimes in play (the reference and the
  `'static` bound), so elision can't pick — you must name it:
  `fn chain<'a>(err: &'a (dyn Error + 'static)) -> Chain<'a>`.
- **Reaching for unstable `Error::backtrace`.** It doesn't exist on stable. Expose
  an inherent getter instead.

## Real-world patterns

- **`thiserror` = this whole file, generated.** `#[derive(Error)]` writes the
  `Display` (`#[error("...")]`), the `source()` (`#[source]` / `#[from]` fields),
  and the `From` impls (`#[from]`). Doing it by hand once means you know exactly
  what the macro emits and can debug it when it surprises you.
- **`anyhow` / `eyre` = rungs 6, 8, 9 packaged.** `anyhow::Error` is essentially a
  `Box<dyn Error + Send + Sync>` plus a captured backtrace, with `.context()` to
  push new layers, `.downcast_ref::<T>()` for recovery, `{:#}` for the one-line
  chain, and `{:?}` for the multi-line `Caused by:` report.
- **`std::error::Error::sources()`** (still unstable) is exactly the `Chain`
  iterator you build in the capstone.
- **Library/app split:** libraries expose a typed enum (callers can match);
  applications collapse everything into `anyhow::Error` (callers only report). The
  typed error survives *inside* the erased one and can be recovered by downcast.

## Capstone insight

`anyhow`'s rich error report — the thing that prints

```text
invalid configuration

Caused by:
    0: invalid port number
    1: number too large to fit in target type
```

— is built from *only the two trait methods you implemented in rungs 1 and 3*.
The capstone proves it by rebuilding the two reusable pieces:

```rust
// A: a lazy iterator over the source chain (std's unstable Error::sources()).
struct Chain<'a> { next: Option<&'a (dyn Error + 'static)> }

impl<'a> Iterator for Chain<'a> {
    type Item = &'a (dyn Error + 'static);
    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next.take()?;   // ? ends iteration at the root
        self.next = current.source();      // advance via the ONE trait method
        Some(current)
    }
}

// B: a Display wrapper that renders the multi-line report, built ON the iterator.
impl fmt::Display for Report<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)?;
        if self.0.source().is_some() {
            write!(f, "\n\nCaused by:")?;
            for (i, src) in chain(self.0).skip(1).enumerate() {
                write!(f, "\n    {i}: {src}")?;   // index 0 = FIRST cause, not the top
            }
        }
        Ok(())
    }
}
```

The `take()?` in `next()` is the elegant core: it yields the current error and
ends the iteration the moment `source()` returns `None` — the chain walk you wrote
three times by hand, now a normal `Iterator` you can `.skip(1)`, `.enumerate()`,
`.count()`, or `.collect()`. Every "rich error" experience in the ecosystem reduces
to `Display` + `source()` plus an iterator over them. That's the entire concept,
owned end to end.

## Explain it back

- Why does an *empty* `impl Error for T {}` matter — what stops compiling without it?
- What two supertraits must every `Error` already satisfy, and why does that force
  `#[derive(Debug)]`?
- What does `?` actually call on the error value before returning it, and what must
  you implement so a foreign error type propagates into your enum?
- Why must `Display` *not* include the text of `source()`? What breaks if it does?
- Why can't an error containing an `Rc<str>` become a `Box<dyn Error + Send + Sync>`,
  and what's the one-word payload fix?
- How do you recover the concrete type from a `&dyn Error`, and why is `Error: 'static`
  what makes that possible?
- `capture()` vs `force_capture()` — which respects `RUST_BACKTRACE`, and which one
  do libraries use by default?
- Sketch the `Chain` iterator's `next()`. Why does `self.next.take()?` correctly end
  the iteration at the root?

## See also

- [Error handling architecture](error-arch.md) — `thiserror` vs `anyhow`, the
  architecture layer built on top of this machinery.
- [Conversion traits](conversions.md) — `From` / `Into` and how `?` leans on them.
- [Box & the Heap](box-heap.md) — `Box<dyn Trait>` and unsizing coercions.
- [Static vs dynamic dispatch](dispatch.md) — what `dyn Error` is and how the
  vtable works.
