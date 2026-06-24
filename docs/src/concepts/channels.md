# Channels

> Ladder: [`src/bin/channels.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/channels.rs) ·
> Run: `cargo run --bin channels` · Phase 4 · 9 rungs

## TL;DR

A channel is a **typed pipe between threads**: a `Sender<T>` end and a `Receiver<T>`
end. Instead of sharing memory behind a lock and coordinating *who touches what when*,
you **move ownership through the pipe** — `send(value)` gives the value away, `recv()`
takes it on the other side. Two facts carry the whole topic:

1. **Ownership moves through the pipe.** `send(v)` transfers `v` out of the sending
   thread. No aliasing, no lock at the call site — the type system already proved only
   one thread owns it.
2. **The channel closes itself.** When every `Sender` drops, the `Receiver` observes
   "disconnected" and stops. That is how loops terminate cleanly — no sentinel value,
   no message count.

Almost every channel bug is a violation of fact 2: a `Sender` you forgot to drop, so
the receiver waits forever.

## Why this exists (from first principles)

Shared-state concurrency (`Arc<Mutex<T>>`) answers "how do many threads safely touch
one piece of data?" Channels answer a different question: "how do threads *hand work
to each other*?" The distinction matters because shared state forces every participant
to agree on a locking protocol, and protocols are where deadlocks live.

A channel removes the protocol. The buffer is owned by the channel, not by any thread.
A producer's only verb is `send`; a consumer's only verb is `recv`. There is no "lock
this, then that" ordering to get wrong, because there is only ever one operation per
side. The classic slogan:

> Do not communicate by sharing memory; instead, share memory by communicating.

Rust enforces the safety of this at the type level. `Sender<T>` and `Receiver<T>` are
`Send` only when `T: Send` — you can ship the ends to other threads precisely because
the values that flow through are themselves safe to move between threads. The "move
ownership through the pipe" model isn't a convention; it's what the borrow checker
already guarantees.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | Foundations | First pipe | `mpsc::channel()`, `move` the sender into a thread, `recv()` |
| 2 | Foundations | Multi-producer | `tx.clone()` — the "m" in mpsc; fan in from N threads |
| 3 | Mechanics | Receiver as iterator | `for v in rx` ends on disconnect; you must `drop(tx)` |
| 4 | Mechanics | Bounded & backpressure | `sync_channel(k)`; `send` blocks when full; `(0)` = rendezvous |
| 5 | Footgun | The hang | stray `Sender` ⇒ `recv` blocks forever; `RecvError` vs `SendError(v)` |
| 6 | Footgun | Non-blocking | `try_recv`: split `Empty` (keep polling) from `Disconnected` (stop) |
| 7 | Real-world | Worker pool | `Arc<Mutex<Receiver>>` shared job queue + a results channel |
| 8 | Real-world | crossbeam | `Receiver: Clone` mpmc, and `select!` over multiple channels |
| 9 | Capstone | Build it | hand-rolled `Channel<T>` from `Mutex` + `Condvar` + `VecDeque` |

## The ideas, built up

### 1. The pipe and the move

```rust
let (tx, rx) = mpsc::channel();
thread::spawn(move || {
    for i in 1..=5 {
        tx.send(i).unwrap();
    }
});
let mut result = Vec::new();
for _ in 0..5 {
    result.push(rx.recv().unwrap());
}
```

Two things to notice. First, `move` on the closure is mandatory: without it the closure
would only *borrow* `tx`, and that borrow would have to outlive the local `tx` in the
parent — a lifetime error. `move` hands ownership of `tx` into the thread.

Second, the signatures tell the whole story:

```text
tx.send(value) -> Result<(), SendError<T>>   // moves value in; Err if receiver gone
rx.recv()      -> Result<T, RecvError>       // blocks until a value arrives or all senders drop
```

`send` *consumes* `value`. After `tx.send(i)`, the sending thread no longer owns `i`.
`recv` *blocks* — it parks the thread until something is in the buffer. A single sender
also preserves order, which is why recv'ing five times yields `1,2,3,4,5`.

### 2. Many senders, one receiver (the "m" in mpsc)

`mpsc` = **multi-producer, single-consumer.** You get extra producers by **cloning** the
`Sender`. Every clone feeds the same `Receiver`.

```rust
let (tx, rx) = mpsc::channel();
for i in 0..n {
    let tx = tx.clone();                 // each thread gets its OWN handle
    thread::spawn(move || {
        tx.send(i * 10).unwrap();
    });
}
```

The clone must happen **inside** the loop. If you tried to `move` the single `tx` into
the closure, it would be consumed on the first iteration and gone on the second. Cloning
first gives each thread a private handle while the original `tx` stays in the parent.

Order across threads is now nondeterministic (the OS schedules them however it likes),
so the rung sums the results rather than asserting a sequence. The single receiver is the
serialization point: whatever interleaving the senders produce, the consumer sees a
well-defined stream of values one at a time.

### 3. Disconnect is the shutdown signal

The receiver is an **iterator**. `for v in rx` yields values until the channel is
*disconnected* — meaning every `Sender` (original and all clones) has dropped:

```rust
for i in 0..n {
    let tx = tx.clone();
    thread::spawn(move || { tx.send(i as i64).unwrap(); });
}
drop(tx);              // <-- the linchpin

let mut result = 0;
for _ in rx {          // ends by itself once every sender is gone
    result += 1;
}
```

The `drop(tx)` is the entire lesson. After the loop, `n` clones live in threads (each
drops when its thread finishes), but the **original** `tx` is still held by `main`. The
iterator only ends when the sender count reaches zero. Leave the original alive and
`for v in rx` waits forever for a value that will never come.

> **Rule of thumb:** the number of live senders is a reference count. The receiver's
> loop terminates exactly when that count hits zero. Every `Sender` you hold is a promise
> "more might come" — drop the promise when it's no longer true.

### 4. Bounded channels and backpressure

`mpsc::channel()` is **unbounded**: `send` never blocks, it just appends to the queue. A
fast producer feeding a slow consumer grows that queue without limit — a memory leak in
slow motion.

`mpsc::sync_channel(k)` is **bounded** to `k` buffered messages. When the buffer is full,
`send` *blocks* until the consumer frees a slot. That blocking **is** backpressure: the
producer is forced down to the consumer's pace.

```rust
let (tx, rx) = mpsc::sync_channel(0);   // capacity 0 = rendezvous
thread::spawn(move || {
    tx.send("a").unwrap();   // blocks until a recv() is ready to take it
    tx.send("b").unwrap();
    tx.send("c").unwrap();
});
```

`sync_channel(0)` is the extreme case: a **rendezvous** channel with zero buffer. Every
`send` blocks until a `recv` is simultaneously ready — the value is handed across
thread-to-thread with no storage in between. `send("b")` literally cannot return until
someone has recv'd `"a"`.

> **Testing note from the ladder:** the rung's assertion passes even with an unbounded
> `channel()`, because it only inspects the *consumer-side* order. To actually *witness*
> backpressure you have to record the **producer's** progress (push to a shared
> `Arc<Mutex<Vec<_>>>` right after each `send` returns) and observe that with capacity 0
> the producer can never get more than one value ahead of the consumer. A green test does
> not always prove the property you care about.

### 5. The edges of a channel's life

When one half is gone, two **symmetric** errors report it:

```text
rx.recv()  -> Err(RecvError)        // buffer empty AND every Sender dropped — nothing more can arrive
tx.send(v) -> Err(SendError(v))     // the Receiver dropped — nobody will take v, so it's handed BACK
```

`RecvError` is what ends `for v in rx`. You can also handle it explicitly:

```rust
let mut result = Vec::new();
while let Ok(value) = rx.recv() {    // exits on Err(RecvError)
    result.push(value);
}
```

`SendError` is the mirror image, and it carries the value with it. Since nobody can ever
receive `v`, `send` gives it back so you can do something else with it:

```rust
let (tx, rx) = mpsc::channel();
drop(rx);
let recovered = tx.send(99).unwrap_err().0;   // SendError is a tuple struct; .0 is the value
assert_eq!(recovered, 99);
```

The footgun lives in the gap between these two errors: **if a `Sender` never drops,
`recv` on an empty channel blocks forever.** No `RecvError` is ever produced because, as
far as the channel knows, more values might still come. The infinite hang and the clean
`RecvError` are the same mechanism viewed from two sides — sender count zero vs not.

### 6. Receiving without blocking

`recv()` blocks, which is wrong for an event loop that must also do other work, or a
consumer with a deadline. `try_recv()` never blocks and returns a richer error:

```rust
loop {
    match rx.try_recv() {
        Ok(value) => result.push(value),
        Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(100)), // keep polling
        Err(TryRecvError::Disconnected) => break,                              // truly done
    }
}
```

The two `TryRecvError` variants are the heart of the rung and must be handled
*separately*:

| Variant | Meaning | Correct response |
|---------|---------|------------------|
| `Empty` | nothing right now, but senders are alive | back off and try again |
| `Disconnected` | empty **and** all senders dropped | stop |

Collapse them and you get a bug either way: treat `Empty` as "done" and you quit early,
losing every later message; treat `Disconnected` as "try again" and you busy-spin
forever. A correct non-blocking drain *must* branch on both. (`recv_timeout(dur)` is the
middle ground: block up to a deadline, then return `Timeout`.)

### 7. The worker pool — channels as architecture

A fixed pool of `N` workers draining a shared job queue, with results flowing back over a
second channel. Two channels, two directions:

- **jobs:** `main --(many)--> workers` (fan-out)
- **results:** `workers --(many)--> main` (fan-in)

The wall you hit: **`Receiver` is not `Clone`** (mpsc = single consumer). N workers can't
each own the receiving end. The classic std thread-pool fix is to wrap it:

```rust
let (job_tx, job_rx) = mpsc::channel();
let (res_tx, res_rx) = mpsc::channel();
let job_rx = Arc::new(Mutex::new(job_rx));     // share one receiver behind a lock

for _ in 0..n_workers {
    let job_rx = Arc::clone(&job_rx);
    let res_tx = res_tx.clone();
    thread::spawn(move || {
        loop {
            let job = {                         // lock held ONLY across recv
                let job_rx = job_rx.lock().unwrap();
                job_rx.recv()
            };
            match job {
                Ok(x) => res_tx.send(x * x).unwrap(),
                Err(_) => break,                // job senders all dropped -> exit
            }
        }
    });
}

for input in inputs { job_tx.send(input).unwrap(); }
drop(job_tx);                                   // so workers see disconnect and exit
drop(res_tx);                                   // so the result drain terminates

let mut results: Vec<i64> = res_rx.into_iter().collect();
results.sort();
```

Two subtleties decide whether this is correct *and* fast:

- **Lock scope.** The inner `{ ... }` block releases the mutex *before* computing `x * x`.
  Hold the lock across the work and your N workers degrade to running one-at-a-time — the
  single most common mistake in hand-rolled pools.
- **Two independent drops.** `drop(job_tx)` lets workers see disconnect and stop;
  `drop(res_tx)` lets the result drain see disconnect and finish. These are two separate
  disconnect chains — rung 3's and rung 5's lessons resurfacing. Keep either original
  alive and you hang.

This is what `threadpool` and the work-distribution core of `rayon` look like underneath
(plus a vector of `JoinHandle`s to `join` on shutdown).

### 8. crossbeam — what std channels structurally can't do

`std::sync::mpsc` is single-consumer by design. `crossbeam-channel` lifts two limits.

**True MPMC: the `Receiver` is `Clone`.** Multiple consumers, no `Arc<Mutex>` wrapper.
The same worker pool collapses to:

```rust
use crossbeam_channel::{select, unbounded};

let (job_tx, job_rx) = unbounded();
for _ in 0..n_workers {
    let job_rx = job_rx.clone();        // clone the RECEIVER itself
    let res_tx = res_tx.clone();
    thread::spawn(move || {
        for job in job_rx {             // shared iterator; ends on disconnect
            res_tx.send(job * job).unwrap();
        }
    });
}
```

No mutex, no manual `lock()`/`recv()` dance, no inner block to scope the guard. The
workers *share* the iterator because the receiver is `Clone + Sync`.

**`select!`: wait on several channels at once.** std has no way to block on two receivers
simultaneously; crossbeam's `select!` blocks until *any* arm is ready, then runs the
first one that fires:

```rust
while open_a && open_b {
    select! {
        recv(rx_a) -> msg => match msg {
            Ok(value) => out.push(value),
            Err(_) => open_a = false,
        },
        recv(rx_b) -> msg => match msg {
            Ok(value) => out.push(value),
            Err(_) => open_b = false,
        },
    }
}
if open_a { out.extend(rx_a); }   // one closed -> drain the survivor to exhaustion
if open_b { out.extend(rx_b); }
```

The subtle correctness point: once a channel disconnects, `recv` on it returns `Err`
*immediately and forever*, so `select!` would keep picking the dead channel and busy-spin.
The fix here is to loop only `while open_a && open_b`, then the instant *either* closes,
fall out and `out.extend(rx_other)` — which consumes the surviving receiver as an iterator
until *its* senders drop. Guaranteed to terminate, no spin. This is exactly how you'd
merge a data stream against a shutdown signal in real code.

## Footguns

| Trap | What bites | The fix |
|------|-----------|---------|
| Stray `Sender` | `for v in rx` / `recv()` blocks forever — no `RecvError` ever fires | `drop(tx)` the original after spawning producers |
| `move` a single `tx` into a loop | consumed on iteration 1, won't compile on iteration 2 | `let tx = tx.clone()` *inside* the loop |
| Unbounded channel, slow consumer | queue grows without limit (memory blowup) | `sync_channel(k)` for backpressure |
| Collapsing `try_recv` errors | quit early on `Empty`, or spin forever on `Disconnected` | branch on both variants explicitly |
| Holding the job lock while computing | N workers serialize into one | scope the guard to just the `recv`, release before work |
| Keeping the original `res_tx` alive | result drain never sees disconnect → hang | `drop(res_tx)` before draining |
| `select!` on a disconnected channel | dead arm fires instantly, busy-spins | stop selecting it; drain the survivor with a plain `for` |

## Real-world patterns

- **Fan-out / fan-in worker pools** are the bread and butter: one job channel out, one
  result channel back. `threadpool`, and the task-distribution layer of `rayon`, are this
  pattern industrialized.
- **`Arc<Mutex<Receiver>>`** is the idiomatic way to give std's single-consumer receiver
  to many workers when you don't want a crossbeam dependency.
- **`crossbeam-channel`** is the go-to when you need real MPMC or `select!`. It's also
  faster than std's `mpsc` and is what many production systems reach for.
- **`select!` for shutdown** — merge a work channel with a "stop" channel so a worker can
  be told to quit between jobs. The same shape as `merge_two`.
- **Async mirrors this exactly:** `tokio::sync::mpsc` is the same model with `.await`
  instead of blocking, and `tokio::select!` is the async sibling of crossbeam's `select!`.
  Learn the threaded version and the async one is a renaming.

## Capstone insight

Rung 9 rebuilds a blocking mpsc channel from three safe primitives, and the payoff is
seeing that the "magic" of `recv` is just a condition variable:

```rust
struct Shared<T> { items: VecDeque<T>, senders: usize }   // buffer + live-sender count
struct Inner<T>  { queue: Mutex<Shared<T>>, available: Condvar }
```

The receiver doesn't busy-wait; it **sleeps on the `Condvar`** and is woken by whoever
changes the state it cares about:

```rust
fn recv(&self) -> Result<T, Disconnected> {
    let mut shared = self.inner.queue.lock().unwrap();
    loop {
        if let Some(item) = shared.items.pop_front() {   // 1. value ready -> take it
            return Ok(item);
        }
        if shared.senders == 0 {                          // 2. drained AND no senders -> done
            return Err(Disconnected);
        }
        shared = self.inner.available.wait(shared).unwrap();  // 3. sleep; wait() unlocks+parks
    }
}
```

Three details make this correct, and each maps onto a behavior you used as a black box:

- **Check `pop_front` *before* `senders == 0`.** If the last sender drops while items
  remain, the receiver must drain them first, and only then report `Disconnected`. Reverse
  the two checks and you silently lose buffered messages on shutdown — this is precisely
  the `RecvError` semantics from rung 5: *empty AND disconnected*, in that order.
- **`wait` in a `loop`, not an `if`.** `Condvar::wait` can return spuriously (woken with no
  real change). Re-checking the predicate in a loop absorbs that. The loop body *is* the
  "while the thing I want isn't true, keep sleeping" pattern.
- **Every state change a receiver waits on is followed by a notify.** `send` does
  `push_back` then `notify_one`. The last `Sender::drop` does `senders -= 1` then
  `notify_all` — that final notify is the *entire* disconnect mechanism: it wakes a parked
  receiver so it can re-check, see zero senders, and return `Err` instead of sleeping
  forever.

```rust
fn send(&self, value: T) {
    { let mut shared = self.inner.queue.lock().unwrap(); shared.items.push_back(value); }
    self.inner.available.notify_one();   // notify AFTER releasing the lock
}

impl<T> Drop for MySender<T> {
    fn drop(&mut self) {
        if self.update_senders(-1) == 0 {       // sender count is a manual refcount
            self.inner.available.notify_all();  // wake the receiver to see disconnect
        }
    }
}
```

Notifying *after* unlocking is the polite habit: the woken receiver won't immediately
re-block on a mutex you're still holding. And the `senders` field is a hand-rolled
reference count — `Clone` increments it, `Drop` decrements it, and the receiver's
termination condition is "count reached zero." That is the same bookkeeping std's real
`mpsc` does, minus the lock-free fast paths. Once you've written this, `for v in rx`
ending on disconnect is no longer magic; it's a `usize` reaching `0` and a `notify_all`.

## Explain it back

Future-you should be able to answer these cold:

1. Why does `for v in rx` sometimes hang forever, and what one line fixes it?
2. Why must you `tx.clone()` *inside* the spawn loop instead of moving one `tx` in?
3. What does `SendError(v)` carry that `RecvError` doesn't, and why?
4. In a non-blocking drain, what goes wrong if you treat `TryRecvError::Empty` as "done"?
   As "the channel is broken, stop"?
5. Why must the worker-pool lock be released *before* the worker does its computation?
6. Why does the std worker pool need `Arc<Mutex<Receiver>>` but the crossbeam version
   doesn't?
7. In `merge_two`, why would a naive `select!` busy-spin once one channel closes?
8. In the capstone `recv`, why is the order of the two checks (`pop_front` then
   `senders == 0`) load-bearing? Why must `wait` sit inside a `loop`?
9. What is the single invariant that, if violated anywhere, makes the hand-rolled receiver
   sleep forever?

## See also

- [`Mutex` / `RwLock`](mutex-rwlock.md) — the lock and `Condvar` the capstone is built on.
- [Threads & scoped threads](threads.md) — `spawn`, `move`, `join`; what channels connect.
- [`Send` & `Sync` deeply](send-sync.md) — why the channel ends are `Send` iff `T: Send`.
- [`Rc` / `Arc`](rc-arc.md) — the `Arc` that lets both channel ends share one `Inner`.
