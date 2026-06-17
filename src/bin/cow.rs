// Concept: Cow (Clone-on-Write) — std::borrow::Cow
//
// All problems for this concept live in THIS file. Each problem is one function
// plus a `check_N` that asserts it. `main` runs them in order and stops at the
// first unimplemented one (todo! panic).
//
// Run with: cargo run --bin cow
//
// Ladder:
//   1. borrow-or-own basics        (DONE)
//   2. prove the no-alloc path     (DONE)
//   3. Cow in a struct             (DONE)
//   4. .to_mut() & Cow<[T]>        (DONE)
//   5. the defining lifetime error (footgun: can't borrow a local)
//   6. Deref + PartialEq ergonomics
//   7. Vec<Cow<str>> + cheap clones of borrowed cows
//   8. serde zero-copy with #[serde(borrow)]
//   9. capstone: re-implement a mini Cow from scratch

use serde::Deserialize;
use std::borrow::Cow;

// ---------------------------------------------------------------------------
// Problem 1 — borrow-or-own basics  ✅ solved
// Return borrowed when already correct, owned only when we must build it.
// ---------------------------------------------------------------------------
fn ensure_https(url: &str) -> Cow<'_, str> {
    if url.starts_with("https://") {
        Cow::Borrowed(url)
    } else {
        Cow::Owned(format!("https://{}", url))
    }
}

fn check_1() {
    let already = ensure_https("https://example.com");
    let fixed = ensure_https("example.com");
    assert_eq!(already.as_ref(), "https://example.com");
    assert_eq!(fixed.as_ref(), "https://example.com");
    assert!(matches!(already, Cow::Borrowed(_)));
    assert!(matches!(fixed, Cow::Owned(_)));
    println!("✅ problem 1: borrowed when possible, owned only when needed");
}

// ---------------------------------------------------------------------------
// Problem 2 — prove the no-alloc path
//
// Task: replace every space ' ' with '_'. If the input has NO spaces, return it
// BORROWED without allocating. The trap: `input.replace(' ', "_")` always
// returns a fresh String even when nothing changed — decide if work is needed
// BEFORE you allocate.
//
// Hints to recall: &str has `.contains(...)`. `.replace(' ', "_")` is fine on
// the dirty path only.
// ---------------------------------------------------------------------------
fn sanitize(input: &str) -> Cow<'_, str> {
    if input.contains(' ') {
        Cow::Owned(input.replace(' ', "_"))
    } else {
        Cow::Borrowed(input)
    }
}

fn check_2() {
    let clean = sanitize("already_clean");
    let dirty = sanitize("hello world foo");
    assert_eq!(clean.as_ref(), "already_clean");
    assert_eq!(dirty.as_ref(), "hello_world_foo");
    assert!(
        matches!(clean, Cow::Borrowed(_)),
        "clean input must stay Borrowed — you should not have allocated"
    );
    assert!(matches!(dirty, Cow::Owned(_)), "dirty input must be Owned");
    println!("✅ problem 2: only allocated when there was real work to do");
}

// ---------------------------------------------------------------------------
// Problem 3 — Cow in a struct
//
// Now Cow is a FIELD, not a return value. A Config can be built two ways:
//   - from a &'static str literal -> stored Borrowed (no allocation)
//   - from a runtime-built String  -> stored Owned
// Both end up as the SAME type, Config<'a>, and `name()` reads it uniformly.
//
// Task: implement the two constructors and the `name` getter.
//
// Hints to recall:
//   - `Cow::from(&str)` gives Borrowed; `Cow::from(String)` gives Owned.
//     (Or construct the variants directly, like before.)
//   - The field type is `Cow<'a, str>`. `name(&self) -> &str` can just deref
//     the cow (`&self.name` coerces, or use `.as_ref()`).
// ---------------------------------------------------------------------------
struct Config<'a> {
    name: Cow<'a, str>,
}

impl<'a> Config<'a> {
    // Borrow a string we don't own (e.g. a literal). No allocation.
    fn borrowed(name: &'a str) -> Self {
        Self {
            name: Cow::Borrowed(name),
        }
    }

    // Take ownership of a String built at runtime.
    fn owned(name: String) -> Self {
        Self {
            name: Cow::Owned(name),
        }
    }

    fn name(&self) -> &str {
        self.name.as_ref()
    }
}

fn check_3() {
    let from_literal = Config::borrowed("default");

    let runtime = format!("user-{}", 42);
    let from_string = Config::owned(runtime);

    assert_eq!(from_literal.name(), "default");
    assert_eq!(from_string.name(), "user-42");

    assert!(
        matches!(from_literal.name, Cow::Borrowed(_)),
        "literal config should be Borrowed"
    );
    assert!(
        matches!(from_string.name, Cow::Owned(_)),
        "runtime config should be Owned"
    );
    println!("✅ problem 3: one struct type holds either a borrow or an owned value");
}

// ---------------------------------------------------------------------------
// Problem 4 — .to_mut() & Cow<[T]>  (the finale)
//
// This is the real "clone on WRITE". Cow isn't string-only — here it's a slice
// of i32. You start BORROWED and only pay for an owned Vec the moment you
// actually mutate, via `.to_mut()`.
//
// Task: implement `clamp_negatives`. Given a borrowed &[i32]:
//   - if every value is already >= 0, return the input BORROWED (no Vec alloc).
//   - if any value is negative, produce an Owned copy with negatives set to 0.
//
// The lesson — `.to_mut()`:
//   `cow.to_mut()` returns a `&mut Vec<i32>`. If the cow is currently Borrowed,
//   to_mut() CLONES it into Owned first, then gives you the mutable ref. If it's
//   already Owned, it just hands back the ref (no extra clone). So calling
//   to_mut() exactly when you hit the first negative gives you lazy allocation.
//
// One valid shape:
//   let mut cow: Cow<[i32]> = Cow::Borrowed(input);
//   for i in 0..input.len() {
//       if input[i] < 0 {
//           cow.to_mut()[i] = 0;   // first negative upgrades Borrowed -> Owned
//       }
//   }
//   cow
//
// Try it yourself before peeking at that. Type to recall: Cow<'_, [i32]>.
// ---------------------------------------------------------------------------
fn clamp_negatives(input: &[i32]) -> Cow<'_, [i32]> {
    let mut cow: Cow<[i32]> = Cow::Borrowed(input);
    for i in 0..input.len() {
        if input[i] < 0 {
            cow.to_mut()[i] = 0;
        }
    }
    cow
}

fn check_4() {
    let all_ok = [1, 2, 3];
    let has_neg = [1, -2, 3, -4];

    let a = clamp_negatives(&all_ok);
    let b = clamp_negatives(&has_neg);

    assert_eq!(a.as_ref(), &[1, 2, 3]);
    assert_eq!(b.as_ref(), &[1, 0, 3, 0]);

    assert!(
        matches!(a, Cow::Borrowed(_)),
        "no negatives -> should stay Borrowed (no Vec alloc)"
    );
    assert!(
        matches!(b, Cow::Owned(_)),
        "had negatives -> to_mut() should have upgraded to Owned"
    );
    println!("✅ problem 4: .to_mut() upgraded Borrowed -> Owned only on first write");
}

// ---------------------------------------------------------------------------
// Problem 5 — the defining lifetime error (footgun tier)
//
// This rung is about a borrow you CANNOT make. First, the experiment:
//
//   Temporarily write this and run it — it MUST fail to compile:
//       fn broken(name: &str) -> Cow<'_, str> {
//           let local = format!("hi {name}");
//           Cow::Borrowed(&local)   // <- borrows a value that dies at fn end
//       }
//   Read the error ("cannot return value referencing local variable `local`").
//   That's the whole point of this rung: a Cow::Borrowed ties its lifetime to
//   data that must OUTLIVE the call. A String built inside the function does
//   not — so you literally cannot hand it back borrowed.
//   (Delete `broken` again afterward so the file compiles.)
//
// Now the actual task: implement `greeting` correctly. It builds a fresh
// "hi <name>" string, so it has no choice but to return Owned. But if `name`
// is already exactly "hi there", return THAT borrowed (no alloc).
//
// Lesson to internalize: Borrowed = "I'm pointing at someone else's data that
// lives long enough". Owned = "I made this myself". You can only borrow inputs
// (or longer-lived data), never locals.
// ---------------------------------------------------------------------------
fn greeting(name: &str) -> Cow<'_, str> {
    if name == "hi there" {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(format!("hi {}", name))
    }
}

fn check_5() {
    let pre = greeting("hi there");
    let built = greeting("sam");
    assert_eq!(pre.as_ref(), "hi there");
    assert_eq!(built.as_ref(), "hi sam");
    assert!(
        matches!(pre, Cow::Borrowed(_)),
        "already-greeting input should stay Borrowed"
    );
    assert!(
        matches!(built, Cow::Owned(_)),
        "built greeting must be Owned — you can't borrow a local"
    );
    println!("✅ problem 5: Borrowed ties to outliving data; locals must be Owned");
}

// ---------------------------------------------------------------------------
// Problem 6 — Deref ergonomics
//
// Why is Cow nice to USE, not just to return? Because Cow<str> implements
// Deref<Target = str>. So you can call ANY &str method straight on a Cow — no
// matches!, no .as_ref(), no caring whether it's Borrowed or Owned.
//
// Task: implement `first_word`. Given &Cow<str>, return the first
// whitespace-separated word as a &str, by calling str methods DIRECTLY on the
// cow (Deref does the rest).
//
// Hints to recall:
//   - `.split_whitespace()` and `.next()` are str methods; you can call them on
//     `c` directly thanks to Deref. `.unwrap_or("")` for the empty case.
//   - Do NOT match the variant. The point is that you don't have to.
//
// (Aside worth trying: `&**c == "hello"` compares the underlying str; Deref is
// what makes `c.len()`, `c.starts_with(..)`, etc. all Just Work too.)
// ---------------------------------------------------------------------------
fn first_word<'a>(c: &'a Cow<'_, str>) -> &'a str {
    c.split_whitespace().next().unwrap_or("")
}

fn check_6() {
    let borrowed: Cow<str> = Cow::Borrowed("hello world foo");
    let owned: Cow<str> = Cow::Owned(String::from("alpha beta"));
    let empty: Cow<str> = Cow::Borrowed("");

    assert_eq!(first_word(&borrowed), "hello");
    assert_eq!(first_word(&owned), "alpha");
    assert_eq!(first_word(&empty), "");
    println!("✅ problem 6: Deref lets you treat any Cow<str> as a &str");
}

// ---------------------------------------------------------------------------
// Problem 7 — Vec<Cow<str>>: borrow most, own a few
//
// Real-world pattern: normalize a list of words to lowercase, but only ALLOCATE
// for the words that actually had uppercase letters. Already-lowercase words
// stay Borrowed (pointing into the original input), so a batch that's mostly
// clean costs almost nothing.
//
// Task: implement `normalize`. Given &'a [&'a str], return Vec<Cow<'a, str>>:
//   - word already all-lowercase -> Cow::Borrowed(word)   (no alloc)
//   - word has uppercase         -> Cow::Owned(word.to_lowercase())
//
// Hints to recall:
//   - `str::chars().any(|c| c.is_uppercase())` detects if work is needed.
//   - Build the Vec with a loop, or `.iter().map(...).collect()`.
//   - This is just problem 2's "inspect before allocating", now per-element.
// ---------------------------------------------------------------------------
fn normalize<'a>(words: &'a [&'a str]) -> Vec<Cow<'a, str>> {
    let mut vec: Vec<Cow<'a, str>> = Vec::new();
    for word in words {
        if word.chars().any(|c| c.is_uppercase()) {
            vec.push(Cow::Owned(word.to_lowercase()));
        } else {
            vec.push(Cow::Borrowed(word));
        }
    }
    vec
}

fn check_7() {
    let input = ["hello", "World", "rust", "COW"];
    let out = normalize(&input);

    let texts: Vec<&str> = out.iter().map(|c| c.as_ref()).collect();
    assert_eq!(texts, ["hello", "world", "rust", "cow"]);

    // clean words borrowed, dirty words owned — counted across the batch
    let borrowed = out.iter().filter(|c| matches!(c, Cow::Borrowed(_))).count();
    let owned = out.iter().filter(|c| matches!(c, Cow::Owned(_))).count();
    assert_eq!(
        borrowed, 2,
        "hello & rust were already lowercase -> Borrowed"
    );
    assert_eq!(owned, 2, "World & COW needed lowercasing -> Owned");
    println!("✅ problem 7: a batch borrows the clean entries, allocates only the dirty ones");
}

// ---------------------------------------------------------------------------
// Problem 8 — serde zero-copy with #[serde(borrow)]
//
// THE real-world payoff. Deserialize a JSON object {"text": "..."} into a
// struct whose field is Cow<'a, str>. With #[serde(borrow)], serde points the
// field straight into the input buffer (Borrowed, zero copy) when the string
// has no escapes. If it DOES contain escapes (\n, \", ...), serde must decode
// it into a fresh String (Owned). One field, both outcomes — that's why Cow.
//
// Task (two parts):
//   A. Add `#[serde(borrow)]` to the `text` field below. (Without it, serde
//      defaults to always-Owned — try removing it later and watch the Borrowed
//      assert fail. That contrast IS the lesson.)
//   B. Implement `parse_text`: deserialize `json` into Msg and return its text.
//
// Hints to recall:
//   - `serde_json::from_str::<Msg>(json)` borrows from `json` (note the
//     `Msg<'a>` lifetime ties to the input). `.unwrap()` is fine here.
//   - Return `msg.text`. The signature already ties the output to `json`.
// ---------------------------------------------------------------------------
#[derive(Deserialize)]
struct Msg<'a> {
    #[serde(borrow)]
    text: Cow<'a, str>,
}

fn parse_text(json: &str) -> Cow<'_, str> {
    let msg: Msg = serde_json::from_str(json).unwrap();
    msg.text
}

fn check_8() {
    let plain = parse_text(r#"{"text":"hello world"}"#);
    let escaped = parse_text(r#"{"text":"line1\nline2"}"#);

    assert_eq!(plain.as_ref(), "hello world");
    assert_eq!(escaped.as_ref(), "line1\nline2");

    assert!(
        matches!(plain, Cow::Borrowed(_)),
        "no escapes -> serde borrows straight from the JSON buffer (zero copy)"
    );
    assert!(
        matches!(escaped, Cow::Owned(_)),
        "escapes -> serde must decode into a new String"
    );
    println!(
        "✅ problem 8: #[serde(borrow)] borrows from the input, owns only when it must decode"
    );
}

// ---------------------------------------------------------------------------
// Problem 9 — CAPSTONE: re-implement Cow from scratch
//
// You've used std's Cow. Now build the machinery yourself so you KNOW how the
// variant gets constructed and how to_mut() upgrades. We specialize to str to
// keep it concrete (real Cow is generic over B: ?Sized + ToOwned).
//
// MyCow is the enum below. Implement its three methods:
//
//   as_str(&self) -> &str
//     Return the inner string slice regardless of variant.
//     - Borrowed(s) -> *s        (it's already a &str)
//     - Owned(s)    -> s.as_str()
//
//   to_mut(&mut self) -> &mut String
//     The heart of clone-on-write:
//     - If currently Owned(s): return `s` (no clone).
//     - If currently Borrowed(s): clone it into a String, REPLACE self with
//       Owned(that String), then return &mut to the new String.
//       (Tip: `*self = MyCow::Owned(s.to_string());` then match again, or use
//        the same trick std uses — reassign self, then return the owned ref.)
//
//   into_owned(self) -> String
//     Consume self and produce an owned String either way.
//     - Borrowed(s) -> s.to_string()
//     - Owned(s)    -> s
//
// This is the whole concept distilled: two states, cheap to read via as_str,
// and to_mut() is the exact point where a borrow becomes an allocation.
// ---------------------------------------------------------------------------
enum MyCow<'a> {
    Borrowed(&'a str),
    Owned(String),
}

impl<'a> MyCow<'a> {
    fn as_str(&self) -> &str {
        match self {
            Self::Borrowed(s) => *s,
            Self::Owned(s) => s.as_str(),
        }
    }

    fn to_mut(&mut self) -> &mut String {
        match self {
            Self::Borrowed(s) => {
                *self = Self::Owned(s.to_string());
                match self {
                    Self::Owned(s) => s,
                    _ => unreachable!(),
                }
            }
            Self::Owned(s) => s,
        }
    }

    fn into_owned(self) -> String {
        match self {
            Self::Borrowed(s) => s.to_string(),
            Self::Owned(s) => s,
        }
    }
}

fn check_9() {
    // as_str works on both variants
    let b = MyCow::Borrowed("hi");
    let o = MyCow::Owned(String::from("yo"));
    assert_eq!(b.as_str(), "hi");
    assert_eq!(o.as_str(), "yo");

    // to_mut on a Borrowed upgrades it to Owned, then lets us mutate
    let mut c = MyCow::Borrowed("ab");
    c.to_mut().push('c');
    assert_eq!(c.as_str(), "abc");
    assert!(
        matches!(c, MyCow::Owned(_)),
        "to_mut must have upgraded Borrowed -> Owned"
    );

    // to_mut on an already-Owned just hands back the ref (still Owned)
    let mut d = MyCow::Owned(String::from("x"));
    d.to_mut().push('y');
    assert_eq!(d.as_str(), "xy");
    assert!(matches!(d, MyCow::Owned(_)));

    // into_owned produces a String from either variant
    assert_eq!(MyCow::Borrowed("z").into_owned(), "z");
    assert_eq!(MyCow::Owned(String::from("w")).into_owned(), "w");

    println!("✅ problem 9: built Cow from scratch — you own the borrow/own/upgrade model");
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
