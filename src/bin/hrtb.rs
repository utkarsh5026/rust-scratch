// HRTB — Higher-Ranked Trait Bounds (`for<'a>`)
// Run: cargo run --bin hrtb
//
// Mental model: a normal `<'a>` means "the CALLER picks one 'a, then the bound
// must hold for it". `for<'a>` flips it: "the bound must hold for EVERY 'a, and
// the CALLEE chooses it fresh at each use". You need it when a closure / impl
// must work for borrows whose lifetime doesn't exist yet at the bound site.
// `Fn(&T)` already secretly means `for<'a> Fn(&'a T)`.
//
// Ladder:
//   1. [x] Implicit for<'a>        — Fn(&T) bound fed borrows of many lifetimes   (foundations)
//   2. [x] Spell it out            — rewrite as explicit for<'a> Fn(&'a T)        (foundations)
//   3. [x] Caller-picks vs callee  — return a borrow from the closure's arg       (mechanics)
//   4. [x] HRTB on your own trait  — for<'a> MyTrait<'a>                           (mechanics)
//   5. [x] "not general enough"    — provoke + read the canonical HRTB error      (footgun)
//   6. [x] Named-lifetime trap     — one <'a> unified to caller scope rejects you (footgun)
//   7. [x] DecodeOwned             — for<'de> Deserialize<'de>, the real pattern  (real-world)
//   8. [x] HRTB trait objects      — Box<dyn for<'a> Fn(&'a str) -> &'a str>      (real-world)
//   9. [x] Parser combinator       — for<'i> Fn(&'i str) -> Option<(&'i str, T)>  (capstone)

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

// ── Rung 1: the implicit for<'a> ─────────────────────────────────────────────
// `apply_to_each` takes a slice and a callback. The callback's bound is
// `Fn(&str)` — note there is NO named lifetime here. That bound is implicitly
// `for<'a> Fn(&'a str)`: the closure must accept a `&str` of WHATEVER lifetime
// we hand it. Implement the function so it calls `f` once per element.
//
// Goal: make check_1 pass. The check feeds it borrows that only live for one
// loop iteration — proving the closure works for a lifetime it never named.
fn apply_to_each<F>(items: &[String], f: F)
where
    F: Fn(&str),
{
    for item in items {
        f(item);
    }
}

fn check_1() {
    use std::cell::RefCell;
    let words = vec!["red".to_string(), "green".to_string(), "blue".to_string()];
    let seen = RefCell::new(Vec::new());
    apply_to_each(&words, |w: &str| seen.borrow_mut().push(w.len()));

    assert_eq!(*seen.borrow(), vec![3, 5, 4]);
    println!("rung 1 ✅  implicit for<'a> — closure took borrows of a lifetime it never named");
}

// ── Rung 2: spell out the for<'a> ────────────────────────────────────────────
// Rung 1's bound `Fn(&str)` was sugar. Now write the SAME thing the long way,
// with the quantifier visible. Fill in the `where` clause so that F is bound by
// an *explicit* higher-ranked bound: F must implement `Fn(&'a str)` for EVERY
// lifetime 'a — i.e. `for<'a> Fn(&'a str)`.
//
// This rung is about the syntax, not new behavior: the body is the same as
// rung 1. The point is to see that `for<'a>` is a real place in the grammar
// (it goes right before the trait), and that the elided and explicit forms are
// interchangeable here.
//
// Your turn:
//   - replace the `where` clause below with the explicit HRTB form
//   - implement the body (call f on each item as &str)
fn apply_to_each_explicit<F>(items: &[String], f: F)
where
    F: for<'a> Fn(&'a str),
{
    for item in items {
        f(item);
    }
}

fn check_2() {
    use std::cell::RefCell;
    let words = vec!["one".to_string(), "two".to_string(), "three".to_string()];
    let seen = RefCell::new(Vec::new());
    apply_to_each_explicit(&words, |w: &str| seen.borrow_mut().push(w.len()));

    assert_eq!(*seen.borrow(), vec![3, 3, 5]);
    println!("rung 2 ✅  explicit for<'a> Fn(&'a str) — same bound, quantifier now visible");
}

// ── Rung 3: caller-picks vs callee-picks (why HRTB is load-bearing) ───────────
// NOW the closure RETURNS a borrow taken from its argument: `Fn(&str) -> &str`.
// The returned &str is tied to the *input's* lifetime.
//
// `measure_on_local` builds a String that lives ONLY inside this function, hands
// a borrow of it to `f`, and uses what `f` returns. Crucially, the lifetime of
// that local borrow is something the CALLER can never name — it doesn't exist at
// the call site. So the bound must be higher-ranked: `f` has to promise it works
// for EVERY input lifetime, including the callee's private one.
//
// Try this to feel the contrast (optional, will NOT compile — that's rung 6's
// lesson): change the bound to a named generic lifetime
//      fn measure_on_local<'a, F: Fn(&'a str) -> &'a str>(f: F) -> usize
// and watch it reject the local borrow, because the caller would have to pick a
// single 'a up front and no outer 'a can cover a string born inside the fn.
//
// The bound is given (note the `-> &'a str`: SAME 'a in and out — the output is
// tied to the input). Your turn is the BODY, and to convince yourself why this
// bound *must* be higher-ranked:
//   - build a local `String` containing "hello world"
//   - call `f` on a borrow of it
//   - return the length of whatever slice `f` gave back
fn measure_on_local<F>(f: F) -> usize
where
    F: for<'a> Fn(&'a str) -> &'a str,
{
    let s = String::from("hello world");
    let result = f(&s);
    result.len()
}

fn check_3() {
    // Closures are passed INLINE (not via `let`) so inference makes them
    // higher-ranked — the `let`-binding gotcha is rung 5.

    // returns the FIRST WORD of its input — a borrow into the argument.
    let n = measure_on_local(|s: &str| s.split(' ').next().unwrap_or(""));
    assert_eq!(n, 5); // "hello" out of "hello world"

    // returns the WHOLE input — borrow passes straight through.
    assert_eq!(measure_on_local(|s: &str| s), 11); // "hello world"

    println!("rung 3 ✅  for<'a> let f return a borrow of a local the caller can't even name");
}

// ── Rung 4: HRTB on YOUR OWN trait, not just Fn ──────────────────────────────
// `for<'a>` isn't special to `Fn` — it works on any trait that has a lifetime
// parameter. Here is your own lifetime-generic trait:
//
//   trait Slicer<'a> { fn slice(&self, input: &'a str) -> &'a str; }
//
// `Slicer<'a>` is really a whole FAMILY of traits, one per lifetime. A type that
// `impl<'a> Slicer<'a> for T` implements EVERY member of the family — which is
// exactly what `for<'a> Slicer<'a>` demands.
//
// Two tasks:
//   (a) finish the impl: `FirstWord::slice` returns the first whitespace-
//       separated word of `input` (a borrow back into `input`).
//   (b) fix the driver `run_slicer`: it builds a local String and calls
//       `.slice()` on it, so its bound must be higher-ranked. The placeholder
//       bound below compiles only while the body is `todo!()`; once you write
//       the real body it'll force you to make the bound `for<'a> Slicer<'a>`.
trait Slicer<'a> {
    fn slice(&self, input: &'a str) -> &'a str;
}

struct FirstWord;

impl<'a> Slicer<'a> for FirstWord {
    fn slice(&self, input: &'a str) -> &'a str {
        input.split(' ').next().unwrap_or("")
    }
}

fn run_slicer<S>(s: S) -> usize
where
    S: for<'a> Slicer<'a>,
{
    let word = String::from("green eggs");
    let result = s.slice(&word);
    result.len()
}

fn check_4() {
    assert_eq!(run_slicer(FirstWord), 5); // "green" out of "green eggs"
    println!("rung 4 ✅  for<'a> works on your own lifetime-generic trait, not just Fn");
}

// ── Rung 5: "implementation of Fn is not general enough" (the classic) ────────
// In rung 3 the closures worked only because they were passed INLINE. The moment
// you factor a reference-returning closure into a `let` binding, inference picks
// ONE concrete lifetime for it — so it no longer implements `for<'a>`, and the
// HRTB-bounded call below rejects it. This is THE error people fight with HRTB.
//
// `apply_str` is higher-ranked; it feeds `f` a borrow of a local string.
fn apply_str<F>(f: F) -> usize
where
    F: for<'a> Fn(&'a str) -> &'a str,
{
    let s = String::from("scaffold");
    f(&s).len()
}

fn check_5() {
    // STEP 1 — SEE THE ERROR. Temporarily paste these two lines in and run:

    //   You'll get "implementation of `Fn` is not general enough" (or "lifetime
    //   may not live long enough"). Meaning: `bad` was inferred for one fixed
    //   lifetime, but `apply_str` demands one closure that works for ALL of them.
    //   Delete those two lines again once you've read the error.
    //
    // STEP 2 — FIX IT. Make a `let`-bound callable `good` that genuinely is
    //   higher-ranked, and pass it. Pick ONE (ideally try both and see both pass):
    //     (i)  fn-pointer coercion:  let good: fn(&str) -> &str = |s| s;
    //     (ii) a real fn item:       fn id(s: &str) -> &str { s }   then  let good = id;
    //   (Why these work: fn pointers and fn items are inherently `for<'a>`; only
    //   *closures* get a single inferred lifetime that breaks HRTB.)
    //
    // Replace this todo with your STEP 2 code + the assert below.
    let good: fn(&str) -> &str = |s| s;

    assert_eq!(apply_str(good), "scaffold".len()); // "scaffold"
    // println!("rung 5 ✅  let-bound closures get ONE lifetime; fn ptr/fn item are for<'a>");
}

// ── Rung 6: the named-lifetime trap (one <'a> unified to caller scope) ────────
// Rung 5 was a CLOSURE that wasn't general enough. This is the dual: a BOUND
// that isn't general enough because YOU wrote a single named lifetime where you
// needed a higher-ranked one.
//
// `sum_two_locals` wants to call `f` on TWO different locals, each living in its
// own little scope. Note the signature below introduces a free lifetime `'a` and
// bounds `f: Fn(&'a str) -> &'a str`. That single `'a` is chosen by the CALLER,
// so it's one fixed lifetime that must outlive the whole function — and the two
// locals inside can't satisfy it.
//
// EXPERIMENT (do this, it's the whole lesson):
//   1. Fill the body: for each of two locals ("ab", "cdef"), call f and sum the
//      returned lengths. Keep the `<'a, F>` signature as-is and run.
//   2. Read the error — "borrowed value does not live long enough", pointing at
//      a local, because `'a` is fixed by the caller and outlives the fn body.
//   3. FIX: drop the named `'a` and make the bound higher-ranked instead:
//          fn sum_two_locals<F>(f: F) -> usize
//          where F: for<'a> Fn(&'a str) -> &'a str
//      Now the callee picks a FRESH, short 'a at each call — both locals fit.
fn sum_two_locals<F>(f: F) -> usize
where
    F: for<'a> Fn(&'a str) -> &'a str,
{
    let s1 = String::from("ab");
    let s2 = String::from("cdef");
    f(&s1).len() + f(&s2).len() // both calls get a fresh 'a
}

fn check_6() {
    let id: fn(&str) -> &str = |s| s;
    assert_eq!(sum_two_locals(id), 6); // 2 + 4
    println!("rung 6 ✅  a single named 'a is caller-chosen & fixed; for<'a> picks fresh per call");
}

// ── Rung 7: DecodeOwned = for<'de> Decode<'de> (the serde pattern) ────────────
// serde has two traits:
//   trait Deserialize<'de>            — may BORROW from the input (zero-copy)
//   trait DeserializeOwned            — owns all its data, borrows nothing
// and the second is literally defined as:
//   pub trait DeserializeOwned: for<'de> Deserialize<'de> {}
//   impl<T> DeserializeOwned for T where T: for<'de> Deserialize<'de> {}
// i.e. "DeserializeOwned = can be deserialized from input of ANY lifetime". A
// type that borrows from the input is Deserialize<'de> for ONE 'de tied to that
// input — so it is NOT DeserializeOwned. A type that owns its bytes implements
// Deserialize<'de> for EVERY 'de — so it IS. That's a higher-ranked bound doing
// real work in the most-used crate in the ecosystem.
//
// Below is the miniature. `Decode<'a>` is our `Deserialize<'de>`. Two impls are
// given to study:
//   - `Borrowed<'a>` keeps a &'a str  -> Decode<'a> only for the matching 'a
//   - `Owned`        keeps a u32      -> Decode<'a> for ALL 'a (you finish it)
//
// Tasks:
//   (a) finish `Owned::decode`: parse a comma-separated list of u32 and store
//       their SUM (e.g. "1,2,3,4" -> Owned { sum: 10 }). Return None if any
//       piece fails to parse.
//   (c) finish `load<T: DecodeOwned>`: it OWNS `source`, so the borrow it hands
//       to `decode` is local — only a DecodeOwned `T` can be loaded this way.
trait Decode<'a>: Sized {
    fn decode(input: &'a str) -> Option<Self>;
}

// DecodeOwned: the for<'a> "owns everything" marker, exactly like serde's.
trait DecodeOwned: for<'a> Decode<'a> {}
impl<T> DecodeOwned for T where T: for<'a> Decode<'a> {}

struct Borrowed<'a> {
    first: &'a str,
}

impl<'a> Decode<'a> for Borrowed<'a> {
    fn decode(input: &'a str) -> Option<Self> {
        input.split(',').next().map(|first| Self { first })
    }
}

struct Owned {
    sum: u32,
}

impl<'a> Decode<'a> for Owned {
    fn decode(input: &'a str) -> Option<Self> {
        let sum = input
            .split(',')
            .map(|part| part.parse::<u32>().ok())
            .sum::<Option<u32>>()?;
        Some(Self { sum })
    }
}

fn load<T: DecodeOwned>(source: String) -> Option<T> {
    let decoded = T::decode(&source);
    decoded.map(|d| d.into())
}

fn check_7() {
    // Owned IS DecodeOwned — load it from an owned String built right here.
    let got: Owned = load("1,2,3,4".to_string()).expect("should parse");
    assert_eq!(got.sum, 10);

    // Borrowed<'_> is NOT DecodeOwned (Self is welded to the input lifetime), so
    // this would fail "Borrowed<'_>: DecodeOwned is not satisfied". Uncomment to
    // confirm the higher-ranked bound is really what excludes it:
    //     let _: Borrowed = load("a,b".to_string()).unwrap();

    // Borrowed still works the NON-owned way: decode straight from a borrow.
    let b = Borrowed::decode("hello,world").unwrap();
    assert_eq!(b.first, "hello");

    println!("rung 7 ✅  DecodeOwned = for<'de> Decode<'de> — owners qualify, borrowers don't");
}

// ── Rung 8: HRTB inside a trait object — Box<dyn for<'a> Fn(&'a str)->&'a str> ─
// So far the higher-ranked thing was a *generic* parameter `F`. But you can also
// erase it behind a trait object. The field type below holds ANY reference-
// returning string transform. Crucially it is higher-ranked: `for<'a>` lives
// INSIDE the `dyn`. That's what lets `StrPipeline` itself carry NO lifetime
// parameter — one pipeline value can be applied to inputs of any lifetime.
// (As with bare `Fn`, you could write the field as `Box<dyn Fn(&str)->&str>` and
//  it would desugar to exactly this `for<'a>` form.)
//
// Contrast: if the box were `Box<dyn Fn(&'x str) -> &'x str>` for some fixed 'x,
// the struct would need a `<'x>` parameter and could only ever process borrows
// of that one lifetime. HRTB is what keeps the type lifetime-free.
//
// Your turn: implement `run`. Thread `input` through every step in order — each
// step takes the current slice and returns a sub-slice of it — and return the
// final slice. Note the signature: `run<'a>(&self, input: &'a str) -> &'a str`.
// Because every boxed step is `for<'a>`, calling one on a `&'a str` hands back a
// `&'a str`, so the borrow flows cleanly through the whole loop.
struct StrPipeline {
    steps: Vec<Box<dyn for<'a> Fn(&'a str) -> &'a str>>,
}

impl StrPipeline {
    fn new() -> Self {
        StrPipeline { steps: Vec::new() }
    }

    // Accept any higher-ranked transform and erase it into a boxed trait object.
    fn add<F>(mut self, f: F) -> Self
    where
        F: for<'a> Fn(&'a str) -> &'a str + 'static,
    {
        self.steps.push(Box::new(f));
        self
    }

    fn run<'a>(&self, input: &'a str) -> &'a str {
        let mut input = input;
        for step in &self.steps {
            input = step(input);
        }
        input
    }
}

fn check_8() {
    let pipeline = StrPipeline::new()
        .add(|s| s.trim()) // "  hello,world  " -> "hello,world"
        .add(|s| s.split(',').next().unwrap_or("")); // -> "hello"

    let out = pipeline.run("  hello,world  ");
    assert_eq!(out, "hello");

    // same pipeline, a totally different (shorter-lived) input — proof the
    // for<'a> inside the dyn lets one lifetime-free pipeline serve any input.
    let local = String::from(" a,b,c ");
    assert_eq!(pipeline.run(&local), "a");

    println!("rung 8 ✅  for<'a> inside dyn keeps the trait object lifetime-free");
}

// ── Rung 9 (capstone): a parser combinator powered by for<'i> ─────────────────
// A parser is a function: given input, either fail, or return (remaining_input,
// value). The remaining slice is a sub-borrow of the input, so a parser is
// fundamentally `for<'i> Fn(&'i str) -> Option<(&'i str, T)>`. HRTB is the load-
// bearing wall of EVERY parser-combinator library (nom, winnow, chumsky): it's
// what lets `Parser<T>` be a lifetime-free type you can store, pass, and compose,
// while each parser still works on input of any lifetime — and lets one parser's
// leftover slice (lifetime 'i) feed straight into the next parser (same 'i).
//
// GIVEN (study these): the `Parser<T>` newtype, `new`/`parse`, and `tag`.
// `tag("x=")` matches a literal prefix and yields it. Notice the closure inside
// is higher-ranked, and T = &'static str never borrows from the input lifetime.
//
// YOUR TURN — implement three combinators so check_9 passes:
//   (a) number()         -> Parser<u64>          parse leading ASCII digits
//   (b) map(p, f)        -> Parser<B>            transform a parser's output
//   (c) then(a, b)       -> Parser<(A, B)>       run a, then b on what's LEFT
//
// In `then`, watch the lifetimes: `a.parse(input)` gives `(rest, va)` where
// `rest: &'i str`. You then call `b.parse(rest)` — only because `b` is `for<'i>`
// can it accept that leftover slice of the very same 'i. THAT composition is the
// whole point of the rung.
struct Parser<T>(Box<dyn for<'i> Fn(&'i str) -> Option<(&'i str, T)>>);

impl<T: 'static> Parser<T> {
    fn new(f: impl for<'i> Fn(&'i str) -> Option<(&'i str, T)> + 'static) -> Self {
        Parser(Box::new(f))
    }

    fn parse<'i>(&self, input: &'i str) -> Option<(&'i str, T)> {
        (self.0)(input)
    }
}

// GIVEN: literal-prefix parser. The model for the ones you'll write.
fn tag(prefix: &'static str) -> Parser<&'static str> {
    Parser::new(move |input: &str| input.strip_prefix(prefix).map(|rest| (rest, prefix)))
}

// (a) parse one-or-more leading ASCII digits into a u64; None if there are none.
fn number() -> Parser<u64> {
    Parser::new(move |input: &str| {
        let index = input
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(input.len());
        let (n, rest) = input.split_at(index);
        if n.is_empty() {
            None
        } else {
            Some((rest, n.parse::<u64>().unwrap()))
        }
    })
}

// (b) run `p`, then apply `f` to its value (input/remaining untouched).
fn map<A: 'static, B: 'static, F>(p: Parser<A>, f: F) -> Parser<B>
where
    F: Fn(A) -> B + 'static,
{
    Parser::new(move |input| p.parse(input).map(|(rest, a)| (rest, f(a))))
}

// (c) run `a`; on success run `b` on the LEFTOVER; pair up the two values.
fn then<A: 'static, B: 'static>(a: Parser<A>, b: Parser<B>) -> Parser<(A, B)> {
    Parser::new(move |input| {
        a.parse(input)
            .and_then(|(rest, a)| b.parse(rest).map(|(rest, b)| (rest, (a, b))))
    })
}

fn check_9() {
    // number on its own
    assert_eq!(number().parse("42!").unwrap(), ("!", 42));
    assert!(number().parse("nope").is_none());

    // map: double the parsed number
    let doubled = map(number(), |n| n * 2);
    assert_eq!(doubled.parse("21").unwrap().1, 42);

    // then: a literal followed by a number — "x=42" with trailing junk
    let assignment = then(tag("x="), number());
    let (rest, (key, value)) = assignment.parse("x=42;").unwrap();
    assert_eq!(key, "x=");
    assert_eq!(value, 42);
    assert_eq!(rest, ";");

    // full compose: map a (tag, number) pair down to just the number, +1
    let incremented = map(then(tag("n:"), number()), |(_, n)| n + 1);
    assert_eq!(incremented.parse("n:99").unwrap().1, 100);

    println!("rung 9 ✅  CAPSTONE — parser combinators stand on for<'i>; you built the core");
}
