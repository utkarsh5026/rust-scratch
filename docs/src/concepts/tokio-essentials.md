# tokio essentials

> Ladder: [`src/bin/tokio_essentials.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/tokio_essentials.rs) ·
> Run: `cargo run --bin tokio_essentials` · Phase 5 · 9 rungs

## TL;DR

A `Future` is **inert** — it computes nothing until something *polls* it. tokio is
that something: an **executor** that owns a small pool of worker threads and polls
thousands of futures to completion on them. Everything in this ladder is about *how
you place futures onto tasks and compose them*:

- `.await` — poll one future to completion **on the current task**.
- `tokio::spawn` — hand a future to the runtime as a **new, independent task** (its
  own poll-loop, possibly on another thread); returns a `JoinHandle`.
- `join!` — drive several futures concurrently **on one task**, get a tuple back.
- `select!` — **race** futures; first ready wins, the losers are dropped (cancelled).
- `spawn_blocking` — run synchronous blocking code on a **separate** pool so it
  can't freeze the async workers.

And one golden rule that everything else orbits: **never block a worker thread.** A
handful of threads cooperatively poll everything; one blocking call stalls all of it.

## Why this exists (from first principles)

Threads are the OS's unit of concurrency, but they're expensive: each has a real
stack, and the kernel schedules them preemptively. If you want 50,000 concurrent
network connections, 50,000 OS threads is a non-starter.

Async flips the model. A `Future` is a **state machine** that runs a little, then
returns `Poll::Pending` when it would block (waiting on a socket, a timer, a lock)
and hands control back. The executor parks it and runs something else on the same
thread. When the thing it was waiting for is ready, the future is *woken* and polled
again. Concurrency becomes cheap — a future is just a struct, not a thread.

But that cheapness comes with a contract: **cooperative scheduling.** A future only
yields the thread at an `.await` that returns `Pending`. There is no preemption. So
if one future runs synchronous, blocking, await-free code, the executor cannot take
the thread back — every other task sharing that thread is frozen. That single fact
generates half of this ladder (rungs 6-7) and the capstone footgun.

tokio provides the executor, the timers, the channels, and the escape hatch
(`spawn_blocking`) for when you *must* block. The ladder walks the surface you use
every day.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|-----------|
| 1 | foundations | `#[tokio::main]` + `.await` a sleep | a future does nothing until polled; `.await` yields, doesn't block |
| 2 | foundations | `spawn` → `JoinHandle`; N tasks | spawn-all-then-await = real concurrency (~max, not sum) |
| 3 | mechanics | `join!` | concurrency on **one** task, tuple out, no spawn/`Send`/`unwrap` |
| 4 | mechanics | `select!` | race; the losing futures are **dropped mid-flight** = cancellation |
| 5 | footgun | `'static + Send` wall; `JoinError` | why we clone `Arc`s into `async move`; task panics become `Result` |
| 6 | footgun | blocking the executor | a blocking hog starves a 20 ms timer to 200 ms+ |
| 7 | real-world | `spawn_blocking` + `mpsc` | move blocking work off-worker; producer/consumer with backpressure |
| 8 | real-world | `interval`/`timeout` + `select!` loop | the shape of every long-running component; `biased;` shutdown |
| 9 | capstone | concurrent worker pool | shared queue + workers + `spawn_blocking` + results channel |

## The ideas, built up

### 1. A future is inert; `.await` polls it

```rust
async fn greet_after(name: &str, ms: u64) -> String {
    tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
    format!("hello, {name}")
}
```

`tokio::time::sleep(..)` returns a `Sleep` future. On its own it does *nothing* — no
timer is armed, no time passes. The `.await` is what matters: it polls the sleep, and
because the deadline hasn't arrived it returns `Pending`, so `.await` yields the task
back to the runtime. The runtime parks it and runs other work; when the timer fires,
the task is woken and polled again, this time returning `Ready(())`.

> **The classic first bug:** forgetting `.await`. `sleep(d);` with no `.await`
> constructs the future and immediately drops it — no sleep happens — and the
> compiler only *warns* (`unused must_use`). Async does nothing without a poller.

`#[tokio::main]` is the bootstrap: it rewrites `async fn main` into a synchronous
`main` that builds a runtime and calls `runtime.block_on(async { .. })`.

### 2. `spawn` makes an independent task — and the timing proves concurrency

```rust
let mut handles = Vec::with_capacity(n);
for i in 0..n {
    handles.push(tokio::spawn(async move {         // spawn FIRST, all of them
        tokio::time::sleep(Duration::from_millis(ms)).await;
        i * 10
    }));
}
let mut sum = 0;
for handle in handles {
    sum += handle.await.unwrap();                  // await SECOND
}
```

`tokio::spawn(future)` hands the future to the runtime as its own task and returns a
`JoinHandle<T>` **immediately** — it doesn't wait. Awaiting the handle later waits for
that task and yields its return value.

The whole lesson is in the ordering. Ten 50 ms tasks finish in **~50 ms**, not
500 ms, because they all run concurrently. But only if you **spawn them all into a
`Vec` first, then await the handles**. The trap:

```rust
// WRONG — serializes: each iteration waits for its own task before spawning the next
for i in 0..n {
    let v = tokio::spawn(async move { sleep(ms).await; i * 10 }).await.unwrap();
    sum += v;
}
// OK — spawn all, then drain the handles (above)
```

Spawn kicks off work in the background; awaiting a handle blocks on *that* task. Mixing
them per-iteration throws the concurrency away.

### 3. `join!` — concurrency without spawning

```rust
async fn fetch_all() -> (u32, u32, u32) {
    tokio::join!(fetch_num(1, 60), fetch_num(2, 60), fetch_num(3, 60))
}
```

`join!` drives all its futures concurrently **on the current task** and returns a
tuple of their outputs once *all* complete. Three 60 ms fetches overlap and finish in
~60 ms.

How it differs from spawn — and why you'd pick it:

| | `join!` | `spawn` |
|---|---------|---------|
| Runs on | the **current** task, one thread | possibly other worker threads |
| Parallelism | concurrent, **not** parallel (interleaved at awaits) | can be truly parallel |
| Bounds | none extra | future must be `Send + 'static` |
| Returns | a tuple, directly | a `JoinHandle<Result<T, JoinError>>` |
| Cost | no allocation, no task | allocates/schedules a task |

Reach for `join!` when you have a fixed handful of awaits to run together right here.
Reach for `spawn` when you want independent, long-lived, or truly-parallel tasks.

> Concurrency ≠ parallelism. `join!`'s three futures share one thread; they *overlap*
> only because each yields at its `.await`. If they did CPU work with no awaits, they'd
> run one after another.

### 4. `select!` — race, and the losers are cancelled

`join!` waits for **all**. `select!` waits for the **first** to finish, runs that
arm's handler, and **drops the other futures where they were suspended.** That drop is
cancellation — the loser never runs to completion.

The ladder proves the drop is real with a `DropSpy`:

```rust
struct DropSpy(Arc<AtomicBool>);
impl Drop for DropSpy {
    fn drop(&mut self) { self.0.store(true, Ordering::SeqCst); }  // fires on cancel
}

async fn slow_task(ms, counter, cancelled) -> &'static str {
    let _spy = DropSpy(cancelled);                 // alive inside the future
    tokio::time::sleep(Duration::from_millis(ms)).await;
    counter.fetch_add(1, Ordering::SeqCst);        // ONLY if we finish
    "slow won"
}

tokio::select! {
    _ = tokio::time::sleep(Duration::from_millis(fast_ms)) => "fast won",
    who = slow_task(slow_ms, counter, cancelled)          => who,
}
```

Race a 30 ms sleep against a 200 ms `slow_task`. The fast arm wins; the slow future is
dropped mid-sleep — so `counter` stays **0** (its increment is never reached) and the
`DropSpy` flips `cancelled` to **true**. The suspended future's local `_spy` gets
dropped exactly like a value going out of scope, because that's what it is.

> **This is the deepest idea in async Rust:** dropping a future *is* cancelling it.
> No cancellation token needed for the basic case — stop polling it, drop it, done.
> The flip side is a hazard: if a future is cancelled at an await point, work in
> progress after that point simply never happens ("cancellation safety").

### 5. The `'static + Send` wall, and catching panics

`tokio::spawn` requires the future be **`Send + 'static`**. Why:

- **`'static`** — the runtime may keep the task alive for an unknown duration, so it
  can't borrow anything from the current stack frame (that frame may be long gone).
- **`Send`** — the runtime may move the task to another worker thread, so everything
  it holds across an await must be safe to send between threads.

The two errors this produces *define* idiomatic async Rust:

```rust
let local = String::from("on my stack");
tokio::spawn(async { println!("{local}"); });     // E0373: borrows `local`, may outlive fn

let rc = std::rc::Rc::new(1);
tokio::spawn(async move {
    sleep(Duration::from_millis(1)).await;
    println!("{rc}");                              // Rc is !Send: "cannot be sent between threads"
});
```

The fix for the first is `async move` (take ownership). The fix for `!Send` shared
state is `Arc` (and `Arc<Mutex<_>>` for shared mutability) — which is exactly why you
see `let x = arc.clone();` right before every `tokio::spawn(async move { .. })`.

**Panics don't crash the process.** A panicking task is caught; the panic surfaces when
you await its handle:

```rust
let handle = tokio::spawn(async move { if should_panic { panic!("boom") } 42 });
match handle.await {                       // JoinHandle::await -> Result<T, JoinError>
    Ok(v) => Ok(v),
    Err(e) if e.is_panic() => Err("task panicked".into()),
    Err(_)                 => Err("task cancelled".into()),
}
```

That's *why* `JoinHandle` yields a `Result` — task failure is isolated, not fatal. You
still see the panic message logged to stderr; that's the runtime reporting the caught
panic, not a crash.

### 6. Blocking the executor starves everyone (the golden rule)

The single most important operational fact about async. On a runtime with N worker
threads, a task that does synchronous blocking work with **no await points** owns its
worker thread the whole time — every other task on that thread is frozen.

The ladder measures it on a **single-threaded** runtime. A 20 ms timer task is spawned,
then a 200 ms `hog` runs:

```rust
async fn hog(block: bool, dur_ms: u64) {
    if block {
        std::thread::sleep(Duration::from_millis(dur_ms));   // NO await — hijacks the worker
    } else {
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(dur_ms) {
            tokio::task::yield_now().await;                  // cooperate: let others run
        }
    }
}
```

Result: the blocking hog makes the 20 ms timer fire **~220 ms** late — it couldn't be
polled until the hog released the thread. The cooperative version (same duration, but
yielding) lets the timer fire at **~20 ms**. A 10× difference, from one `.await`.

> `yield_now().await` returns `Pending` once, scheduling an immediate re-poll — it's
> the manual "give others a turn" primitive. It patches a CPU loop, but the real fix
> for genuinely blocking calls is rung 7.

### 7. `spawn_blocking` + `mpsc`: the real fix and the real plumbing

You don't rewrite blocking code — you **move it off the async workers** onto tokio's
dedicated blocking pool:

```rust
let sq = tokio::task::spawn_blocking(move || heavy_square(n)).await.unwrap();
```

`spawn_blocking` runs the closure on a separate, larger pool meant for blocking work,
returns a `JoinHandle`, and — crucially — lets the async workers keep polling
everything else. Use it for sync file/DB drivers, CPU-heavy compute, any `std`
blocking call.

The other half of rung 7 is **`tokio::sync::mpsc`**, the async channel that wires tasks
together:

```rust
let (tx, mut rx) = tokio::sync::mpsc::channel::<u64>(8);   // bounded, capacity 8

// producer task
for n in inputs {
    let sq = tokio::task::spawn_blocking(move || slow_square(n)).await.unwrap();
    tx.send(sq).await.unwrap();      // .await: blocks when the buffer is full = BACKPRESSURE
}
// tx dropped at end of task -> consumer sees EOF

// consumer
while let Some(v) = rx.recv().await { out.push(v); }   // None once ALL senders dropped
```

Two properties to internalize:

- **Backpressure for free.** `send().await` suspends when the buffer is full, so a fast
  producer can't run away from a slow consumer.
- **EOF = last sender dropped.** `recv()` returns `None` only when *every* `Sender`
  clone is gone. Keep one alive by accident and the consumer hangs forever — see the
  capstone.

### 8. The `select!` event loop: the shape of a real service

Almost every long-running tokio component is a loop that `select!`s over several event
sources each turn — "do periodic work", "handle a message", "notice shutdown":

```rust
async fn worker(mut shutdown: mpsc::Receiver<()>) -> u32 {
    let mut ticks = 0;
    let mut interval = tokio::time::interval(Duration::from_millis(20));
    loop {
        tokio::select! {
            biased;                                  // check arms top-to-bottom, not at random
            _ = shutdown.recv() => break,            // Some(()) OR None (senders gone) => stop
            _ = interval.tick() => ticks += 1,
        }
    }
    ticks
}
```

New tools:

- **`interval(period)`** — a ticker; `interval.tick().await` resolves every `period`
  (the **first** tick fires immediately).
- **`timeout(dur, fut)`** — wraps a future; `Ok(v)` if it finished in time, `Err(Elapsed)`
  if not. The test uses it to guard the join so a broken shutdown can't hang forever.
- **`biased;`** — without it, `select!` polls ready arms in *random* order for fairness.
  With it, arms are checked in source order. Putting `biased;` + the shutdown arm on top
  guarantees shutdown is noticed promptly instead of maybe-next-iteration.

Note the shutdown arm handles `Some(())` *and* `None` with the same `_ =>` — an explicit
signal and a dropped supervisor both mean "stop", for free.

### 9. Capstone — a concurrent worker pool

Everything at once: a fixed pool of workers drains a shared job queue, does the heavy
compute on the blocking pool, and streams results back.

```rust
async fn run_pool(num_workers: usize, jobs: Vec<Job>) -> Vec<(u64, u64)> {
    let (jobs_tx, jobs_rx)       = tokio::sync::mpsc::channel::<Job>(8);
    let (results_tx, mut res_rx) = tokio::sync::mpsc::channel::<(u64, u64)>(8);
    let jobs_rx = Arc::new(tokio::sync::Mutex::new(jobs_rx));   // share a 1-consumer channel

    for _ in 0..num_workers {
        let jobs_rx = jobs_rx.clone();
        let results_tx = results_tx.clone();
        tokio::spawn(async move {
            loop {
                let job = {                                    // <-- lock scope
                    let mut rx = jobs_rx.lock().await;
                    rx.recv().await
                };                                             // guard dropped BEFORE compute
                let Some(job) = job else { break };
                let sq = tokio::task::spawn_blocking(move || heavy_square(job.n))
                    .await.unwrap();
                results_tx.send((job.id, sq)).await.unwrap();
            }
        });
    }

    for job in jobs { jobs_tx.send(job).await.unwrap(); }
    drop(jobs_tx);       // workers hit EOF once the queue drains
    drop(results_tx);    // main's own clone — or res_rx never sees EOF

    let mut out = Vec::new();
    while let Some(r) = res_rx.recv().await { out.push(r); }
    out.sort_by_key(|(id, _)| *id);
    out
}
```

The structural insight: **tokio's `mpsc` is multi-producer, single-consumer.** To let N
workers pull from one queue, you wrap the single `Receiver` in
`Arc<tokio::sync::Mutex<_>>` and hand it around. This is *the* idiom for a shared job
queue on top of `mpsc` (higher-level pools use an mpmc channel like `async-channel`,
but this is the primitive underneath).

Four workers over eight 20 ms jobs finish in **~40 ms** (two rounds), each job squared
exactly once. Two footguns must be avoided to get there — both covered below.

## Footguns

| Trap | What bites | Fix |
|------|-----------|-----|
| Forgetting `.await` | future built and dropped; nothing runs; only a warning | `.await` it |
| spawn-and-await per iteration | tasks serialize; concurrency lost | spawn all into a `Vec`, await after |
| Borrowing a local in `spawn` | `E0373` — future may outlive the frame | `async move` |
| `!Send` across `.await` (`Rc`, `MutexGuard`) | "future cannot be sent between threads" | `Arc` / `Arc<Mutex>`; drop guards before awaiting |
| Blocking call in an async task | starves every task on that worker thread | `spawn_blocking`, or `yield_now` for CPU loops |
| Keeping a `Sender` clone alive | consumer's `recv()` returns `None` never — **hang** | drop every sender you don't need, including your own |
| Holding a `Mutex` guard across the compute | workers serialize to one at a time | scope the lock to just `recv()`, drop the guard first |

The last two are the capstone's deadlock and serialization bugs. Both hinge on the same
subtlety about *lifetimes of temporaries and guards*:

```rust
// WRONG — the guard is a temporary in the `while let` scrutinee, alive for the WHOLE body,
// so the lock is held across spawn_blocking + send: workers run one at a time (~160 ms).
while let Some(job) = jobs_rx.lock().await.recv().await { /* compute + send */ }

// OK — bind recv() in an inner block so the guard drops before the compute (~40 ms).
let job = { let mut rx = jobs_rx.lock().await; rx.recv().await };
let Some(job) = job else { break };
```

And the deadlock:

```rust
// WRONG — main's results_tx is still alive during the collect loop, which therefore
// waits for a sender that only drops AFTER the loop. recv() never returns None -> hang.
while let Some(r) = res_rx.recv().await { out.push(r); }
drop(results_tx);

// OK — drop main's clone BEFORE collecting, so EOF can actually arrive.
drop(results_tx);
while let Some(r) = res_rx.recv().await { out.push(r); }
```

## Real-world patterns

- **`Arc::clone` before `spawn`** — the visible signature of moving shared state into a
  task without violating `Send + 'static`.
- **`spawn_blocking` for the sync world** — wrapping a blocking DB driver, `std::fs`,
  `serde` over a big buffer, or password hashing so it can't stall the reactor.
- **The `select!` loop** — the skeleton of servers, actors, and background workers:
  `biased;` shutdown on top, timers/messages below.
- **Bounded `mpsc` for backpressure** — pipelines where a fast stage shouldn't outrun a
  slow one; the buffer size is your flow-control knob.
- **`Arc<Mutex<Receiver>>` job queue** — the hand-rolled worker pool, the thing crates
  like `deadpool` and custom executors formalize.

## Capstone insight

A worker pool is nothing but the ladder's primitives arranged correctly:
**`spawn` gives you the workers, an `Arc<Mutex<mpsc::Receiver>>` gives them a shared
queue, `spawn_blocking` keeps the heavy compute off the reactor, a results `mpsc`
streams answers home, and dropping the senders is how the whole thing shuts down
cleanly.** The two ways it breaks — a forgotten sender (deadlock) and a guard held too
long (serialization) — are both really lessons about *when a value is dropped*. Master
"who still holds a `Sender`?" and "when does this guard fall out of scope?" and async
tokio stops surprising you.

## Explain it back

- Why does `sleep(d)` with no `.await` do nothing, and why is it only a warning?
- Ten 50 ms tasks: why does spawn-all-then-await take ~50 ms but spawn-and-await-each
  take ~500 ms?
- When would you pick `join!` over `spawn`, and vice versa? What bounds does each need?
- In `select!`, what physically happens to the losing future, and how does the ladder
  *prove* it?
- Why does `spawn` require `Send + 'static`? What does each bound prevent?
- On a single-threaded runtime, why does `std::thread::sleep(200ms)` in one task delay a
  20 ms timer to ~220 ms — and why doesn't `yield_now`?
- When does `mpsc::Receiver::recv()` return `None`? Name two ways to accidentally hang a
  consumer.
- In the capstone, why must the `Mutex` guard be dropped before `spawn_blocking`? Why
  must main drop its own `results_tx` before the collect loop?

## See also

- [`Future` trait & `poll`](future-poll.md) — what `.await` desugars to; the state
  machine and waker that make all of this run.
- [Channels](channels.md) — the `std::sync::mpsc` and crossbeam story; tokio's `mpsc`
  is the async sibling.
- [`Send` & `Sync` deeply](send-sync.md) — why `spawn` demands `Send`, and why `Rc` is
  out but `Arc` is in.
- [`Mutex` / `RwLock`](mutex-rwlock.md) — the sync locks; `tokio::sync::Mutex` is the
  await-friendly version used in the capstone.
- [Shared state vs message passing](concurrency-models.md) — the design axis this whole
  ladder lives on.
