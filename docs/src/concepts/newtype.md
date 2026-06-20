# Newtype & zero-cost wrappers

> Ladder: [`src/bin/newtype.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/newtype.rs) ·
> Run: `cargo run --bin newtype` · Phase 3 · 9 rungs

## TL;DR

A **newtype** is a one-field tuple struct that wraps an existing type:
`struct Meters(f64)`. At runtime it is *nothing* — same bits, same size, the
wrapper compiles away. But to the type checker it is a brand-new, distinct type.

You spend the compiler's type system to buy back guarantees the raw type cannot
give you:

- **Distinct identity** — `Meters` and `Seconds` stop being interchangeable.
- **Your own trait impls** — you control `Add`, `Display`, `Deref`, etc., and
  you can implement *foreign* traits on *foreign* types by wrapping them.
- **Enforced invariants** — a private field plus a smart constructor makes the
  type itself a proof that the data is valid.

The runtime bill for all of this is zero. The recurring tension to manage: a
newtype *hides* its inner type by default, and `Deref` lets you leak the inner
API back for ergonomics — leak too much and the wrapper stops protecting
anything.

## Why this exists (from first principles)

Consider a function that computes speed:

```rust
fn speed(distance: f64, time: f64) -> f64 {
    distance / time
}
```

Nothing stops a caller writing `speed(time, distance)`. Both arguments are
`f64`, so the swap type-checks, runs, and silently returns garbage. The type
system has been told these two numbers are the same kind of thing — but they are
not. A meter and a second are different physical quantities.

The fix is to give each its own type:

```rust
#[derive(Debug, Clone, Copy)]
struct Meters(f64);

#[derive(Debug, Clone, Copy)]
struct Seconds(f64);

fn speed(distance: Meters, time: Seconds) -> f64 {
    distance.0 / time.0
}
```

Now `speed(t, d)` is a compile error (`expected Meters, found Seconds`). The
information "this number is a distance" was lost in the `f64` version; the
newtype encodes it back into the type, and the compiler enforces it for free.
`distance.0` reaches the inner `f64` — the single field of a tuple struct is
named `.0`.

> The newtype's superpower is **the absence of an impl**. `Meters + Seconds`
> won't compile not because anyone forbade it, but because you never wrote that
> impl. Safety by omission.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | Foundations | Distinct identity | `Meters` vs `Seconds`; swapping args is a type error |
| 2 | Foundations | Deriving the basics | A newtype has *no* behavior until you derive it; derives forward to the inner type |
| 3 | Mechanics | Type-safe arithmetic | `impl Add` defines the algebra; `Meters + Seconds` simply doesn't exist |
| 4 | Mechanics | `Deref` for ergonomics | Wrap `String`, deref to `str`, get its methods for free via coercion |
| 5 | Footgun | The `Deref` leak | `SortedVec` must *not* deref to `Vec` — that would leak `.push` and break the invariant |
| 6 | Footgun | Orphan-rule escape hatch | `impl Display` for a foreign type by wrapping it in a local newtype |
| 7 | Real-world | `repr(transparent)` | Prove the layout is identical to the inner type; sound slice reinterpret |
| 8 | Real-world | Parse, don't validate | `Email` with a private field + smart constructor; the type proves validity |
| 9 | Capstone | Phantom-typed `Id<T>` | One generic newtype gives `Id<User>` != `Id<Post>`, zero-cost, `HashMap` key |

## The ideas, built up

### 1. A newtype starts with no behavior

A tuple struct inherits *nothing* from its inner type. `UserId(u64)` cannot be
printed, compared, copied, or sorted — even though the `u64` inside can do all of
those. Every capability must be granted explicitly:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct UserId(u64);
```

Each derive generates an impl that simply forwards to the inner field. `UserId(3)
< UserId(9)` compares the two `u64`s; `==` compares them; `.max()` works because
`Ord + Copy` are present:

```rust
fn max_id(ids: &[UserId]) -> UserId {
    ids.iter().copied().max().unwrap()
}
```

`.copied()` is only valid because we derived `Copy`; `.max()` only because we
derived `Ord`. Without those derives, this is a wall of E0277/E0599 errors —
which is the lesson: a newtype is opt-in.

> **`Eq` needs `PartialEq`, `Ord` needs `PartialOrd`.** They are supertraits.
> You derive both halves.

### 2. You define the algebra (`Add`)

`==` and `<` are derivable; `+` is not. To add two `Meters` you implement
`std::ops::Add` yourself — and that is a feature, because *you* decide what
arithmetic is meaningful:

```rust
use std::ops::Add;

impl Add for Meters {
    type Output = Meters;
    fn add(self, rhs: Meters) -> Meters {
        Meters(self.0 + rhs.0)
    }
}
```

`type Output` is the associated type that says "adding two `Meters` yields a
`Meters`". Because the only `Add` impl in scope is `Meters + Meters`,
`Meters + Seconds` has no impl and is rejected (E0277). This is exactly how
`std::time::Duration` works: `Duration + Duration` is defined, `Duration + u64`
is not.

```rust
fn total(distances: &[Meters]) -> Meters {
    distances.iter().copied().fold(Meters(0.0), |acc, d| acc + d)
}
```

The fold starts from `Meters(0.0)` and threads your `+` through the slice.

### 3. `Deref` for ergonomics

Sometimes you *want* the wrapper to behave like the thing it wraps. Implementing
`Deref` makes `&Wrapper` coerce to `&Target`, so the target's methods and any
`&Target`-taking function work on the wrapper directly:

```rust
use std::ops::Deref;

struct Username(String);

impl Deref for Username {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0          // &String coerces to &str
    }
}
```

Two distinct mechanisms now kick in:

- **Method resolution walks the deref chain.** `username.len()` finds no `len`
  on `Username`, derefs to `str`, and calls `str::len`.
- **Deref coercion** lets `&Username` be passed where `&str` is expected:
  `greet(&u)` compiles even though `greet(name: &str)`.

```rust
let u = Username(String::from("ferris"));
assert_eq!(u.len(), 6);                       // Username -> str
assert_eq!(greet(&u), "Hello, ferris!");      // &Username coerces to &str
```

This is the same machinery that lets you call `&str` methods on a `String`, or
`&T` methods on a `Box<T>`.

## Footguns

### The `Deref` leak

`Deref` is convenient enough to be dangerous. The temptation is to slap
`impl Deref<Target = Vec<i32>>` on any wrapper to "inherit" the inner API. But if
the wrapper exists to **enforce an invariant**, deref leaks the very methods that
break it.

`SortedVec` keeps its `Vec<i32>` sorted. If it derefed to `Vec`, a caller could
reach `.push`, `.swap`, or (with `DerefMut`) mutate the buffer out of order and
silently violate "sorted". The ladder deliberately does **not** implement
`Deref`. Instead it exposes a curated API:

```rust
struct SortedVec(Vec<i32>);

impl SortedVec {
    fn insert(&mut self, value: i32) {
        // partition_point finds the first index where x >= value
        self.0.insert(self.0.partition_point(|&x| x < value), value);
    }

    fn as_slice(&self) -> &[i32] {
        &self.0          // read-only window: no .push leaks out
    }
}
```

```rust
// sv.push(0);  // does NOT compile — push doesn't exist on SortedVec
```

That non-compilation *is* the invariant being protected structurally.

> **Rule of thumb:** `Deref` is for smart *pointers* (`Box`, `Rc`, `Arc`), where
> the wrapper genuinely *is* a stand-in for the inner value. For an
> invariant-holding newtype, expose a curated API, not `Deref`. The Rust API
> guidelines say the same: don't `impl Deref` to emulate inheritance.

### The orphan rule (and the escape hatch)

The orphan rule: you may implement a trait for a type only if the trait **or**
the type is local to your crate. So `impl Display for Vec<i32>` is illegal
(E0117) — both `Display` and `Vec` are foreign.

The newtype is the escape hatch. Wrap the foreign type in a local struct and the
type is now yours, so the impl is legal:

```rust
use std::fmt;

struct PrettyVec(Vec<i32>);

impl fmt::Display for PrettyVec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for (i, v) in self.0.iter().enumerate() {
            if i > 0 { write!(f, ", ")?; }
            write!(f, "{v}")?;
        }
        write!(f, "]")
    }
}
```

`PrettyVec(vec![1, 2, 3]).to_string()` is `"[1, 2, 3]"`. This is exactly how
crates add `Display`, `serde::Serialize`, and other foreign traits to types they
do not own.

## Real-world patterns

### `#[repr(transparent)]` — zero-cost, guaranteed

"Zero-cost" stops being a slogan when you reach for the layout. A newtype over
`T` has the **same size and alignment** as `T`. The optimizer usually exploits
this, but `#[repr(transparent)]` makes it a *guaranteed, ABI-stable* fact: the
struct is laid out exactly like its single non-zero-sized field.

```rust
use std::mem::{align_of, size_of};

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(transparent)]
struct Wrapping64(u64);

assert_eq!(size_of::<Wrapping64>(), size_of::<u64>());   // 8 == 8
assert_eq!(align_of::<Wrapping64>(), align_of::<u64>());
```

The guarantee is what makes it sound to reinterpret a slice of the newtype as a
slice of the raw type, with no copy:

```rust
fn as_raw_slice(xs: &[Wrapping64]) -> &[u64] {
    // SAFETY: Wrapping64 is #[repr(transparent)] over u64, so each element has
    // identical layout and every Wrapping64 is a valid u64. The pointer cast and
    // length are therefore valid for a &[u64] over the same memory.
    unsafe {
        std::slice::from_raw_parts(xs.as_ptr() as *const u64, xs.len())
    }
}
```

> **Direction matters.** This cast is sound because *every* bit pattern of `u64`
> is a valid `u64`. The reverse — `&[u64]` to `&[NonZeroU64]` — would be UB for a
> zero, because `NonZeroU64` has a validity *niche*. `transparent` guarantees
> *layout*, not that arbitrary bytes are valid. `repr(transparent)` is also what
> makes a newtype safe to pass across an FFI boundary where C expects the raw
> type.

### Parse, don't validate (the validated newtype)

The most powerful newtype move: make the **type itself a proof** that an
invariant holds. Put the data behind a private field, offer no public
constructor, and check the invariant exactly once in a fallible smart
constructor:

```rust
mod email {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Email(String);   // private field: only this module can build one

    #[derive(Debug, PartialEq, Eq)]
    pub enum EmailError { Empty, MissingAt }

    impl Email {
        pub fn parse(s: &str) -> Result<Email, EmailError> {
            if s.is_empty()       { return Err(EmailError::Empty); }
            if !s.contains('@')   { return Err(EmailError::MissingAt); }
            Ok(Email(s.to_string()))
        }
        pub fn as_str(&self) -> &str { &self.0 }
    }
}
```

The field privacy is the whole trick: code outside `mod email` literally cannot
write `Email(whatever)`, so the **only** way to obtain an `Email` is through
`parse`. Once you hold one, it is guaranteed to have passed the check. Downstream
code never re-validates:

```rust
fn send_to(addr: &email::Email) -> String {
    format!("sending to {}", addr.as_str())   // no validation needed
}
```

You cannot even *call* `send_to` with an unvalidated string — there is no way to
construct the argument. This is "parse, don't validate": turn unstructured input
into a type that *cannot represent* the invalid state. It is the pattern behind
`std::num::NonZeroU32`, `url::Url`, and most well-designed domain types.

## Capstone insight

A database layer hands out numeric ids for every table. Plain `u64` ids are a bug
factory — nothing stops you passing a user's id where a post's id is expected.
You *could* write `UserId`, `PostId`, `OrderId` by hand, but that is endless
boilerplate.

Instead, one generic newtype with a **phantom type tag**:

```rust
use std::marker::PhantomData;

struct User;   // pure markers — carry no data
struct Post;

struct Id<T> {
    raw: u64,
    _tag: PhantomData<T>,   // "generic over T" without storing a T
}

impl<T> Id<T> {
    fn new(raw: u64) -> Id<T> { Id { raw, _tag: PhantomData } }
    fn get(&self) -> u64 { self.raw }
}
```

`PhantomData<T>` is a zero-sized marker that lets the struct be generic over `T`
without holding one. `Id<User>` and `Id<Post>` are now distinct types that cannot
be mixed, yet each is still just a `u64` at runtime. `assert_eq!(u1, p1)` where
`u1: Id<User>` and `p1: Id<Post>` is a compile error.

### The subtle part: don't let the derive bound your tag

You want `Id<T>` to be `Copy + Clone + PartialEq + Eq + Hash + Debug` for **every**
`T`, so it can be a `HashMap` key regardless of the tag. The trap:

```rust
// SUBTLE: this attaches a `T: Trait` bound you do not want
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Id<T> { raw: u64, _tag: PhantomData<T> }
```

The derive macro expands to `impl<T: Hash> Hash for Id<T>`, `impl<T: Copy> Copy
for Id<T>`, and so on. So `Id<T>` is only `Hash` when `T` is `Hash` — but the tag
`User` holds no data and need not implement anything. The derive makes the wrong
thing the bound: it bounds the *tag* instead of the `u64`.

The ladder's working solution made the tags derive everything too, which compiles
— but it is a coincidence. The day a tag does not implement `Hash`/`Copy`, the id
silently loses those traits. The robust pattern real crates (slotmap, ECS entity
ids) use is to **hand-write the impls so the bound lands on the data, not the
tag**:

```rust
// OK: no bound on T anywhere — works for ANY tag
impl<T> Clone for Id<T> { fn clone(&self) -> Self { *self } }
impl<T> Copy for Id<T> {}
impl<T> PartialEq for Id<T> { fn eq(&self, o: &Self) -> bool { self.raw == o.raw } }
impl<T> Eq for Id<T> {}
impl<T> std::hash::Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) { self.raw.hash(h); }
}
```

The structural "aha": **a phantom type appears in the type signature but never in
the data, so trait impls on the wrapper should be bounded by the data, not by the
phantom.** That is what makes `Id<T>` truly zero-cost and tag-agnostic.

## Explain it back

Future-you should be able to answer these cold:

1. Why does `speed(time, distance)` compile with `f64` args but not with
   `Meters`/`Seconds` args? What information did the newtype restore?
2. Why does a fresh `UserId(u64)` not support `==` or `{:?}`? What does a derive
   actually generate?
3. `Meters + Seconds` fails to compile. Which mechanism rejects it — a forbidding
   rule, or a missing impl?
4. What two things does `impl Deref for Username` enable, and how do they differ?
5. Why does `SortedVec` deliberately *not* implement `Deref<Target = Vec>`? What
   would break?
6. The orphan rule forbids `impl Display for Vec<i32>`. How does `PrettyVec` make
   the same impl legal?
7. What does `#[repr(transparent)]` guarantee beyond what the optimizer already
   does? Why is `&[Wrapping64] -> &[u64]` sound but `&[u64] -> &[NonZeroU64]` not?
8. In the `Email` module, what single language feature makes the "every `Email`
   is valid" guarantee airtight?
9. Why does `#[derive(Hash)]` on `Id<T>` produce the *wrong* bound, and how do the
   hand-written impls fix it?

## See also

- [Conversion traits](conversions.md) — `From`/`Into`/`TryFrom`, the in/out of a
  newtype's smart constructor.
- [Blanket impls & coherence](blanket-coherence.md) — the orphan rule in full,
  and the newtype workaround from the trait-author side.
- [Custom error types](custom-errors.md) — `EmailError` is a tiny error enum;
  the full treatment lives here.
- [Static vs dynamic dispatch](dispatch.md) — monomorphization is why the
  phantom `Id<T>` and `repr(transparent)` wrappers cost nothing at runtime.
