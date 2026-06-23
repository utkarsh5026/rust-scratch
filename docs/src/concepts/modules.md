# Modules & visibility

> Ladder: [`src/bin/modules.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/modules.rs) ·
> Run: `cargo run --bin modules` · Phase 0 · 9 rungs

## TL;DR

A crate is a **tree** of modules rooted at the crate root. `mod` declares a *node*
in that tree — it imports nothing. Everything is **private by default**, and
privacy is *tree-relative*: an item is visible to its own module and all of that
module's descendants; a parent can only see into a child what the child marks
`pub`. Paths walk the tree (`crate::` from the root, `super::` up one, `self::`
here). `use` is just a *local alias* for a path; `pub use` is a *re-export* that
adds a brand-new public path. Master those three facts and everything else —
field privacy, `pub(crate)`, facades, sealed traits — is a corollary.

## Why this exists (from first principles)

Without modules, every name in a crate lives in one flat namespace, and every
function is callable from everywhere. Two problems follow immediately:

1. **No encapsulation.** If any code can call any function and read any field,
   you can't enforce invariants. A `Celsius` could be set to -1000; a half-built
   value could be observed. "It's an internal detail" becomes a comment, not a
   guarantee.
2. **No stable API.** If your internal organization *is* your public surface,
   you can't rename a helper or move a file without breaking everyone who depends
   on you. Refactoring becomes a breaking change.

Modules + visibility solve both. The module tree gives you namespaces and a
place to *hide* things; `pub` and its restricted variants let you publish
*exactly* the surface you intend, and nothing more. The compiler enforces it —
privacy is a checked rule, not advice. That's the whole point: **the things you
don't mark `pub` are things you are free to change.**

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | Foundations | Module tree & paths | `mod` builds a tree; `crate::`/`super::`/`self::` walk it |
| 2 | Foundations | `pub` opens a door | Privacy is tree-relative; keep helpers private |
| 3 | Mechanics | Field privacy | `pub struct` ≠ pub fields; private field + smart constructor guards an invariant |
| 4 | Mechanics | `use` & `pub use` | Local alias vs re-export; flatten a deep tree |
| 5 | Footgun | Leaking a private type | A `pub fn` can't honestly expose a less-visible type |
| 6 | Footgun | Restricted visibility | `pub(crate)` / `pub(super)` / `pub(in path)`: pick the narrowest reach |
| 7 | Real-world | Facade pattern | Private internals, curated public surface via `pub use` |
| 8 | Real-world | Sealed trait | A private-module supertrait gates who can `impl` |
| 9 | Capstone | `inventory` mini-library | A full module tree with a real front door |

## The ideas, built up

### 1. A module is a node; paths walk the tree

`mod kitchen { ... }` does not "import" anything. It creates a node named
`kitchen` under the current module, and the items inside it live at
`kitchen::...`. To *reach* an item you spell a path, and the prefix you choose
says where the path starts:

```rust
fn oven_temp() -> u32 { 220 }              // lives at the crate ROOT

mod kitchen {
    pub mod pantry {
        pub fn flour_grams() -> u32 { 500 }
    }
    pub mod stove {
        pub fn bake() -> (u32, u32) {
            // pantry is a SIBLING of stove → go up one (super), then down
            // oven_temp is at the ROOT → start from the root (crate)
            (super::pantry::flour_grams(), crate::oven_temp())
        }
    }
}
```

Three prefixes, three starting points:

| Prefix | Starts at | Use when |
|--------|-----------|----------|
| `crate::` | the crate root (absolute) | the item lives near the root / "shared, top-level" |
| `super::` | the parent module (relative, up one) | reaching a tight sibling or parent helper |
| `self::` | the current module (relative) | disambiguating a local name; rarely needed |

> **Key insight:** `super::pantry` and `crate::kitchen::pantry` resolve to the
> *same* item here. The difference is robustness to change. `super::` survives
> the whole subtree being moved or renamed; `crate::` survives *local* shuffling.
> Reach for `super::` for tight siblings, `crate::` for things that live near the
> root.

### 2. `pub` opens a door — and privacy is about the tree

By default an item is private to its module. Privacy is **relative**: an item is
visible to its defining module and every descendant of that module. A *parent*
sees a child's item only if it's `pub`.

```rust
mod billing {
    fn tax_rate() -> u32 { 8 }               // PRIVATE: internal detail

    pub fn total_with_tax(price: u32) -> u32 { // PUBLIC entry point
        price + price * tax_rate() / 100        // can call the private helper
    }
}

// at the crate root:
billing::total_with_tax(100);  // OK — it's pub
// billing::tax_rate();        // E0603: function `tax_rate` is private
```

Two subtleties worth internalizing:

- **`mod billing` itself needed no `pub`** because `billing` and its caller both
  live at the crate root — same module, already visible. If `billing` were nested
  inside another module, the outside would need `pub mod billing`.
- **`E0603` is a hard error.** Privacy isn't a lint you can ignore; the compiler
  refuses to let unrelated code reach in.

### 3. `pub struct` is not pub fields

Marking a struct `pub` makes the *type* nameable. Its fields stay private unless
each one is individually `pub`. That asymmetry is the single most useful tool in
the whole topic: **publish the type, hide the data, force everyone through your
methods.**

```rust
mod temperature {
    #[derive(Debug, PartialEq)]
    pub struct Celsius {
        degrees: i32,                 // PRIVATE — note: no `pub`
    }
    impl Celsius {
        pub fn new(d: i32) -> Option<Celsius> {   // smart constructor
            if d >= -273 { Some(Celsius { degrees: d }) } else { None }
        }
        pub fn get(&self) -> i32 { self.degrees }
    }
}

// let bogus = temperature::Celsius { degrees: -1000 };  // E0451: field is private
```

Because the field is private, the struct literal `Celsius { degrees: ... }` is
forbidden *outside* the module (`E0451`), and reading `.degrees` directly is too
(`E0616`). The only way to obtain a `Celsius` is `new`, which checks the
invariant. This is **parse, don't validate** enforced by the module system: a
`Celsius` *existing* is proof it's valid — downstream code never re-checks.

> Inside the defining module you can still use the literal freely — privacy only
> bites when you *cross* the module boundary.

### 4. `use` aliases; `pub use` re-exports

These look similar and do fundamentally different things.

```rust
mod deep { pub mod nested { pub mod core {
    pub struct Engine { pub power: u32 }
    impl Engine { pub fn new(power: u32) -> Engine { Engine { power } } }
}}}

fn start_deep() -> u32 {
    use crate::deep::nested::core::Engine;  // LOCAL alias — only this fn sees it
    Engine::new(9000).power
}

pub use deep::nested::core::Engine;         // RE-EXPORT — adds crate::Engine
```

- `use` is **private plumbing**: it shortens a path for the current scope and
  changes *nobody else's* view of the tree.
- `pub use` is **API surface**: it makes the item reachable through a *new* path
  (`crate::Engine`), in addition to its real one.

This is how every mature crate is laid out: deep folders internally for
organization, a thin layer of `pub use` at the root so users write `tokio::spawn`
instead of `tokio::runtime::task::spawn`. The internal tree is an implementation
detail; the re-exports are the contract.

### 5. You can't leak a private type through a public API

If a `pub fn` returns (or accepts) a type *less visible* than the function
itself, that's a leak: an outside caller would receive a value of a type they
can't name or use. The compiler stops you.

```rust
mod widget {
    pub struct Inner { pub id: u32 }   // must be pub to be honestly returned
    pub fn make(n: u32) -> Inner { Inner { id: n } }
}

let w: widget::Inner = widget::make(7);  // naming the type needs it pub (else E0603)
assert_eq!(w.id, 7);                      // reading the field needs it pub (else E0616)
```

> **Where the error shows up depends on the crate kind.** In a *library* crate,
> exposing a private type in a `pub` signature fires the `private_interfaces`
> lint (and historically the hard error `E0446`). In a *binary* like this ladder,
> there's no external consumer, so the leak instead bites at the **use site**:
> callers can't name the private type (`E0603`) or read its private fields
> (`E0616`).

The fix isn't to silence the error — it's to decide the type's *honest*
visibility. If you return it, you must publish it (this rung). If it should have
stayed internal, don't return it — hide it behind a facade (rung 7).

### 6. The visibility dial: `pub(crate)`, `pub(super)`, `pub(in path)`

`pub` and private are the two extremes. Between them sits a dial: "public, but
only up to *here*."

```rust
mod engine {
    pub(crate) const fn version() -> u32 { 3 }     // anywhere in THIS crate

    pub mod fuel {
        pub(super) const fn secret_formula() -> u32 { 42 }  // only the parent `engine`
    }
    pub mod electrical {
        pub(in crate::engine) const fn calibrate() -> u32 { 7 }  // the engine subtree
    }

    pub fn diagnostics() -> u32 {
        fuel::secret_formula() + electrical::calibrate()  // engine can see both
    }
}

// engine::fuel::secret_formula();  // privacy error: pub(super) doesn't reach the root
```

| Marker | Visible to | Typical use |
|--------|------------|-------------|
| `pub(crate)` | anywhere in this crate, not downstream | the workhorse: "shared internal API" |
| `pub(super)` | the parent module and its descendants | a child exposing something to just its parent |
| `pub(in path)` | within the named ancestor subtree | precise scoping when crate-wide is too loose |

> **Pick the narrowest visibility that satisfies the actual callers.** In the
> ladder, `VERSION` could even stay fully private, because the only thing reading
> it (`version()`) lives in the same module. `pub(crate)` would only earn its
> keep if some *other* module needed to read `VERSION` directly. Don't reach for
> a wider marker than the call sites demand.

## Footguns

- **Forgetting fields aren't auto-`pub`.** `pub struct Foo { x: i32 }` exposes the
  type but not `x`. Outside code can't build it with a literal (`E0451`) or read
  `x` (`E0616`). Usually that's *what you want* — but it surprises people who
  expected `pub struct` to mean "all public."
- **Returning a private type from a `pub fn`.** `private_interfaces` lint in libs,
  `E0603`/`E0616` at use sites in bins. The cure is to decide the type's real
  visibility, not to paper over the symptom.
- **`pub(super)` doesn't reach the crate root** unless the item's parent *is* the
  root. It's exactly one level of upward reach (plus descendants of that parent).
- **`use` vs `pub use` mix-up.** A plain `use` in your lib root does *not* expose
  anything to downstream crates — it only aliases for your own code. If you meant
  to re-export, you need `pub use`.
- **Over-widening visibility to "make it compile."** The compiler will happily
  accept `pub` everywhere; then your entire internal structure becomes API you
  can't change. Tighten to the minimum the callers actually need.

## Real-world patterns

### The facade: private internals, curated surface

This is how production crates are organized. Implementation lives in **private**
modules; a thin layer of `pub use` exposes only the handful of names that form
the public API.

```rust
mod api {
    mod internal {                       // PRIVATE — no `pub`
        struct RawSocket;                // guts the public API must NOT expose
        pub struct Client { pub name: &'static str }
        impl Client {
            pub fn connect(name: &'static str) -> Self { Self { name } }
            pub fn ping(&self) -> &'static str { "pong" }
        }
    }
    pub use internal::Client;            // re-export ONLY Client
}

api::Client::connect("db-1");      // OK
// api::internal::RawSocket;       // E0603 — `internal` is private, path sealed
```

The payoff: you can rename `RawSocket`, split `internal` into ten files,
restructure freely — and *nothing downstream breaks*, because the only public
path is the curated `pub use`. **The module tree stops being part of your API
contract.**

### The sealed trait: a private-module supertrait

Privacy can gate not just *calling* but *implementing*. Make your public trait
require a supertrait that lives in a **private** module. Outsiders can see and
call your trait, but can't `impl` it — satisfying it requires impl'ing the
supertrait, which they can't even name.

```rust
mod format {
    mod sealed { pub trait Sealed {} }      // private module → unnameable downstream

    pub trait Encoder: sealed::Sealed {     // supertrait bound is the seal
        fn encode(&self, input: &str) -> String;
    }

    pub struct Json;
    impl sealed::Sealed for Json {}         // only possible INSIDE `format`
    impl Encoder for Json {
        fn encode(&self, input: &str) -> String { format!("json:{input}") }
    }
}

// Downstream:
// struct Rogue;
// impl format::Encoder for Rogue { ... }   // error: Rogue: format::sealed::Sealed
//                                           // not satisfied — and you can't impl it
```

The "sealed trait" you meet in `typestate` and `blanket_coherence` is, *mechanically*,
just this: privacy applied to a supertrait. Because no downstream `impl` can ever
exist, you're free to add methods to `Encoder` later without breaking anyone — a
genuine API-evolution tool.

## Capstone insight

Rung 9 assembles everything into an `inventory` "library in a file":

```
inventory                  (the library root)
├── util    (PRIVATE)      pub(crate) normalize() — shared by submodules
├── model   (PRIVATE)      Sku (private field + smart constructor), Item
├── store   (PRIVATE)      Warehouse (private items: Vec<Item>)
└── (facade) pub use model::{Sku, Item}; pub use store::Warehouse;
```

The structural "aha": **every visibility decision is driven by who the actual
caller is, and the public surface is a deliberate, tiny re-export layer.**

- `util::normalize` is `pub(crate)` — *both* `model` and `store` call it, so
  crate-wide is the right reach, but it's not part of the public API. (A
  `pub(super)` would have been too narrow once two different submodules needed
  it.)
- `Sku` is `pub` with a **private `code` field**: the only way to get one is the
  normalizing, length-checking `new`, so an existing `Sku` is provably valid.
- The three submodules (`util`, `model`, `store`) are *all private*. The only way
  in is the facade — `inventory::Sku` works, `inventory::model::Sku` does not.
- Two seal probes prove it: `inventory::store::Warehouse::new()` is unreachable
  (the submodule is private) and `inventory::Sku { code: ... }` is forbidden (the
  field is private). The invariant and the encapsulation are both *enforced*, not
  hoped for.

Build this once and the mental model locks in: the module tree is your private
workshop; `pub use` is the storefront window; and the narrowest `pub(...)` that
satisfies the real callers is always the right answer.

## Explain it back

- Why does `mod foo;` not "import" anything? What does it actually do?
- `super::bar` vs `crate::foo::bar` resolve to the same item — when would you
  prefer each, and why?
- Why does marking a struct `pub` *not* make its fields public, and how is that
  the foundation of "parse, don't validate"?
- What's the difference between `use path::Thing` and `pub use path::Thing`? Which
  one is part of your crate's public API?
- A `pub fn` returns a private struct. What happens in a library crate vs a binary
  crate, and what are the two honest fixes?
- When would you choose `pub(crate)` over `pub(super)`? Over leaving it private?
- How does a private module turn a public trait into a *sealed* trait, and why is
  that an API-evolution tool?
- In the facade pattern, what exactly are you free to change without breaking
  downstream code, and why?

## See also

- [The typestate pattern](typestate.md) — sealed `State` trait via a private supertrait.
- [Blanket impls & coherence](blanket-coherence.md) — the sealed extension-trait pattern.
- [API evolution & semver](semver.md) — sealed traits and `#[non_exhaustive]` as evolution levers.
- [Newtype & zero-cost wrappers](newtype.md) — private fields + smart constructors for invariants.
