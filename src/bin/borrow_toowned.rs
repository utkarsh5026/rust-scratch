// Concept: Borrow / ToOwned — std::borrow::{Borrow, ToOwned}
//
// The two traits that sit UNDERNEATH Cow and HashMap-key lookups.
//   - ToOwned: generalized Clone. &Borrowed -> Owned  (&str -> String).
//   - Borrow<B>: view an Owned value as a borrowed &B, with a CONTRACT that the
//     view hashes/compares/orders identically to the owner.
//
// All problems for this concept live in THIS file. Each problem is one function
// plus a `check_N` that asserts it. `main` runs them in order and stops at the
// first unimplemented one (todo! panic).
//
// Run with: cargo run --bin borrow_toowned
//
// Ladder:
//   1. ToOwned basics: &str -> String, &[i32] -> Vec        (foundations)
//   2. Borrow basics: &str out of a String                  (foundations)
//   3. HashMap<String,_>::get(&str) & generic lookup        (mechanics)
//   4. <T as ToOwned>::Owned associated type                (mechanics)
//   5. Borrow vs AsRef: the consistency contract            (footgun)
//   6. the lookup footgun: needless .to_string() alloc      (footgun)
//   7. accept-anything-stringy: impl Borrow<str>            (real-world)
//   8. close the Cow loop: T: ToOwned -> Cow<'_, T>         (real-world)
//   9. capstone: re-implement MyBorrow + MyToOwned          (capstone)

use std::borrow::Borrow;
use std::hash::Hash;

// ---------------------------------------------------------------------------
// Problem 1 — ToOwned basics  (foundations)
//
// `ToOwned` is `Clone` for the case where the borrowed and owned types DIFFER.
// `str` cannot impl `Clone` to produce a `String` (Clone is &T -> T, same type),
// so `str: ToOwned<Owned = String>` exists to bridge that gap. Same for
// `[T]: ToOwned<Owned = Vec<T>>`.
//
// Task: implement `duplicate` so that given a &str it returns an owned String
// holding the same text, using ToOwned (NOT String::from / .to_string()).
// Then implement `duplicate_slice`: given a &[i32], return an owned Vec<i32>
// via ToOwned.
//
// Goal: feel that `.to_owned()` is the ToOwned trait method, and that the
// OWNED type it produces is a *different* type than the borrowed input.
// ---------------------------------------------------------------------------
fn duplicate(s: &str) -> String {
    s.to_owned()
}

fn duplicate_slice(xs: &[i32]) -> Vec<i32> {
    xs.to_owned()
}

fn check_1() {
    let owned: String = duplicate("hello");
    assert_eq!(owned, "hello");

    let v: Vec<i32> = duplicate_slice(&[1, 2, 3]);
    assert_eq!(v, vec![1, 2, 3]);

    println!("✅ problem 1: ToOwned bridges &str->String and &[T]->Vec<T>");
}

// ---------------------------------------------------------------------------
// Problem 2 — Borrow basics  (foundations)
//
// `Borrow<B>` is the other direction: given an OWNED value, hand out a borrowed
// `&B` view of it. `String: Borrow<str>` and `Vec<T>: Borrow<[T]>` (and the
// blanket `T: Borrow<T>`, so &String -> &String works too).
//
// The trait method is `.borrow()` — but it's ambiguous which `B` you want, so
// you usually pin the target type with an annotation or a function signature.
//
// Task A: implement `first_word`. It takes anything that can be Borrow'd as a
// `str` — but for THIS rung keep the param concrete: take `s: &String` and use
// `Borrow<str>` to get a `&str`, then return the first whitespace-delimited word
// as an owned String (reuse ToOwned from rung 1!). Do the &str conversion via
// the Borrow trait, not `.as_str()` / `&s[..]`.
//
// Task B: implement `borrow_sum`, generic over `T: Borrow<[i32]>`. Borrow the
// value as a &[i32] and sum it. This should then accept BOTH a `Vec<i32>` and
// a `&[i32]` at the call site (see check_2).
//
// Goal: see Borrow as "view owned as borrowed", and that being generic over
// Borrow<[i32]> lets ONE function take owned Vec or a slice.
// ---------------------------------------------------------------------------
fn first_word(s: &String) -> String {
    let view: &str = s.borrow();
    view.split_whitespace().next().unwrap_or("").to_owned()
}

fn borrow_sum<T: Borrow<[i32]>>(xs: T) -> i32 {
    let slice = xs.borrow();
    slice.iter().sum()
}

fn check_2() {
    let sentence = String::from("hello brave world");
    assert_eq!(first_word(&sentence), "hello");

    let owned_vec: Vec<i32> = vec![1, 2, 3, 4];
    assert_eq!(borrow_sum(owned_vec), 10);

    let slice: &[i32] = &[10, 20, 30];
    assert_eq!(borrow_sum(slice), 60);

    println!("✅ problem 2: Borrow views owned values as &str / &[T]");
}

// ---------------------------------------------------------------------------
// Problem 3 — the payoff: HashMap<String, _>::get(&str)  (mechanics)
//
// THE reason Borrow exists. A HashMap<String, V> stores OWNED String keys, but
// you want to look up with a cheap &str without allocating a String. The std
// signature is:
//
//     fn get<Q>(&self, k: &Q) -> Option<&V>
//     where K: Borrow<Q>, Q: Hash + Eq + ?Sized
//
// Read that bound: "the stored key K can be Borrow'd as Q". With K = String,
// Q = str, `String: Borrow<str>` holds, so `map.get("key")` works. The contract
// (Borrow guarantees String and str hash/compare identically) is what makes
// this SOUND — the hash you compute from &str finds the bucket the String went
// into.
//
// Task A: implement `count_word` — return how many times `word` appears, reading
// from a &HashMap<String, u32>, looking up by &str (do NOT build a String to
// look up). Return 0 if absent.
//
// Task B: write your OWN generic lookup `contains_key2`, mirroring std's bound:
//     fn contains_key2<K, Q>(map: &HashMap<K, u32>, key: &Q) -> bool
// with the right `where` clauses so it compiles AND check_3 can call it with a
// HashMap<String,_> and a &str key. You'll need: K: Borrow<Q> + Hash + Eq, and
// Q: Hash + Eq + ?Sized.
//
// Goal: be able to READ and WRITE the `K: Borrow<Q>` bound and explain why it's
// there — it's the single most important real-world use of this trait.
// ---------------------------------------------------------------------------
use std::collections::HashMap;

fn count_word(counts: &HashMap<String, u32>, word: &str) -> u32 {
    counts.get(word).copied().unwrap_or(0)
}

// Task B: UNCOMMENT this function and fill in the `where` clause so the body
// `map.contains_key(key)` compiles. Then uncomment the two contains_key2 asserts
// in check_3 below.
//
// Hint: copy std's bound shape. You need K: Borrow<Q> + Hash + Eq and
// Q: Hash + Eq + ?Sized. Why `?Sized` on Q? Because Q = str is unsized, and you
// only ever touch it behind a reference (&Q), so unsized is fine.
//
fn contains_key2<K, Q>(map: &HashMap<K, u32>, key: &Q) -> bool
where
    K: Borrow<Q> + Eq + Hash,
    Q: Eq + Hash + ?Sized,
{
    map.contains_key(key)
}

fn check_3() {
    let mut counts: HashMap<String, u32> = HashMap::new();
    counts.insert("apple".to_string(), 3);
    counts.insert("pear".to_string(), 1);

    assert_eq!(count_word(&counts, "apple"), 3);
    assert_eq!(count_word(&counts, "kiwi"), 0);

    assert!(contains_key2(&counts, "pear"));
    assert!(!contains_key2(&counts, "kiwi"));

    println!("✅ problem 3: K: Borrow<Q> lets a String-keyed map be queried by &str");
}

// ---------------------------------------------------------------------------
// Problem 4 — the <T as ToOwned>::Owned associated type  (mechanics)
//
// `ToOwned` isn't `fn to_owned(&self) -> Self` — the output type is an
// ASSOCIATED type, `type Owned`, so str -> String and [T] -> Vec<T> can each
// pick their own owned form:
//
//     pub trait ToOwned {
//         type Owned: Borrow<Self>;
//         fn to_owned(&self) -> Self::Owned;
//     }
//
// When you're generic over `T: ToOwned`, the owned value's type is spelled
// `<T as ToOwned>::Owned` (or just `T::Owned`). You can't say `T` — `T` is the
// BORROWED type (e.g. `str`), which is usually unsized and can't be returned by
// value.
//
// Task A: implement `owned_pair<T>` — take `&T` where `T: ToOwned`, and return a
// tuple of TWO independently-owned copies: `(T::Owned, T::Owned)`. One line.
//
// Task B: implement `owned_or<T>` — take `value: &T` and a bool `take_it`. If
// `take_it`, return the owned form of `value`; otherwise return the owned form
// of `fallback: &T`. Return type is `T::Owned`. Note `T: ?Sized` is needed since
// you'll call it with `T = str` (unsized) behind references.
//
// Goal: name and use the associated `Owned` type by hand; understand WHY the
// return type can't just be `T`.
// ---------------------------------------------------------------------------
fn owned_pair<T: ToOwned + ?Sized>(value: &T) -> (T::Owned, T::Owned) {
    (value.to_owned(), value.to_owned())
}

fn owned_or<T: ToOwned + ?Sized>(value: &T, fallback: &T, take_it: bool) -> T::Owned {
    if take_it {
        value.to_owned()
    } else {
        fallback.to_owned()
    }
}

fn check_4() {
    let (a, b): (String, String) = owned_pair("hi");
    assert_eq!(a, "hi");
    assert_eq!(b, "hi");

    let (v1, v2): (Vec<i32>, Vec<i32>) = owned_pair(&[1, 2][..]);
    assert_eq!(v1, vec![1, 2]);
    assert_eq!(v2, vec![1, 2]);

    let chosen: String = owned_or("yes", "no", true);
    assert_eq!(chosen, "yes");
    let chosen2: String = owned_or("yes", "no", false);
    assert_eq!(chosen2, "no");

    println!("✅ problem 4: T::Owned names the associated owned type");
}

// ---------------------------------------------------------------------------
// Problem 5 — Borrow vs AsRef: the consistency CONTRACT  (footgun)
//
// `Borrow<T>` and `AsRef<T>` have the SAME shape: `fn(&self) -> &T`. So why two
// traits? The difference is a PROMISE, not a signature:
//
//   - AsRef<T>: "you can view me as a &T." No other guarantee. Use it for
//     flexible function arguments (accept &str | String | PathBuf | ...).
//   - Borrow<T>: viewing must be SEMANTICALLY TRANSPARENT — `x` and `x.borrow()`
//     must produce the SAME Eq / Ord / Hash. This is what HashMap relies on to
//     find the right bucket. Implement it ONLY when that holds.
//
// Below is `CiString`: a string whose equality & hashing are case-INSENSITIVE
// (provided for you — the Eq/Hash impls are the whole point). Because its
// equivalence relation differs from plain `str`'s, a `Borrow<str>` impl for it
// would be UNSOUND: `map.get("HELLO")` would hash "HELLO" with str's exact
// hasher, landing in a different bucket than the case-insensitive key went into
// — a silent miss. So `CiString` deliberately does NOT impl Borrow<str>.
//
// Task A: implement `AsRef<str> for CiString` (legal — AsRef makes no promise).
// Task B: implement `find_ci` — case-insensitive lookup into a CiString-keyed
// map, given a plain &str. Since there's no Borrow<str>, you MUST build an owned
// CiString to query with. Feel that cost: it's the honest price when the
// equivalence relations don't match.
//
// Goal: state the Borrow contract, and decide Borrow-vs-AsRef correctly.
// ---------------------------------------------------------------------------
use std::hash::Hasher;

#[derive(Clone)]
struct CiString(String);

impl CiString {
    fn new(s: &str) -> Self {
        CiString(s.to_owned())
    }
}

impl PartialEq for CiString {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for CiString {}

impl Hash for CiString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // hash the lowercased form, so "Hello" and "HELLO" hash identically
        for b in self.0.bytes() {
            state.write_u8(b.to_ascii_lowercase());
        }
    }
}

// Task A — your turn:
impl AsRef<str> for CiString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

fn find_ci(map: &HashMap<CiString, i32>, query: &str) -> Option<i32> {
    let key = CiString::new(query);
    map.get(&key).copied()
}

fn check_5() {
    let mut m: HashMap<CiString, i32> = HashMap::new();
    m.insert(CiString::new("Hello"), 1);
    m.insert(CiString::new("World"), 2);

    // case-insensitive lookup works because we query with a CiString key
    assert_eq!(find_ci(&m, "HELLO"), Some(1));
    assert_eq!(find_ci(&m, "world"), Some(2));
    assert_eq!(find_ci(&m, "nope"), None);

    // AsRef gives a plain &str view (no contract attached)
    assert_eq!(CiString::new("hi there").as_ref(), "hi there");

    // --- the proof that Borrow<str> would be UNSOUND for CiString ---
    use std::collections::hash_map::DefaultHasher;
    fn h<T: Hash + ?Sized>(t: &T) -> u64 {
        let mut s = DefaultHasher::new();
        t.hash(&mut s);
        s.finish()
    }
    // CiString hashes case-insensitively: "Hello" == "HELLO"
    assert_eq!(h(&CiString::new("Hello")), h(&CiString::new("HELLO")));
    // but plain str hashes exactly: "Hello" != "HELLO"
    assert_ne!(h("Hello"), h("HELLO"));
    // => a Borrow<str> impl (forcing str-hashing on lookup) would miss the
    //    bucket. That mismatch is exactly what Borrow's contract forbids.

    println!("✅ problem 5: Borrow needs Eq/Hash transparency; AsRef does not");
}

// ---------------------------------------------------------------------------
// Problem 6 — the lookup footgun: needless .to_string() allocation  (footgun)
//
// Rung 5 was "the relations DON'T match, so you must allocate." This is the
// opposite and far more common bug: the relations DO match (String/str), Borrow
// is right there — and people allocate anyway, for nothing.
//
// The classic wasteful method, which you should NOT write:
//
//     fn get_bad(&self, key: String) -> Option<&str>   // forces caller to OWN
//     fn get_bad2(&self, key: &str) -> Option<&str> {
//         self.0.get(&key.to_string())...               // allocates per lookup!
//     }
//
// Both throw away the whole point of Borrow. The fix is one generic method that
// accepts a borrowed key directly.
//
// Task A: implement `Cache::get`, generic so a caller can pass EITHER a &str or
// a &String with zero allocation, mirroring HashMap::get's bound:
//     fn get<Q>(&self, key: &Q) -> Option<&str>
//     where String: Borrow<Q>, Q: Hash + Eq + ?Sized
// Body: look up in self.0, map the &String value to &str.
//
// Task B: implement `any_known` — does a HashSet<String> contain ANY of the
// given &str candidates? Query with the &str directly via `set.contains(..)`,
// no String built. (Same Borrow magic, now on a set.)
//
// Goal: reflexively reach for `key: &Q where Container-Key: Borrow<Q>` instead
// of taking/owning a String at lookup boundaries.
// ---------------------------------------------------------------------------
use std::collections::HashSet;

struct Cache(HashMap<String, String>);

impl Cache {
    fn get<Q>(&self, key: &Q) -> Option<&str>
    where
        // your turn: bounds that let `self.0.get(key)` work for &str AND &String
        String: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.0.get(key).map(|v| v.as_str())
    }
}

fn any_known(set: &HashSet<String>, candidates: &[&str]) -> bool {
    candidates.iter().any(|c| set.contains(*c))
}

fn check_6() {
    let mut c = Cache(HashMap::new());
    c.0.insert("host".to_string(), "localhost".to_string());
    c.0.insert("port".to_string(), "8080".to_string());

    // pass a &str literal — no String built
    assert_eq!(c.get("host"), Some("localhost"));
    // pass a &String — also fine, same method
    let k = String::from("port");
    assert_eq!(c.get(&k), Some("8080"));
    assert_eq!(c.get("missing"), None);

    let known: HashSet<String> = ["alice", "bob"].iter().map(|s| s.to_string()).collect();
    assert!(any_known(&known, &["carol", "bob"]));
    assert!(!any_known(&known, &["carol", "dave"]));

    println!("✅ problem 6: borrow the lookup key — don't allocate to query");
}

// ---------------------------------------------------------------------------
// Problem 7 — real-world API design: own at insert, borrow at query  (real-world)
//
// The senior-Rustacean pattern for a keyed collection has TWO distinct
// boundaries, each wanting a different trait:
//
//   - INSERT boundary: you must end up OWNING the key. Accept `impl Into<String>`
//     (or ToOwned) so callers can hand you a &str OR a String and you take
//     ownership with at most one allocation.
//   - QUERY boundary: you only need to LOOK, so borrow. Accept `&Q where
//     String: Borrow<Q>` so callers can probe with a &str and never allocate.
//
// That Into-in / Borrow-out split is exactly how real APIs (e.g. a tag set, a
// header map, an interner) are shaped.
//
// Task A: implement `shout`, generic over `S: Borrow<str>`, returning the
// uppercased text. The payoff is breadth: ONE signature accepts &str, String,
// Box<str>, Rc<str>, AND Cow<str> (see check_7). Borrow `s` as &str, uppercase.
//
// Task B: finish `TagSet`:
//   - `add<S: Into<String>>(&mut self, tag: S)` — own the tag, insert it.
//   - `has<Q>(&self, tag: &Q) -> bool where String: Borrow<Q>, Q: Hash+Eq+?Sized`
//     — borrow to probe; no allocation.
//
// Goal: reach for Into/ToOwned at ownership boundaries and Borrow at lookup
// boundaries — the real-world division of labor between these traits.
// ---------------------------------------------------------------------------
use std::borrow::Cow;
use std::rc::Rc;

fn shout<S: Borrow<str>>(s: S) -> String {
    let view = s.borrow();
    view.to_uppercase()
}

#[derive(Default)]
struct TagSet {
    tags: HashSet<String>,
}

impl TagSet {
    fn add<S: Into<String>>(&mut self, tag: S) {
        self.tags.insert(tag.into());
    }

    fn has<Q>(&self, tag: &Q) -> bool
    where
        String: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.tags.contains(tag)
    }
}

fn check_7() {
    assert_eq!(shout("hi"), "HI"); // &str
    assert_eq!(shout(String::from("yo")), "YO"); // String
    assert_eq!(shout(Box::<str>::from("be")), "BE"); // Box<str>
    assert_eq!(shout(Rc::<str>::from("rc")), "RC"); // Rc<str>
    assert_eq!(shout(Cow::Borrowed("cow")), "COW"); // Cow<str>

    let mut tags = TagSet::default();
    tags.add("rust"); // &str at insert
    tags.add(String::from("async")); // String at insert
    assert!(tags.has("rust")); // &str at query, no alloc
    assert!(tags.has("async"));
    assert!(!tags.has("python"));

    println!("✅ problem 7: Into at the own boundary, Borrow at the query boundary");
}

// ---------------------------------------------------------------------------
// Problem 8 — close the Cow loop: ToOwned IS Cow's reason to exist  (real-world)
//
// Recall Cow's actual definition:
//
//     pub enum Cow<'a, B: ToOwned + ?Sized> {
//         Borrowed(&'a B),
//         Owned(<B as ToOwned>::Owned),
//     }
//
// NOW you can read every piece: the `B: ToOwned` bound is MANDATORY because the
// Owned variant must name a concrete owned type, `<B as ToOwned>::Owned`, and
// `to_owned()` is the only way to manufacture one from a borrow. That's the
// answer to "why does Cow require B: ToOwned?" — without ToOwned, Cow couldn't
// name its owned half nor build it on demand.
//
// Task A: re-implement `Cow::into_owned` yourself as `make_owned`, generic over
// any `B: ToOwned + ?Sized`. Match the two variants: a Borrowed must be
// `to_owned()`'d; an Owned is already there. Return `B::Owned`. This is the
// whole machine in four lines.
//
// Task B: implement `pick`, a generic Cow PRODUCER:
//     fn pick<'a, B>(borrow_it: bool, b: &'a B, owned: B::Owned) -> Cow<'a, B>
//     where B: ToOwned + ?Sized
// If borrow_it, build the Borrowed variant from `b`; else the Owned variant from
// `owned`. One function, works for B = str AND B = [i32] (see check_8).
//
// Goal: be able to explain, from the bound up, exactly why Cow<B> needs
// B: ToOwned — the loop from the very first ladder is now closed.
// ---------------------------------------------------------------------------
fn make_owned<B: ToOwned + ?Sized>(c: Cow<'_, B>) -> B::Owned {
    match c {
        Cow::Borrowed(b) => b.to_owned(),
        Cow::Owned(o) => o,
    }
}

fn pick<'a, B>(borrow_it: bool, b: &'a B, owned: B::Owned) -> Cow<'a, B>
where
    B: ToOwned + ?Sized,
{
    if borrow_it {
        Cow::Borrowed(b)
    } else {
        Cow::Owned(owned)
    }
}

fn check_8() {
    // make_owned == Cow::into_owned, for str and for [i32]
    let s: String = make_owned(Cow::Borrowed("hi"));
    assert_eq!(s, "hi");
    let s2: String = make_owned(Cow::<str>::Owned("yo".to_string()));
    assert_eq!(s2, "yo");
    let v: Vec<i32> = make_owned(Cow::Borrowed(&[1, 2, 3][..]));
    assert_eq!(v, vec![1, 2, 3]);

    // pick builds either variant, generically
    let borrowed: Cow<str> = pick(true, "shared", "fresh".to_string());
    assert!(matches!(borrowed, Cow::Borrowed(_)));
    assert_eq!(borrowed, "shared");

    let owned: Cow<[i32]> = pick(false, &[0][..], vec![9, 9]);
    assert!(matches!(owned, Cow::Owned(_)));
    assert_eq!(owned.as_ref(), &[9, 9]);

    println!("✅ problem 8: Cow<B> needs B: ToOwned — loop closed");
}

// ---------------------------------------------------------------------------
// Problem 9 — CAPSTONE: re-implement the whole machine from scratch  (capstone)
//
// Build your own MyBorrow + MyToOwned + MyCow, mirroring std. The shape (traits
// + associated type + the MyBorrow<Self> bound) is given — you wire every body.
//
// The key structural insight, now that you've earned it:
//   - `MyBorrow<Borrowed: ?Sized>` is generic over the borrowed type, so a type
//     can borrow as several things (Vec<T> -> [T], and reflexively -> Vec<T>).
//   - `MyToOwned::Owned` carries a `MyBorrow<Self>` bound: the owned type must be
//     able to borrow BACK to Self. That round-trip guarantee is what makes
//     MyCow::borrow() able to return `&B` from the Owned variant.
//   - `Self: ?Sized` is the default in trait defs, which is why `impl MyToOwned
//     for str` (an unsized type) is even legal.
//
// Tasks — fill every todo!():
//   A. `MyBorrow::my_borrow` for String->str and Vec<T>->[T].
//   B. `MyToOwned::my_to_owned` for str->String and [T]->Vec<T>.
//   C. `MyCow::into_owned` (consume -> Owned) and `MyCow::borrow` (-> &B).
//
// Goal: prove you own the mental model end to end — the borrowed<->owned
// round-trip, the associated type, and why Cow is built on exactly these two
// traits.
// ---------------------------------------------------------------------------
trait MyBorrow<Borrowed: ?Sized> {
    fn my_borrow(&self) -> &Borrowed;
}

trait MyToOwned {
    type Owned: MyBorrow<Self>;
    fn my_to_owned(&self) -> Self::Owned;
}

// --- A: borrow owned values back down to the borrowed view ---
impl MyBorrow<str> for String {
    fn my_borrow(&self) -> &str {
        &self.as_str()
    }
}

impl<T> MyBorrow<[T]> for Vec<T> {
    fn my_borrow(&self) -> &[T] {
        &self.as_slice()
    }
}

// --- B: manufacture an owned value from a borrowed one ---
impl MyToOwned for str {
    type Owned = String;
    fn my_to_owned(&self) -> String {
        self.to_string()
    }
}

impl<T: Clone> MyToOwned for [T] {
    type Owned = Vec<T>;
    fn my_to_owned(&self) -> Vec<T> {
        self.to_vec()
    }
}

// --- the container, generic over your own ToOwned ---
enum MyCow<'a, B: MyToOwned + ?Sized> {
    Borrowed(&'a B),
    Owned(B::Owned),
}

impl<'a, B: MyToOwned + ?Sized> MyCow<'a, B> {
    // C: consume self, always yielding an owned value
    fn into_owned(self) -> B::Owned {
        match self {
            Self::Borrowed(b) => b.my_to_owned(),
            Self::Owned(o) => o,
        }
    }

    // C: borrow a &B view regardless of which variant we hold.
    // Hint: for the Owned arm, the MyBorrow<Self> bound on B::Owned is what lets
    // you call .my_borrow() to get the &B back.
    fn borrow(&self) -> &B {
        match self {
            Self::Borrowed(b) => b,
            Self::Owned(o) => o.my_borrow(),
        }
    }
}

fn check_9() {
    // str / String round-trip
    let cb: MyCow<str> = MyCow::Borrowed("hi");
    assert_eq!(cb.borrow(), "hi");
    assert_eq!(cb.into_owned(), "hi".to_string());

    let co: MyCow<str> = MyCow::Owned("yo".to_string());
    assert_eq!(co.borrow(), "yo");
    assert_eq!(co.into_owned(), "yo".to_string());

    // [T] / Vec<T> round-trip
    let vb: MyCow<[i32]> = MyCow::Borrowed(&[1, 2, 3][..]);
    assert_eq!(vb.borrow(), &[1, 2, 3]);
    assert_eq!(vb.into_owned(), vec![1, 2, 3]);

    let vo: MyCow<[i32]> = MyCow::Owned(vec![9, 9]);
    assert_eq!(vo.borrow(), &[9, 9]);
    assert_eq!(vo.into_owned(), vec![9, 9]);

    println!("✅ problem 9: hand-rolled MyBorrow + MyToOwned + MyCow — mastery");
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
    println!("\n🎉 all unlocked problems pass");
}
