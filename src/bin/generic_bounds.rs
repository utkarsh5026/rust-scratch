//! Generic bounds & `where` clauses
//! Run: cargo run --bin generic_bounds
//!
//! A generic `T` is a black box — you can't call anything on it until a *bound*
//! grants a capability. Bounds restrict the caller and empower you at once.
//! `where` clauses move bounds below the signature and can express things the
//! inline `T: Bound` form cannot (assoc-type bounds, bounds on `&T` / `Vec<T>`).
//!
//! Ladder:
//!   1. [x] foundations  — single bound: min_item<T: PartialOrd + Copy>
//!   2. [x] foundations  — multiple bounds + rewrite as `where`
//!   3. [x] mechanics    — bounds on struct vs method (don't over-constrain)
//!   4. [x] mechanics    — conditional method (cmp_display, only some T)
//!   5. [x] footgun      — T: ?Sized, accept str / [T] behind a reference
//!   6. [x] footgun      — where-only bounds: I::Item: Display, &'a C HRTB
//!   7. [x] real-world   — blanket impl + coherence/overlap error
//!   8. [x] real-world   — conditional trait impl: PartialEq for MyBox<T>
//!   9. [x] capstone     — IterExt extension trait (blanket + per-method where)

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

// ---------------------------------------------------------------------------
// Rung 1 — Single bound.
// Return the smallest element of a slice. `T` needs to be comparable
// (PartialOrd) and small/copyable so you can return it by value (Copy).
// ---------------------------------------------------------------------------
fn min_item<T>(items: &[T]) -> T
where
    T: PartialOrd + Copy,
{
    *items
        .iter()
        .min_by(|a, b| a.partial_cmp(b).expect("items are comparable"))
        .expect("items is non-empty")
}

fn check_1() {
    assert_eq!(min_item(&[3, 1, 4, 1, 5, 9, 2, 6]), 1);
    assert_eq!(min_item(&[2.5, 0.5, 7.0]), 0.5);
    assert_eq!(min_item(&['c', 'a', 'b']), 'a');
    println!("rung 1 ok");
}

// ---------------------------------------------------------------------------
// Rung 2 — Multiple bounds, and why `where` earns its keep.
// `dedup_describe` takes a slice, removes *consecutive* duplicates, and returns
// a Debug rendering of the result, e.g. [1, 1, 2, 3, 3, 3] -> "[1, 2, 3]".
//
// Figure out which three capabilities you need and bound them in a `where`
// clause (NOT inline in the `<...>`):
//   - you must read elements without consuming the borrowed slice  -> ?
//   - you must compare neighbours for equality                     -> ?
//   - you must format the result with {:?}                          -> ?
// ---------------------------------------------------------------------------
fn dedup_describe<T>(items: &[T]) -> String
where
    T: PartialEq + Copy + Debug, // TODO: your turn — add the three bounds T needs
{
    let mut result = Vec::new();
    for item in items {
        if result.last() != Some(item) {
            result.push(*item);
        }
    }
    format!("{:?}", result)
}

fn check_2() {
    assert_eq!(dedup_describe(&[1, 1, 2, 3, 3, 3]), "[1, 2, 3]");
    assert_eq!(dedup_describe(&['a', 'a', 'b', 'a']), "['a', 'b', 'a']");
    assert_eq!(dedup_describe::<i32>(&[]), "[]");
    println!("rung 2 ok");
}

// ---------------------------------------------------------------------------
// Rung 3 — Bound the *method*, not the struct.
// A `Stack<T>` should hold ANY T — even types that aren't Debug/Clone/etc.
// The classic beginner mistake is `struct Stack<T: Debug>`, which infects every
// use site: now you can't even build a Stack of a non-Debug type.
//
// Rule of thumb: leave the struct unbounded; attach a bound only to the impl
// block / method that actually needs the capability.
//
// Your tasks:
//   a) Keep `struct Stack<T>` with NO bounds.
//   b) In an UNBOUNDED `impl<T> Stack<T>`: implement new(), push(), len().
//   c) In a SEPARATE `impl<T: Debug> Stack<T>`: implement dump() -> String,
//      returning format!("{:?}", &self.items).
// NotDebug below has no Debug impl on purpose — a Stack<NotDebug> must still
// compile and support push/len. Only dump() should require Debug.
// ---------------------------------------------------------------------------
struct Stack<T> {
    items: Vec<T>,
}

impl<T> Stack<T> {
    fn new() -> Self {
        Self { items: Vec::new() }
    }
    fn push(&mut self, value: T) {
        self.items.push(value);
    }
    fn len(&self) -> usize {
        self.items.len()
    }
}

impl<T: Debug> Stack<T> {
    fn dump(&self) -> String {
        format!("{:?}", self.items)
    }
}

struct NotDebug; // deliberately NOT Debug

fn check_3() {
    // Works for a non-Debug T: construction, push, len all bound-free.
    let mut s: Stack<NotDebug> = Stack::new();
    s.push(NotDebug);
    s.push(NotDebug);
    assert_eq!(s.len(), 2);

    // dump() only available because i32: Debug.
    let mut n = Stack::new();
    n.push(1);
    n.push(2);
    n.push(3);
    assert_eq!(n.dump(), "[1, 2, 3]");
    println!("rung 3 ok");
}

// ---------------------------------------------------------------------------
// Rung 4 — A method that EXISTS only for some T (conditional method).
// `Pair<T>` always has new(). But cmp_display() — which prints the larger of
// the two — only makes sense when T can be ordered AND printed. So it lives in
// `impl<T: PartialOrd + Display> Pair<T>`, and a Pair<T> whose T lacks those
// bounds simply doesn't have that method (the call won't compile).
//
// Your tasks:
//   a) `impl<T> Pair<T>`: new(first, second).
//   b) `impl<T: PartialOrd + Display> Pair<T>`: cmp_display(&self) -> String
//      returning "the largest is X" using the bigger of first/second.
// The commented-out block at the bottom of check_4 is the lesson: uncomment it
// briefly and read the E0599 error explaining why a non-ordered T can't call it.
// ---------------------------------------------------------------------------
struct Pair<T> {
    first: T,
    second: T,
}

impl<T> Pair<T> {
    fn new(first: T, second: T) -> Self {
        Self { first, second }
    }
}

impl<T: PartialOrd + std::fmt::Display> Pair<T> {
    fn cmp_display(&self) -> String {
        let largest = if self.first > self.second {
            &self.first
        } else {
            &self.second
        };
        format!("the largest is {}", largest)
    }
}

fn check_4() {
    let p = Pair::new(7, 3);
    assert_eq!(p.cmp_display(), "the largest is 7");
    let q = Pair::new("apple", "banana");
    assert_eq!(q.cmp_display(), "the largest is banana");

    // LESSON (uncomment to see E0599): NotDebug is neither PartialOrd nor Display,
    // so Pair<NotDebug> has new() but NOT cmp_display().
    // let r = Pair::new(NotDebug, NotDebug);
    // r.cmp_display();

    println!("rung 4 ok");
}

// ---------------------------------------------------------------------------
// Rung 5 — The implicit `Sized` bound, and how to relax it with `?Sized`.
// Every generic param has a SECRET default bound: `<T>` really means
// `<T: Sized>`. That's why `str` and `[u8]` (DSTs — dynamically sized types)
// can't normally be a `T`. `?Sized` opts OUT of that default, but then you may
// only touch the value behind a pointer (&T, Box<T>, ...), never by value.
//
// `show` formats anything Displayable. As written it has the hidden Sized bound,
// so calling it on a `str` fails. Your tasks:
//   a) Implement the body: format!("{}", x).
//   b) Then uncomment the `show("hello str")` line in check_5 and run. Read the
//      E0277 ("doesn't have a known size at compile time"). THAT is the Sized
//      default biting: arg "hello" is &str, so T = str, which is unsized.
//   c) Fix it by relaxing the bound to `T: Display + ?Sized`. Re-run: green.
// ---------------------------------------------------------------------------
fn show<T: std::fmt::Display + ?Sized>(x: &T) -> String {
    format!("{}", x)
}

fn check_5() {
    // These T's are all Sized, so they work even with the default bound:
    assert_eq!(show(&42), "42");
    assert_eq!(show(&String::from("owned")), "owned");

    // This one needs ?Sized (T = str, a DST). Uncomment after step (b):
    assert_eq!(show("hello str"), "hello str");

    println!("rung 5 ok");
}

// ---------------------------------------------------------------------------
// Rung 6 — Bounds you can ONLY write in a `where` clause.
// The inline `<T: Bound>` form can only bound a bare type parameter. Anything
// more structured — an associated-type projection (`I::Item`) or a bound on a
// *derived* type like `&'a C` — has no place to go except a `where` clause.
//
// 6a) `join_display<I>`: take any iterable whose items are Display, return the
//     items joined with ", ". You can write `<I: IntoIterator>` inline, but the
//     bound that I::Item is Display has nowhere to live but the where clause.
//
// 6b) `sum_borrowed<C>`: sum a collection of i32 BY REFERENCE (without consuming
//     it), generic over the collection type. The capability you need —
//     "I can `for x in &c`" — is a bound on `&C`, not on `C`, so it requires a
//     higher-ranked `where for<'a> &'a C: IntoIterator<Item = &'a i32>`. There
//     is no way to state this inline. (This reuses your HRTB muscle.)
// ---------------------------------------------------------------------------
fn join_display<I>(iter: I) -> String
where
    I: IntoIterator,
    I::Item: std::fmt::Display,
{
    iter.into_iter()
        .map(|item| item.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn sum_borrowed<'a, C>(collection: &'a C) -> i32
where
    &'a C: IntoIterator<Item = &'a i32>,
{
    let mut sum = 0;
    for item in collection {
        sum += item;
    }
    sum
}

fn check_6() {
    assert_eq!(join_display(vec![1, 2, 3]), "1, 2, 3");
    assert_eq!(join_display(vec!["a", "b"]), "a, b");

    let v = vec![10, 20, 30];
    assert_eq!(sum_borrowed(&v), 60);
    assert_eq!(v.len(), 3);
    let arr = [1, 2, 3, 4];
    assert_eq!(sum_borrowed(&arr), 10);

    println!("rung 6 ok");
}

// ---------------------------------------------------------------------------
// Rung 7 — Blanket impls: implement a trait for EVERY type that meets a bound.
// `impl<T: Bound> MyTrait for T { ... }` is how std gives you `.to_string()` on
// everything Display (`impl<T: Display + ?Sized> ToString for T`) and `Into`
// for free from `From` (`impl<T, U: From<T>> Into<U> for T`).
//
// Define `trait Summary { fn summary(&self) -> String; }` and give it ONE
// blanket impl: every `T: Debug` gets `summary()` = its `{:?}` rendering.
// Then everything Debug — ints, Vecs, your own derive(Debug) structs — has
// `.summary()` with zero per-type work.
//
// Tasks:
//   a) Declare `trait Summary` with `fn summary(&self) -> String;`.
//   b) `impl<T: Debug> Summary for T { ... }` returning format!("{:?}", self).
//   c) LESSON (coherence): below check_7 there's a commented-out SECOND impl
//      `impl Summary for i32`. Uncomment it and read the E0119 "conflicting
//      implementations" error — i32 is already covered by the blanket impl, and
//      Rust forbids overlap. Re-comment it to go green.
// ---------------------------------------------------------------------------
// TODO: your turn — declare `trait Summary` and the blanket `impl<T: Debug>`.

#[derive(Debug)]
struct Point {
    #[allow(dead_code)]
    x: i32,
    #[allow(dead_code)]
    y: i32,
}

trait Summary {
    fn summary(&self) -> String;
}
impl<T: Debug> Summary for T {
    fn summary(&self) -> String {
        format!("{:?}", self)
    }
}

fn check_7() {
    // Once `trait Summary` + the blanket impl exist, delete this todo! and
    // uncomment the assertions below.
    assert_eq!(42.summary(), "42");
    assert_eq!(vec![1, 2].summary(), "[1, 2]");
    assert_eq!(Point { x: 1, y: 2 }.summary(), "Point { x: 1, y: 2 }");
    println!("rung 7 ok");
}

// LESSON — uncomment to trigger E0119 (overlaps the blanket impl):
// impl Summary for i32 {
//     fn summary(&self) -> String {
//         format!("the int {}", self)
//     }
// }

// ---------------------------------------------------------------------------
// Rung 8 — Conditional TRAIT impl: the wrapper has a capability only when its
// contents do. This is exactly what `#[derive(PartialEq)]`/`#[derive(Clone)]`
// generate: `impl<T: PartialEq> PartialEq for MyBox<T>`, NOT a blanket `impl`.
// The bound on the impl block is what propagates the requirement to T.
//
// `MyBox<T>` has NO derives on purpose. Your tasks (hand-write what derive does):
//   a) `impl<T: PartialEq> PartialEq for MyBox<T>` — eq compares the inner values.
//   b) `impl<T: Clone> Clone for MyBox<T>` — clone clones the inner value.
// Result: MyBox<i32> is comparable & cloneable (i32 is both), but a
// MyBox<NotEq> would be NEITHER — the bound gates the impl per type.
//
// Then delete the todo! in check_8 and uncomment the assertions.
// ---------------------------------------------------------------------------
struct MyBox<T>(T);

impl<T: PartialEq> PartialEq for MyBox<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: Clone> Clone for MyBox<T> {
    fn clone(&self) -> Self {
        MyBox(self.0.clone())
    }
}

// TODO: your turn — the two conditional impls (PartialEq, then Clone).

fn check_8() {
    assert!(MyBox(5) == MyBox(5));
    assert!(MyBox(5) != MyBox(6));
    let a = MyBox(vec![1, 2, 3]);
    let b = a.clone();
    assert!(a == b);

    // Wrappers of wrappers compose: MyBox<MyBox<i32>> is eq because MyBox<i32> is.
    assert!(MyBox(MyBox(1)) == MyBox(MyBox(1)));
    println!("rung 8 ok");
}

// ---------------------------------------------------------------------------
// Rung 9 — CAPSTONE: an `IterExt` extension trait (the Itertools pattern).
// This synthesizes the whole ladder:
//   - a SUPERTRAIT bound: `trait IterExt: Iterator` (every method can use
//     Self::Item and call .next()/iterate via self),
//   - a BLANKET IMPL `impl<I: Iterator> IterExt for I {}` so EVERY iterator
//     gains these methods for free (like rung 7),
//   - PER-METHOD `where Self::Item: ...` bounds (like rung 6): each adapter is
//     only callable when the element type supports what it needs. Itertools and
//     std's Iterator do exactly this (e.g. `.sum()` needs `Sum`, `.max()` needs
//     `Ord`).
//
// The trait + empty blanket impl are scaffolded. Your tasks:
//   For EACH method, (1) add the right `where` bound and (2) write the body.
//     • min_max  -> Option<(Item, Item)>: (min, max), None if empty.
//                   needs Self::Item: Ord + Copy  (compare, and duplicate into a pair)
//     • counts   -> HashMap<Item, usize>: frequency of each distinct item.
//                   needs Self::Item: Eq + Hash   (HashMap key requirements)
//     • join_with(sep) -> String: each item rendered, joined by `sep`.
//                   needs Self::Item: Display
//   (Each method takes `self` by value; `where Self: Sized` is implied for the
//    blanket impl over a concrete iterator, but you may state it explicitly.)
// ---------------------------------------------------------------------------
trait IterExt: Iterator {
    fn min_max(self) -> Option<(Self::Item, Self::Item)>
    where
        Self: Sized,
        Self::Item: Ord + Copy,
    {
        let mut min: Option<Self::Item> = None;
        let mut max: Option<Self::Item> = None;
        for item in self {
            min = Some(match min {
                Some(current) if current < item => current,
                _ => item,
            });
            max = Some(match max {
                Some(current) if current > item => current,
                _ => item,
            });
        }
        min.zip(max)
    }

    fn counts(self) -> HashMap<Self::Item, usize>
    where
        Self: Sized,
        Self::Item: Eq + Hash, // TODO: extend with the Self::Item bound HashMap keys need
    {
        let mut counts = HashMap::new();
        for item in self {
            *counts.entry(item).or_insert(0) += 1;
        }
        counts
    }

    fn join_with(self, sep: &str) -> String
    where
        Self: Sized, // TODO: extend with the Self::Item bound for rendering
        Self::Item: std::fmt::Display,
    {
        self.map(|item| item.to_string())
            .collect::<Vec<_>>()
            .join(sep)
    }
}

impl<I: Iterator> IterExt for I {}

fn check_9() {
    assert_eq!(vec![3, 1, 4, 1, 5].into_iter().min_max(), Some((1, 5)));
    assert_eq!(Vec::<i32>::new().into_iter().min_max(), None);

    let counts = "aababc".chars().counts();
    assert_eq!(counts[&'a'], 3);
    assert_eq!(counts[&'b'], 2);
    assert_eq!(counts[&'c'], 1);

    assert_eq!(vec![1, 2, 3].into_iter().join_with(" - "), "1 - 2 - 3");
    assert_eq!(["x", "y"].iter().join_with(""), "xy");
    println!("rung 9 ok — capstone complete!");
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
    // check_9();
}
