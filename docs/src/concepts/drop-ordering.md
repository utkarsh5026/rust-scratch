# Drop & Ordering

> Ladder: [`src/bin/drop_ordering.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/drop_ordering.rs) ·
> Run: `cargo run --bin drop_ordering` · Phase 1 · 9 rungs

## TL;DR

When a value goes out of scope, Rust runs its destructor automatically — no GC,
no `free()`, no forgetting. The `Drop` trait gives you a hook into that moment.
The real depth is in **ordering**: locals drop in reverse declaration order
(LIFO), struct fields drop in declaration order (FIFO), and the compiler inserts
hidden **drop flags** so a conditionally-moved value is dropped exactly once.
`mem::forget`, `mem::replace`, and `ManuallyDrop` give you escape hatches when
the defaults don't fit. The payoff is **RAII**: tie any cleanup action to a
scope, and it runs on every exit path — normal return, early return, or panic.

## Why this exists (from first principles)

C gives you `malloc`/`free` and hopes you pair them. C++ gives you destructors
but lets you misuse them (double free, use after free). Garbage collectors solve
the pairing problem but add latency spikes and can't manage non-memory resources
(file handles, locks, network connections) without finalizers that run "sometime,
maybe."

Rust's answer: **ownership determines cleanup**. Every value has exactly one
owner. When that owner's scope ends, the value is dropped — deterministically,
immediately, in a well-defined order. The `Drop` trait is the hook that lets you
run code at that moment.

This determinism is what makes RAII (Resource Acquisition Is Initialization) a
first-class pattern: a `MutexGuard` unlocks on drop, a `File` flushes and
closes, a `TempDir` deletes itself. The compiler *guarantees* the cleanup runs,
and ownership *guarantees* it runs exactly once.

But "exactly once, in a well-defined order" means you need to know that order.
And you need tools for the cases where the default order is wrong, or where you
want to skip the destructor entirely. That's what this ladder teaches.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | Drop at scope end | `impl Drop` logs when a value dies — destructor is automatic |
| 2 | foundations | Local drop order | Locals drop in **reverse** declaration order (LIFO) |
| 3 | mechanics | Struct & nested order | Container's `drop()` runs first; fields drop in **declaration** order |
| 4 | mechanics | Early drop | `std::mem::drop(x)` ends a value early; `x.drop()` is `E0040` |
| 5 | footguns | Drop flags | Conditional moves tracked at runtime — no double drop, ever |
| 6 | footguns | forget / take / replace | `mem::forget` leaks; `mem::replace` moves a value out of `&mut` |
| 7 | real-world | RAII scope guard | A closure that runs on drop, with `.cancel()` to disarm |
| 8 | real-world | ManuallyDrop | Suppress auto-drop; choose your own field-drop order |
| 9 | capstone | Rollback-on-drop Transaction | Drop + drop flag + forget = auto-rollback unless committed |

## The ideas, built up

### Drop fires at scope end — and you don't call it

The `Drop` trait has one method:

```rust
impl Drop for Noisy {
    fn drop(&mut self) {
        log(format!("drop {}", self.name));
    }
}
```

The compiler inserts a call to this at the end of the owning scope. You never
call `drop()` yourself — in fact, `x.drop()` is a hard compiler error (`E0040:
explicit use of destructor method`). The reason: after your `drop(&mut self)`
body runs, the compiler still drops each field. If you could call `.drop()` on a
live binding, the automatic scope-end drop would run the destructor *again* —
double free. So the compiler forbids the direct call entirely.

To drop early, you use the free function `std::mem::drop(x)`, which takes `x`
**by value**. Ownership moves into `drop()`, the value dies at the end of that
tiny function, and `x` is now moved-from — using it again is a compile error.
That's the mechanism that prevents double free: not a runtime check, but a
move.

### Two orderings to memorize

Here is where people get confused, because locals and struct fields follow
**opposite** rules:

| What | Drop order | Why |
|------|-----------|-----|
| **Locals** in a scope | **Reverse** declaration order (LIFO) | Like a stack: last declared = first cleaned up. This mirrors C++ and ensures that later locals (which might reference earlier ones) die first. |
| **Struct fields** | **Declaration** order (FIFO) | Top to bottom, as written in the struct definition. The container's own `Drop::drop()` runs *before* any field drops. |

The ladder makes this concrete with `Pair { id, a: Noisy, b: Noisy }`:

```rust
impl Drop for Pair {
    fn drop(&mut self) {
        log(format!("drop pair {}", self.id));
    }
}
```

Dropping a `Pair` produces: `["drop pair P", "drop a", "drop b"]`. The
container's body runs first (while fields are still alive — you can read them in
your `drop()`), then fields drop in declaration order: `a` before `b`.

This is the opposite of locals. If you declared `let a; let b;` in a function,
you'd get `b` before `a`. But if `a` and `b` are fields, you get `a` before
`b`.

**Why the container drops first:** Your `Drop` impl gets `&mut self`, meaning
it can still read all the fields. If fields dropped first, your `drop()` body
would be reading dangling references. So the container must go first.

### Drop flags: the compiler's runtime bookkeeping

Consider this:

```rust
fn conditional_move(take_it: bool) -> Vec<String> {
    let x = Noisy::new("x");
    if take_it {
        consume(x);  // x moved into consume, drops there
    }
    // scope end: does x need dropping?
}
```

When `take_it` is true, `x` is moved into `consume()` and drops inside it. When
false, `x` is still alive at scope end and drops there. Either way, `x` drops
**exactly once**. But the compiler can't know at compile time which branch ran.

The solution: a hidden boolean on the stack — a **drop flag** — next to `x`. It
starts as "needs dropping." When `x` is moved, the flag is cleared. At scope
end, the compiler checks the flag and only drops if it's still set.

You never write this flag. You never see it. But it's there, and it's how Rust
guarantees "exactly once" even across conditional control flow. The cost is one
byte and one branch per conditionally-moved value — cheap insurance against
double free or leak.

### forget, replace, take: bending the rules

Three `std::mem` functions that give you manual control over when (or whether)
destructors run:

**`mem::forget(x)`** — moves `x` in and does **not** drop it. The destructor
never runs; the value leaks. This is *safe* (leaking memory isn't undefined
behavior in Rust), and it's how you hand ownership to something that will clean
up later (FFI, `ManuallyDrop`, or intentional leaks like `Box::leak`).

```rust
let x = Noisy::new("leaked");
std::mem::forget(x);
// log is EMPTY — "drop leaked" never appears
```

**`mem::replace(&mut dst, new)`** — swaps `new` into the location behind a
mutable reference and returns the old value. This is **the only way** to move a
non-Copy value out of `&mut self`. You can't write `let v = self.field;` — that
would move out of a borrow (`E0507`). You have to swap something in to take
something out:

```rust
impl Slot {
    fn swap_in(&mut self, replacement: Noisy) -> Noisy {
        std::mem::replace(&mut self.inner, replacement)
    }
}
```

**`mem::take(&mut dst)`** is `replace` with `Default::default()` as the
replacement. It's the idiomatic way to pull a value out of an `Option`, a
`Vec`, or anything with a sensible default.

The key insight: `replace` and `take` don't drop anything. They relocate the
old value into your hands. *You* decide when (or whether) it drops.

### RAII scope guard: the reason Drop exists

The killer application of `Drop` is tying a **cleanup action** to a scope. A
`Guard` owns a closure and runs it when dropped — no matter how the scope
exits:

```rust
struct Guard<F: FnOnce()> {
    action: Option<F>,
}

impl<F: FnOnce()> Drop for Guard<F> {
    fn drop(&mut self) {
        if let Some(action) = self.action.take() {
            action();
        }
    }
}
```

There's a real puzzle here. `drop()` receives `&mut self`, but an `FnOnce`
closure must be called **by value** (consumed). You can't move `self.action`
out of a mutable reference — that's `E0507` again. The solution is the rung-6
trick: store the closure in an `Option<F>` and `.take()` it (which is
`mem::replace` with `None`). Now you have an owned `F` you can call.

**`.cancel()`** disarms the guard: set `self.action = None` before the scope
ends, and `drop()` finds nothing to run.

```rust
impl<F: FnOnce()> Guard<F> {
    fn cancel(mut self) {
        self.action = None;
    }
}
```

This is exactly how `MutexGuard`, `File`, `scopeguard::defer!`, and every
"undo on error" pattern works.

### ManuallyDrop: suppressing the compiler's destructor

`ManuallyDrop<T>` wraps a value and tells the compiler: **do not drop this
automatically**. The wrapped value will leak unless you explicitly call the
unsafe `ManuallyDrop::drop(&mut md)`.

Why it exists: it's the only way to **override the fixed field-drop order**.
Normally fields `a, b` drop in declaration order (`a` then `b`). With
`ManuallyDrop`, you take control:

```rust
struct Custom {
    a: ManuallyDrop<Noisy>,
    b: ManuallyDrop<Noisy>,
}

impl Drop for Custom {
    fn drop(&mut self) {
        // SAFETY: dropping each field exactly once, never used afterward.
        unsafe {
            ManuallyDrop::drop(&mut self.b);  // b first
            ManuallyDrop::drop(&mut self.a);  // then a
        }
    }
}
```

This produces `["drop b", "drop a"]` — the reverse of the default. The
`unsafe` is genuine: calling `ManuallyDrop::drop` twice on the same field is
undefined behavior (double free). You must uphold the invariant that each field
is dropped exactly once and never read afterward.

`ManuallyDrop` is also how `Vec` manages element drops internally — it wraps
its allocation in `ManuallyDrop` so it can drop elements one by one in its own
`Drop` impl, rather than relying on the compiler's default.

### Capstone: rollback-on-drop Transaction

The ladder's synthesis rung combines everything into a pattern used by every
database driver, every temp-file-unless-kept, every undo-on-error mechanism:

```rust
struct Transaction<'a> {
    db: &'a mut Vec<String>,
    added: usize,
    committed: bool,
}
```

The pieces:

- **`begin(db)`** — borrows the database mutably, starts with 0 rows added and
  `committed: false`.
- **`insert(row)`** — pushes the row onto `db` and increments `added`.
- **`commit(mut self)`** — sets `self.committed = true`. Takes `self` by value,
  so the guard is consumed and `drop()` runs with the flag set.
- **`Drop`** — if `!self.committed`, pops `self.added` rows back off and logs
  `"rollback"`. If committed, does nothing.

```rust
impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            for _ in 0..self.added {
                self.db.pop();
            }
            log("rollback");
        }
    }
}
```

The `committed` field is a hand-written drop flag. `commit()` sets it to
`true`, disarming the rollback — exactly like `Guard::cancel()` from rung 7.
The difference: here the state mutation (the inserts) happens eagerly, and
rollback *undoes* it, whereas the guard defers the action entirely.

The critical test: rollback fires **during panic unwinding** too. A
`catch_unwind` around a panicking transaction proves the rows are rolled back
even on the exceptional path. This is the whole point of RAII — cleanup on
*every* exit, not just the happy path.

## Footguns

- **Assuming locals and fields drop in the same order.** They don't. Locals
  are LIFO (reverse declaration); fields are FIFO (declaration order). Getting
  this wrong causes subtle resource-ordering bugs (e.g., dropping a lock guard
  before the data it protects).

- **Calling `x.drop()` directly.** The compiler forbids it (`E0040`). Use
  `std::mem::drop(x)` instead — it moves `x` by value, so ownership transfer
  prevents double free.

- **Forgetting that `mem::forget` is safe.** It doesn't cause UB, but it does
  leak. Any cleanup you rely on (flushing buffers, releasing locks, temp file
  deletion) is skipped. Code must be correct even if `Drop` never runs —
  that's why `mem::forget` being safe is a design choice, not a bug.

- **Moving out of `&mut self` in `drop()`.** You can't do `let f = self.field;`
  because `drop()` only gets a mutable borrow. The workaround is
  `Option::take()` (which is `mem::replace` with `None`) to get an owned value
  you can consume.

- **Double `ManuallyDrop::drop`.** Unlike everything else on this list, this
  *is* undefined behavior. Once you call `ManuallyDrop::drop(&mut md)`, the
  inner value is gone. Calling it again is a double free. There's no compiler
  protection here — you're in `unsafe` territory.

## Signatures to know

```rust
// The Drop trait — one method, &mut self, no return
trait Drop {
    fn drop(&mut self);
}

// Free function: takes ownership, value dies at end
fn std::mem::drop<T>(x: T) {}

// Leak: takes ownership, destructor is skipped
fn std::mem::forget<T>(x: T) {}

// Swap a new value in, get the old one back
fn std::mem::replace<T>(dest: &mut T, src: T) -> T

// replace with Default::default()
fn std::mem::take<T: Default>(dest: &mut T) -> T

// Wrapper that suppresses automatic drop
struct ManuallyDrop<T> { /* ... */ }
impl<T> ManuallyDrop<T> {
    fn new(value: T) -> Self;
    unsafe fn drop(slot: &mut ManuallyDrop<T>);
}
```

## Real-world patterns

| Pattern | Uses | Example |
|---------|------|---------|
| **RAII guard** | Drop runs cleanup on scope exit | `MutexGuard` unlocks, `File` closes, `TempDir` deletes |
| **Commit/rollback** | Drop flag disarms destructor on success | Database transactions, staged file writes |
| **Scope guard with cancel** | `Option::take()` in `drop()` | `scopeguard` crate, the `Guard<F>` from rung 7 |
| **Custom field order** | `ManuallyDrop` + unsafe `drop` | `Vec` dropping elements before freeing the allocation |
| **Intentional leak** | `mem::forget` / `ManuallyDrop` | `Box::leak`, handing ownership to FFI |
| **Move out of `&mut`** | `mem::replace` / `mem::take` | Consuming an `FnOnce` stored behind a borrow |

## Explain it back

- Why are locals dropped in reverse order but struct fields in declaration order?
- Why does `x.drop()` produce a compiler error, and what do you use instead?
- What is a drop flag, and when does the compiler insert one?
- Is `mem::forget` safe? Why or why not — and what are the consequences?
- How do you move a value out of `&mut self` inside a `drop()` implementation?
- What happens if you call `ManuallyDrop::drop` twice on the same field?
- In the `Transaction` capstone, what happens if `commit()` is never called and the scope exits via panic?
- Why does the `Guard`'s action field need to be `Option<F>` rather than just `F`?

## See also

- [Cow](cow.md) — uses `mem::replace` internally for the `to_mut()` upgrade
- [Borrow / ToOwned](borrow-toowned.md) — the `MyCow` capstone also hits the "move out of enum variant" pattern
