// Threads & scoped threads — std::thread, thread::scope, JoinHandle
// Run: cargo run --bin threads
//
// Mental model: thread::spawn launches a thread that may OUTLIVE the spawning
// function, so its closure must own everything (`'static`) and you collect its
// result via JoinHandle::join. thread::scope GUARANTEES all spawned threads
// finish before the scope returns, so threads may safely BORROW locals.
//
// Ladder:
//   1. [ ] Spawn & join            — launch a thread, get a value back via join()
//   2. [ ] Many handles            — spawn N threads, collect results in order
//   3. [ ] move & ownership        — send owned data into a thread
//   4. [ ] Panicking threads       — join() returns Err; inspect the payload
//   5. [ ] The 'static wall        — borrowing a local in spawn fails (E0373)
//   6. [ ] thread::scope rescue    — same borrow, now legal; shared read
//   7. [ ] Scoped parallel mutate  — split &mut [i32], mutate disjoint chunks
//   8. [ ] Parallel fold / fan-in  — partial sums in threads, combine in main
//   9. [ ] Capstone: parallel_map  — split slice across scoped workers, reorder

use std::thread;

// ── Rung 1: Spawn & join ────────────────────────────────────────────────────
// Spawn a thread that computes 2 + 2 and returns it. Back in the calling
// function, join the handle and return the value the thread produced.
//
// Hints in your head: thread::spawn(|| { ... }) returns a JoinHandle<T> where T
// is whatever the closure returns. handle.join() gives you Result<T, _>.
fn spawn_and_join() -> i32 {
    let handle = thread::spawn(|| 2 + 2);
    handle.join().unwrap()
}

fn check_1() {
    assert_eq!(spawn_and_join(), 4);
    println!("rung 1 ok: spawned a thread and joined its result");
}

// ── Rung 2: Many handles ─────────────────────────────────────────────────────
// Spawn `n` threads where thread i (0..n) computes i * i. Collect the squares
// into a Vec, in order: [0, 1, 4, 9, 16, ...].
//
// The trap: don't join inside the spawn loop — that serializes them. Spawn ALL
// handles into a Vec first, THEN join them in a second pass.
fn squares_in_parallel(n: usize) -> Vec<usize> {
    let mut handles = Vec::with_capacity(n);
    for i in 0..n {
        handles.push(thread::spawn(move || i * i));
    }
    handles.into_iter().map(|h| h.join().unwrap()).collect()
}

fn check_2() {
    assert_eq!(squares_in_parallel(5), vec![0, 1, 4, 9, 16]);
    assert_eq!(squares_in_parallel(0), Vec::<usize>::new());
    println!("rung 2 ok: spawned many threads and collected results in order");
}

// ── Rung 3: move & ownership ─────────────────────────────────────────────────
// Take an owned String, hand it to a thread that pushes " world" onto it, and
// return the result via join. The point: the closure must OWN `s` (move it in)
// because the thread may outlive this function — that's the 'static contract.
//
// Try writing the closure WITHOUT `move` first and read the error, then fix it.
fn append_in_thread(s: String) -> String {
    let handle = thread::spawn(move || s + " world");
    handle.join().unwrap()
}

fn check_3() {
    assert_eq!(append_in_thread(String::from("hello")), "hello world");
    println!("rung 3 ok: moved owned data into a thread");
}

// ── Rung 4: Panicking threads ────────────────────────────────────────────────
// A panic in a spawned thread is CAUGHT — it doesn't unwind into the parent.
// That's why join() -> Result: Ok(value) if it finished, Err(payload) if it
// panicked. Spawn a thread that does `panic!("boom")`, then in the Err arm
// recover the message string and return it.
//
// The payload is a Box<dyn Any + Send>. A panic!("literal") stores a &'static
// str; downcast the payload's &reference to that type to read it back.
fn catch_panic_message() -> String {
    let handle = thread::spawn(|| {
        panic!("boom");
    });
    match handle.join() {
        Ok(value) => value,
        Err(payload) => {
            let payload = payload.downcast_ref::<&str>().unwrap();
            payload.to_string()
        }
    }
}

fn check_4() {
    assert_eq!(catch_panic_message(), "boom");
    println!("rung 4 ok: caught a panicking thread and recovered its message");
}

// ── Rung 5: The 'static wall ─────────────────────────────────────────────────
// This rung is meant to FAIL TO COMPILE — that failure is the whole lesson.
// We have a local Vec and want a spawned thread to read it. thread::spawn
// requires the closure (and everything it captures) to be 'static, but `data`
// is a local that gets dropped when sum_with_spawn returns. The thread could
// still be running then → use-after-free → the borrow checker says NO.
//
// YOUR TURN:
//   1. Uncomment the body below and run it. Read the E0373/E0597 error.
//   2. Adding `move` doesn't truly fix it here — try it and see what changes
//      (the thread would take OWNERSHIP of data, so main couldn't use it after;
//      and we want to BORROW, not move). This is the motivation for scope().
//   3. Fill in the // WHY: line in your own words.
//   4. Leave this rung's check commented out — rung 6 is the real fix.
//
// WHY does spawn reject a borrow of `data`? <your turn: one sentence>
#[allow(dead_code)]
fn sum_with_spawn() -> i32 {
    let data = vec![1, 2, 3, 4];
    // TODO: uncomment to witness the error, then re-comment to keep the file building:
    let handle = thread::spawn(move || data.iter().sum::<i32>());
    handle.join().unwrap()
}

// ── Rung 6: thread::scope to the rescue ──────────────────────────────────────
// Same goal as rung 5 — let threads READ a local `data` — but legal this time.
// thread::scope(|s| { ... }) gives you a scope handle `s`. Spawn with
// `s.spawn(|| ...)` instead of thread::spawn. The scope BLOCKS at its closing
// brace until every spawned thread has finished, so borrows of `data` only need
// to outlive the scope — no 'static required, no `move` needed to read.
//
// Spawn TWO scoped threads that both borrow `&data`: one sums the first half,
// one sums the second half. Return their total. Note you can read `data` from
// multiple threads at once because they all hold SHARED (&) borrows.
fn sum_halves_scoped(data: &[i32]) -> i32 {
    let mid = data.len() / 2;

    thread::scope(|s| {
        let s1 = s.spawn(|| data[..mid].iter().sum::<i32>());
        let s2 = s.spawn(|| data[mid..].iter().sum::<i32>());
        s1.join().unwrap() + s2.join().unwrap()
    })
}

fn check_6() {
    assert_eq!(sum_halves_scoped(&[1, 2, 3, 4]), 10);
    assert_eq!(sum_halves_scoped(&[10, 20, 30]), 60);
    assert_eq!(sum_halves_scoped(&[]), 0);
    println!("rung 6 ok: scoped threads borrowed a local and summed it in parallel");
}

// ── Rung 7: Scoped parallel mutation ─────────────────────────────────────────
// Double every element of `data` IN PLACE, but in parallel: each scoped thread
// mutates a disjoint chunk. Two &mut into the same slice is normally forbidden —
// the key is `split_at_mut` (or `chunks_mut`), which the compiler KNOWS yields
// non-overlapping &mut sub-slices, so handing each to a different thread is safe.
//
// Plan:
//   1. let (left, right) = data.split_at_mut(data.len() / 2);
//   2. thread::scope: spawn one thread that doubles `left`, one that doubles
//      `right`. These need `move` (each thread takes ownership of its &mut
//      sub-slice — a &mut can't be shared/copied, so it must be moved in).
//   3. The scope joins automatically at the closing brace; no manual join needed
//      since you don't return a value (but you may join to be explicit).
fn double_in_parallel(data: &mut [i32]) {
    let (left, right) = data.split_at_mut(data.len() / 2);
    thread::scope(|s| {
        s.spawn(move || left.iter_mut().for_each(|x| *x *= 2));
        s.spawn(move || right.iter_mut().for_each(|x| *x *= 2));
    });
}

fn check_7() {
    let mut v = vec![1, 2, 3, 4, 5];
    double_in_parallel(&mut v);
    assert_eq!(v, vec![2, 4, 6, 8, 10]);

    let mut empty: Vec<i32> = vec![];
    double_in_parallel(&mut empty);
    assert_eq!(empty, Vec::<i32>::new());

    println!("rung 7 ok: scoped threads mutated disjoint chunks in parallel");
}

// ── Rung 8: Parallel fold / fan-in ───────────────────────────────────────────
// Map-reduce in miniature. Compute the sum of squares of `data` using up to `n`
// worker threads. Each worker handles one chunk, computes a PARTIAL sum, and the
// main thread folds the partials into the final total.
//
// Plan:
//   1. Split into ~n chunks. `data.chunks(chunk_len)` yields &[i64] sub-slices.
//      Pick chunk_len = ceil(len / n) (and guard n == 0 / empty data).
//   2. thread::scope: for each chunk, s.spawn a worker returning its partial
//      (x*x summed). Collect the JoinHandles into a Vec.
//   3. Join them all and sum the partials. Order doesn't matter for a sum, but
//      collecting handles-then-joining keeps the true parallelism (rung 2 lesson).
//
// Why i64: squares of test values exceed nothing here, but it's the realistic
// type for a fold that could overflow i32.
fn parallel_sum_of_squares(data: &[i64], n: usize) -> i64 {
    if data.is_empty() || n == 0 {
        return 0;
    }

    thread::scope(|s| {
        let chunk_len = (data.len() + n - 1) / n;
        let mut handles = Vec::with_capacity(n);
        for chunk in data.chunks(chunk_len) {
            handles.push(s.spawn(move || chunk.iter().map(|x| x * x).sum::<i64>()));
        }
        handles.into_iter().map(|h| h.join().unwrap()).sum()
    })
}

fn check_8() {
    // 1..=10 squared and summed = 385
    let data: Vec<i64> = (1..=10).collect();
    assert_eq!(parallel_sum_of_squares(&data, 4), 385);
    assert_eq!(parallel_sum_of_squares(&data, 1), 385); // single worker
    assert_eq!(parallel_sum_of_squares(&data, 100), 385); // more workers than items
    assert_eq!(parallel_sum_of_squares(&[], 4), 0); // empty
    println!("rung 8 ok: fanned out partial sums and folded them back in");
}

// ── Rung 9: Capstone — parallel_map ──────────────────────────────────────────
// Build a hand-rolled rayon-lite: split `data` across `n` scoped workers, apply
// `f` to every element, and return the results IN ORDER. Proves you own the
// whole model: scope + chunking + JoinHandle collection + generic Send/Sync.
//
// Signature is given. You fill the body. Bounds explained:
//   T: Sync         — workers share &[T]; &T must be safely sendable across
//                     threads, which is exactly what Sync means.
//   R: Send         — each result R is produced on a worker and moved back to
//                     the main thread, so R must be Send.
//   F: Fn(&T) -> R  — applied per element; called from multiple threads...
//   F: Sync         — ...concurrently, so &F must cross threads → F: Sync.
//
// Plan:
//   1. Guards: empty data → empty Vec; clamp n to at least 1.
//   2. chunk_len = ceil(len / n).
//   3. thread::scope: per chunk, s.spawn a worker that maps its chunk to a
//      Vec<R> (chunk.iter().map(&f).collect()). Keep handles in chunk order.
//   4. Join each handle and EXTEND a result Vec with its Vec<R>, in order.
//
// Note: `&f` — you borrow the closure into each worker (that's why F: Sync).
fn parallel_map<T, R, F>(data: &[T], n: usize, f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    if data.is_empty() || n == 0 {
        return Vec::new();
    }

    thread::scope(|s| {
        let f = &f;
        let chunk_len = (data.len() + n - 1) / n;
        let mut handles = Vec::with_capacity(n);
        for chunk in data.chunks(chunk_len) {
            handles.push(s.spawn(move || chunk.iter().map(f).collect::<Vec<R>>()));
        }
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    })
}

fn check_9() {
    let data: Vec<i32> = (1..=10).collect();

    // square each, preserving order
    let squares = parallel_map(&data, 4, |x| x * x);
    assert_eq!(squares, vec![1, 4, 9, 16, 25, 36, 49, 64, 81, 100]);

    // map to a different type R (i32 -> String), order preserved
    let labels = parallel_map(&data, 3, |x| format!("n{x}"));
    assert_eq!(labels[0], "n1");
    assert_eq!(labels[9], "n10");
    assert_eq!(labels.len(), 10);

    // edge cases
    assert_eq!(
        parallel_map::<i32, i32, _>(&[], 4, |x| x * 2),
        Vec::<i32>::new()
    );
    assert_eq!(
        parallel_map(&data, 1, |x| x + 1),
        (2..=11).collect::<Vec<_>>()
    );
    assert_eq!(parallel_map(&data, 100, |x| *x), data); // more workers than items

    println!("rung 9 ok: CAPSTONE — built a parallel_map (rayon-lite) that preserves order");
}

fn main() {
    check_1();
    check_2();
    check_3();
    check_4();
    check_6();
    check_7();
    check_8();
    check_9();
}
