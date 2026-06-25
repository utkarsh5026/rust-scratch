// Rung 9 capstone — INTEGRATION test for the Ledger (black-box, public API).
//
// Run:  cargo test -p testing_lab --test ledger

mod common;

use testing_lab::ledger::{Ledger, Overdraw};

#[test]
fn deposit_then_withdraw_flow() {
    // your turn: drive a Ledger through deposit/withdraw using ONLY the public
    // API, and check the balance via the shared helper:
    //   common::assert_balance(&ledger, expected_cents)
    let mut ledger = Ledger::new();

    ledger.deposit(1000);
    ledger.withdraw(350).unwrap();

    common::assert_balance(&ledger, 650);
}

#[test]
fn overdraw_surfaces_error() {
    // your turn: a withdrawal beyond the balance returns Err(Overdraw { .. })
    // with the right `requested` / `available`. (You can construct Overdraw to
    // compare because its fields are pub — that's a deliberate public API.)
    let mut l = Ledger::new();
    l.deposit(100);

    assert_eq!(
        l.withdraw(500),
        Err(Overdraw {
            requested: 500,
            available: 100,
        })
    );
}
