# `Future` trait & `poll`

> Ladder: [`src/bin/future_poll.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/future_poll.rs) ·
> Run: `cargo run --bin future_poll` · Phase 5 · 9 rungs

## TL;DR

A `Future` is a struct with exactly one method:

```rust
fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
```

where `Poll` is `Ready(T) | Pending`. That's the *whole* interface. Two facts fall out of it, and the entire async ecosystem is built on them:

1. **`.await` is a poll loop.** "Run this future" means: call `poll` repeatedly until it returns `Ready`. When a sub-future returns `Pending`, you propagate that `Pending` up to *your* caller and try again later.
2. **`async fn` compiles to an enum state machine.** Each `.await` is a suspension point, and the compiler emits one enum variant per suspension point. `poll` is a `loop { match self.state { ... } }` driver that resumes at the current state.

Everything else — wakers, executors, `Pin`, `join!` — is machinery around those two facts.

## Why this exists (from first principles)

Start with the problem async is solving: you have thousands of tasks that spend most of their time *waiting* (on sockets, timers, locks). Threads don't scale here — a thread that blocks on a socket costs a full OS stack and a context switch to wake. You want thousands of concurrent waits on a handful of threads.

To do that, a task has to be **suspendable and resumable as a plain value** — something you can poll, park, and come back to, without a dedicated stack. That value is a `Future`, and its `poll` method is "make as much progress as you can right now, then tell me whether you finished (`Ready`) or you're parked waiting (`Pending`)".

Two constraints immediately appear, and the whole design answers them:

- **How does a parked task get resumed?** The executor can't busy-poll ten thousand idle tasks. So `poll` receives a `Context` carrying a `Waker` — a callback meaning "when I can make progress, call this and you'll re-poll me". That's how a task sleeps at zero CPU cost until its socket is actually readable.
- **How is per-suspension state preserved?** A synchronous function keeps locals on the stack across a blocking call. A future has no stack. So the state that must survive across an `.await` gets stored **in the future's own struct** — which is exactly why the compiler turns `async fn` into a state-machine enum, and why those state machines can become self-referential (the topic of the next ladder, `Pin`).

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `Ready<T>` | The `Poll` enum and the `poll(Pin<&mut Self>, cx)` shape |
| 2 | foundations | `block_on` | `.await` / "run a future" IS a poll loop |
| 3 | mechanics | `YieldOnce` | `Pending` means "poll me again"; state persists across polls |
| 4 | mechanics | `Map<Fut, F>` | Delegation: poll a child, propagate `Pending`, transform `Ready` |
| 5 | footgun | `TwoStep` | The generated state machine, written by hand |
| 6 | footgun | contract | The waker contract + poll-after-`Ready`, both made to fail |
| 7 | real-world | `async fn` | An `async` block *is* your hand-rolled `TwoStep` |
| 8 | real-world | `Join` | Poll two futures per turn — concurrency, not parallelism |
| 9 | capstone | `BranchMachine` | Desugar `async fn` (2 awaits + a branch) + a real waker executor |

## The ideas, built up

### 1. The shape: `Poll` and one method

The simplest possible future is one that is already done:

```rust
struct Ready<T> { value: Option<T> }

impl<T> Future for Ready<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let value = this.value.take().unwrap();  // move T out of &mut self
        Poll::Ready(value)
    }
}
```

Three details that recur everywhere:

- **`self: Pin<&mut Self>`** is a *pinned* mutable borrow (a promise the value won't move again — see `Pin` ladder). `Ready<T>` isn't self-referential, so `self.get_mut()` unwraps it back to `&mut Self`. `get_mut` is only offered when `Self: Unpin`, because handing out `&mut T` lets safe code `mem::swap` the value elsewhere — fine only when moving is harmless.
- **`value: Option<T>`** exists because you can't move a `T` out of a `&mut` borrow, but you *can* `.take()` it out of an `Option`, leaving `None` behind. Same trick reappears for `FnOnce` closures (rung 4).
- **`_cx` is ignored** — a future that is already done has no reason to arrange a wake-up. That "why doesn't this one need the waker?" question is answered in rung 3.

### 2. `.await` is a poll loop

There is no magic under "run this future". It's a loop:

```rust
fn block_on<F: Future + Unpin>(fut: F) -> F::Output {
    let mut fut = fut;
    let mut fut = Pin::new(&mut fut);
    let mut cx = noop_cx();                    // a do-nothing Waker::noop()
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending  => {}               // busy-spin: just poll again
        }
    }
}
```

This is a *busy-spin* executor: on `Pending` it immediately re-polls, burning CPU. That's fine for a toy and terrible for real life — but it's the honest skeleton. A real executor differs in exactly one place: instead of `Poll::Pending => {}`, it **parks the thread** and only re-polls when the waker fires (rungs 6 and 9).

**The pinning choice matters.** `Pin::new(&mut fut)` is alloc-free but requires `F: Unpin`. The alternative, `Box::pin(fut)`, heap-allocates and yields `Pin<Box<F>>`, which is *always* `Unpin` regardless of `F` — so it accepts even `!Unpin` async state machines with no `unsafe`. The `Unpin` bound here is what forces the E0277 in rung 7.

### 3. `Pending` means "poll me again"

Every future so far finished on the first poll. The interesting ones suspend:

```rust
struct YieldOnce { yielded: bool }

impl Future for YieldOnce {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.yielded {
            Poll::Ready(())
        } else {
            this.yielded = true;
            Poll::Pending          // "not done — call me again later"
        }
    }
}
```

Poll it by hand and you see the two-step: first poll → `Pending`, second poll → `Ready`. This is what a single `.await` on something-not-yet-ready looks like from the inside.

The important insight hides in `yielded: bool`. That flag is **state persisted across `poll` calls**. Scale it up from "one bool" to "which `.await` am I parked at, and what locals are still alive?" and you have invented the async state machine (rung 5).

### 4. Delegation: the combinator shape

A `Map` future owns another future and transforms its output:

```rust
impl<Fut, F, B> Future for Map<Fut, F>
where Fut: Future + Unpin, F: FnOnce(Fut::Output) -> B {
    type Output = B;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Poll::Ready(v) = Pin::new(&mut this.inner).poll(cx) {
            let f = this.f.take().unwrap();     // FnOnce: move it out to call it
            Poll::Ready(f(v))
        } else {
            Poll::Pending                        // propagate the child's suspension
        }
    }
}
```

This "poll the child, propagate `Pending`, map the `Ready`" pattern is the entire job of every adapter (`.map`, `.and_then`, the `?`-in-async plumbing). Two things worth burning in:

- **`cx` is forwarded, unchanged, to the child.** A leaf future five layers deep registers *the executor's* waker; every combinator in between is just a pipe. Poll flows down; the waker (registered from that same `cx`) flows back up.
- **`f` is an `Option<F>`** for the same reason `Ready`'s value was — calling an `FnOnce` moves it, so you `.take()` it out of `&mut self`. It runs exactly once, on the single `Ready`.

### 5. The state machine, by hand

This is the center of the ladder. Consider:

```rust
async {
    let a = fut_a().await;   // suspension point #1
    let b = fut_b().await;   // suspension point #2
    a + b
}
```

The compiler turns this into an enum with one variant per suspension point:

```rust
enum TwoStep {
    Running1 { a_fut: FutA },            // awaiting #1
    Running2 { b_fut: FutB, a: i32 },    // #1 done; awaiting #2, carrying `a`
    Done,                                 // terminal
}
```

And `poll` is a `loop { match state }` driver:

```rust
fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let this = self.get_mut();
    loop {
        match this {
            TwoStep::Running1 { a_fut } => {
                if let Poll::Ready(a) = Pin::new(a_fut).poll(cx) {
                    *this = TwoStep::Running2 { b_fut: FutB { polled: false }, a };
                    continue;                     // fall through to poll #2 immediately
                } else {
                    return Poll::Pending;
                }
            }
            TwoStep::Running2 { b_fut, a } => {
                if let Poll::Ready(b) = Pin::new(b_fut).poll(cx) {
                    let a = *a;                   // copy out BEFORE overwriting *this
                    *this = TwoStep::Done;
                    return Poll::Ready(a + b);
                } else {
                    return Poll::Pending;
                }
            }
            TwoStep::Done => panic!("TwoStep polled after completion"),
        }
    }
}
```

Three ideas made concrete:

- **Each `.await` is a variant.** Two awaits → `Running1`, `Running2`. The `loop { match state }` is the resume mechanism: a fresh `poll` call re-enters at whichever variant it left off in.
- **The `continue` after a transition is "keep making progress".** An `async fn` does not return to the executor merely because one sub-future finished — it drives forward until something is genuinely `Pending`. If `fut_a` were instantly ready, the hand-poll would see one fewer `Pending`.
- **`Running2` carries `a`.** A local alive *across* an await must be stored in the future's own struct. That is why async state machines get large — and, in the real compiler output, why they become self-referential and `!Unpin`.

> **Borrow-checker subtlety:** `let a = *a;` must come *before* `*this = TwoStep::Done;`. `a` is borrowed out of `this`; overwriting `*this` invalidates that borrow. Copying it out first is exactly what the compiler does when it moves a live-across-await local out of the old state. For a non-`Copy` local you'd reach for `std::mem::replace(this, Done)` to take the old variant *by value* and then move fields out of it — the heavyweight generalization of the `Option::take` trick.

### 6. `async fn` IS a `Future`

Rung 7 closes the loop by writing the same logic as an `async` block and proving it is indistinguishable from the hand-rolled `TwoStep`:

```rust
fn two_step_async() -> impl Future<Output = i32> {
    async {
        let a = fut_a().await;
        let b = fut_b().await;
        a + b
    }
}
```

The assertions confirm both the same result (`42`) *and* the same `Pending, Pending, Ready` rhythm when polled by hand — because the compiler placed a suspension point at each `.await`, right where `Running1`/`Running2` were. The generated enum is a type you can't name or inspect (the "one hidden type" of `-> impl Future`), but it behaves exactly like the one you wrote.

### 7. `join` — concurrency without parallelism

`TwoStep` was *sequential*: it never touched `fut_b` until `fut_a` finished. `Join` drives both every poll and finishes when both are done:

```rust
fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let this = self.get_mut();
    if let Some(fut) = &mut this.a {
        if let Poll::Ready(v) = Pin::new(fut).poll(cx) {
            this.a_out = Some(v);
            this.a = None;                 // done — never poll it again
        }
    }
    if let Some(fut) = &mut this.b { /* ...same for b... */ }

    if this.a_out.is_some() && this.b_out.is_some() {
        Poll::Ready((this.a_out.take().unwrap(), this.b_out.take().unwrap()))
    } else {
        Poll::Pending
    }
}
```

This is `futures::join!` / `tokio::join!` in miniature. The design teaches three things:

- **You poll *both* per call.** That interleaving on a single task *is* async concurrency — no extra threads. `join(fut_a, fut_b)` finishes in exactly two polls because both sides advance together.
- **`this.a = None` after `Ready` is a structural fuse.** Setting the `Option` to `None` guarantees you never poll a completed future again (the rung-6 rule). The `is_some()` guard is a hand-rolled `FusedFuture`.
- **You must stash results.** The two sides finish at different times; taking `a_out` early would discard A's value permanently (its future is already `None`), so `Join` would never complete. The mixed-readiness case (`join(ready(99), fut_b())`) exercises exactly this.

## Footguns

### The waker contract (the #1 real-world async bug)

`Pending` is a two-part promise: *"I'm not done — AND I have arranged (via `cx.waker()`) to be woken when I can progress."* A `Pending` returned **without registering the waker** is a lost task: under a parking executor it is polled once and never again — a permanent hang.

The busy-spin `block_on` hides this because it re-polls unconditionally. A realistic executor exposes it. The ladder builds one from a flag waker:

```rust
struct FlagWaker { woken: AtomicBool }
impl Wake for FlagWaker {
    fn wake(self: Arc<Self>)         { self.woken.store(true, Ordering::SeqCst); }
    fn wake_by_ref(self: &Arc<Self>) { self.woken.store(true, Ordering::SeqCst); }
}
```

```rust
// re-poll ONLY if the waker fired since the last poll
if !flag.woken.swap(false, Ordering::SeqCst) {
    return Err("stuck: future returned Pending without registering a wake");
}
```

Now the split is visible:

```rust
// OK: arms the waker before parking -> executor re-polls -> Ok(7)
this.yielded = true;
cx.waker().wake_by_ref();
Poll::Pending

// WRONG (rung 3's YieldOnce): returns Pending, never touches cx
// -> parking executor returns Err("stuck") -> in production, a real hang
this.yielded = true;
Poll::Pending
```

`YieldOnce` "worked" under busy-spin and *hangs* under a parking executor. A production runtime replaces "check a bool, else give up" with "block the thread until woken", and `wake()` replaces "set a bool" with "push this task back on the ready queue".

### Poll-after-`Ready`

Once `poll` returns `Ready`, the future is spent and must not be polled again — it may panic (like `TwoStep::Done`) or return garbage. The ladder drives `TwoStep` to completion and pokes it once more inside `catch_unwind` to confirm the panic. This is why combinators that might over-poll (`select!`, racing) require `FusedFuture` / `.fuse()`, which makes an already-finished future return `Pending` instead of exploding.

## Real-world patterns

- **`Waker` construction.** In real runtimes a `Waker` is a hand-built vtable (`RawWaker` + `RawWakerVTable`) so it needs no allocator and can point at anything. Std's friendly door is the `Wake` trait on an `Arc<T>` plus `Waker::from(arc)` — exactly what `FlagWaker` uses. `Waker::noop()` (stable since 1.85) is the do-nothing waker for hand-polling.
- **`Context` is a `Waker` carrier.** Today it is essentially `&Waker` with room to grow. Every combinator takes `cx` and forwards the *same* `cx` down — which is how a deep leaf registers the executor's waker.
- **`block_on` takes pinned futures.** `futures::executor::block_on` and friends pin the future (often via `Box::pin` or `pin!`) because compiler-generated async futures are `!Unpin`. That single requirement is the `Unpin` wall you hit in rung 7.
- **`join!` interleaves on one task.** Concurrency (interleaved progress) is distinct from parallelism (multiple threads). `Join` gives the former with zero threads.

## Capstone insight

Rung 9 desugars an `async fn` with **a runtime branch** and drives it with a proper waker-parking executor:

```rust
async fn compute(n: i32) -> i32 {
    let a = wake_yield(n).await;        // await #1 — ALWAYS
    if a % 2 == 0 {
        let b = wake_yield(a * 2).await;   // await #2 — ONLY on the even branch
        a + b
    } else {
        a                                   // odd branch: no second await
    }
}
```

The hand-written machine encodes the branch as a transition that only *sometimes* reaches the second suspension point:

```rust
enum BranchMachine {
    Await1 { fut1: WakeYield },
    Await2 { fut2: WakeYield, a: i32 },   // even branch only
    Done,
}
```

The `Await1 -> Ready(a)` arm chooses: if `a` is even, transition to `Await2` and `continue`; if odd, go straight to `Done` and return. The branch means the **number of suspension points depends on a runtime value** — the odd path never constructs `Await2`, exactly as the `else` arm never hits a second `.await`.

The executor is the synthesis of rungs 2 and 6:

```rust
fn run<F: Future>(fut: F) -> F::Output {
    let flag = Arc::new(FlagWaker { woken: AtomicBool::new(true) });
    let waker = Waker::from(flag.clone());
    let mut cx = Context::from_waker(&waker);
    let mut fut = Box::pin(fut);              // accept !Unpin async futures
    loop {
        if !flag.woken.swap(false, Ordering::SeqCst) { panic!("stuck"); }
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending  => {}             // parked until the next wake
        }
    }
}
```

The payoff assertion sweeps `run(compute_machine(n)) == run(compute(n))` for every `n` in `0..20`, covering both branches. **The aha:** `async`/`.await` is not a runtime feature — it is a *source transformation* into a `Future` state machine, and once you can write that machine by hand, `.await` becomes readable as the enum it compiles to.

## Explain it back

- What are the two variants of `Poll`, and what does returning each one obligate you to do?
- Why does `poll` take `Pin<&mut Self>` and not `&mut self`? Why does `get_mut` require `Unpin`?
- What does `.await` desugar to? How many enum variants does an `async fn` with three `.await`s generate?
- A future returns `Pending` but never touches `cx`. Why does it work under a busy-spin `block_on` and hang under a real executor?
- Why must a local used *after* an `.await` be stored in the future's struct rather than on the stack? What does that imply about the size and `Unpin`-ness of async state machines?
- How does `join` differ from two sequential `.await`s, and why must it set each side's `Option` to `None` after `Ready`?
- Why does `block_on(some_async_fn())` fail to compile with an `Unpin` bound, and how does `Box::pin` fix it?

## See also

- [`Send` & `Sync` deeply](send-sync.md) — an async future is `Send` iff every value held across an `.await` is `Send`.
- [Shared state vs message passing](concurrency-models.md) — the sync concurrency models the async ones mirror.
- [`impl Trait` & RPIT](impl-trait.md) — `async fn` is sugar for `-> impl Future`, the "one hidden type" this ladder relies on.
- Next in Phase 5: `Pin` & `Unpin` (why the state machines above are `!Unpin`), writing a future by hand (wakers & `Context` in full), and building a tiny executor (the parking loop, for real).
