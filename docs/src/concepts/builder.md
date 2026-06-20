# Builder pattern

> Ladder: [`src/bin/builder.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/builder.rs) ·
> Run: `cargo run --bin builder` · Phase 3 · 8 rungs

## TL;DR

A **builder** is a half-built value that accumulates configuration through a
fluent method chain, then `build()` turns it into the real, validated thing.
It exists because a single `new(a, b, c, d, e)` constructor stops scaling the
moment a type has many fields — some optional, some defaulted, some mutually
constrained. The builder splits construction into named steps so callers set
only what they care about, and gives *you* exactly one place (`build()`) to
apply defaults and reject invalid combinations.

The two axes that define every builder:

1. **Ownership of the setter receiver** — `self` by value (consuming) vs
   `&mut self` (mutating). This decides whether the builder is reusable and how
   it chains.
2. **When validation happens** — never, at runtime (`build() -> Result`), or at
   compile time (typestate, where `build()` doesn't even exist until required
   fields are set).

## Why this exists (from first principles)

Start with the problem. A telescoping constructor:

```rust
// What we're trying to avoid:
HttpRequest::new("POST", "https://x.com", "body", true, 30, vec![], None)
//               ^ which arg is which? what's `true`? what's `30`?
```

Three things break here as the type grows:

- **Unreadable call sites.** Positional args give no clue what each value means,
  and `true, true, false` is a bug waiting to happen.
- **No optionality.** Every caller must pass every argument, even the ones they
  don't care about. Adding a field is a breaking change to every call site.
- **Scattered validation.** If "port must be non-zero" matters, *every*
  constructor and setter has to re-check it, or callers can build nonsense.

The builder fixes all three: named setters document intent, unset fields fall
back to defaults, and `build()` is the single funnel where validity is decided.
The cost is a second type (the builder) and a small amount of boilerplate — which
is exactly what `#[derive(Builder)]` macros (the `derive_builder` crate) exist to
erase.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | Foundations | Consuming builder | `self` by value, `build(self) -> T`; chains because it returns `Self` |
| 2 | Foundations | `&mut self` builder | borrow-and-return; reusable, but `build(&self)` must clone |
| 3 | Mechanics | Optionals & defaults | `Option<T>` fields collapse to defaults in `build()` |
| 4 | Mechanics | Fallible build | required/invalid fields ⇒ `build() -> Result<T, E>` |
| 5 | Footgun | Temporary-drop trap | E0716 on a captured `&mut` chain; the owning-binding fix |
| 6 | Real-world | Repeatable setters + `Into` | accumulate into `Vec`/`HashMap`; `impl Into<String>` args |
| 7 | Real-world | Typestate builder | markers in the type; `build()` only on `<Yes, Yes>` |
| 8 | Capstone | Real config builder | consume + `Into` + optionals + repeatable + validated build |

## The ideas, built up

### 1. The consuming builder: chaining falls out of the signature

The whole pattern hinges on one signature shape: a setter takes `self` **by
value** and returns `Self`.

```rust
fn method(self, m: &str) -> Self {
    HttpRequestBuilder { method: m.to_string(), ..self }
}

fn build(self) -> HttpRequest {
    HttpRequest { method: self.method, url: self.url, body: self.body }
}
```

Because each setter *consumes and returns* the builder, the only thing you can
do with the result is call the next method on it — that is what makes
`Builder::new().method(..).url(..).build()` read as one fluent chain.

Two details worth internalizing:

- **`..self` (functional update syntax)** moves the remaining fields out of the
  old builder into the new one. That's sound *precisely because* you own `self`
  by value and are discarding the old builder — "I own it, so I can dismantle
  it." A `&self`/`&mut self` setter could not do this.
- **`build` moves fields out for free.** `self.method` is a move, not a clone,
  because `build` consumed `self`. Hold onto this — rung 2 pays a clone tax for
  giving it up.

### 2. The `&mut self` builder: reusable, at the cost of a clone

The mirror-image choice: borrow `&mut self`, mutate one field, return
`&mut Self`. `build` then takes `&self`.

```rust
fn method(&mut self, m: &str) -> &mut Self {
    self.method = m.to_string();
    self // auto-reborrows as &mut Self
}

fn build(&self) -> HttpRequest {
    HttpRequest {
        method: self.method.clone(), // must clone — only a shared borrow
        url: self.url.clone(),
        body: self.body.clone(),
    }
}
```

What this buys you, that rung 1 cannot:

```rust
let mut b = ReqBuilder::new();
b.url("https://reuse.test").method("GET");
let r1 = b.build();          // builder still alive
b.method("DELETE");
let r2 = b.build();          // build again, tweaked
```

The builder *survives* `build()`, so you can build twice, or conditionally set
fields across statements. The price: `build(&self)` only has a shared borrow, so
it cannot move the fields out — it must **clone** them. That is the fundamental
trade between the two styles.

| | Consuming (`self`) | Mutating (`&mut self`) |
|---|---|---|
| `build` cost | moves fields (free) | clones fields |
| Reusable after build | no (consumed) | yes |
| Chains as one expression | yes | yes |
| Capture partial chain in a `let` | yes (owns `Self`) | **no** — see rung 5 |

### 3. Optionals & defaults: model "unset" honestly

A real builder lets callers set only what they care about. Model that directly:
make every builder field an `Option`, starting `None`; a setter stores `Some`;
`build()` resolves each `None` to a default.

```rust
#[derive(Default)]              // gives you new() = Self::default() for free
struct ServerOptsBuilder {
    host: Option<String>,
    port: Option<u16>,
    // ...
}

fn build(&self) -> ServerOpts {
    ServerOpts {
        host: self.host.clone().unwrap_or_else(|| "127.0.0.1".to_string()),
        port: self.port.unwrap_or(8080),
        // ...
    }
}
```

The key split: **`Option`-ness lives only in the builder.** The finished
`ServerOpts` has plain `String`/`u16` fields — `build()` is where "the caller
never set this" collapses into a concrete value.

> **`unwrap_or` vs `unwrap_or_else`.** `unwrap_or(x)` evaluates `x`
> *eagerly*, every time, even when the `Option` is `Some`. For the cheap `Copy`
> default `8080` that's fine. For `host`, `unwrap_or("127.0.0.1".to_string())`
> would allocate that string on *every* build even when the host was set. Use
> `unwrap_or_else(|| ...)` so the default is computed only on the `None` path.

### 4. Fallible build: one validation checkpoint

Defaults cover fields that *have* a sensible default. But some fields are
genuinely required (a `Connection` with no `name`), and some values are invalid
(`port == 0`). The builder can't stop a caller from leaving `name` unset — so
`build()` becomes the single checkpoint that returns `Result`.

```rust
fn build(&self) -> Result<Connection, BuildError> {
    let port = self.port.unwrap_or(8080);
    if port == 0 {
        return Err(BuildError::InvalidPort);
    }
    Ok(Connection {
        name: self.name.clone().ok_or(BuildError::MissingName)?,
        port,
        retries: self.retries.unwrap_or(3),
    })
}
```

- `ok_or(err)` turns `Option<T>` into `Result<T, E>`; the `?` then early-returns
  the error and unwraps the `String` on the happy path.
- Resolve a default *then* validate (`unwrap_or(8080)` before the `== 0` check).

The lesson: no matter how the caller chained the builder, every path funnels
through this one function. Invalid states are caught in exactly one place.

### 5. The temporary-drop footgun (and its fix)

The `&mut self` builder chains fine in one expression, because the temporary
builder lives until the end of the statement. The trap appears when you try to
capture a partially-built `&mut` builder in a `let`:

```rust
// WRONG — E0716 "temporary value dropped while borrowed"
let builder = ConnectionBuilder::new().name("db").port(5432);
let conn = builder.build().unwrap();
```

Why it fails: `new()` produces a *temporary* `ConnectionBuilder`. `.name().port()`
return `&mut` references *into* that temporary. At the `;`, the temporary is
dropped — so `builder` would be a reference to freed memory. The borrow checker
refuses.

The fix: give the builder an **owning binding** first, then call setters on it.
Now the references the setters return are created and dropped within each
statement, while the owner `b` stays alive.

```rust
// OK
let mut b = ConnectionBuilder::new();
b.name("svc");
b.port(5432);
if many_retries { b.retries(10); } // the across-statements case &mut excels at
b.build()
```

This is *the* defining difference between the two styles: the consuming builder
(which returns owned `Self`) can be freely split across `let` bindings; the
`&mut` builder cannot, because its intermediate values are borrows, not values.

### 6. Repeatable setters + `Into` bounds: real-world ergonomics

Two tricks every production builder uses.

**Repeatable setters accumulate instead of overwrite.** The field is a
collection; the setter pushes/inserts. Calling `.to(..)` three times appends
three entries (this is exactly how `reqwest::RequestBuilder::header` works).

```rust
fn to(&mut self, addr: impl Into<String>) -> &mut Self {
    self.to.push(addr.into());     // append, don't replace
    self
}
fn header(&mut self, k: impl Into<String>, v: impl Into<String>) -> &mut Self {
    self.headers.insert(k.into(), v.into());
    self
}
```

**`impl Into<String>` arguments** let callers pass `&str` *or* `String` (or
anything convertible) without sprinkling `.to_string()` at every call site. You
call `.into()` once inside the setter to normalize to the owned type.

```rust
b.to("a@x.com")        // &str
 .to(owned_string)     // String — Into<String> covers both
 .header("X-Env", "prod");
```

### 7. Typestate: make a missing field a *compile* error

Rung 4 caught a missing required field at runtime (`Err(MissingName)`). Typestate
moves that check into the type system: `build()` simply **does not exist** until
every required field is set.

Encode "is this field set?" in a generic type parameter, using zero-sized marker
types. Each required setter *returns a different type* with its marker flipped
from `No` to `Yes`.

```rust
struct Yes;
struct No;

struct ApiCallBuilder<E, T> {   // E = endpoint-set?, T = token-set?
    endpoint: Option<String>,
    token: Option<String>,
    timeout_ms: Option<u64>,
    _state: PhantomData<(E, T)>,
}

impl ApiCallBuilder<No, No> {
    fn new() -> Self { /* both markers start at No */ }
}

impl<E, T> ApiCallBuilder<E, T> {
    // flips E -> Yes, THREADS T through unchanged
    fn endpoint(self, e: &str) -> ApiCallBuilder<Yes, T> {
        ApiCallBuilder { endpoint: Some(e.to_string()), token: self.token,
                         timeout_ms: self.timeout_ms, _state: PhantomData }
    }
    fn token(self, t: &str) -> ApiCallBuilder<E, Yes> { /* flips T, threads E */ }
}

// build() EXISTS ONLY for the fully-set type:
impl ApiCallBuilder<Yes, Yes> {
    fn build(self) -> ApiCall {
        ApiCall {
            endpoint: self.endpoint.unwrap(), // .unwrap() is HONEST here —
            token: self.token.unwrap(),       // the <Yes,Yes> bound proves Some
            timeout_ms: self.timeout_ms.unwrap_or(30_000),
        }
    }
}
```

The result:

```rust
let bad = ApiCallBuilder::new().endpoint("x").build();
// error[E0599]: no method named `build` found for ApiCallBuilder<Yes, No>
```

Three things make this work:

- **`PhantomData<(E, T)>`** lets you carry the marker type-params without storing
  any value of them — they're compile-time-only state.
- **Setters are generic over the *other* marker** (`endpoint` is `impl<E, T>`,
  returns `<Yes, T>`). Threading `T` through unchanged is what remembers "token
  was already set" across the call.
- **Because the return type differs from `Self`, you can't use `..self`** — the
  source and target are different types, so you move each field across by hand.

The payoff: `.unwrap()` in `build()` is provably correct. The type
`<Yes, Yes>` is a proof that both fields are `Some`, so there is no runtime check
left to do — the compiler already did it.

## Footguns

| Footgun | What bites | Fix |
|---|---|---|
| Capturing a `&mut` chain in a `let` | E0716, temporary dropped while borrowed (rung 5) | bind the builder to an owner first, then call setters |
| `unwrap_or` for an allocating default | allocates on *every* build, even when the field was set (rung 3) | `unwrap_or_else(\|\| ...)` — lazy |
| Forgetting `build` consumes `self` in typestate | can't reuse the builder after `build()` | intended — typestate transitions are one-shot |
| Repeatable setter that assigns instead of pushes | silently overwrites previous values | `.push` / `.insert`, never `=` |
| `&mut self` `build(&self)` | must clone every field out of a shared borrow | use the consuming style if you want moves |

## Real-world patterns

- **`Foo::builder()` entry point.** Rather than a free `FooBuilder::new()`, expose
  a `Foo::builder()` associated function — it's discoverable from the type you
  actually want and reads as `Foo::builder()...build()` (std/`tokio`/`reqwest` all
  do this).
- **Consuming style for one-shot config, `&mut` for reuse.** `std::process::Command`
  uses `&mut self` (so you can conditionally `.arg(..)` in a loop);
  `reqwest::ClientBuilder` consumes. Pick by whether callers need to reuse the
  builder.
- **Repeatable setters + `Into` everywhere** is the house style of HTTP/builder
  crates: `.header(k, v)` accumulates, all string args are `impl Into<String>`.
- **`derive_builder` / `bon`** generate all of this from the struct definition.
  Knowing the hand-rolled shape is what lets you read and debug the macro output.
- **Typestate (`bon`'s required fields, embedded HALs)** for APIs where a missing
  step should be a compile error, not a runtime panic.

## Capstone insight

The capstone (`ServerConfig::builder()`) fuses every rung into one idiomatic API:
a consuming fluent chain, `impl Into<String>` args, `Option` fields with defaults,
repeatable `.route(..)`/`.env(k, v)` setters, and a single fallible `build()` that
validates everything and ends the chain with `.build()?`.

The structural "aha": because `build` **consumes `self`**, it can *move*
`routes` and `env` straight into the finished `ServerConfig` — no clone, unlike
the `&mut`-style `build(&self)` of rungs 2 and 6.

```rust
fn build(self) -> Result<ServerConfig, ConfigError> {
    let bind_addr = self.bind_addr.ok_or(ConfigError::MissingBindAddr)?;
    let port = self.port.unwrap_or(8080);
    let workers = self.workers.unwrap_or(4);
    if port == 0 { return Err(ConfigError::ZeroPort); }
    if workers == 0 { return Err(ConfigError::ZeroWorkers); }
    if self.routes.is_empty() { return Err(ConfigError::NoRoutes); }
    Ok(ServerConfig {
        bind_addr, port, workers,
        routes: self.routes,  // MOVED, not cloned — we own self
        env: self.env,        // MOVED
    })
}
```

That `ok_or(...)?` for the required field plus the move-not-clone of the
collections *is* the senior-Rustacean shape of a builder. Everything else —
defaults, validation, repeatable setters — hangs off those two decisions:
**who owns the receiver**, and **where validity is decided**.

## Explain it back

- Why does a setter return `Self`/`&mut Self` at all? What would break if it
  returned `()`?
- What exactly does `..self` do, and why is it sound only in the consuming style?
- Why does `build(&self)` in the `&mut` builder have to clone, while `build(self)`
  in the consuming builder can move?
- Reproduce the E0716 temporary-drop error from memory. Why does an owning `let`
  binding fix it?
- When should a setter accumulate (`.push`) vs overwrite (`=`)? Give an example
  of each.
- In the typestate builder, why is `endpoint`'s impl block `impl<E, T>` and not
  `impl ApiCallBuilder<No, No>`? What does threading `T` through accomplish?
- Why is `.unwrap()` in the typestate `build()` not a code smell?
- `unwrap_or` vs `unwrap_or_else` — when does the difference actually matter?

## See also

- [Custom error types](custom-errors.md) — the `BuildError`/`ConfigError` enums
  the fallible builds return.
- [Error handling architecture](error-arch.md) — `Result`, `?`, and where
  validation errors belong.
- [Conversion traits](conversions.md) — `Into`/`From`, the engine behind
  `impl Into<String>` setters.
