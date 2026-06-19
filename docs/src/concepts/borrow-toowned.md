# Borrow / ToOwned

> Ladder: [`src/bin/borrow_toowned.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/borrow_toowned.rs) ·
> Run: `cargo run --bin borrow_toowned` · Phase 1 · 9 rungs

## TL;DR

`ToOwned` and `Borrow` are the two traits that sit **underneath** `Cow` and
`HashMap`-key lookups.

- **`ToOwned`** is a *generalized `Clone`* for when the borrowed and owned types
  **differ**: `&str -> String`, `&[T] -> Vec<T>`. `Clone` is `&T -> T` (same
  type), so it can't express `str -> String`; `ToOwned` can, via an associated
  `Owned` type.
- **`Borrow<B>`** is the other direction — view an owned value as a borrowed `&B`
  (`String -> &str`) — but with a **contract**: the view must hash, compare, and
  order *identically* to the owner. That contract is exactly what lets a
  `HashMap<String, V>` be queried by `&str` without allocating.

## Why this exists (from first principles)

A `HashMap<String, V>` stores **owned** `String` keys. You want to look something
up with a cheap `&str` literal — without building a throwaway `String` every call.
That's only **sound** if `&str` hashes to the same bucket the `String` went into.
`Borrow<str> for String` is the *promise* that it does.

But the problem is deeper than just HashMap. Consider `str` and `String`: they
are *different types*, yet they represent the same data in different ownership
modes. Standard `Clone` can't express this — `Clone` is `&T -> T`, same type in,
same type out. You can't `impl Clone for str` to produce a `String`. So Rust
needs a trait that says "given a borrowed `&str`, produce its owned counterpart
`String`" — that's `ToOwned`. And it needs the reverse: "given an owned `String`,
produce a borrowed `&str` view" — that's `Borrow`.

Together, these two traits form a **round-trip contract** between borrowed and
owned forms. `Cow` is built directly on top of them: its `Owned` variant is
`<B as ToOwned>::Owned`, and `Borrow` is how it hands out `&B` from that variant.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `&str -> String`, `&[i32] -> Vec` via `.to_owned()` | The owned type is a *different* type than the input. |
| 2 | foundations | `Borrow` a `&str` out of a `&String`; `borrow_sum<T: Borrow<[i32]>>` | "View owned as borrowed"; one fn takes `Vec` **or** slice. |
| 3 | mechanics | `HashMap<String,_>::get("key")` + hand-written `contains_key2` | Read and write the `K: Borrow<Q>` bound — the payoff. |
| 4 | mechanics | `owned_pair<T: ToOwned>` returning `(T::Owned, T::Owned)` | Name the associated `Owned` type; why you can't return `T`. |
| 5 | footgun | `CiString` (case-insensitive) — `AsRef` yes, `Borrow` **no** | Borrow needs Eq/Hash transparency; AsRef makes no promise. |
| 6 | footgun | `Cache::get<Q>` instead of `.to_string()` per lookup | Borrow the lookup key — don't allocate to query. |
| 7 | real-world | `TagSet`: `add<S: Into<String>>` + `has<Q: Borrow>` | **Own at insert, borrow at query.** |
| 8 | real-world | `make_owned` (= `Cow::into_owned`) + `pick` (Cow producer) | Why `Cow<B>` *requires* `B: ToOwned`. |
| 9 | capstone | Hand-rolled `MyBorrow` + `MyToOwned` + `MyCow` | The whole machine, from scratch. |

## The ideas, built up

### ToOwned: Clone across type boundaries

`Clone` is `&T -> T` — same type. That works fine for `i32` or `Vec<String>`,
where the owned form and the borrowed form are the same type. But `str` and
`String` are fundamentally different types. `str` is unsized (a `[u8]` with a
UTF-8 invariant), living behind references. `String` is a `Vec<u8>` on the
heap. You can't clone a `str` into a `str` — there's nowhere to put it.

`ToOwned` bridges the gap with an associated type:

```rust
pub trait ToOwned {
    type Owned: Borrow<Self>;       // the owned form must borrow BACK to Self
    fn to_owned(&self) -> Self::Owned;
}
```

So `str: ToOwned<Owned = String>` and `[T]: ToOwned<Owned = Vec<T>>`. The
`.to_owned()` call on a `&str` produces a `String`:

```rust
fn duplicate(s: &str) -> String {
    s.to_owned()
}

fn duplicate_slice(xs: &[i32]) -> Vec<i32> {
    xs.to_owned()
}
```

The return types are *different types* than the inputs. That's the whole point —
`Clone` can't do this.

### Borrow: the other direction, with a contract

`Borrow<B>` goes the opposite way: given an owned value, hand out a borrowed
`&B` view. `String: Borrow<str>` and `Vec<T>: Borrow<[T]>`. There's also a
blanket `T: Borrow<T>` so every type can borrow as itself.

```rust
fn borrow_sum<T: Borrow<[i32]>>(xs: T) -> i32 {
    let slice: &[i32] = xs.borrow();
    slice.iter().sum()
}
```

This one function accepts both `Vec<i32>` and `&[i32]` — `borrow()` normalizes
either to `&[i32]`.

But `Borrow` is not just "give me a reference." It carries a **semantic
contract**: `x` and `x.borrow()` must produce the same `Eq`, `Ord`, and `Hash`
results. This is critical for HashMap and is what distinguishes Borrow from
AsRef.

### The payoff: HashMap lookup without allocation

This is why Borrow exists. The `HashMap::get` signature is:

```rust
fn get<Q>(&self, k: &Q) -> Option<&V>
where
    K: Borrow<Q>,        // the stored key can be viewed as Q
    Q: Hash + Eq + ?Sized // Q = str is unsized; only touched behind &Q
```

Read it as: *"the stored key `K` can be `Borrow`'d as `Q`."* With `K = String`
and `Q = str`, `String: Borrow<str>` holds, so `map.get("key")` just works —
no `String` allocation needed.

The contract is what makes this *sound*: when the map hashes the `&str` query,
it computes the same hash that the `String` key produced at insertion time. If
those hashes differed, the lookup would silently miss the bucket.

Writing the bound yourself makes it stick:

```rust
fn contains_key2<K, Q>(map: &HashMap<K, u32>, key: &Q) -> bool
where
    K: Borrow<Q> + Eq + Hash,
    Q: Eq + Hash + ?Sized,
{
    map.contains_key(key)
}
```

The `?Sized` on `Q` is required because `Q = str` is unsized — it's only ever
touched behind `&Q`, so unsized is fine.

### The associated type puzzle

When generic over `T: ToOwned`, the owned value's type is spelled `T::Owned`
(or `<T as ToOwned>::Owned`) — **never** `T`. This trips people up. `T` is the
*borrowed* type (e.g. `str`), which is usually unsized and can't be returned by
value:

```rust
fn owned_pair<T: ToOwned + ?Sized>(value: &T) -> (T::Owned, T::Owned) {
    (value.to_owned(), value.to_owned())
}

// Called with T = str:
let (a, b): (String, String) = owned_pair("hi");
// Called with T = [i32]:
let (v1, v2): (Vec<i32>, Vec<i32>) = owned_pair(&[1, 2][..]);
```

The `?Sized` bound on `T` is needed because `str` and `[T]` are unsized types —
without it, the compiler demands `T: Sized` and rejects `owned_pair::<str>`.

## Footguns

### Borrow vs AsRef: same shape, different promise

`Borrow<T>` and `AsRef<T>` have the **same signature**: `fn(&self) -> &T`. So
why two traits?

- **`AsRef<T>`**: "you can view me as `&T`." No other guarantee. Use it for
  flexible function arguments (accept `&str`, `String`, `PathBuf`, ...).
- **`Borrow<T>`**: the view is **semantically transparent** — `x` and
  `x.borrow()` must produce the same `Eq` / `Ord` / `Hash`. Implement it
  **only** when that holds.

### The CiString proof (rung 5)

A case-insensitive string hashes `"Hello"` and `"HELLO"` identically, but
plain `str` hashes them differently:

```rust
impl Hash for CiString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for b in self.0.bytes() {
            state.write_u8(b.to_ascii_lowercase());
        }
    }
}
```

A `Borrow<str>` impl for `CiString` would force `str`'s hasher on lookup and
**silently miss the bucket**. The ladder proves this by computing hashes both
ways:

```rust
// CiString hashes case-insensitively: "Hello" == "HELLO"
assert_eq!(h(&CiString::new("Hello")), h(&CiString::new("HELLO")));
// but plain str hashes exactly: "Hello" != "HELLO"
assert_ne!(h("Hello"), h("HELLO"));
```

So `CiString` implements `AsRef<str>` (legal — AsRef makes no promise) but
deliberately **not** `Borrow<str>`. When the equivalence relations don't match,
you must honestly allocate a `CiString` to query:

```rust
fn find_ci(map: &HashMap<CiString, i32>, query: &str) -> Option<i32> {
    let key = CiString::new(query);   // must allocate — no Borrow shortcut
    map.get(&key).copied()
}
```

### Needless `.to_string()` at lookup (rung 6)

The classic wasteful pattern:

```rust
fn get_bad(&self, key: &str) -> Option<&str> {
    self.0.get(&key.to_string())...   // WRONG: allocates per lookup!
}
```

The fix is one generic method that accepts a borrowed key directly:

```rust
fn get<Q>(&self, key: &Q) -> Option<&str>
where
    String: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
{
    self.0.get(key).map(|v| v.as_str())   // OK: zero allocation
}
```

Reflexively reach for `key: &Q where Key: Borrow<Q>` instead of taking or
owning a `String` at query boundaries.

## Real-world patterns

### Into-in / Borrow-out (rung 7)

A keyed collection has two boundaries that want **different** traits:

```rust
impl TagSet {
    // INSERT: you must end up OWNING -> accept impl Into<String> (at most 1 alloc)
    fn add<S: Into<String>>(&mut self, tag: S) {
        self.tags.insert(tag.into());
    }

    // QUERY: you only LOOK -> borrow, never allocate
    fn has<Q>(&self, tag: &Q) -> bool
    where
        String: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.tags.contains(tag)
    }
}
```

This is the pattern real APIs use: `Into` at the ownership boundary (insert,
store, construct), `Borrow` at the lookup boundary (get, contains, find).

### Borrow gives breadth for free

One `Borrow<str>` bound accepts `&str`, `String`, `Box<str>`, `Rc<str>`, **and**
`Cow<str>`:

```rust
fn shout<S: Borrow<str>>(s: S) -> String {
    s.borrow().to_uppercase()
}

assert_eq!(shout("hi"), "HI");                  // &str
assert_eq!(shout(String::from("yo")), "YO");    // String
assert_eq!(shout(Box::<str>::from("be")), "BE"); // Box<str>
assert_eq!(shout(Rc::<str>::from("rc")), "RC");  // Rc<str>
assert_eq!(shout(Cow::Borrowed("cow")), "COW");  // Cow<str>
```

### Closing the Cow loop (rung 8)

```rust
pub enum Cow<'a, B: ToOwned + ?Sized> {
    Borrowed(&'a B),
    Owned(<B as ToOwned>::Owned),
}
```

`B: ToOwned` is **mandatory**: the `Owned` variant must name a concrete owned
type (`<B as ToOwned>::Owned`), and `to_owned()` is the only way to manufacture
one from a borrow. Re-implementing `Cow::into_owned` yourself proves this is
the *only* mechanism:

```rust
fn make_owned<B: ToOwned + ?Sized>(c: Cow<'_, B>) -> B::Owned {
    match c {
        Cow::Borrowed(b) => b.to_owned(),   // ToOwned builds the owned form
        Cow::Owned(o) => o,                 // already there
    }
}
```

That's the full answer to *"why does `Cow` require `B: ToOwned`?"* — without
it, Cow couldn't name its owned half nor build it on demand.

## Signatures to know

```rust
// ToOwned — generalized Clone across type boundaries
pub trait ToOwned {
    type Owned: Borrow<Self>;       // the owned form must borrow BACK to Self
    fn to_owned(&self) -> Self::Owned;
}

// Borrow — view owned as borrowed, with Eq/Ord/Hash transparency
pub trait Borrow<Borrowed: ?Sized> {
    fn borrow(&self) -> &Borrowed;
}

// HashMap::get — the single most important real-world use
fn get<Q>(&self, k: &Q) -> Option<&V>
where
    K: Borrow<Q>,        // the stored key can be viewed as Q
    Q: Hash + Eq + ?Sized // Q = str is unsized; only touched behind &Q
```

## Capstone insight

The structural insight from building `MyBorrow` + `MyToOwned` + `MyCow` from
scratch: `MyToOwned::Owned` carries a `MyBorrow<Self>` bound — *the owned type
must borrow back to `Self`*. That round-trip guarantee is exactly what lets
`MyCow::borrow()` return `&B` from the `Owned` variant:

```rust
trait MyToOwned {
    type Owned: MyBorrow<Self>;
    fn my_to_owned(&self) -> Self::Owned;
}

impl<'a, B: MyToOwned + ?Sized> MyCow<'a, B> {
    fn borrow(&self) -> &B {
        match self {
            Self::Borrowed(b) => b,
            Self::Owned(o) => o.my_borrow(),   // MyBorrow<Self> makes this possible
        }
    }
}
```

Without the `Owned: MyBorrow<Self>` bound, the `Owned` arm couldn't produce a
`&B` — there'd be no trait method to call. And `Self: ?Sized` being the default
in trait defs is why `impl MyToOwned for str` (an unsized type) is even legal.

## Explain it back

- Why can't `str` just `impl Clone` to produce a `String`? *(Clone is `&T -> T`,
  same type; `str -> String` needs the differing `Owned` associated type.)*
- What exactly is the `Borrow` contract, and what breaks if you violate it?
- Why is the bound `K: Borrow<Q>` and not `Q: Borrow<K>`?
- In `T::Owned`, why can't the return type just be `T`?
- Why does `Cow<B>` require `B: ToOwned`? Name both reasons (name it / build it).
- When do you pick `AsRef<T>` over `Borrow<T>` for a function argument?

## See also

- [Cow](cow.md) — this note closes the loop opened there; `Cow` is built
  directly on `ToOwned` and `Borrow`.
- [Drop & Ordering](drop-ordering.md) — `mem::replace`, used internally by
  `Cow::to_mut()`, is covered in depth there.
