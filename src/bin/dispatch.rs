// Concept: Static vs dynamic dispatch — monomorphization, code size, when each wins
// Run: cargo run --bin dispatch
//
// Mental model:
//   STATIC dispatch (`<T: Trait>` / `impl Trait`): the compiler knows the concrete
//     type at the call site, so it STAMPS OUT a specialized copy of the function
//     per type (MONOMORPHIZATION) and can inline. Fast, zero indirection — but code
//     bloats and the set of types is fixed at compile time.
//   DYNAMIC dispatch (`dyn Trait`): the concrete type is ERASED behind a fat pointer
//     (data ptr + vtable ptr). The method is looked up at RUNTIME via the vtable.
//     One copy of the code, flexible (heterogeneous collections, runtime choice) —
//     but an indirection that usually can't be inlined.
// Every rung is a choice about which side of that trade you want.
//
// (Sibling file `trait_objects.rs` covers vtable layout & object safety in depth.
//  This ladder is about the STATIC-vs-DYNAMIC axis: cost, code size, when each wins.)
//
// Ladder (DONE marks finished rungs):
//   1. stamp_vs_dyn        - same method via <T: Trait> vs &dyn Trait            [DONE]
//   2. impl_trait_positions- impl Trait in arg position vs return position       [DONE]
//   3. monomorph_proof     - type_name::<T>() proves a copy is stamped per type  [DONE]
//   4. return_branch       - return 1 of 2 types: impl Trait fails, Box<dyn> ok  [DONE]
//   5. hetero_collection   - Vec<T> can't mix; Vec<Box<dyn>> can                 [DONE]
//   6. returns_self        - fn dup(&self)->Self: ok under generic, not via dyn  [DONE]
//   7. closure_pipeline    - Vec<Box<dyn Fn>> registry vs one generic F: Fn      [DONE]
//   8. enum_dispatch       - closed-set third way: enum + match, no vtable       [DONE]
//   9. pipeline_both_ways  - same pipeline static AND dynamic; compare           [ ] <-- capstone

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
    println!("\nAll unlocked rungs passed ✅");
}

// ── Rung 1: stamp_vs_dyn ────────────────────────────────────────────────────────
// Goal: one trait, two ways to accept it. Implement `Greet` for both types, then
// write the SAME logic twice:
//   - `greet_static<T: Greet>(g: &T) -> String`  → static dispatch (monomorphized)
//   - `greet_dynamic(g: &dyn Greet) -> String`    → dynamic dispatch (vtable lookup)
// Both should just return `g.hello()`. The lesson: identical behavior, but the
// static one gets a fresh compiled copy per `T`, while the dynamic one is a single
// function that takes a fat pointer. check_1 proves they agree.

trait Greet {
    fn hello(&self) -> String;
}

struct English;
struct French;

impl Greet for English {
    fn hello(&self) -> String {
        "hello".to_string()
    }
}

impl Greet for French {
    fn hello(&self) -> String {
        "bonjour".to_string()
    }
}

fn greet_static<T: Greet>(g: &T) -> String {
    g.hello()
}

fn greet_dynamic(g: &dyn Greet) -> String {
    g.hello()
}

fn check_1() {
    let en = English;
    let fr = French;

    // Static: the compiler stamps a separate greet_static for English and French.
    assert_eq!(greet_static(&en), "hello");
    assert_eq!(greet_static(&fr), "bonjour");

    // Dynamic: ONE greet_dynamic; the &dyn Greet carries a vtable to the right impl.
    assert_eq!(greet_dynamic(&en), "hello");
    assert_eq!(greet_dynamic(&fr), "bonjour");

    // And you can mix concrete types behind one reference type at runtime:
    let who: &dyn Greet = if std::env::args().count() % 2 == 0 {
        &en
    } else {
        &fr
    };
    let _ = greet_dynamic(who);

    println!("rung 1 ✅ same method, two dispatch strategies");
}

// ── Rung 2: impl_trait_positions ────────────────────────────────────────────────
// `impl Trait` means two very different things depending on WHERE it appears:
//
//   ARGUMENT position:  fn f(x: impl Greet)   ≡  fn f<T: Greet>(x: T)
//     Pure sugar for a generic bound → STATIC dispatch, monomorphized per caller.
//     The CALLER picks the concrete type.
//
//   RETURN position:    fn g() -> impl Greet
//     "I return ONE specific concrete type that I'm not naming." Still STATIC —
//     the compiler knows the real type, the caller just can't. The CALLEE picks,
//     and it must be a SINGLE type for all paths (you'll feel that limit in rung 4).
//
// Goal: implement both functions below.
//   - `loudest(g: impl Greet) -> String`  → return g.hello().to_uppercase()
//   - `default_greeter() -> impl Greet`    → return a French (its concrete type is
//                                             hidden from the caller, but fixed)
// Note check_2 can store the result of default_greeter() in a variable but can
// NOT name its type — that's the abstraction `impl Trait` in return position buys.

fn loudest(g: impl Greet) -> String {
    g.hello().to_uppercase()
}

fn default_greeter() -> impl Greet {
    // NOTE: a bare `todo!()` here won't even compile — the inferred return type
    // would be `!`, and `!: Greet` is false. Return-position `impl Trait` demands
    // a real concrete type *at compile time*. This placeholder is the WRONG one;
    // your turn: make it return the greeter whose hello() is "bonjour".
    French
}

fn check_2() {
    assert_eq!(loudest(English), "HELLO");
    assert_eq!(loudest(French), "BONJOUR");

    // The caller only knows "something that implements Greet":
    let g = default_greeter();
    assert_eq!(g.hello(), "bonjour");

    // Because it's static, you can even feed it straight back into the generic fn:
    assert_eq!(loudest(default_greeter()), "BONJOUR");

    println!("rung 2 ✅ impl Trait: arg = generic sugar, return = one hidden type");
}

// ── Rung 3: monomorph_proof ─────────────────────────────────────────────────────
// "Monomorphization" is an abstract word until you SEE it. Here's the proof: a
// single generic function `tag::<T>()` can report `std::any::type_name::<T>()`.
// If there were really only one compiled `tag`, it couldn't know which T it was
// called with. It can — because the compiler stamped a SEPARATE copy of `tag` for
// every distinct T, each baking in its own type name. That's monomorphization.
//
// Contrast: a `&dyn` parameter erases the type, so a single function literally
// cannot recover the concrete type name this way (it'd just see `dyn ...`).
//
// Goal: implement `tag<T>() -> &'static str` returning the type name of T.
//   Hint: `std::any::type_name::<T>()` — note it takes NO `self`/value argument,
//   it works purely from the type parameter. That's only possible under static
//   dispatch, where T is known at compile time.

fn tag<T>() -> &'static str {
    std::any::type_name::<T>()
}

fn check_3() {
    // Each call is a DIFFERENT monomorphized instance of `tag`, baking in its T.
    let a = tag::<English>();
    let b = tag::<French>();
    let c = tag::<i32>();
    let d = tag::<Vec<String>>();

    // The names differ → distinct compiled copies, each knowing its own type.
    assert!(a.ends_with("English"), "got {a}");
    assert!(b.ends_with("French"), "got {b}");
    assert_eq!(c, "i32");
    assert!(d.contains("Vec") && d.contains("String"), "got {d}");
    assert_ne!(a, b);

    println!("rung 3 ✅ monomorphization: one copy of `tag` stamped per type ({a}, {c}, ...)");
}

// ── Rung 4: return_branch (the footgun) ──────────────────────────────────────────
// Now the limit from rung 2 bites. We want to choose the concrete type at RUNTIME:
//   return English OR French depending on a flag.
//
// Return-position `impl Trait` CANNOT express this — it means "one fixed concrete
// type", but here the type depends on a runtime value. Prove it to yourself:
// temporarily uncomment `broken_pick` below and run — read the compiler error
// ("`if` and `else` have incompatible types" / "expected English, found French").
// Then re-comment it so the file builds.
//
// THIS is the moment static dispatch can't help: the type isn't known until
// runtime. The fix is to ERASE it behind a trait object — `Box<dyn Greet>` — which
// gives a single uniform type whatever branch you take. That's dynamic dispatch
// earning its keep.
//
// Goal: implement `pick_greeter(french: bool) -> Box<dyn Greet>` returning a boxed
// English or French based on the flag.

// fn broken_pick(french: bool) -> impl Greet {
//     if french { French } else { English }   // <-- uncomment to SEE the error
// }

fn pick_greeter(french: bool) -> Box<dyn Greet> {
    if french {
        Box::new(French)
    } else {
        Box::new(English)
    }
}

fn check_4() {
    assert_eq!(pick_greeter(true).hello(), "bonjour");
    assert_eq!(pick_greeter(false).hello(), "hello");

    // Runtime-decided, stored uniformly — only possible because the type is erased:
    let flags = [true, false, true];
    let greeters: Vec<Box<dyn Greet>> = flags.iter().map(|&f| pick_greeter(f)).collect();
    let joined: Vec<String> = greeters.iter().map(|g| g.hello()).collect();
    assert_eq!(joined, vec!["bonjour", "hello", "bonjour"]);

    println!("rung 4 ✅ runtime-chosen type needs Box<dyn>; impl Trait can't");
}

// ── Rung 5: hetero_collection ────────────────────────────────────────────────────
// A `Vec<T>` is MONOMORPHIC: every element is the exact same type T. So you cannot
// put an English and a French in the same plain Vec — they're different types.
// See it: uncomment `_bad` below ("expected `English`, found `French`"), then
// re-comment. (`vec![English; 3]` is fine — all one type. Mixing is what breaks.)
//
// To store a MIXED bag of "things that implement Greet", erase each element behind
// a trait object: `Vec<Box<dyn Greet>>`. Every slot is now the same type (a fat
// pointer), so the Vec is happy, and each `.hello()` dispatches dynamically.
// This heterogeneity is the single biggest thing dynamic dispatch buys you.
//
// Goal: implement `build_crowd()` to return a Vec<Box<dyn Greet>> containing, in
// order, an English, a French, then another English.

// let _bad = vec![English, French];   // <-- uncomment INSIDE check_5 to see it fail

fn build_crowd() -> Vec<Box<dyn Greet>> {
    vec![Box::new(English), Box::new(French), Box::new(English)]
}

fn check_5() {
    let crowd = build_crowd();
    let hellos: Vec<String> = crowd.iter().map(|g| g.hello()).collect();
    assert_eq!(hellos, vec!["hello", "bonjour", "hello"]);

    // For contrast: a homogeneous Vec needs no boxing — all one known type.
    let same = vec![English, English];
    assert_eq!(same.len(), 2);

    println!("rung 5 ✅ mixed types live only in Vec<Box<dyn>>, not Vec<T>");
}

// ── Rung 6: returns_self ─────────────────────────────────────────────────────────
// Rung 5 showed something dynamic dispatch can do that static can't. This rung is
// the MIRROR: a trait static dispatch handles fine but that can't be a `dyn` at all.
//
// A method that returns `Self` (or takes `Self` by value, or is generic) makes a
// trait NOT object-safe: a vtable can't describe "returns a value whose size/type
// is the erased concrete type". So `Box<dyn Doubler>` is rejected outright. But a
// generic bound `<T: Doubler>` is totally fine — there, the concrete type is known,
// so the compiler knows exactly what `-> Self` means and how big the result is.
//
// See it: uncomment the `_obj` line in check_6 ("the trait `Doubler` cannot be made
// into an object ... because method `doubled` references the `Self` type"), then
// re-comment.
//
// Goal:
//   - impl Doubler for i32     → doubled() returns self * 2
//   - impl Doubler for String  → doubled() returns the string concatenated with
//                                itself (e.g. "ab" -> "abab")
//   - twice<T: Doubler>(x: T) -> T  → return x.doubled()

trait Doubler {
    fn doubled(&self) -> Self;
}

impl Doubler for i32 {
    fn doubled(&self) -> Self {
        *self * 2
    }
}

impl Doubler for String {
    fn doubled(&self) -> Self {
        self.clone() + self
    }
}

fn twice<T: Doubler>(x: T) -> T {
    x.doubled()
}

fn check_6() {
    // Static dispatch: -> Self is no problem, the type is known per instantiation.
    assert_eq!(twice(21_i32), 42);
    assert_eq!(twice("ab".to_string()), "abab");

    // Dynamic dispatch is IMPOSSIBLE here — uncomment to read the object-safety error:
    // let _obj: Box<dyn Doubler> = Box::new(21_i32);

    println!("rung 6 ✅ -> Self: fine under generics, forbidden behind dyn");
}

// ── Rung 7: closure_pipeline ─────────────────────────────────────────────────────
// Closures are where every Rust programmer meets this decision in the wild. EVERY
// closure has its own unique, unnameable type — even two closures with identical
// signatures are different types. So:
//   - To accept ONE closure: a generic `F: Fn(...)` bound → STATIC dispatch, the
//     closure gets inlined, zero overhead. The classic `Iterator::map` shape.
//   - To STORE MANY different closures together (a callback registry, a pipeline,
//     an event table): you must erase them → `Box<dyn Fn(...)>`. One vtable call
//     per invocation, but now they share a type and can live in one Vec.
//
// See the footgun: uncomment `_steps` in check_7 — `vec![|x| x+1, |x| x*2]` refuses
// to compile because the two closures are different types. Boxing as dyn fixes it.
//
// Goal:
//   - apply_static<F: Fn(i32) -> i32>(f: F, x: i32) -> i32   → return f(x)
//   - run_pipeline(steps: &[Box<dyn Fn(i32) -> i32>], start: i32) -> i32
//        → fold `start` through every step in order (apply steps[0], then [1], ...)
//   - build_pipeline(add: i32) -> Vec<Box<dyn Fn(i32) -> i32>>
//        → three boxed closures: |x| x + 1, then |x| x * 2, then a closure that
//          CAPTURES `add` and returns x + add. (The capture is why each is a
//          distinct type — and why they must be boxed to share a Vec.)

fn apply_static<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
    f(x)
}

fn run_pipeline(steps: &[Box<dyn Fn(i32) -> i32>], start: i32) -> i32 {
    steps.iter().fold(start, |acc, step| step(acc))
}

fn build_pipeline(add: i32) -> Vec<Box<dyn Fn(i32) -> i32>> {
    vec![
        Box::new(|x| x + 1),
        Box::new(|x| x * 2),
        Box::new(move |x| x + add),
    ]
}

fn check_7() {
    // Static: one closure, inlined, no allocation.
    assert_eq!(apply_static(|x| x + 1, 10), 11);
    assert_eq!(apply_static(|x| x * 3, 10), 30);

    // This won't compile — different closure types in one Vec. Uncomment to see:
    // let _steps = vec![|x: i32| x + 1, |x: i32| x * 2];

    // Dynamic: heterogeneous closures (incl. a capturing one) sharing a pipeline.
    // start=5 -> +1 =6 -> *2 =12 -> +add(=100) =112
    let pipe = build_pipeline(100);
    assert_eq!(run_pipeline(&pipe, 5), 112);

    println!("rung 7 ✅ one closure = generic F; many closures = Vec<Box<dyn Fn>>");
}

// ── Rung 8: enum_dispatch (the closed-set third way) ─────────────────────────────
// dyn buys heterogeneity but costs an allocation + vtable indirection per element.
// Generics are zero-cost but can't store mixed types. When your set of types is
// CLOSED (you know all of them at compile time), there's a third option that gets
// the best of both: put them in an ENUM and dispatch with `match`.
//
//   - Heterogeneous storage:  Vec<Shape> holds circles AND rects — no Box, no heap
//     allocation per element; each value lives INLINE in the Vec.
//   - Static dispatch:        `match` compiles to a jump on the discriminant; the
//     arms can inline. No vtable pointer chase.
//   - The trade:              the set is CLOSED. Adding a variant means editing the
//     enum + every match (the compiler forces exhaustiveness — a feature here).
//     And every element is sized to the LARGEST variant.
//
// This is exactly what the `enum_dispatch` crate automates, and why `serde_json::
// Value`, AST nodes, and state machines are enums, not `Vec<Box<dyn Node>>`.
//
// Goal:
//   - impl Shape::area(&self) -> f64 via `match self { ... }`
//        Circle { r }      → std::f64::consts::PI * r * r
//        Rect { w, h }     → w * h
//   - total_area(shapes: &[Shape]) -> f64  → sum of each shape's area
//     (note: &[Shape], NOT &[Box<dyn ...>] — no boxing needed)

enum Shape {
    Circle { r: f64 },
    Rect { w: f64, h: f64 },
}

impl Shape {
    fn area(&self) -> f64 {
        match self {
            Self::Circle { r } => std::f64::consts::PI * r * r,
            Self::Rect { w, h } => w * h,
        }
    }
}

fn total_area(shapes: &[Shape]) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}

fn check_8() {
    // A heterogeneous, inline, alloc-free collection — no Box in sight:
    let shapes = vec![
        Shape::Circle { r: 1.0 },
        Shape::Rect { w: 2.0, h: 3.0 },
        Shape::Circle { r: 2.0 },
    ];
    let total = total_area(&shapes);
    // PI*1 + 6 + PI*4 = 5*PI + 6 ≈ 21.7080
    assert!(
        (total - (5.0 * std::f64::consts::PI + 6.0)).abs() < 1e-9,
        "got {total}"
    );

    // Each element is stored INLINE, sized to the largest variant — not a fat ptr.
    // (Rect = two f64s = 16 bytes payload; + discriminant, rounded for align.)
    assert_eq!(std::mem::size_of::<Box<dyn Greet>>(), 16); // dyn = 2 words (fat ptr)
    assert!(std::mem::size_of::<Shape>() >= 16);

    println!(
        "rung 8 ✅ enum dispatch: mixed types inline + match, no vtable (Shape = {} bytes)",
        std::mem::size_of::<Shape>()
    );
}

// ── Rung 9: pipeline_both_ways (CAPSTONE) ────────────────────────────────────────
// Prove you own the whole trade: build the SAME data-transform pipeline three ways
// and show they compute identical results.
//
// The logical pipeline is: Add(3) → Mul(2) → Neg, applied to a start value.
//   For start = 5:  (5 + 3) = 8  → (8 * 2) = 16  → -16.
//
// (A) STATIC, type-level composition  — Compose<A, B>
//       The entire pipeline is ONE type, `Compose<Add, Compose<Mul, Neg>>`. No
//       allocation, no vtable; the compiler can inline the whole chain into a few
//       instructions. Cost: the pipeline's shape & length are FIXED at compile time
//       (it's literally encoded in the type) — you can't decide it at runtime.
//
// (B) DYNAMIC, trait objects          — Vec<Box<dyn Transform>>
//       Build the pipeline at RUNTIME: any length, any order, read from config, etc.
//       Cost: a heap box per stage + a vtable call per `apply`.
//
// (C) ENUM dispatch                   — Vec<Op>
//       Runtime-built like (B) but a CLOSED set: inline storage, `match` dispatch,
//       no per-stage allocation, no vtable. Cost: adding an op means editing `Op`.
//
// Goal — fill in every `todo!`:
//   - impl Transform for Add / Mul / Neg   (x + n, x * n, -x)
//   - Compose::apply                        (apply A, then feed result into B)
//   - run_static                            (build the nested Compose and apply)
//   - run_dynamic                           (fold start through the boxed stages)
//   - Op::apply                             (match)
//   - run_enum                              (fold start through the enum stages)

trait Transform {
    fn apply(&self, x: i32) -> i32;
}

struct Add(i32);
struct Mul(i32);
struct Neg;

impl Transform for Add {
    fn apply(&self, x: i32) -> i32 {
        x + self.0
    }
}
impl Transform for Mul {
    fn apply(&self, x: i32) -> i32 {
        x * self.0
    }
}
impl Transform for Neg {
    fn apply(&self, x: i32) -> i32 {
        -x
    }
}

// (A) STATIC: A then B, both known at compile time. The whole pipeline is a TYPE.
struct Compose<A, B>(A, B);
impl<A: Transform, B: Transform> Transform for Compose<A, B> {
    fn apply(&self, x: i32) -> i32 {
        self.1.apply(self.0.apply(x))
    }
}

fn run_static(start: i32) -> i32 {
    Compose(Add(3), Compose(Mul(2), Neg)).apply(start)
}

// (B) DYNAMIC: trait objects, pipeline assembled at runtime.
fn run_dynamic(start: i32) -> i32 {
    let pipe: Vec<Box<dyn Transform>> = vec![Box::new(Add(3)), Box::new(Mul(2)), Box::new(Neg)];
    // your turn: fold `start` through pipe (each stage dispatched via its vtable)
    pipe.iter().fold(start, |acc, t| t.apply(acc))
}

// (C) ENUM: closed set, inline, match dispatch.
enum Op {
    Add(i32),
    Mul(i32),
    Neg,
}
impl Op {
    fn apply(&self, x: i32) -> i32 {
        match self {
            Self::Add(n) => x + n,
            Self::Mul(n) => x * n,
            Self::Neg => -x,
        }
    }
}

fn run_enum(start: i32) -> i32 {
    let pipe = vec![Op::Add(3), Op::Mul(2), Op::Neg];
    // your turn: fold `start` through pipe (each stage dispatched via match)
    pipe.iter().fold(start, |acc, op| op.apply(acc))
}

fn check_9() {
    let s = run_static(5);
    let d = run_dynamic(5);
    let e = run_enum(5);

    // All three strategies compute the SAME thing — they only differ in HOW.
    assert_eq!(s, -16, "static");
    assert_eq!(d, -16, "dynamic");
    assert_eq!(e, -16, "enum");
    assert_eq!(s, d);
    assert_eq!(d, e);

    // And a sanity check on a different input, to be sure it's real composition:
    assert_eq!(run_static(0), -6); // (0+3)*2 = 6 -> -6
    assert_eq!(run_dynamic(0), -6);
    assert_eq!(run_enum(0), -6);

    println!("rung 9 ✅ CAPSTONE — same pipeline, three dispatch strategies, one result");
}
