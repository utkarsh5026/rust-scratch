//! Data parallelism with `rayon`.
//!
//! Run: `cargo run --bin rayon_parallel`        (release for honest timings: add --release)
//!
//! Mental model: rayon turns a sequential iterator chain into a parallel one
//! over a thread pool. The engine is WORK-STEALING — each worker has its own
//! task deque and steals from busy peers when idle, so uneven work still
//! balances. You opt in with `.iter()` -> `.par_iter()`; Send/Sync keep it sound.
//! Two hard lessons: parallelism has OVERHEAD (loses on small/cheap work), and
//! reduce/fold need an ASSOCIATIVE op (partials recombine in unspecified order).
//!
//! Ladder (DONE marks finished rungs):
//!   1. par_iter first contact — sum a big Vec in parallel                  [foundations]
//!   2. adapter zoo: map/filter/collect, order preserved by collect          [foundations]
//!   3. reduce & fold — identity + fold-then-reduce                          [mechanics]
//!   4. rayon::join — the fork-join primitive underneath par_iter            [mechanics]
//!   5. when parallelism LOSES — measure the overhead / break-even          [footgun]
//!   6. non-associative reduce is a bug — non-determinism                    [footgun]
//!   7. shared-state footgun — par for_each push won't compile; fix it       [footgun]
//!   8. par_sort, par_bridge, custom splitting                               [real-world]
//!   9. capstone: hand-rolled fork-join parallel_map + parallel quicksort    [capstone]

use std::sync::Mutex;

use rayon::prelude::*;

// ── Rung 1: par_iter first contact ──────────────────────────────────────────
// Sum 0..1_000_000 (as u64) using rayon's parallel iterator. The answer must
// match the sequential sum. Swap .iter() thinking for .par_iter().
fn parallel_sum(data: &[u64]) -> u64 {
    data.par_iter().sum()
}

fn check_1() {
    let data: Vec<u64> = (0..1_000_000).collect();
    let seq: u64 = data.iter().sum();
    let par = parallel_sum(&data);
    assert_eq!(par, seq, "parallel sum must equal sequential sum");
    assert_eq!(par, 499_999_500_000);
    println!("rung 1 ok: parallel sum = {par}");

    // Timing peek (run with --release for meaningful numbers). Summing 1M u64s
    // is the WORST showcase for rayon: trivial per-item work + memory-bound, so
    // parallel often ties or LOSES here. That break-even is rung 5's lesson.
    use std::time::Instant;
    let t = Instant::now();
    let s: u64 = data.iter().sum();
    println!("  seq: {:>10?}  ({s})", t.elapsed());
    let t = Instant::now();
    let p: u64 = data.par_iter().sum();
    println!("  par: {:>10?}  ({p})", t.elapsed());
}

// ── Rung 2: the adapter zoo is the same ──────────────────────────────────────
// Given a slice of numbers, in PARALLEL: keep only the even ones, square each,
// and collect the results into a Vec<u64>. The chain reads like the sequential
// version — par_iter().filter(..).map(..).collect(). The point: `collect` puts
// results back in INPUT ORDER (rayon tracks indices), unlike `for_each` which
// runs in whatever order threads finish. So this returns a deterministic Vec.
fn even_squares(data: &[u64]) -> Vec<u64> {
    data.par_iter()
        .filter(|x| *x % 2 == 0)
        .map(|x| x.pow(2))
        .collect()
}

fn check_2() {
    let data: Vec<u64> = (0..10).collect();
    let got = even_squares(&data);
    // 0,2,4,6,8 squared -> 0,4,16,36,64, IN ORDER.
    assert_eq!(
        got,
        vec![0, 4, 16, 36, 64],
        "collect must preserve input order"
    );

    let big: Vec<u64> = (0..1000).collect();
    let seq: Vec<u64> = big.iter().filter(|n| *n % 2 == 0).map(|n| n * n).collect();
    assert_eq!(even_squares(&big), seq);
    println!("rung 2 ok: {got:?} ...");
}

// ── Rung 3: reduce & fold ────────────────────────────────────────────────────
// `sum()` is just a special reduce. Here you wield the general tool.
//
// (a) word_count_total: given &[&str] (words), find the TOTAL number of bytes
//     across all words, using `.map(|w| w.len()).reduce(identity, combine)`.
//     reduce(id, op) needs:
//       - an IDENTITY closure `|| 0usize` — rayon calls it to seed each chunk,
//         possibly many times, so it must return the neutral element.
//       - a combine closure `|a, b| a + b` — associative, merges two partials.
//
// (b) concat_lengths: same data, but demonstrate fold-then-reduce. `fold` builds
//     a PER-THREAD local accumulator (here: sum of lengths in this chunk),
//     producing a parallel iterator of partials; then `.reduce(id, op)` merges
//     those partials across threads. fold's signature is fold(identity, |acc, item| ..).
//     Make both (a) and (b) return the same number.
fn word_count_total(words: &[&str]) -> usize {
    words
        .par_iter()
        .map(|w| w.len())
        .reduce(|| 0_usize, |a, b| a + b)
}

fn concat_lengths(words: &[&str]) -> usize {
    words
        .par_iter()
        .fold(|| 0_usize, |acc, w| acc + w.len())
        .reduce(|| 0_usize, |a, b| a + b)
}

fn check_3() {
    let words = ["the", "quick", "brown", "fox", "jumps"];
    let total: usize = words.iter().map(|w| w.len()).sum(); // 3+5+5+3+5 = 21
    assert_eq!(
        word_count_total(&words),
        total,
        "reduce must total the lengths"
    );
    assert_eq!(concat_lengths(&words), total, "fold-then-reduce must agree");
    println!("rung 3 ok: total bytes = {total}");
}

// ── Rung 4: rayon::join — the fork-join primitive ────────────────────────────
// par_iter is sugar. Underneath, rayon recursively SPLITS work with one
// primitive: `rayon::join(a, b)` runs closures `a` and `b` POTENTIALLY in
// parallel and returns `(a_result, b_result)`. "Potentially" is the magic:
// join pushes `b` onto this thread's deque and runs `a`; if another worker is
// idle it STEALS `b` and runs it concurrently; if not, this thread just runs b
// itself after a. Zero waste either way — that's work-stealing.
//
// Implement a recursive parallel sum of a slice by hand:
//   - base case: slice short enough (say len <= 1024) -> sum it sequentially.
//   - else: split in half with `slice.split_at(mid)`, then
//     `let (l, r) = rayon::join(|| sum_split(left), || sum_split(right));`
//     and return l + r.
// This is literally the skeleton par_iter().sum() generates for you.
fn sum_split(data: &[u64]) -> u64 {
    if data.len() <= 1024 {
        return data.iter().sum();
    }
    let (left, right) = data.split_at(data.len() / 2);
    let (l, r) = rayon::join(|| sum_split(left), || sum_split(right));
    l + r
}

fn check_4() {
    let data: Vec<u64> = (0..1_000_000).collect();
    assert_eq!(sum_split(&data), 499_999_500_000);
    assert_eq!(sum_split(&[]), 0, "empty slice");
    assert_eq!(sum_split(&[42]), 42, "single element");
    println!("rung 4 ok: fork-join sum = {}", sum_split(&data));
}

// ── Rung 5: when parallelism LOSES — measure the break-even ──────────────────
// Rule of thumb: parallel speedup ≈ (work per item × item count) / overhead.
// You'll make the per-item work tunable and watch parallel go from LOSS to WIN.
//
// (a) `expensive(x)`: a deliberately costly PURE function of one number, with a
//     tunable amount of work. Do `iters` rounds of some cheap arithmetic that
//     the optimizer can't delete, e.g. start acc = x, loop: acc = acc.wrapping_mul(31).wrapping_add(7).
//     Return acc. (Pure + CPU-bound is what rayon loves.)
fn expensive(x: u64, iters: u64) -> u64 {
    let mut acc = x;
    for _ in 0..iters {
        acc = acc.wrapping_mul(31).wrapping_add(7);
    }
    acc
}

// (b) two pipelines mapping expensive() over a slice and summing. Same answer,
//     one sequential, one parallel. (Use wrapping_add for the sum to avoid overflow.)
fn process_seq(data: &[u64], iters: u64) -> u64 {
    data.iter()
        .map(|&x| expensive(x, iters))
        .fold(0_u64, |a, b| a.wrapping_add(b))
}
fn process_par(data: &[u64], iters: u64) -> u64 {
    data.par_iter()
        .map(|&x| expensive(x, iters))
        .reduce(|| 0_u64, |a, b| a.wrapping_add(b))
}

fn check_5() {
    use std::time::Instant;
    // Correctness first: both pipelines must agree, regardless of timing.
    let data: Vec<u64> = (0..100_000).collect();
    assert_eq!(
        process_seq(&data, 50),
        process_par(&data, 50),
        "results must match"
    );

    // Now the lesson — sweep work-per-item and watch the verdict flip.
    println!("rung 5: break-even sweep ({} items)", data.len());
    for iters in [0u64, 1, 10, 100, 1000] {
        let t = Instant::now();
        let s = process_seq(&data, iters);
        let seq = t.elapsed();
        let t = Instant::now();
        let p = process_par(&data, iters);
        let par = t.elapsed();
        assert_eq!(s, p);
        let speedup = seq.as_secs_f64() / par.as_secs_f64();
        let verdict = if speedup > 1.0 { "WIN " } else { "loss" };
        println!("  iters={iters:>4}: seq {seq:>10?}  par {par:>10?}  ->{speedup:>5.2}x {verdict}");
    }
    println!("rung 5 ok (run --release for honest numbers)");
}

// ── Rung 6: non-associative reduce is a bug ──────────────────────────────────
// `reduce(id, op)` recombines partial results in an UNSPECIFIED tree shape that
// depends on how rayon split the work — which depends on runtime scheduling.
// So `op` MUST be associative: op(op(a,b),c) == op(a,op(b,c)). If it isn't, your
// answer changes with the split — non-deterministic, silently wrong.
//
// (a) par_diff: parallel reduce over i64 with SUBTRACTION as the op
//     (`data.par_iter().copied().reduce(|| 0, |a, b| a - b)`). Subtraction is the
//     classic non-associative op. This compiles and runs — that's the trap.
//
// (b) seq_diff: the "intended" left-to-right meaning — `data.iter().fold(0, |a,b| a-b)`.
//     This is deterministic. par_diff is not guaranteed to match it.
fn par_diff(data: &[i64]) -> i64 {
    data.par_iter().copied().reduce(|| 0_i64, |a, b| a - b)
}

fn seq_diff(data: &[i64]) -> i64 {
    data.iter().fold(0_i64, |a, b| a - b)
}

fn check_6() {
    use std::collections::HashSet;

    // The ROOT of the bug, proven deterministically: subtraction breaks the
    // associativity law, so any grouping (= any parallel split) can disagree.
    assert_ne!((10 - 5) - 3, 10 - (5 - 3), "subtraction is non-associative");

    // Big varied input so rayon actually splits into multiple chunks.
    let data: Vec<i64> = (0..200_000).map(|i| i * 3 - 7).collect();

    // Run the buggy reduce many times; collect the DISTINCT answers we observe.
    let mut seen = HashSet::new();
    for _ in 0..200 {
        seen.insert(par_diff(&data));
    }
    let seq = seq_diff(&data);
    println!("rung 6: seq_diff = {seq}");
    println!(
        "        par_diff produced {} distinct value(s) over 200 runs: {:?}",
        seen.len(),
        seen
    );
    // Whether you see 1 or many depends on scheduling, but the guarantee is gone:
    // a correct program must not depend on this. The lesson is the assert above.
    println!("rung 6 ok: only associative ops are safe in reduce/fold");
}

// ── Rung 7: the shared-state footgun ─────────────────────────────────────────
// The reflex from other languages: "make an empty Vec, then have each parallel
// task push into it." In Rust that DOESN'T COMPILE — and the error is the lesson.
//
// FIRST, witness the wall. Temporarily paste this into squares_broken and run:
//
//     let mut out = Vec::new();
//     data.par_iter().for_each(|&x| out.push(x * x));   // ❌
//     out
//
// par_iter().for_each takes an `Fn` closure that rayon calls from MANY threads at
// once, so the closure must be `Sync` and may only borrow `out` SHARED (&). But
// `out.push` needs `&mut out`. Two threads pushing at once would be a data race,
// so the borrow checker rejects it (E0596 / "cannot borrow as mutable" / closure
// is FnMut not Fn). Rust makes the race a COMPILE error, not a runtime crash.
//
// Now FIX it, two ways:
// (a) squares_collect — the idiomatic fix: don't share state at all. Each task
//     returns its value; `collect` assembles them IN ORDER. No locks, no races.
//         data.par_iter().map(|&x| x * x).collect()
// (b) squares_mutex — the "works but worse" fix: wrap the Vec in a Mutex so pushes
//     are serialized. It compiles and is correct, BUT: order is NONdeterministic
//     (threads finish in any order) and every push contends on one lock, killing
//     the parallelism you paid for. Use std::sync::Mutex; push inside for_each.
fn squares_collect(data: &[u64]) -> Vec<u64> {
    data.par_iter().map(|&x| x * x).collect()
}
fn squares_mutex(data: &[u64]) -> Vec<u64> {
    let out = Mutex::new(Vec::new());
    data.par_iter()
        .for_each(|&x| out.lock().unwrap().push(x * x));
    out.into_inner().unwrap()
}

fn check_7() {
    let data: Vec<u64> = (0..1000).collect();
    let want: Vec<u64> = data.iter().map(|&x| x * x).collect();

    // collect preserves input order -> matches the sequential answer exactly.
    assert_eq!(
        squares_collect(&data),
        want,
        "collect must be ordered & correct"
    );

    // mutex version is correct as a SET, but order is not guaranteed -> sort to compare.
    let mut got = squares_mutex(&data);
    got.sort_unstable();
    let mut want_sorted = want.clone();
    want_sorted.sort_unstable();
    assert_eq!(
        got, want_sorted,
        "mutex version has all the right elements (order lost)"
    );
    println!("rung 7 ok: collect (ordered, lock-free) beats Mutex<Vec> (unordered, contended)");
}

// ── Rung 8: par_sort & par_bridge — the real-world APIs ──────────────────────
// Two tools you'll actually reach for.
//
// (a) parallel_sort: rayon adds `par_sort` / `par_sort_unstable` to slices (a
//     parallel merge/quicksort). Same signature as the std sort — just sort
//     `v` IN PLACE with `v.par_sort_unstable()`.
//
// (b) bridge_word_sum: par_iter only works on things rayon can SPLIT by index
//     (slices, ranges, Vec...). A plain sequential `Iterator` (here:
//     `str::split_whitespace`, which yields tokens one at a time and can't be
//     indexed) has no `.par_iter()`. `.par_bridge()` adapts ANY `Iterator: Send`
//     into a ParallelIterator: workers pull items from the shared sequential
//     source behind a lock, then process them in parallel. Parse each whitespace
//     token to u64 and sum them, in parallel, via par_bridge.
//     Caveat to remember: par_bridge has a serial pull-bottleneck + does NOT
//     preserve order, so a native par_iter over a Vec is faster when you have one.
fn parallel_sort(v: &mut [i32]) {
    v.par_sort_unstable()
}
fn bridge_word_sum(text: &str) -> u64 {
    text.split_whitespace()
        .par_bridge()
        .map(|w| w.parse::<u64>().unwrap())
        .sum()
}

fn check_8() {
    // (a) sort
    let mut v: Vec<i32> = (0..10_000).rev().collect(); // worst case: fully reversed
    v.push(-5);
    parallel_sort(&mut v);
    assert!(
        v.windows(2).all(|w| w[0] <= w[1]),
        "must be sorted ascending"
    );
    assert_eq!(v[0], -5);

    // (b) par_bridge over a sequential iterator
    let text = "10 20 30 40 50 1 2 3 4";
    let want: u64 = text
        .split_whitespace()
        .map(|w| w.parse::<u64>().unwrap())
        .sum();
    assert_eq!(
        bridge_word_sum(text),
        want,
        "bridge sum must match sequential"
    );
    assert_eq!(bridge_word_sum(text), 160);
    println!(
        "rung 8 ok: par_sort sorted {} elems; bridge sum = 160",
        v.len()
    );
}

// ── Rung 9: CAPSTONE — hand-rolled fork-join parallel_map + parallel quicksort ─
// Prove you own the model: build rayon-style machinery from the ONE primitive,
// rayon::join. No par_iter, no par_sort — just join + recursion + a cutoff.
//
// (a) parallel_map: map `f` over a slice in parallel, returning a Vec in INPUT
//     ORDER (like collect). Recursion:
//       - cutoff: if data.len() <= THRESHOLD, map sequentially into a Vec.
//       - else: split_at(mid); rayon::join(|| parallel_map(left,f), || parallel_map(right,f));
//         then concatenate the two Vecs (left first) and return.
//     Think hard about the BOUNDS — this is the real lesson:
//       T: Sync   (both halves read &[T] from different threads at once)
//       R: Send   (each half's Vec<R> travels back to the joining thread)
//       F: Fn(&T) -> R + Sync   (the SAME closure is shared & called on many threads;
//                                Fn not FnMut — no shared mutable state — and Sync
//                                so &F can cross the join. Pass `f` by &reference
//                                down the recursion so you don't need F: Clone.)
const THRESHOLD: usize = 2048;

fn parallel_map<T, R, F>(data: &[T], f: &F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    if data.len() <= THRESHOLD {
        return data.iter().map(f).collect();
    }
    let (left, right) = data.split_at(data.len() / 2);
    let (mut left, right) = rayon::join(|| parallel_map(left, f), || parallel_map(right, f));
    left.extend(right);
    left
}

// (b) parallel_quicksort: sort a &mut [T] in place. Recursion:
//       - cutoff: small slice -> sort sequentially (slice::sort_unstable is fine).
//       - else: partition around a pivot, then split the slice into the two sides
//         with split_at_mut (gives you two DISJOINT &mut halves — exactly what lets
//         rayon run them in parallel safely), and rayon::join the two recursive sorts.
//     A simple scheme: pick pivot = last element, Lomuto-partition so everything
//     <= pivot is on the left, return the pivot's final index `p`; then recurse on
//     [..p] and [p+1..]. Bounds: T: Ord + Send (compared, and &mut chunks cross threads).
fn parallel_quicksort<T: Ord + Send>(data: &mut [T]) {
    if data.len() <= THRESHOLD {
        data.sort_unstable();
        return;
    }

    let len = data.len();
    data.swap(len / 2, len - 1);

    let mut pivot_index = 0;
    for i in 0..len - 1 {
        if data[i] <= data[len - 1] {
            data.swap(i, pivot_index);
            pivot_index += 1;
        }
    }
    data.swap(pivot_index, len - 1);

    let (left, pivot_and_right) = data.split_at_mut(pivot_index);
    let (_, right) = pivot_and_right.split_at_mut(1);
    rayon::join(|| parallel_quicksort(left), || parallel_quicksort(right));
}

fn check_9() {
    use std::time::Instant;

    // (a) parallel_map matches a sequential map, in order.
    let data: Vec<u64> = (0..100_000).collect();
    let got = parallel_map(&data, &|&x| expensive(x, 200));
    let want: Vec<u64> = data.iter().map(|&x| expensive(x, 200)).collect();
    assert_eq!(
        got, want,
        "parallel_map must equal sequential map, in order"
    );

    // and it should actually be faster than sequential on heavy work (release).
    let t = Instant::now();
    let _ = parallel_map(&data, &|&x| expensive(x, 500));
    let par = t.elapsed();
    let t = Instant::now();
    let _: Vec<u64> = data.iter().map(|&x| expensive(x, 500)).collect();
    let seq = t.elapsed();
    println!(
        "  parallel_map: seq {seq:?}  par {par:?}  ({:.2}x)",
        seq.as_secs_f64() / par.as_secs_f64()
    );

    // (b) parallel_quicksort matches the stdlib sort.
    let mut a: Vec<i32> = (0..50_000).rev().collect();
    a.extend([7, -3, 7, 0, -999, 12345, -3]);
    let mut b = a.clone();
    parallel_quicksort(&mut a);
    b.sort();
    assert_eq!(a, b, "parallel_quicksort must match std sort");
    assert!(a.windows(2).all(|w| w[0] <= w[1]));
    // edge cases
    let mut empty: Vec<i32> = vec![];
    parallel_quicksort(&mut empty);
    let mut one = vec![42];
    parallel_quicksort(&mut one);
    assert_eq!(one, vec![42]);
    println!(
        "rung 9 ok: hand-rolled parallel_map + quicksort match sequential — you own fork-join"
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
