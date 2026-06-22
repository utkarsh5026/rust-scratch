//! `impl Trait` & RPIT — `impl Trait` in args/returns, `async fn` desugaring
//!
//! Run: `cargo run --bin impl_trait`
//!
//! The one question that unlocks everything: WHO PICKS THE TYPE?
//!   - arg position  `fn f(x: impl Trait)`     -> the CALLER picks (sugar for a generic param)
//!   - return position `fn f() -> impl Trait`  -> the CALLEE picks (one hidden concrete type)
//!
//! Ladder (mark DONE as you go):
//!   1. [DONE] APIT basics            — `impl Display` arg; caller picks                 (foundations)
//!   2. [DONE] RPIT basics            — return `impl Iterator`; type unspellable         (foundations)
//!   3. [DONE] who picks + turbofish  — APIT == generic; impl-arg kills turbofish        (mechanics)
//!   4. [DONE] the killer app         — return a closure & an adapter chain              (mechanics)
//!   5. [DONE] one hidden type        — if/else two iterators won't compile; fix 3 ways  (footgun)
//!   6. [DONE] RPIT captures lifetime — borrow input; 2024 auto-capture + `use<>`         (footgun)
//!   7. [DONE] async fn IS rpit       — `async fn` ≡ `-> impl Future`; Send bound        (real-world)
//!   8. [DONE] RPITIT                  — impl Trait in trait return; async-fn-in-trait    (real-world)
//!   9. [WIP ] capstone               — combinator toolkit, impl Trait everywhere        (capstone)

use std::fmt::Display;

// ───────────────────────────── Problem 1: APIT basics ─────────────────────────────
// `impl Trait` in ARGUMENT position. This is just sugar for `<T: Display>`.
// The CALLER decides what concrete type flows in.
//
// Your turn: implement `describe` so it returns a String "[<value>]" using the
// `Display` impl of whatever was passed. e.g. describe(42) == "[42]",
// describe("hi") == "[hi]". The signature must keep `x: impl Display`.
fn describe(x: impl Display) -> String {
    format!("[{x}]")
}

fn check_1() {
    assert_eq!(describe(42), "[42]");
    assert_eq!(describe("hi"), "[hi]");
    assert_eq!(describe(3.5), "[3.5]");
    // Same function body served three different concrete types — the caller picked each.
    println!("check_1 ok: APIT — caller picks the type");
}

// ───────────────────────────── Problem 2: RPIT basics ─────────────────────────────
// `impl Trait` in RETURN position ("RPIT"). Now the CALLEE picks ONE hidden concrete
// type. The caller can't name it — they only know "it implements Iterator<Item=u32>".
//
// Your turn: implement `evens_up_to(n)` to return an iterator over the even numbers
// 0, 2, 4, ... that are < n. Build it from range + iterator adapters; do NOT collect
// into a Vec, and do NOT write the concrete return type — that's the whole point.
// Try to even WRITE the concrete type in the signature and watch it be impossible.
fn evens_up_to(n: u32) -> impl Iterator<Item = u32> {
    (0..n).filter(|x| x % 2 == 0)
}

fn check_2() {
    let v: Vec<u32> = evens_up_to(10).collect();
    assert_eq!(v, vec![0, 2, 4, 6, 8]);
    // The caller had to `.collect()` to get something nameable — they never named
    // the iterator's real type (it's some Filter<Range<u32>, {closure}>).
    println!("check_2 ok: RPIT — callee picks one hidden type");
}

// ──────────────────── Problem 3: who picks + the turbofish footgun ────────────────────
// APIT desugars to a generic param — so the CALLER picks the type. But there's a real,
// observable consequence: with `impl Trait` in an arg you CANNOT use turbofish to pick
// the type explicitly, because there's no named type parameter to fill. With a real
// generic param, you can.
//
// Two functions that do the same thing, written both ways:
//   make_one_apli(x: impl Display)   — impl-Trait arg, no turbofish possible
//   make_one_generic::<T: Display>() — named param, turbofish works
//
// Your turn:
//  (a) implement `count_args` (impl-Trait style) to return how many chars the
//      Display form of `x` has. e.g. count_args(123) == 3, count_args("hello") == 5.
//  (b) implement `default_string::<T>()` — a GENERIC fn (named param `T: Default +
//      Display`) returning T::default()'s Display form. It takes NO value arg, so the
//      ONLY way to call it is turbofish: default_string::<i32>() == "0".
//      (This is the case impl-Trait-in-arg literally cannot express — there's no
//       value to infer T from, so you NEED a nameable param.)
fn count_args(x: impl Display) -> usize {
    x.to_string().len()
}

fn default_string<T: Default + Display>() -> String {
    T::default().to_string()
}

fn check_3() {
    assert_eq!(count_args(123), 3);
    assert_eq!(count_args("hello"), 5);
    // The payoff line: this can ONLY be called by naming T with turbofish.
    assert_eq!(default_string::<i32>(), "0");
    assert_eq!(default_string::<bool>(), "false");
    println!("check_3 ok: APIT == generic, but only a named param takes turbofish");
}

// ──────────────────── Problem 4: the killer app — return a closure / chain ────────────────────
// The real reason RPIT exists: closures and iterator-adapter chains have types you
// literally cannot write down. Before `impl Trait`, your only option was `Box<dyn ...>`
// (heap + vtable). RPIT lets you return them by value, monomorphized, zero overhead.
//
// Your turn:
//  (a) `adder(n)` returns a CLOSURE that adds n to its argument. A closure's type is
//      anonymous, so the return type MUST be `impl Fn(i32) -> i32`. Note `n` is captured
//      by the closure — think about `move`.
//  (b) `pipeline(words)` takes a slice of &str and returns an ITERATOR (not a Vec) that
//      yields the uppercased form of each word whose length is > 3. Chain adapters;
//      the return type is `impl Iterator<Item = String>`.
//      (Heads up: this borrows `words`. If the borrow checker complains about a lifetime,
//       park it — that exact problem is rung 6. For now `words: &[&str]` with the default
//       elided lifetime should work because each yielded String is owned.)
fn adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| x + n
}

fn pipeline<'a>(words: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
    words
        .iter()
        .filter(|word| word.len() > 3)
        .map(|word| word.to_uppercase())
}

fn check_4() {
    let add5 = adder(5);
    assert_eq!(add5(10), 15);
    assert_eq!(add5(0), 5);

    let words = ["hi", "rust", "ok", "trait", "go"];
    let out: Vec<String> = pipeline(&words).collect();
    assert_eq!(out, vec!["RUST".to_string(), "TRAIT".to_string()]);
    println!("check_4 ok: returned a closure and an adapter chain — no Box, no vtable");
}

// ──────────────────── Problem 5: one hidden type — all branches must agree ────────────────────
// RPIT = ONE hidden concrete type. So this DOESN'T compile (uncomment to see E0308:
// "`if` and `else` have incompatible types" — Range vs Rev<Range>):
//
//   fn ranged_broken(rev: bool, n: u32) -> impl Iterator<Item = u32> {
//       if rev { (0..n).rev() } else { 0..n }   // two different types ⇒ rejected
//   }
//
// Fix it THREE ways, each a real tool you'll reach for:
//
//  (a) ERASE with a trait object: return `Box<dyn Iterator<Item = u32>>`. Both arms
//      coerce to the SAME erased type. Cost: heap alloc + dynamic dispatch.
//
//  (b) UNIFY to one concrete type: collect each arm into a Vec and return
//      `vec.into_iter()`. Now BOTH arms are `std::vec::IntoIter<u32>` — one type, so
//      plain `impl Iterator` works again. Cost: eager allocation, loses laziness.
//
//  (c) BRANCH-AS-DATA with an enum: implement `Iterator` for the `Either` enum below,
//      then return `Either::Left(..)` / `Either::Right(..)` as `impl Iterator`. One
//      type (the enum), no heap, stays lazy. This is exactly what `itertools::Either` is.

enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Iterator for Either<L, R>
where
    L: Iterator,
    R: Iterator<Item = L::Item>,
{
    type Item = L::Item;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Either::Left(l) => l.next(),
            Either::Right(r) => r.next(),
        }
    }
}

fn ranged_box(rev: bool, n: u32) -> Box<dyn Iterator<Item = u32>> {
    if rev {
        Box::new((0..n).rev())
    } else {
        Box::new(0..n)
    }
}

fn ranged_vec(rev: bool, n: u32) -> impl Iterator<Item = u32> {
    if rev {
        (0..n).rev().collect::<Vec<_>>().into_iter()
    } else {
        (0..n).collect::<Vec<_>>().into_iter()
    }
}

fn ranged_either(rev: bool, n: u32) -> impl Iterator<Item = u32> {
    if rev {
        Either::Left((0..n).rev())
    } else {
        Either::Right(0..n)
    }
}

fn check_5() {
    let fwd = vec![0u32, 1, 2, 3, 4];
    let rev = vec![4u32, 3, 2, 1, 0];

    assert_eq!(ranged_box(false, 5).collect::<Vec<_>>(), fwd);
    assert_eq!(ranged_box(true, 5).collect::<Vec<_>>(), rev);

    assert_eq!(ranged_vec(false, 5).collect::<Vec<_>>(), fwd);
    assert_eq!(ranged_vec(true, 5).collect::<Vec<_>>(), rev);

    assert_eq!(ranged_either(false, 5).collect::<Vec<_>>(), fwd);
    assert_eq!(ranged_either(true, 5).collect::<Vec<_>>(), rev);

    println!("check_5 ok: erase (Box) / unify (Vec) / branch-as-data (Either)");
}

// ──────────────────── Problem 6: RPIT captures lifetimes (edition 2024) ────────────────────
// An RPIT hides a concrete type. That type may BORROW from the function's inputs — so
// the question is: which lifetimes/type-params does the hidden type "capture"?
//
// HISTORY (edition 2021): RPIT captured NOTHING unless you spelled it. Borrowing an
// input gave E0700 ("hidden type captures lifetime that does not appear in bounds"),
// and you fixed it by adding `+ '_` / `+ 'a` to the return type.
//
// NOW (edition 2024, this crate): RPIT auto-captures ALL in-scope generic params and
// lifetimes. So (a) below "just works" with no `+ 'a`. The new skill is the OPPOSITE:
// opting OUT of an over-broad capture with the precise-capturing `+ use<...>` syntax.
//
//  (a) `lengths(words)` borrows `words` internally but yields an OWNED `usize` per word.
//      On 2024 you do NOT need `+ 'a` — auto-capture handles it. Implement the body.
//      (On 2021 this exact fn needs `-> impl Iterator<Item = usize> + '_` or it's E0700.)
//
//  (b) `counter(_data)` ignores its borrowed arg and returns `0..3` — the result owns
//      nothing. But 2024 auto-capture still captures `'a`, so the returned iterator is
//      (wrongly) tied to the borrow's lifetime. check_6 tries to let it OUTLIVE the
//      borrowed slice and you'll get E0597 ("does not live long enough"). Fix it by
//      making the capture set EMPTY: add `+ use<>` to counter's return type.
fn lengths<'a>(words: &'a [&'a str]) -> impl Iterator<Item = usize> {
    words.iter().map(|word| word.len())
}

// (b) TODO: this signature OVER-captures `'a`. After you uncomment check_6 you'll get
// E0597. Add `+ use<>` here to capture nothing, so the iterator can outlive `_data`.
fn counter<'a>(_data: &'a [i32]) -> impl Iterator<Item = i32> + use<> {
    (0..3).collect::<Vec<_>>().into_iter()
}

fn check_6() {
    let words = ["a", "bb", "ccc"];
    assert_eq!(lengths(&words).sum::<usize>(), 6);

    let it = {
        let data = vec![10, 20, 30];
        counter(&data)
    };
    assert_eq!(it.collect::<Vec<_>>(), vec![0, 1, 2]);

    println!("check_6 ok: auto-capture (a) and `use<>` opt-out (b)");
}

// ──────────────────── Problem 7: `async fn` IS return-position impl Trait ────────────────────
// THE big reveal. These two signatures are the SAME thing:
//
//     async fn f(x: u32) -> u32 { ... }
//     fn        f(x: u32) -> impl Future<Output = u32> { async move { ... } }
//
// `async fn` is pure sugar: the compiler turns the body into a state machine of some
// anonymous type that implements `Future`, and hands it back via RPIT. The hidden
// `Output` type is the thing after the original `->`. Every RPIT rule you learned still
// applies — including capture (the future borrows what the async block borrows) and the
// `Send` question (is the state machine `Send`? only if everything held across an
// `.await` is `Send`).
//
// Your turn — write the SAME async function two ways and prove they're equal:
//
//  (a) `double_async`: a normal `async fn` returning u32 that returns x * 2.
//  (b) `double_rpit`:  a plain `fn` whose return type you write as
//      `impl Future<Output = u32>`, with body `async move { x * 2 }`. No `async fn`.
//  (c) `sum_then`: `async fn` that `.await`s BOTH of the above and returns their sum,
//      proving the desugared one is awaitable exactly like the sugared one.
//
// check_7 builds a tokio runtime and blocks on the futures (main stays sync).
// `assert_send` shows an async block is only `Send` if its awaited state is `Send` —
// the same auto-trait reasoning from your send_sync ladder, now applied to RPIT.

use std::future::Future;

async fn double_async(x: u32) -> u32 {
    x * 2
}

fn double_rpit(x: u32) -> impl Future<Output = u32> {
    async move { x * 2 }
}

async fn sum_then(x: u32) -> u32 {
    double_async(x).await + double_rpit(x).await
}

fn assert_send<T: Send>(_: &T) {}

fn check_7() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        assert_eq!(double_async(21).await, 42);
        assert_eq!(double_rpit(21).await, 42);
        assert_eq!(sum_then(10).await, 40);

        let fut = double_rpit(5);
        assert_send(&fut);
        assert_eq!(fut.await, 10);
    });
    println!("check_7 ok: async fn == fn -> impl Future; both awaitable, both Send");
}

// ──────────────────── Problem 8: RPITIT — impl Trait in trait method returns ────────────────────
// Since Rust 1.75 you can put `impl Trait` in a TRAIT method's return type ("RPITIT").
// And `async fn` in traits is just RPITIT under the hood:
//
//     trait T { async fn f(&self) -> S; }
//       ≡  trait T { fn f(&self) -> impl Future<Output = S>; }
//
// THE CATCH: a trait with an RPITIT (or async fn) method is NOT dyn-compatible — each
// impl returns a DIFFERENT hidden type, so there's no single vtable signature. You can
// use it through GENERICS (`impl Trait` / `<T: Trait>`, static dispatch) but not as
// `&dyn Trait`. (Try uncommenting the `dyn` line in check_8 to witness E0038.)
//
// Your turn:
//  (a) Implement `Source::values` for `Squares` — yield 1, 4, 9 (squares of 1..=3) as an
//      iterator (RPITIT). The trait method returns `impl Iterator<Item = u32>`.
//  (b) Implement `Greeter::greet` for `Robot` (an `async fn` in a trait) to return
//      "beep <name>". This desugars to RPITIT returning a Future.
//  (c) `sum_source` is GENERIC over `impl Source` — fill it in to sum the values. This is
//      how you consume an RPITIT trait: static dispatch, because `dyn Source` is illegal.

trait Source {
    fn values(&self) -> impl Iterator<Item = u32>;
}

struct Squares;
impl Source for Squares {
    fn values(&self) -> impl Iterator<Item = u32> {
        (1..=3).map(|x| x * x)
    }
}

trait Greeter {
    async fn greet(&self) -> String;
}

struct Robot {
    name: &'static str,
}
impl Greeter for Robot {
    async fn greet(&self) -> String {
        format!("beep {}", self.name)
    }
}

fn sum_source(s: impl Source) -> u32 {
    s.values().sum()
}

fn check_8() {
    // (a) + (c): consume the RPITIT trait via generics. 1+4+9 == 14.
    assert_eq!(sum_source(Squares), 14);

    // (b): async fn in a trait, awaited on the runtime.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let msg = rt.block_on(Robot { name: "R2" }.greet());
    assert_eq!(msg, "beep R2");

    // Uncomment to witness E0038: `Source` is not dyn-compatible because `values`
    // is an RPITIT method (each impl returns a different hidden iterator type):
    //   let _boxed: Box<dyn Source> = Box::new(Squares);

    println!("check_8 ok: RPITIT via generics; async fn in trait; dyn is illegal here");
}

// ──────────────────── Problem 9 (capstone): a combinator toolkit ────────────────────
// Build a tiny lazy data-processing toolkit. EVERY builder hands back `impl Trait` —
// except the ONE place where runtime branching forces you to erase to `Box<dyn>`.
// The whole point: feel exactly where the static `impl Trait` machinery runs out and
// type erasure becomes mandatory.
//
// Implement each `todo!`, then the assembled pipeline in check_9 lights up:
//
//  (a) compose(f, g)  — APIT bounds in, RPIT closure out. Returns a closure computing
//      g(f(a)). This is RPIT + APIT in one signature.
//  (b) naturals()     — an infinite lazy source: 1, 2, 3, ... as `impl Iterator<Item=u64>`.
//  (c) keep(it, pred) — a generic combinator threading ANY iterator `I` through a filter,
//      returning `impl Iterator<Item = I::Item>`. Stays lazy; no collect.
//  (d) op_fn(op)      — runtime-selected unary op. Each match arm is a DIFFERENT closure
//      type, so the one-hidden-type rule means RPIT can't express it: you MUST return
//      `Box<dyn Fn(u64) -> u64>`. This is the deliberate "erase here" spot.
//  (e) MulStage::apply — a trait `Stage` with APIT input + RPITIT output: takes
//      `impl Iterator<Item=u64>`, returns `impl Iterator<Item=u64>` scaled by the factor.

fn compose<A, B, C>(f: impl Fn(A) -> B, g: impl Fn(B) -> C) -> impl Fn(A) -> C {
    move |a: A| -> C { g(f(a)) }
}

fn naturals() -> impl Iterator<Item = u64> {
    (1..).into_iter()
}

fn keep<I: Iterator>(it: I, pred: impl Fn(&I::Item) -> bool) -> impl Iterator<Item = I::Item> {
    it.filter(move |x| pred(x))
}

enum Op {
    Inc,
    Double,
    Square,
}

fn op_fn(op: Op) -> Box<dyn Fn(u64) -> u64> {
    match op {
        Op::Inc => Box::new(|x| x + 1),
        Op::Double => Box::new(|x| x * 2),
        Op::Square => Box::new(|x| x * x),
    }
}

trait Stage {
    fn apply(&self, input: impl Iterator<Item = u64>) -> impl Iterator<Item = u64>;
}

struct MulStage(u64);
impl Stage for MulStage {
    fn apply(&self, input: impl Iterator<Item = u64>) -> impl Iterator<Item = u64> {
        let factor = self.0;
        input.map(move |x| x * factor)
    }
}

fn check_9() {
    // (a) compose: (x + 1) then (* 2)
    let f = compose(|x: i32| x + 1, |x: i32| x * 2);
    assert_eq!(f(10), 22);

    // (b) infinite lazy source, tamed by take
    assert_eq!(naturals().take(3).collect::<Vec<_>>(), vec![1, 2, 3]);

    // (c) generic combinator threads naturals() through a filter, stays lazy
    let evens: Vec<u64> = keep(naturals(), |n: &u64| n % 2 == 0).take(3).collect();
    assert_eq!(evens, vec![2, 4, 6]);

    // (d) runtime-selected op: different closure type per branch => Box<dyn> forced
    assert_eq!(op_fn(Op::Inc)(5), 6);
    assert_eq!(op_fn(Op::Double)(5), 10);
    assert_eq!(op_fn(Op::Square)(5), 25);

    // (e) RPITIT stage: APIT iterator in, RPITIT iterator out
    let scaled: Vec<u64> = MulStage(10).apply(naturals()).take(3).collect();
    assert_eq!(scaled, vec![10, 20, 30]);

    // FULL PIPELINE, fully lazy until the final collect:
    //   naturals -> keep evens (2,4,6) -> stage *10 (20,40,60) -> post (x+1)*2 (42,82,122)
    let post = compose(|x: u64| x + 1, |x: u64| x * 2);
    let out: Vec<u64> = MulStage(10)
        .apply(keep(naturals(), |n: &u64| n % 2 == 0))
        .map(move |x| post(x))
        .take(3)
        .collect();
    assert_eq!(out, vec![42, 82, 122]);

    println!(
        "check_9 ok: combinator toolkit — RPIT/APIT/RPITIT throughout, Box<dyn> only where branching forces it"
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
