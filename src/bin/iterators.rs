// Iterators end-to-end
// Run: cargo run --bin iterators
//
// Ladder (all DONE ✅):
//   1. Consume & transform           — next()/for, then map/filter/sum            [foundations]  DONE
//   2. iter vs iter_mut vs into_iter  — &T / &mut T / T                            [foundations]  DONE
//   3. Adapter zoo                    — enumerate/zip/flat_map/filter_map/fold/scan [mechanics]    DONE
//   4. Laziness, proven               — inspect + infinite iterator, pull one      [mechanics]    DONE
//   5. Ownership & collect traps      — move, turbofish, Result short-circuit       [footgun]      DONE
//   6. impl Iterator for MyType       — write next(), get adapters for free         [footgun]      DONE
//   7. IntoIterator + DoubleEnded     — for-loop desugar, rev(), size_hint          [real-world]   DONE
//   8. Custom lazy adapter + ext trait— .pairs() on every iterator (itertools)      [real-world]   DONE
//   9. Capstone: mini iterator engine — own trait + lazy map/filter/take + collect  [capstone]     DONE

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
    println!("all checks passed ✅");
}

// ── Rung 1: consume & transform ────────────────────────────────────────────
// Implement `sum_of_even_squares`: given a slice of i32, take only the EVEN
// numbers, square each, and return their sum.
//
// Do it with an iterator chain — .iter() then some combination of
// .filter(...), .map(...), and .sum(). No manual loop, no mutable accumulator.
fn sum_of_even_squares(nums: &[i32]) -> i32 {
    nums.iter().filter(|&x| x % 2 == 0).map(|x| x * x).sum()
}

fn check_1() {
    assert_eq!(sum_of_even_squares(&[1, 2, 3, 4, 5]), 2 * 2 + 4 * 4); // 20
    assert_eq!(sum_of_even_squares(&[]), 0);
    assert_eq!(sum_of_even_squares(&[1, 3, 5]), 0);
    assert_eq!(sum_of_even_squares(&[6]), 36);
    println!("rung 1 ✅  sum_of_even_squares works");
}

// ── Rung 2: iter vs iter_mut vs into_iter ──────────────────────────────────
// A Vec gives you THREE iterators, each yielding a different item type:
//   .iter()      -> &T       (borrow, read-only)
//   .iter_mut()  -> &mut T   (borrow, can mutate in place)
//   .into_iter() -> T        (consumes the Vec, hands you owned values)
//
// Implement all three:

// (a) count how many strings have length > 3, WITHOUT consuming `words`
//     (caller still uses `words` afterward, so you must borrow).
fn count_long(words: &[String]) -> usize {
    words.iter().filter(|w| w.len() > 3).count()
}

// (b) double every number IN PLACE. Takes &mut Vec so the caller sees changes.
//     Use .iter_mut() and mutate through the &mut i32 you get.
fn double_in_place(nums: &mut Vec<i32>) {
    nums.iter_mut().for_each(|n| *n *= 2);
}

// (c) consume the Vec and join the owned Strings with ", ".
//     Take `words` BY VALUE and .into_iter() it. After this returns the Vec
//     is gone — that's the point of consuming.
fn join_owned(words: Vec<String>) -> String {
    words.into_iter().collect::<Vec<_>>().join(", ")
}

fn check_2() {
    let words = vec!["a".to_string(), "bbbb".to_string(), "ccccc".to_string()];
    assert_eq!(count_long(&words), 2);
    // `words` is still usable here because count_long only borrowed it:
    assert_eq!(words.len(), 3);

    let mut nums = vec![1, 2, 3];
    double_in_place(&mut nums);
    assert_eq!(nums, vec![2, 4, 6]);

    let owned = vec!["x".to_string(), "y".to_string(), "z".to_string()];
    assert_eq!(join_owned(owned), "x, y, z");
    println!("rung 2 ✅  iter / iter_mut / into_iter");
}

// ── Rung 3: the adapter zoo ────────────────────────────────────────────────
// Five small tasks, each a different adapter you'll use constantly.
// Use only iterator chains (no manual loops).
use std::collections::HashMap;

// (a) enumerate + filter: return the INDICES of all even values.
//     [10, 7, 4, 3] -> [0, 2]  (10 at idx 0, 4 at idx 2 are even)
fn indices_of_evens(nums: &[i32]) -> Vec<usize> {
    nums.iter()
        .enumerate()
        .filter_map(|(i, &x)| if x % 2 == 0 { Some(i) } else { None })
        .collect()
}

// (b) zip: pair up names with scores into "name=score" strings.
//     names=[a,b], scores=[1,2] -> ["a=1", "b=2"]
//     If lengths differ, zip stops at the shorter one (that's the lesson).
fn label_scores(names: &[&str], scores: &[i32]) -> Vec<String> {
    names
        .iter()
        .zip(scores)
        .map(|(n, s)| format!("{}={}", n, s))
        .collect()
}

// (c) flat_map: explode each word into its chars, all in one flat Vec.
//     ["ab", "c"] -> ['a', 'b', 'c']
fn all_chars(words: &[&str]) -> Vec<char> {
    words.iter().flat_map(|w| w.chars()).collect()
}

// (d) filter_map: parse each string to i32, KEEPING only the ones that parse.
//     ["1", "x", "3"] -> [1, 3]   (filter + map in one pass; .ok() turns Result->Option)
fn parse_all(strs: &[&str]) -> Vec<i32> {
    strs.iter().filter_map(|s| s.parse().ok()).collect()
}

// (e) fold: build a frequency map of chars. (Then notice an Entry-API would do
//     it too — but here practice fold's accumulator threading.)
//     "aab" -> {a:2, b:1}
fn char_freq(s: &str) -> HashMap<char, usize> {
    s.chars().fold(HashMap::new(), |mut acc, c| {
        *acc.entry(c).or_insert(0) += 1;
        acc
    })
}

fn check_3() {
    assert_eq!(indices_of_evens(&[10, 7, 4, 3]), vec![0, 2]);
    assert_eq!(
        label_scores(&["a", "b"], &[1, 2]),
        vec!["a=1".to_string(), "b=2".to_string()]
    );
    // zip stops at the shorter input:
    assert_eq!(
        label_scores(&["a", "b", "c"], &[9]),
        vec!["a=9".to_string()]
    );
    assert_eq!(all_chars(&["ab", "c"]), vec!['a', 'b', 'c']);
    assert_eq!(parse_all(&["1", "x", "3"]), vec![1, 3]);

    let f = char_freq("aab");
    assert_eq!(f.get(&'a'), Some(&2));
    assert_eq!(f.get(&'b'), Some(&1));
    assert_eq!(f.get(&'z'), None);
    println!("rung 3 ✅  adapter zoo");
}

// ── Rung 4: laziness, proven ───────────────────────────────────────────────
// THE big idea: adapters (map/filter/take/…) are LAZY. They just build a
// struct that remembers what to do. NOTHING runs until a consumer (collect,
// sum, for, next, count…) starts pulling items through with .next().
//
// We'll prove it two ways.

// (a) Build a chain over 0..1_000_000 with a .map() that PUSHES onto `log`
//     every time it actually runs. DON'T consume it — just build it and drop it.
//     Then return how many times the closure ran. It must be 0: lazy means the
//     map body never executed because nobody pulled.
//
//     Hint: the closure captures `&mut log`. Build `(0..1_000_000).map(|x| {
//     log.push(x); x })` into a binding, then DON'T call any consumer on it.
fn lazy_never_runs(log: &mut Vec<i32>) {
    let _lazy = (0..1_000_000).map(|x| log.push(x));
}

// (b) Laziness is what makes INFINITE iterators usable. `0..` is endless.
//     Take an infinite count-up, keep multiples of 3, square them, and grab the
//     first 4. If any adapter were eager this would hang forever — but `take(4)`
//     stops the pulling after 4 items.
//     Expected: 3,6,9,12 -> squared -> [9, 36, 81, 144]
fn first_4_triple_squares() -> Vec<u64> {
    (0u64..)
        .filter(|n| n % 3 == 0 && *n != 0)
        .map(|n| n * n)
        .take(4)
        .collect()
}

fn check_4() {
    let mut log = Vec::new();
    lazy_never_runs(&mut log);
    assert_eq!(log.len(), 0, "lazy! the map closure must never have run");

    assert_eq!(first_4_triple_squares(), vec![9, 36, 81, 144]);
    println!("rung 4 ✅  laziness (closure ran 0 times; infinite iter tamed by take)");
}

// ── Rung 5: ownership & collect traps ──────────────────────────────────────
// Three places iterators bite real code.

// (a) THE MOVE TRAP. This function is written to NOT compile yet — that's the
//     lesson. `into_iter()` consumes `v`, so using `v` afterward is use-after-
//     move (E0382). Read the error, understand it, then FIX it so the function
//     both sums the values AND returns the original Vec's len — without cloning
//     the Vec. (Hint: do you actually need to *consume* v to sum it? What does
//     .iter() give you instead? Or: capture len BEFORE the consuming call.)
fn sum_then_len(v: Vec<i32>) -> (i32, usize) {
    let total: i32 = v.iter().sum();
    let n = v.len();
    (total, n)
}

// (b) TURBOFISH / type annotation. `collect()` is generic over its return type
//     (FromIterator). With nothing telling it WHAT to build, it can't infer.
//     Return a Vec<i32> of 0..5 doubled. Pick ONE: annotate the binding
//     `let v: Vec<i32> = ...collect();` OR turbofish `.collect::<Vec<i32>>()`.
fn doubled_0_to_5() -> Vec<i32> {
    (0..5).map(|x| x * 2).collect::<Vec<i32>>()
}

// (c) collect into Result — the SHORT-CIRCUIT superpower.
//     Parsing a list where ALL must succeed. collect::<Result<Vec<i32>,_>>()
//     yields Ok(vec) if every parse worked, or the FIRST Err the moment one
//     fails (and stops early). Implement both cases via the same collect.
fn parse_all_or_fail(strs: &[&str]) -> Result<Vec<i32>, std::num::ParseIntError> {
    strs.iter().map(|s| s.parse::<i32>()).collect()
}

fn check_5() {
    assert_eq!(sum_then_len(vec![1, 2, 3]), (6, 3));

    assert_eq!(doubled_0_to_5(), vec![0, 2, 4, 6, 8]);

    assert_eq!(parse_all_or_fail(&["1", "2", "3"]), Ok(vec![1, 2, 3]));
    assert!(parse_all_or_fail(&["1", "nope", "3"]).is_err());
    println!("rung 5 ✅  move trap / turbofish / Result short-circuit");
}

// ── Rung 6: impl Iterator for your own type ────────────────────────────────
// The whole Iterator trait has ONE required method:
//     fn next(&mut self) -> Option<Self::Item>;
// Implement that, and you inherit ALL the adapters (map/filter/take/sum/…) for
// free, because they're default methods built on next().
//
// Build a Fibonacci generator. Each .next() returns the current value and
// advances the internal state. It's INFINITE — next() never returns None — so
// callers must bound it with take()/zip()/etc. (rung 4's lesson, now from the
// producer side).
struct Fib {
    curr: u64,
    next: u64,
}

impl Fib {
    fn new() -> Self {
        Fib { curr: 0, next: 1 }
    }
}

impl Iterator for Fib {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        let curr = std::mem::replace(&mut self.curr, self.next);
        self.next = curr + self.next;
        Some(curr)
    }
}

fn check_6() {
    // Pull the first 10 by hand-ish (via take) — note the adapters just WORK:
    let first10: Vec<u64> = Fib::new().take(10).collect();
    assert_eq!(first10, vec![0, 1, 1, 2, 3, 5, 8, 13, 21, 34]);

    // Adapters you never wrote, now free on your type:
    let sum_even_fibs: u64 = Fib::new().take(10).filter(|n| n % 2 == 0).sum();
    assert_eq!(sum_even_fibs, 0 + 2 + 8 + 34); // 44

    // nth / find / position all come for free too:
    assert_eq!(Fib::new().nth(7), Some(13));
    println!("rung 6 ✅  impl Iterator — one next(), all adapters free");
}

// ── Rung 7: IntoIterator (x how `for` works) + DoubleEndedIterator ──────────
// `for x in thing { … }` is SUGAR for:
//     let mut it = IntoIterator::into_iter(thing);
//     while let Some(x) = it.next() { … }
// So to make `for x in my_collection` work, you implement IntoIterator.
// Real collections impl it THREE times so all three of these work:
//     for x in coll      -> yields T      (consumes; impl on `MyVec<T>`)
//     for x in &coll     -> yields &T     (impl on `&MyVec<T>`)
//     for x in &mut coll -> yields &mut T (impl on `&mut MyVec<T>`)
//
// We'll do the BY-VALUE one (consuming) plus DoubleEndedIterator so rev() works.

struct MyVec<T> {
    items: Vec<T>,
}

// The consuming iterator we hand back. It owns the data and pops from a cursor.
struct MyVecIntoIter<T> {
    inner: std::vec::IntoIter<T>,
}

impl<T> IntoIterator for MyVec<T> {
    type Item = T;
    type IntoIter = MyVecIntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        // your turn: wrap self.items.into_iter() into a MyVecIntoIter
        MyVecIntoIter {
            inner: self.items.into_iter(),
        }
    }
}

impl<T> Iterator for MyVecIntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        // your turn: delegate to the inner std IntoIter
        self.inner.next()
    }

    // size_hint lets consumers pre-allocate. Forward the inner one.
    fn size_hint(&self) -> (usize, Option<usize>) {
        // your turn: delegate to self.inner.size_hint()
        self.inner.size_hint()
    }
}

// DoubleEndedIterator: add next_back() and you get .rev() for free.
impl<T> DoubleEndedIterator for MyVecIntoIter<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        // your turn: delegate to self.inner.next_back()
        self.inner.next_back()
    }
}

fn check_7() {
    // `for x in coll` now works because of IntoIterator:
    let coll = MyVec {
        items: vec![1, 2, 3],
    };
    let mut seen = Vec::new();
    for x in coll {
        seen.push(x);
    }
    assert_eq!(seen, vec![1, 2, 3]);

    // size_hint forwarded -> collect can pre-allocate exactly:
    let coll2 = MyVec {
        items: vec![10, 20, 30, 40],
    };
    let it = coll2.into_iter();
    assert_eq!(it.size_hint(), (4, Some(4)));

    // rev() works because we implemented DoubleEndedIterator:
    let coll3 = MyVec {
        items: vec!['a', 'b', 'c'],
    };
    let reversed: Vec<char> = coll3.into_iter().rev().collect();
    assert_eq!(reversed, vec!['c', 'b', 'a']);

    println!("rung 7 ✅  IntoIterator (for-loop sugar) + DoubleEnded (rev) + size_hint");
}

// ── Rung 8: a custom lazy adapter + an extension trait ─────────────────────
// This is how itertools works. You'll build a `.pairs()` adapter that turns an
// iterator of items into an iterator of (prev, curr) overlapping windows:
//     [1, 2, 3, 4].pairs()  ->  (1,2), (2,3), (3,4)
//
// It must be LAZY (like rung 4): pulling one pair pulls at most one new item.
//
// PART A — the adapter struct. It wraps an inner iterator and remembers the
// previous item. Implement Iterator for it.
struct Pairs<I: Iterator> {
    inner: I,
    prev: Option<I::Item>,
}

impl<I> Iterator for Pairs<I>
where
    I: Iterator,
    I::Item: Clone, // we need to keep a copy of prev AND emit it
{
    type Item = (I::Item, I::Item);

    fn next(&mut self) -> Option<Self::Item> {
        if self.prev.is_none() {
            self.prev = self.inner.next();
        }
        let curr = self.inner.next()?;
        let prev = self.prev.replace(curr.clone())?;
        Some((prev, curr))
    }
}

// PART B — the extension trait. A blanket impl over ALL iterators gives every
// iterator a `.pairs()` method (this is the itertools / Itertools pattern).
trait IterPairsExt: Iterator + Sized {
    fn pairs(self) -> Pairs<Self> {
        // your turn: construct a Pairs wrapping `self`, prev starts as None
        Pairs {
            inner: self,
            prev: None,
        }
    }
}

// blanket impl: every Iterator now has .pairs()
impl<I: Iterator> IterPairsExt for I {}

fn check_8() {
    let p: Vec<(i32, i32)> = vec![1, 2, 3, 4].into_iter().pairs().collect();
    assert_eq!(p, vec![(1, 2), (2, 3), (3, 4)]);

    // empty / single -> no pairs:
    assert_eq!(Vec::<i32>::new().into_iter().pairs().count(), 0);
    assert_eq!(vec![42].into_iter().pairs().count(), 0);

    // laziness + composability: works on an INFINITE source bounded by take():
    let diffs: Vec<u64> = (0u64..)
        .map(|x| x * x)
        .pairs()
        .map(|(a, b)| b - a)
        .take(4)
        .collect();
    // squares 0,1,4,9,16 -> consecutive diffs 1,3,5,7
    assert_eq!(diffs, vec![1, 3, 5, 7]);

    println!("rung 8 ✅  custom lazy adapter (.pairs()) via blanket extension trait");
}

// ── Rung 9 (CAPSTONE): a mini iterator engine, from scratch ─────────────────
// Re-implement the core of std::iter WITHOUT using it. You'll build:
//   - your own `MyIterator` trait with the ONE required method `next()`,
//   - default methods `map`, `filter`, `take` that return LAZY adapter structs,
//   - a default consumer `collect_vec` that drains into a Vec,
//   - a concrete source `Counter` so there's something to iterate.
// Then a pipeline proves laziness end-to-end: counter -> map -> filter -> take.
//
// This is the whole architecture in miniature. Take your time.

// (1) THE TRAIT. One required method; everything else has a default body that
//     builds on next(). `Sized` is needed so the adapters can take `self` by value.
trait MyIterator: Sized {
    type Item;

    fn next(&mut self) -> Option<Self::Item>;

    // map: lazily transform each item with F. Returns a MyMap adapter — does NOT
    // run F here. (Default method: provide the body.)
    fn map<B, F: FnMut(Self::Item) -> B>(self, f: F) -> MyMap<Self, F> {
        MyMap { iter: self, f }
    }

    // filter: lazily keep items where P returns true. Returns a MyFilter adapter.
    fn filter<P: FnMut(&Self::Item) -> bool>(self, pred: P) -> MyFilter<Self, P> {
        MyFilter { iter: self, pred }
    }

    // take: lazily yield at most `n` items, then stop. Returns a MyTake adapter.
    fn take(self, n: usize) -> MyTake<Self> {
        MyTake {
            iter: self,
            remaining: n,
        }
    }

    // collect_vec: THE CONSUMER. This is where pulling actually happens — loop
    // calling self.next() until None, pushing into a Vec.
    fn collect_vec(mut self) -> Vec<Self::Item> {
        let mut out = Vec::new();
        while let Some(x) = self.next() {
            out.push(x);
        }
        out
    }
}

// (2) THE SOURCE. Counts up from `curr`, forever. (Infinite — bound it with take.)
struct Counter {
    curr: u64,
}

impl MyIterator for Counter {
    type Item = u64;
    fn next(&mut self) -> Option<Self::Item> {
        let curr = self.curr;
        self.curr += 1;
        Some(curr)
    }
}

// (3) THE ADAPTERS. Each wraps an inner MyIterator and implements MyIterator by
//     pulling from inner in its own next(). This is where laziness lives: each
//     next() pulls only as much as it needs.

struct MyMap<I, F> {
    iter: I,
    f: F,
}
impl<I: MyIterator, B, F: FnMut(I::Item) -> B> MyIterator for MyMap<I, F> {
    type Item = B;
    fn next(&mut self) -> Option<B> {
        self.iter.next().map(|x| (self.f)(x))
    }
}

struct MyFilter<I, P> {
    iter: I,
    pred: P,
}
impl<I: MyIterator, P: FnMut(&I::Item) -> bool> MyIterator for MyFilter<I, P> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        while let Some(x) = self.iter.next() {
            if (self.pred)(&x) {
                return Some(x);
            }
        }
        None
    }
}

struct MyTake<I> {
    iter: I,
    remaining: usize,
}
impl<I: MyIterator> MyIterator for MyTake<I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        if self.remaining == 0 {
            None
        } else {
            self.remaining -= 1;
            self.iter.next()
        }
    }
}

fn check_9() {
    let out: Vec<u64> = Counter { curr: 0 }
        .map(|x| x * x) // 0,1,4,9,16,25,36,49,64,81,100,...
        .filter(|x| x % 2 == 0) // 0,4,16,36,64,100,...
        .take(5) // 0,4,16,36,64
        .collect_vec();
    assert_eq!(out, vec![0, 4, 16, 36, 64]);

    // Prove the source is genuinely infinite yet tamed by take (would hang if
    // any adapter were eager): take just 3 raw counts.
    let three: Vec<u64> = Counter { curr: 100 }.take(3).collect_vec();
    assert_eq!(three, vec![100, 101, 102]);

    // Prove filter can skip arbitrarily far without take seeing the skips:
    let first_two_mult_of_7: Vec<u64> = Counter { curr: 1 }
        .filter(|x| x % 7 == 0)
        .take(2)
        .collect_vec();
    assert_eq!(first_two_mult_of_7, vec![7, 14]);

    println!("rung 9 ✅  CAPSTONE — hand-built lazy iterator engine (trait+adapters+consumer)");
}
