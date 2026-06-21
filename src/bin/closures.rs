// Closures & Fn / FnMut / FnOnce
// Run: cargo run --bin closures
//
// Mental model: a closure is an anonymous struct the compiler generates.
// Its fields are the variables it captures; HOW it captures them (by &, by &mut,
// or by value) decides which trait it implements:
//   Fn      -> only reads captures      (callable via &self)
//   FnMut   -> mutates captures         (&mut self)
//   FnOnce  -> consumes captures        (self, callable once)
// They nest:  Fn  ⊂  FnMut  ⊂  FnOnce.
//
// Ladder:
//   1. [x] foundations — define & call a closure; capture a local by reference
//   2. [x] foundations — the three capture modes (borrow / mut-borrow / move)
//   3. [x] mechanics   — the Fn/FnMut/FnOnce hierarchy via three call-helpers
//   4. [x] mechanics   — desugar a closure by hand (struct + method)
//   5. [x] footgun     — FnMut needs `mut`; FnOnce can only be called once
//   6. [x] footgun     — returning closures: impl Fn vs Box<dyn Fn>
//   7. [x] real-world  — fn pointers vs closures & coercion
//   8. [ ] real-world  — closures with the stdlib + a closure factory
//   9. [ ] capstone    — callback registry / event dispatcher

fn main() {
    check_1();
    check_2();
    check_3();
    check_4();
    check_5();
    check_6();
    check_7();
    check_8();
    // check_9();
    println!("all checks passed ✅");
}

// ---------------------------------------------------------------------------
// Problem 1 — foundations: define a closure that captures `factor` by reference
// and multiplies its argument by it. No type annotations on the closure.
//
// Implement `make_and_use`: given a `factor` and a slice of numbers, build a
// closure `times` that maps each number to number * factor, and return the
// resulting Vec. The point: the closure reads `factor` from its environment
// without you passing it as a parameter.
// ---------------------------------------------------------------------------
fn make_and_use(factor: i32, nums: &[i32]) -> Vec<i32> {
    nums.iter().map(|x| x * factor).collect()
}

fn check_1() {
    assert_eq!(make_and_use(3, &[1, 2, 3]), vec![3, 6, 9]);
    assert_eq!(make_and_use(0, &[5, 9]), vec![0, 0]);
    assert_eq!(make_and_use(-2, &[1, 2]), vec![-2, -4]);
    println!("check_1 ✅ closures capture their environment");
}

// Problem 2 — foundations: the three capture modes.
//
// The compiler picks the *least invasive* capture that makes the body work:
//   - read-only use      -> capture by &      (the closure is Fn)
//   - mutating use       -> capture by &mut   (the closure is FnMut)
//   - `move` / consuming -> capture by value  (often FnOnce)
//
// Implement the three functions below. Each one builds a closure with a
// different capture mode, then drives it so the assertions in check_2 hold.
//
// (a) `borrow_capture`: capture `data: &Vec<i32>` read-only and return the sum.
//     The original `data` must still be usable by the caller afterward.
//
// (b) `mut_capture`: capture a local `log: Vec<String>` by &mut. Build a closure
//     `record` that pushes a formatted string; call it a few times; return `log`.
//
// (c) `move_capture`: capture `owned: String` BY VALUE with `move`, so the
//     closure owns it. Return a closure (boxed) that, when called, yields the
//     owned string's length. `owned` is moved into the closure.
fn borrow_capture(data: &Vec<i32>) -> i32 {
    data.iter().sum()
}

fn mut_capture() -> Vec<String> {
    let mut log: Vec<String> = Vec::new();
    let mut record = |s: String| log.push(s);
    record("event 0".to_string());
    record("event 1".to_string());
    record("event 2".to_string());
    log
}

fn move_capture(owned: String) -> Box<dyn Fn() -> usize> {
    Box::new(move || owned.len())
}

fn check_2() {
    let v = vec![10, 20, 30];
    assert_eq!(borrow_capture(&v), 60);
    assert_eq!(v.len(), 3, "borrow_capture must not consume `data`");

    assert_eq!(
        mut_capture(),
        vec![
            "event 0".to_string(),
            "event 1".to_string(),
            "event 2".to_string()
        ]
    );

    let f = move_capture("hello".to_string());
    assert_eq!(f(), 5);
    assert_eq!(f(), 5, "an Fn closure can be called repeatedly");
    println!("check_2 ✅ borrow / mut-borrow / move capture modes");
}

// ---------------------------------------------------------------------------
// Problem 3 — mechanics: the Fn / FnMut / FnOnce trait hierarchy.
//
// The three traits nest:  Fn : FnMut : FnOnce  (each is a SUBtrait of the next).
//   - Fn      can be called via &self      -> also satisfies FnMut and FnOnce
//   - FnMut   can be called via &mut self  -> also satisfies FnOnce
//   - FnOnce  can be called via self       -> the loosest bound
// So `F: Fn` is the STRICTEST requirement (works in the fewest places as a value
// but is accepted by every helper), and `F: FnOnce` is the LOOSEST (accepts the
// most closures, but you can only call it once).
//
// Write three generic helpers. Each takes a closure with a different bound and
// invokes it. The point of the rung: see which bound accepts which closure, and
// that an Fn closure flows into ALL THREE helpers while an FnOnce closure only
// flows into `apply_once`.
//
//   apply_fn<F: Fn() -> i32>      -> call f twice, return the sum of both calls
//   apply_mut<F: FnMut() -> i32>  -> call f twice, return the sum (note: param must be `mut f`)
//   apply_once<F: FnOnce() -> i32> -> call f once, return its result
// ---------------------------------------------------------------------------
fn apply_fn<F: Fn() -> i32>(f: F) -> i32 {
    f() + f()
}

fn apply_mut<F: FnMut() -> i32>(mut f: F) -> i32 {
    f() + f()
}

fn apply_once<F: FnOnce() -> i32>(f: F) -> i32 {
    f()
}

fn check_3() {
    // A pure Fn closure: reads only. It satisfies ALL THREE bounds.
    let read = || 7;
    assert_eq!(apply_fn(read), 14);
    assert_eq!(apply_mut(read), 14);
    assert_eq!(apply_once(read), 7);

    // An FnMut closure: mutates a counter each call. Fits apply_mut & apply_once,
    // but NOT apply_fn (try it later: uncomment and watch the bound fail).
    let mut n = 0;
    let counter = move || {
        n += 1;
        n
    };
    assert_eq!(apply_mut(counter), 3); // returns 1 then 2 -> 1 + 2 = 3
    // assert_eq!(apply_fn(counter), ...); // <- would be E0525: expected Fn, found FnMut

    // An FnOnce closure: consumes a captured String. Only apply_once accepts it.
    let s = String::from("rust");
    let consume = move || s.len() as i32;
    assert_eq!(apply_once(consume), 4);

    println!("check_3 ✅ Fn ⊂ FnMut ⊂ FnOnce — strictest to loosest bound");
}

// ---------------------------------------------------------------------------
// Problem 4 — mechanics: desugar a closure by hand.
//
// "A closure is an anonymous struct + a call method." Prove it. The compiler
// can't let you `impl Fn for ...` on stable (those traits are unstable to
// implement directly), so we mirror them with inherent methods that have the
// SAME self-type the real traits use:
//   Fn::call(&self, args)            ~  fn call(&self, ...)
//   FnMut::call_mut(&mut self, args) ~  fn call_mut(&mut self, ...)
//   FnOnce::call_once(self, args)    ~  fn call_once(self, ...)
//
// (a) The closure  `move |x: i32| x + offset`  desugars to a struct holding the
//     captured `offset` by value, with a `&self` call method (it only READS the
//     capture -> Fn). Build `AddOffset` + `fn call(&self, x: i32) -> i32`.
//
// (b) The closure  `move || { count += step; count }`  desugars to a struct
//     holding BOTH captures by value, with a `&mut self` call method (it MUTATES
//     `count` -> FnMut). Build `Counter` + `fn call_mut(&mut self) -> i32`.
//
// check_4 runs each hand-built struct side-by-side with the equivalent real
// closure and asserts identical behavior.
// ---------------------------------------------------------------------------
struct AddOffset {
    offset: i32,
}

impl AddOffset {
    fn call(&self, x: i32) -> i32 {
        x + self.offset
    }
}

struct Counter {
    count: i32,
    step: i32,
}

impl Counter {
    fn call_mut(&mut self) -> i32 {
        self.count += self.step;
        self.count
    }
}

fn check_4() {
    let offset = 100;
    let hand = AddOffset { offset };
    let real = move |x: i32| x + offset;
    for x in [-5, 0, 7, 42] {
        assert_eq!(hand.call(x), real(x), "AddOffset must match the closure");
    }

    let step = 5;
    let mut hand = Counter { count: 0, step };
    let mut count = 0;
    let mut real = move || {
        count += step;
        count
    };
    for _ in 0..4 {
        assert_eq!(hand.call_mut(), real(), "Counter must match the closure");
    }
    assert_eq!(hand.call_mut(), 25);
    println!("check_4 ✅ a closure is just a struct + a call method");
}

// ---------------------------------------------------------------------------
// Problem 5 — footgun: "called once" and the `mut` binding.
//
// (a) A closure that MOVES a captured value out of itself can only be `FnOnce`:
//     running it consumes the capture, so a second call would use a moved value.
//     Build `unwrap_factory`: capture `s: String` by `move` and return a closure
//     that yields `s` itself (by value). Its only valid bound is `impl FnOnce`.
//
// (b) Calling a closure through a `FnMut` bound needs a MUTABLE binding, because
//     the call goes through `&mut self`. `run_n_times` takes a counter-style
//     closure and calls it `n` times, collecting each result. The parameter is
//     `mut f` on purpose — your experiment is to delete the `mut` and read E0596.
// ---------------------------------------------------------------------------
fn unwrap_factory(s: String) -> impl FnOnce() -> String {
    move || s
}

fn run_n_times<F: FnMut() -> i32>(mut f: F, n: usize) -> Vec<i32> {
    let mut results = Vec::new();
    for _ in 0..n {
        results.push(f());
    }
    results
}

fn check_5() {
    let f = unwrap_factory(String::from("payload"));
    assert_eq!(f(), "payload");

    let mut tick = 0;
    let counter = move || {
        tick += 1;
        tick
    };
    assert_eq!(run_n_times(counter, 4), vec![1, 2, 3, 4]);
    println!("check_5 ✅ FnOnce consumes its captures; FnMut needs a mut binding");
}

// ---------------------------------------------------------------------------
// Problem 6 — footgun: returning closures.
//
// Every closure has a unique, unnameable, compiler-generated type. So you can't
// write `-> Closure`. You have two ways to return one:
//   - `-> impl Fn(..)`  : ONE concrete hidden type. Static dispatch, no alloc.
//                          Fails if different code paths return different closures.
//   - `-> Box<dyn Fn(..)>` : a heap-allocated trait object. Dynamic dispatch.
//                          Required when branches return DIFFERENT closure types.
//
// (a) `make_adder`: return `impl Fn(i32) -> i32` that adds the captured `n`.
//     One closure type, so `impl Fn` is the right (zero-cost) choice.
//
// (b) `pick_op`: given a char ('+', '-', '*'), return a `Box<dyn Fn(i32,i32)->i32>`
//     for that operation. Each arm is a DIFFERENT closure type — that's exactly
//     why this must be boxed, not `impl Fn`.
// ---------------------------------------------------------------------------
fn make_adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| x + n
}

fn pick_op(op: char) -> Box<dyn Fn(i32, i32) -> i32> {
    match op {
        '+' => Box::new(|a, b| a + b),
        '-' => Box::new(|a, b| a - b),
        '*' => Box::new(|a, b| a * b),
        _ => Box::new(|_, _| 0),
    }
}

fn check_6() {
    let add10 = make_adder(10);
    assert_eq!(add10(5), 15);
    assert_eq!(add10(-3), 7);
    assert_eq!(add10(0), 10);

    let plus = pick_op('+');
    let minus = pick_op('-');
    let times = pick_op('*');
    let bad = pick_op('?');
    assert_eq!(plus(3, 4), 7);
    assert_eq!(minus(10, 4), 6);
    assert_eq!(times(6, 7), 42);
    assert_eq!(bad(1, 1), 0);

    println!("check_6 ✅ impl Fn (one type, static) vs Box<dyn Fn> (branchy, dynamic)");
}

// ---------------------------------------------------------------------------
// Problem 7 — real-world: fn pointers vs closures & coercion.
//
// `fn(i32) -> i32` is the FUNCTION POINTER type: a single pointer-sized value
// aiming at some code, with NO captured environment. Three things coerce to it:
//   - a function ITEM (a top-level `fn`, like `triple` below)
//   - a NON-capturing closure (it captures nothing, so it needs no environment)
// A CAPTURING closure does NOT coerce — it carries data, so it can only be an
// `Fn`/`FnMut`/`FnOnce` value, never a bare `fn`. (And every `fn` pointer DOES
// implement all three Fn traits, so it fits any `F: Fn` bound too.)
//
// Implement `transform_all`: map `xs` through the fn pointer `f` into a Vec.
// The interesting part is the check: function items AND non-capturing closures
// both flow in, and you can store fn pointers directly in a `Vec` (they're Copy
// and Sized — no Box, no vtable, unlike rung 6's branchy closures).
// ---------------------------------------------------------------------------
fn triple(x: i32) -> i32 {
    x * 3
}

fn transform_all(xs: &[i32], f: fn(i32) -> i32) -> Vec<i32> {
    xs.iter().map(|x| f(*x)).collect()
}

fn check_7() {
    assert_eq!(transform_all(&[1, 2, 3], triple), vec![3, 6, 9]);
    assert_eq!(transform_all(&[1, 2, 3], |x| x + 100), vec![101, 102, 103]);

    let ops: Vec<fn(i32) -> i32> = vec![triple, |x| x + 1, |x| x * x];
    let results: Vec<i32> = ops.iter().map(|f| f(5)).collect();
    assert_eq!(results, vec![15, 6, 25]);

    println!("check_7 ✅ fn items & non-capturing closures coerce to fn pointers");
}

// ---------------------------------------------------------------------------
// Problem 8 — real-world: closures with the stdlib + a closure factory.
//
// Almost every stdlib adapter takes a closure. Note which Fn trait each wants:
//   Iterator::map / filter / fold   -> FnMut  (called once per element)
//   <[T]>::sort_by_key              -> FnMut
//   Option::unwrap_or_else          -> FnOnce (called at most once)
// A "closure factory" is a function that BUILDS and returns a closure capturing
// its arguments — the workhorse pattern (think `make_validator(min, max)`).
//
// (a) `top_squares(nums, threshold)`: keep the elements strictly greater than
//     `threshold` (filter), square each (map), and return them sorted DESCENDING.
//     Build it as an iterator pipeline + a sort.
//
// (b) `make_validator(min, max)`: return `impl Fn(i32) -> bool` that reports
//     whether its argument is in `min..=max`. The check reuses ONE such closure
//     across a `filter`, proving an Fn closure is freely re-callable.
// ---------------------------------------------------------------------------
fn top_squares(nums: &[i32], threshold: i32) -> Vec<i32> {
    nums.iter()
        .filter(|&x| *x > threshold)
        .map(|x| x * x)
        .collect::<Vec<i32>>()
}

fn make_validator(min: i32, max: i32) -> impl Fn(i32) -> bool {
    move |x| x >= min && x <= max
}

fn check_8() {
    assert_eq!(top_squares(&[1, 5, 2, 8, 3], 2), vec![64, 25, 9]);
    assert_eq!(top_squares(&[10, 10], 10), Vec::<i32>::new());

    let valid = make_validator(1, 10);
    assert!(valid(5));
    assert!(!valid(0));
    assert!(!valid(11));
    let kept: Vec<i32> = (0..15).filter(|&n| valid(n)).collect();
    assert_eq!(kept, (1..=10).collect::<Vec<i32>>());

    let mut rows = vec![("a", 3), ("b", 1), ("c", 2)];
    rows.sort_by_key(|&(_, n)| n);
    assert_eq!(rows, vec![("b", 1), ("c", 2), ("a", 3)]);

    println!("check_8 ✅ stdlib adapters take closures; factories return them");
}
