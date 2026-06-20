// Newtype & zero-cost wrappers
// Run: cargo run --bin newtype
//
// A newtype is a one-field tuple struct wrapping an existing type. At runtime
// it's nothing (same bits, same size); to the type checker it's a brand-new
// type. You pay the compiler in types to buy back guarantees the raw type
// can't give: distinct identity, your own trait impls, enforced invariants.
//
// Ladder (DONE marks finished rungs):
//   1. Distinct identity            — Meters vs Seconds, compiler rejects mixing   [DONE]
//   2. Deriving the basics          — UserId, derive Debug/Copy/Eq/Ord, access .0  [DONE]
//   3. Type-safe arithmetic         — impl Add for Meters; m+s won't compile       [DONE]
//   4. Deref for ergonomics         — wrap String, deref to str, free methods      [DONE]
//   5. The Deref leak (footgun)     — SortedVec derefs to Vec, .push breaks it      [DONE]
//   6. Orphan-rule escape hatch     — impl Display for a foreign type via newtype  [DONE]
//   7. repr(transparent) & zero cost— prove layout identical via size_of           [DONE]
//   8. Parse don't validate         — Email private field + smart constructor      [DONE]
//   9. Capstone: phantom Id<T>       — Id<User> != Id<Post>, zero-cost, HashMap key [DONE]

// ---------------------------------------------------------------------------
// Rung 1 — Distinct identity
//
// Define two newtypes over f64: Meters and Seconds. Implement `speed`, which
// takes a distance in Meters and a time in Seconds and returns meters/second
// as a plain f64. The point: the function signature must make it IMPOSSIBLE
// to call speed(seconds, meters) by accident — swapping the args is a type
// error, even though both wrap f64.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Meters(f64);

#[derive(Debug, Clone, Copy)]
struct Seconds(f64);

fn speed(distance: Meters, time: Seconds) -> f64 {
    distance.0 / time.0
}

fn check_1() {
    let d = Meters(100.0);
    let t = Seconds(9.58);
    let s = speed(d, t);
    assert!((s - 10.438).abs() < 0.01, "expected ~10.438 m/s, got {s}");

    // The real test is at compile time: uncomment the next line and it must
    // FAIL to compile (mismatched types Seconds vs Meters). Re-comment it after
    // you've seen the error.
    // let _ = speed(t, d);

    println!("check_1 ok: {s:.3} m/s");
}

// ---------------------------------------------------------------------------
// Rung 2 — Deriving the basics
//
// A newtype starts with NO behavior — it doesn't even print, compare, or copy
// until you say so. Make `UserId` a comfortable value type by deriving the
// usual suspects, then prove each capability in check_2:
//   - Debug    (so {:?} works)
//   - Clone + Copy (so it's pass-by-value like the u64 inside)
//   - PartialEq + Eq (so == works)
//   - PartialOrd + Ord (so it sorts)
//
// Add the right derive(...) attribute above the struct. Then implement
// `max_id`: given a &[UserId], return the largest one. The body is the
// exercise — reach for the slice's own iterator/Ord machinery.
// ---------------------------------------------------------------------------

// TODO: add the derive attribute here
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct UserId(u64);

fn max_id(ids: &[UserId]) -> UserId {
    ids.iter().copied().max().unwrap()
}

fn check_2() {
    let a = UserId(7);
    let b = UserId(7);
    assert_eq!(a, b, "two UserId(7) should be =="); // needs PartialEq
    assert!(UserId(3) < UserId(9), "ordering by inner u64"); // needs Ord
    let copy = a; // needs Copy...
    assert_eq!(a.0, 7, "a still usable after `let copy = a`"); // ...a not moved
    assert_eq!(copy.0, 7);

    let ids = [UserId(4), UserId(42), UserId(8)];
    let biggest = max_id(&ids);
    assert_eq!(biggest, UserId(42), "max should be 42, got {biggest:?}"); // needs Debug+Eq

    println!("check_2 ok: biggest = {biggest:?}");
}

// ---------------------------------------------------------------------------
// Rung 3 — Type-safe arithmetic
//
// Derives gave us ==, <, etc. — but `+` is not derivable. To add two Meters
// you implement the std::ops::Add trait yourself. The payoff: you decide the
// algebra. Meters + Meters = Meters makes sense; Meters + Seconds does NOT,
// and because you only impl Add for (Meters, Meters), the compiler refuses it.
//
// 1. impl Add for Meters so that Meters(a) + Meters(b) == Meters(a + b).
//    (std::ops::Add has an associated `type Output` and a method `add`.)
// 2. Implement `total`, summing a &[Meters] into one Meters, starting from
//    Meters(0.0). Use your own `+`.
//
// Then uncomment the "won't compile" line in check_3 to watch Meters + Seconds
// get rejected, and re-comment it.
// ---------------------------------------------------------------------------

use std::ops::Add;

impl Add for Meters {
    type Output = Meters;
    fn add(self, rhs: Meters) -> Meters {
        Meters(self.0 + rhs.0)
    }
}

fn total(distances: &[Meters]) -> Meters {
    distances
        .iter()
        .copied()
        .fold(Meters(0.0), |acc, d| acc + d)
}

fn check_3() {
    let a = Meters(3.0) + Meters(4.0);
    assert_eq!(a.0, 7.0, "3m + 4m should be 7m, got {}", a.0);

    let legs = [Meters(1.5), Meters(2.5), Meters(6.0)];
    let sum = total(&legs);
    assert_eq!(sum.0, 10.0, "total should be 10m, got {}", sum.0);

    // Type algebra you DEFINED: adding Seconds to Meters is meaningless, and
    // the compiler enforces that. Uncomment to see E0277, then re-comment.
    // let _ = Meters(1.0) + Seconds(1.0);

    println!("check_3 ok: total = {}m", sum.0);
}

// ---------------------------------------------------------------------------
// Rung 4 — Deref for ergonomics
//
// So far the inner value is hidden behind `.0`. Sometimes you WANT the wrapper
// to behave like the thing it wraps. Implementing Deref lets `&Wrapper` coerce
// to `&Target`, so all of the target's methods (and &-taking fns) work on the
// wrapper directly — no `.0` needed. This is "deref coercion".
//
// `Username` wraps a String. Implement Deref with Target = str so that:
//   - username.len()         works (str::len via &String -> &str)
//   - username.to_uppercase()works
//   - a &Username can be passed where &str is expected (greet)
//
// Fill in the Deref impl. (Hint: a &String already derefs to &str, so the body
// is a one-liner. `type Target = str;`)
// ---------------------------------------------------------------------------

use std::ops::Deref;

struct Username(String);

impl Deref for Username {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}

fn check_4() {
    let u = Username(String::from("ferris"));

    // Method resolution walks the Deref chain: Username -> str.
    assert_eq!(u.len(), 6, "len() should reach str::len");
    assert_eq!(&*u.to_uppercase(), "FERRIS");

    // Deref COERCION: &Username is accepted where &str is wanted.
    assert_eq!(greet(&u), "Hello, ferris!");

    println!("check_4 ok: {}", greet(&u));
}

// ---------------------------------------------------------------------------
// Rung 5 — The Deref leak (footgun)
//
// Deref is a sharp tool. If you `impl Deref<Target = Vec<i32>>` for a wrapper
// whose whole JOB is to keep the Vec sorted, you ALSO hand the caller every
// &self method on Vec... but NOT &mut ones unless you add DerefMut. The subtle
// trap: some Vec methods take &self yet still let you observe/leak internals,
// and if you reach for DerefMut you leak `.push`, `.swap`, etc. — any of which
// silently breaks "sorted". Rule of thumb: Deref is for smart POINTERS, not for
// "inherit the API". A newtype with an invariant should expose a CURATED API,
// not Deref to its inner collection.
//
// SortedVec keeps its inner Vec<i32> sorted ascending. Your job:
//   1. Implement `insert` so the vec stays sorted after every insert.
//      (Find the insertion point, then Vec::insert. `partition_point` or
//       `binary_search` both work.)
//   2. Implement `as_slice` -> &[i32] : a CURATED read-only view. This is the
//      RIGHT way to expose inner data — NOT a blanket Deref<Target=Vec> that
//      would also surface `.push` and let callers wreck the invariant.
//
// (We deliberately do NOT impl Deref/DerefMut here — that's the lesson.)
// ---------------------------------------------------------------------------

struct SortedVec(Vec<i32>);

impl SortedVec {
    fn new() -> Self {
        SortedVec(Vec::new())
    }

    fn insert(&mut self, value: i32) {
        self.0.insert(self.0.partition_point(|&x| x < value), value);
    }

    fn as_slice(&self) -> &[i32] {
        &self.0
    }
}

fn check_5() {
    let mut sv = SortedVec::new();
    for v in [5, 1, 4, 1, 3, 2] {
        sv.insert(v);
    }
    assert_eq!(sv.as_slice(), &[1, 1, 2, 3, 4, 5], "must stay sorted");

    // The point of the rung: because there's NO Deref to Vec, this line will
    // NOT compile — `push` doesn't exist on SortedVec. Uncomment to confirm the
    // invariant is actually protected, then re-comment.
    // sv.push(0);

    println!("check_5 ok: {:?}", sv.as_slice());
}

// ---------------------------------------------------------------------------
// Rung 6 — Orphan-rule escape hatch
//
// The orphan rule: you can only impl a trait for a type if the trait OR the
// type is local to your crate. So you CANNOT `impl Display for Vec<i32>` —
// both Display and Vec are foreign. The classic fix is a newtype: wrap the
// foreign type in YOUR struct, and now the type is local, so the impl is legal.
//
// This is how crates add formatting/serde/etc. to types they don't own.
//
// 1. PrettyVec wraps a Vec<i32>.
// 2. impl Display for PrettyVec so it prints like "[1, 2, 3]" (comma+space
//    separated, square brackets). Reach into self.0 to iterate.
//
// (Optional, to FEEL the rule: try writing `impl fmt::Display for Vec<i32>`
//  at module scope and watch E0117 — then delete it.)
// ---------------------------------------------------------------------------

use std::fmt;

struct PrettyVec(Vec<i32>);

impl fmt::Display for PrettyVec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for (i, v) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{v}")?;
        }
        write!(f, "]")
    }
}

fn check_6() {
    let pv = PrettyVec(vec![1, 2, 3]);
    assert_eq!(pv.to_string(), "[1, 2, 3]", "got {:?}", pv.to_string());

    let empty = PrettyVec(vec![]);
    assert_eq!(empty.to_string(), "[]", "empty got {:?}", empty.to_string());

    println!("check_6 ok: {pv}");
}

// ---------------------------------------------------------------------------
// Rung 7 — repr(transparent) & the zero-cost proof
//
// "Zero-cost" isn't a slogan — a newtype over T has the SAME size and alignment
// as T, and the wrapper compiles away. `#[repr(transparent)]` makes that a
// GUARANTEED, ABI-stable fact (not just an optimization): the newtype is laid
// out exactly like its single non-zero-sized field. This is what lets you, e.g.,
// safely transmute a &T to a &Newtype, or pass a newtype across an FFI boundary
// where the C side expects the raw T.
//
// Wrap a u64 in a transparent newtype `Wrapping64`. Then:
//   1. Add the `#[repr(transparent)]` attribute.
//   2. Implement `as_raw_slice`: given &[Wrapping64], return &[u64] WITHOUT
//      copying — reinterpret the slice in place. Because the layout is
//      guaranteed identical, this is a sound transmute of the pointer.
//      Fill in the SAFETY comment with WHY it's sound.
// ---------------------------------------------------------------------------

use std::mem::{align_of, size_of};

// TODO: add the repr attribute
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(transparent)]
struct Wrapping64(u64);

fn as_raw_slice(xs: &[Wrapping64]) -> &[u64] {
    // SAFETY: The layout of Wrapping64 is guaranteed to be identical to the layout of u64.
    unsafe {
        let ptr = xs.as_ptr();
        let len = xs.len();
        std::slice::from_raw_parts(ptr as *const u64, len)
    }
}

fn check_7() {
    // Zero-cost: identical size & alignment to the bare u64.
    assert_eq!(
        size_of::<Wrapping64>(),
        size_of::<u64>(),
        "size must match u64"
    );
    assert_eq!(
        align_of::<Wrapping64>(),
        align_of::<u64>(),
        "align must match u64"
    );

    let wrapped = [Wrapping64(10), Wrapping64(20), Wrapping64(30)];
    let raw = as_raw_slice(&wrapped);
    assert_eq!(
        raw,
        &[10u64, 20, 30],
        "reinterpreted slice must read the same bytes"
    );
    assert_eq!(raw.len(), 3);

    println!(
        "check_7 ok: {} bytes, raw = {raw:?}",
        size_of::<Wrapping64>()
    );
}

// ---------------------------------------------------------------------------
// Rung 8 — Parse, don't validate (the validated newtype)
//
// The most powerful newtype move: make the TYPE itself a proof that an
// invariant holds. Put the inner data behind a PRIVATE field and offer NO public
// constructor — only a fallible smart constructor that checks the invariant
// ONCE, at the boundary. After that, every `Email` value in the program is
// guaranteed valid, so downstream code never re-checks. ("Parse, don't
// validate": turn unstructured input into a type that can't represent the
// invalid state.)
//
// Note the module: `Email`'s field is private to `mod email`, so code OUTSIDE
// the module literally cannot write `Email(whatever)` — the only way in is
// `Email::parse`. That privacy is what makes the guarantee airtight.
//
// In `mod email`, implement:
//   1. `parse(s: &str) -> Result<Email, EmailError>`:
//        - Err(EmailError::Empty) if s is empty
//        - Err(EmailError::MissingAt) if s has no '@'
//        - else Ok(Email(s.to_string()))
//      (A real one checks much more; the point is the PATTERN, not the regex.)
//   2. `as_str(&self) -> &str` to read it back.
// ---------------------------------------------------------------------------

mod email {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Email(String); // private field: only this module can construct one

    #[derive(Debug, PartialEq, Eq)]
    pub enum EmailError {
        Empty,
        MissingAt,
    }

    impl Email {
        pub fn parse(s: &str) -> Result<Email, EmailError> {
            if s.is_empty() {
                return Err(EmailError::Empty);
            }
            if !s.contains('@') {
                return Err(EmailError::MissingAt);
            }
            Ok(Email(s.to_string()))
        }

        pub fn as_str(&self) -> &str {
            &self.0
        }
    }
}

fn check_8() {
    use email::{Email, EmailError};

    assert_eq!(Email::parse(""), Err(EmailError::Empty));
    assert_eq!(Email::parse("ferris"), Err(EmailError::MissingAt));

    let e = Email::parse("ferris@rust-lang.org").expect("should be valid");
    assert_eq!(e.as_str(), "ferris@rust-lang.org");

    // The guarantee in action: a function that takes `Email` needs NO
    // validation — the type already proves the address parsed.
    fn send_to(addr: &Email) -> String {
        format!("sending to {}", addr.as_str())
    }
    assert_eq!(send_to(&e), "sending to ferris@rust-lang.org");

    println!("check_8 ok: {}", e.as_str());
}

// ---------------------------------------------------------------------------
// Rung 9 — CAPSTONE: the phantom-typed Id<T>
//
// The real-world pattern that ties it all together. A DB layer hands out
// numeric ids for every table. Plain `u64` ids are a bug factory: nothing stops
// you passing a user's id where a post's id is expected. You COULD write a
// separate newtype per table (UserId, PostId, ...) — but that's boilerplate.
//
// Instead: ONE generic newtype `Id<T>` carrying a u64, where the type parameter
// T is a PHANTOM "tag" that exists only at compile time. `Id<User>` and
// `Id<Post>` are then DISTINCT types that cannot be mixed — yet at runtime each
// is still just a u64 (zero cost). PhantomData<T> is the zero-sized marker that
// lets a struct be "generic over T" without actually storing a T.
//
// Tag types carry no data; they're just names: `struct User;` `struct Post;`.
//
// Your tasks:
//   1. Define `Id<T>(u64, PhantomData<T>)`.
//   2. `Id::new(raw: u64) -> Id<T>` and `get(&self) -> u64`.
//   3. Make Id<T> usable as a HashMap key and comparable: it must be
//      Copy + Clone + PartialEq + Eq + Hash + Debug — for EVERY T, even Ts that
//      aren't themselves Copy/Eq/etc. (a tag like `User` has no data, but the
//      derive macros don't know that). Derives add a `T: Trait` bound you do NOT
//      want. Decide: can `#[derive(...)]` work here, or must you hand-write the
//      impls with the bound on PhantomData<T> only? (See the hint when you hit
//      it — this is THE subtle part of the rung.)
//
// check_9 stores Id<User> keys in a HashMap and proves Id<User> != Id<Post>
// won't even type-check.
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::marker::PhantomData;

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
struct User;

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
struct Post;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Id<T> {
    raw: u64,
    _tag: PhantomData<T>,
}

impl<T> Id<T> {
    fn new(raw: u64) -> Id<T> {
        Id {
            raw,
            _tag: PhantomData,
        }
    }

    fn get(&self) -> u64 {
        self.raw
    }
}

fn check_9() {
    let u1: Id<User> = Id::new(1);
    let u2: Id<User> = Id::new(1);
    let u3: Id<User> = Id::new(2);
    let p1: Id<Post> = Id::new(1);

    assert_eq!(u1, u2, "same tag + same raw => equal");
    assert_ne!(u1, u3, "same tag, different raw => not equal");
    assert_eq!(u1.get(), 1);

    // Copy: using u1 after a copy still works.
    let copy = u1;
    assert_eq!(u1.get(), copy.get());

    // HashMap keyed by Id<User>.
    let mut names: HashMap<Id<User>, &str> = HashMap::new();
    names.insert(u1, "alice");
    names.insert(u3, "bob");
    assert_eq!(names.get(&u2), Some(&"alice")); // u2 == u1, finds alice
    assert_eq!(names.len(), 2);

    // The capstone guarantee: Id<User> and Id<Post> are different types.
    // `p1` exists only to prove that. Uncomment to watch it fail to compile,
    // then re-comment:
    // assert_eq!(u1, p1);
    let _ = p1.get(); // keep p1 used

    println!("check_9 ok: phantom-typed ids, {} keys", names.len());
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
