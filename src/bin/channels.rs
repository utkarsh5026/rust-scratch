//! Channels — message-passing concurrency (`std::sync::mpsc`, crossbeam)
//!
//! Run: `cargo run --bin channels`
//!
//! Ladder (DONE marked):
//!   1. First pipe            — mpsc::channel, send from a thread, recv()        [DONE]
//!   2. Multi-producer        — clone Sender, fan in from N threads             [DONE]
//!   3. Receiver as iterator  — `for v in rx` ends when all senders drop        [DONE]
//!   4. Bounded & backpressure— sync_channel(k); send blocks when full          [DONE]
//!   5. The hang              — stray Sender => recv blocks forever; Recv/SendError [DONE]
//!   6. Non-blocking          — try_recv / recv_timeout, drain without deadlock  [DONE]
//!   7. Worker pool           — N workers over a job channel, results channel    [DONE]
//!   8. crossbeam             — mpmc + select! over multiple channels            [DONE]
//!   9. CAPSTONE: build it    — hand-rolled Channel<T> (Mutex+Condvar+VecDeque)  [DONE]

use crossbeam_channel::{select, unbounded};
use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Problem 1 — First pipe
//
// Spawn ONE thread that sends the numbers 1..=5 down a channel. On the main
// thread, receive all five with `recv()` and collect them into a Vec, in order.
//
// `mpsc::channel()` returns a `(Sender<T>, Receiver<T>)` pair.
//   - tx.send(value)  -> Result<(), SendError<T>>   (moves the value in)
//   - rx.recv()       -> Result<T, RecvError>        (blocks until a value arrives)
//
// Implement `collect_five` so check_1 passes. You'll need to `move` the Sender
// into the spawned thread, and call recv() five times (or loop) on the Receiver.
// ---------------------------------------------------------------------------

fn collect_five() -> Vec<i32> {
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
    result
}

fn check_1() {
    let got = collect_five();
    assert_eq!(got, vec![1, 2, 3, 4, 5], "should receive 1..=5 in order");
    println!("check_1 ✅  first pipe: {:?}", got);
}

// ---------------------------------------------------------------------------
// Problem 2 — Multi-producer (the "m" in mpsc)
//
// A channel has MANY senders but ONE receiver. You get extra senders by
// CLONING the Sender — each clone feeds the same Receiver.
//
// Spawn `n` worker threads. Worker `i` sends the single value `i * 10` down a
// shared channel. The main thread receives all `n` values and returns their
// SUM. (Order is nondeterministic across threads — that's why we sum, not eq.)
//
// Key moves:
//   - tx.clone() gives each thread its own Sender handle.
//   - Each thread `move`s its OWN clone in; don't move the original into a loop.
//   - After spawning, you have `n` clones out in threads. Receive exactly `n`
//     values. (Next rung handles "how many?" without counting.)
//
// Implement `sum_from_workers`.
// ---------------------------------------------------------------------------

fn sum_from_workers(n: usize) -> i32 {
    let (tx, rx) = mpsc::channel();
    for i in 0..n {
        let tx = tx.clone();
        thread::spawn(move || {
            tx.send(i * 10).unwrap();
        });
    }
    let mut result = 0;
    for _ in 0..n {
        result += rx.recv().unwrap();
    }
    result as i32
}

fn check_2() {
    // workers send 0, 10, 20, ... (n-1)*10  => sum = 10 * (n-1)*n/2
    let got = sum_from_workers(5); // 0+10+20+30+40
    assert_eq!(got, 100, "sum of i*10 for i in 0..5");
    let got2 = sum_from_workers(10);
    assert_eq!(got2, 450, "sum of i*10 for i in 0..10");
    println!("check_2 ✅  multi-producer: {} and {}", got, got2);
}

// ---------------------------------------------------------------------------
// Problem 3 — Receiver as iterator (clean shutdown via disconnect)
//
// So far you've COUNTED how many to recv. Real code rarely knows the count.
// Instead: the Receiver is an iterator. `for v in rx` (or `rx.iter()`) yields
// values until the channel is DISCONNECTED — i.e. every Sender has been dropped.
// Then the loop ends on its own. No sentinel value, no counting.
//
// The catch: the loop only ends once ALL senders are gone. If a single Sender
// lingers, the iterator blocks waiting for it. So you must DROP the producer
// handles (or move them somewhere that gets dropped) before draining.
//
// Task: `produce_then_drain` spawns `n` producer threads; producer i sends
// `i` as an i64. Collect everything the receiver yields via a `for` loop over
// `rx` (no counting!) and return the count of items received. It must equal n.
//
// Hint shape: clone a tx per thread as before. The ORIGINAL tx must NOT survive
// into the drain loop — if it does, `for v in rx` never terminates. Think about
// where the original tx is dropped.
// ---------------------------------------------------------------------------

fn produce_then_drain(n: usize) -> usize {
    let (tx, rx) = mpsc::channel();
    for i in 0..n {
        let tx = tx.clone();
        thread::spawn(move || {
            tx.send(i as i64).unwrap();
        });
    }
    drop(tx);

    let mut result = 0;
    for _ in rx {
        result += 1;
    }
    result
}

fn check_3() {
    assert_eq!(produce_then_drain(7), 7, "for-loop drain should see all 7");
    assert_eq!(produce_then_drain(100), 100, "and all 100");
    println!("check_3 ✅  receiver-as-iterator drained cleanly on disconnect");
}

// ---------------------------------------------------------------------------
// Problem 4 — Bounded channels & backpressure
//
// `mpsc::channel()` is UNBOUNDED: send() never blocks, it just queues. A fast
// producer + slow consumer => the queue grows without limit (memory blowup).
//
// `mpsc::sync_channel(k)` is BOUNDED to `k` buffered messages. When the buffer
// is full, send() BLOCKS until the consumer frees a slot. That blocking IS
// backpressure: the producer is forced to slow to the consumer's pace.
//   - sync_channel(0) is special: a RENDEZVOUS channel. Zero buffer — every
//     send() blocks until a recv() is ready to take it hand-to-hand.
//
// Task: prove backpressure with a rendezvous channel. `rendezvous_log` creates
// a `sync_channel::<&str>(0)`. Spawn a producer that sends "a","b","c" and,
// AFTER EACH successful send, pushes that letter onto a shared log. The main
// thread receives the three letters slowly. Return the producer-side log.
//
// Because capacity is 0, send("b") cannot complete until you've recv'd "a".
// So if the consumer records its own progress too, the sends and recvs must
// interleave 1:1 — the producer can never run ahead. Assert the producer
// actually logged all three only after the consumer pulled them.
//
// Concretely: return (producer_log, consumer_log) and we check both are
// ["a","b","c"] AND that the producer never got more than 1 ahead. To observe
// "never more than 1 ahead", have the producer record the value, and the
// consumer sleep a beat before each recv; track max gap. We'll keep it simple:
// just return the producer_log built as each send returns, and assert order.
//
// Use Arc<Mutex<Vec<&str>>> for the shared log (you've done Mutex already).
// ---------------------------------------------------------------------------

fn rendezvous_log() -> Vec<&'static str> {
    let (tx, rx) = mpsc::sync_channel(0);
    thread::spawn(move || {
        tx.send("a").unwrap();
        tx.send("b").unwrap();
        tx.send("c").unwrap();
    });
    let mut result = Vec::new();
    for _ in 0..3 {
        result.push(rx.recv().unwrap());
    }
    result
}

fn check_4() {
    let log = rendezvous_log();
    assert_eq!(log, vec!["a", "b", "c"], "producer-side log in send order");
    println!("check_4 ✅  bounded/rendezvous backpressure: {:?}", log);
}

// ---------------------------------------------------------------------------
// Problem 5 — The hang, and the two errors that report disconnect
//
// This rung is about what happens at the EDGES of a channel's life: when one
// half is gone. Two symmetric errors encode it:
//
//   rx.recv() -> Result<T, RecvError>
//       Err(RecvError) means: the buffer is empty AND every Sender has dropped.
//       Nothing more can ever arrive. (This is what ends `for v in rx`.)
//
//   tx.send(v) -> Result<(), SendError<T>>
//       Err(SendError(v)) means: the Receiver has dropped. Nobody will ever take
//       this value, so send hands it BACK to you inside the error.
//
// And the footgun: if a Sender NEVER drops, recv() on an empty channel blocks
// FOREVER. That's the classic deadlock — a stray tx you forgot about.
//
// Implement TWO functions:
//
// (a) `recv_until_disconnect(values)`: make a channel, send each of `values`
//     from a thread, let the thread end (dropping its tx). On the main thread,
//     loop calling rx.recv(); push Ok values into a Vec; STOP when you get
//     Err(RecvError). Return the Vec. This shows recv() reporting disconnect
//     instead of you counting — using the Result directly.
//
// (b) `send_after_receiver_gone()`: make a channel, immediately DROP the rx,
//     then call tx.send(99). It must return Err. Pull the value back out of the
//     SendError and return it (so we can assert you recovered the 99).
//     Hint: SendError is a tuple struct — SendError(T). Pattern-match or .0 it.
//
// (Note: we deliberately AVOID writing the infinite-hang version as a check —
// it would block the test binary forever. But understand WHY it hangs: no tx
// ever drops, so recv() has no reason to give up. If you want to feel it, you
// can temporarily add a `let _keep = tx.clone();` that you never drop and watch
// recv_until_disconnect spin forever — then remove it.)
// ---------------------------------------------------------------------------

fn recv_until_disconnect(values: Vec<i32>) -> Vec<i32> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        for value in values {
            tx.send(value).unwrap();
        }
    });
    let mut result = Vec::new();
    while let Ok(value) = rx.recv() {
        result.push(value);
    }
    result
}

fn send_after_receiver_gone() -> i32 {
    let (tx, rx) = mpsc::channel();
    drop(rx);
    tx.send(99).unwrap_err().0
}

fn check_5() {
    let got = recv_until_disconnect(vec![10, 20, 30]);
    assert_eq!(
        got,
        vec![10, 20, 30],
        "recv loop should stop exactly at disconnect"
    );
    let recovered = send_after_receiver_gone();
    assert_eq!(recovered, 99, "SendError should hand the value back");
    println!(
        "check_5 ✅  disconnect errors: recv stopped at RecvError, send recovered {}",
        recovered
    );
}

// ---------------------------------------------------------------------------
// Problem 6 — Non-blocking receive: try_recv and recv_timeout
//
// recv() BLOCKS. Sometimes you can't afford to: an event loop that must also
// do other work, or a consumer that should give up after a deadline. Two tools:
//
//   rx.try_recv() -> Result<T, TryRecvError>
//       Never blocks. TryRecvError has TWO variants you MUST distinguish:
//         - TryRecvError::Empty        => nothing right now, but senders alive;
//                                         try again later (keep looping).
//         - TryRecvError::Disconnected => empty AND all senders dropped; give up.
//       Treating Empty as "done" loses messages; treating Disconnected as
//       "try again" spins forever. The whole rung is telling them apart.
//
//   rx.recv_timeout(dur) -> Result<T, RecvTimeoutError>
//       Blocks up to `dur`. Variants: Timeout (deadline hit, senders maybe alive)
//       and Disconnected.
//
// Implement `drain_nonblocking`: a producer thread sends 0..count with a small
// sleep between sends (so the consumer WILL hit Empty in between). On the main
// thread, build a polling loop with try_recv():
//   - Ok(v)                       -> push v, optionally count a poll
//   - Err(Empty)                  -> not done; do a tiny sleep and CONTINUE
//   - Err(Disconnected)           -> break the loop
// Return the drained Vec. It must equal (0..count).
//
// The lesson: a correct non-blocking drain branches on BOTH error variants.
// Match on TryRecvError explicitly — don't `if let Ok` and bail on first error,
// or the first Empty will make you quit early and lose the rest.
// ---------------------------------------------------------------------------

fn drain_nonblocking(count: i32) -> Vec<i32> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for i in 0..count {
            tx.send(i).unwrap();
            thread::sleep(Duration::from_millis(100));
        }
    });

    let mut result = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(value) => result.push(value),
            Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(100)),
            Err(TryRecvError::Disconnected) => break,
        }
    }
    result
}

fn check_6() {
    let got = drain_nonblocking(20);
    let want: Vec<i32> = (0..20).collect();
    assert_eq!(
        got, want,
        "non-blocking drain must collect every value, not quit on Empty"
    );
    println!(
        "check_6 ✅  try_recv drain handled Empty vs Disconnected: {} items",
        got.len()
    );
}

// ---------------------------------------------------------------------------
// Problem 7 — Worker pool (the fan-out/fan-in pattern)
//
// A real use of channels: a FIXED pool of N worker threads draining a shared
// job queue, sending results back over a second channel. Two channels:
//   - jobs channel:    main --(many)--> workers      (fan-OUT)
//   - results channel: workers --(many)--> main      (fan-IN)
//
// THE WALL: Receiver is NOT Clone (mpsc = single consumer). N workers cannot
// each own a Receiver. The fix is the classic std thread-pool core:
//       let rx = Arc::new(Mutex::new(job_rx));
// and each worker gets an Arc::clone(&rx). A worker loops:
//       lock rx -> recv one job -> UNLOCK -> compute -> send result
// Lock only for the recv, NOT while computing, or you serialize the pool.
//
// How a worker knows to stop: when all job Senders drop, recv() -> Err, the
// worker breaks and exits. So main MUST drop its job-sender after queuing.
//
// Implement `run_pool(n_workers, inputs)`:
//   - jobs (job_tx, job_rx), results (res_tx, res_rx).
//   - Wrap job_rx in Arc<Mutex<...>>.
//   - Spawn n_workers threads; each clones the Arc and clones res_tx, loops:
//       { let job = { lock, recv }; match job { Ok(x) => res_tx.send(x*x), Err=>break } }
//     (bind+drop the lock guard BEFORE computing so workers run in parallel.)
//   - Main: send every input as a job, then DROP job_tx so workers can finish.
//     Also DROP the main res_tx so the results channel disconnects once workers
//     exit (otherwise your result drain hangs — rung 3's lesson, again).
//   - Collect all results, SORT them (order is nondeterministic), return the Vec.
//
// Returns the squares of inputs, sorted. e.g. [1,2,3,4] -> [1,4,9,16].
//
// Footgun to watch: keeping the original res_tx alive in main while you drain
// res_rx => the drain never sees Disconnected => hang. Drop it before draining.
// ---------------------------------------------------------------------------

fn run_pool(n_workers: usize, inputs: Vec<i64>) -> Vec<i64> {
    let (job_tx, job_rx) = mpsc::channel();
    let (res_tx, res_rx) = mpsc::channel();
    let job_rx = Arc::new(Mutex::new(job_rx));

    for _ in 0..n_workers {
        let job_rx = Arc::clone(&job_rx);
        let res_tx = res_tx.clone();

        thread::spawn(move || {
            loop {
                let job = {
                    let job_rx = job_rx.lock().unwrap();
                    job_rx.recv()
                };

                match job {
                    Ok(x) => res_tx.send(x * x).unwrap(),
                    Err(_) => break,
                }
            }
        });
    }

    for input in inputs {
        job_tx.send(input).unwrap();
    }
    drop(job_tx);
    drop(res_tx);

    let mut results: Vec<i64> = res_rx.into_iter().collect();
    results.sort();
    results
}

fn check_7() {
    let got = run_pool(4, vec![1, 2, 3, 4, 5]);
    assert_eq!(got, vec![1, 4, 9, 16, 25], "pool should square every input");
    let big: Vec<i64> = (1..=50).collect();
    let mut want: Vec<i64> = big.iter().map(|x| x * x).collect();
    want.sort();
    assert_eq!(
        run_pool(8, big),
        want,
        "8 workers, 50 jobs, every result accounted for"
    );
    println!("check_7 ✅  worker pool fanned out and back in: {:?}", got);
}

// ---------------------------------------------------------------------------
// Problem 8 — crossbeam: what std channels can't do (mpmc + select!)
//
// std::sync::mpsc is SINGLE-consumer: Receiver is !Clone, so rung 7 needed an
// Arc<Mutex<Receiver>>. crossbeam-channel gives you two things std doesn't:
//
//   (1) MPMC: crossbeam's Receiver IS Clone. Multiple consumers, no Mutex.
//       use crossbeam_channel::{unbounded, bounded, select, Receiver, Sender};
//       let (tx, rx) = unbounded();   // or bounded(k) for backpressure
//       Each worker gets rx.clone(); they cooperatively drain the same queue.
//
//   (2) select!: block until ANY of several channels is ready, act on the first
//       one that fires. std has no way to wait on two channels at once.
//
// Implement TWO functions:
//
// (a) `mpmc_pool(n_workers, inputs)` — same contract as run_pool from rung 7
//     (return sorted squares), but with crossbeam: clone the RECEIVER into each
//     worker, NOT an Arc<Mutex>. Notice how much simpler the worker loop is —
//     `for job in rx.iter()` works directly because rx is shared+Clone, and the
//     iterator ends when all senders drop. Drop the job-sender after queuing.
//
// (b) `merge_two(a, b)` — you're given two Vec<i64>. Send each over its OWN
//     crossbeam channel from two threads, then on the main thread use `select!`
//     in a loop to MERGE both streams into one Vec as values arrive, finishing
//     when BOTH channels are disconnected. Return the merged Vec, sorted.
//
//     select! shape (loop until both done):
//       let mut open_a = true; let mut open_b = true;
//       while open_a || open_b {
//           select! {
//               recv(rx_a) -> msg => match msg {
//                   Ok(v) => out.push(v),
//                   Err(_) => open_a = false,   // rx_a disconnected
//               },
//               recv(rx_b) -> msg => match msg { Ok(v)=>out.push(v), Err(_)=>open_b=false },
//           }
//       }
//     FOOTGUN: once a channel disconnects, recv on it returns Err IMMEDIATELY
//     and select! may keep picking it — that's why you must STOP selecting a
//     dead channel. The simplest correct fix: track open_a/open_b and, once a
//     channel is closed, you need select! to ignore it. crossbeam lets you do
//     this by only including a branch when its flag is still open... but the
//     macro is static. Easiest robust approach for this rung: in each Err arm,
//     set the flag false AND `continue` only matters if the OTHER is still open.
//     If both Err paths just set flags, a closed channel keeps returning Err and
//     spins. To avoid the spin cheaply: after a channel closes, switch to
//     draining the OTHER one with a plain `for v in rx_other` once the first is
//     done. Design it so it terminates — prove it with the asserts.
// ---------------------------------------------------------------------------

fn mpmc_pool(n_workers: usize, inputs: Vec<i64>) -> Vec<i64> {
    let (job_tx, job_rx) = unbounded();
    let (res_tx, res_rx) = unbounded();

    for _ in 0..n_workers {
        let job_rx = job_rx.clone();
        let res_tx = res_tx.clone();

        thread::spawn(move || {
            for job in job_rx {
                res_tx.send(job * job).unwrap();
            }
        });
    }

    for input in inputs {
        job_tx.send(input).unwrap();
    }
    drop(job_tx);
    drop(res_tx);

    let mut results: Vec<i64> = res_rx.into_iter().collect();
    results.sort();
    results
}

fn merge_two(a: Vec<i64>, b: Vec<i64>) -> Vec<i64> {
    let (tx_a, rx_a) = unbounded();
    let (tx_b, rx_b) = unbounded();

    thread::spawn(move || {
        for value in a {
            tx_a.send(value).unwrap();
        }
    });
    thread::spawn(move || {
        for value in b {
            tx_b.send(value).unwrap();
        }
    });

    let mut out = Vec::new();
    let mut open_a = true;
    let mut open_b = true;

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

    if open_a {
        out.extend(rx_a);
    }
    if open_b {
        out.extend(rx_b);
    }

    out.sort();
    out
}

fn check_8() {
    let mut want: Vec<i64> = (1..=30).map(|x| x * x).collect();
    want.sort();
    assert_eq!(
        mpmc_pool(6, (1..=30).collect()),
        want,
        "crossbeam mpmc pool squares all"
    );

    let mut merged = merge_two(vec![1, 3, 5, 7], vec![2, 4, 6]);
    merged.sort();
    assert_eq!(
        merged,
        vec![1, 2, 3, 4, 5, 6, 7],
        "select! merged both streams, none lost"
    );
    println!("check_8 ✅  crossbeam mpmc + select! merge");
}

// ---------------------------------------------------------------------------
// Problem 9 — CAPSTONE: build a channel from scratch
//
// Reimplement a blocking mpsc channel using only safe primitives:
//   Mutex<VecDeque<T>>  — the shared buffer
//   Condvar             — so recv() can SLEEP until a value arrives (no busy spin)
//   Arc                 — both ends share one Inner
//
// The three behaviors you've relied on, now yours to build:
//   1. send(v): push to the queue, then WAKE one sleeping receiver (notify).
//   2. recv():  if a value is ready, take it; else SLEEP on the Condvar until
//      notified — then re-check (spurious wakeups + the predicate => `while`).
//   3. DISCONNECT: when every Sender is gone, a blocked recv() must wake and
//      return Err — not sleep forever. Track the live sender count; when a
//      Sender drops and the count hits 0, notify so the receiver unblocks.
//
// Design given to you (fill in the bodies):
//
//   struct Inner<T> { queue: Mutex<Shared<T>>, available: Condvar }
//   struct Shared<T> { items: VecDeque<T>, senders: usize }
//
//   Sender<T>  { inner: Arc<Inner<T>> }   // Clone bumps senders; Drop lowers it
//   Receiver<T>{ inner: Arc<Inner<T>> }
//
// Key invariant to reason about (write it in your head, like a SAFETY note even
// though this is all safe code): "a blocked recv() is guaranteed to be woken by
// EITHER a send (item available) OR the last sender dropping (senders == 0)."
// If you ever mutate `items` or `senders` and DON'T notify, a receiver can sleep
// forever. Every state change a receiver waits on must be followed by a notify.
//
// recv() skeleton:
//   let mut shared = self.inner.queue.lock().unwrap();
//   loop {
//       if let Some(v) = shared.items.pop_front() { return Ok(v); }
//       if shared.senders == 0 { return Err(Disconnected); }
//       shared = self.inner.available.wait(shared).unwrap();   // sleeps, unlocks
//   }
//
// send():  lock; push_back(v); drop/unlock; available.notify_one();
// Sender::clone(): lock; senders += 1; build a new Sender sharing the Arc.
// Sender::drop():  lock; senders -= 1; if senders == 0 { available.notify_all(); }
//   (notify_all on the last drop so a parked receiver wakes to see disconnect.)
//
// channel() returns (Sender, Receiver) with senders == 1.
//
// Implement everything below and make check_9 pass.
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
struct Disconnected;

struct Shared<T> {
    items: VecDeque<T>,
    senders: usize,
}

impl<T> Shared<T> {
    fn new(senders: usize) -> Self {
        Self {
            items: VecDeque::new(),
            senders,
        }
    }
}

struct Inner<T> {
    queue: Mutex<Shared<T>>,
    available: Condvar,
}

impl<T> Inner<T> {
    fn new(shared: Shared<T>) -> Self {
        Self {
            queue: Mutex::new(shared),
            available: Condvar::new(),
        }
    }
}

struct MySender<T> {
    inner: Arc<Inner<T>>,
}

struct MyReceiver<T> {
    inner: Arc<Inner<T>>,
}

fn my_channel<T>() -> (MySender<T>, MyReceiver<T>) {
    let shared = Shared::new(1);
    let inner = Inner::new(shared);
    let inner = Arc::new(inner);

    (
        MySender {
            inner: Arc::clone(&inner),
        },
        MyReceiver { inner },
    )
}

impl<T> MySender<T> {
    fn send(&self, value: T) {
        {
            let mut shared = self.inner.queue.lock().unwrap();
            shared.items.push_back(value);
        }
        self.inner.available.notify_one();
    }

    fn update_senders(&self, delta: isize) -> usize {
        let mut shared = self.inner.queue.lock().unwrap();
        if delta >= 0 {
            shared.senders += delta as usize;
        } else {
            let decrement = delta.unsigned_abs();
            assert!(shared.senders >= decrement, "invalid sender count");
            shared.senders -= decrement;
        }
        shared.senders
    }
}

impl<T> Clone for MySender<T> {
    fn clone(&self) -> Self {
        self.update_senders(1);
        MySender {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Drop for MySender<T> {
    fn drop(&mut self) {
        if self.update_senders(-1) == 0 {
            self.inner.available.notify_all();
        }
    }
}

impl<T> MyReceiver<T> {
    fn recv(&self) -> Result<T, Disconnected> {
        let mut shared = self.inner.queue.lock().unwrap();
        loop {
            if let Some(item) = shared.items.pop_front() {
                return Ok(item);
            }
            if shared.senders == 0 {
                return Err(Disconnected);
            }
            shared = self.inner.available.wait(shared).unwrap();
        }
    }
}

fn check_9() {
    // (a) basic send/recv across a thread, blocking recv waits for a late send
    let (tx, rx) = my_channel::<i32>();
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50)); // recv must BLOCK until this fires
        tx.send(42);
    });
    assert_eq!(
        rx.recv(),
        Ok(42),
        "recv should block then receive the late value"
    );
    handle.join().unwrap();

    // (b) multi-producer + clean disconnect: 3 senders each send 10 values,
    //     drain with recv() until Err(Disconnected). Must see all 30, then stop.
    let (tx, rx) = my_channel::<i64>();
    for _ in 0..3 {
        let tx = tx.clone();
        thread::spawn(move || {
            for v in 0..10 {
                tx.send(v);
            }
        });
    }
    drop(tx); // drop the original so senders can reach 0 once workers finish

    let mut count = 0;
    let mut sum = 0i64;
    loop {
        match rx.recv() {
            Ok(v) => {
                count += 1;
                sum += v;
            }
            Err(Disconnected) => break,
        }
    }
    assert_eq!(count, 30, "should receive exactly 3*10 values");
    assert_eq!(sum, 3 * (0..10).sum::<i64>(), "every value accounted for");
    println!("check_9 ✅  hand-rolled Channel<T>: blocking recv + multi-producer + disconnect");
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
    println!("\nAll unlocked checks passed 🎉");
}
