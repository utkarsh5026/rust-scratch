# Borrow / ToOwned

> Ladder: [`src/bin/borrow_toowned.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/borrow_toowned.rs) ·
> Run: `cargo run --bin borrow_toowned` · Phase 1 · 9 rungs

## TL;DR

`ToOwned` and `Borrow` are the two traits that sit **underneath** `Cow` and
`HashMap`-key lookups.

- **`ToOwned`** is a *generalized `Clone`* for when the borrowed and owned types
  **differ**: `&str → String`, `&[T] → Vec<T>`. `Clone` is `&T → T` (same type),
  so it can't express `str → String`; `ToOwned` can, via an associated `Owned` type.
- **`Borrow<B>`** is the other direction — view an owned value as a borrowed `&B`
  (`String → &str`) — but with a **contract**: the view must hash, compare, and
  order *identically* to the owner. That contract is exactly what lets a
  `HashMap<String, V>` be queried by `&str` without allocating.

## Why it exists

A `HashMap<String, V>` stores **owned** `String` keys. You want to look something
up with a cheap `&str` literal — without building a throwaway `String` every call.
That's only **sound** if `&str` hashes to the same bucket the `String` went into.
`Borrow<str> for String` is the *promise* that it does. The whole machinery
(`Borrow`, the `K: Borrow<Q>` bound, `?Sized`) exists to make borrowed-key lookup
both ergonomic and correct.

## The ladder

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `&str → String`, `&[i32] → Vec` via `.to_owned()` | The owned type is a *different* type than the input. |
| 2 | foundations | `Borrow` a `&str` out of a `&String`; `borrow_sum<T: Borrow<[i32]>>` | "View owned as borrowed"; one fn takes `Vec` **or** slice. |
| 3 | mechanics | `HashMap<String,_>::get("key")` + hand-written `contains_key2` | Read & write the `K: Borrow<Q>` bound — the payoff. |
| 4 | mechanics | `owned_pair<T: ToOwned>` returning `(T::Owned, T::Owned)` | Name the associated `Owned` type; why you can't return `T`. |
| 5 | footgun | `CiString` (case-insensitive) — `AsRef` yes, `Borrow` **no** | Borrow needs Eq/Hash transparency; AsRef makes no promise. |
| 6 | footgun | `Cache::get<Q>` instead of `.to_string()` per lookup | Borrow the lookup key — don't allocate to query. |
| 7 | real-world | `TagSet`: `add<S: Into<String>>` + `has<Q: Borrow>` | **Own at insert, borrow at query.** |
| 8 | real-world | `make_owned` (= `Cow::into_owned`) + `pick` (Cow producer) | Why `Cow<B>` *requires* `B: ToOwned`. |
| 9 | capstone | Hand-rolled `MyBorrow` + `MyToOwned` + `MyCow` | The whole machine, from scratch. |

## Signatures to know

The trait definitions — memorize the associated type and its bound:

```rust
pub trait ToOwned {
    type Owned: Borrow<Self>;       // the owned form must borrow BACK to Self
    fn to_owned(&self) -> Self::Owned;
}

pub trait Borrow<Borrowed: ?Sized> {
    fn borrow(&self) -> &Borrowed;
}
```

The `HashMap::get` bound — the single most important real-world use:

```rust
fn get<Q>(&self, k: &Q) -> Option<&V>
where
    K: Borrow<Q>,        // the stored key can be viewed as Q
    Q: Hash + Eq + ?Sized // Q = str is unsized; only touched behind &Q
```

Read it as: *"the stored key `K` can be `Borrow`'d as `Q`."* With `K = String`,
`Q = str`, `String: Borrow<str>` holds → `map.get("key")` just works.

**The associated-type rule (rung 4):** when generic over `T: ToOwned`, the owned
value's type is `<T as ToOwned>::Owned` (or `T::Owned`) — *never* `T`, because `T`
is the borrowed type (e.g. `str`), usually unsized and unreturnable by value.

## The real-world pattern: Into-in / Borrow-out (rung 7)

A keyed collection has two boundaries that want **different** traits:

```rust
impl TagSet {
    // INSERT: you must end up OWNING → accept impl Into<String> (≤1 alloc)
    fn add<S: Into<String>>(&mut self, tag: S) { self.tags.insert(tag.into()); }

    // QUERY: you only LOOK → borrow, never allocate
    fn has<Q>(&self, tag: &Q) -> bool
    where String: Borrow<Q>, Q: Hash + Eq + ?Sized { self.tags.contains(tag) }
}
```

And `Borrow<str>` as a bound gives you breadth for free — one signature accepts
`&str`, `String`, `Box<str>`, `Rc<str>`, **and** `Cow<str>`:

```rust
fn shout<S: Borrow<str>>(s: S) -> String { s.borrow().to_uppercase() }
```

## Closing the Cow loop (rung 8)

```rust
pub enum Cow<'a, B: ToOwned + ?Sized> {
    Borrowed(&'a B),
    Owned(<B as ToOwned>::Owned),
}
```

`B: ToOwned` is **mandatory**: the `Owned` variant must name a concrete owned
type (`<B as ToOwned>::Owned`), and `to_owned()` is the only way to manufacture
one from a borrow. That's the full answer to *"why does `Cow` require
`B: ToOwned`?"*

## Footguns

- **`Borrow` vs `AsRef` — same shape, different promise.** Both are `fn(&self) -> &T`.
  `AsRef<T>` = "you can view me as `&T`", no other guarantee → use it for flexible
  args. `Borrow<T>` = the view is **semantically transparent** (same `Eq`/`Ord`/
  `Hash`) → implement it **only** when that holds.
- **The `CiString` proof (rung 5).** A case-insensitive string hashes `"Hello" ==
  "HELLO"`, but plain `str` hashes them differently. A `Borrow<str>` impl would
  force `str`'s hasher on lookup and **silently miss the bucket**. So `CiString`
  impls `AsRef<str>` but deliberately **not** `Borrow<str>` — when the equivalence
  relations don't match, you must honestly allocate a `CiString` to query.
- **Needless `.to_string()` at lookup (rung 6).** `self.map.get(&key.to_string())`
  allocates *every call* for nothing. Reach reflexively for
  `key: &Q where ContainerKey: Borrow<Q>` instead of taking/owning a `String` at
  query boundaries.
- **`?Sized` on `Q`.** `Q = str` is unsized; it's only ever touched behind `&Q`,
  so `?Sized` is required and harmless.

## Explain it back

- Why can't `str` just `impl Clone` to produce a `String`? *(Clone is `&T → T`,
  same type; `str → String` needs the differing `Owned` associated type.)*
- What exactly is the `Borrow` contract, and what breaks if you violate it?
- Why is the bound `K: Borrow<Q>` and not `Q: Borrow<K>`?
- In `T::Owned`, why can't the return type just be `T`?
- Why does `Cow<B>` require `B: ToOwned`? Name both reasons (name it / build it).
- When do you pick `AsRef<T>` over `Borrow<T>` for a function argument?

## Capstone takeaway (rung 9)

The structural insight, earned: `MyToOwned::Owned` carries a `MyBorrow<Self>`
bound — *the owned type must borrow back to `Self`*. That round-trip guarantee is
exactly what lets `MyCow::borrow()` return `&B` from the `Owned` variant. And
`Self: ?Sized` being the default in trait defs is why `impl MyToOwned for str`
(an unsized type) is even legal.

## See also

- `Cow` ladder — this note closes the loop opened there.
- Conversion traits ladder — `Into` (own boundary) vs `Borrow`/`AsRef` (view boundary).
