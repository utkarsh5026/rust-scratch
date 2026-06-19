# `Rc` / `Arc`

> Ladder: [`src/bin/rc_arc.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/rc_arc.rs) ·
> Run: `cargo run --bin rc_arc` · Phase 1 · 9 rungs

## TL;DR

`Rc<T>` is **shared ownership by counting**. One heap allocation holds your
value *plus* a counter; every `Rc` handle is a pointer to that allocation and
owns one unit of the count. `clone()` bumps the count (cheap — it copies a
pointer, never the data); `drop` decrements it; when the count hits **0** the
value is freed exactly once. That's the entire machine. `Rc` only ever hands out
`&T` (shared, immutable access), which is what makes the counting sound. `Arc`
is the same machine with an **atomic** counter, so it can be shared across
threads; `Rc` uses a plain integer and is therefore single-threaded only. The
two failure modes to internalize: `Rc` gives you aliasing but not mutation
(reach for `make_mut` or `RefCell`), and a strong **reference cycle leaks**
because the counts never reach 0.

## Why this exists (from first principles)

Rust's default ownership is a **tree**: each value has exactly one owner, and
when that owner goes out of scope the value is freed. `Box<T>` is the canonical
single-owner heap pointer. This is wonderful — it makes "when is this freed?"
decidable at compile time with zero runtime bookkeeping — but it can't express
every shape.

Some data is a **DAG or a graph**: one node reachable from two parents, a value
several structs all need to keep alive, a string tag shared by thousands of
records. There is no single, statically-known owner. So the question "when is
this freed?" can't be answered at compile time. You need to answer it **at
runtime**, and the simplest correct answer is: *free it when the last user is
gone.* That requires counting users.

That is precisely `Rc`:

| Approach | Owners | Freed when | Cost |
|---|---|---|---|
| `T` (move) | exactly 1 | owner scope ends | none |
| `Box<T>` | exactly 1 (heap) | owner scope ends | one allocation |
| `Rc<T>` | many | **last** handle dropped | allocation + a counter, bumped per clone/drop |

What the compiler still guarantees, even with shared ownership: no
use-after-free (the value lives as long as any handle does) and no double-free
(only the `0`-transition frees). What it gives up: it can no longer prove the
value is *uniquely* owned, so it refuses to hand out `&mut T` through an `Rc`.
That single restriction — shared access only — is the source of everything
interesting in this ladder.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | two owners | `Rc::new` + `clone()` -> two handles, one allocation; `Rc::ptr_eq` proves it |
| 2 | foundations | the count moves | `strong_count` rises on `clone`, falls on scope-end drop; you can watch `[1,2,3,1]` |
| 3 | mechanics | shared diamond | one node owned by two parents — the shape `Box` *cannot* express |
| 4 | mechanics | `Rc<str>` | intern an immutable string once; N records share one allocation via cheap clones |
| 5 | mechanics | `make_mut` | clone-on-write: mutate in place when sole owner, copy when shared |
| 6 | footgun | the cycle leak | `a <-> b` strong cycle: counts never hit 0, **`Drop` never runs**, memory leaks |
| 7 | footgun -> fix | `Weak` breaks it | own down with `Rc`, point back with `Weak`; `downgrade` / `upgrade` |
| 8 | real-world | `Rc` is `!Send` | atomic `Arc` crosses threads; `Arc<Mutex<T>>` for shared mutation |
| 9 | capstone | `MyRc<T>` | build it from scratch: `NonNull` + `Cell<usize>` count, last drop frees once |

## The ideas, built up

### Two owners, one allocation

The foundational move is just `new` then `clone`:

```rust
fn two_owners(text: &str) -> (Rc<String>, Rc<String>) {
    let rc = Rc::new(text.to_string());
    (rc.clone(), rc.clone())
}
```

The original `rc` is moved out by the time the tuple is built (both elements are
clones), so we return two handles to the **same** `String`. The proof is not
that the values are equal — it's that they share an address:

```rust
let (a, b) = two_owners("shared");
assert_eq!(*a, "shared");
assert_eq!(*b, "shared");
assert!(Rc::ptr_eq(&a, &b));   // SAME allocation, not two copies
```

`Rc::ptr_eq` compares the raw pointer inside each handle. This is the literal
meaning of shared ownership: not two equal `String`s, but two pointers to one
`String`. `clone()` here copied 16 bytes of pointer + length + capacity... no,
it copied a *single* pointer-to-the-`Rc`-allocation and incremented a counter.
The heap `String` and its bytes were never touched.

### The count is the whole machine

`Rc`'s entire correctness rests on one number: `strong_count`. Rung 2 makes it
observable by sampling it at four moments:

```rust
fn count_lifecycle(rc: &Rc<String>) -> [usize; 4] {
    let a = Rc::strong_count(rc);        // 1: just the original

    let (b, c) = {
        let _rc2 = Rc::clone(rc);
        let b = Rc::strong_count(rc);    // 2: one clone alive
        let _rc3 = Rc::clone(rc);
        let c = Rc::strong_count(rc);    // 3: two clones alive
        (b, c)
    };                                   // _rc2, _rc3 drop here

    let d = Rc::strong_count(rc);        // 1: back to just the original
    [a, b, c, d]
}
```

The result is `[1, 2, 3, 1]`. `clone()` increments; the end of the inner scope
runs the `Drop` for `_rc2` and `_rc3`, each decrementing. Note the function
takes `&Rc<String>` — a *borrow* of a handle, which does **not** add an owner.
Only `clone()` does. This distinction (borrowing a handle vs. cloning it) is
worth burning in: passing `&Rc` lets you read the value or the count without
participating in ownership.

### The shared diamond — the shape `Box` can't make

This is *why* `Rc` exists, drawn out:

```
        top
       /    \
   left      right
       \    /
       shared      <- ONE node, owned by BOTH left and right
```

With `Box`, `shared` would need a single owner — `left` *or* `right`, not both.
`Rc` lets both branches hold a handle to the same node:

```rust
struct Node { name: String, children: Vec<Rc<Node>> }

let shared = Rc::new(Node { name: "shared".into(), children: vec![] });
let left  = Rc::new(Node { name: "left".into(),  children: vec![Rc::clone(&shared)] });
let right = Rc::new(Node { name: "right".into(), children: vec![Rc::clone(&shared)] });
```

After building it, the shared node's `strong_count` is **2** (held by `left`'s
and `right`'s children vectors), and the two paths to it are pointer-equal:

```rust
assert!(Rc::ptr_eq(shared_via_left, shared_via_right));
assert_eq!(Rc::strong_count(shared_via_left), 2);
```

This is a DAG. As long as you only ever follow edges *downward* (parent to
child), the counts behave and everything frees when the roots go. The moment you
add an edge *back upward* with a strong `Rc`, you get rung 6's leak.

### `Rc<str>` — interning an immutable string the cheap way

`Rc<T>` shines when `T` is large and immutable and shared widely. The classic
case: thousands of records all tagged `"electronics"`. Storing a `String` in
each is one heap allocation *per record*. Instead, allocate the string **once**
as `Rc<str>` and hand each record a clone:

```rust
fn tag_all(category: &str, n: usize) -> Vec<Rc<str>> {
    let rc: Rc<str> = Rc::from(category);   // ONE allocation of the bytes
    let mut tags = Vec::with_capacity(n);
    for _ in 0..n {
        tags.push(Rc::clone(&rc));          // each push: pointer copy + count bump
    }
    tags
}
```

All `n` elements are the same allocation:

```rust
let tags = tag_all("electronics", 4);
for t in &tags[1..] {
    assert!(Rc::ptr_eq(&tags[0], t));       // every tag clones the SAME Rc<str>
}
assert_eq!(Rc::strong_count(&tags[0]), 4);  // the count sees all four
```

> **`Rc<str>` vs `Rc<String>`.** `Rc<String>` is a *double* indirection:
> `Rc` -> `String` (ptr/len/cap on the heap) -> the bytes. `Rc<str>` stores the
> length in the `Rc`'s fat pointer and points directly at the bytes — one
> indirection, no `String` header. For an immutable shared string, `Rc<str>` is
> the leaner choice. Build it with `Rc::from(&str)` or `.into()`. The same logic
> gives `Rc<[T]>` for shared immutable slices.

### `make_mut` — clone-on-write through a shared handle

`Rc` won't give you `&mut T` directly, because while other handles exist a
mutation would be visible through them and break aliasing. `Rc::make_mut`
resolves this by *checking the count first*:

```rust
fn push_isolated(rc: &mut Rc<Vec<i32>>, value: i32) {
    Rc::make_mut(rc).push(value);
}
```

- **Sole owner (`count == 1`)**: hands you `&mut T` to the existing allocation —
  mutate in place, no copy.
- **Shared (`count > 1`)**: clones the inner `T` into a fresh allocation, points
  *this* `Rc` at the clone, and gives you `&mut` to that. The other owners keep
  seeing the original. This is the "write" half of copy-on-write.

The ladder proves both branches. Sole owner mutates in place — same address
before and after:

```rust
let mut solo = Rc::new(vec![1, 2, 3]);
let addr_before = Rc::as_ptr(&solo);
push_isolated(&mut solo, 4);
assert_eq!(Rc::as_ptr(&solo), addr_before);   // no reallocation
```

Shared owner forces a copy that isolates the writer:

```rust
let original = Rc::new(vec![1, 2, 3]);
let mut writer = Rc::clone(&original);         // count == 2
push_isolated(&mut writer, 99);

assert_eq!(*writer,   vec![1, 2, 3, 99]);      // writer sees its push
assert_eq!(*original, vec![1, 2, 3]);          // original UNCHANGED
assert!(!Rc::ptr_eq(&original, &writer));      // writer points at a fresh clone
assert_eq!(Rc::strong_count(&original), 1);    // the split made each sole again
assert_eq!(Rc::strong_count(&writer),   1);
```

This is exactly the `Cow` mental model, but the "am I shared?" test is the
refcount rather than an explicit enum tag. It's how `Rc::make_mut` and friends
power cheap, structural-sharing-friendly data structures.

### The reference cycle that leaks — the defining `Rc` failure

`Rc` frees its value when `strong_count` reaches 0. So what if two nodes hold
strong handles to each other?

```rust
struct Cycle { name: &'static str, link: RefCell<Option<Rc<Cycle>>> }

fn make_leaky_cycle() {
    let a = Rc::new(Cycle::new("a"));
    let b = Rc::new(Cycle::new("b"));
    a.link.borrow_mut().replace(Rc::clone(&b));   // a -> b (strong)
    b.link.borrow_mut().replace(Rc::clone(&a));   // b -> a (strong)
}   // a and b go out of scope here
```

(The `RefCell` is only there because the back-edge must be wired *after* both
nodes exist — you need interior mutability to mutate `a` once it's already in an
`Rc`.)

Walk the counts. After wiring, `a` has 2 strong owners (the local `a` + `b`'s
link); same for `b`. When the function returns, the locals `a` and `b` drop —
each count falls from 2 to **1**, never to 0, because each node's `link` still
holds the other. Neither `Drop` ever fires:

```rust
let drops = DROP_COUNT.with(|c| c.get());
assert_eq!(drops, 0, "expected the cycle to LEAK (0 drops)");
```

This is **safe** code. Rust guarantees no use-after-free and no double-free — it
does **not** guarantee no leaks. An `Rc` cycle is the single-threaded equivalent
of an object graph that's unreachable but uncollected: the memory is gone for
the rest of the program.

### `Weak` breaks the cycle — the parent/child tree

The fix is `Weak<T>`: a handle that points at the allocation and bumps the
**weak** count, but never the **strong** count. Because it doesn't touch the
strong count, a `Weak` can't keep a value alive, so a chain of weak edges can't
form a keep-alive cycle. To use one you must `upgrade()` it — which returns
`Option<Rc<T>>`, `Some` if the target is still alive, `None` if it's gone.

The ownership rule that makes graphs leak-free:

> The direction that **owns** uses `Rc` (strong).
> The direction that merely **refers back** uses `Weak`.

In a tree: parent -> child is strong (the parent owns its children); child ->
parent is weak (a child can navigate up but must not pin its parent alive).

```rust
struct TreeNode {
    name: &'static str,
    parent: RefCell<Weak<TreeNode>>,        // weak: does NOT own
    children: RefCell<Vec<Rc<TreeNode>>>,   // strong: owns
}

fn link_parent_child(parent: &Rc<TreeNode>, child: &Rc<TreeNode>) {
    parent.children.borrow_mut().push(Rc::clone(child));   // strong down
    *child.parent.borrow_mut() = Rc::downgrade(parent);    // weak up
}

fn parent_name(child: &Rc<TreeNode>) -> &'static str {
    child.parent.borrow().upgrade()
        .map(|p| p.name)
        .unwrap_or("<no parent>")           // None if the parent is gone
}
```

The counts confirm the weak edge is free:

```rust
assert_eq!(Rc::strong_count(&root), 1);   // ONLY the `root` binding owns it
assert_eq!(Rc::strong_count(&leaf), 2);   // `leaf` binding + root.children
```

And the payoff — dropping the parent actually frees it, *and* the child's weak
pointer correctly reports the parent is gone:

```rust
drop(root);
assert_eq!(parent_name(&leaf), "<no parent>");   // upgrade() now returns None
```

When both nodes leave scope, **both** `Drop`s run (the test asserts 2 drops) —
no leak, unlike rung 6. `Rc::downgrade(&rc)` makes a `Weak` from an `Rc`;
`weak.upgrade()` tries to promote it back, succeeding only while a strong owner
remains.

### `Rc` is `!Send` -> `Arc` across threads

`Rc`'s counter is a plain `usize`. If two threads cloned/dropped the same `Rc`
concurrently, their increments and decrements could interleave and corrupt the
count — leading to a double-free or a leak. Rust forbids this at **compile
time** by making `Rc: !Send`: you literally cannot move one into another thread.

```rust
// WRONG — won't compile:
// let data = Rc::new(0);
// thread::spawn(move || { let _ = data; });
// error: `Rc<i32>` cannot be sent between threads safely
```

`Arc` ("atomic Rc") is the same machine with an **atomic** counter. The atomic
increment/decrement is safe under contention, so `Arc` is `Send + Sync` and
crosses threads. But `Arc`, like `Rc`, still only gives **shared** access — to
*mutate* shared state across threads you wrap the data in a lock: `Arc<Mutex<T>>`.
`Arc` shares the lock; the `Mutex` hands out `&mut T` to one thread at a time.

```rust
fn concurrent_count(n_threads: usize, per_thread: usize) -> usize {
    let counter = Arc::new(Mutex::new(0usize));
    let handles = (0..n_threads).map(|_| {
        let counter = Arc::clone(&counter);     // each thread gets its own handle
        thread::spawn(move || {
            let mut counter = counter.lock().unwrap();
            *counter += per_thread;
        })
    }).collect::<Vec<_>>();

    for h in handles { h.join().unwrap(); }
    *counter.lock().unwrap()
}
```

```rust
assert_eq!(concurrent_count(8, 10_000), 80_000);   // no lost updates
```

> **Two different counters.** `Arc`'s *atomic* counter protects the
> **reference count** (how many handles exist). The `Mutex` protects the
> **data**. Atomicity of the refcount does *not* make the inner value
> thread-safe to mutate — that's the `Mutex`'s job. `Arc<T>` alone gives shared
> reads; `Arc<Mutex<T>>` gives synchronized writes.

Atomic operations cost more than a plain integer bump, which is why `Rc` exists
at all: when you're single-threaded, you shouldn't pay for atomics. `Rc` and
`Arc` are otherwise the same API.

## Capstone insight: build `MyRc<T>` from scratch

The capstone strips `Rc` to its essence and reveals there's no magic — just one
heap box holding `{ count, value }` and a pointer to it.

```rust
struct MyRcInner<T> {
    strong: Cell<usize>,   // Cell: mutate the count through a shared &self
    value: T,
}

struct MyRc<T> {
    ptr: NonNull<MyRcInner<T>>,
    _marker: PhantomData<MyRcInner<T>>,   // "I logically own a T" for drop-check
}
```

Two design choices encode deep facts about real `Rc`:

- **`strong: Cell<usize>`** — the count must be mutable through `&self` (clone
  and drop both take shared references), so it needs interior mutability. A
  `Cell` (non-atomic) is exactly why real `Rc` is `!Sync`: a non-atomic counter
  is unsafe to touch from two threads. `Arc` swaps this for `AtomicUsize`.
- **`PhantomData<MyRcInner<T>>`** — we hold the value behind a raw `NonNull`, so
  the compiler can't see that `MyRc` owns a `T`. The marker tells dropck "I own
  a `T`," which makes drop-checking correct for `T`s with lifetimes.

The four operations *are* the machine:

```rust
fn new(value: T) -> MyRc<T> {                       // allocate inner, strong = 1
    let inner = Box::new(MyRcInner { strong: Cell::new(1), value });
    MyRc { ptr: NonNull::new(Box::into_raw(inner)).unwrap(), _marker: PhantomData }
}

fn clone(&self) -> MyRc<T> {                        // bump count, copy the pointer
    self.inner().strong.set(self.inner().strong.get() + 1);
    MyRc { ptr: self.ptr, _marker: PhantomData }
}

fn deref(&self) -> &T {                             // SHARED access only
    &self.inner().value
}

fn drop(&mut self) {
    if self.inner().strong.get() == 1 {             // I'm the last one
        unsafe { drop(Box::from_raw(self.ptr.as_ptr())); }   // free once, runs T's Drop
    } else {
        self.inner().strong.set(self.inner().strong.get() - 1);  // others remain
    }
}
```

The whole correctness argument: `new` starts at 1, `clone` adds 1 and shares the
pointer, `drop` either frees (on the `1`-transition, reconstructing the `Box` so
its destructor runs `T`'s `Drop` exactly once) or decrements. The verification
uses a `Dropper` that logs its own drop to prove the inner value is freed
**exactly once** — not zero (leak), not twice (double-free):

```rust
let a = MyRc::new(Dropper("payload"));
{
    let b = MyRc::clone(&a);
    assert_eq!(MyRc::strong_count(&a), 2);   // clone bumped the shared count
    assert_eq!(a.ptr, b.ptr);                // same inner, no deep copy
}   // b drops: count 2 -> 1, inner still alive
assert_eq!(DROP_COUNT, 0);                   // nothing freed yet
// ... a drops: count 1 -> 0, Dropper runs once
```

Once you've written these four functions, `Rc` stops being a black box. It's a
counter, a pointer, and the discipline of freeing on the last drop — and `Arc`
is the same four functions with `Cell` swapped for an atomic.

> Reaching for `unsafe` here is unavoidable (raw pointer deref, manual free), so
> this is the rung to validate with **Miri**: `cargo miri run --bin rc_arc`
> catches a leak, a double-free, or use-after-free that a normal run might miss.

## Footguns

- **`Rc` gives you aliasing, not mutation.** `Rc<T>` only ever yields `&T`. To
  mutate, either use `Rc::make_mut` (clone-on-write — fine when sharing is rare)
  or stack a `RefCell`: `Rc<RefCell<T>>` (runtime-checked shared mutation). See
  the [`Rc<RefCell<T>>` note](rc-refcell.md).

- **Strong cycles leak — silently.** `a` strong-points at `b` and vice versa ->
  neither count reaches 0 -> destructors never run. Safe Rust prevents
  use-after-free and double-free; it does **not** prevent leaks. Fix: make the
  back-edge `Weak`.

- **The ownership rule for back-pointers:** the direction that *owns* is `Rc`
  (strong); the direction that merely *navigates back* is `Weak`. Parent -> child
  strong, child -> parent weak.

- **`Weak::upgrade()` can return `None`.** A `Weak` doesn't keep the value alive,
  so by the time you `upgrade()` the target may be gone. You *must* handle the
  `None` — that's the whole point of `Weak`.

- **`Rc` is `!Send` / `!Sync`.** You cannot move it across threads, by design —
  its counter isn't atomic. Use `Arc` for that. But don't reach for `Arc`
  reflexively when single-threaded: you'd pay for atomics you don't need.

- **`Arc` shares; it doesn't synchronize the data.** `Arc<T>` gives shared
  reads. For cross-thread *mutation* you still need a `Mutex`/`RwLock`:
  `Arc<Mutex<T>>`. The atomic refcount protects the *handle count*, not the
  *value*.

- **`Rc<String>` is a double indirection.** Prefer `Rc<str>` (or `Rc<[T]>`) for
  shared immutable strings/slices — one fewer pointer hop and no `String` header.

## Real-world patterns

| Pattern | Shape | Example |
|---|---|---|
| **Interned immutable data** | `Rc<str>` / `Rc<[T]>` cloned across many records | Category tags, symbol tables, shared config |
| **Shared DAG node** | One node held by several parents via `Rc` | Expression trees with common sub-expressions, scene graphs |
| **Copy-on-write** | `Rc::make_mut` mutates in place when unshared, copies when shared | Persistent/immutable data structures, `Cow`-like APIs |
| **Tree with parent pointers** | children `Rc` (own), parent `Weak` (navigate back) | DOM, file-system models, ASTs with parent links |
| **Cross-thread shared state** | `Arc<Mutex<T>>` / `Arc<RwLock<T>>` | Counters, caches, connection pools, shared registries |
| **Cheap immutable snapshots** | hand out `Arc<T>` clones of a config/state | Hot-reloadable config, lock-free read paths |

## Explain it back

- What two things live in an `Rc`'s heap allocation, and what does a single
  `Rc` handle own?
- Why is `clone()` on an `Rc` cheap, and what exactly gets copied?
- Give a data shape `Box` cannot express but `Rc` can. Why not?
- When does `Rc::make_mut` mutate in place, and when does it copy? What decides?
- Walk the strong counts through an `a <-> b` strong cycle as the locals drop.
  Which count stays non-zero, and what's the consequence?
- In a parent/child tree, which edge is `Rc` and which is `Weak`? What leaks if
  you swap them?
- What does `Weak::upgrade()` return, and when is it `None`?
- Why is `Rc` `!Send`? What does `Arc` change, and what does it *not* change
  about mutating the inner value?
- Which two distinct things does `Arc<Mutex<T>>` protect, and with which
  mechanism each?
- In `MyRc`, why is the count a `Cell<usize>` rather than a plain `usize`, and
  what would you change to get `Arc`? Why does the last `drop` free exactly once?

## See also

- [`Rc<RefCell<T>>` patterns](rc-refcell.md) — add interior mutability on top of
  the shared-ownership layer built here; the cycle/`Weak` story in full
- [`Cell` / `RefCell`](cell-refcell.md) — the interior-mutability layer (the
  `make_mut` and capstone `Cell<usize>` both rely on it)
- [`Drop` & Ordering](drop-ordering.md) — why a cycle means destructors never
  run, and how the last `Rc` drop triggers the free
- [`Cow` — Clone-on-Write](cow.md) — `make_mut` is the refcount-driven version of
  the same copy-on-write idea
