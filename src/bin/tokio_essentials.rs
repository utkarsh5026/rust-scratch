//! tokio essentials — tasks, spawn, join!, select!, spawn_blocking
//!
//! Run:  cargo run --bin tokio_essentials
//!
//! Mental model: a Future does NOTHING until polled. tokio is the executor
//! that polls futures to completion. `.await` polls one future on the CURRENT
//! task; `tokio::spawn` hands a future to the runtime as a NEW task. The rest
//! is how you compose many futures onto tasks — and the golden rule: never
//! block a worker thread, because a few threads poll thousands of tasks.
//!
//! Ladder (DONE marked):
//!   1. [x] foundations  — #[tokio::main] + first async fn + .await a sleep
//!   2. [x] foundations  — tokio::spawn -> JoinHandle; N tasks run concurrently
//!   3. [x] mechanics    — tokio::join! runs futures concurrently on one task
//!   4. [x] mechanics    — tokio::select! races; losers are dropped (cancel)
//!   5. [x] footgun      — 'static + Send wall; JoinHandle -> Result<T, JoinError>
//!   6. [x] footgun      — blocking the executor starves other tasks
//!   7. [x] real-world   — spawn_blocking + an mpsc channel between tasks
//!   8. [x] real-world   — time::{sleep,timeout} + a select! loop w/ shutdown
//!   9. [x] capstone     — a concurrent worker pool / job scheduler

use std::time::Instant;

// ─────────────────────────────────────────────────────────────────────────
// Problem 1 (foundations): your first async fn.
//
// Implement `greet_after` so that it:
//   - sleeps asynchronously for `ms` milliseconds (tokio::time::sleep + .await),
//   - then returns the String  "hello, <name>"  (e.g. "hello, tokio").
//
// The point: `.await` yields control back to the runtime while sleeping,
// instead of blocking the thread. check_1 asserts the return value AND that
// the wait actually happened (elapsed >= the requested time).
// ─────────────────────────────────────────────────────────────────────────
async fn greet_after(name: &str, ms: u64) -> String {
    tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
    format!("hello, {name}")
}

async fn check_1() {
    let start = Instant::now();
    let msg = greet_after("tokio", 50).await;
    let elapsed = start.elapsed();

    assert_eq!(msg, "hello, tokio", "greet_after returned the wrong string");
    assert!(
        elapsed >= tokio::time::Duration::from_millis(50),
        "expected to actually wait ~50ms, only waited {elapsed:?}"
    );
    println!("check_1 ✅  got {msg:?} after {elapsed:?}");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 2 (foundations): tokio::spawn -> JoinHandle, and real concurrency.
//
// `tokio::spawn(future)` hands the future to the runtime as an independent
// TASK and immediately returns a `JoinHandle<T>`. Awaiting the handle waits
// for the task to finish and gives you its return value.
//
// Implement `run_concurrently(n, ms)`:
//   - spawn `n` tasks; task i sleeps `ms` millis, then returns i * 10 (usize).
//   - collect every JoinHandle, THEN await them all, summing the results.
//   - return the sum:  0*10 + 1*10 + ... + (n-1)*10.
//
// The lesson is in the timing: because all n tasks run concurrently, the whole
// thing takes ~ms total, NOT n*ms. So spawn FIRST into a Vec, await SECOND.
// (If you spawn-and-await in the same loop iteration you serialize them —
// that's the trap this rung makes you avoid.)
//
// Note: JoinHandle::await returns Result<T, JoinError>; unwrap it for now.
// ─────────────────────────────────────────────────────────────────────────
async fn run_concurrently(n: usize, ms: u64) -> usize {
    let mut handles = Vec::with_capacity(n);
    for i in 0..n {
        handles.push(tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
            i * 10
        }));
    }
    let mut sum = 0;
    for handle in handles {
        sum += handle.await.unwrap();
    }
    sum
}

async fn check_2() {
    let start = Instant::now();
    let sum = run_concurrently(10, 50).await;
    let elapsed = start.elapsed();

    assert_eq!(
        sum, 450,
        "sum of i*10 for i in 0..10 should be 450, got {sum}"
    );
    assert!(
        elapsed < tokio::time::Duration::from_millis(250),
        "10 tasks of 50ms should run CONCURRENTLY (~50ms), but took {elapsed:?} \
         — did you await each handle in the spawn loop instead of after it?"
    );
    println!("check_2 ✅  sum={sum}, 10×50ms tasks finished in {elapsed:?}");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 3 (mechanics): tokio::join! — concurrency WITHOUT spawning.
//
// spawn moves work onto (possibly) other tasks/threads. join! is different:
// it drives several futures concurrently ON THE CURRENT TASK, polling each
// whenever it's ready, and returns a TUPLE of all their outputs once ALL
// finish. No new task, no JoinHandle, no Send/'static requirement, no unwrap.
//
// Think of it as: `let (a, b, c) = join!(fut_a, fut_b, fut_c);`
//
// Implement `fetch_all`. You're given three async "fetches" (provided below).
//   - Run all three CONCURRENTLY with tokio::join! (do NOT await them one by
//     one — that would be sequential and slow).
//   - Return their results as a tuple (u32, u32, u32) in order.
//
// check_3 asserts both the values and that it finished in ~one delay, not three.
// ─────────────────────────────────────────────────────────────────────────
async fn fetch_num(id: u32, ms: u64) -> u32 {
    tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
    id * 100
}

async fn fetch_all() -> (u32, u32, u32) {
    tokio::join!(fetch_num(1, 60), fetch_num(2, 60), fetch_num(3, 60))
}

async fn check_3() {
    let start = Instant::now();
    let got = fetch_all().await;
    let elapsed = start.elapsed();

    assert_eq!(got, (100, 200, 300), "fetch_all returned {got:?}");
    assert!(
        elapsed < tokio::time::Duration::from_millis(150),
        "three 60ms fetches under join! should overlap (~60ms), took {elapsed:?} \
         — did you .await them sequentially instead of join!-ing?"
    );
    println!("check_3 ✅  {got:?} in {elapsed:?} (concurrent, no spawn)");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 4 (mechanics): tokio::select! — race futures, cancel the losers.
//
// join! waits for ALL. select! waits for the FIRST to finish, runs that arm's
// handler, and then DROPS the other futures right where they were suspended.
// That drop IS cancellation — the losing future never runs to completion.
//
// select! shape:
//     tokio::select! {
//         val = fut_a => { /* fut_a won */ }
//         val = fut_b => { /* fut_b won */ }
//     }
//
// `slow_task` below increments a shared counter ONLY if it runs to completion.
// It also holds a `DropSpy` that sets `cancelled=true` when the future is
// dropped before finishing. You will race a fast future against slow_task.
//
// Implement `race(fast_ms, slow_ms) -> &'static str`:
//   - select! between:
//       * a sleep of `fast_ms` (the "fast" branch) -> yield  "fast won"
//       * slow_task(slow_ms, counter, cancelled)   -> yield  "slow won"
//   - return whichever &str the winning arm produces.
//
// check_4 calls it with fast=30, slow=200: the fast branch wins, slow_task is
// dropped mid-sleep, so the counter stays 0 and `cancelled` flips to true.
// ─────────────────────────────────────────────────────────────────────────
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

struct DropSpy(Arc<AtomicBool>);
impl Drop for DropSpy {
    fn drop(&mut self) {
        // Fires when the future holding this spy is dropped (i.e. cancelled).
        self.0.store(true, Ordering::SeqCst);
    }
}

async fn slow_task(ms: u64, counter: Arc<AtomicU32>, cancelled: Arc<AtomicBool>) -> &'static str {
    let _spy = DropSpy(cancelled); // dropped if we're cancelled before finishing
    tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
    counter.fetch_add(1, Ordering::SeqCst); // only reached if we finish
    "slow won"
}

async fn race(
    fast_ms: u64,
    slow_ms: u64,
    counter: Arc<AtomicU32>,
    cancelled: Arc<AtomicBool>,
) -> &'static str {
    tokio::select! {
        _ = tokio::time::sleep(tokio::time::Duration::from_millis(fast_ms)) => {
            "fast won"
        }
        slow_result = slow_task(slow_ms, counter, cancelled) => {
            slow_result
        }
    }
}

async fn check_4() {
    let counter = Arc::new(AtomicU32::new(0));
    let cancelled = Arc::new(AtomicBool::new(false));

    let who = race(30, 200, Arc::clone(&counter), Arc::clone(&cancelled)).await;

    assert_eq!(who, "fast won", "the 30ms branch should beat the 200ms one");
    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "slow_task must NOT have completed — its counter increment should never run"
    );
    assert!(
        cancelled.load(Ordering::SeqCst),
        "slow_task's future should have been DROPPED (cancelled) when fast won"
    );
    println!("check_4 ✅  {who:?}; slow_task cancelled mid-flight (counter=0)");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 5 (footgun): the 'static + Send wall, and JoinHandle -> Result.
//
// tokio::spawn requires the future be  Send + 'static  — the runtime may move
// it to another worker thread and run it for an unknown duration, so it can't
// borrow anything from the current stack frame, and everything it touches must
// be safe to send across threads.
//
// (A) THE WALL — read, then experiment. Uncomment the block below, run, read
//     the two compiler errors, then re-comment it so the file builds again.
//     This is the error that DEFINES why we clone Arcs into `async move` tasks.
//
//   // let local = String::from("on my stack");
//   // tokio::spawn(async {                 // borrows `local` -> E0373: may outlive
//   //     println!("{local}");             //   the current function
//   // });
//   // let rc = std::rc::Rc::new(1);
//   // tokio::spawn(async move {            // Rc is !Send -> "future cannot be sent
//   //     tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
//   //     println!("{}", rc);              //   between threads safely"
//   // });
//
// (B) CATCHING A PANIC. A panicking task does NOT abort the process — the panic
//     is captured and surfaced when you await the handle: JoinHandle::await
//     returns Result<T, JoinError>, and JoinError::is_panic() tells you why.
//
// Implement `spawn_and_catch(should_panic) -> Result<i32, String>`:
//   - spawn a task that: panics with "boom" if should_panic, else returns 42.
//   - await the handle. On Ok(v) return Ok(v). On Err(e):
//       * if e.is_panic() -> Err("task panicked".to_string())
//       * else            -> Err("task cancelled".to_string())
//
// (You'll see "boom" panic output on stderr for the panic case — that's the
//  runtime logging the caught panic, not a crash. Expected.)
// ─────────────────────────────────────────────────────────────────────────
async fn spawn_and_catch(should_panic: bool) -> Result<i32, String> {
    let handle = tokio::spawn(async move {
        if should_panic {
            panic!("boom");
        }
        42
    });

    let result = handle.await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            if e.is_panic() {
                Err("task panicked".to_string())
            } else {
                Err("task cancelled".to_string())
            }
        }
    }
}

async fn check_5() {
    let ok = spawn_and_catch(false).await;
    assert_eq!(ok, Ok(42), "a clean task should yield Ok(42), got {ok:?}");

    let boom = spawn_and_catch(true).await;
    assert_eq!(
        boom,
        Err("task panicked".to_string()),
        "a panicking task should be caught as Err via JoinError::is_panic(), got {boom:?}"
    );
    println!("check_5 ✅  clean task -> {ok:?}; panicking task caught -> {boom:?}");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 6 (footgun): blocking the executor starves other tasks.
//
// A tokio worker thread runs many tasks COOPERATIVELY: a task only lets others
// run when it hits an `.await` that returns Pending. If a task instead does
// synchronous blocking work (std::thread::sleep, a big CPU loop, a blocking
// file/DB call) with NO await points, it OWNS that worker thread the whole
// time — and every other task pinned to that thread is FROZEN.
//
// We prove it on a single-threaded runtime (one worker). A 20ms timer task is
// spawned; then `hog` runs for ~200ms. We measure when the timer actually
// fires (ms since start):
//   - if hog BLOCKS the thread, the timer can't even be polled until hog is
//     done -> it fires ~200ms+ late.
//   - if hog is COOPERATIVE (yields), the timer fires at ~20ms as intended.
//
// Implement `hog(block, dur_ms)` so it stays busy for ~dur_ms, two ways:
//   - block == true : do BLOCKING work with NO .await — e.g.
//                     std::thread::sleep(Duration::from_millis(dur_ms));
//                     (or a tight `while start.elapsed() < dur {}` spin-loop).
//                     This is the footgun: it hijacks the worker thread.
//   - block == false: stay busy but COOPERATE — loop until dur_ms has elapsed,
//                     calling `tokio::task::yield_now().await` each iteration so
//                     the runtime can poll other tasks in between.
// ─────────────────────────────────────────────────────────────────────────
async fn hog(block: bool, dur_ms: u64) {
    if block {
        std::thread::sleep(std::time::Duration::from_millis(dur_ms));
    } else {
        let start = Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(dur_ms) {
            tokio::task::yield_now().await;
        }
    }
}

// Provided plumbing: runs the scenario on its OWN single-threaded runtime in a
// separate OS thread (so it doesn't nest inside #[tokio::main]'s runtime), and
// returns the ms-since-start at which the 20ms timer fired.
fn timer_lateness(block: bool) -> u128 {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let start = Instant::now();
            let timer = tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
                start.elapsed().as_millis()
            });
            hog(block, 200).await;
            timer.await.unwrap()
        })
    })
    .join()
    .unwrap()
}

fn check_6() {
    let blocked = timer_lateness(true);
    let cooperative = timer_lateness(false);

    assert!(
        blocked > 150,
        "blocking hog should STARVE the 20ms timer (~200ms late), but it fired at {blocked}ms \
         — did your block==true path still contain an .await?"
    );
    assert!(
        cooperative < 80,
        "cooperative hog should let the 20ms timer fire on time (<80ms), but it fired at {cooperative}ms \
         — did your block==false path forget to yield_now().await?"
    );
    println!(
        "check_6 ✅  blocking hog starved timer ({blocked}ms late) vs cooperative ({cooperative}ms)"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 7 (real-world): spawn_blocking + an mpsc channel between tasks.
//
// Rung 6 showed blocking work freezes a worker. The fix isn't to rewrite the
// blocking code — it's to move it OFF the async worker threads onto tokio's
// dedicated BLOCKING thread pool:
//
//     let handle = tokio::task::spawn_blocking(move || { /* sync blocking work */ });
//     let result = handle.await.unwrap();   // JoinHandle again — same as spawn
//
// spawn_blocking runs the closure on a separate pool sized for blocking work,
// so the async workers keep polling other tasks. You get a JoinHandle back and
// await it just like spawn.
//
// You'll also wire two tasks together with an async channel:
// tokio::sync::mpsc — a producer task SENDS items, a consumer RECEIVES them.
//   - let (tx, mut rx) = tokio::sync::mpsc::channel::<T>(capacity);
//   - producer:  tx.send(item).await.unwrap();   (async — applies backpressure)
//   - consumer:  while let Some(item) = rx.recv().await { ... }
//                recv() returns None once ALL senders are dropped (EOF).
//
// Implement `pipeline(inputs) -> Vec<u64>`:
//   - Create an mpsc channel (pick a small capacity, e.g. 8).
//   - Spawn a PRODUCER task that, for each n in `inputs`, computes n*n via
//     `spawn_blocking(move || slow_square(n))` (a deliberately blocking fn,
//     provided), awaits it, and sends the result down the channel. Drop the
//     sender when done so the consumer sees EOF.
//   - In the CURRENT task, be the CONSUMER: recv() in a loop, collecting every
//     value into a Vec, until recv() returns None. Return that Vec.
//
// Result should be each input squared, in the order they were produced.
// (inputs [2,3,4,5] -> [4,9,16,25].)
// ─────────────────────────────────────────────────────────────────────────
fn slow_square(n: u64) -> u64 {
    std::thread::sleep(std::time::Duration::from_millis(10));
    n * n
}

async fn pipeline(inputs: Vec<u64>) -> Vec<u64> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<u64>(8);
    let producer = tokio::spawn(async move {
        for n in inputs {
            tx.send(slow_square(n)).await.unwrap();
        }
        drop(tx);
    });
    let mut results = Vec::new();
    while let Some(result) = rx.recv().await {
        results.push(result);
    }
    producer.await.unwrap();
    results
}

async fn check_7() {
    let out = pipeline(vec![2, 3, 4, 5]).await;
    assert_eq!(
        out,
        vec![4, 9, 16, 25],
        "pipeline should square each input in order via spawn_blocking + mpsc"
    );
    println!("check_7 ✅  spawn_blocking + mpsc pipeline -> {out:?}");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 8 (real-world): a select! event loop — timers, timeout, shutdown.
//
// The canonical shape of a long-running tokio component is a loop that
// select!s over several event sources every iteration: "do periodic work",
// "handle an incoming message", "notice a shutdown signal". Whichever fires
// first wins that turn; the loop goes around again.
//
// Two time tools you'll use:
//   - tokio::time::interval(period): a ticker. `interval.tick().await` resolves
//     every `period` (the FIRST tick fires immediately).
//   - tokio::time::timeout(dur, fut): wraps a future; resolves Ok(v) if `fut`
//     finished in time, or Err(Elapsed) if it took too long.
//
// Also note `biased;` at the top of a select!: without it, select! picks a
// READY branch at random (fairness); with `biased;` it checks arms top-to-
// bottom in order. We want shutdown checked FIRST, so put `biased;` and the
// shutdown arm on top.
//
// Implement `worker(mut shutdown) -> u32`:
//   - Create an interval of 20ms.
//   - Loop, each turn select!-ing (biased, shutdown first) between:
//       * `shutdown.recv()` (a tokio::sync::mpsc::Receiver<()>): on Some(())
//         OR None (sender dropped), BREAK out of the loop.
//       * `interval.tick()`: increment a local `ticks` counter.
//   - Return the number of ticks completed before shutdown.
//
// check_8 spawns worker, lets it tick for ~110ms, then sends the shutdown
// signal. It should have ticked several times (>=3) and returned promptly.
// ─────────────────────────────────────────────────────────────────────────
async fn worker(mut shutdown: tokio::sync::mpsc::Receiver<()>) -> u32 {
    let mut ticks = 0;
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(20));
    loop {
        tokio::select! {
            biased;
            _ = shutdown.recv() => break,
            _ = interval.tick() => ticks += 1,
        }
    }
    ticks
}

async fn check_8() {
    let (tx, rx) = tokio::sync::mpsc::channel::<()>(1);
    let handle = tokio::spawn(worker(rx));

    // Let it tick for a while, then ask it to stop.
    tokio::time::sleep(tokio::time::Duration::from_millis(110)).await;
    tx.send(()).await.unwrap();

    // The worker should shut down promptly; guard with a timeout so a bug can't
    // hang the test forever.
    let ticks = tokio::time::timeout(tokio::time::Duration::from_millis(200), handle)
        .await
        .expect("worker did not shut down within 200ms — is your shutdown arm breaking the loop?")
        .expect("worker task panicked");

    assert!(
        ticks >= 3,
        "worker ran ~110ms with a 20ms interval; expected >=3 ticks, got {ticks}"
    );
    println!("check_8 ✅  select! loop ticked {ticks}× then shut down on signal");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 9 (CAPSTONE): a concurrent worker pool / job scheduler.
//
// Build a fixed pool of `num_workers` async workers that drain a shared queue
// of jobs, doing the heavy per-job compute on the BLOCKING pool, and stream
// results back — proving you own every primitive from this ladder at once:
//
//   • tokio::spawn            — one long-lived task per worker
//   • Arc<tokio::sync::Mutex<mpsc::Receiver<Job>>>
//                             — SINGLE-consumer channel shared by N workers,
//                               so the Mutex hands the receiver around (this is
//                               THE idiom for a multi-consumer job queue on
//                               top of tokio's mpsc)
//   • spawn_blocking          — the CPU-heavy `heavy_square` goes off-worker
//   • a results mpsc          — workers send (id, result) back to main
//   • EOF-driven shutdown     — dropping the last sender ends each recv loop
//
// Implement `run_pool(num_workers, jobs) -> Vec<(u64, u64)>`:
//   Returns (job.id, job.n squared) for every job, SORTED BY id.
//
//   1. Make a jobs channel and a results channel (mpsc, small capacity).
//      Wrap the jobs RECEIVER in Arc<tokio::sync::Mutex<..>> so workers share it.
//   2. Spawn `num_workers` worker tasks. Each worker loops:
//        - lock the shared receiver, `recv().await`, then RELEASE THE LOCK
//          IMMEDIATELY (drop the guard) — do NOT hold it across the compute,
//          or your workers serialize and lose all concurrency.
//        - None  => the queue is drained, break.
//        - Some(job) => let sq = spawn_blocking(move || heavy_square(job.n))
//                                 .await.unwrap();
//                       send (job.id, sq) on the results channel.
//   3. Dispatcher: send every job into the jobs channel, then DROP the jobs
//      sender so workers hit EOF. Also DROP main's own clone of the results
//      sender, so once all workers finish, the results receiver hits EOF too.
//   4. Collect results with `while let Some(r) = results_rx.recv().await`,
//      sort by id, and return.
//
// check_9 runs 4 workers over 8 jobs. heavy_square sleeps 20ms each, so:
//   - correctness: every job squared exactly once, none dropped/duplicated.
//   - CONCURRENCY: 8 jobs / 4 workers = ~2 rounds ≈ ~40ms. If you held the lock
//     across the compute (serializing to one worker) it'd be ~160ms — the
//     timing assert will catch that.
// ─────────────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, Debug)]
struct Job {
    id: u64,
    n: u64,
}

fn heavy_square(n: u64) -> u64 {
    // Stand-in for real blocking CPU/IO work — belongs on spawn_blocking.
    std::thread::sleep(std::time::Duration::from_millis(20));
    n * n
}

async fn run_pool(num_workers: usize, jobs: Vec<Job>) -> Vec<(u64, u64)> {
    let (jobs_tx, jobs_rx) = tokio::sync::mpsc::channel::<Job>(8);
    let (results_tx, mut results_rx) = tokio::sync::mpsc::channel::<(u64, u64)>(8);
    let jobs_receiver = Arc::new(tokio::sync::Mutex::new(jobs_rx));
    for _ in 0..num_workers {
        let jobs_receiver = jobs_receiver.clone();
        let results_tx = results_tx.clone();
        tokio::spawn(async move {
            loop {
                let job = {
                    let mut rx = jobs_receiver.lock().await;
                    rx.recv().await
                }; // MutexGuard dropped here — before compute
                let Some(job) = job else { break };
                let result = tokio::task::spawn_blocking(move || heavy_square(job.n))
                    .await
                    .unwrap();
                results_tx.send((job.id, result)).await.unwrap();
            }
        });
    }
    for job in jobs {
        jobs_tx.send(job).await.unwrap();
    }
    drop(jobs_tx);
    drop(results_tx); // must drop before collect — else recv never sees EOF
    let mut results = Vec::new();
    while let Some(result) = results_rx.recv().await {
        results.push(result);
    }
    results.sort_by_key(|(id, _)| *id);
    results
}

async fn check_9() {
    let jobs: Vec<Job> = (1..=8).map(|n| Job { id: n, n }).collect();
    let expected: Vec<(u64, u64)> = (1..=8).map(|n| (n, n * n)).collect();

    let start = Instant::now();
    let got = run_pool(4, jobs).await;
    let elapsed = start.elapsed();

    assert_eq!(
        got, expected,
        "each job should be squared exactly once, sorted by id"
    );
    assert!(
        elapsed < tokio::time::Duration::from_millis(120),
        "4 workers over 8×20ms jobs should finish in ~40ms; took {elapsed:?} \
         — are you holding the receiver lock across the compute (serializing them)?"
    );
    println!("check_9 ✅  worker pool squared 8 jobs across 4 workers in {elapsed:?} -> {got:?}");
}

#[tokio::main]
async fn main() {
    check_1().await;
    check_2().await;
    check_3().await;
    check_4().await;
    check_5().await;
    check_6();
    check_7().await;
    check_8().await;
    check_9().await;
    println!("\nall wired-up checks passed 🎉");
}
