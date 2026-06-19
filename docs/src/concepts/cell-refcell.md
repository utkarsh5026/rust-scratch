# Cell & RefCell — Interior Mutability

> Ladder: [`src/bin/cell_refcell.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/cell_refcell.rs) ·
> Run: `cargo run --bin cell_refcell` · Phase 1 · 9 rungs

## TL;DR

Rust enforces **many `&T` XOR one `&mut T`** at compile time. Interior mutability
lets you mutate through a *shared* `&T` by upholding that same rule a different
way. `Cell<T>` never hands out references at all — it copies values in and out,
so no aliasing can occur. `RefCell<T>` hands out real `&T` / `&mut T`, but checks
the borrow rule **at runtime** (and panics if you break it). Both are `!Sync` —
single-threaded only; the multi-threaded counterparts are `Mutex` and `RwLock`.

> **Mental model:** `Cell` is a slot you can only peek at or swap.
> `RefCell` is a slot with a borrow-checker bouncer who works the night shift
> (runtime) instead of the day shift (compile time).

## Why this exists (from first principles)

The borrow checker is conservative. It enforces "many readers XOR one writer" at
compile time by tracking `&` and `&mut` through the type system. This is sound
and zero-cost — but it rejects programs that are *actually safe*:

```rust
struct Stats { count: u32 }

impl Stats {
    fn record(&mut self) { self.count += 1; }
    //        ^^^^^^^^^ requires exclusive access
}
```

If two parts of your program hold `&Stats`, neither can call `record` — the
compiler can't prove they won't alias. But you *know* you're single-threaded and
the mutation is fine. The compiler won't budge.

Interior mutability is the escape hatch: wrap the field in `Cell` or `RefCell`,
and the *type itself* enforces the aliasing rule (by copying or by runtime
checks), so the compiler can accept `&self` methods that mutate.

Without `Cell`/`RefCell`, you'd need `&mut` all the way up the call chain for
any mutation — which is often impossible when multiple owners (`Rc`) or callbacks
need to write.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `bump` via `Cell` | Mutate a `Copy` value through `&` with `get`/`set`. |
| 2 | foundations | `log` via `RefCell` | `borrow_mut()` to push into a Vec through `&`. |
| 3 | mechanics | Cell toolbox | `replace`, `take`, `update`, `into_inner`; `Cell<Option<T>>` for non-Copy. |
| 4 | mechanics | RefCell toolbox | `&self` methods that mutate; many coexisting borrows; `try_borrow`. |
| 5 | footgun | borrow panic | Overlap `borrow_mut` with `borrow` -- runtime panic. Fix by scoping. |
| 6 | footgun | `!Sync` + re-entrancy | RefCell can't cross threads; callback that re-borrows panics. |
| 7 | real-world | `Rc<RefCell<Node>>` | Shared mutable tree; mutate through one handle, see it through another. |
| 8 | real-world | `Ref::map` projection | Borrow a single field out of a RefCell without losing the guard. |
| 9 | capstone | `MyRefCell` from scratch | `UnsafeCell` + borrow flag + RAII guards. |

## The ideas, built up

### Cell: mutate by copying, never by reference

`Cell<T>` provides interior mutability for `Copy` types with zero runtime
overhead. The API is deliberately narrow — you can `get()` a copy of the value
and `set()` a new one, but you **never get a reference** to the contents:

```rust
fn bump(counter: &Cell<u32>, by: u32) {
    counter.set(counter.get() + by);
}
```

The signature is `&Cell<u32>`, not `&mut Cell<u32>` — two shared references can
both drive mutations because no aliasing reference to the inner `u32` ever
exists. The value is copied out, modified, and copied back in. This is why `get`
requires `T: Copy` — it can't hand you a reference (that would create aliasing),
so it must copy.

```rust
let counter = Cell::new(0u32);
let r1 = &counter;
let r2 = &counter;
bump(r1, 5);
bump(r2, 3);   // both shared refs can mutate — no &mut anywhere
assert_eq!(counter.get(), 8);
```

### The Cell toolbox: replace, take, update

`get`/`set` handle `Copy` types, but what about a `String` in a `Cell`? You
can't copy it out. The toolbox fills the gap with **swapping** operations:

| Method | What it does | Requires |
|--------|-------------|----------|
| `replace(new) -> old` | Store `new`, return the previous value | nothing |
| `take() -> T` | Store `T::default()`, return the previous value | `T: Default` |
| `update(f)` | `set(f(get()))` — read-modify-write in one shot | `T: Copy` |
| `into_inner() -> T` | Consume the Cell, extract the value | ownership |

The classic trick for non-Copy types: **`Cell<Option<T>>`**. You can `take()` the
`Option`, which replaces it with `None` (the `Default` for `Option`), giving you
the owned value without needing `Copy`:

```rust
fn steal(slot: &Cell<Option<String>>) -> Option<String> {
    slot.take()   // moves the String out, leaves None behind
}

let name = Cell::new(Some(String::from("ferris")));
assert_eq!(steal(&name), Some(String::from("ferris")));
assert_eq!(steal(&name), None);   // already taken
```

### RefCell: runtime borrow checking

Cell can't help when you need a reference to the contents — you can't `get()` a
`Vec` and push to it. `RefCell<T>` solves this by handing out real references,
guarded by a runtime borrow flag:

- `borrow() -> Ref<T>` : shared read borrow (many allowed)
- `borrow_mut() -> RefMut<T>` : exclusive write borrow (only one, no readers)

The returned `Ref`/`RefMut` are RAII guards. While they live, the borrow flag is
held. When they drop, the flag resets.

```rust
fn log(entries: &RefCell<Vec<String>>, msg: &str) {
    entries.borrow_mut().push(msg.to_string());
}
```

Again: `&RefCell`, not `&mut RefCell`. The RefCell enforces exclusivity at
runtime, so the compiler accepts the shared reference.

### The "&self that mutates" pattern

This is the real reason `RefCell` exists in practice. A struct wraps its mutable
state in `RefCell` and exposes all-`&self` methods — callers see a read-only
interface, but the struct mutates internally:

```rust
struct Stats {
    samples: RefCell<Vec<i32>>,
}

impl Stats {
    fn add(&self, n: i32) {           // &self, NOT &mut self
        self.samples.borrow_mut().push(n);
    }
    fn len(&self) -> usize {
        self.samples.borrow().len()
    }
    fn sum(&self) -> i32 {
        self.samples.borrow().iter().sum()
    }
}
```

This is how caches, loggers, lazy-init fields, and counters work in safe Rust
when `&mut self` isn't available.

**Multiple simultaneous read borrows are fine** — `borrow()` can be called many
times while other `Ref` guards are alive:

```rust
let a = s.samples.borrow();
let b = s.samples.borrow();    // both Refs alive — OK, many readers
assert_eq!(a.len(), b.len());
assert!(s.samples.try_borrow_mut().is_err());   // but a writer is refused
```

`try_borrow` / `try_borrow_mut` return `Result` instead of panicking — useful
when you're unsure whether a borrow is already active.

### Ref::map — projecting a borrow to a single field

A common need: borrow one field out of a `RefCell<Struct>`. You can't return a
plain `&str` — the `Ref` guard would drop at function end, resetting the borrow
flag, and the reference would dangle. The compiler won't let you.

`Ref::map` solves this by **transforming the guard** while keeping it alive:

```rust
fn borrow_name(c: &RefCell<Config>) -> Ref<'_, str> {
    Ref::map(c.borrow(), |cfg| cfg.name.as_str())
}

fn borrow_retries_mut(c: &RefCell<Config>) -> RefMut<'_, u32> {
    RefMut::map(c.borrow_mut(), |cfg| &mut cfg.retries)
}
```

The returned `Ref<str>` still holds the borrow flag down — a `try_borrow_mut`
will fail while it lives. When it drops, the flag releases. This lets you expose
fine-grained borrows of individual fields without leaking the whole struct.

## Footguns

### The runtime borrow panic (rung 5)

This is **the** defining `RefCell` hazard. Overlap a read borrow with a write
borrow and you get a panic at runtime, not a compile error:

```rust
fn trigger_panic(v: &RefCell<Vec<i32>>) {
    let _r = v.borrow();           // Ref alive for the rest of the scope
    v.borrow_mut().push(1);        // PANIC: "already borrowed"
}
```

The fix is **scope the borrow** — end the read borrow before taking the write
borrow. Copy what you need out, drop the `Ref`, then mutate:

```rust
fn duplicate_first(v: &RefCell<Vec<i32>>) {
    let first = v.borrow()[0];     // temporary Ref dropped at semicolon
    v.borrow_mut().push(first);    // now safe — no outstanding borrows
}
```

The trap is subtle: `v.borrow()[0]` creates a temporary `Ref` that lives only
for the expression. But `let r = v.borrow(); ... r[0]` keeps the `Ref` alive
until `r` goes out of scope. The difference between a temporary and a binding
is the difference between working code and a panic.

### Re-entrant borrow through a callback (rung 6)

The most insidious variant: a read borrow held during iteration, and a callback
that tries to write to the same `RefCell`:

```rust
fn each<F: FnMut(i32)>(v: &RefCell<Vec<i32>>, mut f: F) {
    for &x in v.borrow().iter() {   // Ref alive for the whole loop
        f(x);                        // if f() borrows v mutably -> PANIC
    }
}

fn double_into_buggy(v: &RefCell<Vec<i32>>) {
    each(v, |x| {
        v.borrow_mut().push(x * 2);   // re-entrant: panics
    });
}
```

The `borrow()` in `each` holds a `Ref` for the entire loop body. The closure
calls `borrow_mut()` on the same `RefCell` — boom. The two borrows aren't
adjacent in the source; the mutable one is buried in a closure. This is why
re-entrancy is the real danger with `RefCell`.

**The fix: snapshot and release.** Collect what you need, drop the read borrow,
*then* mutate:

```rust
fn double_into_fixed(v: &RefCell<Vec<i32>>) {
    let doubles = v.borrow().iter().map(|x| x * 2).collect::<Vec<_>>();
    v.borrow_mut().extend(doubles);
}
```

The `borrow()` is a temporary — it lives for the `collect()` expression and
drops before `borrow_mut()` is called.

### RefCell is !Sync

`RefCell`'s borrow flag is a plain `Cell<isize>` with no atomics. Sharing
`&RefCell` across threads would race on the flag. The compiler prevents this:
`RefCell<T>` is `!Sync`, so `std::thread::scope` with a shared `&RefCell` is a
compile error. The thread-safe equivalents are `Mutex` (one writer, blocks) and
`RwLock` (many readers or one writer, blocks).

## Real-world patterns

### Rc\<RefCell\<T\>\> — shared mutable state

`Rc` gives multiple owners. `RefCell` gives mutation through `&`. Together:
multiple handles to the same data, any of which can mutate it. This is how
graphs, trees, and observer state work in single-threaded Rust:

```rust
fn new_node(value: i32) -> Rc<RefCell<Node>> {
    Rc::new(RefCell::new(Node { value, children: vec![] }))
}

fn add_child(parent: &Rc<RefCell<Node>>, child: Rc<RefCell<Node>>) {
    parent.borrow_mut().children.push(child);
}
```

The payoff: mutate through one handle, observe through another — they share the
same underlying `RefCell`:

```rust
let root = new_node(1);
let a = new_node(2);
add_child(&root, Rc::clone(&a));

a.borrow_mut().value = 20;             // mutate through `a`
assert_eq!(sum_tree(&root), 1 + 20);   // see it through `root`
```

The threaded counterpart is `Arc<Mutex<T>>`.

### Caches and lazy fields

A struct with a `RefCell<Option<ExpensiveResult>>` can lazily compute and cache
a value through `&self`:

```rust
fn get_result(&self) -> Ref<'_, ExpensiveResult> {
    if self.cache.borrow().is_none() {
        *self.cache.borrow_mut() = Some(expensive_compute());
    }
    Ref::map(self.cache.borrow(), |opt| opt.as_ref().unwrap())
}
```

For single-init cases, `OnceCell` / `OnceLock` are simpler; `RefCell` shines
when the cached value can be invalidated and recomputed.

## Capstone insight

Building `MyRefCell<T>` from scratch reveals that the whole mechanism is just
three pieces:

**1. `UnsafeCell<T>`** — the *only* legal way to get a `*mut T` from a shared
`&T`. Any other route to `&T -> &mut T` is instant UB. `UnsafeCell` is the
compiler-blessed primitive that says "I know what I'm doing; don't optimize
based on immutability."

**2. A borrow flag** — a `Cell<isize>` tracking the state:

| Flag value | Meaning |
|-----------|---------|
| `0` | Free — no borrows outstanding |
| `> 0` | That many shared borrows are out |
| `-1` | One exclusive (mutable) borrow is out |

The rules:
- `borrow()`: panic if flag < 0 (writer out), else flag += 1.
- `borrow_mut()`: panic if flag != 0 (anyone out), else flag = -1.

**3. RAII guard types** — `MyRef` and `MyRefMut`. They `Deref` to the data
(via the `UnsafeCell`'s raw pointer), and their `Drop` impl restores the flag.
This is *why borrows auto-release* — when the guard goes out of scope, the
destructor runs and the flag resets:

```rust
impl<T> Deref for MyRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.cell.value.get() }
    }
}

impl<T> Drop for MyRef<'_, T> {
    fn drop(&mut self) {
        self.cell.flag.set(self.cell.flag.get() - 1);
    }
}

impl<T> Drop for MyRefMut<'_, T> {
    fn drop(&mut self) {
        self.cell.flag.set(0);
    }
}
```

The `unsafe` in `Deref` is sound because the flag guarantees the aliasing
invariant: if a `MyRef` exists, no `MyRefMut` can exist (flag would be -1, but
it's > 0), and vice versa. The flag is the proof obligation — get it right and
the `unsafe` is justified; get it wrong and you have UB.

Once you've written this, `RefCell` stops being magic. It's a `Cell<isize>`
counter plus two RAII types that hold it. The borrow checker didn't go away — it
moved into your flag arithmetic.

## Explain it back

- Why does `Cell::get` require `T: Copy`? What would go wrong if it handed out
  a `&T` instead?
- What is the exact runtime cost of `RefCell` compared to a plain `&mut`?
  (Hint: it's a flag check, not a lock.)
- When a `Ref` drops, what happens to the borrow flag? Why is this an RAII
  pattern?
- Why does `let _r = v.borrow(); v.borrow_mut().push(1);` panic, but
  `let x = v.borrow()[0]; v.borrow_mut().push(x);` doesn't?
- What makes `RefCell` `!Sync`? What's the thread-safe replacement?
- In the re-entrant callback rung, *where* is the read borrow still alive when
  the write borrow fires? Why can't the compiler catch this at compile time?
- What is `UnsafeCell` and why is it the foundation of *all* interior
  mutability in Rust?
- In `MyRefCell`, why is the `unsafe` in `Deref for MyRef` sound?

## See also

- [Rc\<RefCell\<T\>\> patterns](rc-refcell.md) — the full treatment of the
  `Rc<RefCell<T>>` combo: cycles, `Weak`, observer pattern, doubly-linked list.
- [Box & the Heap](box-heap.md) — sole ownership on the heap; `Box<dyn Trait>`
  is the owned trait-object counterpart to `Rc<RefCell<dyn Trait>>`.
- [Drop & Ordering](drop-ordering.md) — RAII guards and `mem::take`/`replace`,
  the same patterns that make `RefCell` guards and `Cell::take` work.
