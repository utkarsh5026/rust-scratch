# Threads & scoped threads

> Ladder: [`src/bin/threads.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/threads.rs) ·
> Run: `cargo run --bin threads` · Phase 4 · 9 rungs

## TL;DR

There are two ways to start an OS thread in std, and the whole topic is the difference between them:

- **`thread::spawn`** launches a thread that can **outlive** the function that started it. Because nobody promises when it ends, its closure must own everything it touches: the bound is `F: 'static`. You get a `JoinHandle<T>` to later collect the thread's return value (or its panic).
- **`thread::scope`** opens a region that **blocks until every thread spawned inside it has finished**, right at the closing brace. Since the threads provably can't escape that region, the borrow checker relaxes `'static` down to "outlives the scope" — so scoped threads can **borrow local variables**, even mutably.

`'static` ownership versus structured borrowing. That is the entire mental model.

## Why this exists (from first principles)

A thread is a separate flow of execution that the OS scheduler can run at any time, in any order, possibly still running after the function that spawned it has returned. That last clause is the source of every rule here.

Consider what `spawn` would have to allow if it let a closure borrow a local:

```rust
fn danger() {
    let data = vec![1, 2, 3];
    thread::spawn(|| println!("{:?}", data)); // borrows `data`
} // <- `data` is dropped HERE, freeing its heap buffer
  //    ...but the spawned thread may not have run yet.
```

The thread holds a reference into `data`'s heap allocation, but `danger` frees that allocation the instant it returns. If the scheduler runs the thread afterward, it reads freed memory — a use-after-free. Rust has no garbage collector and no runtime to keep `data` alive, so the only way to make this sound at compile time is to forbid it. That is what the `'static` bound on `spawn` does: **any reference the closure captures must be valid for the entire rest of the program** (`&'static`), which a borrow of a local is not.

`'static` does *not* mean "no outside data." Owned data is fine — you can `move` a `String` or `Vec` into the thread, transferring ownership so there is no dangling borrow to worry about. `'static` specifically forbids *borrows that could dangle*.

So `spawn` gives you safety by demanding ownership. But sometimes you genuinely want to lend a local to a few threads, run them in parallel, and get the borrow back — the classic "split this array across cores" pattern. `thread::scope` exists to make exactly that sound: it adds the one missing guarantee (all threads join before the borrow ends) so the borrow checker can permit the borrow.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | Foundations | Spawn & join | `spawn` returns a `JoinHandle<T>`; `join()` retrieves the value |
| 2 | Foundations | Many handles | Spawn all, *then* join all — joining in the loop serializes |
| 3 | Mechanics | `move` & ownership | The closure must own captured data; `move` makes it `'static` |
| 4 | Mechanics | Panicking threads | A panic is caught and returned as `join() -> Err(payload)` |
| 5 | Footgun | The `'static` wall | Borrowing a local in `spawn` fails (E0373/E0597) — and why |
| 6 | Footgun | `thread::scope` rescue | Same borrow, now legal; many shared `&` reads at once |
| 7 | Real-world | Scoped parallel mutate | `split_at_mut` → disjoint `&mut` chunks mutated in parallel |
| 8 | Real-world | Parallel fold / fan-in | Map-reduce: chunk → partial results → combine in main |
| 9 | Capstone | `parallel_map` | A generic, order-preserving rayon-lite |

## The ideas, built up

### 1. Spawn returns a handle; join collects the result

```rust
fn spawn_and_join() -> i32 {
    let handle = thread::spawn(|| 2 + 2);
    handle.join().unwrap()
}
```

`thread::spawn(closure)` returns immediately with a `JoinHandle<T>`, where `T` is the closure's return type. The new thread runs concurrently. `handle.join()` blocks the calling thread until that thread finishes, then hands back its result.

Why does `join()` return a `Result` rather than a bare `T`? Because the thread might not have finished *cleanly* — it could have panicked. `Ok(value)` means it returned normally; `Err(payload)` means it panicked. The `.unwrap()` here says "I expect success," which is fine until rung 4 deliberately breaks it.

### 2. Spawn all, then join all

```rust
fn squares_in_parallel(n: usize) -> Vec<usize> {
    let mut handles = Vec::with_capacity(n);
    for i in 0..n {
        handles.push(thread::spawn(move || i * i)); // spawn ALL first
    }
    handles.into_iter().map(|h| h.join().unwrap()).collect() // THEN join
}
```

The trap this rung sets: if you call `.join()` *inside* the spawn loop, each thread finishes before the next one starts, and you have written sequential code with extra steps. To get real parallelism you must **launch every thread first**, then join them in a second pass.

There is a subtle correctness point too: the results come out **in spawn order, not completion order**. Thread 4 might finish before thread 1, but because you join the handles in the order you stored them, the squares land sorted regardless of who finished when. Order is determined by *which handle you join*, never by timing.

Note `move || i * i`: each iteration's `i` is captured by value, so every closure owns its own copy. Without `move`, the closure would try to borrow the loop variable.

### 3. `move` and the `'static` contract

```rust
fn append_in_thread(s: String) -> String {
    let handle = thread::spawn(move || s + " world");
    handle.join().unwrap()
}
```

Write this without `move` and the compiler stops you with **E0373: closure may outlive the current function, but it borrows `s`**. That error *is* the `'static` rule speaking. `move` resolves it by transferring ownership of `s` into the closure: the thread now owns the `String`, there is no borrow of a local left to dangle, and the closure satisfies `'static`.

(`s + " world"` consumes the owned `String` and reuses its buffer — a small idiom worth noting, but the lesson is the `move`.)

### 4. A panic in a thread is caught, not propagated

```rust
fn catch_panic_message() -> String {
    let handle = thread::spawn(|| { panic!("boom"); });
    match handle.join() {
        Ok(value) => value,
        Err(payload) => payload.downcast_ref::<&str>().unwrap().to_string(),
    }
}
```

A panic in a spawned thread does **not** unwind into the parent. It unwinds that thread, is captured at the thread boundary, and is delivered to whoever calls `join()` as `Err(payload)`. This is why threads isolate failure: one worker blowing up doesn't tear down `main`.

The payload type is `Box<dyn Any + Send + 'static>` — type-erased, because a panic can carry any value. `panic!("boom")` with a string literal stores a `&'static str`, so you recover it with `downcast_ref::<&str>()`. (A `panic!("{}", x)` with formatting stores a `String` instead, so the downcast target depends on how the panic was raised.)

> **Caveat:** this catch-and-return behavior only exists under the default `panic = "unwind"`. If the crate is built with `panic = "abort"`, a panicking thread aborts the whole process and `join` never gets the chance to return `Err`.

### 5. The `'static` wall, seen directly

This rung is built to *fail to compile* — the error is the lesson.

```rust
fn sum_with_spawn() -> i32 {
    let data = vec![1, 2, 3, 4];
    let handle = thread::spawn(|| data.iter().sum::<i32>()); // E0373/E0597
    handle.join().unwrap()
}
```

The compiler rejects the borrow of `data` because `spawn` requires `'static` and `data` is a local that drops when the function returns. Adding `move` makes it compile — but notice that it is now a *different program*: the thread **owns** `data`, so the function can no longer use `data` afterward, and if `data` had been borrowed from a caller you don't own, `move` wouldn't even be available.

That gap — "I want to *lend* a local to a thread and get it back" — is precisely the hole the next rung fills.

### 6. `thread::scope` relaxes `'static` to "outlives the scope"

```rust
fn sum_halves_scoped(data: &[i32]) -> i32 {
    let mid = data.len() / 2;
    thread::scope(|s| {
        let s1 = s.spawn(|| data[..mid].iter().sum::<i32>());
        let s2 = s.spawn(|| data[mid..].iter().sum::<i32>());
        s1.join().unwrap() + s2.join().unwrap()
    })
}
```

`thread::scope(|s| { ... })` gives you a scope handle `s`; you spawn with `s.spawn(...)` instead of `thread::spawn(...)`. The defining property: **the scope does not return until every thread spawned inside it has finished**. That join happens automatically at the closing brace.

Because the runtime now *guarantees* the threads end before the scope (and therefore before `data`) does, the borrow only needs to outlive the scope, not the whole program. So the closures can capture `&data` and `mid` **by reference, with no `move` and no `'static`**. Two threads holding shared `&` borrows of the same slice at once is fine — shared reads never conflict. No `Arc`, no `Mutex`, no clone.

### 7. Disjoint mutable borrows in parallel

Shared reads were easy. The hard, genuinely useful case is *mutating* different parts of one slice from different threads.

```rust
fn double_in_parallel(data: &mut [i32]) {
    let (left, right) = data.split_at_mut(data.len() / 2);
    thread::scope(|s| {
        s.spawn(move || left.iter_mut().for_each(|x| *x *= 2));
        s.spawn(move || right.iter_mut().for_each(|x| *x *= 2));
    });
}
```

Two `&mut` into the same slice is normally a borrow-checker violation. The key that unlocks it is `slice::split_at_mut`, which returns two **provably non-overlapping** mutable sub-slices — the standard library guarantees they share no element, so handing each to a different thread races nothing.

Note the closures need `move` this time, in contrast to rung 6. A `&mut` is not `Copy` and cannot be shared, so each thread must *take* its sub-slice by moving it in. And since the scope joins at `}`, you can let the threads auto-join without binding their handles. This split → scope → parallel-mutate skeleton is exactly how `rayon`'s `par_iter_mut` works underneath.

### 8. Map-reduce: fan out partials, fold them in

```rust
fn parallel_sum_of_squares(data: &[i64], n: usize) -> i64 {
    if data.is_empty() || n == 0 { return 0; }
    thread::scope(|s| {
        let chunk_len = (data.len() + n - 1) / n; // ceil(len / n)
        let mut handles = Vec::with_capacity(n);
        for chunk in data.chunks(chunk_len) {
            handles.push(s.spawn(move || chunk.iter().map(|x| x * x).sum::<i64>()));
        }
        handles.into_iter().map(|h| h.join().unwrap()).sum()
    })
}
```

The generalized form of rung 7 and the heart of map-reduce: split into chunks, each worker computes a **partial** result, the main thread **combines** the partials. `data.chunks(chunk_len)` yields shared `&[i64]` sub-slices; `chunk_len = ceil(len / n)` ensures at most `n` chunks; the guards handle `n == 0` and empty input.

The collect-then-join discipline from rung 2 reappears: push every handle, *then* join them, so the workers actually run concurrently. (For a sum the order of partials is irrelevant, but the parallelism still depends on not joining inside the loop.)

## Footguns

| Trap | What happens | Fix |
|------|--------------|-----|
| `join()` inside the spawn loop | Threads run one at a time — no parallelism | Spawn all handles first, join in a second pass |
| Borrowing a local in `thread::spawn` | E0373 / E0597 — closure may outlive the borrow | `move` to transfer ownership, or use `thread::scope` to borrow |
| Reaching for `move` to "fix" rung 5 | Compiles, but the thread now *owns* the data — a different program | Use `thread::scope` when you need to *borrow* and get it back |
| Two `&mut` into one slice | Borrow-checker rejection | `split_at_mut` / `chunks_mut` for provably-disjoint sub-slices |
| Forgetting the panic boundary | Under `panic = "abort"`, a thread panic kills the process | Default `unwind` lets `join()` return `Err`; don't rely on it under abort |
| Assuming result order = finish order | Results follow handle order, not timing | Keep handles ordered; order is deterministic regardless of scheduling |

## Real-world patterns

- **Structured parallelism (`thread::scope`)** is the modern default for "fork a few workers over borrowed data and join them here." Before it was stabilized (Rust 1.63), this required `Arc` + `'static` gymnastics or unsafe; now it's a safe, allocation-free block.
- **`split_at_mut` / `chunks_mut`** is the standard way to express "these regions are disjoint" to the borrow checker — the foundation of every data-parallel mutation.
- **Collect-then-join** is how you keep `JoinHandle`s parallel; joining eagerly is the most common accidental-serialization bug.
- **`rayon`** is the production answer to everything in rungs 6–9: `par_iter()`, `par_iter_mut()`, `map().sum()`, `map().collect()`. The capstone is a hand-rolled, fixed-chunk version of exactly that API, which is why writing it cements what `rayon` does for you.

## Capstone insight

```rust
fn parallel_map<T, R, F>(data: &[T], n: usize, f: F) -> Vec<R>
where
    T: Sync,                 // workers share &[T]; &T crossing threads needs Sync
    R: Send,                 // each result is produced on a worker and moved back
    F: Fn(&T) -> R + Sync,   // f runs on many threads at once → &F must cross → Sync
{
    if data.is_empty() || n == 0 { return Vec::new(); }
    thread::scope(|s| {
        let f = &f;                                   // borrow the closure ONCE
        let chunk_len = (data.len() + n - 1) / n;
        let mut handles = Vec::with_capacity(n);
        for chunk in data.chunks(chunk_len) {
            handles.push(s.spawn(move || chunk.iter().map(f).collect::<Vec<R>>()));
        }
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect() // concat in order
    })
}
```

Three insights make this work, and each is a payoff from an earlier rung:

1. **The bounds are forced by data flow, not decoration.** `T: Sync` because each worker reads `&[T]` and a `&T` only crosses a thread boundary safely if `T: Sync`. `R: Send` because each result is created on a worker and *moved* back to main. `F: Fn(&T) -> R + Sync` because the same closure is called from several threads concurrently, so `&F` must cross threads — which is what `Sync` certifies. Write it without the bounds and the compiler dictates them one error at a time.

2. **`let f = &f;` then `move ||` shares one closure.** A bare `move` would try to move `f` into every worker, but you only have one `f`. Rebinding to a shared reference and moving *that reference* (which is `Copy`) lets all workers borrow the single closure. This is the concrete reason the bound is `F: Sync` rather than `F: Send + Clone`.

3. **Order falls out of structure.** Each worker returns a `Vec<R>` for *its* chunk. Because `data.chunks()` yields chunks left to right and the handles stay in that order, `flat_map` over the joined results concatenates per-chunk outputs in global order — no sorting, no indices. Order preservation is a consequence of keeping handles ordered (the rung-2 lesson), not an extra step.

Put together: scope provides the borrowing, chunking provides the parallel decomposition, the `Send`/`Sync` bounds provide the safety proof, and ordered handles provide deterministic output. That is `rayon::par_iter().map().collect()` with the lid off.

## Explain it back

- Why does `thread::spawn` require `F: 'static`, and why is moving an owned `String` in still allowed?
- What does `thread::scope` guarantee that lets the borrow checker accept a borrow of a local? Where exactly does the join happen?
- You spawned 5 threads and joined them in order. A later-spawned thread finishes first. What order are the results in, and why?
- Why does `join()` return a `Result`? Under what build setting does that stop being true?
- Why is `split_at_mut` the thing that makes parallel mutation type-check, when two `&mut` to a slice normally don't?
- In `parallel_map`, why is the bound on `F` `Sync` and not `Send`? What would change if each worker needed its own copy of `f`?

## See also

- [`Send` & `Sync`](https://doc.rust-lang.org/nomicon/send-and-sync.html) — the two marker traits the capstone's bounds rest on (next ladder in Phase 4).
- [Rc / Arc](./rc-arc.md) — `Arc<Mutex<T>>` is the alternative when you *can't* use a scope (threads that outlive the spawning frame).
- [Lifetimes in depth](./lifetimes-depth.md) — the `'static` bound and "outlives" reasoning that scope relaxes.
- [Drop & ordering](./drop-ordering.md) — why "the scope joins at the closing brace" matters for when borrows end.
