// Blanket impls & coherence
// Run: cargo run --bin blanket_coherence
//
// Mental model: an `impl` block is a *fact* you assert to the compiler.
// COHERENCE = "there is exactly one impl of trait T for type X" — never two,
// never ambiguous. A BLANKET impl (`impl<T> Trait for T`) asserts a fact about
// infinitely many types at once. The ORPHAN RULE stops two crates from
// asserting conflicting facts about types neither of them owns.
//
// Ladder (✅ = done):
//   1. Unconditional blanket impl        impl<T> Named for T            [foundations]
//   2. Conditional blanket impl          impl<T: Display> Loud for T    [foundations]
//   3. From -> Into trick                blanket impl gives .into()     [mechanics]
//   4. Extension trait                   impl<I: Iterator> IterExt      [mechanics]
//   5. Orphan rule (E0117)               foreign trait + foreign type   [footgun]
//   6. Overlapping impls (E0119)         blanket vs concrete collide    [footgun]
//   7. Uncovered type param (E0210)      the "covered" rule             [footgun]
//   8. Newtype workaround                wrap foreign type legally      [real-world]
//   9. Capstone: sealed extension trait  blanket + sealing + coherence  [capstone]

use std::fmt::Display;

// ---------------------------------------------------------------------------
// Rung 1 — Unconditional blanket impl  [foundations]
//
// Define a trait `Named` with a method `type_label(&self) -> &'static str`
// that just returns a fixed string, say "a value".
//
// Then write ONE blanket impl that gives this method to EVERY type T with no
// bounds at all:  impl<T> Named for T { ... }
//
// The point: after that single impl, an i32, a String, and your own structs
// ALL have `.type_label()` for free. You never wrote a per-type impl.
// ---------------------------------------------------------------------------

trait Named {
    fn type_label(&self) -> &'static str;
}

// TODO rung 1: write `impl<T> Named for T { ... }` here.
impl<T> Named for T {
    fn type_label(&self) -> &'static str {
        "a value"
    }
}

struct Widget;

fn check_1() {
    let n = 42i32;
    let s = String::from("hi");
    let w = Widget;
    // All three got the method from a single blanket impl:
    assert_eq!(n.type_label(), "a value");
    assert_eq!(s.type_label(), "a value");
    assert_eq!(w.type_label(), "a value");
    println!("check_1 ok: one blanket impl, every type gets the method");
}

// ---------------------------------------------------------------------------
// Rung 2 — Conditional blanket impl  [foundations]
//
// Real blanket impls almost always carry a BOUND. Define a trait `Loud` with
// `fn loud(&self) -> String`, and blanket-impl it ONLY for types that are
// `Display`:   impl<T: Display> Loud for T { ... }
// `loud` should format self and append "!!!" — e.g. 7 -> "7!!!".
//
// The lesson: the impl now applies to a *subset* of all types. `i32` and `&str`
// are Display, so they get `.loud()`. A type that is NOT Display does not —
// uncomment the two lines marked (A) below and watch the compiler reject them,
// then re-comment them. That rejection IS the bound doing its job.
// ---------------------------------------------------------------------------

trait Loud {
    fn loud(&self) -> String;
}

// TODO rung 2: write `impl<T: Display> Loud for T { ... }` here.
impl<T: Display> Loud for T {
    fn loud(&self) -> String {
        format!("{}!!!", self)
    }
}

struct NotDisplay; // deliberately does NOT implement Display

fn check_2() {
    assert_eq!(7i32.loud(), "7!!!");
    assert_eq!("hi".loud(), "hi!!!");

    // (A) Uncomment these two lines: the compiler should reject `.loud()` on a
    //     non-Display type. Read the error, understand it, then re-comment.
    // let nd = NotDisplay;
    // let _ = nd.loud();

    let _ = NotDisplay; // keep the type "used" so it doesn't warn
    println!("check_2 ok: blanket impl gated by a Display bound");
}

// ---------------------------------------------------------------------------
// Rung 3 — The From -> Into trick  [mechanics]
//
// This is THE famous blanket impl in std:
//     impl<T, U> Into<U> for T where U: From<T> { fn into(self) -> U { U::from(self) } }
// You implement `From`, and `.into()` appears for free, in the right direction.
// Let's reconstruct it with our own traits so the machinery is visible.
//
// Given `MyFrom<T>` (below), write ONE blanket impl of `MyInto<U>` such that:
//   - any type T that can be turned into U via `U: MyFrom<T>` automatically
//     gets `t.my_into()` returning a U.
// Then implement `MyFrom<Celsius> for Fahrenheit` so the conversion exists.
//
// Note the generic shape: the impl is `for T` (the *source*), and `U` is a free
// type parameter constrained by the where-clause. You write ZERO impls of
// MyInto directly — the blanket impl covers every convertible pair.
// ---------------------------------------------------------------------------
trait MyFrom<T> {
    fn my_from(value: T) -> Self;
}

trait MyInto<U> {
    fn my_into(self) -> U;
}

// TODO rung 3a: blanket impl `MyInto<U> for T where U: MyFrom<T>`.
impl<T, U> MyInto<U> for T
where
    U: MyFrom<T>,
{
    fn my_into(self) -> U {
        U::my_from(self)
    }
}

#[derive(Clone, Copy)]
struct Celsius(f64);

struct Fahrenheit(f64);

impl MyFrom<Celsius> for Fahrenheit {
    fn my_from(value: Celsius) -> Fahrenheit {
        Fahrenheit(value.0 * 9.0 / 5.0 + 32.0)
    }
}

// TODO rung 3b: impl `MyFrom<Celsius> for Fahrenheit` (F = C * 9/5 + 32).

fn check_3() {
    let c = Celsius(100.0);
    // We only ever implemented MyFrom, yet `.my_into()` exists via the blanket:
    let f: Fahrenheit = c.my_into();
    assert_eq!(f.0, 212.0);

    // And the explicit MyFrom direction still works:
    let f2 = Fahrenheit::my_from(Celsius(0.0));
    assert_eq!(f2.0, 32.0);
    println!("check_3 ok: implement MyFrom, get MyInto free via one blanket impl");
}

// ---------------------------------------------------------------------------
// Rung 4 — Extension trait  [mechanics]
//
// The "extension trait" pattern: you can't add an inherent method to a foreign
// type (you don't own `Iterator`), but you CAN define your own trait and
// blanket-impl it for everything that implements `Iterator`. This is exactly
// how `itertools` bolts `.chunks()`, `.dedup()` etc. onto every iterator.
//
// Define `IterExt: Iterator` (supertrait) with a provided method
//     fn sum_of_squares(self) -> u64
// that consumes the iterator and returns the sum of i*i for each item, where
// items are u64. Then ONE blanket impl: `impl<I: Iterator<Item = u64>> IterExt for I`.
//
// Hints on shape:
//   - Make `IterExt` require `Iterator<Item = u64>` so `.map`/`.sum` are usable.
//   - You can give `sum_of_squares` a default body in the trait, then leave the
//     blanket impl body EMPTY ({}) — it inherits the default. That's the
//     idiomatic ext-trait style. (Or implement it in the impl; your call.)
//   - `self` is the iterator; you may call `self.map(...).sum()`.
// ---------------------------------------------------------------------------

// TODO rung 4a: define `trait IterExt: Iterator<Item = u64> { fn sum_of_squares(self) -> u64 {...} }`
//               (a provided/default method body).

trait IterExt<I: Iterator<Item = u64>> {
    fn sum_of_squares(self) -> u64;
}

impl<I: Iterator<Item = u64>> IterExt<I> for I {
    fn sum_of_squares(self) -> u64 {
        self.map(|i| i * i).sum()
    }
}

fn check_4() {
    let v = vec![1u64, 2, 3, 4];
    // 1 + 4 + 9 + 16 = 30
    assert_eq!(v.into_iter().sum_of_squares(), 30);
    // Works on a range too — anything that is Iterator<Item = u64>:
    assert_eq!((1u64..=3).sum_of_squares(), 14);
    // And it composes with normal adapters, since the receiver is just an iterator:
    assert_eq!((0u64..10).filter(|n| n % 2 == 0).sum_of_squares(), 120);
    println!("check_4 ok: extension trait via blanket impl over Iterator");
}

// ---------------------------------------------------------------------------
// Rung 5 — The orphan rule (E0117)  [footgun]
//
// Coherence's flagship guardrail. To `impl SomeTrait for SomeType`, AT LEAST
// ONE of {the trait, the type} must be LOCAL to your crate. If BOTH are foreign
// (you own neither), it's rejected — E0117 "only traits defined in the current
// crate can be implemented for types defined outside of the crate". Why: two
// crates could each `impl Display for Vec<i32>` differently and link together,
// and the compiler would have two conflicting facts. The orphan rule makes that
// collision impossible to even write.
//
// Three sub-tasks:
//
// 5a (SEE the error): uncomment the `(B)` impl below. `Display` is foreign and
//     `Vec<i32>` is foreign -> E0117. Read it, then re-comment it.
//
// 5b (LOCAL TRAIT + foreign type = LEGAL): you own `Summary`, so you may impl it
//     for the foreign `Vec<i32>`. Implement `Summary for Vec<i32>` returning
//     e.g. "Vec of N items".
//
// 5c (foreign trait + LOCAL TYPE = LEGAL): you own `Temperature`, so you may impl
//     the foreign `Display` for it. Implement `Display for Temperature` printing
//     e.g. "21.5°C".
// ---------------------------------------------------------------------------

// (B) rung 5a — UNCOMMENT to witness E0117, then RE-COMMENT:
// impl Display for Vec<i32> {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{:?}", self)
//     }
// }

trait Summary {
    fn summary(&self) -> String;
}

// TODO rung 5b: impl `Summary for Vec<i32>` (local trait, foreign type => legal).
impl Summary for Vec<i32> {
    fn summary(&self) -> String {
        format!("Vec of {} items", self.len())
    }
}

struct Temperature(f64);

impl Display for Temperature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}°C", self.0)
    }
}

fn check_5() {
    let v = vec![10, 20, 30];
    assert_eq!(v.summary(), "Vec of 3 items");

    let t = Temperature(21.5);
    assert_eq!(format!("{}", t), "21.5°C");
    println!("check_5 ok: orphan rule — local trait OR local type makes an impl legal");
}

// ---------------------------------------------------------------------------
// Rung 6 — Overlapping impls (E0119)  [footgun]
//
// The OTHER coherence error. The orphan rule (rung 5) is about who's allowed to
// write an impl. OVERLAP is about two impls — both legal on their own, both in
// YOUR crate — that could match the same type. Coherence still says "exactly
// one", so it rejects them: E0119 "conflicting implementations".
//
// You'll provoke it, understand WHY stable Rust can't just "prefer the more
// specific one" (that's specialization — still nightly-only), then resolve it.
//
// Setup: trait `Kind { fn kind(&self) -> &'static str; }` with a blanket impl
// `impl<T> Kind for T` returning "generic". You ALSO want i32 to say "integer".
//
// 6a (SEE the error): uncomment the `(C)` concrete impl below. It overlaps the
//     blanket impl (i32 matches BOTH) -> E0119. Read it. Why can't the compiler
//     pick the i32-specific one? Because choosing "most specific" = specialization,
//     which isn't stable; without it, two matching impls is just ambiguous.
//
// 6b (RESOLVE without specialization): the stable fix is to NOT overlap. Remove
//     the blanket impl's claim on i32 by NOT using a blanket impl for the
//     general case. Instead, write CONCRETE impls per type. Implement `Kind`
//     explicitly for `i32` ("integer") and for `&str` ("string"). Delete/keep
//     the (C) edit as needed so that exactly ONE impl matches each type.
//     (Leave the blanket `impl<T> Kind for T` COMMENTED OUT — it's the problem.)
// ---------------------------------------------------------------------------

trait Kind {
    fn kind(&self) -> &'static str;
}

// (D) The blanket impl that CONFLICTS — keep this COMMENTED OUT for 6b:
// impl<T> Kind for T {
//     fn kind(&self) -> &'static str {
//         "generic"
//     }
// }

// (C) rung 6a — with (D) uncommented, ALSO uncomment this to witness E0119,
//     then re-comment BOTH and do 6b:
impl Kind for i32 {
    fn kind(&self) -> &'static str {
        "integer"
    }
}

// TODO rung 6b: write concrete `impl Kind for i32` ("integer") and
//               `impl Kind for &str` ("string"). No blanket impl => no overlap.

impl Kind for &str {
    fn kind(&self) -> &'static str {
        "string"
    }
}

fn check_6() {
    assert_eq!(42i32.kind(), "integer");
    assert_eq!("hi".kind(), "string");
    println!("check_6 ok: no overlap — exactly one impl matches each type");
}

// ---------------------------------------------------------------------------
// Rung 7 — Uncovered type parameter (E0210)  [footgun]
//
// The orphan rule has a subtle clause most people never internalize: it's not
// just "some type must be local" — it's about ORDER and COVERAGE. For a foreign
// trait, the impl is allowed only if a LOCAL type appears *before* any bare
// (uncovered) type parameter, scanning Self then the trait's type args.
//
//   - A bare `T` is UNCOVERED.
//   - A `T` wrapped in YOUR local type, like `Wrapper<T>`, is COVERED.
//
// Why: an uncovered `T` sitting in the Self position means your blanket impl
// reaches out and claims a foreign trait for types you don't own (every T) —
// which a future upstream impl could collide with. The rule forbids it.
//
// 7a (SEE E0210): uncomment the `(E)` impl. `Add` is foreign; here Self is a
//     BARE `T` (uncovered) and the only local type, `Meters`, appears AFTER it
//     as the Rhs. Local type doesn't come first -> E0210. Read it, re-comment.
//
// 7b (FIX by ordering): put the local type in the Self position. Implement
//     `Add` for `Meters` (Rhs = Meters, Output = Meters) so Meters + Meters
//     adds the inner f64s. Now Self is local and first -> legal.
//
// 7c (COVERED param is fine): implement `From<f64> for Length<f64>`... actually
//     the lesson is that a param COVERED by a local type is allowed even with a
//     foreign trait. Implement `impl<T> From<T> for Wrapped<T>` (foreign trait
//     From, but T is covered by your local `Wrapped<T>`) storing the value.
// ---------------------------------------------------------------------------

use std::ops::Add;

#[derive(Clone, Copy, PartialEq, Debug)]
struct Meters(f64);

// (E) rung 7a — UNCOMMENT to witness E0210 (bare Self `T` before local `Meters`),
//     then RE-COMMENT:
// impl<T> Add<Meters> for T {
//     type Output = Meters;
//     fn add(self, rhs: Meters) -> Meters {
//         rhs
//     }
// }

// TODO rung 7b: impl `Add for Meters` (Output = Meters), adding the inner f64s.

impl Add<Meters> for Meters {
    type Output = Meters;
    fn add(self, rhs: Meters) -> Meters {
        Meters(self.0 + rhs.0)
    }
}

struct Wrapped<T>(T);

// TODO rung 7c: impl `From<T> for Wrapped<T>` — legal because T is COVERED by
//               your local `Wrapped<T>`, even though `From` is foreign.

impl<T> From<T> for Wrapped<T> {
    fn from(value: T) -> Self {
        Wrapped(value)
    }
}

fn check_7() {
    assert_eq!(Meters(2.0) + Meters(3.0), Meters(5.0));

    let w: Wrapped<i32> = 42.into(); // uses your covered From impl
    assert_eq!(w.0, 42);
    println!("check_7 ok: uncovered param => E0210; local-as-Self & covered params are legal");
}

// ---------------------------------------------------------------------------
// Rung 8 — The newtype workaround  [real-world]
//
// Rung 5a showed `impl Display for Vec<i32>` is illegal (foreign + foreign).
// The standard escape hatch: wrap the foreign type in YOUR OWN local newtype,
// then impl the foreign trait for the newtype. Now one side is local => legal.
// This is THE idiomatic way to "add a trait impl to a type you don't own".
//
// Build `Wrapper(Vec<i32>)` and:
//
// 8a: impl `Display for Wrapper` printing the elements joined by ", " inside
//     brackets, e.g. Wrapper(vec![1,2,3]) -> "[1, 2, 3]".  (Now legal: Wrapper
//     is local.)
//
// 8b: the ergonomic cost of newtypes is you lose the inner type's methods. Fix
//     it with `Deref`: impl `Deref for Wrapper { type Target = Vec<i32>; ... }`
//     so `wrapper.len()`, `wrapper.iter()` etc. work via deref coercion.
//     (This is exactly what std's `String`/`PathBuf` newtype-ish wrappers do.)
// ---------------------------------------------------------------------------

use std::ops::Deref;

struct Wrapper(Vec<i32>);

impl Display for Wrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}]",
            self.0
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        )
    }
}

impl Deref for Wrapper {
    type Target = Vec<i32>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn check_8() {
    let w = Wrapper(vec![1, 2, 3]);
    assert_eq!(format!("{}", w), "[1, 2, 3]");

    // Thanks to Deref, Vec<i32>'s methods work directly on Wrapper:
    assert_eq!(w.len(), 3);
    assert_eq!(w.iter().sum::<i32>(), 6);
    assert_eq!(w.first(), Some(&1));
    println!("check_8 ok: newtype makes the impl legal; Deref restores ergonomics");
}

// ---------------------------------------------------------------------------
// Rung 9 — Capstone: a sealed extension-trait mini-library  [capstone]
//
// Tie it all together. You'll ship a tiny "stats" library: an extension trait
// `StatsExt` that adds `.mean()` and `.variance()` to ANY iterator of f64 via a
// blanket impl (rung 4 pattern). Then you'll SEAL it so downstream code can use
// the methods but can NOT implement `StatsExt` for their own types.
//
// Why sealing needs coherence: the seal is a PRIVATE trait `Sealed` with its own
// blanket impl. You make `StatsExt: Sealed`. Downstream can't name `Sealed`
// (it's private) and can't satisfy it (only YOUR blanket impl does), so by
// coherence their `impl StatsExt for TheirType` can never compile — there's no
// way for them to also assert `TheirType: Sealed`. This is exactly how
// `std::error::Error`-adjacent and many real crates lock a trait down.
//
// Build it in pieces:
//
// 9a: a private module `sealed` with `pub trait Sealed {}` and ONE blanket impl
//     `impl<I: Iterator<Item = f64>> Sealed for I {}`. (Public-in-private: the
//     trait is pub *inside* a private module, so outsiders can't name it.)
//
// 9b: `trait StatsExt: Iterator<Item = f64> + sealed::Sealed` with two PROVIDED
//     methods:
//       fn mean(self) -> f64;          // average; empty iterator -> 0.0
//       fn variance(self) -> f64;      // population variance; empty -> 0.0
//     Hint: collect into a Vec<f64> first (variance is a two-pass computation:
//     mean, then average of (x - mean)^2). Require `Self: Sized` on methods that
//     take `self` by value.
//
// 9c: the blanket impl `impl<I: Iterator<Item = f64>> StatsExt for I {}` (empty
//     body — methods come from the provided defaults).
//
// 9d (PROVE the seal): uncomment the `(F)` block below — a downstream-style
//     `impl StatsExt for NotAnIterator`. It must FAIL to compile because
//     `NotAnIterator` is neither an `Iterator<Item = f64>` nor `Sealed`. Read
//     the error (it'll complain the bounds aren't satisfied), then re-comment.
// ---------------------------------------------------------------------------

// TODO rung 9a: write `mod sealed { pub trait Sealed {} impl<...> Sealed for I {} }`
mod sealed {
    pub trait Sealed {}
    impl<I: Iterator<Item = f64>> Sealed for I {}
}

trait StatsExt: Iterator<Item = f64> + sealed::Sealed {
    fn mean(self) -> f64;
    fn variance(self) -> f64;
}

impl<I: Iterator<Item = f64> + sealed::Sealed> StatsExt for I {
    fn mean(self) -> f64 {
        let values: Vec<f64> = self.collect();
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / values.len() as f64
        }
    }
    fn variance(self) -> f64 {
        let values: Vec<f64> = self.collect();
        if values.is_empty() {
            0.0
        } else {
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / values.len() as f64
        }
    }
}

// TODO rung 9b: define `trait StatsExt: Iterator<Item = f64> + sealed::Sealed { ... }`
//               with provided `mean` and `variance`.

// TODO rung 9c: blanket impl `impl<I: Iterator<Item = f64>> StatsExt for I {}`

struct NotAnIterator;

// (F) rung 9d — UNCOMMENT to prove the seal holds, then RE-COMMENT:
// impl StatsExt for NotAnIterator {}

fn check_9() {
    let data = [1.0, 2.0, 3.0, 4.0];
    assert_eq!(data.into_iter().mean(), 2.5);
    assert_eq!(data.into_iter().variance(), 1.25);

    // Works on any f64 iterator, composes with adapters:
    let evens_mean = (1..=6).map(|n| n as f64).filter(|x| x % 2.0 == 0.0).mean();
    assert_eq!(evens_mean, 4.0); // (2 + 4 + 6) / 3

    // Empty case:
    let empty: [f64; 0] = [];
    assert_eq!(empty.into_iter().mean(), 0.0);

    let _ = NotAnIterator;
    println!("check_9 ok: sealed extension trait — blanket impl + sealing + coherence");
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
    // check_6();
    // check_7();
    // check_8();
    // check_9();
}
