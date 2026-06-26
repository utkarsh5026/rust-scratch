//! Architecture: shared state vs message passing — choosing, and combining.
//!
//! Run: `cargo run --bin concurrency_models`
//!
//! Two ways threads coordinate on data:
//!   - SHARED STATE   : one piece of memory, many pointers, serialized by a lock
//!                      / atomics. "Communicate by sharing memory."
//!   - MESSAGE PASSING: one owner at a time, hand data off down a channel, no
//!                      lock needed. "Share memory by communicating."
//!
//! Ladder:
//!   1. [x] Two roads, one counter   — sum 1..=N via Arc<Mutex> AND via mpsc     (foundations)
//!   2. [x] Ownership transfer       — owned jobs through a channel, no lock     (foundations)
//!   3. [x] Pipeline of stages       — produce→map→filter→collect via channels   (mechanics)
//!   4. [x] Fan-out/fan-in both ways — shared VecDeque queue vs mpsc job queue   (mechanics)
//!   5. [x] Lock held too long       — shrink the critical section               (footgun)
//!   6. [x] The message-passing hang — full bounded chan / stray sender / Arc trap (footgun)
//!   7. [x] The actor                — one owner of a HashMap, command + reply    (real-world)
//!   8. [x] Hybrid                   — writes via actor, reads from snapshot      (real-world)
//!   9. [ ] Mini KV store, two impls — one trait, shared-state vs actor          (capstone)

use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

// ---------------------------------------------------------------------------
// Problem 1 — Two roads, one counter.
//
// Sum 1 + 2 + ... + N across THREADS=8 worker threads, two different ways.
// Each worker is responsible for a contiguous slice of the range and produces
// the partial sum of its slice; the two designs differ only in HOW the partials
// become a single total.
//
// (a) sum_shared: a single Arc<Mutex<u64>> that every worker locks and adds its
//     partial into. The total lives in shared memory.
//
// (b) sum_message: each worker SENDS its partial down an mpsc channel; the main
//     thread is the sole owner of the total and folds the received partials.
//     No Mutex anywhere.
//
// Both must return the same value: N*(N+1)/2.
// ---------------------------------------------------------------------------

const THREADS: u64 = 8;

/// Split 1..=n into THREADS contiguous chunks; return the (lo, hi) inclusive
/// bounds for chunk index `i`. Helper so both versions partition identically.
fn chunk_bounds(n: u64, i: u64) -> (u64, u64) {
    let per = n / THREADS;
    let lo = i * per + 1;
    let hi = if i == THREADS - 1 { n } else { (i + 1) * per };
    (lo, hi)
}

/// (a) SHARED STATE: workers lock a shared accumulator and add their partial.
fn sum_shared(n: u64) -> u64 {
    let total = Arc::new(Mutex::new(0));
    let mut handles = Vec::new();
    for i in 0..THREADS {
        let total = Arc::clone(&total);
        handles.push(thread::spawn(move || {
            let (lo, hi) = chunk_bounds(n, i);
            let mut total = total.lock().unwrap();
            *total += (lo + hi) * (hi - lo + 1) / 2;
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    *total.lock().unwrap()
}

/// (b) MESSAGE PASSING: workers send their partial down a channel; main folds.
fn sum_message(n: u64) -> u64 {
    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::new();
    for i in 0..THREADS {
        let tx = mpsc::Sender::clone(&tx);
        handles.push(thread::spawn(move || {
            let (lo, hi) = chunk_bounds(n, i);
            let partial = (lo + hi) * (hi - lo + 1) / 2;
            tx.send(partial).unwrap();
        }));
    }
    drop(tx);
    let mut total = 0;
    while let Ok(partial) = rx.recv() {
        total += partial;
    }
    for handle in handles {
        handle.join().unwrap();
    }
    total
}

fn check_1() {
    let n = 1_000_000;
    let expected = n * (n + 1) / 2;
    assert_eq!(sum_shared(n), expected, "sum_shared wrong");
    assert_eq!(sum_message(n), expected, "sum_message wrong");
    println!("check_1 ok: both roads give {expected}");
}

// ---------------------------------------------------------------------------
// Problem 2 — Ownership transfer = no lock.
//
// You have a batch of independent jobs, each an owned Vec<u8> payload. A pool of
// WORKERS threads should each process some jobs. "Process" = sum the bytes of
// the payload (as u64). We want the grand total of every job's byte-sum.
//
// THE POINT of this rung: with message passing, each job is OWNED by exactly one
// worker while being processed, then its result is handed back. The Vec<u8> is
// never shared, so there is NO Mutex on the payloads at all.
//
// Design:
//   - A jobs channel: main sends every Vec<u8> down `job_tx`; clone `job_rx`?
//     No — mpsc Receiver is single-consumer. Instead share ONE receiver across
//     workers with Arc<Mutex<Receiver>> (lock only to PULL the next job, then
//     release before processing — the work happens off-lock).
//   - A results channel: each worker sends its per-job byte-sum down `res_tx`.
//   - Main drops its job_tx so workers' `recv()` ends, then folds the results.
//
// Allowed locking: ONLY the Arc<Mutex<Receiver>> to dequeue. The payload itself
// is moved into the worker and never locked. (Rung 4 revisits this contrast.)
// ---------------------------------------------------------------------------

const WORKERS: usize = 4;

fn process_jobs(jobs: Vec<Vec<u8>>) -> u64 {
    let (job_tx, job_rx) = mpsc::channel::<Vec<u8>>();
    let (res_tx, res_rx) = mpsc::channel::<u64>();
    let job_rx = Arc::new(Mutex::new(job_rx));

    for _ in 0..WORKERS {
        let job_rx = Arc::clone(&job_rx);
        let res_tx = mpsc::Sender::clone(&res_tx);
        thread::spawn(move || {
            loop {
                let job = {
                    let job_rx = job_rx.lock().unwrap();
                    job_rx.recv()
                };

                match job {
                    Ok(payload) => {
                        let sum = payload.into_iter().map(u64::from).sum();
                        res_tx.send(sum).unwrap();
                    }
                    Err(_) => break,
                }
            }
        });
    }

    for job in jobs {
        job_tx.send(job).unwrap();
    }
    drop(job_tx);
    drop(res_tx);

    res_rx.into_iter().sum()
}

fn check_2() {
    let jobs: Vec<Vec<u8>> = (0..1000u32)
        .map(|i| vec![(i % 7) as u8; (i % 13) as usize])
        .collect();
    let expected: u64 = jobs
        .iter()
        .map(|j| j.iter().map(|&b| b as u64).sum::<u64>())
        .sum();
    assert_eq!(process_jobs(jobs), expected, "process_jobs wrong total");
    println!("check_2 ok: grand byte-sum = {expected}");
}

// ---------------------------------------------------------------------------
// Problem 3 — Pipeline of stages.
//
// Message passing shines for PIPELINES: each stage is its own thread, stages are
// linked by channels, and an item flows stage→stage, owned by one stage at a
// time. This is the Unix-pipe model (`producer | map | filter`) in threads.
//
// Build a 3-stage pipeline over the input numbers:
//   stage 1 (produce): send each input number down channel c1.
//   stage 2 (map):     recv from c1, multiply by 3, send down c2.
//   stage 3 (filter):  recv from c2, keep only EVEN values, send down c3.
//   main (collect):    drain c3 into a Vec and return it (in order).
//
// Rules that make it a real pipeline:
//   - Each of the 3 stages runs in its OWN thread::spawn.
//   - Stages are connected ONLY by channels (no shared Vec, no Mutex).
//   - Each stage owns its receiver, loops over it (`for x in rx`), and drops its
//     sender when its input ends so the NEXT stage's loop terminates too. The
//     "end of stream" signal propagates down the pipe automatically.
//   - Order is preserved because a single thread feeds each channel in order.
// ---------------------------------------------------------------------------

fn pipeline(input: Vec<i64>) -> Vec<i64> {
    let (tx, rx) = mpsc::channel();
    let (tx1, rx1) = mpsc::channel();
    let (tx2, rx2) = mpsc::channel();

    thread::spawn(move || {
        for x in input {
            tx.send(x).unwrap();
        }
    });

    thread::spawn(move || {
        for x in rx {
            tx1.send(x * 3).unwrap();
        }
    });

    thread::spawn(move || {
        for x in rx1 {
            if x % 2 == 0 {
                tx2.send(x).unwrap();
            }
        }
    });

    let mut out = Vec::new();
    for x in rx2 {
        out.push(x);
    }
    out
}

fn check_3() {
    let out = pipeline((1..=10).collect());
    // ×3: 3,6,9,12,15,18,21,24,27,30 ; keep even: 6,12,18,24,30
    assert_eq!(out, vec![6, 12, 18, 24, 30], "pipeline output wrong");
    println!("check_3 ok: pipeline -> {out:?}");
}

// ---------------------------------------------------------------------------
// Problem 4 — Fan-out / fan-in, the SAME job pool built both ways.
//
// Goal both times: given `tasks` (each a u64), run WORKERS threads that each
// compute `work(task)` and return the SUM of all results. The two impls differ
// only in HOW workers get their next task — that's the whole comparison.
//
// `work` is shared so both versions do identical CPU.
//
// (a) fanout_shared_queue: SHARED STATE queue.
//     Put all tasks in an Arc<Mutex<VecDeque<u64>>>. Each worker loops:
//     lock, pop_front, unlock, then compute off-lock and accumulate locally;
//     return its partial via join() (a JoinHandle<u64>). Main sums the partials.
//     When the deque is empty, the worker stops. NOTE: there is no "blocking" —
//     an empty queue just means "done", because all tasks are pre-loaded.
//
// (b) fanout_channel: MESSAGE PASSING queue.
//     Send all tasks down a job channel; share the Receiver via
//     Arc<Mutex<Receiver>> (like rung 2). Workers recv until disconnect, compute
//     off-lock, send results down a results channel; main folds the results.
//
// Same answer. As you write both, FEEL the difference:
//   - shared queue: YOU manage "is it empty?" and there's no natural blocking /
//     backpressure — if tasks arrived over time you'd be busy-spinning.
//   - channel: recv() blocks until a task or disconnect; "stream ended" is free.
// ---------------------------------------------------------------------------

fn work(task: u64) -> u64 {
    // a little deterministic compute so it's not totally trivial
    (task.wrapping_mul(2654435761)) % 1_000_000
}

/// (a) SHARED STATE: an Arc<Mutex<VecDeque>> the workers drain.
fn fanout_shared_queue(tasks: Vec<u64>) -> u64 {
    let queue = tasks.into_iter().collect::<VecDeque<_>>();
    let queue = Arc::new(Mutex::new(queue));
    let mut handles = Vec::new();
    for _ in 0..WORKERS {
        let queue = Arc::clone(&queue);
        handles.push(thread::spawn(move || {
            let mut partial = 0;
            loop {
                let task = {
                    let mut queue = queue.lock().unwrap();
                    queue.pop_front()
                };
                if task.is_none() {
                    break;
                }
                let task = task.unwrap();
                partial += work(task);
            }
            partial
        }));
    }
    let partials = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    partials.into_iter().sum()
}

/// (b) MESSAGE PASSING: a job channel + shared Receiver, results channel.
fn fanout_channel(tasks: Vec<u64>) -> u64 {
    let (job_tx, job_rx) = mpsc::channel::<u64>();
    let (res_tx, res_rx) = mpsc::channel::<u64>();
    let job_rx = Arc::new(Mutex::new(job_rx));
    let mut handles = Vec::new();

    for _ in 0..WORKERS {
        let job_rx = Arc::clone(&job_rx);
        let res_tx = mpsc::Sender::clone(&res_tx);
        handles.push(thread::spawn(move || {
            loop {
                let task = {
                    let job_rx = job_rx.lock().unwrap();
                    job_rx.recv()
                };

                match task {
                    Ok(task) => res_tx.send(work(task)).unwrap(),
                    Err(_) => break,
                }
            }
        }));
    }

    for task in tasks {
        job_tx.send(task).unwrap();
    }
    drop(job_tx);
    drop(res_tx);

    let total = res_rx.into_iter().sum();
    for handle in handles {
        handle.join().unwrap();
    }
    total
}

fn check_4() {
    let tasks: Vec<u64> = (1..=5000).collect();
    let expected: u64 = tasks.iter().map(|&t| work(t)).sum();
    assert_eq!(
        fanout_shared_queue(tasks.clone()),
        expected,
        "shared-queue total wrong"
    );
    assert_eq!(fanout_channel(tasks), expected, "channel total wrong");
    println!("check_4 ok: both fan-out/fan-in totals = {expected}");
}

// ---------------------------------------------------------------------------
// Problem 5 — Lock held too long (the shared-state tax).
//
// The defining footgun of shared state: if you do SLOW WORK while holding the
// lock, every other thread is blocked waiting — you've serialized your program
// back down to one core, no matter how many threads you spawned.
//
// Both functions compute the SAME sum of expensive(task) over all tasks, using
// WORKERS threads and a single Arc<Mutex<u64>> accumulator. Tasks are split into
// WORKERS contiguous chunks (one per thread). The ONLY difference is whether the
// expensive call happens inside or outside the critical section.
//
//   (a) accumulate_lock_held:  lock total, then call expensive() while holding
//       the lock, then += . Every worker is stuck behind whoever holds the lock,
//       so total runtime ~= (#tasks) * SLOW  (fully serialized).
//
//   (b) accumulate_lock_shrunk: call expensive() OFF the lock, then lock only to
//       do the += . Workers compute in parallel; runtime ~= (#tasks/WORKERS) * SLOW.
//
// check_5 times both and asserts shrunk is clearly faster (and both sums equal).
// ---------------------------------------------------------------------------

use std::time::{Duration, Instant};

/// Simulates an expensive, lock-IRRELEVANT computation (e.g. parsing, hashing,
/// an I/O call). The sleep stands in for real work that has no business being
/// done while holding a lock.
fn expensive(task: u64) -> u64 {
    thread::sleep(Duration::from_millis(2));
    work(task)
}

/// (a) BAD: expensive() runs while the accumulator lock is held.
fn accumulate_lock_held(tasks: Vec<u64>) -> u64 {
    let total = Arc::new(Mutex::new(0));
    let mut handles = Vec::new();
    for task in tasks {
        let total = Arc::clone(&total);
        handles.push(thread::spawn(move || {
            let mut total = total.lock().unwrap();
            *total += expensive(task);
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    *total.lock().unwrap()
}

/// (b) GOOD: expensive() runs off-lock; lock only for the +=.
fn accumulate_lock_shrunk(tasks: Vec<u64>) -> u64 {
    let total = Arc::new(Mutex::new(0));
    let mut handles = Vec::new();
    for task in tasks {
        let total = Arc::clone(&total);
        handles.push(thread::spawn(move || {
            let v = expensive(task);
            let mut total = total.lock().unwrap();
            *total += v;
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    *total.lock().unwrap()
}

fn check_5() {
    let tasks: Vec<u64> = (1..=40).collect();
    let expected: u64 = tasks.iter().map(|&t| work(t)).sum();

    let t0 = Instant::now();
    let held = accumulate_lock_held(tasks.clone());
    let held_ms = t0.elapsed().as_millis();

    let t1 = Instant::now();
    let shrunk = accumulate_lock_shrunk(tasks);
    let shrunk_ms = t1.elapsed().as_millis();

    assert_eq!(held, expected, "lock-held sum wrong");
    assert_eq!(shrunk, expected, "lock-shrunk sum wrong");
    assert!(
        shrunk_ms * 2 < held_ms,
        "expected shrunk to be much faster: held={held_ms}ms shrunk={shrunk_ms}ms"
    );
    println!("check_5 ok: held={held_ms}ms  shrunk={shrunk_ms}ms  (sum={expected})");
}

// ---------------------------------------------------------------------------
// Problem 6 — The message-passing footguns (made observable, not hung).
//
// Channels have their own sharp edges. We expose each one with a timeout / a
// try_* call so "it would hang" becomes a testable Err instead of a real hang.
//
//   (a) recv_blocks_with_stray_sender: ONE live Sender (even an unused clone)
//       keeps the channel open, so recv waits forever for data that never comes.
//       Prove it via recv_timeout -> Err(Timeout).
//
//   (b) recv_disconnects_when_all_senders_dropped: drop EVERY sender, then recv
//       returns Err(RecvError) — the clean "all senders gone" shutdown signal.
//       (Only difference from (a): is a sender still alive?)
//
//   (c) bounded_full: a sync_channel(2) buffer fills; try_send the 3rd value ->
//       Err(Full). Backpressure made visible. A blocking send here would park
//       until a consumer drains — with no consumer, that's a deadlock.
//
//   (d) arc_through_channel_is_shared: THE TRAP. Sending an Arc<Mutex<T>> through
//       a channel does NOT transfer ownership of the data — both Arcs alias the
//       same Mutex. You've re-introduced shared state behind a message-passing
//       facade. Prove it: a worker mutates through its clone, main observes the
//       change through its own clone.
// ---------------------------------------------------------------------------

fn recv_blocks_with_stray_sender() -> Result<i32, mpsc::RecvTimeoutError> {
    let (tx, rx) = mpsc::channel();
    let _tx_clone = tx.clone();
    rx.recv_timeout(Duration::from_millis(50))
}

fn recv_disconnects_when_all_senders_dropped() -> Result<i32, mpsc::RecvError> {
    let (tx, rx) = mpsc::channel();
    let tx_clone = tx.clone();
    drop(tx);
    drop(tx_clone);
    rx.recv()
}

fn bounded_full() -> Result<(), mpsc::TrySendError<i32>> {
    let (tx, _rx) = mpsc::sync_channel::<i32>(2);
    tx.send(1).unwrap();
    tx.send(2).unwrap();
    tx.try_send(99)
}

fn arc_through_channel_is_shared() -> i32 {
    let arc = Arc::new(Mutex::new(0));
    let arc_clone = arc.clone();
    let (tx, rx) = mpsc::channel();
    tx.send(arc_clone).unwrap();
    let worker = thread::spawn(move || {
        let arc = rx.recv().unwrap();
        let mut arc = arc.lock().unwrap();
        *arc += 10;
    });
    worker.join().unwrap();
    *arc.lock().unwrap()
}

fn check_6() {
    assert!(
        matches!(
            recv_blocks_with_stray_sender(),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "(a) expected Timeout: a live sender should keep recv blocking"
    );
    assert!(
        recv_disconnects_when_all_senders_dropped().is_err(),
        "(b) expected RecvError once every sender is dropped"
    );
    assert!(
        matches!(bounded_full(), Err(mpsc::TrySendError::Full(99))),
        "(c) expected Full: a sync_channel(2) with 2 buffered should reject the 3rd"
    );
    assert_eq!(
        arc_through_channel_is_shared(),
        10,
        "(d) Arc<Mutex> sent through a channel still ALIASES — main should see the worker's +=10"
    );
    println!(
        "check_6 ok: stray-sender hang, clean disconnect, bounded backpressure, and the Arc-through-channel trap"
    );
}

// ---------------------------------------------------------------------------
// Problem 7 — The actor: one owner, command + reply.
//
// THE combining pattern. A single thread privately OWNS a HashMap — no Mutex,
// no Arc around the map. Other threads hold a cheap clonable handle (just a
// Sender) and send COMMAND messages. For reads, the command carries a one-shot
// reply channel so the actor can send the answer back.
//
// Why no lock on the map? Because exactly ONE thread ever touches it. The actor
// pulls commands off its queue and processes them one at a time — serialization
// is automatic. Shared-state SEMANTICS (one logical store everyone uses) with
// message-passing MECHANICS (zero locks on the data). "Share memory by
// communicating."
//
// Implement:
//   - KvActor::spawn(): command channel + a worker thread owning a HashMap that
//     loops `for cmd in rx` and handles Get (reply with the value) / Set (insert).
//   - get(&self, key): fresh reply channel, send Get, block on reply.recv().
//   - set(&self, key, value): fire-and-forget Set.
// ---------------------------------------------------------------------------

use std::collections::HashMap;

enum Command {
    Get {
        key: String,
        reply: mpsc::Sender<Option<String>>,
    },
    Set {
        key: String,
        value: String,
    },
}

#[derive(Clone)]
struct KvActor {
    tx: mpsc::Sender<Command>,
}

impl KvActor {
    fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut map = HashMap::new();
            for cmd in rx {
                match cmd {
                    Command::Get { key, reply } => {
                        reply.send(map.get(&key).cloned()).unwrap();
                    }
                    Command::Set { key, value } => {
                        map.insert(key, value);
                    }
                }
            }
        });
        KvActor { tx }
    }

    fn get(&self, key: &str) -> Option<String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::Get {
                key: key.to_string(),
                reply: reply_tx,
            })
            .unwrap();
        reply_rx.recv().unwrap()
    }

    fn set(&self, key: &str, value: &str) {
        self.tx
            .send(Command::Set {
                key: key.to_string(),
                value: value.to_string(),
            })
            .unwrap();
    }
}

fn check_7() {
    let kv = KvActor::spawn();
    assert_eq!(kv.get("missing"), None, "absent key should be None");
    kv.set("a", "1");
    kv.set("b", "2");
    assert_eq!(kv.get("a"), Some("1".to_string()));
    assert_eq!(kv.get("b"), Some("2".to_string()));
    kv.set("a", "99");
    assert_eq!(kv.get("a"), Some("99".to_string()), "set should overwrite");

    // Many threads, one actor: clone the cheap handle into each.
    let mut handles = Vec::new();
    for i in 0..10 {
        let kv = kv.clone();
        handles.push(thread::spawn(move || {
            kv.set(&format!("k{i}"), &format!("v{i}"));
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    for i in 0..10 {
        assert_eq!(
            kv.get(&format!("k{i}")),
            Some(format!("v{i}")),
            "concurrent set lost"
        );
    }
    println!("check_7 ok: actor owns the map, 10 threads talk to it — no Mutex on the data");
}

// ---------------------------------------------------------------------------
// Problem 8 — Hybrid: writes through the actor, reads from a published snapshot.
//
// The rung-7 actor made reads queue behind writes (a Get is a command on the
// same channel). For read-heavy workloads, decouple them:
//   - WRITES stay serialized through the actor (single writer, authoritative map
//     owned privately, no write lock on the data).
//   - After each write the actor PUBLISHES an immutable snapshot into a shared
//     Arc<RwLock<Arc<HashMap>>>.
//   - READS bypass the actor: take a microsecond read-lock, clone the inner Arc
//     (a refcount bump), release, then read with NO lock held. Read latency is
//     now independent of how backed-up the write queue is.
//
// Tradeoff: publishing clones the whole map per write (the price of lock-light,
// never-changes-under-you reads). In production: arc-swap for an atomic pointer
// swap instead of RwLock, and/or `im`/persistent maps so the clone is cheap.
//
// Implement spawn (worker publishes after each insert, then acks), set (block on
// ack for read-your-writes), get (snapshot read, no lock during the lookup).
// ---------------------------------------------------------------------------

use std::sync::RwLock;

type Snapshot = Arc<HashMap<String, String>>;

enum WriteCmd {
    Set {
        key: String,
        value: String,
        ack: mpsc::Sender<()>,
    },
}

#[derive(Clone)]
struct HybridKv {
    tx: mpsc::Sender<WriteCmd>,
    snapshot: Arc<RwLock<Snapshot>>,
}

impl HybridKv {
    fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        let snapshot = Arc::new(RwLock::new(Arc::new(HashMap::new())));
        let worker_snapshot = Arc::clone(&snapshot);
        thread::spawn(move || {
            let mut map = HashMap::new();
            for cmd in rx {
                match cmd {
                    WriteCmd::Set { key, value, ack } => {
                        map.insert(key, value);
                        *worker_snapshot.write().unwrap() = Arc::new(map.clone());
                        ack.send(()).unwrap();
                    }
                }
            }
        });
        HybridKv { tx, snapshot }
    }

    fn set(&self, key: &str, value: &str) {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.tx
            .send(WriteCmd::Set {
                key: key.to_string(),
                value: value.to_string(),
                ack: ack_tx,
            })
            .unwrap();
        ack_rx.recv().unwrap();
    }

    fn get(&self, key: &str) -> Option<String> {
        let snapshot = Arc::clone(&self.snapshot.read().unwrap());
        snapshot.get(key).cloned()
    }
}

fn check_8() {
    let kv = HybridKv::spawn();
    assert_eq!(kv.get("nope"), None);
    kv.set("x", "1");
    kv.set("y", "2");
    assert_eq!(
        kv.get("x"),
        Some("1".to_string()),
        "read-your-writes failed"
    );
    assert_eq!(kv.get("y"), Some("2".to_string()));
    kv.set("x", "9");
    assert_eq!(
        kv.get("x"),
        Some("9".to_string()),
        "overwrite not published"
    );

    // Concurrent storm: writers mutate while readers snapshot-read in parallel.
    let mut handles = Vec::new();
    for i in 0..8 {
        let kv = kv.clone();
        handles.push(thread::spawn(move || {
            kv.set(&format!("w{i}"), &i.to_string());
        }));
    }
    for _ in 0..8 {
        let kv = kv.clone();
        handles.push(thread::spawn(move || {
            // readers never block writers; they just read whatever snapshot exists
            let _ = kv.get("x");
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    for i in 0..8 {
        assert_eq!(
            kv.get(&format!("w{i}")),
            Some(i.to_string()),
            "concurrent write lost"
        );
    }
    println!("check_8 ok: single-writer actor + lock-light snapshot reads");
}

// ---------------------------------------------------------------------------
// Problem 9 — CAPSTONE: one trait, two concurrency architectures.
//
// Define ONE KvStore trait and implement it TWICE — once as pure shared state,
// once as a pure actor. The same exercise() + hammer() (concurrent storm) run
// against both via Arc<dyn KvStore>, proving two opposite designs are observably
// identical. The skill this proves: you can pick (or swap) the concurrency model
// behind a stable interface without callers caring.
//
//   SharedStore  : Arc<RwLock<HashMap>> — every op takes a lock.
//   ActorStore   : worker thread owns the map; ops are commands (Get/Len carry
//                  a reply channel; Set/Delete are fire-and-forget... but make
//                  them synchronous enough that hammer() sees its writes).
//
// Implement every method of both. Then fill in YOUR ANSWER below: when would you
// ship each in a real system? (No assert checks it — but writing it IS the rung.)
//
// YOUR ANSWER (when to ship which):
//   SharedStore is best when: operations are simple data access and lock
//     contention is low enough that direct reads/writes stay easy and fast.
//   ActorStore  is best when: you want one owner to serialize richer state
//     transitions, hide mutation behind commands, or later move the owner to an
//     async task / process boundary.
// ---------------------------------------------------------------------------

trait KvStore: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str);
    fn delete(&self, key: &str);
    fn len(&self) -> usize;
}

// ---- Implementation A: shared state -------------------------------------------
struct SharedStore {
    map: Arc<RwLock<HashMap<String, String>>>,
}

impl SharedStore {
    fn new() -> Self {
        SharedStore {
            map: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl KvStore for SharedStore {
    fn get(&self, key: &str) -> Option<String> {
        self.map.read().unwrap().get(key).cloned()
    }

    fn set(&self, key: &str, value: &str) {
        self.map
            .write()
            .unwrap()
            .insert(key.to_string(), value.to_string());
    }

    fn delete(&self, key: &str) {
        self.map.write().unwrap().remove(key);
    }

    fn len(&self) -> usize {
        self.map.read().unwrap().len()
    }
}

// ---- Implementation B: actor / message passing --------------------------------
enum StoreCmd {
    Get {
        key: String,
        reply: mpsc::Sender<Option<String>>,
    },
    Set {
        key: String,
        value: String,
        ack: mpsc::Sender<()>,
    },
    Delete {
        key: String,
        ack: mpsc::Sender<()>,
    },
    Len {
        reply: mpsc::Sender<usize>,
    },
}

struct ActorStore {
    tx: mpsc::Sender<StoreCmd>,
}

impl ActorStore {
    fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut map = HashMap::new();
            for cmd in rx {
                match cmd {
                    StoreCmd::Get { key, reply } => {
                        reply.send(map.get(&key).cloned()).unwrap();
                    }
                    StoreCmd::Set { key, value, ack } => {
                        map.insert(key, value);
                        ack.send(()).unwrap();
                    }
                    StoreCmd::Delete { key, ack } => {
                        map.remove(&key);
                        ack.send(()).unwrap();
                    }
                    StoreCmd::Len { reply } => {
                        reply.send(map.len()).unwrap();
                    }
                }
            }
        });
        ActorStore { tx }
    }
}

impl KvStore for ActorStore {
    fn get(&self, key: &str) -> Option<String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(StoreCmd::Get {
                key: key.to_string(),
                reply: reply_tx,
            })
            .unwrap();
        reply_rx.recv().unwrap()
    }
    fn set(&self, key: &str, value: &str) {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.tx
            .send(StoreCmd::Set {
                key: key.to_string(),
                value: value.to_string(),
                ack: ack_tx,
            })
            .unwrap();
        ack_rx.recv().unwrap();
    }
    fn delete(&self, key: &str) {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.tx
            .send(StoreCmd::Delete {
                key: key.to_string(),
                ack: ack_tx,
            })
            .unwrap();
        ack_rx.recv().unwrap();
    }
    fn len(&self) -> usize {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx.send(StoreCmd::Len { reply: reply_tx }).unwrap();
        reply_rx.recv().unwrap()
    }
}

// ---- The shared test harness, run against BOTH impls --------------------------
fn exercise(store: &dyn KvStore, label: &str) {
    assert_eq!(store.get("a"), None, "[{label}] absent key");
    store.set("a", "1");
    store.set("b", "2");
    assert_eq!(
        store.get("a"),
        Some("1".to_string()),
        "[{label}] get after set"
    );
    assert_eq!(store.len(), 2, "[{label}] len after 2 sets");
    store.set("a", "3");
    assert_eq!(store.get("a"), Some("3".to_string()), "[{label}] overwrite");
    store.delete("b");
    assert_eq!(store.get("b"), None, "[{label}] get after delete");
    assert_eq!(store.len(), 1, "[{label}] len after delete");
    println!("  {label}: single-threaded behavior ok");
}

fn hammer(store: Arc<dyn KvStore>, label: &str) {
    let mut handles = Vec::new();
    for t in 0..8 {
        let store = Arc::clone(&store);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                store.set(&format!("t{t}_{i}"), &(t * 100 + i).to_string());
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    for t in 0..8 {
        for i in 0..100 {
            assert_eq!(
                store.get(&format!("t{t}_{i}")),
                Some((t * 100 + i).to_string()),
                "[{label}] concurrent write lost"
            );
        }
    }
    println!("  {label}: 8-thread concurrent storm ok (800 writes survived)");
}

fn check_9() {
    let shared: Arc<dyn KvStore> = Arc::new(SharedStore::new());
    let actor: Arc<dyn KvStore> = Arc::new(ActorStore::spawn());
    for (store, label) in [(shared, "SharedStore"), (actor, "ActorStore")] {
        exercise(store.as_ref(), label);
        hammer(store, label);
    }
    println!("check_9 ok: one trait, two architectures, identical behavior");
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
    // check_4();
    // check_5();
    // check_6();
    // check_7();
    // check_8();
    // check_9();
}
