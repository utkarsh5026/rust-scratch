// Concept: Cow (Clone-on-Write) — std::borrow::Cow
//
// Problem 1 of the ladder: "borrow-or-own basics"
//
// Task: implement `ensure_https`. If the url ALREADY starts with "https://",
// return it as-is WITHOUT allocating a new String (return Cow::Borrowed).
// Otherwise, prepend "https://" — which requires a new String (Cow::Owned).
//
// Success criterion: all the asserts in main() pass. The key thing they check
// is not just the text, but WHICH variant you returned — borrowed for the
// already-correct url, owned for the one you had to build.
//
// Run with: cargo run --bin cow

use std::borrow::Cow;

fn ensure_https(url: &str) -> Cow<str> {
    // YOUR TURN:
    // - if `url` starts with "https://"  -> return it borrowed (no allocation)
    // - otherwise build "https://" + url -> return it owned
    //
    // Hints to recall: &str has `.starts_with(...)`. Cow has two variants you
    // can construct directly: Cow::Borrowed(...) and Cow::Owned(...).
    todo!("implement ensure_https")
}

fn main() {
    let already = ensure_https("https://example.com");
    let fixed = ensure_https("example.com");

    // Both should read like the right string...
    assert_eq!(already.as_ref(), "https://example.com");
    assert_eq!(fixed.as_ref(), "https://example.com");

    // ...but the first should NOT have allocated (still borrowed),
    // while the second HAD to allocate (owned).
    assert!(matches!(already, Cow::Borrowed(_)), "already-https url should stay Borrowed (no alloc)");
    assert!(matches!(fixed, Cow::Owned(_)), "fixed url should be Owned (we built a new String)");

    println!("✅ problem 1 passed: borrowed when possible, owned only when needed");
}
