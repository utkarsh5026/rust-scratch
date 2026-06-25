# Marker & auto traits

> Ladder: [`src/bin/marker_auto_traits.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/marker_auto_traits.rs) ·
> Run: `cargo run --bin marker_auto_traits` · Phase 2 · 9 rungs

## TL;DR

A **marker trait** is a trait with no methods. It carries no behavior — it is a compile-time *tag* that means "this type has this property" or "this type is permitted here." You use it as a bound (`T: Marker`), and the bound alone is the whole point.

An **auto trait** is a special marker the compiler implements *for you*, automatically and recursively, based on a type's fields. `Send` and `Sync` are the famous ones. You reason about them **negatively**: a type is `Send` *unless* it contains something that isn't. You opt out by adding a `!Send` field (a raw pointer via `PhantomData`), and opt back in by promising soundness yourself with `unsafe impl`.

`PhantomData<T>` is the glue: a zero-sized field that lets a type *carry* a type parameter it never stores, controlling auto traits, variance, and drop-checking at zero runtime cost.

## Why this exists (from first principles)

Rust needs to express type-level *facts* that are checked by the compiler but require no methods to call:

- **"This duplicates instead of moves"** — `Copy`. The fact changes assignment semantics, not behavior.
- **"This has a known size at compile time"** — `Sized`. Generic code needs it to put values on the stack.
- **"This is safe to move to another thread"** — `Send`. **"Safe to share `&T` across threads"** — `Sync`.

None of these are behaviors you invoke. They are properties the type system reasons about. A normal trait (with methods) is the wrong tool — there is nothing to implement. So Rust gives you the empty trait as a *permission slip* and the auto trait as an *inferred property*. Both are checked at compile time and erased entirely at runtime.

The payoff: you encode invariants — "only authorized types," "thread-bound handles," "legal protocol states" — directly into the type system, and the compiler enforces them with zero runtime cost.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | Marker trait as a permission tag | An empty trait used as a bound gates which types a function accepts. |
| 2 | foundations | `Copy` is a marker | `Copy: Clone` flips assignment/argument-passing from move to bitwise copy. |
| 3 | mechanics | `Sized` and `?Sized` | Every generic has a silent `T: Sized`; relax it to accept DSTs. |
| 4 | mechanics | Auto traits compose structurally | A type is `Send`/`Sync` iff all its fields are. |
| 5 | footgun | `Rc` is `!Send` | A non-atomic refcount poisons the whole closure; `Arc` fixes it. |
| 6 | footgun | Negative reasoning & opt-out | `PhantomData<*const ()>` makes your own type `!Send`/`!Sync`. |
| 7 | real-world | `PhantomData` as a marker | Typed IDs: `Id<User> != Id<Post>`; the marker shape controls auto traits. |
| 8 | real-world | `unsafe impl Send` done right | Re-grant auto traits a raw pointer removed — under the right bound, with a SAFETY contract. |
| 9 | capstone | Typestate from markers | Sealed ZST states + `PhantomData<S>` make illegal operations not compile. |

## The ideas, built up

### 1. A marker trait is a permission slip

An empty trait has no methods, so what could it possibly do? It tags types, and a generic bound consumes the tag.

```rust
trait Approved {}            // no methods — pure tag

struct Admin;
struct Editor;
struct Guest;                // deliberately NOT tagged

impl Approved for Admin {}
impl Approved for Editor {}

fn can_publish<T: Approved>(_user: &T) -> bool { true }
```

`can_publish` has no idea what `Approved` *means* — it never calls anything on `T`. The bound `T: Approved` is the entire mechanism. `can_publish(&Admin)` compiles; `can_publish(&Guest)` is a compile error (`the trait bound Guest: Approved is not satisfied`). You built a type-level access list, enforced at compile time, costing nothing at runtime.

### 2. `Copy` is a marker that changes language semantics

`Copy` is the most famous marker. It has no method of its own — the duplication logic lives on its supertrait `Clone` (`Copy: Clone`). What it *does* is tell the compiler: "duplicate this bit-for-bit on assignment instead of moving it."

```rust
#[derive(Copy, Clone)]       // Copy needs Clone — you can't have one without the other
struct Point { x: i32, y: i32 }

fn manhattan(p: Point) -> i32 { p.x.abs() + p.y.abs() }

fn sum_uses_original() -> i32 {
    let p = Point { x: 3, y: -4 };
    let d = manhattan(p);    // p is COPIED in, not moved
    d + p.x + p.y            // ...so p is STILL valid here
}
```

Without `Copy`, `manhattan(p)` *moves* `p`, and the next line is `error[E0382]: use of moved value`. Adding the marker silently rewrites what `manhattan(p)` means. Note the structural rule already appearing: a `String` field blocks `Copy` entirely, because `String` isn't `Copy`. The property composes from the fields up — exactly how auto traits will behave.

### 3. `Sized` and the invisible bound

`Sized` is auto-implemented for every type whose size is known at compile time. It is *not* implemented for **dynamically sized types** (DSTs): `str`, `[T]`, `dyn Trait`. You can never hold a bare DST by value — only behind a pointer (`&str`, `Box<str>`, `&[T]`).

The twist: **every** generic `<T>` carries a silent `T: Sized` the compiler inserts for you. So `fn f<T>(x: T)` secretly means `fn f<T: Sized>(x: T)`. To accept DSTs you opt out with `?Sized` — the only place `?` ever appears on a bound.

```rust
// WRONG: the implicit `T: Sized` rejects str and [u8]
// fn last_byte<T: Bytes>(value: &T) -> Option<u8> { ... }

// OK: relax the implicit bound; keep the value behind a reference
fn last_byte<T: Bytes + ?Sized>(value: &T) -> Option<u8> {
    value.view().last().copied()
}
```

The compiler's own diagnostic spells out the lesson: *"the size for values of type `str` cannot be known at compilation time ... required by an implicit `Sized` bound ... consider relaxing the implicit `Sized` restriction: `+ ?Sized`."* Once `T: ?Sized`, you must keep `T` behind a pointer (`&T`), because a bare `T` would have unknown size on the stack. Calling `last_byte(s)` where `s: &str` binds `T = str` (the unsized part), and the reference is yours.

### 4. Auto traits compose structurally

Now the real auto traits. `Send` = safe to **move** to another thread. `Sync` = safe to **share** `&T` across threads. You almost never `impl` these: the compiler grants them to a type *if and only if* every field already has them.

The classic way to *prove* a type carries an auto trait is a zero-cost witness function — all the work is in the bound, the body is empty:

```rust
fn assert_send<T: Send>() {}   // compiles to nothing; pure type-level check
fn assert_sync<T: Sync>() {}

struct Wrapper { id: u64, name: String, tags: Vec<u8> }

assert_send::<Wrapper>();       // OK — u64, String, Vec<u8> are all Send
assert_sync::<Wrapper>();       // OK
```

Nowhere did you write `impl Send for Wrapper`. The compiler walked the fields, found them all `Send`, and granted it. That is what "auto" means: opt-out, not opt-in.

### 5. The defining footgun: `Rc` is `!Send`

`Rc<T>` uses a plain, non-atomic reference count. If two threads cloned or dropped the same `Rc` at once, the count would race and you would get a use-after-free. So the standard library marks `Rc` as `!Send` and `!Sync`. Because `Send` is an auto trait, that one negative *poisons* anything containing an `Rc`.

```rust
// WRONG: thread::spawn requires its closure to be Send;
// capturing an Rc makes the closure !Send.
// let data = Rc::new(41);
// thread::spawn(move || *data + 1);   // error: `Rc<i32>` cannot be sent between threads safely

// OK: Arc has an ATOMIC refcount, so it is Send + Sync
fn parallel_sum(value: i32, n_threads: usize) -> i32 {
    let data = Arc::new(value);
    let handles: Vec<_> = (0..n_threads)
        .map(|_| {
            let data = Arc::clone(&data);   // each thread gets its own handle
            thread::spawn(move || *data)
        })
        .collect();
    handles.into_iter().map(|h| h.join().unwrap()).sum()
}
```

`Arc` and `Rc` have *identical* APIs. The only difference is the atomic refcount — and that single invariant is what earns the auto traits back. Collecting handles into a `Vec` *before* joining keeps all threads running concurrently.

### 6. Negative reasoning: opting out on purpose

Auto traits are reasoned about negatively, so to make your *own* type `!Send` when all its real fields are `Send`, you add a zero-sized field whose *type* is `!Send`. The canonical "thread-bound" token is `PhantomData<*const ()>` — a raw pointer is `!Send` and `!Sync`, and `PhantomData` carries that property at zero size.

```rust
struct ThreadBound {
    id: u32,                        // perfectly Send on its own
    phantom: PhantomData<*const ()>, // ...but this poisons Send + Sync
}
```

`ThreadBound` holds nothing but a `u32`, yet the compiler now refuses to move it across a thread boundary. Why does a raw pointer poison the auto traits? The compiler can't verify what it points to or who else touches it, so it conservatively refuses. This is exactly how `MutexGuard`, `Rc`, and thread-local handles keep themselves on one thread. `size_of::<ThreadBound>()` is still 4 — pure type-level enforcement.

> The ladder verifies `!Send`-ness at runtime with an autoref-specialization probe exposed as a macro (`is_send!(T)`). It must resolve at a *concrete* type — a generic `fn is_send<T>()` wrapper erases the `Send` info and always reports `false`.

### 7. `PhantomData` as a marker, and choosing its shape

`PhantomData<T>` lets a type carry a parameter `T` it never stores. The classic use is a **typed ID**: a `u64` tagged with which entity it belongs to, so `Id<User>` and `Id<Post>` are different types and mixing them is a compile error — at zero runtime cost (it is still just a `u64`).

The deep part is that **the marker shape inside `PhantomData` controls auto-trait and variance behavior**:

| `PhantomData<…>` | Meaning | Auto-trait effect |
|---|---|---|
| `PhantomData<T>` | "I own a T" | inherits T's `Send`/`Sync`; participates in drop check |
| `PhantomData<fn() -> T>` | "I produce T" (pure tag) | always `Send + Sync + Copy`, covariant, regardless of T |
| `PhantomData<*const T>` | thread-bound token | `!Send` + `!Sync` |

A typed ID is a *pure tag* — holding a user's id doesn't mean you own a `User`. So it should stay `Send`/`Sync`/`Copy` even if `User` is `!Send`. The right marker is `fn() -> T`:

```rust
struct Id<T> {
    raw: u64,
    _tag: PhantomData<fn() -> T>,   // pure tag: stays Send + Copy even if T is !Send
}
```

And the trait impls are **hand-written, not derived**, so the bound lands where it belongs:

```rust
// WRONG: #[derive(Clone)] emits `impl<T: Clone> Clone for Id<T>`
//        — needlessly requires the TAG to be Clone.
// OK: hand-write it with no bound on T; the requirement is on the u64.
impl<T> Clone for Id<T> {
    fn clone(&self) -> Self { Id::new(self.raw) }
}
impl<T> Copy for Id<T> {}            // valid because raw: u64 is Copy
impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool { self.raw == other.raw }
}
```

Now `fetch_user(some_post_id)` is a compile error — a whole class of "wrong ID" bugs deleted. This is how `sqlx`, ECS libraries, and unit-of-measure crates work.

### 8. `unsafe impl Send` done right

Sometimes auto-derivation is *too* conservative. A type built on a raw pointer is automatically `!Send`/`!Sync`, but you, the author, may *know* it is safe — and you take responsibility with `unsafe impl`. This is the manual opt-IN, the mirror of rung 6's opt-OUT.

```rust
struct MyBox<T> { ptr: *mut T }      // *mut T → compiler refuses Send/Sync

impl<T> MyBox<T> {
    fn new(value: T) -> Self { Self { ptr: Box::into_raw(Box::new(value)) } }
    fn get(&self) -> &T {
        // SAFETY: ptr came from Box::into_raw (non-null, aligned, initialized);
        // MyBox uniquely owns it until Drop; &self yields only a shared &T.
        unsafe { &*self.ptr }
    }
}
impl<T> Drop for MyBox<T> {
    fn drop(&mut self) {
        // SAFETY: unique owner; drop runs at most once, so from_raw reclaims exactly one Box.
        unsafe { drop(Box::from_raw(self.ptr)); }
    }
}

// Re-grant the auto traits — but only under the SAME bound the safe type needs.
unsafe impl<T: Send> Send for MyBox<T> {}   // moving the box moves the T → needs T: Send
unsafe impl<T: Sync> Sync for MyBox<T> {}   // get() shares &T → needs T: Sync
```

The crux is the **bound**. `MyBox` *owns* its `T`; moving the box to another thread moves the `T` there, which is sound only if `T: Send`. Write `unsafe impl<T> Send` with no bound and you could smuggle a `MyBox<Rc<_>>` across threads — the exact UB rung 5 prevents. This is character-for-character how `std::boxed::Box` grants its auto traits. The bound is the safety contract; the `// SAFETY:` comment is where you write down why it holds.

## Footguns

- **The invisible `Sized` bound.** `fn f<T>(x: T)` silently means `T: Sized` and rejects `str`/`[T]`/`dyn Trait`. Relax with `?Sized` and keep the value behind a pointer.
- **One bad field poisons an auto trait.** A single `Rc`, `Cell`, or raw-pointer field makes the whole struct lose `Send`/`Sync`. The error often points at `thread::spawn`'s `Send` bound, far from the offending field.
- **Wrong `PhantomData` shape.** `PhantomData<T>` drags T's thread-safety in; `PhantomData<fn() -> T>` keeps a pure tag `Send`/`Copy`; `PhantomData<*const T>` opts out. Picking the wrong one silently changes whether your wrapper crosses threads.
- **Deriving bounds onto phantom tags.** `#[derive(Clone)]` on `Id<T>` emits `impl<T: Clone>` — a needless bound on a tag you never store. Hand-write the impl so the requirement lands on the real fields.
- **Forgetting the state-marker field.** A generic `Conn<S>` that never uses `S` is `error[E0392]: parameter S is never used`. The fix is a `PhantomData<S>` field.
- **Over-promising in `unsafe impl`.** `unsafe impl<T> Send` (no bound) on an owning wrapper is unsound. Match the bound the safe abstraction would need (`T: Send`).

## Real-world patterns

- **`Box`, `Vec`, `Arc`** all use bounded `unsafe impl Send/Sync` over their internal raw pointers — exactly the rung-8 pattern.
- **Thread-bound handles** (`MutexGuard`, `Rc`, FFI/GUI context handles) use `PhantomData<*const ()>` (or `*mut`) to stay `!Send`, so the compiler enforces single-thread use.
- **Typed IDs / units of measure** (`sqlx`, ECS frameworks, `uom`) use `PhantomData<fn() -> T>` to get distinct types with zero runtime cost.
- **Sealed traits** (private supertrait) appear throughout std and crates like `serde` to mark a closed set of types that downstream code cannot extend.

## Capstone insight

The capstone builds a `Conn<S>` whose *state* is a type parameter, combining every thread of the ladder:

```rust
mod sealed { pub trait Sealed {} }       // private — only this crate can implement
trait State: sealed::Sealed { const NAME: &'static str; }

struct Disconnected; struct Connected; struct Authenticated;  // ZST markers
// impl Sealed + State for each...

struct Conn<S: State> {
    peer: String,
    log: Vec<String>,
    _state: PhantomData<S>,              // avoids E0392; zero size
}

impl Conn<Disconnected> { fn connect(self) -> Conn<Connected> { /* ... */ } }
impl Conn<Connected>    { fn authenticate(self, t: &str) -> Conn<Authenticated> { /* ... */ } }
impl Conn<Authenticated>{ fn send(&mut self, msg: &str) -> usize { /* ... */ } }  // ONLY here
impl<S: State> Conn<S>  { fn status(&self) -> &'static str { S::NAME } }          // every state
```

The "aha": four small features combine into a state machine the compiler checks for free.

- **ZST marker structs** are the states — `Send`, zero-size, behavior-free.
- **A sealed `State` trait** (private `Sealed` supertrait) is the marker that says "this is a legal state," and being sealed, *no downstream crate can invent a new state*. `impl State for Rogue` fails with "the trait `Sealed` is not satisfied."
- **`PhantomData<S>`** tags the state onto `Conn` at zero cost (and satisfies E0392).
- **Consuming transitions** (`self`, not `&self`) move the old handle away, so a stale `Conn<Connected>` *cannot* be reused after `authenticate`.

`send` exists only in `impl Conn<Authenticated>`, so `disconnected_conn.send(..)` is `no method named send` — a *compile* error, not a runtime check. Your protocol's rules became type errors. This is the engine behind `typed-builder`, embedded-HAL peripheral states, and session-typed protocols.

## Explain it back

- Why is `Copy` a marker trait even though `Clone` has the `clone` method?
- Why does `T: ?Sized` only make sense when the value is behind a pointer like `&T`?
- What exactly makes `Rc<T>` `!Send`, and which single property does `Arc<T>` change to fix it?
- Why does a `PhantomData<*const ()>` field make a struct `!Send`, while `PhantomData<()>` does not?
- When should the marker be `PhantomData<T>` vs `PhantomData<fn() -> T>`?
- Why is `unsafe impl<T: Send> Send for MyBox<T>` sound, but `unsafe impl<T> Send` too strong?
- How does a sealed supertrait make it impossible for downstream code to add a new typestate?

## See also

- [Static vs dynamic dispatch](dispatch.md) — `Sized` and object safety
- [Blanket impls & coherence](blanket-coherence.md) — bounds and the orphan rule
- [The typestate pattern](typestate.md) — the capstone, in depth
- [`Send` & `Sync` deeply](send-sync.md) — Phase 4 follow-up on thread safety
- [Newtype & zero-cost wrappers](newtype.md) — `PhantomData` typed IDs
