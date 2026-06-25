// Shared test helpers for the integration tests.
//
// WHY tests/common/mod.rs AND NOT tests/common.rs?
//   Every file directly under tests/ is compiled as its OWN integration-test
//   binary. So tests/common.rs would run as a (probably empty, confusing) test
//   target of its own. But a subdirectory module file — tests/common/mod.rs —
//   is NOT a test target; it's just a module other test files pull in with
//   `mod common;`. That's the idiom for shared test helpers.

use testing_lab::normalize;

/// Assert that `normalize(input)` equals `expected`, with a helpful message.
pub fn assert_normalizes(input: &str, expected: &str) {
    assert_eq!(normalize(input), expected, "input: {}", input);
}

/// Rung 9: assert a ledger's balance equals `expected_cents`, with a message.
/// A shared, reusable custom assertion — the kind of helper that keeps an
/// integration suite readable. your turn: implement it.
pub fn assert_balance(ledger: &testing_lab::ledger::Ledger, expected_cents: i64) {
    assert_eq!(
        ledger.balance(),
        expected_cents,
        "expected balance: {}",
        expected_cents
    );
}
