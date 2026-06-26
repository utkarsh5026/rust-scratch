# Shared state vs message passing

> Ladder: [`src/bin/concurrency_models.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/concurrency_models.rs) ·
> Run: `cargo run --bin concurrency_models` · Phase 4 · 9 rungs

## TL;DR

Two threads need to agree on some data. There are exactly two ways to arrange that:

- **Shared state** — one piece of memory, many pointers into it, access serialized
  by a lock (`Arc<Mutex<T>>` / `Arc<RwLock<T>>`) or atomics. *"Communicate by sharing memory."*
- **Message passing** — the data has **one owner at a time** and is handed off down a
  channel. No lock, because there is nothing shared. *"Share memory by communicating."*

Neither is "better." The skill is reading a workload and picking — and the senior move
is **combining them**: an *actor* thread privately owns the state (message-passing
mechanics) while presenting a single logical store to everyone (shared-state semantics).

The deepest one-liner from the whole ladder:

> A **lock** serializes access at the **critical section**. An **actor** serializes
> access at the **queue**. Both give you mutual exclusion — they just put the
> "one-at-a-time" gate in a different place.

## Why this exists (from first principles)

A data race is two threads touching the same memory with at least one writing, and no
synchronization between them. Rust makes data races a *compile error* via `Send`/`Sync`
and the borrow rules — but it doesn't pick your architecture. You still have to choose
*how* threads coordinate, and that choice has two fundamentally different answers.

The reason both exist is that they fail in opposite ways:

- Shared state is cheap to read and write (a lock is just a flag), but every thread
  contends for the same lock, so a fat critical section silently serializes your whole
  program (rung 5).
- Message passing has no lock to contend, and ownership transfer means there's nothing
  to race over — but channels add per-message overhead, can deadlock under backpressure,
  and let you *accidentally* re-share state if you send the wrong type (rung 6).

Understanding both, and when each bites, is the entire topic.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|-----------|
| 1 | foundations | Two roads, one counter | The same sum via `Arc<Mutex>` and via an mpsc aggregator |
| 2 | foundations | Ownership transfer | Owned jobs through a channel — the `Mutex` guards the *queue*, not the *data* |
| 3 | mechanics | Pipeline of stages | `produce → ×3 → keep-even → collect`, each its own thread; EOF cascades |
| 4 | mechanics | Fan-out / fan-in both ways | Shared `VecDeque` queue vs mpsc job queue, side by side |
| 5 | footgun | Lock held too long | Slow work inside the critical section serializes N threads to 1 |
| 6 | footgun | Message-passing footguns | Stray sender hang, clean disconnect, bounded backpressure, the `Arc`-through-channel trap |
| 7 | real-world | The actor | One thread owns a `HashMap`; others send commands + a one-shot reply |
| 8 | real-world | Hybrid | Writes through the actor, lock-light reads from a published snapshot |
| 9 | capstone | One trait, two impls | `KvStore` backed by `Arc<RwLock<HashMap>>` vs an actor; same tests pass both |

## The ideas, built up

### 1. The same problem, two shapes

Summing `1..=N` across 8 threads. The threads do identical work; the only difference is
how their partial sums become one total.

**Shared state** — every worker locks one accumulator and adds into it:

```rust
fn sum_shared(n: u64) -> u64 {
    let total = Arc::new(Mutex::new(0));
    for i in 0..THREADS {
        let total = Arc::clone(&total);
        thread::spawn(move || {
            let (lo, hi) = chunk_bounds(n, i);
            let mut total = total.lock().unwrap();          // lock ONCE per thread
            *total += (lo + hi) * (hi - lo + 1) / 2;
        });
    }
    // join all ...
    *total.lock().unwrap()
}
```

The discipline already shows: lock **once per thread**, not once per number. The lock is
a coordination point, so you touch it as rarely as possible — 8 acquisitions, not a
million.

**Message passing** — each worker *sends* its partial; main is the sole owner of the total:

```rust
fn sum_message(n: u64) -> u64 {
    let (tx, rx) = mpsc::channel();
    for i in 0..THREADS {
        let tx = tx.clone();
        thread::spawn(move || {
            let (lo, hi) = chunk_bounds(n, i);
            tx.send((lo + hi) * (hi - lo + 1) / 2).unwrap();
        });
    }
    drop(tx);                                   // <-- critical: see below
    let mut total = 0;
    while let Ok(partial) = rx.recv() { total += partial; }
    total
}
```

There is **no `Mutex` anywhere**, yet this is perfectly correct under concurrency. The
channel *is* the synchronization, and the total only ever lives in one thread.

> The `drop(tx)` is not optional. `rx.recv()` returns `Err` only when *all* senders are
> gone. The original `tx` lives in `sum_message`'s scope; if you don't drop it, the loop
> waits forever for a value from a sender that will never send. (Rung 6 makes this its
> own lesson.)

### 2. Ownership transfer = no lock on the data

Send owned `Vec<u8>` payloads to a worker pool. Each payload is owned by exactly one
worker while processed, then its result is handed back.

```rust
let job_rx = Arc::new(Mutex::new(job_rx));      // mpsc Receiver is single-consumer
// worker:
let job = {
    let job_rx = job_rx.lock().unwrap();
    job_rx.recv()
};                                              // <-- guard dropped HERE
match job {
    Ok(payload) => {
        let sum = payload.into_iter().map(u64::from).sum();   // processed OFF the lock
        res_tx.send(sum).unwrap();
    }
    Err(_) => break,
}
```

Two things make this work:

- The **only** lock guards the *queue* (the shared `Receiver`), never the *payloads*.
  Each `Vec<u8>` is moved into one worker and owned by it for its entire life.
- The lock guard is bound in a **tight inner scope** so it drops *before* processing.

> **Footgun preview.** Had you written
> `while let Ok(job) = job_rx.lock().unwrap().recv()`, the temporary guard would live
> until the end of the loop body — so you'd process *while holding the receiver lock*,
> serializing your whole pool back to one worker. Bind the guard, then drop it.

### 3. Pipelines: ownership flows down the pipe

Message passing shines for pipelines — each stage is its own thread, linked only by
channels, and an item flows stage → stage owned by one stage at a time:

```
[produce] --tx--> rx --[×3] --tx1--> rx1 --[keep even] --tx2--> rx2 --[collect]
```

```rust
thread::spawn(move || { for x in input { tx.send(x).unwrap(); } });
thread::spawn(move || { for x in rx  { tx1.send(x * 3).unwrap(); } });
thread::spawn(move || { for x in rx1 { if x % 2 == 0 { tx2.send(x).unwrap(); } } });
let out: Vec<_> = rx2.into_iter().collect();
```

Why this is elegant:

- **The EOF cascade.** When `input` is exhausted, stage 1's closure ends → `tx` drops →
  stage 2's `for x in rx` ends → `tx1` drops → ... → main's loop ends. One "done" signal
  at the source propagates the whole way down with zero extra code. This is the same
  shutdown mechanism the actor uses (rung 7).
- **All stages run concurrently** — a true assembly line, throughput gated by the slowest
  stage.
- **Order is preserved for free**, because each channel has exactly one feeder thread.

The price of "for free" here: the channels are unbounded, so a fast producer can outrun a
slow consumer and grow memory. Bounded channels (rung 6c) add backpressure.

### 4. Fan-out / fan-in, both ways — the comparison

The same worker pool built twice, differing *only* in how a worker gets its next task.

**Shared-state queue** — workers drain a pre-loaded `Arc<Mutex<VecDeque>>`:

```rust
let task = { let mut q = queue.lock().unwrap(); q.pop_front() };
match task { None => break, Some(t) => partial += work(t) }
```

**Message-passing queue** — workers `recv()` from a shared channel until disconnect.

Both give the same total. The difference you should *feel*:

| | Shared `VecDeque` | mpsc channel |
|---|---|---|
| "no more work" | **you** decide: `pop_front()` returns `None` | `recv()` returns `Err` on disconnect |
| empty queue, work still arriving | busy-spins on the lock (CPU burn) | parks the thread (no CPU) |
| backpressure | build it yourself | bounded channel gives it for free |

The decision rule this rung hands you:

> **Pre-loaded, bounded work → a shared queue is fine. Open-ended / streaming arrival →
> a channel,** because blocking and end-of-stream come for free instead of being hand-rolled.

### 5. The shared-state tax: lock held too long

This is the footgun that *defines* shared state. The same sum, computed two ways, where
`expensive(task)` sleeps ~2 ms (standing in for parsing / hashing / I/O):

```rust
// WRONG: expensive() runs while the accumulator lock is held
let mut total = total.lock().unwrap();
*total += expensive(task);

// OK: expensive() runs off-lock; lock only for the +=
let v = expensive(task);
*total.lock().unwrap() += v;
```

Measured with 40 tasks, one thread per task:

```
held = 87 ms        shrunk = 3 ms
```

The punchline is brutal: in the bad version the **number of threads didn't matter at all**.
Forty threads delivered the throughput of *one*, because each held the lock for its whole
2 ms sleep and they queued single-file. Runtime is `N × (time under lock)` regardless of
core count.

> The entire discipline of shared-state concurrency in one sentence: **do the work
> outside the lock; touch the shared state for as short as possible.** Snapshot-then-release,
> compute-then-commit.

### 6. The message-passing footguns

Channels have their own sharp edges. The ladder makes each *observable* (timeouts and
`try_*` so "it would hang" becomes a testable `Err`):

```rust
// (a) A live sender — even an unused clone — keeps recv blocking forever
let (tx, rx) = mpsc::channel();
let _tx_clone = tx.clone();
rx.recv_timeout(Duration::from_millis(50))   // Err(Timeout)

// (b) Drop EVERY sender -> the clean shutdown signal
drop(tx); drop(tx_clone);
rx.recv()                                     // Err(RecvError)

// (c) Bounded buffer full -> backpressure made visible
let (tx, _rx) = mpsc::sync_channel::<i32>(2);
tx.send(1).unwrap(); tx.send(2).unwrap();
tx.try_send(99)                               // Err(Full(99))
```

The most insidious one is **(d), the aliasing trap**:

```rust
let arc = Arc::new(Mutex::new(0));
let arc_clone = arc.clone();
tx.send(arc_clone).unwrap();        // "sent" — but main still holds `arc`
// worker: *arc.lock().unwrap() += 10
*arc.lock().unwrap()                 // == 10  <- main SEES the worker's write
```

Sending an `Arc<Mutex<T>>` through a channel does **not** transfer ownership of the data.
Moving an `Arc` moves a *pointer*; both `Arc`s still alias the same `Mutex`. You've
re-introduced shared state behind a message-passing facade — and every shared-state hazard
with it.

> The transfer-vs-share distinction lives in the **type you send**, not in the fact that
> you used a channel. Send a `Vec<u8>` → genuine handoff. Send an `Arc<Mutex<_>>` → you're
> back in lock-land. This is how teams convince themselves they've "gone lock-free with
> channels" while quietly shipping shared state down those channels.

### 7. The actor: combining both models

The senior pattern. One thread **privately owns** a `HashMap` — no `Mutex`, no `Arc`
around the map. Everyone else holds a cheap clonable handle (just a `Sender`) and sends
**command messages**; reads carry a one-shot reply channel.

```rust
enum Command {
    Get { key: String, reply: mpsc::Sender<Option<String>> },
    Set { key: String, value: String },
}

impl KvActor {
    fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut map = HashMap::new();          // plain local — owned by one thread
            for cmd in rx {
                match cmd {
                    Command::Get { key, reply } => { reply.send(map.get(&key).cloned()).unwrap(); }
                    Command::Set { key, value }  => { map.insert(key, value); }
                }
            }
        });
        KvActor { tx }
    }

    fn get(&self, key: &str) -> Option<String> {
        let (reply_tx, reply_rx) = mpsc::channel();         // fresh one-shot
        self.tx.send(Command::Get { key: key.into(), reply: reply_tx }).unwrap();
        reply_rx.recv().unwrap()                            // blocks until the answer
    }
}
```

Why this is the combining pattern:

- **No lock on the map**, because exactly one thread ever touches it. The borrow checker
  never even has to consider it shared — it isn't.
- **Concurrent correctness is automatic.** Ten threads can clone the handle and fire
  commands; they can't corrupt the map because they never touch it. The actor applies
  commands one-at-a-time off its queue.
- **`get` is a synchronous round-trip over two messages** — send request with a reply
  channel, block on the reply. Reads *feel* like function calls.
- **Shutdown is free.** When the last handle drops, its `tx` drops, the `for cmd in rx`
  loop ends, the thread exits — the same EOF cascade as the pipeline.

This is shared-state **semantics** (one logical store everyone uses) delivered through
message-passing **mechanics** (zero locks on the data).

### 8. Hybrid: writes through the actor, reads from a snapshot

The plain actor has one weakness: **reads queue behind writes** (a `Get` is a command on
the same channel). For read-heavy workloads, decouple them.

The actor stays the single writer, but after each write it **publishes an immutable
snapshot** into a shared `Arc<RwLock<Arc<HashMap>>>`. Readers bypass the actor entirely:

```rust
// writer (inside the actor loop):
map.insert(key, value);
*snapshot.write().unwrap() = Arc::new(map.clone());   // publish: atomic pointer swap
ack.send(()).unwrap();                                // ack so set() is read-your-writes

// reader:
let snap = Arc::clone(&self.snapshot.read().unwrap());  // bump refcount under a brief read-lock
snap.get(key).cloned()                                  // lookup with NO lock held
```

Two subtleties make this correct *and* fast:

- The `RwLockReadGuard` is a temporary that drops at the end of the `let` statement, so
  the lookup runs lock-free on a private `Arc` clone. The read-lock is held for exactly
  one refcount bump.
- Publishing is an **atomic pointer swap** of the whole map, so a reader holds *either*
  the old map or the new one — never a torn, half-applied state.

The tradeoff: publishing **clones the whole map per write**. That's the price of
lock-light, never-changes-under-you reads. In production you'd reach for `arc-swap` (a
single atomic pointer swap, no `RwLock`) and/or persistent maps (`im`) so the clone is
structural-sharing-cheap.

### 9. Capstone: one trait, two architectures

The proof that the model is an *implementation detail* behind a stable interface:

```rust
trait KvStore: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str);
    fn delete(&self, key: &str);
    fn len(&self) -> usize;
}
```

- **`SharedStore`** = `Arc<RwLock<HashMap>>`; every op takes the appropriate lock.
- **`ActorStore`** = a worker thread owns the map; ops are `Get`/`Set`/`Delete`/`Len`
  commands, reads/`len` carry a reply channel, writes carry an ack so the caller gets
  read-your-writes.

The same `exercise()` and an 8-thread `hammer()` storm run against **both** via
`Arc<dyn KvStore>` and pass identically. A lock-based store and a lock-free actor store
are observably indistinguishable to callers.

> One enabler worth noting: `mpsc::Sender<T>` is `Sync` in current std, so `ActorStore`
> (which holds a `Sender`) is `Send + Sync` and fits behind `Arc<dyn KvStore>` exactly
> like the lock-based store.

## Footguns

| Trap | What happens | Fix |
|------|--------------|-----|
| Forgot `drop(tx)` | `rx.recv()` blocks forever — a live sender means "more might come" | drop every extra sender before draining |
| Stray sender clone | same hang, but harder to spot (an unused clone counts) | give each sender a clear owner/scope; `drop` deliberately |
| Lock across slow work | N threads serialize to 1; thread count stops mattering | compute off-lock, lock only to commit |
| Guard held too long | `lock().recv()` in a `while let` holds the guard across the body | bind the guard in a tight scope so it drops first |
| Bounded channel full, no consumer | blocking `send` parks forever = deadlock | size the buffer; use `try_send`; ensure a draining consumer |
| `Arc<Mutex>` through a channel | "message passing" that secretly shares state | send *owned* data for real handoff; if you must share, own it |

## Real-world patterns

- **mpsc worker pool** — `Arc<Mutex<Receiver>>` shared by N workers, lock only to dequeue
  (rungs 2, 4). This is the bones of a thread pool.
- **Pipelines** — stage-per-thread linked by channels (rung 3); the threaded form of Unix
  pipes / streaming ETL.
- **The actor** — `tokio`'s recommended pattern for shared mutable state in async code is
  exactly rung 7 (a task owning the state + an mpsc command channel). The `Command` enum
  *is* a protocol; swap the transport (channel → socket) and callers don't change.
- **Read-optimized snapshots** — `arc-swap` and copy-on-write config publishing are rung 8
  in production form.
- **Strategy behind a trait** — rung 9 is how you keep a concurrency choice swappable; the
  same shape lets you A/B a lock-based and actor-based backend.

## Capstone insight

The whole ladder collapses to a single realization:

> Mutual exclusion has to live *somewhere*. A **lock** puts the gate at the **critical
> section** — threads queue to touch the data. An **actor** puts the gate at the
> **message queue** — threads queue to send a command, and one owner touches the data.

Once you see that, "shared state vs message passing" stops being a religious debate and
becomes an engineering trade-off: where do you want the queue, how expensive is each
crossing, and what does the data's ownership story actually look like.

## Explain it back

- Why does `sum_message` need no `Mutex`, and why is `drop(tx)` mandatory?
- In the worker pool, what exactly does the one `Mutex` protect — and what does it *not*?
- Why did 40 threads run at the speed of one in the "lock held too long" rung?
- A coworker says "we use channels so we're lock-free." They send `Arc<Mutex<State>>`
  down those channels. What's wrong?
- Why does the actor's `HashMap` need no lock? Where did the serialization go?
- In the hybrid store, why is a reader guaranteed never to see a half-applied write?
- State the "where is the gate?" insight in one sentence.

## See also

- [Channels](channels.md) — the mpsc / `sync_channel` / crossbeam mechanics this builds on
- [`Mutex` / `RwLock`](mutex-rwlock.md) — guards, poisoning, deadlock, lock ordering
- [`Send` & `Sync` deeply](send-sync.md) — why `Arc<Mutex>` crosses threads and `Rc` can't
- [Threads & scoped threads](threads.md) — `spawn`, `join`, the `'static` wall
- [Data parallelism with `rayon`](rayon-parallel.md) — when the answer is "neither, just `par_iter`"
