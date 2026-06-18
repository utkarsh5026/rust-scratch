// Concept: Lifetimes in depth — elision, 'a: 'b bounds, lifetimes in structs/impls
// Run: cargo run --bin lifetimes_depth
//
// Mental model: a lifetime label like 'a does NOT create or extend anything. It
// is a NAME you attach to references so the compiler can check ONE rule:
//   "a reference must never outlive the data it points to."
// `<'a>` on a function is a generic parameter — but over a *lifetime*, not a
// type. It describes how the input borrows and the output borrow are connected,
// so the compiler can prove the result is still valid.
//
// Ladder (DONE marks finished rungs):
//   1. longest        - annotate a fn returning one of two &str                 [DONE]
//   2. elision        - fns that need no annotation; the 3 elision rules        [DONE]
//   3. excerpt_struct - a struct that holds a &str reference                    [DONE]
//   4. impl_methods   - impl<'a>, and the &self-vs-param return gotcha          [DONE]
//   5. dangling       - make the borrow checker reject a bad borrow, then fix   [DONE]
//   6. outlives       - a multi-lifetime fn needing an 'a: 'b bound             [DONE]
//   7. announce       - lifetimes + generics + trait bounds; where 'static fits [DONE]
//   8. words_iter     - implement Iterator yielding &str borrowed from a struct [DONE]
//   9. str_split      - zero-copy split iterator over a borrowed string         [DONE] <-- capstone

// ── Rung 1: the classic — annotate `longest` ─────────────────────────────────
// Return whichever of `a` / `b` is the longer string (by .len()).
//
// As written, this WILL NOT COMPILE — that missing-lifetime error IS the lesson.
// The compiler can't tell whether the returned &str borrows from `a` or from
// `b`, so it can't know how long the result is valid. Your job: introduce a
// lifetime parameter and annotate the signature so both inputs and the output
// share it, then fill in the body.
fn longest<'a>(a: &'a str, b: &'a str) -> &'a str {
    if a.len() > b.len() { a } else { b }
}

fn check_1() {
    let s1 = String::from("a long string");
    let result;
    {
        let s2 = String::from("short");
        result = longest(&s1, &s2);
        assert_eq!(result, "a long string");
        println!("rung 1 ok: longest = {:?}", result);
    }
}

// ── Rung 2: elision — when the compiler fills lifetimes in for you ────────────
// The compiler applies 3 rules, in order, to elide (omit) lifetimes:
//   Rule 1: each elided INPUT reference gets its OWN fresh lifetime.
//   Rule 2: if there's exactly ONE input lifetime, it's assigned to ALL outputs.
//   Rule 3: if one of the inputs is &self / &mut self, ITS lifetime goes to all
//           outputs (this is why methods rarely need annotations).
// If after these rules an output lifetime is still unknown -> hard error.
//
// Part A — Rules 1 + 2 in action. ONE input ref, so the output is unambiguous.
// Implement `first_word` with NO lifetime annotations at all (prove it compiles):
// return the slice of `s` up to the first ' ', or all of `s` if there is none.
fn first_word(s: &str) -> &str {
    let first_space = s.find(' ').unwrap_or(s.len());
    &s[..first_space]
}

// Part B — where elision RUNS OUT. Two input refs => rule 2 doesn't apply and
// there's no &self for rule 3, so the compiler can't guess which one the output
// borrows from. As written this WON'T COMPILE (missing lifetime specifier).
// Fix the signature yourself — but here's the real lesson: only `text` flows to
// the output, so only `text` needs the named lifetime tied to the return type.
// Leave `marker` with its own (elided) lifetime. Then return the part of `text`
// BEFORE the first occurrence of `marker` (or all of `text` if not found).
fn prefix_before<'a>(text: &'a str, marker: &str) -> &'a str {
    let first_occurrence = text.find(marker).unwrap_or(text.len());
    &text[..first_occurrence]
}

fn check_2() {
    assert_eq!(first_word("hello world"), "hello");
    assert_eq!(first_word("solo"), "solo");
    assert_eq!(prefix_before("key=value", "="), "key");
    assert_eq!(prefix_before("no-marker-here", "="), "no-marker-here");
    println!("rung 2 ok: first_word & prefix_before behave");
}

// ── Rung 3: lifetimes in struct definitions ──────────────────────────────────
// Every struct that HOLDS a reference must declare a lifetime parameter, and the
// field's reference is tagged with it. This lets the compiler enforce a brand
// new rule: an `Excerpt` value may NEVER outlive the `str` it borrows from.
//
// As written this WON'T COMPILE (the field is a borrowed value with no declared
// lifetime). Add a lifetime parameter to the struct and tag the field with it.
struct Excerpt<'a> {
    part: &'a str,
}

// Build an Excerpt that borrows the first sentence of `text` (everything up to
// AND including the first '.'), or the whole `text` if there's no '.'.
// Note the return type `Excerpt<'_>`: the '_ means "an inferred lifetime" — by
// elision it's tied to `text`. (You could also write it out as Excerpt<'a>.)
fn first_sentence(text: &str) -> Excerpt<'_> {
    let first_dot = text.find('.').unwrap_or(text.len());
    let offset = if first_dot == text.len() { 0 } else { 1 };
    Excerpt {
        part: &text[..first_dot + offset],
    }
}

fn check_3() {
    let novel = String::from("Call me Ishmael. Some years ago never mind how long.");
    let e = first_sentence(&novel);
    assert_eq!(e.part, "Call me Ishmael.");

    let no_dot = String::from("just a fragment");
    assert_eq!(first_sentence(&no_dot).part, "just a fragment");
    println!("rung 3 ok: excerpt = {:?}", e.part);
}

// ── Rung 4: impl blocks + the &self elision gotcha ───────────────────────────
// To write methods on a struct that has a lifetime, you DECLARE the lifetime
// after `impl` and USE it on the type:  `impl<'a> Excerpt<'a> { ... }`.
// The `<'a>` after impl is "I'm introducing a lifetime named 'a"; the `<'a>` on
// Excerpt is "I'm using it". (Same shape as `impl<T> Vec<T>`.)
impl<'a> Excerpt<'a> {
    // Part A — elision rule 3 in action. One of the inputs is &self, so the
    // elided return lifetime is tied to &self. No annotations needed.
    // Implement: just hand back the held slice.
    fn part(&self) -> &str {
        self.part
    }

    // Part B — THE GOTCHA. Return whichever is longer (by .len()): the held
    // `self.part`, or the `candidate` passed in. The body wants to return EITHER
    // one... but elision rule 3 has already tied the elided return to &self, so
    // returning `candidate` will be rejected: its lifetime is unrelated to &self.
    //
    // Fix the signature so the return may legitimately come from `candidate`
    // too. Hint: self.part is &'a str. Give `candidate` that SAME 'a and make the
    // return &'a str. Then the compiler knows both sources share one lifetime.
    fn longer_of<'b>(&'b self, candidate: &'b str) -> &'b str {
        if self.part.len() > candidate.len() {
            self.part
        } else {
            candidate
        }
    }
}

fn check_4() {
    let novel = String::from("Call me Ishmael. Some years ago.");
    let e = first_sentence(&novel); // e.part == "Call me Ishmael."
    assert_eq!(e.part(), "Call me Ishmael.");

    let challenger = String::from("A substantially longer candidate string here");
    assert_eq!(e.longer_of(&challenger), challenger); // candidate wins
    assert_eq!(e.longer_of("hi"), "Call me Ishmael."); // self.part wins
    println!("rung 4 ok: methods on Excerpt<'a>");
}

// ── Rung 5: the dangling-reference footgun (owned vs borrowed return) ─────────
// THE defining lifetime error. A function can only hand out references to data
// that outlives the call — i.e. data the CALLER owns. It can NOT return a
// reference to something it created locally, because that local is dropped the
// moment the function returns; the reference would dangle.
//
// Part A — BROKEN AS WRITTEN (E0515: cannot return reference to local variable).
// `label` is created here and dies at the closing brace, so `&label` can't
// escape. There is no lifetime annotation that fixes this — the data simply does
// not live long enough. The honest fix is to return OWNED data: change the
// return type to `String` and hand back `label` itself (move it out).
fn make_label(id: u32) -> String {
    let label = format!("item-{id}");
    label
}

// Part B — the contrast that makes it click. Here the returned slice borrows the
// PARAMETER `haystack`, which the caller owns and which outlives the call, so
// returning a reference is totally fine (elision ties the result to haystack).
// Implement: return everything AFTER the first '/' in `haystack`, or "" if there
// is no '/'. (Hint: haystack.find('/') -> Option<usize>; slice from idx + 1.)
fn after_slash(haystack: &str) -> &str {
    match haystack.find('/') {
        Some(index) => &haystack[index + 1..],
        None => "",
    }
}

fn check_5() {
    assert_eq!(make_label(7), "item-7");
    assert_eq!(after_slash("usr/local/bin"), "local/bin");
    assert_eq!(after_slash("nodelimiter"), "");
    println!("rung 5 ok: owned return vs borrowed-from-input return");
}

// ── Rung 6: 'a: 'b outlives bounds ───────────────────────────────────────────
// Syntax: `'a: 'b` is a bound meaning "'a outlives 'b" — 'a lasts at LEAST as
// long as 'b. (Same slot as a trait bound `T: Clone`, but for lifetimes.)
// It's what lets the compiler treat a longer-lived &'a T as a shorter &'b T.
//
// Part A — two genuinely different lifetimes, returning the 'a one as 'b.
// Return `primary` if non-empty, otherwise `fallback`. The signature promises a
// &'b str, but one branch returns `primary: &'a str`. The compiler will refuse
// ("lifetime 'a may not live long enough") UNTIL you promise 'a outlives 'b.
// Add the `'a: 'b` bound to the signature, then implement the body.
fn or_default<'a: 'b, 'b>(primary: &'a str, fallback: &'b str) -> &'b str {
    if !primary.is_empty() {
        primary
    } else {
        fallback
    }
}

// Part B — where the bound is UNAVOIDABLE (you can't just unify here). `store`
// overwrites the string a slot points at: `*slot = value`. The slot already
// holds a &'b str, so to legally drop `value: &'a str` into it, `value` must
// live at least as long as 'b. Without `'a: 'b`, the assignment is rejected.
// Add the bound, then write the one-line body.
fn store<'a: 'b, 'b>(slot: &mut &'b str, value: &'a str) {
    *slot = value;
}

fn check_6() {
    let long = String::from("primary value");
    let short = String::from("fallback value");
    assert_eq!(or_default(&long, &short), "primary value");
    assert_eq!(or_default("", &short), "fallback value");

    let mut slot: &str = "initial";
    let replacement = String::from("updated");
    store(&mut slot, &replacement);
    assert_eq!(slot, "updated");
    println!("rung 6 ok: 'a: 'b outlives bounds");
}

// ── Rung 7: lifetimes + generics + trait bounds, and 'static ─────────────────
// Lifetime params and TYPE params share one `<...>` list (lifetimes come first):
//   fn foo<'a, T: Trait>(...)
// They don't interfere — 'a constrains borrows, T constrains a type.
use std::fmt::Display;

// Part A — the Book's classic. Print `ann` before deciding, then return the
// longer of x / y. As written it WON'T COMPILE once you use `ann` in a format
// string: T has no bound, so the compiler doesn't know it can be Displayed.
// Add a `T: Display` bound (in the same <...> list as 'a), then implement.
fn longest_with_announcement<'a, T>(x: &'a str, y: &'a str, ann: T) -> &'a str
where
    T: Display,
{
    println!("Announcing: {}", ann);
    if x.len() > y.len() { x } else { y }
}

// Part B — 'static, the lifetime that lasts the whole program. A `&'static str`
// borrows data that never goes away (string literals are baked into the binary).
// You met the dual of this in rung 5: there you returned an owned String because
// a local can't be borrowed out. Here's the OTHER escape hatch — intentionally
// LEAK the allocation so it lives forever, yielding a genuine &'static str.
// Implement with Box::leak (you used it in the box_heap ladder):
//   Box::leak(some_string.into_boxed_str())  ->  &'static str
fn leak_label(id: u32) -> &'static str {
    let label = format!("id-{}", id);
    Box::leak(label.into_boxed_str())
}

fn check_7() {
    assert_eq!(
        longest_with_announcement("alpha", "betas!", "comparing two"),
        "betas!"
    );

    let s: &'static str = leak_label(42);
    assert_eq!(s, "id-42");
    println!("rung 7 ok: lifetimes + generics + 'static");
}

// ── Rung 8: a borrowing iterator (Iterator yielding &str) ────────────────────
// `Words` walks a string and yields each whitespace-separated word as a &str
// that BORROWS the original string. This is how real iterators (str::split,
// slice::iter, etc.) hand out references without cloning.
//
// THE crux is the `Item` lifetime. Read carefully:
//   fn next(&mut self) -> Option<Self::Item>
// The &mut self borrow lasts only for the duration of this one next() call. But
// the &str we return points into the underlying string (lifetime 'a), which
// lives MUCH longer than one call. So `Item` must be `&'a str` — tied to the
// string, NOT to &mut self. That's what lets you hold a yielded word across a
// later next() call, and what makes `.collect::<Vec<&str>>()` possible.
// (Tying Item to &mut self instead is the classic mistake; you'd then be unable
//  to keep any item past the next iteration.)
struct Words<'a> {
    remainder: &'a str,
}

impl<'a> Words<'a> {
    fn new(text: &'a str) -> Self {
        Words { remainder: text }
    }
}

impl<'a> Iterator for Words<'a> {
    // Item borrows the underlying string — lifetime 'a, the struct's lifetime.
    type Item = &'a str;

    // Implement: skip any leading spaces; if nothing remains, return None.
    // Otherwise carve off the next word: find the next ' ', return the slice
    // before it as the item, and store the slice AFTER it back in self.remainder
    // (store "" when there's no more). Return Some(word).
    // Hints: str::trim_start_matches(' '), str::find(' ') -> Option<usize>,
    // and slice with &s[..i] / &s[i + 1..].
    fn next(&mut self) -> Option<Self::Item> {
        let trimmed = self.remainder.trim_start_matches(' ');
        if trimmed.is_empty() {
            return None;
        }

        let word = match trimmed.find(' ') {
            Some(index) => {
                self.remainder = &trimmed[index + 1..];
                &trimmed[..index]
            }
            None => {
                self.remainder = "";
                trimmed
            }
        };

        Some(word)
    }
}

fn check_8() {
    let text = String::from("the quick  brown fox"); // note the double space
    let words: Vec<&str> = Words::new(&text).collect();
    assert_eq!(words, vec!["the", "quick", "brown", "fox"]);

    // Proof the Item lifetime is 'a (the string), not &mut self: we hold `first`
    // while calling next() AGAIN for `second`. This only compiles because both
    // borrow `text`, not the iterator.
    let mut it = Words::new(&text);
    let first = it.next().unwrap();
    let second = it.next().unwrap();
    assert_eq!((first, second), ("the", "quick"));

    println!("rung 8 ok: borrowing iterator yields {words:?}");
}

// ── Rung 9 (CAPSTONE): StrSplit — a zero-copy split iterator, two lifetimes ───
// Build your own `"a,b,c".split(",")`. It must NOT allocate — every yielded
// piece is a &str slice borrowing the original haystack.
//
// TWO distinct lifetimes, and this is the heart of the rung:
//   - 'haystack: the string being split. The yielded items borrow THIS.
//   - 'delimiter: the separator. The items do NOT borrow this — once we've found
//     a delimiter we don't keep a reference to it in the output.
// Keeping them separate means a caller can pass a short-lived delimiter (even a
// temporary that drops before the results are used) and still keep the pieces.
// (Collapsing both into one lifetime would over-constrain every caller — exactly
//  the lesson from rung 2b and rung 6, now at struct scale.)
//
// Why `remainder: Option<&str>` and not just `&str`? To distinguish "more to
// yield, possibly an empty final field" from "fully exhausted". Splitting "a,"
// on "," must yield ["a", ""] — a trailing empty piece — and then stop. The
// Option lets `next` hand out that last "" once (via .take()) and then return
// None forever after.
struct StrSplit<'haystack, 'delimiter> {
    remainder: Option<&'haystack str>,
    delimiter: &'delimiter str,
}

impl<'haystack, 'delimiter> StrSplit<'haystack, 'delimiter> {
    // Start with the whole haystack remaining.
    fn new(haystack: &'haystack str, delimiter: &'delimiter str) -> Self {
        Self {
            remainder: Some(haystack),
            delimiter,
        }
    }
}

impl<'haystack, 'delimiter> Iterator for StrSplit<'haystack, 'delimiter> {
    // The crux: items borrow the HAYSTACK, so Item is &'haystack str — NOT
    // &'delimiter. (Confirm to yourself why 'delimiter would be wrong here.)
    type Item = &'haystack str;

    // Algorithm:
    //   1. Get a &mut to the remaining str, bailing out with None if it's already
    //      exhausted. The idiom is:  let remainder = self.remainder.as_mut()?;
    //      (as_mut() turns &mut Option<&str> into Option<&mut &str>; ? returns
    //       None for you when remainder is None.)
    //   2. If `remainder` contains the delimiter at index i:
    //        - the piece to yield is everything BEFORE i,
    //        - update *remainder to everything AFTER the delimiter
    //          (advance by i + self.delimiter.len() — delimiters can be >1 char),
    //        - return Some(piece).
    //   3. If the delimiter is NOT found, this is the last piece: yield the whole
    //      remainder and mark the split exhausted in one move with
    //      self.remainder.take() (returns Some(last) and sets the field to None).
    // Hint for step 2's slicing: *remainder is a &'haystack str, so &remainder[..i]
    // and &remainder[i + len..] are &'haystack str too.
    fn next(&mut self) -> Option<Self::Item> {
        let remainder = self.remainder.as_mut()?;

        match remainder.find(self.delimiter) {
            Some(idx) => {
                let current = *remainder;
                let piece = &current[..idx];
                *remainder = &current[idx + self.delimiter.len()..];
                Some(piece)
            }
            None => self.remainder.take(),
        }
    }
}

fn check_9() {
    // basic split
    let parts: Vec<&str> = StrSplit::new("a,b,c,d,e", ",").collect();
    assert_eq!(parts, vec!["a", "b", "c", "d", "e"]);

    // trailing empty field is preserved (the Option<remainder> reason)
    let trailing: Vec<&str> = StrSplit::new("a,b,c,", ",").collect();
    assert_eq!(trailing, vec!["a", "b", "c", ""]);

    // multi-char delimiter
    let multi: Vec<&str> = StrSplit::new("1::2::3", "::").collect();
    assert_eq!(multi, vec!["1", "2", "3"]);

    // THE two-lifetime payoff: the delimiter is a temporary that drops BEFORE we
    // read the results. Compiles only because pieces borrow the haystack, while
    // 'delimiter is an independent (shorter) lifetime.
    let haystack = String::from("x-y-z");
    let result: Vec<&str>;
    {
        let delim = String::from("-");
        result = StrSplit::new(&haystack, &delim).collect();
    } // delim dropped here, result still valid
    assert_eq!(result, vec!["x", "y", "z"]);

    println!("rung 9 ok: StrSplit yields {parts:?} (zero-copy, two lifetimes)");
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
