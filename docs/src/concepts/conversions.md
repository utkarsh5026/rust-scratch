# Conversion traits — `From` / `Into`, `TryFrom` / `TryInto`, `AsRef` / `AsMut`

> Ladder: [`src/bin/conversions.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/conversions.rs) ·
> Run: `cargo run --bin conversions` · Phase 1 · 9 rungs

## TL;DR

Type conversion in Rust is a small family of traits, split on **two questions**:
*can it fail?* and *do you consume or just borrow?*

|  | infallible | fallible |
|---|---|---|
| **take ownership** | `From` / `Into` | `TryFrom` / `TryInto` |
| **just borrow** | `AsRef` / `AsMut` | — |

The unlock that makes the whole family small: **you only ever implement `From`
and `TryFrom`.** The `Into` and `TryInto` directions are handed to you for free
by blanket impls. And the `?` operator converts error types through `From`, so
making heterogeneous errors collapse into one type is *also* just writing `From`
impls. Almost everything on this page falls out of those two facts.

## Why this exists (from first principles)

A conversion is a function `A -> B`. You *could* just write free functions
(`celsius_to_fahrenheit`, `string_from_char`, ...) and be done. The reason Rust
lifts conversions into **traits** is that traits are how you write code generic
over "anything convertible." Once conversion is a trait, a function can say
"give me anything that becomes a `String`" and the compiler wires up the right
conversion at each call site. Free functions can't do that.

But one trait isn't enough, because conversions differ along two independent
axes that the type system has to respect:

1. **Can it fail?** Turning a `Celsius` into a `Fahrenheit` always succeeds —
   the result type can hold any value. Turning an `i32` into a `u8` *cannot*
   always succeed: `300` doesn't fit. An infallible conversion returns `B`; a
   fallible one must return `Result<B, E>`. You cannot model both with one
   signature, so the family splits into `From` (returns `Self`) and `TryFrom`
   (returns `Result<Self, Self::Error>`).

2. **Do you need to own the input?** Producing a `String` from a `&str`
   **allocates** and consumes nothing it can't recreate — that's `From`/`Into`,
   which take the value by move. But a function that only *reads* text shouldn't
   demand ownership or force a clone. It just needs a `&str` view of whatever you
   have. That's `AsRef`: a cheap, non-consuming "give me a `&T` of yourself."

What the compiler guarantees, given these traits: conversions are **explicit and
type-directed**. There's no silent coercion between unrelated types — you either
call `.into()`/`.try_into()` (and the target type drives which impl runs) or you
get a compile error. The one infamous exception is the `as` keyword, which is
*not* a trait and silently truncates — rung 6 is about why you should reach for
`TryInto` instead.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `From` basics | impl `From<Celsius>` -> `.into()` comes free via the blanket impl |
| 2 | foundations | `Into` bounds | `impl Into<String>` params accept `&str`, `String`, `char`... convert once at the boundary |
| 3 | mechanics | `From` powers `?` | `?` inserts `From::from` on the error -> many error types collapse into one |
| 4 | mechanics | `TryFrom` | fallible construction with an associated `Error`; `try_into()` comes free |
| 5 | footgun | reflexivity & orphan rule | `From<T> for T` is identity; you can't impl a foreign trait for a foreign type -> newtype |
| 6 | footgun | `as` vs `TryInto` | `as` silently wraps (`300 as u8 == 44`); `TryInto<u8>` catches the overflow |
| 7 | real-world | `AsRef<str>` / `AsRef<[u8]>` | accept many types *by reference*, no allocation — the stdlib API shape |
| 8 | real-world | `AsRef<Path>` + `AsMut` | the `File::open` trick; `AsMut` for an in-place mutable view |
| 9 | capstone | mini JSON `Value` | `From` in (infallible), `AsRef<str>` lookup, `TryFrom` out (fallible) |

## The ideas, built up

### `From` is the one you implement; `Into` is the one you get

Implement `From` in one direction and the reverse `.into()` appears for free:

```rust
struct Celsius(f64);
struct Fahrenheit(f64);

impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self {
        Fahrenheit(c.0 * 9.0 / 5.0 + 32.0)
    }
}
```

Both of these call the **same** impl:

```rust
let f1 = Fahrenheit::from(Celsius(100.0));   // explicit From
let f2: Fahrenheit = Celsius(0.0).into();    // .into() — free, type-driven
```

You never write `impl Into<Fahrenheit> for Celsius`. The stdlib has a blanket
impl that derives it from your `From`:

```rust
impl<T, U: From<T>> Into<U> for T { /* calls U::from(self) */ }
```

This is the rule to memorize: **implement `From`, callers enjoy `Into`.** The
asymmetry exists because the blanket impl only flows one way — `From` -> `Into` —
and (historically) you couldn't even `impl Into` for a foreign type. `From` is
always the right thing to write.

Notice in `f2` the conversion is driven by the **target type annotation**
(`let f2: Fahrenheit`). `.into()` is "convert into *something*"; the compiler
figures out which `From` impl from the type it's assigned to. No annotation, no
resolution.

### `Into` bounds make APIs ergonomic — convert once at the boundary

The real reason `Into` matters at the *call* site: a parameter typed
`impl Into<String>` accepts anything that knows how to become a `String`, and
the function converts exactly once, at the boundary.

```rust
struct Tag { name: String }

impl Tag {
    fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}
```

One function, three different argument types, zero clones written by the caller:

```rust
let a = Tag::new("literal");              // &'static str
let b = Tag::new(String::from("owned"));  // String — a no-op conversion
let c = Tag::new('x');                    // char -> String!
```

The `String -> String` case is free because of the reflexive impl (rung 5): it's
a real but no-op `From`. So you pay nothing for the flexibility when the caller
already has the owned type.

> **Rule of thumb:** put `impl Into<T>` (or `T: From<X>`) on the **caller side**
> of a generic boundary when the function needs to **store an owned `T`**. If it
> only needs to *read* the data, use `AsRef` instead (rung 7) — don't take
> ownership you don't need.

### `From` powers the `?` operator — the most important fact here

This is why `From` matters more than any other trait on the page. When you write
`?` on a `Result` whose error type doesn't match the function's return error
type, the compiler inserts `.map_err(From::from)` for you. So you make
heterogeneous errors flow into **one** error type just by implementing `From` for
each source error.

```rust
#[derive(Debug, PartialEq)]
enum ConfigError {
    NotANumber(ParseIntError),
    OutOfRange(i32),
}

impl From<ParseIntError> for ConfigError {
    fn from(error: ParseIntError) -> Self {
        ConfigError::NotANumber(error)
    }
}

fn parse_config(s: &str) -> Result<i32, ConfigError> {
    let n: i32 = s.parse()?;                    // parse() errors with ParseIntError
    if !(0..=100).contains(&n) {
        return Err(ConfigError::OutOfRange(n)); // returned explicitly
    }
    Ok(n)
}
```

The `s.parse()?` line is the whole lesson. `parse()` returns
`Result<i32, ParseIntError>`, but the function returns `Result<_, ConfigError>`.
The `?` desugars roughly to:

```rust
let n = match s.parse() {
    Ok(v) => v,
    Err(e) => return Err(ConfigError::from(e)),   // From::from inserted here
};
```

Because you wrote `From<ParseIntError> for ConfigError`, that conversion exists
and the code compiles. This is the engine behind `anyhow`, `thiserror`, and
every hand-rolled error enum: `?` + `From` turns many failure types into one.

### `TryFrom` — when the conversion can fail

`From::from` returns `Self` — it has no way to signal failure. So when a
conversion can fail, `From` is simply the wrong trait. `TryFrom` is the fallible
twin: `fn try_from(v) -> Result<Self, Self::Error>`, with an **associated error
type** you choose.

```rust
struct Percent(i32);

#[derive(Debug, PartialEq)]
enum PercentError { OutOfRange(i32) }

impl TryFrom<i32> for Percent {
    type Error = PercentError;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        if value < 0 || value > 100 {
            return Err(PercentError::OutOfRange(value));
        }
        Ok(Percent(value))
    }
}
```

Exactly mirroring `From -> Into`, implementing `TryFrom` gives you `try_into()`
for free:

```rust
let p: Result<Percent, _> = 100.try_into();   // free from the TryFrom impl
```

Note you must annotate the target (`Result<Percent, _>`) so the compiler knows
which `TryInto` to pick — same type-direction rule as `.into()`. And `TryFrom`
composes with `?` because the error type already matches:

```rust
fn make(n: i32) -> Result<Percent, PercentError> {
    let p = Percent::try_from(n)?;   // ? works — error type is already PercentError
    Ok(p)
}
```

Here the `?` calls `From::from` on the error, but since it's `PercentError ->
PercentError` that's the reflexive identity — no conversion needed. Which leads
straight to rung 5.

### Reflexivity and the orphan rule — two coherence facts that bite

**(a) `impl<T> From<T> for T` exists in the stdlib.** Every type can "convert" to
itself — a no-op identity. This quietly makes three earlier things work:

- `?` works even when the error types already match (it calls `From::from`, which
  is identity here).
- `impl Into<String>` accepts a `String` at zero cost (`String: From<String>`).
- `u64::from(42u64)` is a real, if pointless, conversion.

```rust
let same = u64::from(42u64);   // identity From — a genuine impl, just a no-op
assert_eq!(same, 42);
```

**(b) The orphan rule (coherence).** You may implement a trait for a type only if
**the trait or the type is local to your crate**. So this is rejected:

```rust
// WRONG — both From and Duration are foreign to your crate:
// impl From<u64> for std::time::Duration { ... }   // E0117
```

Uncommenting that in the source produces `E0117 "only traits defined in the
current crate can be implemented for types defined outside of the crate."` You
*cannot* make it compile from this crate — that's the entire point. The rule
exists so two different crates can't write conflicting impls for the same
trait/type pair and break each other.

The universal fix: a **newtype** you own.

```rust
struct Timeout(Duration);   // a local type -> now you CAN impl From for it

impl From<u64> for Timeout {
    fn from(secs: u64) -> Self {
        Timeout(Duration::from_secs(secs))
    }
}

fn secs_to_timeout(secs: u64) -> Timeout {
    secs.into()   // resolves to your From<u64> for Timeout
}
```

Because `Timeout` is local, the orphan rule is satisfied and the impl is allowed.
This is also a second reason you implement `From` and never `Into`: the blanket
impl gives `Into` for free *and* historically you couldn't `impl Into` for a
foreign type at all.

### `as` truncates silently; `TryInto` is the checked path

`as` casts between numeric types and **never fails** — it silently
truncates/wraps. This is a notorious bug source:

```rust
let truncated = 300i32 as u8;   // == 44, no error, no warning
assert_eq!(truncated, 44);      // 300 - 256 = 44, wrapped around
```

The safe counterpart is `TryFrom`/`TryInto`, which returns `Err` when the value
doesn't fit. You can write a generic that narrows *anything* try-convertible into
a `u8`:

```rust
fn narrow<T: TryInto<u8>>(value: T) -> Result<u8, T::Error> {
    value.try_into()
}
```

Two things to unpack in that signature:

- **`T: TryInto<u8>`** — the bound is on the *caller's* type, accepting any
  integer type that knows how to *try* to become a `u8`.
- **`T::Error`** — the error type isn't named; it's the trait's associated type.
  Different source types may have different error types, and the return type
  tracks whichever one `T` brings.

```rust
assert!(narrow(300i32).is_err());     // doesn't fit -> Err (vs. `as` -> 44)
assert_eq!(narrow(200i32), Ok(200));  // fits
assert_eq!(narrow(200u32), Ok(200));  // different input type, same bound
assert!(narrow(-1i32).is_err());      // negative -> Err
```

This is exactly how the stdlib downcasts integers safely: `u8::try_from(x)` and
`x.try_into()`. Reach for them whenever a numeric narrowing could lose data.

### `AsRef` — cheap reference conversions, no allocation

`From`/`Into` consume a value and usually allocate. But often a function only
needs to *read* the data — it shouldn't demand ownership or force a clone.
`AsRef<T>` is the answer: a zero-cost "give me a `&T` view of myself." `&str`,
`String`, `&String`, `Box<str>` all impl `AsRef<str>`, so a single bound accepts
all of them **by reference**:

```rust
fn shout<S: AsRef<str>>(s: S) -> String {
    s.as_ref().to_uppercase()
}

fn byte_len<B: AsRef<[u8]>>(b: B) -> usize {
    b.as_ref().len()
}
```

The caller passes whatever it has, and a borrowed input stays usable afterward:

```rust
let owned = String::from("hi");
assert_eq!(shout(&owned), "HI");   // &String -> &str view
assert_eq!(owned, "hi");           // still usable: shout only borrowed it
```

`AsRef<[u8]>` is even broader — it unifies `&str`, `String`, `&[u8]`, `Vec<u8>`,
and arrays as a byte view:

```rust
assert_eq!(byte_len("abc"), 3);              // &str
assert_eq!(byte_len(vec![1u8, 2, 3]), 3);    // Vec<u8>
assert_eq!(byte_len([0u8; 5]), 5);           // [u8; 5]
```

> **`AsRef` vs `Into`, the decision:** use `impl Into<String>` when you need to
> **store an owned `String`** (rung 2). Use `impl AsRef<str>` when you only need
> to **look at the text** (rung 7). Taking ownership you don't need forces
> needless clones on the caller.

### `AsRef<Path>` (the `File::open` trick) and `AsMut`

The most famous `AsRef` in the stdlib is the signature of `File::open`:

```rust
fn open<P: AsRef<Path>>(path: P) -> io::Result<File>
```

That single bound is why `File::open("f.txt")`, `File::open(string)`, and
`File::open(path_buf)` all work — `&str`, `String`, `PathBuf`, and `&Path` all
impl `AsRef<Path>`. You write the bound once; callers pass whatever path-like
thing they hold. The ladder mirrors it:

```rust
fn extension<P: AsRef<Path>>(p: P) -> Option<String> {
    p.as_ref()
        .extension()                 // Option<&OsStr>
        .and_then(|e| e.to_str())    // Option<&str>
        .map(String::from)           // Option<String>
}
```

`AsMut` is the mutable mirror: `as_mut()` hands back a `&mut T` view, so one
function can mutate a `Vec`, an array, or a `&mut` slice in place:

```rust
fn double_all<T: AsMut<[i32]>>(mut data: T) -> T {
    data.as_mut().iter_mut().for_each(|x| *x *= 2);
    data
}

assert_eq!(double_all(vec![1, 2, 3]), vec![2, 4, 6]);  // Vec<i32>
assert_eq!(double_all([10, 20]), [20, 40]);            // [i32; 2]
```

`AsMut<[i32]>` abstracts over "anything that can lend a mutable `i32` slice,"
so the in-place algorithm is written once and works across container types.

## Capstone insight: data flows in infallibly, out fallibly

The capstone builds a mini `serde_json::Value` and wires the whole family
together — and the structural "aha" is the **asymmetry**:

> Data flows **into** a dynamic type **infallibly** (`From` — a `bool` always
> makes a valid `Value`). Data flows **out** **fallibly** (`TryFrom` — a `Value`
> might not be the type you asked for). That asymmetry is the entire reason both
> traits exist.

```rust
enum Value {
    Null, Bool(bool), Num(f64), Str(String),
    Array(Vec<Value>), Object(Vec<(String, Value)>),
}
```

**In, infallibly** — every Rust value maps to *some* valid `Value`:

```rust
impl From<bool> for Value   { fn from(b: bool) -> Self { Value::Bool(b) } }
impl From<i64> for Value    { fn from(n: i64)  -> Self { Value::Num(n as f64) } }
impl From<&str> for Value   { fn from(s: &str) -> Self { Value::Str(s.to_string()) } }
impl From<Vec<Value>> for Value { fn from(v: Vec<Value>) -> Self { Value::Array(v) } }
```

This makes construction ergonomic, even for heterogeneous nested data — every
element just `.into()`s:

```rust
let arr: Value = vec![1i64.into(), "two".into(), true.into()].into();
```

**Lookup, by `AsRef<str>`** — the key bound lets callers pass a `&str` *or* a
`String`:

```rust
fn get<S: AsRef<str>>(&self, key: S) -> Option<&Value> {
    let key = key.as_ref();
    if let Value::Object(object) = self {
        object.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    } else {
        None   // not an object -> no key
    }
}

obj.get("name");                  // &str key
obj.get(String::from("age"));     // String key — same function
```

**Out, fallibly** — extraction can disagree with the stored variant, so it
returns `Result`:

```rust
impl TryFrom<Value> for f64 {
    type Error = WrongType;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        if let Value::Num(n) = v { Ok(n) } else { Err(WrongType) }
    }
}

impl TryFrom<Value> for String {
    type Error = WrongType;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        if let Value::Str(s) = v { Ok(s) } else { Err(WrongType) }
    }
}
```

```rust
let name = String::try_from(obj.get("name").unwrap().clone()).unwrap();  // "ada"
let age: f64 = obj.get("age").unwrap().clone().try_into().unwrap();      // 36.0
assert_eq!(f64::try_from(Value::Bool(true)), Err(WrongType));            // wrong type -> Err
```

Once you see this, every dynamic/serialization boundary in Rust reads the same
way: `From` to build the loose representation, `TryFrom` to safely pull typed
values back out, `AsRef` to keep the read-side flexible.

## Footguns

- **`as` silently truncates/wraps numeric casts.** `300i32 as u8 == 44`, no
  warning. Use `u8::try_from(x)` / `x.try_into()` whenever a narrowing could lose
  data — they return `Err` instead of corrupting the value.

- **The orphan rule blocks `impl ForeignTrait for ForeignType`.** You can't
  `impl From<u64> for Duration` from your crate (`E0117`). Wrap the foreign type
  in a **newtype** you own and impl on that.

- **Implement `From`, never `Into`.** The blanket impl derives `Into` from your
  `From`. Writing `Into` by hand is redundant and was historically impossible for
  foreign types.

- **`.into()` / `.try_into()` need a known target type.** They convert "into
  *something*"; if the target isn't pinned by an annotation or the surrounding
  context, the compiler can't pick an impl. Annotate the binding or the return.

- **Taking ownership when you only read.** Using `impl Into<String>` where
  `impl AsRef<str>` would do forces callers to give up (or clone) their data.
  Match the bound to what the function actually needs: store -> `Into`, read ->
  `AsRef`.

- **`From` can't fail.** If a conversion has any invalid inputs, it must be
  `TryFrom`. Reaching for `From` and panicking inside is a code smell — return a
  `Result` instead.

## Real-world patterns

| Pattern | Trait | Example |
|---|---|---|
| Ergonomic constructor | `impl Into<String>` param | `Tag::new("x")`, builder APIs |
| Collapse many errors into one | `From<E>` + `?` | `anyhow`, `thiserror`, custom error enums |
| Validated construction | `TryFrom<Raw>` | `Percent::try_from(150) -> Err`, `Ipv4Addr::try_from(bytes)` |
| Safe numeric narrowing | `u8::try_from` / `TryInto<u8>` | downcasting integers without `as` |
| Read-only string/byte arg | `impl AsRef<str>` / `AsRef<[u8]>` | `str` helpers, hashing, parsing |
| Path-like argument | `impl AsRef<Path>` | `File::open`, `fs::read`, `Path::join` |
| In-place mutation over containers | `impl AsMut<[T]>` | generic slice transforms |
| Dynamic value boundary | `From` in, `TryFrom` out, `AsRef` lookup | `serde_json::Value`, config trees |

## Explain it back

- Why do you only ever implement `From`, and where does `.into()` come from?
- What does `?` insert on the error path, and which trait must you implement to
  make a foreign error type flow into your error enum?
- When is `From` the wrong choice, and what's the fallible replacement? What does
  its associated `Error` type let you control?
- Why does `?` compile even when the error types already match? (Which std impl?)
- State the orphan rule in one sentence. Why can't you `impl From<u64> for
  Duration`, and what's the standard fix?
- `300i32 as u8` is what, and why? What should you write instead, and what does
  it return on overflow?
- You have a function that only needs to read a string. `impl Into<String>` or
  `impl AsRef<str>` — which, and why does it matter to the caller?
- Why is `File::open`'s `P: AsRef<Path>` bound so convenient? Name three types
  that satisfy it.
- In the JSON `Value` capstone, why is construction `From` but extraction
  `TryFrom`? What does that asymmetry reflect about the data?

## See also

- [`Borrow` / `ToOwned`](borrow-toowned.md) — `AsRef`'s cousins; `Borrow` adds
  an `Eq`/`Hash` contract that `AsRef` doesn't, which is why `HashMap` keys use
  `Borrow`, not `AsRef`
- [`Cow` — Clone-on-Write](cow.md) — pairs with `Into`/`AsRef` for APIs that
  borrow when they can and own when they must
- [`Box` & the Heap](box-heap.md) — `Box<dyn Error>` is the other half of the
  `?`/`From` error-conversion story
