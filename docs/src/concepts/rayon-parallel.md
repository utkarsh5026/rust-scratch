# Data parallelism with `rayon`

> Ladder: [`src/bin/rayon_parallel.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/rayon_parallel.rs) ·
> Run: `cargo run --bin rayon_parallel` (add `--release` for honest timings) · Phase 4 · 9 rungs

## TL;DR

Rayon turns a sequential iterator chain into a parallel one over a thread pool:
where you wrote `.iter()`, write `.par_iter()` and the work spreads across cores.
The engine underneath is **work-stealing fork-join** — every worker thread owns a
task deque, and an idle worker *steals* tasks from a busy one, so uneven work
still balances itself. The whole library is built from one primitive,
`rayon::join(a, b)`, which runs two closures *potentially* in parallel.

Two lessons separate "I sprinkled `par_` everywhere" from actually understanding it:

1. **Parallelism has overhead.** It loses on small or cheap or memory-bound work.
   `par_iter` pays off only when `total_work / cores` clearly exceeds rayon's
   ~hundreds-of-microseconds setup cost.
2. **`reduce`/`fold` need an associative operation.** Partial results recombine
   in an unspecified tree shape, so a non-associative op (subtraction, float `+`)
   gives a *different, non-deterministic* answer every run.

The type system you already drilled (`Send`/`Sync`) is what makes all of this
sound: data races become compile errors, not crashes.

## Why this exists (from first principles)

You have a million items and N cores. You want to use all N. The naive plan —
"spawn N threads, give each a chunk" — has three problems:

- **`'static` and ownership.** `std::thread::spawn` needs `'static` closures, so
  borrowing a local slice across threads doesn't compile without scoped threads.
- **Load imbalance.** Equal-sized chunks are not equal *work*. If chunk 3 happens
  to contain all the expensive items, three cores finish early and idle while one
  grinds. Static partitioning wastes exactly the parallelism you wanted.
- **Boilerplate.** Handles, joins, chunk math, result reassembly — every time.

Rayon answers all three. It runs on a **thread pool** sized to your core count
(created lazily on first use), so there are no per-call thread spawns. It splits
work **recursively and dynamically**, and its **work-stealing** scheduler means a
core that runs dry grabs pending work from a busy core — load balances itself, no
matter how lumpy the per-item cost. And it exposes all of it as a drop-in
parallel `Iterator`.

What keeps it safe is the same thing that keeps `std::thread::scope` safe:
closures handed to the pool must satisfy `Send`/`Sync`, so the compiler rejects
any sharing that would be a data race. Parallel bugs that are runtime disasters
in C++ are *type errors* here.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|-----------|
| 1 | foundations | `par_iter` first contact | `.iter().sum()` → `.par_iter().sum()`; same answer |
| 2 | foundations | adapter zoo | `map`/`filter`/`collect`; `collect` preserves **input order** |
| 3 | mechanics | `reduce` & `fold` | identity *closure*; fold-then-reduce (local acc → combine) |
| 4 | mechanics | `rayon::join` | the fork-join primitive `par_iter` is built on |
| 5 | footgun | when parallelism **loses** | measure the overhead; find the break-even |
| 6 | footgun | non-associative reduce | subtraction ⇒ a different answer every run |
| 7 | footgun | the shared-state wall | `for_each` push won't compile; `collect` vs `Mutex` |
| 8 | real-world | `par_sort` & `par_bridge` | parallel sort; adapt any sequential `Iterator` |
| 9 | capstone | hand-rolled fork-join | `parallel_map` + parallel quicksort from `join` |

## The ideas, built up

### 1–2. `par_iter` is the iterator you know, parallelized

The entire entry point is one import and one method swap:

```rust
use rayon::prelude::*;

fn parallel_sum(data: &[u64]) -> u64 {
    data.par_iter().sum()      // was: data.iter().sum()
}
```

`use rayon::prelude::*` brings the `par_iter()` method and the `ParallelIterator`
adapters (`map`, `filter`, `reduce`, `collect`, …) into scope. The chain reads
identically to the sequential version — that's the design goal.

The adapter zoo behaves the same, with one subtlety worth internalizing:

```rust
fn even_squares(data: &[u64]) -> Vec<u64> {
    data.par_iter()
        .filter(|x| *x % 2 == 0)
        .map(|x| x.pow(2))
        .collect()              // results land back IN INPUT ORDER
}
```

> **`collect` preserves order; `for_each` does not.** Threads finish in whatever
> order they finish, but `collect` tracks each item's index and reassembles a
> deterministic `Vec` matching the sequential result. If you need ordered output,
> reach for `map(...).collect()`, never `for_each` with a side effect.

`filter`'s closure receives a double reference (`&&u64`) — one `&` from
`par_iter` yielding `&u64`, another from `filter` borrowing it — same as
sequential iterators.

### 3. `reduce` and `fold`: why an *identity closure*?

`sum()` is a special case of `reduce`. The general tool looks like this:

```rust
fn word_count_total(words: &[&str]) -> usize {
    words.par_iter()
        .map(|w| w.len())
        .reduce(|| 0_usize, |a, b| a + b)
    //         ^^^^^^^^^^  ^^^^^^^^^^^^^^
    //         identity     combine
}
```

The first argument is **a closure that returns the identity**, not a single value
like `Iterator::fold` takes. Why? Because rayon splits the data into an unknown
number of independent chunks and must **seed each one separately**. There is no
single starting accumulator threaded left-to-right; each chunk starts from the
neutral element and the partials get combined. So rayon calls `|| 0` possibly
many times — once per chunk it decides to create.

`fold`-then-`reduce` makes the two-level structure explicit:

```rust
fn concat_lengths(words: &[&str]) -> usize {
    words.par_iter()
        .fold(|| 0_usize, |acc, w| acc + w.len())  // per-thread local accumulator
        .reduce(|| 0_usize, |a, b| a + b)          // merge the few partials
}
```

> Rayon's `fold` is **not** `Iterator::fold`. It returns *another parallel
> iterator* of partial results — one accumulator per chunk — which is why you
> chain `.reduce(...)` after it to collapse those partials to a scalar. The win:
> the per-item hot loop touches only a thread-local accumulator (cheap, no
> cross-thread coordination), and only the handful of partials pay the merge cost.

### 4. `rayon::join`: the primitive everything is built on

`par_iter` is sugar. Underneath, rayon recursively splits work with a single
primitive. `rayon::join(a, b)` runs closures `a` and `b` *potentially* in
parallel and returns `(a_result, b_result)`:

```rust
fn sum_split(data: &[u64]) -> u64 {
    if data.len() <= 1024 {
        return data.iter().sum();          // base case: go sequential
    }
    let (left, right) = data.split_at(data.len() / 2);
    let (l, r) = rayon::join(|| sum_split(left), || sum_split(right));
    l + r
}
```

The word **potentially** is the whole magic. `join` pushes task `b` onto the
current thread's deque and runs `a` itself. If another worker is idle, it
*steals* `b` and runs it concurrently. If no one is free, the current thread just
runs `b` after `a`. Either way there is zero wasted scheduling — that is
work-stealing, and it is why a recursion tree of `join`s automatically uses
however many cores happen to be free, with no manual chunk math.

> The base-case cutoff (`len <= 1024`) matters: recursing all the way down to
> single elements would drown the actual work in `join` overhead. This same
> "go sequential below a threshold" pattern reappears in the capstone.

### 5. When parallelism actually helps (and when it loses)

Rule of thumb: `speedup ≈ (work_per_item × item_count) / overhead`. The ladder
makes per-item work tunable and sweeps it:

```rust
fn expensive(x: u64, iters: u64) -> u64 {   // tunable, pure, CPU-bound
    let mut acc = x;
    for _ in 0..iters { acc = acc.wrapping_mul(31).wrapping_add(7); }
    acc
}
```

A representative `--release` run summing `expensive` over 100,000 items:

```
iters=   0: seq  20µs   par 376µs   -> 0.06x loss   <- work ~ 0, pure overhead
iters=   1: seq 141µs   par 426µs   -> 0.33x loss
iters=  10: seq 162µs   par 421µs   -> 0.39x loss
iters= 100: seq 633µs   par 1.60ms  -> 0.39x loss
iters=1000: seq 8.57ms  par 2.25ms  -> 3.81x WIN    <- work finally dominates
```

Read it like this:

- **The parallel column has a floor (~400µs).** That is rayon's fixed cost:
  splitting, deque pushes, steal coordination, recombination. Below that floor,
  parallel can never win no matter how you write it.
- **The crossover is between 100 and 1000 iters.** At `iters=100`, sequential is
  *still cheaper* (633µs) than parallel's overhead-laden 1.6ms. Only when one
  pass costs ~8.5ms does dividing it across cores swamp the coordination cost.
- **3.81×, not Ncores×.** Perfect linear scaling never happens — memory
  bandwidth, the serial recombine step, and hyperthreads all skim off the top.
  ~4× on a typical machine is a healthy real result.

> **Takeaway.** Use `par_iter` when `total_work / cores` clearly exceeds rayon's
> ~hundreds-of-µs setup. Tiny collections or trivial per-item work → stay
> sequential. Summing a million plain integers is the *worst* showcase: the work
> is one add per item and the loop is memory-bound, so extra threads just fight
> over the memory bus. When unsure, measure exactly like the table above.

### 6. Non-associative reduce is a silent bug

Because `reduce` recombines partials in a tree shape that depends on how rayon
split the work — which depends on runtime scheduling — the combine operation
**must be associative**: `(a ∘ b) ∘ c == a ∘ (b ∘ c)`. Subtraction is the classic
violator:

```rust
fn par_diff(data: &[i64]) -> i64 {
    data.par_iter().copied().reduce(|| 0, |a, b| a - b)   // BUG: not associative
}
fn seq_diff(data: &[i64]) -> i64 {
    data.iter().fold(0, |a, b| a - b)                     // deterministic meaning
}
```

The root cause is provable without any threads at all:

```rust
assert_ne!((10 - 5) - 3, 10 - (5 - 3));   // 2 != 8 — grouping changes the answer
```

Running `par_diff` over a 200,000-element vector 200 times produced **200
distinct answers** in 200 runs, and not one matched `seq_diff`. Every run, rayon
made slightly different steal decisions, grouped the subtractions differently, and
returned a different number.

> This is the nightmare class of bug: it compiles, runs, and returns a
> plausible-looking value that is wrong and never the same twice. The fix is never
> "rearrange the reduce" — it is **only feed `reduce`/`fold` an associative op.**
> Note floating-point `+` is *technically* non-associative too (rounding depends
> on order), so parallel float sums can differ slightly from the sequential sum.

### 7. The shared-state wall

The reflex from other languages — "make an empty list, have each task push into
it" — does not compile in Rust, and the rejection is the lesson:

```rust
let mut out = Vec::new();
data.par_iter().for_each(|&x| out.push(x * x));   // WRONG: does not compile
out
```

`for_each` calls its closure from many threads at once, so the closure must be
`Fn` (shareable, borrowing captures by `&` only). But `out.push` needs
`&mut out`, and two threads mutating one `Vec` simultaneously is a data race — so
the borrow checker refuses (the closure would have to be `FnMut`, and `&mut out`
can't be shared). **Rust turns the data race into a compile error.**

Two fixes, with a clear preference:

```rust
// OK, idiomatic: don't share state at all. Each task returns a value;
// collect reassembles them in order. Lock-free, race-free, deterministic.
fn squares_collect(data: &[u64]) -> Vec<u64> {
    data.par_iter().map(|&x| x * x).collect()
}

// Works, but worse: serialize pushes behind a lock.
fn squares_mutex(data: &[u64]) -> Vec<u64> {
    let out = Mutex::new(Vec::new());
    data.par_iter().for_each(|&x| out.lock().unwrap().push(x * x));
    out.into_inner().unwrap()
}
```

The `Mutex` version compiles and is correct *as a set*, but:

- **order is lost** — threads finish in any order, so you must sort to compare;
- **every push contends on one lock**, serializing the very work you parallelized.

Note it needs only a plain `Mutex`, no `Arc`: `for_each` merely *borrows* `out`,
and rayon guarantees all tasks finish before it returns, so a shared `&Mutex`
across the scoped tasks suffices — the same reasoning as scoped threads.
`into_inner()` then consumes the mutex to hand back the `Vec` with no clone.

> If you find yourself locking to collect results, `collect` was the better tool.

### 8. Real-world APIs: `par_sort` and `par_bridge`

```rust
v.par_sort_unstable();                       // parallel sort, std-identical API
```

Rayon adds `par_sort` / `par_sort_unstable` to slices — a drop-in parallel
quicksort/mergesort with the same signature as the std sort.

`par_iter` only works on things rayon can **split by index** (slices, ranges,
`Vec`). A plain sequential `Iterator` — like `str::split_whitespace`, which yields
tokens one at a time and can't be indexed — has no `.par_iter()`. `par_bridge`
adapts any `Iterator: Send` into a parallel one:

```rust
fn bridge_word_sum(text: &str) -> u64 {
    text.split_whitespace()
        .par_bridge()                        // adapt sequential Iterator -> parallel
        .map(|w| w.parse::<u64>().unwrap())
        .sum()
}
```

> `par_bridge` has workers pull items from the shared sequential source behind a
> lock, so it has a serial pull-bottleneck and does **not** preserve order. When
> you can get a slice or `Vec`, native `par_iter` is faster. Reach for
> `par_bridge` only when the source is fundamentally sequential — lines from a
> reader, an FFI iterator, a generator.

## Footguns

| Footgun | What bites | Fix |
|--------|-----------|-----|
| Parallelizing cheap work | `par_iter` slower than `iter` on small/memory-bound work | Measure; stay sequential below the break-even |
| Non-associative `reduce`/`fold` | silent, non-deterministic wrong answers | Only use associative ops; beware float `+` |
| Shared mutable state in `for_each` | won't compile (`Fn`/`&mut` conflict) | `map().collect()`; `Mutex` only if forced |
| Expecting `for_each` order | runs in arbitrary completion order | use `collect` for ordered output |
| `par_bridge` for an indexable source | serial pull-bottleneck, unordered | use native `par_iter` on the slice/Vec |
| Benchmarking unused results | dead-code elimination deletes the work | `std::hint::black_box(result)` |

The benchmark footgun is worth a closer look. In the capstone's timing block:

```rust
let _: Vec<u64> = data.iter().map(|&x| expensive(x, 500)).collect();  // result dropped
```

In `--release`, the optimizer proved `expensive` is pure and the result unused, so
it **deleted the entire sequential loop** — the timer reported ~88ns, a lie. The
parallel side survived only because `rayon::join` is an opaque call the optimizer
can't see through. To benchmark honestly you must *consume* the result (e.g.
`black_box`), or assert on it as rung 5 does.

## Real-world patterns

- **Embarrassingly parallel transforms.** `data.par_iter().map(expensive).collect()`
  is the bread and butter — image pixels, rows of a dataset, files to process.
- **Parallel aggregation.** `par_iter().map(...).sum()` / `.reduce(...)` for stats
  over large collections, as long as the combine is associative.
- **`par_sort_unstable`** for large in-memory sorts.
- **`par_bridge`** to parallelize work over a streaming source you can't index.
- **Custom thread pools** (`rayon::ThreadPoolBuilder`) when you need to bound
  parallelism or isolate workloads — the same `par_iter`/`join` API runs inside.

## Capstone insight

The capstone rebuilds rayon-style machinery from the single primitive `join` — no
`par_iter`, no `par_sort`. The structural "aha": **every parallel algorithm here
is the same shape — recurse, fork the two halves with `join`, fall back to
sequential below a cutoff.**

```rust
fn parallel_map<T, R, F>(data: &[T], f: &F) -> Vec<R>
where
    T: Sync,                       // both halves read &[T] from different threads
    R: Send,                       // each half's Vec<R> travels back to the joiner
    F: Fn(&T) -> R + Sync,         // the SAME closure is shared across threads
{
    if data.len() <= THRESHOLD {
        return data.iter().map(f).collect();          // sequential base case
    }
    let (left, right) = data.split_at(data.len() / 2);
    let (mut left, right) = rayon::join(
        || parallel_map(left, f),
        || parallel_map(right, f),
    );
    left.extend(right);                                // left first -> input order
    left
}
```

The **bounds are the real lesson**, and they fall straight out of what crosses
threads:

- `T: Sync` — `&[T]` is read concurrently by both recursive calls, and
  `&T: Send ⟺ T: Sync`.
- `R: Send` — each half builds a `Vec<R>` on its worker and ships it back to the
  thread that called `join`.
- `F: Fn(&T) -> R + Sync` — the *same* closure runs on many threads, so it must
  be `Sync` and must be `Fn` (no shared mutable state; `FnMut` would be a race).
  Passing `f` as `&F` down the recursion avoids needing `F: Clone`.

Parallel quicksort is the same skeleton, but the disjointness that makes parallel
*mutation* sound comes from `split_at_mut`:

```rust
fn parallel_quicksort<T: Ord + Send>(data: &mut [T]) {
    if data.len() <= THRESHOLD { data.sort_unstable(); return; }

    let len = data.len();
    data.swap(len / 2, len - 1);                 // mid as pivot: avoids O(n^2) on sorted input

    let mut p = 0;                               // Lomuto partition
    for i in 0..len - 1 {
        if data[i] <= data[len - 1] { data.swap(i, p); p += 1; }
    }
    data.swap(p, len - 1);                        // pivot to its final resting place

    let (left, pivot_and_right) = data.split_at_mut(p);
    let (_, right) = pivot_and_right.split_at_mut(1);   // skip the pivot
    rayon::join(|| parallel_quicksort(left), || parallel_quicksort(right));
}
```

> `split_at_mut` hands back **two disjoint `&mut` halves**. That non-overlap is
> exactly what lets rayon sort both sides in parallel safely — the borrow checker
> knows the two `&mut [T]` can't alias, so there is no data race, and no `unsafe`
> is needed. This is the same trick as `split_at_mut` in the scoped-threads
> ladder, now powering a parallel sort. Choosing the middle element as pivot
> (`swap(len/2, len-1)`) is the standard defense against quicksort's O(n²)
> worst case on already-sorted or reversed input — which the test deliberately
> feeds it.

## Explain it back

- Why does `reduce` take an *identity closure* instead of a single initial value,
  while `Iterator::fold` takes a value?
- What is work-stealing, and why does it beat statically chunking a slice into N
  equal pieces?
- `rayon::join(a, b)` "potentially" runs in parallel. What does it actually do
  when no worker is idle, and why is that not a waste?
- You parallelized a sum over a million `u64`s and it got *slower*. Give two
  reasons and the rule for when to expect a speedup.
- Why does `par_iter().for_each(|x| vec.push(x))` fail to compile? What two fixes
  exist and which is better?
- A parallel reduce with subtraction gives a different answer every run. What law
  is broken and why does the parallel split expose it?
- In the capstone `parallel_map`, justify each bound: `T: Sync`, `R: Send`,
  `F: Fn(&T) -> R + Sync`.
- What makes parallel in-place quicksort sound without `unsafe`?

## See also

- [Threads & scoped threads](threads.md) — `thread::scope`, `split_at_mut`, the
  hand-rolled `parallel_map` that this ladder rebuilds on `rayon::join`.
- [`Send` & `Sync` deeply](send-sync.md) — the bounds that make all of rayon sound.
- [`Mutex` / `RwLock`](mutex-rwlock.md) — the lock used (and avoided) in rung 7.
- [Iterators end-to-end](iterators.md) — the sequential adapters `par_iter` mirrors.
- [Closures & `Fn`/`FnMut`/`FnOnce`](closures.md) — why `for_each` needs `Fn`.
