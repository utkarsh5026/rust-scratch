// Mutex / RwLock — poisoning, lock guards, deadlock avoidance, lock ordering
// Run: cargo run --bin mutex_rwlock
//
// Mental model: a Mutex<T> protects DATA, not code. The only path to the T is
// lock(), which returns a MutexGuard — an RAII token that derefs to &mut T and
// UNLOCKS WHEN DROPPED. RwLock<T> = many readers XOR one writer.
//
// Ladder (DONE marks finished rungs):
//   1. [x] Mutex basics        — lock().unwrap(), guard deref, mutate (single thread)   [foundations]
//   2. [x] Arc<Mutex> counter  — shared counter across N threads                        [foundations]
//   3. [x] Guard lifetime      — early release via drop()/inner scope                   [mechanics]
//   4. [x] RwLock              — many readers XOR one writer                             [mechanics]
//   5. [x] Poisoning           — panic-while-locked poisons; recover via into_inner     [footgun]
//   6. [x] Non-reentrancy      — locking twice in one thread self-deadlocks (try_lock)  [footgun]
//   7. [x] Lock-ordering ABBA  — induce a deadlock, fix with a canonical lock order     [footgun]
//   8. [x] Mutex + Condvar     — bounded blocking queue (wait/notify)                   [real-world]
//   9. [ ] Concurrent Bank     — deadlock-free transfers + poison recovery (capstone)   [capstone]

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};

// ── Rung 1: Mutex basics ────────────────────────────────────────────────────
// A Mutex<i32> guards a single counter. Implement `bump`: lock the mutex, then
// add `by` to the value behind the guard. The guard derefs to the inner i32, so
// `*guard += by` works. Return nothing — the mutation lives inside the Mutex.
//
// Notes:
//   - lock() returns Result<MutexGuard, PoisonError>; for now just .unwrap() it.
//   - You do NOT need `mut m` — Mutex gives interior mutability through &self.
fn bump(m: &Mutex<i32>, by: i32) {
    let mut guard = m.lock().unwrap();
    *guard += by;
}

fn check_1() {
    let m = Mutex::new(0);
    bump(&m, 5);
    bump(&m, 10);
    bump(&m, -3);

    let got = *m.lock().unwrap();
    assert_eq!(got, 12, "expected 5+10-3 = 12, got {got}");
    println!("rung 1 ok: counter = {got}");
}

// ── Rung 2: Arc<Mutex<T>> shared counter across threads ──────────────────────
// THE reason Mutex exists. Spawn `n_threads`, each adding 1 to a shared counter
// `per_thread` times. Return the final total.
//
// A bare Mutex can't be moved into multiple threads (each `move` closure would
// take ownership). Wrap it in an Arc so every thread gets its own owning handle
// to the SAME mutex: `Arc<Mutex<i32>>`. Clone the Arc once per thread before the
// move, lock inside the loop, and += 1.
//
// Collect the JoinHandles, join them all, then read the final value.
//
// Hints:
//   - use std::sync::Arc;  (already a Mutex import above)
//   - let counter = Arc::new(Mutex::new(0));
//   - in the spawn loop: let c = Arc::clone(&counter);  then move c in.
//   - keep the critical section tiny: lock, +=1, let the guard drop.
fn race_counter(n_threads: usize, per_thread: usize) -> i32 {
    let counter = Arc::new(Mutex::new(0));

    std::thread::scope(|s| {
        for _ in 0..n_threads {
            let c = Arc::clone(&counter);
            s.spawn(move || {
                for _ in 0..per_thread {
                    bump(&c, 1);
                }
            });
        }
    });

    let total = *counter.lock().unwrap();
    total
}

fn check_2() {
    let total = race_counter(8, 1000);
    assert_eq!(
        total,
        8 * 1000,
        "expected 8000, got {total} — a lost update means the lock isn't held across read+write"
    );
    println!("rung 2 ok: total = {total}");
}

// ── Rung 3: Guard lifetime & early release ───────────────────────────────────
// The guard holds the lock until it DROPS. Hold it across slow work and every
// other thread blocks. This rung makes you control the critical section's length
// explicitly.
//
// `slow_sum` is given a shared Mutex<Vec<i32>> and an `expensive(i32) -> i32`.
// The naive version locks, then calls `expensive` on each element WHILE HOLDING
// the lock — serializing all the "expensive" work behind the mutex. Instead:
//   1. lock, CLONE the vec out (or copy what you need), then release the lock
//      EARLY — before doing any expensive work.
//   2. run `expensive` on the snapshot with the lock NOT held.
//   3. return the sum.
//
// To release early you have two tools — use whichever reads better:
//   - `drop(guard);`  explicitly, or
//   - a `{ ... }` inner scope so the guard drops at the closing brace.
//
// The check enforces it: `expensive` asserts the lock is NOT held while it runs
// (it tries to lock the SAME mutex and panics if it can't). So if you hold the
// guard across the expensive calls, the test fails.
fn slow_sum(data: &Mutex<Vec<i32>>, expensive: impl Fn(i32) -> i32) -> i32 {
    let guard = data.lock().unwrap();
    let snapshot = guard.clone();
    drop(guard);
    let sum = snapshot.iter().map(|x| expensive(*x)).sum();
    sum
}

fn check_3() {
    let data = Mutex::new(vec![1, 2, 3, 4, 5]);
    // `expensive` proves the lock is free while it runs: if it can't grab the
    // lock, you're still holding the guard → you didn't release early.
    let expensive = |x: i32| {
        assert!(
            data.try_lock().is_ok(),
            "lock is still held while `expensive` runs — release the guard before the slow work!"
        );
        x * x
    };
    let got = slow_sum(&data, expensive);
    assert_eq!(got, 1 + 4 + 9 + 16 + 25, "expected 55, got {got}");
    println!("rung 3 ok: sum of squares = {got}");
}

// ── Rung 4: RwLock — many readers XOR one writer ─────────────────────────────
// A Mutex gives EXCLUSIVE access even to readers — two threads that only want to
// *read* still serialize. RwLock<T> splits the lock:
//   - read()  -> RwLockReadGuard  (&T):     many can hold this at once
//   - write() -> RwLockWriteGuard (&mut T): exclusive, blocks all readers
// Read-heavy shared state (config, caches, routing tables) is the use case.
//
// Implement two functions over a shared Arc<RwLock<Vec<i32>>>:
//   - `reader_sum(rw)` — take a READ guard and return the sum of the vec.
//   - `writer_push(rw, v)` — take a WRITE guard and push `v`.
//
// Then `concurrent_reads` (already written for you) spawns many readers that all
// hold read guards simultaneously and proves they don't block each other.
fn reader_sum(rw: &RwLock<Vec<i32>>) -> i32 {
    let guard = rw.read().unwrap();
    let sum = guard.iter().sum();
    sum
}

fn writer_push(rw: &RwLock<Vec<i32>>, v: i32) {
    let mut guard = rw.write().unwrap();
    guard.push(v);
}

// Proves multiple read guards coexist: each reader grabs a read guard, then we
// check that the count of simultaneously-held read guards reached >= 2. If
// read() were exclusive (like a Mutex), they'd serialize and never overlap.
fn concurrent_reads(rw: &Arc<RwLock<Vec<i32>>>) -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let live = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));

    std::thread::scope(|s| {
        for _ in 0..4 {
            let rw = Arc::clone(rw);
            let live = Arc::clone(&live);
            let max_seen = Arc::clone(&max_seen);
            s.spawn(move || {
                let _g = rw.read().unwrap(); // hold a READ guard
                let now = live.fetch_add(1, Ordering::SeqCst) + 1;
                max_seen.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(50));
                live.fetch_sub(1, Ordering::SeqCst);
                // _g drops here, releasing the read lock
            });
        }
    });
    max_seen.load(Ordering::SeqCst)
}

fn check_4() {
    let rw = Arc::new(RwLock::new(vec![10, 20, 30]));
    assert_eq!(reader_sum(&rw), 60, "read guard sum wrong");
    writer_push(&rw, 40);
    assert_eq!(reader_sum(&rw), 100, "after write the sum should be 100");

    let overlap = concurrent_reads(&rw);
    assert!(
        overlap >= 2,
        "expected >=2 readers to hold the lock at once, saw {overlap} — are reader_sum/writer_push using read()/write()?"
    );
    println!("rung 4 ok: sum=100, max simultaneous readers = {overlap}");
}

// ── Rung 5: Poisoning ────────────────────────────────────────────────────────
// Why does lock() return a Result at all? POISONING. If a thread PANICS while
// holding the guard, the data may be in a half-updated, inconsistent state. Rust
// records this: the mutex becomes "poisoned", and every later lock() returns
// Err(PoisonError). It's a tripwire — "someone died mid-update, the invariant
// may be broken, are you sure you want this data?"
//
// This rung has two parts.
//
// Part A — `poison_it`: take an &Arc<Mutex<i32>>, spawn a thread that locks the
// mutex, mutates it (set to 999), then PANICS while still holding the guard.
// join() the thread and observe it returns Err (the thread unwound). After this,
// the mutex is poisoned.
//   - spawn with std::thread + a cloned Arc; the closure does:
//       let mut g = m.lock().unwrap(); *g = 999; panic!("boom");
//   - the join handle's .join() returns Err(..) — that's expected, swallow it.
//
// Part B — `recover`: the mutex is now poisoned. A plain `.lock().unwrap()` would
// panic. Instead, lock() returns Err(PoisonError). Recover the inner guard from
// the error so you can still read/return the value (the 999 that was written).
// Pattern:  m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
// Return the i32 inside.
fn poison_it(m: &Arc<Mutex<i32>>) {
    let m = Arc::clone(m);
    let handle = std::thread::spawn(move || {
        let mut guard = m.lock().unwrap();
        *guard = 999;
        panic!("boom");
    });
    let _ = handle.join();
}

fn recover(m: &Mutex<i32>) -> i32 {
    let guard = m.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard
}

fn check_5() {
    let m = Arc::new(Mutex::new(0));

    // Before poisoning, a normal lock works.
    assert!(m.lock().is_ok(), "fresh mutex should not be poisoned");

    poison_it(&m);

    // Now it must be poisoned: a plain lock() returns Err.
    assert!(
        m.lock().is_err(),
        "mutex should be POISONED after a thread panicked holding the guard"
    );

    // ...but we can still recover the (last-written) value.
    let val = recover(&m);
    assert_eq!(
        val, 999,
        "recovered value should be the 999 written before the panic, got {val}"
    );
    println!("rung 5 ok: recovered poisoned value = {val}");
}

// ── Rung 6: Non-reentrancy — the self-deadlock ───────────────────────────────
// std::sync::Mutex is NOT recursive/reentrant. If a thread holds the guard and
// then tries to lock() the SAME mutex again, it blocks FOREVER waiting for...
// itself. (Some languages' locks are reentrant and count nesting; Rust's is not,
// because a reentrant lock would hand you a second &mut to data you already have
// a &mut to — aliasing UB. The non-reentrant lock is what keeps it sound.)
//
// We won't actually hang the test. Instead you'll PROVE the deadlock would
// happen using try_lock(), which returns immediately instead of blocking:
//   - try_lock() -> Ok(guard)        if the lock was free
//   - try_lock() -> Err(TryLockError::WouldBlock)  if it's already held
//
// Implement `would_self_deadlock`:
//   1. lock the mutex and KEEP the guard in a binding (don't drop it).
//   2. while still holding it, call m.try_lock() and observe it's Err.
//   3. return true if the second attempt failed (WouldBlock), false otherwise.
//      `blocking lock()` here would hang forever — that Err IS the deadlock,
//      caught in the act.
//
// Then explain to yourself (no code needed): a plain `m.lock()` on line 2 would
// hang the whole program. That's the bug; try_lock just makes it observable.
fn would_self_deadlock(m: &Mutex<i32>) -> bool {
    let _guard = m.lock().unwrap();
    let result = m.try_lock();
    result.is_err()
}

fn check_6() {
    let m = Mutex::new(7);
    assert!(
        would_self_deadlock(&m),
        "the second lock attempt while holding the guard should fail — if it succeeded you must have dropped the first guard"
    );

    assert!(
        m.try_lock().is_ok(),
        "lock should be free again after would_self_deadlock returns"
    );
    println!("rung 6 ok: proved the self-deadlock without hanging");
}

// ── Rung 7: Lock-ordering deadlock (ABBA) and the fix ────────────────────────
// The classic multi-lock deadlock. Two accounts, each behind its own Mutex.
// Thread 1 transfers A→B: locks A, then B. Thread 2 transfers B→A: locks B, then
// A. Run them at once and you can hit:
//     T1 holds A, waiting for B   ┐
//     T2 holds B, waiting for A   ┘  → neither can proceed. Deadlock. Hang.
// This is ABBA: a CYCLE in the "who-waits-for-whom" graph.
//
// THE FIX — a global lock ORDER. If every thread always acquires locks in the
// same order (here: by ascending account `id`), no cycle can form, so no
// deadlock — no matter the transfer direction. (Other strategies exist:
// try_lock-and-back-off, or a single coarse lock. Ordering is the standard one.)
//
// Implement `transfer_ordered(from, to, amt)`:
//   - Move `amt` from `from.balance` to `to.balance`.
//   - You must hold BOTH guards at once (a transfer is atomic), but you must
//     acquire them in a CANONICAL order: lock the account with the smaller `id`
//     FIRST, regardless of which is `from` vs `to`.
//   - Then mutate: *from_balance -= amt;  *to_balance += amt;
//
// Shape hint (you bind the guards to the right names in each branch):
//     if from.id < to.id {
//         let mut fg = from.balance.lock().unwrap();
//         let mut tg = to.balance.lock().unwrap();
//         ...
//     } else {
//         // lock `to` first (smaller id), but still subtract from `from`
//     }
// (Assume from.id != to.id here — a self-transfer would double-lock = rung 6.)
//
// The harness runs two threads doing 100k opposing transfers each, with a 5s
// WATCHDOG: if your ordering is wrong it will actually deadlock, and the watchdog
// prints a hint and exits instead of hanging your terminal forever.
struct Account {
    id: usize,
    balance: Mutex<i64>,
}

fn transfer_ordered(from: &Account, to: &Account, amt: i64) {
    if from.id < to.id {
        let mut fg = from.balance.lock().unwrap();
        let mut tg = to.balance.lock().unwrap();
        *fg -= amt;
        *tg += amt;
    } else {
        let mut tg = to.balance.lock().unwrap();
        let mut fg = from.balance.lock().unwrap();
        *fg -= amt;
        *tg += amt;
    }
}

fn check_7() {
    use std::sync::mpsc;
    use std::time::Duration;

    let a = Arc::new(Account {
        id: 0,
        balance: Mutex::new(1_000),
    });
    let b = Arc::new(Account {
        id: 1,
        balance: Mutex::new(1_000),
    });

    let (tx, rx) = mpsc::channel();

    // Thread 1: A → B (100k times)
    {
        let (a, b, tx) = (Arc::clone(&a), Arc::clone(&b), tx.clone());
        std::thread::spawn(move || {
            for _ in 0..100_000 {
                transfer_ordered(&a, &b, 1);
            }
            let _ = tx.send(());
        });
    }
    // Thread 2: B → A (100k times) — the OPPOSITE direction. With naive
    // lock-in-argument-order, this is the ABBA partner that deadlocks T1.
    {
        let (a, b, tx) = (Arc::clone(&a), Arc::clone(&b), tx.clone());
        std::thread::spawn(move || {
            for _ in 0..100_000 {
                transfer_ordered(&b, &a, 1);
            }
            let _ = tx.send(());
        });
    }
    drop(tx);

    // Watchdog: wait for both threads, but never longer than 5s total.
    for _ in 0..2 {
        if rx.recv_timeout(Duration::from_secs(5)).is_err() {
            eprintln!(
                "\n*** DEADLOCK DETECTED ***\nA transfer thread is stuck. Your two threads acquired the \
                 two locks in OPPOSITE orders (ABBA). Fix: always lock the lower `id` first, \
                 regardless of transfer direction.\n"
            );
            std::process::exit(1);
        }
    }

    let total = *a.balance.lock().unwrap() + *b.balance.lock().unwrap();
    assert_eq!(total, 2_000, "money created/destroyed: total = {total}");
    println!("rung 7 ok: 200k opposing transfers, no deadlock, total conserved = {total}");
}

// ── Rung 8: Mutex + Condvar — a bounded blocking queue ───────────────────────
// A Mutex lets you READ shared state safely, but it can't make you WAIT for that
// state to become a certain way. Busy-looping `while queue.lock().is_empty() {}`
// burns a core. The answer is a Condvar (condition variable): a parking lot tied
// to a Mutex where a thread can SLEEP until another thread notifies it.
//
// The one method that matters:
//     let guard = cv.wait(guard).unwrap();
// It ATOMICALLY (a) unlocks the mutex and parks the thread, then (b) on wakeup
// re-locks the mutex and hands the guard back. The atomic unlock-and-sleep is the
// whole point — it closes the race where you check the condition, then sleep, and
// miss a notify that lands in between.
//
// You will build a BoundedQueue<T> (capacity-limited):
//   - push(v): if the queue is FULL, wait until there's room, then push_back and
//     notify waiters (a popper might be waiting for an item).
//   - pop():   if the queue is EMPTY, wait until an item arrives, then pop_front
//     and notify waiters (a pusher might be waiting for room). Return the item.
//
// TWO RULES that define correct Condvar use:
//   1. WAIT IN A `while` LOOP, NEVER AN `if`. After wait() returns you only know
//      you were woken — not that the condition holds. Spurious wakeups happen,
//      AND another thread may have raced in and re-emptied/re-filled the queue
//      before you re-acquired the lock. Re-check the predicate in a loop:
//          while <bad condition> { guard = self.cv.wait(guard).unwrap(); }
//   2. NOTIFY AFTER YOU MUTATE. Once you've pushed/popped, call
//      self.cv.notify_all() (or notify_one) so a parked thread re-checks.
//
// One shared Condvar for both "not full" and "not empty" is fine here — waiters
// re-check their own predicate, and notify_all wakes everyone to re-test.
struct BoundedQueue<T> {
    inner: Mutex<VecDeque<T>>,
    cap: usize,
    cv: Condvar,
}

impl<T> BoundedQueue<T> {
    fn new(cap: usize) -> Self {
        BoundedQueue {
            inner: Mutex::new(VecDeque::new()),
            cap,
            cv: Condvar::new(),
        }
    }

    fn push(&self, v: T) {
        let mut guard = self.inner.lock().unwrap();
        while guard.len() == self.cap {
            guard = self.cv.wait(guard).unwrap();
        }
        guard.push_back(v);
        self.cv.notify_all();
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
}

fn check_8() {
    const N: usize = 1_000;
    // Capacity 2 is deliberately tiny so the producer MUST block and wait for the
    // consumer — exercising the Condvar, not just the Mutex.
    let q = Arc::new(BoundedQueue::<usize>::new(2));

    let producer = {
        let q = Arc::clone(&q);
        std::thread::spawn(move || {
            for i in 0..N {
                q.push(i);
            }
        })
    };

    let consumer = {
        let q = Arc::clone(&q);
        std::thread::spawn(move || {
            let mut got = Vec::with_capacity(N);
            for _ in 0..N {
                got.push(q.pop());
            }
            got
        })
    };

    producer.join().unwrap();
    let got = consumer.join().unwrap();

    assert_eq!(got.len(), N, "consumer should receive exactly N items");
    assert!(
        got.iter().copied().eq(0..N),
        "items should arrive in FIFO order 0..N — a single producer/consumer over a queue preserves order"
    );
    println!("rung 8 ok: bounded(2) queue moved {N} items in order via Condvar wait/notify");
}

// ── Rung 9 (capstone): a concurrent, deadlock-free, poison-tolerant Bank ──────
// Synthesize the whole ladder. A Bank holds N accounts, each an independent
// Mutex<i64> (fine-grained locking — two transfers over disjoint accounts run in
// parallel, unlike one coarse Mutex<Vec>). Many threads hammer it with random
// transfers. It must:
//   • never deadlock        (lock ordering, rung 7)
//   • never self-deadlock   (reject same-account transfers, rung 6)
//   • survive a poisoned account (recover, rung 5)
//   • conserve money        (correct guard discipline, rung 3)
//
// You implement THREE methods. `new`, `poison_account`, and the test harness are
// written for you.
//
// 1. lock_recover(m) -> MutexGuard<i64>
//    A poison-tolerant lock. The harness deliberately poisons one account; if you
//    .unwrap() a poisoned lock your worker thread panics. Recover the guard from a
//    PoisonError instead (rung 5's into_inner trick) so every access still works.
//
// 2. transfer(from, to, amt) -> Result<(), TransferError>
//    - Err(SameAccount) if from == to (locking one Mutex twice = self-deadlock).
//    - Err(NoSuchAccount) if either index is out of range.
//    - Acquire BOTH guards (via lock_recover) in CANONICAL ORDER: lower index
//      first, regardless of direction. Bind them so you still subtract from
//      `from` and add to `to`.
//    - Err(InsufficientFunds { have, need }) if the `from` balance < amt — and
//      do NOT mutate in that case (no overdraft, money stays conserved).
//    - Otherwise move the funds and return Ok(()).
//
// 3. total() -> i64
//    Sum every account's balance (using lock_recover). The harness calls this
//    before and after the storm; both must equal the starting total.
#[derive(Debug, PartialEq)]
enum TransferError {
    SameAccount,
    NoSuchAccount,
    InsufficientFunds { have: i64, need: i64 },
}

struct Bank {
    accounts: Vec<Mutex<i64>>,
}

impl Bank {
    fn new(n: usize, start: i64) -> Self {
        Bank {
            accounts: (0..n).map(|_| Mutex::new(start)).collect(),
        }
    }

    // provided: poison account `i` WITHOUT changing its balance — a thread locks
    // it and panics; the guard's Drop during unwind sets the poison flag. We catch
    // the panic inside a scoped thread so it doesn't propagate here.
    fn poison_account(&self, i: usize) {
        std::thread::scope(|s| {
            s.spawn(|| {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _g = self.accounts[i].lock().unwrap();
                    panic!("intentionally poisoning account {i}");
                }));
            });
        });
    }

    fn lock_recover(m: &Mutex<i64>) -> MutexGuard<'_, i64> {
        match m.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        }
    }

    fn transfer(&self, from: usize, to: usize, amt: i64) -> Result<(), TransferError> {
        if from == to {
            return Err(TransferError::SameAccount);
        }
        if from >= self.accounts.len() || to >= self.accounts.len() {
            return Err(TransferError::NoSuchAccount);
        }
        let mut fg = Self::lock_recover(&self.accounts[from]);
        let mut tg = Self::lock_recover(&self.accounts[to]);
        if *fg < amt {
            return Err(TransferError::InsufficientFunds {
                have: *fg,
                need: amt,
            });
        }
        *fg -= amt;
        *tg += amt;
        Ok(())
    }

    fn total(&self) -> i64 {
        self.accounts.iter().map(|m| *Self::lock_recover(m)).sum()
    }
}

fn check_9() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    const N: usize = 8;
    const START: i64 = 1_000;
    const THREADS: usize = 8;
    const OPS: usize = 50_000;
    let expected_total = N as i64 * START;

    let bank = Arc::new(Bank::new(N, START));
    assert_eq!(bank.total(), expected_total, "starting total wrong");

    // Poison one account up front: every transfer that touches it must recover.
    bank.poison_account(3);

    // Watchdog: if a wrong lock order deadlocks, the worker joins below hang
    // forever — this fires after 8s and exits with a hint instead.
    let done = Arc::new(AtomicBool::new(false));
    {
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            let start = Instant::now();
            while !done.load(Ordering::Relaxed) {
                if start.elapsed() > Duration::from_secs(8) {
                    eprintln!(
                        "\n*** DEADLOCK in the Bank ***\nWorkers are stuck — transfers acquired the two \
                         account locks in inconsistent orders. Lock the lower INDEX first, always.\n"
                    );
                    std::process::exit(1);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
    }

    let ok = Arc::new(AtomicUsize::new(0));
    let denied = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for t in 0..THREADS {
        let bank = Arc::clone(&bank);
        let ok = Arc::clone(&ok);
        let denied = Arc::clone(&denied);
        handles.push(std::thread::spawn(move || {
            // tiny per-thread xorshift PRNG — deterministic, no deps
            let mut state = 0x9E3779B97F4A7C15u64 ^ (t as u64 + 1).wrapping_mul(0xD1B54A32D192ED03);
            let mut next = || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state
            };
            for _ in 0..OPS {
                let from = (next() % N as u64) as usize;
                let to = (next() % N as u64) as usize;
                let amt = (next() % 50) as i64 + 1;
                match bank.transfer(from, to, amt) {
                    Ok(()) => {
                        ok.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        denied.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }));
    }

    // If a worker panicked (e.g. .unwrap() on the poisoned account), join is Err.
    for h in handles {
        if h.join().is_err() {
            done.store(true, Ordering::Relaxed);
            panic!(
                "a worker thread PANICKED — did you handle the poisoned account? \
                 lock_recover must recover from PoisonError, not .unwrap()"
            );
        }
    }
    done.store(true, Ordering::Relaxed);

    // The invariant: money is conserved no matter how the transfers interleaved.
    let total = bank.total();
    assert_eq!(
        total, expected_total,
        "money was created or destroyed: total = {total}, expected {expected_total}"
    );
    // No account went negative (InsufficientFunds must block overdrafts).
    for i in 0..N {
        let bal = *Bank::lock_recover(&bank.accounts[i]);
        assert!(bal >= 0, "account {i} went negative: {bal}");
    }

    println!(
        "rung 9 ok: {} threads × {} ops, {} transfers applied / {} denied, \
         poisoned account survived, total conserved = {total}",
        THREADS,
        OPS,
        ok.load(Ordering::Relaxed),
        denied.load(Ordering::Relaxed),
    );
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
}
