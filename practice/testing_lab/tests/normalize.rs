// Rung 7 — INTEGRATION TEST.
//
// This file is a SEPARATE CRATE from `testing_lab`. It compiles as its own test
// binary and links against the library, so it sees ONLY the public API
// (`testing_lab::normalize`) — never private helpers like `squeeze_spaces`.
// That's "black-box" testing: you exercise the crate exactly as a user would.
//
// Run just these:  cargo test -p testing_lab --test normalize

use crate::common::assert_normalizes;

// Pull in the shared helpers from tests/common/mod.rs.
mod common;

#[test]
fn normalizes_messy_input() {
    // your turn: use common::assert_normalizes to check that
    //   "  Hello   WORLD  "  normalizes to  "hello world"
    // (or call testing_lab::normalize directly and assert_eq! — your choice).
    assert_normalizes("  Hello   WORLD  ", "hello world");
}
