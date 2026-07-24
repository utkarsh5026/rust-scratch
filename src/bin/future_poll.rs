// Future trait & poll — what `.await` desugars to, the generated state machine
// Run: cargo run --bin future_poll
//
// Mental model: a Future is a struct with ONE method —
//     fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Output>
// where Poll is `Ready(T) | Pending`. `.await` is not magic: it's a loop that
// calls poll until Ready, returning Pending up to its own caller in between.
// An `async fn` compiles to an ENUM STATE MACHINE — one variant per `.await`.
//
// Ladder:
//   1. Poll enum & Future shape — trivial `Ready<T>`                 [foundations]  <- YOU ARE HERE
//   2. `.await` is a poll loop — hand-write `block_on`               [foundations]
//   3. Pending means "poll me again" — `YieldOnce`                   [mechanics]
//   4. Delegation — `Map<Fut, F>` combinator                        [mechanics]
//   5. The state machine, by hand — enum TwoStep                    [footgun]
//   6. Contract violations — busy-loop + poll-after-Ready           [footgun]
//   7. `async fn` IS a Future — prove equivalence                   [real-world]
//   8. `join` by hand — poll two futures, done when both Ready      [real-world]
//   9. Capstone — desugar an async fn (2 awaits + branch) + waker   [capstone]

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

// A tiny helper so you can poll a future by hand without an executor.
// `Waker::noop()` is a real, do-nothing waker (stable since 1.85).
fn noop_cx() -> Context<'static> {
    Context::from_waker(Waker::noop())
}

// ---------------------------------------------------------------------------
// Problem 1 [foundations]: The Poll enum & the Future shape.
//
// `Ready<T>` is the simplest possible future: it is already done, so the very
// first poll must hand back the value as `Poll::Ready(value)`.
//
// The value is stored in an `Option<T>` so you can move it out of `&mut self`
// (you can't move a `T` out of a borrow, but you can `.take()` it from Option).
//
// Signature notes:
//   - `self: Pin<&mut Self>` — a pinned mutable borrow. `Ready<T>` isn't
//     self-referential, so unwrap it with `self.get_mut()` to get `&mut Self`.
//   - return `Poll::Ready(...)`, never `Poll::Pending` (it's already complete).
struct Ready<T> {
    value: Option<T>,
}

fn ready<T>(value: T) -> Ready<T> {
    Ready { value: Some(value) }
}

// Ready never creates self-references, so it is always safe to move after pinning.
// Without this, Ready<T> is Unpin only when T: Unpin, and get_mut()/Pin::new fail.
impl<T> Unpin for Ready<T> {}

impl<T> Future for Ready<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let value = this.value.take().unwrap();
        Poll::Ready(value)
    }
}

fn check_1() {
    let mut fut = ready(42);
    // SAFETY-free: Ready<T> is Unpin, so we can pin it to the stack.
    let mut fut = Pin::new(&mut fut);
    let mut cx = noop_cx();
    match fut.as_mut().poll(&mut cx) {
        Poll::Ready(v) => assert_eq!(v, 42, "Ready should yield the stored value"),
        Poll::Pending => panic!("a Ready future must never return Pending"),
    }
    println!("check_1 ok: Ready<T> completes on first poll");
}

// ---------------------------------------------------------------------------
// Problem 2 [foundations]: `.await` is a poll loop — hand-write `block_on`.
//
// There is no magic behind `.await` or an executor's "run this future": at the
// bottom it's just a LOOP that calls `poll` and reacts to the result:
//   - Poll::Ready(v)  -> we're done, return v.
//   - Poll::Pending   -> not done yet; go around and poll again.
//
// A real executor would PARK the thread on Pending and only wake when the waker
// fires (rung 9). For now, busy-spin: just loop and re-poll. Because our futures
// so far complete quickly, the loop terminates.
//
// Implementation notes:
//   - Pin the future to the stack: `let mut fut = fut;` then
//     `let mut fut = unsafe { Pin::new_unchecked(&mut fut) };` — OR, since every
//     future you'll build in early rungs is Unpin, `Pin::new(&mut fut)` is fine.
//   - Build a Context from `noop_cx()` each iteration (or once — noop never changes).
//   - `loop { match fut.as_mut().poll(&mut cx) { Ready(v) => return v, Pending => {} } }`
fn block_on<F: Future + Unpin>(fut: F) -> F::Output {
    let mut fut = fut;
    let mut fut = Pin::new(&mut fut);
    let mut cx = noop_cx();
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => {}
        }
    }
}

fn check_2() {
    let out = block_on(ready("hello from a future"));
    assert_eq!(out, "hello from a future");

    // Nesting a computation: block_on drives ANY future to completion.
    let n = block_on(ready(20)) + block_on(ready(22));
    assert_eq!(n, 42);
    println!("check_2 ok: block_on drives poll to Ready");
}

// ---------------------------------------------------------------------------
// Problem 3 [mechanics]: `Pending` means "poll me again" — `YieldOnce`.
//
// Every future so far finished on the first poll. The interesting ones DON'T:
// they return `Poll::Pending` to say "I'm not done — suspend me and call poll
// again later." `YieldOnce` is the minimal example: it returns Pending the
// FIRST time it's polled, and Ready the SECOND time.
//
// This is what a single `.await` on something-not-ready looks like from the
// inside: poll -> Pending (control returns to block_on), then later poll again
// -> Ready. YieldOnce is essentially `tokio::task::yield_now()`.
//
// You need to remember "have I been polled before?" ACROSS poll calls — poll
// takes `&mut self`, so store a bool (or a small state) in the struct and flip
// it. This tiny bit of persisted state IS the seed of the state machine idea.
//
// IMPORTANT: on the Pending branch, a real future would register the waker
// (`cx.waker().wake_by_ref()` or store it) so the executor knows to re-poll.
// With our busy-spin block_on we get re-polled regardless, but do it anyway —
// it's the contract, and rung 6 shows what breaks without a real waker.
struct YieldOnce {
    yielded: bool,
}

fn yield_once() -> YieldOnce {
    YieldOnce { yielded: false }
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.yielded {
            Poll::Ready(())
        } else {
            this.yielded = true;
            Poll::Pending
        }
    }
}

fn check_3() {
    // Poll it by hand so you can SEE the two-step: Pending then Ready.
    let mut fut = yield_once();
    let mut fut = Pin::new(&mut fut);
    let mut cx = noop_cx();
    assert!(
        fut.as_mut().poll(&mut cx).is_pending(),
        "first poll must be Pending"
    );
    assert!(
        fut.as_mut().poll(&mut cx).is_ready(),
        "second poll must be Ready"
    );

    // And it terminates under block_on (which just polls again after Pending).
    block_on(yield_once());
    println!("check_3 ok: YieldOnce suspends once, then completes");
}

// ---------------------------------------------------------------------------
// Problem 4 [mechanics]: Delegation — a `Map<Fut, F>` combinator.
//
// So far each future was a leaf. Now build a future that CONTAINS another future
// and transforms its output: `Map { inner, f }` polls `inner`, and when it's
// Ready(v) returns Ready(f(v)); while `inner` is Pending, Map is Pending too.
// This "poll the child, propagate Pending, map the Ready" pattern is the whole
// job of every combinator (`.map`, `.and_then`, the desugaring of `?` in async).
//
// The catch — Pin projection. `self` is `Pin<&mut Map>`, but `inner`'s poll wants
// `Pin<&mut Fut>`. You must turn a pin of the WHOLE into a pin of the FIELD. The
// rule: if the outer type is Unpin you may reach in safely; in general it's an
// `unsafe` projection because you promise not to move `inner` out. To keep this
// rung about delegation (not Pin mechanics — that's the NEXT ladder), we require
// `Fut: Unpin` and use `Pin::new(&mut ...)` on the field. No unsafe needed.
//
// Also note `f` is an `Option<F>`: an `FnOnce` is consumed when called, and you
// can only move it out of `&mut self` via `.take()` (same trick as rung 1).
//
// Steps inside poll:
//   1. `let this = self.get_mut();`  (Map: Fut is Unpin -> Map is Unpin)
//   2. poll the inner future: `Pin::new(&mut this.inner).poll(cx)`
//   3. match: Pending -> return Pending; Ready(v) -> take f, return Ready(f(v))
struct Map<Fut, F> {
    inner: Fut,
    f: Option<F>,
}

fn map<Fut, F, B>(inner: Fut, f: F) -> Map<Fut, F>
where
    Fut: Future,
    F: FnOnce(Fut::Output) -> B,
{
    Map { inner, f: Some(f) }
}

impl<Fut, F, B> Future for Map<Fut, F>
where
    Fut: Future + Unpin,
    F: FnOnce(Fut::Output) -> B,
    Map<Fut, F>: Unpin,
{
    type Output = B;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Poll::Ready(v) = Pin::new(&mut this.inner).poll(cx) {
            let f = this.f.take().unwrap();
            Poll::Ready(f(v))
        } else {
            Poll::Pending
        }
    }
}

fn check_4() {
    let out = block_on(map(ready(21), |x| x * 2));
    assert_eq!(out, 42);

    // Map over a future that suspends first: proves Pending is propagated, then
    // the transform runs exactly once on the eventual value.
    let out = block_on(map(map(yield_once(), |()| 40), |n| n + 2));
    assert_eq!(out, 42);
    println!("check_4 ok: Map delegates poll and transforms the Ready value");
}

// ---------------------------------------------------------------------------
// Problem 5 [footgun]: The state machine, by hand — enum `TwoStep`.
//
// THIS IS THE CENTER OF THE WHOLE LADDER. Consider this async block:
//
//     async {
//         let a = first.await;   // suspension point #1
//         let b = second.await;  // suspension point #2
//         a + b
//     }
//
// The compiler turns it into an ENUM with one variant per suspension point
// (plus a Done variant). Each `.await` is a place the function can PAUSE and
// later RESUME — so the enum must store, in each variant, the child future
// currently being awaited AND any locals that are still alive across that await
// (here: `a`, which is computed at await #1 and used after await #2).
//
// `poll` is a state-machine DRIVER: a `loop` with a `match self.state`. In each
// state it polls the current child; on Pending it returns Pending (staying in
// the same state); on Ready it advances `self.state` to the next variant and
// loops again to immediately poll the next child. When it reaches Done it
// returns the final value — and it must never be polled past that.
//
// Your task: complete `poll` for TwoStep, which awaits two `FutA`/`FutB`
// futures in sequence and returns the sum of their outputs. We use two concrete
// leaf futures (FutA -> i32, FutB -> i32) built on YieldOnce-style suspension so
// each one actually pends once before completing. This mirrors:
//     async { let a = fut_a().await; let b = fut_b().await; a + b }
//
// State design (already written for you): the enum threads the surviving local
// `a` into the second variant. Study how `Running2` carries BOTH the second
// future and the already-computed `a` — that carrying-locals-across-awaits is
// precisely why async state machines can be large and why they're !Unpin.

// Two leaf futures that each pend once, then yield an i32. (FutA -> 20, FutB -> 22.)
struct FutA {
    polled: bool,
}

impl Future for FutA {
    type Output = i32;
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<i32> {
        let this = self.get_mut();
        if this.polled {
            Poll::Ready(20)
        } else {
            this.polled = true;
            Poll::Pending
        }
    }
}

struct FutB {
    polled: bool,
}

impl Future for FutB {
    type Output = i32;
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<i32> {
        let this = self.get_mut();
        if this.polled {
            Poll::Ready(22)
        } else {
            this.polled = true;
            Poll::Pending
        }
    }
}

// The generated state machine. Each variant = one suspension point.
enum TwoStep {
    // Awaiting fut_a; nothing computed yet.
    Running1 { a_fut: FutA },
    // fut_a produced `a`; now awaiting fut_b, carrying `a` across the await.
    Running2 { b_fut: FutB, a: i32 },
    // Terminal: already produced the result. Polling here is a contract violation.
    Done,
}

fn two_step() -> TwoStep {
    TwoStep::Running1 {
        a_fut: FutA { polled: false },
    }
}

impl Future for TwoStep {
    type Output = i32;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // TwoStep contains only Unpin futures, so get_mut() is fine (real async
        // state machines are !Unpin — that's the NEXT ladder's whole point).
        let this = self.get_mut();
        loop {
            match this {
                TwoStep::Running1 { a_fut } => {
                    if let Poll::Ready(a) = Pin::new(a_fut).poll(cx) {
                        let running2 = TwoStep::Running2 {
                            b_fut: FutB { polled: false },
                            a,
                        };
                        let _ = std::mem::replace(this, running2);
                        continue;
                    } else {
                        return Poll::Pending;
                    }
                }
                TwoStep::Running2 { b_fut, a } => {
                    if let Poll::Ready(b) = Pin::new(b_fut).poll(cx) {
                        let a = *a;
                        let _ = std::mem::replace(this, TwoStep::Done);
                        return Poll::Ready(a + b);
                    } else {
                        return Poll::Pending;
                    }
                }
                TwoStep::Done => {
                    panic!("TwoStep polled after completion")
                }
            }
        }
    }
}

fn check_5() {
    let out = block_on(two_step());
    assert_eq!(out, 42, "20 + 22");
    let mut fut = two_step();
    let mut fut = Pin::new(&mut fut);
    let mut cx = noop_cx();
    assert!(fut.as_mut().poll(&mut cx).is_pending(), "await #1 pends");
    assert!(fut.as_mut().poll(&mut cx).is_pending(), "await #2 pends");
    assert_eq!(fut.as_mut().poll(&mut cx), Poll::Ready(42), "then done");
    println!("check_5 ok: hand-written state machine == async {{ a.await; b.await; a+b }}");
}

// ---------------------------------------------------------------------------
// Problem 6 [footgun]: Contract violations — the two ways poll goes wrong.
//
// The Future::poll contract has two rules that our busy-spin block_on has been
// silently papering over. This rung makes both failures VISIBLE.
//
// (A) THE WAKER CONTRACT. "Return Pending" is a PROMISE: *I have arranged to be
//     woken (via cx.waker()) when I can make progress.* A future that returns
//     Pending without ever registering the waker is buggy — under a real
//     executor that PARKS on Pending, it will be polled once and then never
//     again: a permanent hang. Our busy-spin block_on hides this because it
//     re-polls unconditionally. Here you'll write a tiny REALISTIC executor that
//     only re-polls when the waker is invoked, and watch the difference:
//       - YieldOnce (rung 3) never calls wake -> would hang -> so we detect it.
//       - WellBehaved calls cx.waker().wake_by_ref() before Pending -> progresses.
//
// (B) POLL-AFTER-READY. Once poll returns Ready, the future is DONE and must not
//     be polled again — it may panic (like your TwoStep::Done), or return
//     garbage. This is why combinators that might over-poll (e.g. select, or
//     racing) rely on `FusedFuture`/fuse() to make "already done" return Pending
//     safely instead of exploding. You'll trigger the panic and confirm it.
//
// ---- Part A scaffolding: a park-on-Pending executor with a wake flag. ----
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Wake;

// A waker that just flips a shared "woken" flag. Waking == "please re-poll me".
struct FlagWaker {
    woken: AtomicBool,
}
impl Wake for FlagWaker {
    fn wake(self: Arc<Self>) {
        self.woken.store(true, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.woken.store(true, Ordering::SeqCst);
    }
}

// A realistic-ish block_on: after a Pending, it only re-polls if the waker was
// invoked since the last poll. If Pending came back WITHOUT a wake, there is no
// way to ever make progress, so we return an error instead of looping forever.
// (A real executor would sleep the thread until woken; here "no wake" == stuck.)
fn block_on_parking<F: Future + Unpin>(fut: F) -> Result<F::Output, &'static str> {
    let flag = Arc::new(FlagWaker {
        woken: AtomicBool::new(true), // allow the first poll
    });
    let waker = Waker::from(flag.clone());
    let mut cx = Context::from_waker(&waker);
    let mut fut = fut;
    let mut fut = Pin::new(&mut fut);
    loop {
        if !flag.woken.swap(false, Ordering::SeqCst) {
            // Pending was returned but nobody armed the waker -> we'd sleep forever.
            return Err("stuck: future returned Pending without registering a wake");
        }
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return Ok(v),
            Poll::Pending => { /* loop; only continues if woken flag got set */ }
        }
    }
}

// A well-behaved one-shot suspend: registers the waker before returning Pending.
struct WellBehaved {
    yielded: bool,
}
impl Future for WellBehaved {
    type Output = i32;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
        let this = self.get_mut();
        if this.yielded {
            Poll::Ready(7)
        } else {
            this.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

fn check_6() {
    // (A) The well-behaved future arms the waker -> the parking executor makes progress.
    assert_eq!(
        block_on_parking(WellBehaved { yielded: false }),
        Ok(7),
        "WellBehaved must wake the executor before Pending"
    );

    // ...whereas rung 3's YieldOnce never wakes -> the SAME executor detects the stall.
    assert_eq!(
        block_on_parking(yield_once()),
        Err("stuck: future returned Pending without registering a wake"),
        "YieldOnce never registers a wake, so a parking executor would hang"
    );

    // (B) Poll-after-Ready is a contract violation. Drive TwoStep to completion,
    // then poll once more and confirm it panics (your Done arm).
    let mut fut = two_step();
    let mut fut = Pin::new(&mut fut);
    let mut cx = noop_cx();
    while fut.as_mut().poll(&mut cx).is_pending() {}
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = fut.as_mut().poll(&mut cx);
    }));
    assert!(
        caught.is_err(),
        "polling past Ready must be rejected (Done arm panics)"
    );

    println!("check_6 ok: waker contract + poll-after-Ready both made visible");
}

// ---------------------------------------------------------------------------
// Problem 7 [real-world]: `async fn` IS a Future — prove equivalence.
//
// You hand-wrote TwoStep in rung 5. Now write the SAME logic as an `async`
// block and prove the compiler generates a future that behaves identically —
// same result, same Pending/Pending/Ready rhythm. That equivalence is the
// entire point of this ladder: `async { .. }` is sugar for a state machine like
// the one you built by hand.
//
// Constructors so the async block reads cleanly (reuse rung 5's leaf futures):
fn fut_a() -> FutA {
    FutA { polled: false }
}
fn fut_b() -> FutB {
    FutB { polled: false }
}

// YOUR TASK: fill in the async block so it awaits fut_a then fut_b and returns
// their sum — the exact logic your TwoStep enum encodes:
//     let a = fut_a().await;
//     let b = fut_b().await;
//     a + b
// (An `async` block is an expression whose value is a Future. This function
//  returns that future via RPIT — `impl Future`, one hidden compiler-generated
//  state-machine type, just like TwoStep but written for you by rustc.)
fn two_step_async() -> impl Future<Output = i32> {
    async {
        let a = fut_a().await;
        let b = fut_b().await;
        a + b
    }
}

fn check_7() {
    // THE UNPIN WALL (the E0277 promised back at rung 2): a compiler-generated
    // async future is `!Unpin` (it may store self-references across awaits), so
    // `block_on(two_step_async())` won't compile — block_on requires F: Unpin.
    //   Box::pin(fut) moves it to the heap and yields `Pin<Box<F>>`, which is
    //   ALWAYS Unpin regardless of F, so block_on accepts it. Try deleting the
    //   Box::pin below to see the E0277, then put it back.
    let out = block_on(Box::pin(two_step_async()));
    assert_eq!(out, 42, "async version must equal the hand-written TwoStep");

    // Same suspension rhythm as check_5: two awaits, each pends once.
    let mut fut = Box::pin(two_step_async());
    let mut cx = noop_cx();
    assert!(fut.as_mut().poll(&mut cx).is_pending(), "await #1 pends");
    assert!(fut.as_mut().poll(&mut cx).is_pending(), "await #2 pends");
    assert_eq!(fut.as_mut().poll(&mut cx), Poll::Ready(42), "then done");

    println!("check_7 ok: `async {{ a.await; b.await; a+b }}` == hand-rolled TwoStep");
}

// ---------------------------------------------------------------------------
// Problem 8 [real-world]: `join` by hand — poll two futures, done when BOTH ready.
//
// TwoStep ran its futures SEQUENTIALLY: it didn't even touch fut_b until fut_a
// finished. `join` runs two futures CONCURRENTLY: on every poll it drives BOTH
// as far as they'll go, and only returns Ready once BOTH have produced a value.
// This is exactly `futures::join!` / `tokio::join!` — concurrency (interleaving
// on one task) without parallelism (no extra threads).
//
// The design carries each side as an `Option`:
//   - `a: Option<A>` holds the future while it's still running; becomes `None`
//     once it completes (so we STOP polling it — polling past Ready is the rung-6
//     contract violation!).
//   - `a_out: Option<A::Output>` stashes its result until the OTHER side is done.
// Same for b. When both outputs are present, take them and return Ready((oa, ob)).
//
// This is why join must remember results: side A may finish on poll #1 while B
// is still Pending; you hold A's value and keep polling only B.
//
// Inside poll (A, B are Unpin so get_mut + Pin::new work, no projection):
//   1. let this = self.get_mut();
//   2. if let Some(fut) = &mut this.a  { if Ready(v) = Pin::new(fut).poll(cx) {
//         this.a_out = Some(v); this.a = None; } }
//   3. same for b.
//   4. if both a_out and b_out are Some, .take() both and return Ready((oa, ob));
//      else return Pending.
struct Join<A: Future, B: Future> {
    a: Option<A>,
    a_out: Option<A::Output>,
    b: Option<B>,
    b_out: Option<B::Output>,
}

fn join<A: Future, B: Future>(a: A, b: B) -> Join<A, B> {
    Join {
        a: Some(a),
        a_out: None,
        b: Some(b),
        b_out: None,
    }
}

impl<A, B> Future for Join<A, B>
where
    A: Future + Unpin,
    B: Future + Unpin,
    A::Output: Unpin,
    B::Output: Unpin,
{
    type Output = (A::Output, B::Output);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Some(a) = &mut this.a {
            if let Poll::Ready(a) = Pin::new(a).poll(cx) {
                this.a_out = Some(a);
                this.a = None;
            }
        }
        if let Some(b) = &mut this.b {
            if let Poll::Ready(b) = Pin::new(b).poll(cx) {
                this.b_out = Some(b);
                this.b = None;
            }
        }

        // Only take once BOTH are present. Taking one early discards it forever
        // (the finished side's future is already None), so block_on spins forever.
        if this.a_out.is_some() && this.b_out.is_some() {
            return Poll::Ready((this.a_out.take().unwrap(), this.b_out.take().unwrap()));
        }
        Poll::Pending
    }
}

fn check_8() {
    // Two independent futures; join completes when both do.
    let out = block_on(join(ready(1), ready(2)));
    assert_eq!(out, (1, 2));

    // Concurrency proof: each side pends once. A sequential runner would need
    // more polls; join drives both per poll, so it finishes in exactly 2 polls
    // (poll #1: both go Pending; poll #2: both go Ready).
    let mut fut = join(fut_a(), fut_b()); // FutA -> 20, FutB -> 22
    let mut fut = Pin::new(&mut fut);
    let mut cx = noop_cx();
    assert!(
        fut.as_mut().poll(&mut cx).is_pending(),
        "poll #1: both pend"
    );
    assert_eq!(
        fut.as_mut().poll(&mut cx),
        Poll::Ready((20, 22)),
        "poll #2: both ready"
    );

    // Mixed readiness: one side ready immediately, the other pends once. join
    // must stash the ready value and keep polling only the slow side.
    let out = block_on(join(ready(99), fut_b()));
    assert_eq!(out, (99, 22));
    println!("check_8 ok: join drives both concurrently, done when both Ready");
}

// ---------------------------------------------------------------------------
// Problem 9 [capstone]: Desugar an async fn (2 awaits + a BRANCH) end-to-end,
// and drive it with a proper waker-based executor.
//
// Everything you've built, together. The async fn we're desugaring:
//
//     async fn compute(n: i32) -> i32 {
//         let a = wake_yield(n).await;        // await #1 — ALWAYS happens
//         if a % 2 == 0 {
//             let b = wake_yield(a * 2).await; // await #2 — ONLY on the even branch
//             a + b
//         } else {
//             a                                // odd branch: no second await
//         }
//     }
//
// The branch means the number of suspension points depends on a RUNTIME value —
// so the generated state machine has a variant that only some executions reach.
// Your `BranchMachine` enum encodes exactly that.
//
// You implement THREE things (each fuses earlier rungs):
//   (a) `run` — a real executor: Box::pin (accepts !Unpin async futures, rung 7)
//       + the FlagWaker parking loop (rung 2 + rung 6). Re-poll only when woken.
//   (b) `BranchMachine::poll` — the hand-written state machine WITH the branch
//       (rung 5 + a conditional transition).
//   (c) `compute` — the async fn body, so we can prove hand == compiler.

// A proper leaf: pends once, REGISTERS THE WAKER (so `run` re-polls it), then Ready.
struct WakeYield {
    value: i32,
    yielded: bool,
}
fn wake_yield(value: i32) -> WakeYield {
    WakeYield {
        value,
        yielded: false,
    }
}
impl Future for WakeYield {
    type Output = i32;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
        let this = self.get_mut();
        if this.yielded {
            Poll::Ready(this.value)
        } else {
            this.yielded = true;
            cx.waker().wake_by_ref(); // contract: arrange to be re-polled
            Poll::Pending
        }
    }
}

// The state machine `compute` desugars to. Each variant = a suspension point;
// the branch is decided when Await1 completes.
enum BranchMachine {
    Await1 { fut1: WakeYield },         // awaiting the first value
    Await2 { fut2: WakeYield, a: i32 }, // even branch only: awaiting second, carrying a
    Done,
}
fn compute_machine(n: i32) -> BranchMachine {
    BranchMachine::Await1 {
        fut1: wake_yield(n),
    }
}
impl Future for BranchMachine {
    type Output = i32;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        loop {
            match this {
                BranchMachine::Await1 { fut1 } => match Pin::new(fut1).poll(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(a) => {
                        if a % 2 == 0 {
                            let await2 = BranchMachine::Await2 {
                                fut2: wake_yield(a * 2),
                                a,
                            };
                            let _ = std::mem::replace(this, await2);
                            continue;
                        } else {
                            let done = BranchMachine::Done;
                            let _ = std::mem::replace(this, done);
                            return Poll::Ready(a);
                        }
                    }
                },
                BranchMachine::Await2 { fut2, a } => match Pin::new(fut2).poll(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(b) => {
                        let a = *a;
                        let done = BranchMachine::Done;
                        let _ = std::mem::replace(this, done);
                        return Poll::Ready(a + b);
                    }
                },
                BranchMachine::Done => panic!("BranchMachine polled after completion"),
            }
        }
    }
}

// (c) The async equivalent — fill in the body to match compute_machine's logic.
async fn compute(n: i32) -> i32 {
    let a = wake_yield(n).await;
    if a % 2 == 0 {
        let b = wake_yield(a * 2).await;
        a + b
    } else {
        a
    }
}

// (a) A real executor: parks on Pending, re-polls only when the waker fires.
// Box::pin so it accepts !Unpin async futures (like `compute`). Reuses FlagWaker.
fn run<F: Future>(fut: F) -> F::Output {
    let flag = Arc::new(FlagWaker {
        woken: AtomicBool::new(true), // allow the first poll
    });
    let waker = Waker::from(flag.clone());
    let mut cx = Context::from_waker(&waker);
    let mut fut = Box::pin(fut);
    loop {
        // Clear the flag before polling. After Pending, we only re-enter if
        // wake()/wake_by_ref() set it again — otherwise we'd spin forever.
        if !flag.woken.swap(false, Ordering::SeqCst) {
            panic!("stuck: future returned Pending without registering a wake");
        }
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => {}
        }
    }
}

fn check_9() {
    // Even branch (two awaits): a = n, b = 2n, result = 3n.
    assert_eq!(run(compute_machine(4)), 12, "machine, even: 4 + 8");
    assert_eq!(run(compute(4)), 12, "async, even: 4 + 8");

    // Odd branch (one await): result = n.
    assert_eq!(run(compute_machine(5)), 5, "machine, odd: 5");
    assert_eq!(run(compute(5)), 5, "async, odd: 5");

    // The whole thesis: hand-written state machine == compiler-generated async,
    // across BOTH branches, driven by a real waker-based executor.
    for n in 0..20 {
        assert_eq!(
            run(compute_machine(n)),
            run(compute(n)),
            "hand-rolled desugaring must equal `async fn` for n={n}"
        );
    }
    println!(
        "check_9 ok: async fn (2 awaits + branch) == hand-rolled state machine, driven by a waker executor"
    );
    println!("\n  *** future_poll ladder complete — you can read `.await` as an enum now. ***");
}

fn main() {
    check_1();
    check_2();
    check_3();
    check_4();
    check_5();
    check_6();
    check_7();
    check_8();
    check_9();
    // check_9();
}
