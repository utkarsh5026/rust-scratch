# `Send` & `Sync` deeply

> Ladder: [`src/bin/send_sync.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/send_sync.rs) ·
> Run: `cargo run --bin send_sync` · Phase 4 · 9 rungs

## TL;DR

Two marker traits decide what is allowed to cross a thread boundary, and they mean
exactly two different things:

- **`T: Send`** — it is safe to **move ownership** of a `T` to another thread.
- **`T: Sync`** — it is safe to **share `&T`** between threads. Formally:
  `T: Sync  ⟺  &T: Send`.

Both are **auto traits**: the compiler implements them for you, *structurally*, from
your fields. There is no `#[derive(Send)]`. A struct is `Send` iff every field is
`Send`; `Sync` iff every field is `Sync`. One non-`Send` field poisons the whole
type — like a single rotten apple.

The two axes are **independent**. The four combinations all exist, and the
surprising ones (`Cell` is `Send` but not `Sync`; `MutexGuard` is `Sync` but not
`Send`) fall straight out of the one question: *can a **reference** to this safely
cross threads?*

## Why this exists (from first principles)

A data race is two threads touching the same memory at the same time with at least
one write and no synchronization. It is undefined behavior in every systems
language. Most languages fight data races at runtime (locks you must remember to
take) or not at all. Rust eliminates a whole class of them *at compile time* — and
`Send`/`Sync` are the mechanism.

The insight: a data race needs **shared mutable access across threads**. Rust
already controls sharing and mutation within a single thread via ownership and
borrowing. To extend that guarantee across threads, the compiler needs to know two
facts about every type:

1. Is it sound to **hand this value off** to another thread? (`Send`)
2. Is it sound for two threads to hold a **shared reference** to it at once?
   (`Sync`)

`thread::spawn` then simply *requires* these bounds. If your type can't prove it,
the code doesn't compile. The race becomes impossible to write rather than a bug
you find in production.

```rust
pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
```

The closure is *moved* onto the new thread, so it must be `Send`; everything it
captures must therefore be `Send` too. That single bound is the gate everything
else passes through.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `sum_on_thread` | `spawn` requires `Send`; move owned data in, `join` the result out |
| 2 | foundations | `parallel_contains` | `Sync` = shareable `&T`; many threads read one `&haystack` via `scope` |
| 3 | mechanics | `assert_send`/`assert_sync` | auto-derivation is structural; build compile-time probes |
| 4 | mechanics | `check_4` | predict then verify `Send`/`Sync` across the std library |
| 5 | footgun | `count_racy` vs `count_atomic` | reproduce the non-atomic refcount race that makes `Rc` `!Send` |
| 6 | footgun | the four quadrants | `Cell`/`RefCell` = Send+!Sync; `MutexGuard` = !Send+Sync |
| 7 | real-world | `concurrent_sum` | `Arc<Mutex<T>>` (Send+Sync) vs `Rc<RefCell<T>>` (neither) |
| 8 | real-world | `ThreadBound` / `Buffer` | opt out with `PhantomData`, opt in with `unsafe impl Send` |
| 9 | capstone | `SpinLock<T>` | build a lock; `unsafe impl<T: Send> Sync` and *why only `Send`* |

## The ideas, built up

### 1. `Send` is about moving

The first rung does nothing but move owned data across a boundary:

```rust
fn sum_on_thread(data: Vec<i64>) -> i64 {
    thread::spawn(move || data.iter().sum::<i64>())
        .join()
        .unwrap()
}
```

`Vec<i64>` is `Send` — it owns its heap buffer with no shared aliasing, so handing
the whole thing to another thread transfers *exclusive* access. The `move` keyword
is load-bearing: it makes the closure *own* `data` rather than borrow it. A borrow
of a local can't satisfy `'static`, which previews rung 5's wall.

### 2. `Sync` is about sharing — and it's defined via `Send`

Rung 2 has several threads read the *same* data at once through shared references:

```rust
fn parallel_contains(haystack: &[i64], needles: &[i64]) -> Vec<bool> {
    thread::scope(|s| {
        let mut handles = Vec::with_capacity(needles.len());
        for needle in needles {
            handles.push(s.spawn(move || haystack.contains(needle)));
        }
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}
```

Each closure captures `&haystack` (a `&[i64]`). For that shared reference to cross
into a thread, `[i64]` must be `Sync`. And here is the definition that runs the
whole topic:

> `T: Sync` is *defined* as `&T: Send`.

"It's safe to share `&T` across threads" is literally "it's safe to send a `&T` to
another thread." Sync isn't a separate idea bolted on — it's `Send` applied to
references. `thread::scope` is what lets borrows (not just `'static` data) cross,
because the scope joins every thread before the borrowed data can die.

### 3. The traits are inferred from your fields

There is no `derive`. The compiler walks your type's layout: a struct is `Send` iff
every field is `Send`, `Sync` iff every field is `Sync`. To *observe* a marker bound
you use a generic function whose only content is its bound:

```rust
fn assert_send<T: Send>() {}
fn assert_sync<T: Sync>() {}
```

If `assert_send::<Foo>()` **compiles**, then `Foo: Send`. If it doesn't, you get a
precise error pointing at the offending type. These two empty functions are the
instrument the rest of the ladder runs on.

```rust
struct Telemetry { count: u64, label: String }
// Telemetry is Send + Sync — not by derive, but because u64 and String both are.
assert_send::<Telemetry>();
assert_sync::<Telemetry>();
```

Swap `label` to `Rc<str>` and `assert_send::<Telemetry>()` stops compiling — and the
error names the *struct*, not the field. One rotten apple.

### 4. Probing the standard library

Auto traits prove *positives*: a probe that compiles is proof. There is no stable
*negative* bound, so you witness negatives by uncommenting a probe that *should*
fail and reading the compiler's prose ("`Rc<i32>` cannot be sent between threads
safely"). Predict first, then let the compiler grade you:

| type | `Send` | `Sync` | why |
|------|:------:|:------:|-----|
| `i32`, `String`, `Box<i32>` | yes | yes | owned, no shared aliasing |
| `&i32` | yes | yes | `&T: Send` because `i32: Sync`; `&T: Sync` because `i32: Sync` |
| `Rc<i32>` | **no** | **no** | non-atomic refcount (rung 5) |
| `Arc<i32>` | yes | yes | atomic refcount |
| `Cell<i32>` | yes | **no** | interior mutation, unsynchronized |
| `RefCell<i32>` | yes | **no** | non-atomic borrow flag |
| `Mutex<i32>` | yes | yes | real lock provides synchronization |
| `*const i32` | **no** | **no** | compiler assumes nothing about a raw pointer |

The rows people get wrong are `Cell`/`RefCell` (they *are* `Send`) and the raw
pointer (it is *neither*). Keep reading.

### 5. Why `Rc` is `!Send` — the actual race

`Rc::clone` is, in essence, `self.count += 1` on a plain integer. `Arc::clone` is
`self.count.fetch_add(1, ...)` — a single atomic read-modify-write. If two threads
could clone an `Rc` at once, their non-atomic increments would interleave and lose
updates. A refcount that reads too low frees memory that is still referenced:
use-after-free, then double-free.

You can't share an `Rc` across threads (the compiler forbids it), so the ladder
reproduces the *mechanism* directly on a shared atomic, two ways:

```rust
// non-atomic style: load, then a SEPARATE store — mimics `Rc`'s `count += 1`
let v = c.load(Relaxed);
c.store(v + 1, Relaxed);

// atomic: one indivisible operation — mimics `Arc`'s clone
c.fetch_add(1, Relaxed);
```

Run it with 8 threads × 50,000 iterations and the atomic version is always exactly
400,000, while the racy version loses *hundreds of thousands* of updates:

```text
atomic=400000 (exact, = Arc), racy=53462 lost 346538 updates
```

Translate that to `Rc`: 346,538 clones whose count never registered. That is the
corruption `Rc: !Send` makes impossible to even write.

> The takeaway: `Send`/`Sync` convert a class of runtime data races into compile
> errors. The marker is a proof obligation; the auto-derive discharges it
> structurally.

### 6. The four quadrants

`Send` and `Sync` are independent axes, and every box is occupied:

|            | `Sync` (can share `&T`)        | `!Sync` (cannot share `&T`)     |
|------------|--------------------------------|---------------------------------|
| **`Send`** | `i32`, `String`, `Mutex<T>`, `Arc<T>` | `Cell<T>`, `RefCell<T>` |
| **`!Send`**| `MutexGuard<'_, T>`            | `Rc<T>`, `*const T`, `*mut T`   |

The two that bend intuition:

- **`Cell`/`RefCell`: `Send` + `!Sync`.** Moving the *whole* cell to one thread
  (exclusive ownership, one accessor) is fine. *Sharing* `&Cell` would let two
  threads `.set()` concurrently with zero synchronization — a data race. **Move ≠
  share.**
- **`MutexGuard`: `!Send` + `Sync`.** The canonical Sync-but-not-Send type. Many
  platforms require the locking thread to unlock, so the guard must not be *moved*
  to another thread (`!Send`). But lending `&guard` (which derefs to `&T`) out is
  fine when `T: Sync`. This is also why holding a `std::sync::MutexGuard` across an
  `.await` makes a future `!Send`.

A corollary worth internalizing: `&T: Send ⟺ T: Sync`. So `&Cell<i32>` is *not*
`Send` even though `Cell<i32>` itself *is* `Send` — because `Cell` is `!Sync`.

### 7. The shared-mutable-state workhorse

The famous idiom is just composition of everything above:

```text
Rc<RefCell<T>>   (single-threaded)        Arc<Mutex<T>>   (multi-threaded)
  Rc:      !Send  !Send                      Arc:     Send   Sync   (atomic count)
  RefCell:  Send  !Sync                      Mutex:   Send   Sync   (real lock)
  => NEITHER Send nor Sync                   => Send + Sync
```

Going from one to the other is *literally* swapping non-atomic machinery for
atomic/locked machinery — and the marker traits flip as a consequence. The rung
forces `std::thread::spawn` (not `scope`), so the `'static` bound *requires* `Arc`:

```rust
let accumulator = Arc::new(Mutex::new(0));
for chunk in values.chunks(chunk_len) {
    let accumulator = Arc::clone(&accumulator);   // same Mutex, new handle
    let chunk = chunk.to_vec();                    // own the data ('static)
    handles.push(thread::spawn(move || {
        let partial = chunk.into_iter().sum::<i64>();
        *accumulator.lock().unwrap() += partial;   // lock only to combine
    }));
}
```

Note the discipline: each thread sums its chunk *without* the lock held, then takes
the lock only to add its partial. Holding the lock while iterating would serialize
the threads and defeat the parallelism.

### 8. Overriding the auto-derive — both directions

You can steer the inference instead of just accepting it.

**Opt OUT (safe).** A field whose type isn't `Send`/`Sync` drags the whole type out.
The zero-cost, deliberate way is a `PhantomData<*const ()>` marker — a raw pointer
is `!Send + !Sync`, and `PhantomData<T>` makes your struct behave, for auto-trait
purposes, as if it owned a `T`, storing nothing:

```rust
struct ThreadBound {
    id: u32,
    _pd: PhantomData<*const ()>,   // now !Send and !Sync, at zero runtime cost
}
```

This is how you build a handle that must never leave its thread (an FFI/thread-local
context).

**Opt IN (unsafe).** A type holding a raw pointer is `!Send` by default — the
compiler won't assume anything about it. If *you* know the access is sound, you
promise it:

```rust
struct Buffer { ptr: *mut u8, len: usize }

// SAFETY: Buffer uniquely owns the allocation described by ptr/len.
// Moving it to another thread transfers that ownership; no aliases are exposed,
// and Drop reconstructs and frees the allocation exactly once.
unsafe impl Send for Buffer {}
```

`unsafe` here means "compiler, I take responsibility for this invariant." It is
exactly how `Arc`, `Vec`, `Box`, and channels get their `Send`/`Sync` impls.
`Buffer` deliberately does *not* impl `Sync`: moving it is sound (unique
ownership), but sharing `&Buffer` with an unsynchronized raw read is a different,
unproven claim.

> The `// SAFETY:` comment is not decoration. Stating the invariant *is* the work —
> it's the audit discipline real `unsafe` demands. "Owned by this thread" is the
> *wrong* justification for a `Send` type; the whole point is another thread owns it.

## Footguns

- **A green test can hide a wrong model.** Forcing `unsafe impl Send + Sync` onto a
  type that you *wanted* to be thread-bound makes the code compile while lying to
  the compiler. The probe must match the intent: thread-bound types belong in the
  *commented negative* block, proven by failing to compile.
- **`Cell`/`RefCell` are `Send`.** Easy to assume "interior mutability = not
  thread-safe = neither trait." Wrong: they're `Send` (move is fine), only `!Sync`.
- **`&Cell<T>` is `!Send`** even though `Cell<T>` is `Send`. The reference's
  Send-ness follows the cell's Sync-ness, not its Send-ness.
- **`MutexGuard` across `.await`** makes a future `!Send`, breaking
  `tokio::spawn`. Same root cause as `MutexGuard: !Send`.
- **Reading offset 0 of a possibly-empty buffer is UB.** `Buffer::new(0)` then
  `first()` would read out of bounds; the fix is an explicit `assert!(len > 0)`
  before the `unsafe` read. Run unsafe rungs under `cargo miri` to catch this.

## Real-world patterns

- **`Arc<Mutex<T>>` / `Arc<RwLock<T>>`** — shared mutable state across threads, the
  default reach.
- **`Arc<T>` (no lock)** — shared *immutable* state; needs only `T: Send + Sync`.
- **`PhantomData<*const ()>`** — opt a handle out of `Send`/`Sync` deliberately.
- **`unsafe impl Send/Sync`** — how every concurrency primitive in std bridges from
  raw pointers / `UnsafeCell` back to safe, shareable types.
- **`Send` bound on `spawn` and `tokio::spawn`** — the entire fearless-concurrency
  guarantee enters through this one bound.

## Capstone insight — `SpinLock<T>`

Building a lock from scratch proves you own the model end to end:

```rust
struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for SpinLock<T> {}
unsafe impl<T: Send> Sync for SpinLock<T> {}   // <- the whole ladder, in one line
```

Two pieces matter:

**`UnsafeCell<T>` is the only legal way to get `&mut T` from `&self`.** Every
interior-mutability type — `Cell`, `RefCell`, `Mutex`, the atomics — is built on it.
A plain field behind `&self` can *never* yield `&mut`. `UnsafeCell` is also exactly
what makes a type `!Sync` by default, which is why you must opt back in.

**The bound is `T: Send`, not `T: Sync` — and that is the entire topic.**

> The lock guarantees mutual exclusion: only one thread ever touches the `T` at a
> time. So the value is effectively *handed between* threads (Send), never
> *simultaneously shared* (which would need Sync). Two threads never hold `&T` at
> once, so `T: Sync` is never required.

This is precisely the signature of `std::sync::Mutex<T>: Sync where T: Send`. The
`lock`/`unlock` use `Acquire`/`Release` ordering so that one holder's writes are
visible to the next:

```rust
fn lock(&self) -> SpinGuard<'_, T> {
    while self.locked
        .compare_exchange(false, true, Acquire, Relaxed)
        .is_err()
    {
        std::hint::spin_loop();
    }
    SpinGuard { lock: self }
}

impl<T> Drop for SpinGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Release);   // publish writes to next holder
    }
}
```

Share one `&SpinLock` across eight scoped threads, each locking to increment, and
the total is exact — the lock serializes what rung 5 showed racing. (And it's
Miri-clean.)

## Explain it back

- Define `Send` and `Sync` without using the other word, then state the one-line
  relationship between them.
- Why is `Rc` `!Send` but `Arc` `Send`? Describe the exact race, not just "it's not
  thread-safe."
- `Cell<i32>` is `Send` but `!Sync`. Why is moving it fine but sharing `&` to it
  not?
- Why is `MutexGuard` `Sync` but `!Send`?
- Is `&Cell<i32>` `Send`? Derive the answer from `T: Sync ⟺ &T: Send`.
- In `unsafe impl<T: Send> Sync for SpinLock<T>`, why is the bound `T: Send` and not
  `T: Sync`?
- What does `UnsafeCell` provide that an ordinary field cannot, and why does a type
  containing one need an explicit `unsafe impl Sync`?

## See also

- [Threads & scoped threads](threads.md) — where `Send`/`Sync` bounds first bite.
- [`Rc` / `Arc`](rc-arc.md) — the atomic-vs-non-atomic refcount this ladder dissects.
- [`Cell` / `RefCell`](cell-refcell.md) — the interior-mutability types in the
  Send-but-!Sync quadrant.
- [`Rc<RefCell<T>>` patterns](rc-refcell.md) — the single-threaded counterpart to
  `Arc<Mutex<T>>`.
