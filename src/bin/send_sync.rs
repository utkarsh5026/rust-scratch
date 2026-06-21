// Send & Sync deeply — what they really guarantee, why Rc is !Send
//
// Run: cargo run --bin send_sync
//
// The mental model:
//   T: Send  => safe to MOVE ownership of a T to another thread.
//   T: Sync  => safe to SHARE &T between threads.  Precisely: T: Sync  <=>  &T: Send.
// Both are AUTO traits: the compiler implements them structurally from your
// fields. You rarely write `impl` for them (rung 8/9 is where you do, unsafely).
//
// Ladder (DONE marked):
//   1. [x] foundations — spawn requires Send: move owned data into a thread
//   2. [x] foundations — Sync = shareable &T: share &data across scoped threads
//   3. [x] mechanics   — auto-derivation is structural (struct is Send iff all fields are)
//   4. [x] mechanics   — probe the std library: predict then verify Send/Sync
//   5. [x] footgun     — why Rc is !Send: the data race the rule prevents
//   6. [x] footgun     — the four quadrants (Cell/RefCell, MutexGuard)
//   7. [x] real-world  — Arc<Mutex<T>> vs Rc<RefCell<T>> across spawn
//   8. [x] real-world  — unsafe impl Send escape hatch + PhantomData opt-out
//   9. [x] capstone    — SpinLock<T> from scratch (unsafe impl Sync where T: Send)

use std::thread;

// ─────────────────────────────────────────────────────────────────────────────
// Problem 1 (foundations): spawn requires Send.
//
// `thread::spawn` has the signature (roughly):
//     pub fn spawn<F, T>(f: F) -> JoinHandle<T>
//     where F: FnOnce() -> T + Send + 'static, T: Send + 'static
//
// The closure (and everything it captures by move) must be `Send` — because it
// is literally moved onto another thread. Here you'll do that move yourself.
//
// Your turn: implement `sum_on_thread`. It should take an owned Vec<i64>, move
// it into a spawned thread, compute the sum THERE, and return the sum to the
// caller by `join`-ing the handle. (No borrowing — own the data and move it.)
// ─────────────────────────────────────────────────────────────────────────────
fn sum_on_thread(data: Vec<i64>) -> i64 {
    thread::spawn(move || data.iter().sum::<i64>())
        .join()
        .unwrap()
}

fn check_1() {
    let v = vec![1, 2, 3, 4, 5];
    assert_eq!(sum_on_thread(v), 15);
    println!("check_1 ok: owned Vec moved into a thread (it was Send) and summed");
}

// ─────────────────────────────────────────────────────────────────────────────
// Problem 2 (foundations): Sync = a reference you can share across threads.
//
// Rung 1 MOVED owned data to one thread. Now you want SEVERAL threads to read
// the SAME data at once, through shared `&` references. That's what `Sync` is
// for: `T: Sync` means `&T` is safe to hand to another thread — formally,
//     T: Sync   <=>   &T: Send
//
// `thread::scope` lets borrows (not just 'static) cross into threads, because
// the scope guarantees all spawned threads finish before the borrowed data dies.
// Each `s.spawn(...)` closure here will capture `&data` — so the bound that must
// hold is `Vec<i64>: Sync` (it is: shared read access to a Vec is fine).
//
// Your turn: implement `parallel_contains`. Given a slice and a list of needles,
// spawn ONE scoped thread per needle, each searching the SAME shared `haystack`
// by reference, and return a Vec<bool> (same order as `needles`) of whether each
// needle was found. No cloning the haystack, no Mutex — just shared &reads.
// ─────────────────────────────────────────────────────────────────────────────
fn parallel_contains(haystack: &[i64], needles: &[i64]) -> Vec<bool> {
    thread::scope(|s| {
        let mut handles = Vec::with_capacity(needles.len());
        for needle in needles {
            handles.push(s.spawn(move || haystack.contains(needle)));
        }
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

fn check_2() {
    let hay = vec![10, 20, 30, 40, 50];
    let needles = vec![30, 99, 10, 7];
    assert_eq!(
        parallel_contains(&hay, &needles),
        vec![true, false, true, false]
    );
    println!("check_2 ok: many threads shared &haystack at once (Vec<i64>: Sync, so &Vec: Send)");
}

// ─────────────────────────────────────────────────────────────────────────────
// Problem 3 (mechanics): auto-derivation is STRUCTURAL.
//
// Send and Sync are auto traits: the compiler implements them for your type
// automatically IFF every field is also Send / Sync. There is no `derive(Send)`;
// it's inferred from layout. Add one non-Send field and the whole struct stops
// being Send — like a single rotten apple.
//
// The classic way to *test* a marker bound at compile time is a generic helper
// that only accepts types satisfying the bound. If the call compiles, the bound
// holds. These are your probes for the rest of the ladder.
//
// Your turn (two parts):
//   (a) Implement the two probe helpers `assert_send::<T>()` and
//       `assert_sync::<T>()`. Each takes no args, returns nothing; the WHOLE
//       point is the bound in its signature. (Bodies are empty.)
//   (b) Make the struct `Telemetry` below derive nothing special but BE both
//       Send and Sync by choosing field types that already are. Fill in the two
//       field types where marked so `check_3` (which calls the probes on
//       Telemetry) compiles. Keep the fields meaningfully typed (a counter and
//       a label), not unit.
// ─────────────────────────────────────────────────────────────────────────────

fn assert_send<T: Send>() {}

fn assert_sync<T: Sync>() {}

struct Telemetry {
    #[allow(unused)]
    count: u64,
    #[allow(unused)]
    label: String,
}

fn check_3() {
    // These calls compile ONLY if Telemetry is Send and Sync, which it is ONLY
    // if every field is. That's the structural rule, enforced by the compiler.
    assert_send::<Telemetry>();
    assert_sync::<Telemetry>();

    // Sanity: primitives and owned collections are Send+Sync too.
    assert_send::<i32>();
    assert_sync::<String>();
    assert_send::<Vec<u8>>();

    let _t = Telemetry {
        count: 0,
        label: String::new(),
    };
    println!(
        "check_3 ok: Telemetry is Send+Sync because all its fields are (structural derivation)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Problem 4 (mechanics): probe the std library — PREDICT, then VERIFY.
//
// Your probes prove POSITIVES: `assert_send::<T>()` compiles  =>  T: Send.
// There is no stable "negative bound", so you witness NEGATIVES differently:
// uncomment a probe that SHOULD fail and read the compiler's explanation, then
// re-comment it so the file builds again. The error message itself is the lesson
// ("`Rc<...>` cannot be sent between threads safely").
//
// STEP 1 — fill in your prediction for each type (Y / N) BEFORE touching code:
//
//   type              Send?   Sync?
//   i32                yes       yes
//   String             yes       yes
//   &i32               yes       yes
//   Box<i32>           yes       yes
//   Rc<i32>            no        no
//   Arc<i32>           yes       yes
//   Cell<i32>          yes       no
//   RefCell<i32>       yes       no
//   Mutex<i32>         yes       yes
//   *const i32         no        no
//
// STEP 2 — implement `check_4`:
//   (a) For every type you predicted Send, add an `assert_send::<T>();` call.
//       For every type you predicted Sync, add an `assert_sync::<T>();` call.
//       If a call you expected to compile does NOT, your prediction was wrong —
//       fix the prediction, understand why, move on.
//   (b) Leave the NEGATIVE witnesses in the commented block as-is. Uncomment
//       each ONE AT A TIME, run `cargo build --bin send_sync`, read the error,
//       then re-comment. (Keep them commented for the final passing run.)
// ─────────────────────────────────────────────────────────────────────────────
use std::cell::{Cell, RefCell};
use std::sync::{Arc, Mutex};

fn check_4() {
    // Negative witnesses — uncomment one at a time, read the error, re-comment.
    // assert_send::<Rc<i32>>();
    // assert_sync::<Rc<i32>>();
    // assert_sync::<Cell<i32>>();
    // assert_sync::<RefCell<i32>>();
    // assert_send::<*const i32>();
    // assert_sync::<*const i32>();

    assert_send::<i32>();
    assert_sync::<i32>();

    assert_send::<String>();
    assert_sync::<String>();

    assert_send::<&i32>();
    assert_sync::<&i32>();

    assert_send::<Box<i32>>();
    assert_sync::<Box<i32>>();

    assert_send::<Arc<i32>>();
    assert_sync::<Arc<i32>>();

    assert_send::<Cell<i32>>();
    assert_send::<RefCell<i32>>();
    assert_send::<Mutex<i32>>();
    assert_sync::<Mutex<i32>>();

    println!("check_4 ok: predictions verified against the compiler");
}

// ─────────────────────────────────────────────────────────────────────────────
// Problem 5 (footgun): WHY is `Rc` !Send? Reproduce the exact race it prevents.
//
// `Rc::clone` is, in essence:   self.count += 1   on a PLAIN integer.
// `Arc::clone` is:              self.count.fetch_add(1, ...)   — an ATOMIC RMW.
// If an Rc could be shared across threads, two threads cloning at once would do
// a non-atomic read-modify-write on the same counter and LOSE updates. A lost
// increment means the count reads too low -> the value is freed while a clone is
// still alive -> use-after-free / double-free. THAT is the race `!Send` forbids
// at compile time, before it can ever happen.
//
// You can't actually share an Rc across threads (the compiler stops you — you
// witnessed that in rung 4). So here you reproduce the *mechanism* directly on a
// shared atomic, doing the increment two ways:
//
// Your turn — implement both, using `thread::scope` + a shared `&AtomicUsize`:
//   (a) `count_racy(n_threads, iters)`: each thread loops `iters` times doing a
//       NON-atomic-style RMW — `let v = c.load(Relaxed); c.store(v + 1, Relaxed);`
//       (a load, then a separate store). This mimics `Rc`'s `count += 1`.
//       Return the final counter value.
//   (b) `count_atomic(n_threads, iters)`: same loop but a single
//       `c.fetch_add(1, Relaxed)`. This mimics `Arc`'s atomic clone.
//       Return the final counter value.
//
// check_5 asserts the atomic version is ALWAYS exact, and shows the racy version
// losing updates (final < expected) — the corrupted refcount in miniature.
// ─────────────────────────────────────────────────────────────────────────────
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

fn count_racy(n_threads: usize, iters: usize) -> usize {
    let c = &AtomicUsize::new(0);
    thread::scope(|s| {
        let mut handles = Vec::with_capacity(n_threads);
        for _ in 0..n_threads {
            handles.push(s.spawn(move || {
                for _ in 0..iters {
                    let v = c.load(Relaxed);
                    c.store(v + 1, Relaxed);
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
    });
    c.load(Relaxed)
}

fn count_atomic(n_threads: usize, iters: usize) -> usize {
    let c = &AtomicUsize::new(0);
    thread::scope(|s| {
        let mut handles = Vec::with_capacity(n_threads);
        for _ in 0..n_threads {
            handles.push(s.spawn(move || {
                for _ in 0..iters {
                    c.fetch_add(1, Relaxed);
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
    });
    c.load(Relaxed)
}

fn check_5() {
    let (threads, iters) = (8, 50_000);
    let expected = threads * iters;

    let atomic = count_atomic(threads, iters);
    assert_eq!(
        atomic, expected,
        "atomic RMW must never lose an update (this is Arc)"
    );

    let racy = count_racy(threads, iters);
    assert!(racy <= expected);
    println!(
        "check_5 ok: atomic={atomic} (exact, = Arc), racy={racy} lost {} updates \
         (= the Rc refcount corruption !Send prevents)",
        expected - racy
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Problem 6 (footgun): the FOUR QUADRANTS. Send and Sync are INDEPENDENT axes.
//
// A type lands in one of four boxes. You've met three; this rung makes you
// justify every cell — especially the two that feel backwards.
//
//                 Sync (can share &T)        !Sync (cannot share &T)
//   Send       │  i32, String, Mutex<T>,   │  Cell<T>, RefCell<T>
//   (can move) │  Arc<T>                    │  (move whole thing: fine;
//              │                            │   share &: data race)
//   ───────────┼────────────────────────────┼──────────────────────────────
//   !Send      │  MutexGuard<'_, T>         │  Rc<T>, *const T,
//   (cannot    │  (the ONE that's Sync but  │  *mut T
//    move)     │   not Send)                │
//
// Two genuinely surprising types:
//
//  • Cell<T>/RefCell<T> are Send but !Sync. Moving the whole cell to another
//    thread (exclusive ownership, one accessor) is fine. SHARING &Cell would let
//    two threads `.set()` concurrently with no synchronization — a data race.
//    So: Send yes, Sync no.
//
//  • std::sync::MutexGuard is !Send but Sync. It must be DROPPED (unlocked) on
//    the SAME thread that locked it — many platforms' mutexes require the locking
//    thread to unlock — so you must not MOVE the guard to another thread (!Send).
//    But handing out a `&MutexGuard` (which derefs to `&T`) to another thread is
//    fine as long as T: Sync (Sync yes). It's the canonical Sync-but-not-Send.
//
// A corollary worth feeling: `&T: Send  <=>  T: Sync`. So `&Cell<i32>` is NOT
// Send (because Cell isn't Sync), even though `Cell` itself IS Send.
//
// Your turn: implement `check_6` to PROVE the positive cells with your probes,
// and WITNESS the negatives. Specifically:
//   (a) assert_send + assert_sync for: i32, Mutex<i32>, Arc<i32>.
//   (b) assert_send (only) for: Cell<i32>, RefCell<i32>.   (they're !Sync)
//   (c) assert_sync (only) for: Cell<i32>? NO — that won't compile. Instead
//       prove the corollary: assert_send::<&Mutex<i32>>() compiles (Mutex: Sync)
//       but the matching line for &Cell is a NEGATIVE witness.
//   (d) In the commented NEGATIVE block, uncomment each line one at a time, read
//       the error, re-comment:
//         - assert_sync::<Cell<i32>>()        // Cell !Sync
//         - assert_send::<&Cell<i32>>()       // &Cell !Send  (because Cell !Sync)
//         - assert_send::<MutexGuard<i32>>()  // guard !Send
//       and CONFIRM the guard is Sync with a positive: assert_sync::<MutexGuard<i32>>()
// ─────────────────────────────────────────────────────────────────────────────
use std::sync::MutexGuard;

fn check_6() {
    // ── NEGATIVE witnesses: uncomment one at a time, read the error, re-comment.
    // assert_sync::<Cell<i32>>();
    // assert_send::<&Cell<i32>>();
    // assert_send::<MutexGuard<i32>>();

    assert_send::<i32>();
    assert_sync::<i32>();
    assert_send::<Mutex<i32>>();
    assert_sync::<Mutex<i32>>();
    assert_send::<Arc<i32>>();
    assert_sync::<Arc<i32>>();
    assert_send::<Cell<i32>>();
    assert_send::<RefCell<i32>>();
    assert_sync::<MutexGuard<i32>>();

    #[allow(unreachable_code)]
    {
        println!("check_6 ok: the four quadrants — Send and Sync are independent axes");
    }
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

// ─────────────────────────────────────────────────────────────────────────────
// Problem 9 (CAPSTONE): build a SpinLock<T> from scratch.
//
// This is the synthesis: a lock you write yourself proves you own the whole
// Send/Sync mental model. A SpinLock is a Mutex that "busy-waits" (spins) on an
// AtomicBool instead of parking the thread. You'll provide the interior
// mutability AND the unsafe trait impls that make it shareable.
//
// The pieces:
//   struct SpinLock<T> { locked: AtomicBool, value: UnsafeCell<T> }
//
// WHY UnsafeCell: it's the ONLY legal way to get a `&mut T` from a `&self`.
//   Every interior-mutability type (Cell, RefCell, Mutex, atomics) is built on
//   it. A plain field behind `&self` can never yield `&mut`. UnsafeCell is also
//   the thing that makes a type !Sync by default — so we must opt back in.
//
// THE KEY BOUND (the one fact this whole ladder builds to):
//   unsafe impl<T> Sync for SpinLock<T> where T: Send {}
//   - We need Sync so `&SpinLock<T>` can be shared across threads (the point).
//   - The bound is `T: Send`, NOT `T: Sync`. Think hard about WHY:
//       The lock guarantees only ONE thread touches the T at a time (mutual
//       exclusion). So the T is effectively MOVED between threads (handed off),
//       never simultaneously SHARED. "Moved between threads" = Send. We never
//       need T: Sync because two threads never hold &T at once. This is exactly
//       why std::sync::Mutex<T>: Sync requires only T: Send.
//
// ── Your turn — implement the four todo!s below:
//   (1) `lock(&self) -> SpinGuard<'_, T>`: spin with compare_exchange on `locked`
//       until you win (false -> true). Acquire ordering on success. Then return a
//       guard. Use `std::hint::spin_loop()` while waiting.
//   (2) `SpinGuard` Deref/DerefMut: hand out &T / &mut T via the UnsafeCell ptr.
//   (3) `SpinGuard` Drop: release the lock (store false, Release ordering).
//   (4) the two unsafe impls with correct bounds + SAFETY comments.
//
// check_9 shares ONE &SpinLock across many scoped threads, each locking to
// increment — and the total must be exact (the lock prevents the rung-5 race).
// ─────────────────────────────────────────────────────────────────────────────
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering::Acquire, Ordering::Release};

struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

// (4) your turn: the two unsafe impls.
//   - SpinLock<T> must be Send when ... ?   (you can move the lock if you can move T)
//   - SpinLock<T> must be Sync when ... ?   (the KEY bound — see the header)

// SAFETY: Moving the lock moves its contained T. That is sound exactly when T
// itself may be moved to another thread.
unsafe impl<T: Send> Send for SpinLock<T> {}

// SAFETY: The atomic flag gives mutual exclusion, so shared &SpinLock<T> access
// never allows two threads to access T at the same time. The protected value is
// handed from one locking thread to the next, so T only needs to be Send.
unsafe impl<T: Send> Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
    fn new(value: T) -> Self {
        SpinLock {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    fn lock(&self) -> SpinGuard<'_, T> {
        loop {
            if self
                .locked
                .compare_exchange(false, true, Acquire, Relaxed)
                .is_ok()
            {
                return SpinGuard { lock: self };
            }
            std::hint::spin_loop();
        }
    }
}

struct SpinGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> Deref for SpinGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: A SpinGuard exists only after acquiring the lock, so no mutable
        // reference to the protected value can coexist with this shared reference.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for SpinGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: We hold the lock, and `&mut self` guarantees this guard is the
        // only guard currently yielding access through this call.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for SpinGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Release);
    }
}

fn check_9() {
    // ── uncomment everything below once the impls are in ──────────────────────
    // // probe: a SpinLock<i32> must be Send AND Sync (i32 is Send).
    assert_send::<SpinLock<i32>>();
    assert_sync::<SpinLock<i32>>();

    let counter = SpinLock::new(0_u64);
    let n_threads = 8;
    let per = 10_000;

    thread::scope(|s| {
        for _ in 0..n_threads {
            // every thread shares the SAME &SpinLock — only legal because it's Sync
            let c = &counter;
            s.spawn(move || {
                for _ in 0..per {
                    let mut guard = c.lock();
                    *guard += 1;
                } // guard drops here -> unlock
            });
        }
    });

    let total = *counter.lock();
    assert_eq!(
        total,
        (n_threads * per) as u64,
        "the spinlock must serialize all increments"
    );

    #[allow(unreachable_code)]
    {
        println!(
            "check_9 ok: hand-rolled SpinLock<T> (UnsafeCell + AtomicBool + RAII guard); \
                  Sync requires only T: Send because the lock hands T between threads, never shares it"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Problem 8 (real-world): overriding the auto-derive — opt OUT and opt IN.
//
// Auto traits are inferred, but you can override the inference in BOTH directions.
//
// OPT OUT (safe): add a field whose type isn't Send/Sync and your struct loses
// the trait — even if logically it'd be fine. The zero-size way to do this on
// purpose is `PhantomData<*const ()>` (a raw pointer is !Send and !Sync, and
// PhantomData makes the struct "act as if" it owns one, with no runtime cost).
// This is how you build a type that MUST stay on one thread (e.g. a handle tied
// to a thread-local / FFI context).
//
// OPT IN (unsafe): when a type contains a raw pointer it is !Send/!Sync by
// default, because the compiler can't know the pointer is used safely. If YOU
// know the access is actually sound, you promise it with `unsafe impl Send`.
// This is exactly how Arc, Vec, Box, channels, etc. get their Send/Sync impls.
// The `unsafe` means: "compiler, I take responsibility for the invariant."
//
// ── Part A (opt out): make `ThreadBound` NOT Send and NOT Sync, at zero cost.
//    It holds a real `id: u32` plus a marker. Add the right PhantomData field.
//
// ── Part B (opt in): `Buffer` wraps a raw `*mut u8` + len. As written it's
//    !Send (raw ptr). The buffer uniquely OWNS its allocation, so MOVING it to
//    another thread is sound. Write `unsafe impl Send for Buffer {}` and fill in
//    the SAFETY comment with the invariant that makes it sound. (We deliberately
//    do NOT impl Sync — shared &Buffer access isn't synchronized.)
// ─────────────────────────────────────────────────────────────────────────────
use std::marker::PhantomData;

#[allow(unused)]
struct ThreadBound {
    #[allow(unused)]
    id: u32,
    _pd: PhantomData<*const ()>,
    // your turn: add a PhantomData field that makes this type !Send and !Sync
}

struct Buffer {
    ptr: *mut u8,
    len: usize,
}

impl Buffer {
    fn new(len: usize) -> Self {
        // allocate `len` zeroed bytes via a Vec, then leak it to a raw pointer
        let mut v = vec![0u8; len];
        let ptr = v.as_mut_ptr();
        std::mem::forget(v); // ownership now lives in `ptr` (we free it in Drop)
        Buffer { ptr, len }
    }
    fn first(&self) -> u8 {
        assert!(self.len > 0, "cannot read first byte of an empty Buffer");
        // SAFETY: `ptr` came from a `Vec<u8>` allocation of length `len` and is
        // kept alive until Drop. The assertion above guarantees `len > 0`, so
        // reading the first initialized byte is within bounds.
        unsafe { *self.ptr }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: `ptr` was produced by a `Vec<u8>` with length and capacity
        // equal to `len`, then forgotten. This `Buffer` is the unique owner, so
        // reconstructing that Vec here frees the allocation exactly once.
        unsafe {
            drop(Vec::from_raw_parts(self.ptr, self.len, self.len));
        }
    }
}

// SAFETY: Buffer uniquely owns the allocation described by ptr/len.
// Moving it to another thread transfers that ownership; no aliases are exposed,
// and Drop reconstructs and frees the allocation exactly once.
unsafe impl Send for Buffer {}

fn check_8() {
    // assert_send::<ThreadBound>();
    // assert_sync::<ThreadBound>();
    assert_send::<Buffer>();

    // Part A: these NEGATIVE witnesses must FAIL to compile until you add the
    // PhantomData; once added, KEEP THEM COMMENTED (they should stay failing).

    // Part B: uncomment these once your `unsafe impl Send for Buffer` is in.
    // assert_send::<Buffer>();
    //
    // // Move a Buffer into another thread and read from it there — only legal
    // // because Buffer: Send.
    let buf = Buffer::new(8);
    let got = thread::spawn(move || buf.first()).join().unwrap();
    assert_eq!(got, 0);

    #[allow(unreachable_code)]
    {
        println!("check_8 ok: PhantomData opted ThreadBound OUT; unsafe impl Send opted Buffer IN");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Problem 7 (real-world): the shared-mutable-state workhorse.
//
// You now have the pieces to explain Rust's #1 concurrency idiom by composition:
//
//   Rc<RefCell<T>>   — single-threaded shared mutability.
//     Rc:      Send? NO  Sync? NO   (non-atomic refcount, rung 5)
//     RefCell: Send YES  Sync? NO   (non-atomic borrow flag, rung 6)
//     => the whole thing is NEITHER Send nor Sync. Cannot cross a thread.
//
//   Arc<Mutex<T>>    — multi-threaded shared mutability.
//     Arc:     Send YES Sync YES  (when T: Send+Sync)  (atomic refcount)
//     Mutex:   Send YES Sync YES  (when T: Send)       (real lock)
//     => Send + Sync. Clone the Arc, move a clone into each thread, lock to mutate.
//
// The swap from Rc<RefCell> to Arc<Mutex> is EXACTLY swapping the non-atomic
// machinery for atomic/locked machinery — and the marker traits flip as a result.
//
// Your turn — two parts:
//
//  (a) Implement `concurrent_sum(values: Vec<i64>, n_threads) -> i64` using
//      `Arc<Mutex<i64>>` as a shared accumulator. Split `values` into n_threads
//      chunks, spawn a thread per chunk (use std::thread::spawn — NOT scope, so
//      the closures must be 'static + Send), each thread locks the accumulator
//      and adds its chunk's partial sum. Join all, return the total.
//      (Clone the Arc before moving each clone into its thread.)
//
//  (b) Prove the marker story with probes:
//        assert_send + assert_sync for Arc<Mutex<i64>>
//      and witness (commented negatives) that Rc<RefCell<i64>> is neither:
//        // assert_send::<Rc<RefCell<i64>>>();
//        // assert_sync::<Rc<RefCell<i64>>>();
// ─────────────────────────────────────────────────────────────────────────────
fn concurrent_sum(values: Vec<i64>, n_threads: usize) -> i64 {
    if values.is_empty() || n_threads == 0 {
        return 0;
    }

    let accumulator = Arc::new(Mutex::new(0));
    let chunk_len = values.len().div_ceil(n_threads);
    let mut handles = Vec::new();

    for chunk in values.chunks(chunk_len) {
        let accumulator = Arc::clone(&accumulator);
        let chunk = chunk.to_vec();
        handles.push(thread::spawn(move || {
            let partial = chunk.into_iter().sum::<i64>();
            let mut total = accumulator.lock().unwrap();
            *total += partial;
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    *accumulator.lock().unwrap()
}

fn check_7() {
    let values: Vec<i64> = (1..=1000).collect();
    let total = concurrent_sum(values, 4);
    assert_eq!(total, 500_500);
    assert_eq!(concurrent_sum(Vec::new(), 4), 0);
    assert_eq!(concurrent_sum(vec![1, 2, 3], 0), 0);

    // (b) marker proof:
    assert_send::<Arc<Mutex<i64>>>();
    assert_sync::<Arc<Mutex<i64>>>();
    // Negatives — uncomment to witness, then re-comment:
    // assert_send::<Rc<RefCell<i64>>>();
    // assert_sync::<Rc<RefCell<i64>>>();

    println!(
        "check_7 ok: Arc<Mutex<T>> is Send+Sync and shares mutable state across threads; \
              Rc<RefCell<T>> is neither"
    );
}
