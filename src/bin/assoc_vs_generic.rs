// Associated types vs generic params
// Run: cargo run --bin assoc_vs_generic
//
// Mental model: an ASSOCIATED TYPE is an *output* the implementor chooses once
// (one `Item` per impl — a functional dependency Self -> Item). A GENERIC PARAM
// is an *input* the caller/impl picks, so one type can implement the trait many
// times. Rule of thumb: input -> generic param, output -> associated type.
//
// Ladder (DONE marks finished rungs):
//   1. Two shapes              - same trait with `type Item` vs `<T>`        [x]
//   2. The defining rule       - one impl per type vs many impls per type     [x]
//   3. Equality bounds         - `where I: Iterator<Item = u64>`             [x]
//   4. Your own iterator       - `impl Iterator` with `type Item`            [x]
//   5. Inference & turbofish   - generic .into() ambiguity vs assoc output   [x]
//   6. Trait objects           - dyn Iterator<Item=..> vs dyn Trait<T>       [x]
//   7. Add uses both           - Rhs generic param + Output associated       [x]
//   8. Design the split        - Graph trait: what's assoc vs generic        [x]
//   9. Capstone                - mini Iterator + Map adapter from scratch    [ ]

// ---------------------------------------------------------------------------
// Rung 1 — Two shapes
//
// Below are TWO traits expressing the same idea ("a thing you can pop an item
// out of"), one using an associated type, one using a generic parameter.
// Your job: implement BOTH for the same `Stack` so you feel the syntax.

struct Stack {
    items: Vec<i32>,
}

// Shape A: associated type. The implementor names the output type once.
trait PopAssoc {
    type Item;
    fn pop_it(&mut self) -> Option<Self::Item>;
}

// Shape B: generic parameter. The output type is a parameter on the trait.
trait PopGeneric<T> {
    fn pop_it(&mut self) -> Option<T>;
}

impl PopAssoc for Stack {
    type Item = i32;
    fn pop_it(&mut self) -> Option<Self::Item> {
        self.items.pop()
    }
}

impl PopGeneric<i32> for Stack {
    fn pop_it(&mut self) -> Option<i32> {
        self.items.pop()
    }
}

fn check_1() {
    let mut s = Stack {
        items: vec![1, 2, 3],
    };
    // Disambiguate which trait's method we mean with fully-qualified syntax.
    let a: Option<i32> = PopAssoc::pop_it(&mut s);
    assert_eq!(a, Some(3));
    let b: Option<i32> = PopGeneric::<i32>::pop_it(&mut s);
    assert_eq!(b, Some(2));
    println!("check_1 ok: implemented the same trait both ways");
}

// ---------------------------------------------------------------------------
// Rung 2 — The defining rule: one impl per type vs many impls per type
//
// This is THE difference. An associated type makes the trait a *function* of
// Self: one impl, one Output. A generic param makes the trait a *relation*: a
// type can implement it once per parameter value.
//
// `Counter` will implement BOTH a single-output trait and a multi-output trait.

struct Counter {
    n: i32,
}

// Associated-type version: Counter can implement this exactly ONCE.
trait Producer {
    type Output;
    fn produce(&self) -> Self::Output;
}

impl Producer for Counter {
    type Output = i32;
    fn produce(&self) -> Self::Output {
        self.n
    }
}

// EXPERIMENT (do this, then re-comment it):
// Uncomment the block below. You'll get error[E0119]: "conflicting
// implementations of trait `Producer` for type `Counter`". Even though the
// Output differs, the compiler refuses — because `Self -> Output` must be a
// function with a single answer. THIS is what "one impl per type" means.
//
// impl Producer for Counter {
//     type Output = String;
//     fn produce(&self) -> Self::Output {
//         self.n.to_string()
//     }
// }

// Generic-param version: Counter can implement this once PER type argument.
trait Convert<T> {
    fn convert(&self) -> T;
}

impl Convert<i32> for Counter {
    fn convert(&self) -> i32 {
        self.n
    }
}

impl Convert<String> for Counter {
    fn convert(&self) -> String {
        self.n.to_string()
    }
}

fn check_2() {
    let c = Counter { n: 42 };
    // Associated type: produce() has exactly one answer, no annotation needed.
    let p = c.produce();
    assert_eq!(p, 42);
    // Generic param: there are TWO convert()s; the caller must pick which.
    let as_int: i32 = c.convert();
    let as_str: String = c.convert();
    assert_eq!(as_int, 42);
    assert_eq!(as_str, "42");
    println!("check_2 ok: one assoc impl, two generic impls — and you saw E0119");
}

// ---------------------------------------------------------------------------
// Rung 3 — Equality bounds & projection
//
// Associated types unlock two things generic params make awkward:
//
//   (a) EQUALITY BOUNDS: `where I: Iterator<Item = u64>` pins the output type
//       *inside the bound*. The iterator type I stays the only type parameter.
//
//   (b) PROJECTION: you can name the output as `I::Item` in your own return
//       type — again without adding a type parameter.
//
// With a generic-param iterator trait (`Stream<T>`) you'd be forced to add a
// separate `T` parameter that leaks into every signature, e.g.
//     fn first_g<S, T>(s: S) -> Option<T> where S: Stream<T>
// ...and because a type could implement Stream<u64> AND Stream<String>, callers
// would have to disambiguate T. Associated types make T a deduced output instead.

// (a) Sum an iterator whose Item is *exactly* u64. Use the equality bound.
fn sum_items<I>(it: I) -> u64
where
    I: Iterator<Item = u64>,
{
    it.sum()
}

// (b) Return the first item, naming the output via projection `I::Item`.
// Note there is NO second type parameter — the return type is I::Item.
fn first<I>(mut it: I) -> Option<I::Item>
where
    I: Iterator,
{
    it.next()
}

fn check_3() {
    let total = sum_items(vec![10u64, 20, 30].into_iter());
    assert_eq!(total, 60);

    let f = first(vec!["a", "b", "c"].into_iter());
    assert_eq!(f, Some("a"));

    // Projection also works with explicit types: I::Item here is i32.
    let n = first(0..5);
    assert_eq!(n, Some(0));
    println!("check_3 ok: equality bound + I::Item projection");
}

// ---------------------------------------------------------------------------
// Rung 4 — Your own iterator: why `Item` is associated, not generic
//
// `Iterator` is the canonical case study. Its signature is:
//     trait Iterator { type Item; fn next(&mut self) -> Option<Self::Item>; }
// Item is ASSOCIATED because a given iterator yields exactly ONE type of value.
// If Item were a generic param (`Iterator<T>`), a single type could "implement
// Iterator" for many T, and then `for x in it` wouldn't know what x is, and the
// whole adapter ecosystem (.map, .filter, .sum) would be ambiguous.
//
// Implement the real std `Iterator` for `Countdown`: it yields n, n-1, ... 1,
// then None. Because you implement the std trait, you get .sum(), .collect(),
// for-loops, etc. for free.

struct Countdown {
    current: u32,
}

impl Iterator for Countdown {
    type Item = u32;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current == 0 {
            None
        } else {
            let current = self.current;
            self.current -= 1;
            Some(current)
        }
    }
}

fn check_4() {
    let cd = Countdown { current: 5 };
    let collected: Vec<u32> = cd.collect();
    assert_eq!(collected, vec![5, 4, 3, 2, 1]);

    // Free adapters, because you implemented the *real* Iterator trait:
    let sum: u32 = Countdown { current: 4 }.sum();
    assert_eq!(sum, 10);

    // And it composes with the rung-3 functions that took `I: Iterator`:
    let total = sum_items(Countdown { current: 3 }.map(|x| x as u64));
    assert_eq!(total, 6);
    println!("check_4 ok: hand-rolled Iterator with type Item");
}

// ---------------------------------------------------------------------------
// Rung 5 — Inference footgun: generic ambiguity vs determined associated output
//
// Generic params are an INPUT — so when several impls exist, the compiler can't
// guess which one you mean, and you must disambiguate (annotation or turbofish).
// Associated types are an OUTPUT determined by Self — so the compiler deduces
// them and you never annotate. Feel both sides.
//
// `Counter` from rung 2 implements BOTH Convert<i32> and Convert<String>.

// A generic free function over the multi-impl trait. T is the OUTPUT but it is
// a generic *param*, so callers must pin it.
fn pull<T>(c: &Counter) -> T
where
    Counter: Convert<T>,
{
    c.convert()
}

fn check_5() {
    let c = Counter { n: 7 };

    // Associated output: produce() is unambiguous, no annotation needed.
    let determined = c.produce();
    assert_eq!(determined, 7);

    // EXPERIMENT (do, then re-comment): uncomment the next line. You'll get
    // error[E0283]: "type annotations needed" — because Counter: Convert<T>
    // holds for MORE THAN ONE T, the compiler can't pick. That's the generic
    // ambiguity associated types avoid.
    //
    // let oops = pull(&c);

    // Two ways to disambiguate a generic param:
    let via_annotation: i32 = pull(&c); // tell it via the binding's type
    let via_turbofish = pull::<String>(&c); // tell it via the call site
    assert_eq!(via_annotation, 7);
    assert_eq!(via_turbofish, "7");
    println!("check_5 ok: generic needs disambiguation, associated does not");
}

// ---------------------------------------------------------------------------
// Rung 6 — Trait objects: dyn forces you to pin associated types
//
// A `dyn Trait` must be a *concrete, fully-known* type behind the pointer. So:
//   - For an associated type, you MUST bind it: `dyn Iterator<Item = u32>`.
//     `dyn Iterator` alone is an error (E0191) — Item is unknown.
//   - For a generic param, each value gives a DIFFERENT trait object type:
//     `dyn Convert<i32>` and `dyn Convert<String>` are unrelated types.
//
// Same underlying idea as before: the associated type is part of the object's
// identity and must be nailed down; the generic param picks which object you mean.

// Return a boxed iterator. Note the REQUIRED `Item = u32` in the dyn type.
fn boxed_counter(n: u32) -> Box<dyn Iterator<Item = u32>> {
    Box::new(Countdown { current: n })
}

// EXPERIMENT (do, then re-comment): try writing this with the Item binding
// removed and see error[E0191]: "the value of the associated type `Item` ...
// must be specified".
//
// fn boxed_counter_bad(n: u32) -> Box<dyn Iterator> {
//     Box::new(Countdown { current: n })
// }

// Return a boxed value behind a *specific* generic instantiation of Convert.
fn boxed_convert_to_int(c: Counter) -> Box<dyn Convert<i32>> {
    Box::new(c)
}

fn check_6() {
    let it = boxed_counter(3);
    let v: Vec<u32> = it.collect();
    assert_eq!(v, vec![3, 2, 1]);

    let dc: Box<dyn Convert<i32>> = boxed_convert_to_int(Counter { n: 9 });
    assert_eq!(dc.convert(), 9);
    println!("check_6 ok: dyn pins the associated type; generic chooses the param");
}

// ---------------------------------------------------------------------------
// Rung 7 — The real-world masterclass: `Add` uses BOTH at once
//
// std's operator trait is:
//     pub trait Add<Rhs = Self> {
//         type Output;
//         fn add(self, rhs: Rhs) -> Self::Output;
//     }
//
// Look at the design decision baked in:
//   - `Rhs` is a GENERIC PARAM (an INPUT): you may want to add Meters to Meters,
//     OR Meters to f64, OR Meters to a Vector. Multiple right-hand sides => many
//     impls per type => generic. It even has a default `Rhs = Self`.
//   - `Output` is an ASSOCIATED TYPE (an OUTPUT): once you fix the *pair*
//     (Self, Rhs), the result type is determined. Meters + f64 always yields one
//     specific type. One answer per pair => associated.
//
// Implement Add for Meters, twice (once per Rhs), each choosing Output.

use std::ops::Add;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Meters(f64);

// Meters + Meters -> Meters. Omitting <...> uses the default `Rhs = Self`.
impl Add for Meters {
    type Output = Meters;
    fn add(self, rhs: Meters) -> Self::Output {
        Meters(self.0 + rhs.0)
    }
}

// Meters + f64 -> Meters. A SECOND impl with a different Rhs (just like rung 2's
// multiple Convert impls), but each still pins exactly one Output.
impl Add<f64> for Meters {
    type Output = Meters;
    fn add(self, rhs: f64) -> Self::Output {
        Meters(self.0 + rhs)
    }
}

fn check_7() {
    let a = Meters(3.0);
    let b = Meters(4.0);

    assert_eq!(a + b, Meters(7.0)); // Add<Meters> via default Rhs = Self
    assert_eq!(a + 0.5, Meters(3.5)); // Add<f64>, the second impl

    // Output is associated => for the pair (Meters, f64) the result type is
    // fixed, and we can even name it with fully-qualified projection syntax.
    let r: <Meters as Add<f64>>::Output = a + 1.0;
    assert_eq!(r, Meters(4.0));
    println!("check_7 ok: Rhs = generic (many impls), Output = associated (one per pair)");
}

// ---------------------------------------------------------------------------
// Rung 8 — Design the split yourself
//
// You're designing a `Graph` abstraction. Apply the rule of thumb (input =>
// generic param, output => associated type) to two decisions:
//
//   - `NodeId`: how a vertex is named (e.g. (i32,i32), or u32, or &str). A given
//     graph has exactly ONE node-id type. Is that an input or an output? -> ?
//   - `Weight`: the cost on an edge. A given graph has exactly ONE weight type.
//     Input or output? -> ?
//
// The trait below already commits to the answer (both are associated — each is a
// single fact about the graph, determined once per impl, not chosen by callers).
// YOUR job: in the impl for `Grid`, *bind* those associated types (the act of
// design), then implement `neighbors`. Leaving the bindings out gives E0046,
// the same "you must name the associated type" error from rung 1.

trait Graph {
    type NodeId: Copy + Eq;
    type Weight;
    // Orthogonal in-bounds neighbors, each paired with the edge weight.
    fn neighbors(&self, n: Self::NodeId) -> Vec<(Self::NodeId, Self::Weight)>;
}

struct Grid {
    w: i32,
    h: i32,
}

impl Graph for Grid {
    type NodeId = (i32, i32);
    type Weight = u32;
    fn neighbors(&self, n: Self::NodeId) -> Vec<(Self::NodeId, Self::Weight)> {
        let (x, y) = n;
        let directions = vec![(1, 0), (-1, 0), (0, 1), (0, -1)];
        directions
            .into_iter()
            .map(|(dx, dy)| ((x + dx, y + dy), 1))
            .filter(|(n, _)| n.0 >= 0 && n.0 < self.w && n.1 >= 0 && n.1 < self.h)
            .collect()
    }
}

// A generic consumer of ANY graph — note it talks about G::NodeId via projection
// (rung 3 again) and needs no extra type parameter for the node type.
fn neighbor_count<G: Graph>(g: &G, n: G::NodeId) -> usize {
    g.neighbors(n).len()
}

fn check_8() {
    let grid = Grid { w: 3, h: 3 };
    assert_eq!(neighbor_count(&grid, (0, 0)), 2); // corner: right, down
    assert_eq!(neighbor_count(&grid, (1, 0)), 3); // top edge
    assert_eq!(neighbor_count(&grid, (1, 1)), 4); // center

    // Weight comes back as the associated type, fixed by the Grid impl.
    let mut ns = grid.neighbors((0, 0));
    ns.sort();
    assert_eq!(ns, vec![((0, 1), 1u32), ((1, 0), 1u32)]);
    println!("check_8 ok: you designed the assoc-type split for Graph");
}

// ---------------------------------------------------------------------------
// Rung 9 — Capstone: build MyIterator + a Map adapter from scratch
//
// This is the whole concept in one machine. You'll re-create the std pattern:
//   - `MyIterator` has `type Item` (associated — one element type per iterator).
//   - `Map<I, F>` is a GENERIC adapter struct (I = inner iterator, F = closure).
//   - The magic line: Map's associated `Item` is COMPUTED from its generics —
//     it's `B`, the closure's output type. So the associated type of the adapter
//     is a function of its generic parameters. That threading is exactly how
//     std's Iterator adapters carry element types through long .map().filter()
//     chains, all resolved at compile time.

trait MyIterator {
    type Item;
    fn next(&mut self) -> Option<Self::Item>;

    // Provided adapter: wrap self in a Map. (This glue is written for you so you
    // can focus on the associated-type threading in the impls below.)
    fn map_it<B, F>(self, f: F) -> Map<Self, F>
    where
        Self: Sized,
        F: FnMut(Self::Item) -> B,
    {
        Map { iter: self, f }
    }
}

// A source iterator yielding next, next+1, ..., end-1.
struct Upto {
    next: u32,
    end: u32,
}

impl MyIterator for Upto {
    type Item = u32;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.end {
            None
        } else {
            let current = self.next;
            self.next += 1;
            Some(current)
        }
    }
}

// The generic adapter. It holds any inner iterator I and a closure F.
struct Map<I, F> {
    iter: I,
    f: F,
}

// Thread the associated type through the generics. The bound says: F maps the
// INNER item type (I::Item) to some output B.
impl<I, F, B> MyIterator for Map<I, F>
where
    I: MyIterator,
    F: FnMut(I::Item) -> B,
{
    type Item = B;
    fn next(&mut self) -> Option<Self::Item> {
        let next = self.iter.next();
        match next {
            Some(x) => Some((self.f)(x)),
            None => None,
        }
    }
}

// Generic consumer over any MyIterator — projection again (no extra type param).
fn collect_my<I: MyIterator>(mut it: I) -> Vec<I::Item> {
    let mut v = Vec::new();
    while let Some(x) = it.next() {
        v.push(x);
    }
    v
}

fn check_9() {
    // u32 -> u32 (squares)
    let squares = Upto { next: 2, end: 5 }.map_it(|x| x * x);
    assert_eq!(collect_my(squares), vec![4, 9, 16]);

    // Map CHANGES the Item type: u32 -> String
    let labels = Upto { next: 0, end: 3 }.map_it(|x| format!("n{x}"));
    assert_eq!(collect_my(labels), vec!["n0", "n1", "n2"]);

    // Chained maps: the associated Item threads u32 -> u32 -> usize, all static.
    let chained = Upto { next: 1, end: 4 }
        .map_it(|x| x + 10)
        .map_it(|x| x as usize * 2);
    assert_eq!(collect_my(chained), vec![22, 24, 26]);
    println!("check_9 ok: hand-rolled MyIterator + Map threads the associated Item");
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
