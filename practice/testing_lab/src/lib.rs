//! `testing_lab` — a tiny library the Testing ladder uses to practice the kinds
//! of tests that only make sense against a *library* crate:
//!
//! - rung 7: **integration tests** in `tests/` (black-box the public API)
//! - rung 8: **doctests** (runnable `///` examples)
//! - rung 9: the **capstone** (every test flavor at once)
//!
//! Run its tests with:  `cargo test -p testing_lab`

/// Normalize a piece of user text: trim the ends, lowercase it, and collapse
/// any run of internal whitespace down to a single space.
///
/// # Examples
///
/// ```
/// use testing_lab::normalize;
/// # let _unused = 1;
///
/// assert_eq!(normalize("  Hi   THERE "), "hi there");
/// ```
pub fn normalize(s: &str) -> String {
    squeeze_spaces(s.trim()).to_lowercase()
}

/// Parse a TCP port number from a string.
///
/// # Examples
///
/// ```
/// use testing_lab::parse_port;
///
/// let p = parse_port("8080")?;
/// assert_eq!(p, 8080);
/// # Ok::<(), std::num::ParseIntError>(())
/// ```
pub fn parse_port(s: &str) -> Result<u16, std::num::ParseIntError> {
    s.trim().parse()
}

/// Return the `n`th whitespace-separated word (0-indexed).
///
/// # Panics
///
/// Panics if `n` is out of range.
///
/// ```should_panic
/// testing_lab::nth_word("a b", 5);
/// ```
///
/// ```compile_fail
/// testing_lab::nth_word(42, 0);
/// ```
pub fn nth_word(s: &str, n: usize) -> &str {
    s.split_whitespace()
        .nth(n)
        .unwrap_or_else(|| panic!("word index {n} out of range"))
}

// ─────────────────────────── Rung 9: CAPSTONE ───────────────────────────
// `Ledger` is implemented for you. Your job is to TEST it every way you've
// learned, in one coherent suite. See the checklist in the module doc below.
pub mod ledger {
    //! A tiny append-only money ledger (balances in integer cents).
    //!
    //! CAPSTONE CHECKLIST — make all of these real and green:
    //!   [ ] doctest on `deposit`     — basic runnable example
    //!   [ ] doctest on `withdraw`    — uses `?` (returns Result)
    //!   [ ] doctest (should_panic)   — `deposit(-5)` panics
    //!   [ ] unit test (white-box)    — reaches the PRIVATE `entries`/invariant
    //!   [ ] integration test         — tests/ledger.rs, public API only
    //!   [ ] custom assert helper      — assert_balance(...) in tests/common
    //! Run it all with:  cargo test -p testing_lab

    /// Error returned when a withdrawal exceeds the available balance.
    #[derive(Debug, PartialEq, Eq)]
    pub struct Overdraw {
        pub requested: i64,
        pub available: i64,
    }

    /// An append-only ledger. Balance is tracked in integer cents and may
    /// never go negative (a private invariant enforced on every mutation).
    #[derive(Debug, Default)]
    pub struct Ledger {
        balance_cents: i64,
        entries: u32, // private: how many deposits+withdrawals happened
    }

    impl Ledger {
        /// Create an empty ledger.
        ///
        /// ```
        /// use testing_lab::ledger::Ledger;
        ///
        /// let ledger = Ledger::new();
        /// assert_eq!(ledger.balance(), 0);
        /// ```
        pub fn new() -> Self {
            Self::default()
        }

        /// Deposit a positive amount of cents.
        ///
        /// # Panics
        /// Panics if `cents <= 0`.
        ///
        /// ```should_panic
        /// let mut ledger = testing_lab::ledger::Ledger::new();
        ///
        /// ledger.deposit(-5);
        /// ```
        pub fn deposit(&mut self, cents: i64) {
            assert!(cents > 0, "deposit must be positive, got {cents}");
            self.balance_cents += cents;
            self.entries += 1;
            self.check_invariant();
        }

        /// Withdraw a positive amount; `Err(Overdraw)` if funds are insufficient.
        ///
        /// ```
        /// use testing_lab::ledger::{Ledger, Overdraw};
        ///
        /// let mut ledger = Ledger::new();
        /// ledger.deposit(1_000);
        /// ledger.withdraw(350)?;
        ///
        /// assert_eq!(ledger.balance(), 650);
        /// # Ok::<(), Overdraw>(())
        /// ```
        pub fn withdraw(&mut self, cents: i64) -> Result<(), Overdraw> {
            assert!(cents > 0, "withdraw must be positive, got {cents}");
            if cents > self.balance_cents {
                return Err(Overdraw {
                    requested: cents,
                    available: self.balance_cents,
                });
            }
            self.balance_cents -= cents;
            self.entries += 1;
            self.check_invariant();
            Ok(())
        }

        /// Current balance in cents.
        pub fn balance(&self) -> i64 {
            self.balance_cents
        }

        /// PRIVATE invariant — balance is never negative. Reachable from the
        /// unit tests below (same module), invisible to integration tests.
        fn check_invariant(&self) {
            assert!(
                self.balance_cents >= 0,
                "INVARIANT VIOLATED: negative balance {}",
                self.balance_cents
            );
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // your turn (WHITE-BOX unit test): exercise the public API, then assert
        // a PRIVATE detail an integration test could never see — e.g. that two
        // deposits + one withdraw left `entries == 3`, and call check_invariant().
        #[test]
        fn tracks_private_entry_count() {
            let mut ledger = Ledger::new();
            ledger.deposit(100);
            ledger.deposit(200);
            let _ = ledger.withdraw(150);
            assert_eq!(ledger.entries, 3);
            ledger.check_invariant();
        }

        // your turn: an overdraw returns the right Err WITHOUT mutating balance.
        #[test]
        fn overdraw_is_rejected() {
            let mut ledger = Ledger::new();
            ledger.deposit(100);
            let result = ledger.withdraw(150);
            assert!(result.is_err());
            assert_eq!(ledger.balance(), 100);
        }
    }
}

// PRIVATE helper (no `pub`). Unit tests in THIS file can call it; an
// integration test in `tests/` cannot — it's a separate crate that only sees
// the public surface. That contrast is the point of rung 7.
fn squeeze_spaces(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
