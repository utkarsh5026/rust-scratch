# Mutex / RwLock

> Ladder: [`src/bin/mutex_rwlock.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/mutex_rwlock.rs) ·
> Run: `cargo run --bin mutex_rwlock` · Phase 4 · 9 rungs

## TL;DR

A `Mutex<T>` protects **data, not code**. The only way to reach the `T` is to
`lock()`, which hands you a **`MutexGuard`** — an RAII token that *is* `&mut T`
and **unlocks when it drops**. The borrow checker then enforces, at compile time,
that you can only touch the data while you hold the lock. `RwLock<T>` splits that
into **many readers XOR one writer**.

Everything hard about locks is one of three things:

- **Guard lifetime** — the lock is held for exactly as long as the guard is alive.
  Hold it too long and you serialize the program; hold it across a second lock in
  the wrong order and you deadlock.
- **Poisoning** — a panic while locked taints the lock so later `lock()` calls
  return `Err`, warning you the data may be half-updated.
- **Lock ordering** — two locks taken in opposite orders by two threads form a
  cycle and hang forever (ABBA deadlock). The fix is a global acquisition order.

## Why this exists (from first principles)

Shared mutable state across threads is the original sin of concurrency. If two
threads run `counter += 1` at the same time, the operation is really *read,
add, write* — and the two reads can both see the old value, so one increment is
lost. This is a **data race**, and in most languages it is silent corruption or
undefined behavior.

Rust makes data races a **compile error**. The rule: you may have many `&T`
(shared, read-only) **or** one `&mut T` (exclusive), never both. But a counter
shared by 8 threads needs all of them to write. How do you get `&mut` from a
shared `&`?

A `Mutex<T>` is the answer: it provides **interior mutability** guarded at
runtime. You only ever hold a shared `&Mutex<T>`, but `lock()` returns a guard
that derefs to `&mut T`. The mutex guarantees that at most one guard exists at a
time, so the `&mut` it hands out is genuinely exclusive — the borrow rule holds,
just enforced by a runtime lock instead of the compiler.

> The mental shift: a `Mutex` doesn't make a region of *code* atomic. It makes
> access to a piece of *data* exclusive. The "critical section" is exactly the
> span where the guard is alive.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|-----------|
| 1 | foundations | Mutex basics | `lock().unwrap()` → guard → `*guard += by`; `let mut guard` for `DerefMut` |
| 2 | foundations | `Arc<Mutex>` counter | share one mutex across N threads; hold the lock across read-modify-write |
| 3 | mechanics | Guard lifetime | snapshot + `drop(guard)` to shrink the critical section |
| 4 | mechanics | RwLock | many readers XOR one writer; `read()` is `&T`, `write()` is `&mut T` |
| 5 | footgun | Poisoning | panic-while-locked poisons; recover via `into_inner()` |
| 6 | footgun | Non-reentrancy | std `Mutex` isn't recursive; double-lock self-deadlocks |
| 7 | footgun | Lock-ordering ABBA | induce a deadlock, fix with a canonical lock order |
| 8 | real-world | Mutex + Condvar | bounded blocking queue; `wait()` in a `while` loop |
| 9 | capstone | Concurrent Bank | deadlock-free transfers + poison recovery under a thread storm |

## The ideas, built up

### 1. The guard is the lock (rungs 1–2)

```rust
fn bump(m: &Mutex<i32>, by: i32) {
    let mut guard = m.lock().unwrap();   // guard: MutexGuard<i32>
    *guard += by;                        // DerefMut → &mut i32
}                                        // guard drops here → unlock
```

Three things to notice:

- **`&Mutex`, not `&mut Mutex`.** The mutex hands out mutability through a shared
  reference. That is what lets an `Arc<Mutex<T>>` (which only ever gives you `&`)
  still mutate.
- **`let mut guard`.** The mutex turned a shared `&` into mutable access, but the
  "mut-ness" has to reappear *somewhere* — it reappears on the guard binding,
  because `*guard += by` goes through `DerefMut`, which needs `mut`.
- **Unlock is `Drop`.** There is no `unlock()` method. The lock is released when
  the guard goes out of scope. This is the single most important fact about
  locks in Rust, and rung 3 is entirely about controlling *when* that happens.

To share across threads, wrap in `Arc` and clone one handle per thread:

```rust
let counter = Arc::new(Mutex::new(0));
for _ in 0..n_threads {
    let c = Arc::clone(&counter);          // each thread gets its own handle
    s.spawn(move || {
        for _ in 0..per_thread { bump(&c, 1); }
    });
}
```

> Why both `Arc` *and* `Mutex`? They are orthogonal. `Arc` answers **"who owns
> it?"** (shared ownership, so the data lives as long as any thread needs it).
> `Mutex` answers **"who can touch it right now?"** (exclusive access). You need
> both: `Arc` to share the handle, `Mutex` to coordinate the mutation.

The assertion `8 * 1000 == 8000` is the data-race detector. The lock must be held
across the *whole* read-modify-write. If you ever did `read, unlock, +1, lock,
write`, two threads could read the same value and one update would be lost — and
the total would land below 8000.

> Note on `Arc` vs `scope`: rung 2 uses `thread::scope`, which *also* lets threads
> borrow locals (the scope guarantees they join first), so the `Arc` is technically
> redundant there. With plain `thread::spawn` (which requires `'static`), the `Arc`
> is load-bearing — each thread genuinely needs its own owning handle.

### 2. Guard lifetime = critical section length (rung 3)

Because the guard holds the lock until it drops, holding it across slow work
serializes every other thread behind you. The fix is to **shrink the critical
section**: grab what you need, release, then do the slow part unlocked.

```rust
fn slow_sum(data: &Mutex<Vec<i32>>, expensive: impl Fn(i32) -> i32) -> i32 {
    let guard = data.lock().unwrap();
    let snapshot = guard.clone();   // copy the data out
    drop(guard);                    // release the lock BEFORE the slow work
    snapshot.iter().map(|x| expensive(*x)).sum()
}
```

The lock is held for microseconds (one clone) instead of for the entire
`expensive` pass. The ladder enforces this: the `expensive` closure tries to
`try_lock()` the same mutex and panics if it can't — so holding the guard across
the loop fails the test.

Two tools to release early, equivalent in effect:

```rust
drop(guard);            // explicit
{ let g = m.lock()...; /* use g */ }   // inner scope: g drops at the brace
```

> Real-world echo: this is why production code clones out of the lock, or computes
> a new value and *then* takes a brief lock to store it, rather than holding a
> mutex across I/O or heavy CPU.

### 3. RwLock — split the lock when reads dominate (rung 4)

A `Mutex` gives exclusive access **even to readers** — two threads that only want
to read still serialize. When reads vastly outnumber writes (config, caches,
routing tables), that is wasted parallelism. `RwLock<T>` splits the lock:

| Method | Guard | Access | Concurrency |
|--------|-------|--------|-------------|
| `read()`  | `RwLockReadGuard`  | `&T`     | **many** at once |
| `write()` | `RwLockWriteGuard` | `&mut T` | **one**, blocks all readers |

```rust
fn reader_sum(rw: &RwLock<Vec<i32>>) -> i32 {
    let guard = rw.read().unwrap();   // &Vec<i32> — shared
    guard.iter().sum()
}
fn writer_push(rw: &RwLock<Vec<i32>>, v: i32) {
    let mut guard = rw.write().unwrap();  // &mut Vec<i32> — exclusive
    guard.push(v);
}
```

The asymmetry mirrors the borrow rules exactly: `read()` needs only `let guard`
(it's `&T`), `write()` needs `let mut guard` (it's `&mut T`). The rung proves the
sharing is real — 4 reader threads all hold read guards simultaneously, and the
max-overlap counter reaches 4. With a `Mutex` it would never exceed 1.

> Caveat — writer starvation: std's `RwLock` gives **no fairness guarantee**.
> On some platforms a steady stream of readers can starve a waiting writer. That
> is why "read-*heavy*" is the rule of thumb; under write pressure a plain `Mutex`
> can actually be faster and fairer.

### 4. Poisoning — the lock as a tripwire (rung 5)

Now the question rung 1 deferred: **why does `lock()` return a `Result`?**

If a thread panics while holding the guard, the data might be half-updated — an
invariant could be broken mid-mutation. Rust records this: the mutex becomes
**poisoned**, and every later `lock()` returns `Err(PoisonError)`.

```rust
// A thread dies mid-mutation:
let mut g = m.lock().unwrap();
*g = 999;
panic!("boom");   // guard's Drop runs during unwind → mutex is now poisoned
```

After that, a plain `.lock().unwrap()` would itself panic. To keep going, recover
the guard *out of the error*:

```rust
fn recover(m: &Mutex<i32>) -> i32 {
    let guard = m.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard
}
```

`PoisonError::into_inner()` hands you the guard anyway. Poisoning is **advisory**,
not a wall: it says "the invariant *might* be broken," and you decide whether the
data is still usable. In rung 5 the `999` was fully written before the panic, so
the data is fine and recovery is correct.

> Poisoning is contested. `parking_lot::Mutex` and `tokio::sync::Mutex` **don't
> poison at all** — they decided the ergonomic tax wasn't worth it. So
> `.unwrap()` on a std lock is really an assertion that *no holder ever panics*;
> code that must survive panics handles the `PoisonError`. (Recent Rust also adds
> `Mutex::clear_poison()`.)

### 5. Non-reentrancy — the single-thread deadlock (rung 6)

Unlike some languages, std's `Mutex` is **not recursive**. If a thread holds the
guard and calls `lock()` on the same mutex again, it blocks forever waiting for
itself.

```rust
fn would_self_deadlock(m: &Mutex<i32>) -> bool {
    let _guard = m.lock().unwrap();   // hold it
    m.try_lock().is_err()             // second attempt → Err(WouldBlock)
}
```

This isn't an oversight — it's *required* for soundness. A reentrant lock would
hand you a **second `&mut`** to data you already hold a `&mut` to, which is
aliasing UB. Non-reentrancy is what keeps the guard's `&mut` exclusive.

The rung proves the deadlock without hanging by using `try_lock()`, which returns
immediately (`Err(WouldBlock)`) instead of blocking. The lesson: a real `lock()`
on line 2 would freeze the program **invisibly** — no panic, no error, just a
hung thread. `try_lock` turns an unobservable hang into a returnable `Err`, which
is also the real tool when you genuinely might re-enter: detect and back off
instead of wedging.

### 6. Lock ordering — the ABBA deadlock (rung 7)

The classic multi-lock deadlock. Two accounts, each behind its own mutex:

```
Thread 1 (A→B): lock A, then lock B
Thread 2 (B→A): lock B, then lock A

    T1 holds A, waiting for B   ┐
    T2 holds B, waiting for A   ┘  → cycle → neither proceeds → hang
```

This is **ABBA**: a cycle in the "who-waits-for-whom" graph. The fix is a global
lock **order**. If every thread always acquires locks in the same order, no cycle
can form. Here, order by ascending account `id`:

```rust
fn transfer_ordered(from: &Account, to: &Account, amt: i64) {
    if from.id < to.id {
        let mut fg = from.balance.lock().unwrap();   // lower id first
        let mut tg = to.balance.lock().unwrap();
        *fg -= amt; *tg += amt;
    } else {
        let mut tg = to.balance.lock().unwrap();     // lower id first
        let mut fg = from.balance.lock().unwrap();
        *fg -= amt; *tg += amt;                       // still from→to
    }
}
```

The trick is to keep two concerns independent:

- **Acquisition order** is always lower-id-first (deadlock avoidance).
- **Mutation** still subtracts from `from`, adds to `to` (correctness).

You bind the guards to the right *roles* in each branch, but lock in the canonical
*order*. The harness runs 100k transfers each way at once — the exact ABBA setup —
and a 5-second watchdog catches a wrong ordering instead of hanging your terminal.

> When there's no natural `id`, order by the mutex's memory address
> (`std::ptr::from_ref(m) as usize`) or any stable total order. The *content* of
> the order doesn't matter — only that every thread agrees on it.
>
> Deeper point: deadlock-freedom is a property of the **whole system**, not one
> function. `transfer_ordered` is safe only because *every* caller obeys the same
> order. One rogue lock-in-argument-order site reintroduces the cycle.

### 7. Condvar — waiting for a condition (rung 8)

A `Mutex` lets you *read* shared state safely, but it can't make you *wait* for
that state to become a certain way. Busy-looping `while q.lock().is_empty() {}`
burns a core. A **`Condvar`** (condition variable) is a parking lot tied to a
mutex: a thread can sleep until another thread notifies it.

The one method that matters:

```rust
guard = self.cv.wait(guard).unwrap();
```

It **atomically** (a) unlocks the mutex and parks the thread, then (b) on wakeup
re-locks the mutex and returns the guard. The atomic unlock-and-sleep is the
whole point: it closes the race where you check the condition, then sleep, and
miss a notify that lands in between.

```rust
fn push(&self, v: T) {
    let mut guard = self.inner.lock().unwrap();
    while guard.len() == self.cap {              // WHILE, not if
        guard = self.cv.wait(guard).unwrap();
    }
    guard.push_back(v);
    self.cv.notify_all();                        // notify AFTER mutating
}
fn pop(&self) -> T {
    let mut guard = self.inner.lock().unwrap();
    while guard.is_empty() {
        guard = self.cv.wait(guard).unwrap();
    }
    let item = guard.pop_front().unwrap();
    self.cv.notify_all();
    item
}
```

Two rules that *define* correct Condvar use:

1. **Wait in a `while`, never an `if`.** After `wait()` returns you only know you
   were *woken*, not that the condition holds. Spurious wakeups happen, and with
   one shared condvar a `notify_all` wakes every parked thread — including other
   poppers. If two poppers wake on one item, the `while` makes the loser re-check
   `is_empty()` and go back to sleep instead of calling `pop_front().unwrap()` on
   an empty deque and panicking. The loop is what makes a shared condvar safe.
2. **Notify after you mutate**, so a parked thread wakes to re-test its predicate.

> `notify_one` vs `notify_all`: `notify_one` is cheaper but only safe when any
> single waiter can make progress on the event. With mixed waiter *kinds* on one
> condvar (pushers + poppers), `notify_one` can wake the wrong kind and stall;
> `notify_all` is the safe default. The throughput fix is **two** condvars
> (`not_full`, `not_empty`) so you only wake the relevant side — which is exactly
> how `std::sync::mpsc` and most bounded channels are built.

## Footguns

| Trap | What bites | Fix |
|------|-----------|-----|
| Holding the guard too long | every other thread serializes behind you | snapshot + `drop(guard)` before slow work (rung 3) |
| `.lock().unwrap()` everywhere | a panicked holder poisons the lock → all later locks panic | recover via `unwrap_or_else(\|e\| e.into_inner())` (rung 5) |
| Re-locking the same mutex in one thread | self-deadlock, hangs *silently* | don't; std `Mutex` isn't reentrant. Use `try_lock` to detect (rung 6) |
| Two locks in opposite orders | ABBA deadlock under concurrency | canonical lock order (lower id / address first) (rung 7) |
| `if cond { cv.wait() }` | spurious wakeup or a raced predicate → act on a false condition | always `while cond { cv.wait() }` (rung 8) |
| Forgetting `notify` after mutating | waiters sleep forever | `notify_all()` after every state change |
| Same-account transfer | `from.lock(); to.lock();` is a double-lock = rung 6 | reject `from == to` up front (rung 9) |

## Real-world patterns

- **`Arc<Mutex<T>>`** is the canonical "shared mutable state across threads"
  handle. `Arc` for shared ownership, `Mutex` for exclusive access — orthogonal,
  both needed.
- **Fine-grained locking.** One `Mutex` per item (`Vec<Mutex<i64>>` in the
  capstone) lets disjoint operations run in parallel, unlike one coarse
  `Mutex<Vec<_>>` that serializes everything.
- **`RwLock` for read-heavy state** — config snapshots, caches, routing tables —
  with the writer-starvation caveat in mind.
- **`Mutex + Condvar`** is the primitive under channels, thread pools, and
  producer/consumer pipelines. `std::sync::mpsc` is essentially this.
- **`parking_lot`** offers faster, smaller, non-poisoning `Mutex`/`RwLock` and is
  a common drop-in in production crates.

## Capstone insight (rung 9)

The `Bank` fuses the entire ladder into one stress test: 8 threads × 50,000
random transfers against a bank with a deliberately **poisoned** account, an
8-second deadlock watchdog, and one invariant — **money is conserved**.

```rust
fn lock_recover(m: &Mutex<i64>) -> MutexGuard<'_, i64> {
    match m.lock() { Ok(g) => g, Err(e) => e.into_inner() }   // rung 5
}

fn transfer(&self, from: usize, to: usize, amt: i64) -> Result<(), TransferError> {
    if from == to { return Err(TransferError::SameAccount); }          // rung 6
    if from >= self.accounts.len() || to >= self.accounts.len() {
        return Err(TransferError::NoSuchAccount);
    }
    if from < to {                                                     // rung 7
        let mut fg = Self::lock_recover(&self.accounts[from]);  // lower index first
        let mut tg = Self::lock_recover(&self.accounts[to]);
        if *fg < amt { return Err(InsufficientFunds { have: *fg, need: amt }); }
        *fg -= amt; *tg += amt; Ok(())                                 // rung 3
    } else {
        let mut tg = Self::lock_recover(&self.accounts[to]);
        let mut fg = Self::lock_recover(&self.accounts[from]);
        if *fg < amt { return Err(InsufficientFunds { have: *fg, need: amt }); }
        *fg -= amt; *tg += amt; Ok(())
    }
}
```

The structural "aha": **each safety property is an independent line of defense,
and they compose.** Three distinct failure modes, three distinct guarantees:

| If you... | You get... | The defense |
|-----------|-----------|-------------|
| `.unwrap()` the poisoned account | a worker *panics* | `lock_recover` (poison recovery) |
| lock in inconsistent order | the bank *hangs* (watchdog fires) | lower-index-first (lock ordering) |
| check funds after mutating, or overflow | money *created/destroyed* | check-before-mutate, hold both guards |

Money is conserved (8000 → 8000) only because all three hold at once. The proof
is in the numbers: ~340k transfers applied, ~58k denied for insufficient funds,
the poisoned account survived, total unchanged. Correctness under concurrency is
not one clever trick — it's several disciplines layered, each closing one hole.

## Explain it back

- Why does a `Mutex` let you mutate through a shared `&`, and where does the
  "mut" reappear?
- There is no `unlock()`. When exactly is the lock released, and how do you
  release it *early*?
- Why does `lock()` return a `Result`? What does `into_inner()` recover, and why
  is poisoning "advisory"?
- Why is std's `Mutex` non-reentrant, and why is that *required* for soundness
  rather than a limitation?
- Draw the ABBA cycle. What single rule breaks it, and why must *every* caller
  obey it?
- Why must `cv.wait()` live in a `while` loop and never an `if`? Give two distinct
  reasons.
- In the capstone, name the three independent defenses and the failure each one
  prevents.

## See also

- [Threads & scoped threads](threads.md) — `spawn`, `join`, `thread::scope`, the
  `'static` wall the `Arc` clone works around.
- [`Send` & `Sync` deeply](send-sync.md) — *why* `Arc<Mutex<T>>` is `Send + Sync`
  and `Rc<RefCell<T>>` is neither; the hand-rolled `SpinLock`.
- [`Cell` / `RefCell`](cell-refcell.md) — interior mutability in a single thread;
  `RefCell`'s runtime borrow check is the non-atomic cousin of a `Mutex`.
