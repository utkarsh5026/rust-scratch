// Testing — unit, integration (tests/), doctests, #[should_panic], test organization
//
// HOW TO RUN THIS LADDER:  cargo test --bin testing
//   (NOT `cargo run` — the action is in the `#[cfg(test)] mod tests` blocks,
//    which only get compiled under `cargo test`.)
//
// Ladder (rungs 1-6 live here; 7-9 spin up a real lib in practice/testing_lab/):
//   1. First test            — #[cfg(test)] mod tests, #[test], assert_eq!        [DONE]
//   2. Assertion toolbox     — assert!/assert_ne!, custom messages               [DONE]
//   3. Result-returning test — fn() -> Result<(), E>, the ? operator in tests    [DONE]
//   4. #[should_panic]       — assert it panics; tighten with expected = "..."    [DONE]
//   5. Execution control     — #[ignore], filtering, --nocapture, parallelism     [DONE]
//   6. Assertion footguns    — loose should_panic, float compares, private reach   [TODO]
//   7. Integration tests     — tests/ dir, black-box public API, common/ helper
//   8. Doctests              — runnable /// examples, hidden lines, ? , fences   [DONE]
//   9. Capstone              — a tiny lib tested every way at once  (in testing_lab) [TODO]

fn main() {
    // A bin needs a main, but tests don't run through it. Run the ladder with:
    println!("Run the tests:  cargo test --bin testing");
}

// ───────────────────────── Rung 1: your first test ─────────────────────────
// `add` is already written. Your job is to TEST it.
#[cfg(test)]
fn add(a: i64, b: i64) -> i64 {
    a + b
}

#[cfg(test)]
mod tests {
    // `super::*` pulls the parent module (this file) into scope so the test
    // can see `add`. `#[cfg(test)]` means this whole module is compiled ONLY
    // under `cargo test` — it adds zero bytes to a normal build.
    use super::*;

    #[test]
    fn add_works() {
        // your turn: assert that add(2, 3) equals 5, using assert_eq!.
        // Then run:  cargo test --bin testing
        assert_eq!(add(2, 3), 5);
    }

    // ─────────────────── Rung 2: the assertion toolbox ───────────────────
    // Four macros do most of the work. Write one test that exercises them:
    //   - assert!(cond)            — fails if cond is false
    //   - assert_eq!(a, b)         — fails if a != b, and PRINTS both values
    //   - assert_ne!(a, b)         — fails if a == b
    //   - any of them takes a trailing custom message: assert!(c, "why: {}", x)
    //
    // `classify` is written for you. Test it below.
    #[test]
    fn classify_toolbox() {
        assert!(classify(0) == "zero");
        assert_eq!(classify(-4), "negative");
        assert_ne!(classify(7), "zero");
        assert_eq!(classify(2), "positive", "classify(2) was wrong: {}", 2);
    }

    // ──────────────── Rung 3: tests that return Result, and `?` ────────────────
    // A #[test] fn may return `Result<(), E>`. Ok => pass, Err => fail (and the
    // Err is printed). That lets you use `?` instead of `.unwrap()` everywhere.
    //
    // `parse_pair("3,4")` should give Ok((3, 4)). It can fail two ways: no comma,
    // or a non-number half — both already wired to return an Err.
    //
    // Write this test so its body uses `?` (NOT .unwrap()) to unwrap the
    // successful parse, then assert_eq! the result. The return type is the key.
    #[test]
    fn parse_pair_ok() -> Result<(), Box<dyn std::error::Error>> {
        let (a, b) = parse_pair("3,4")?;
        assert_eq!((a, b), (3, 4));
        Ok(())
    }

    // ──────────────────────── Rung 4: #[should_panic] ────────────────────────
    // `seat` panics if you ask for a seat outside 1..=10. We want a test that
    // asserts that panic happens — a passing test that EXPECTS a panic.
    //
    // (a) Write `seat_too_high`: annotate it with #[should_panic], body calls
    //     seat(99). With no #[should_panic] this body would FAIL the test
    //     (panics propagate); with it, the panic is the success condition.
    //
    // (b) Then make it precise: change the attribute to
    //     #[should_panic(expected = "seat 99 out of range")]
    //     so it only passes for THAT panic, not any random one.
    //     (Footgun preview — rung 6 — a bare #[should_panic] would also "pass"
    //      if seat(99) panicked for a totally unrelated reason.)

    // your turn: write the #[should_panic] test here.
    // #[should_panic]   // <- start here, then tighten to expected = "..."
    // #[test]
    // fn seat_too_high() { seat(99); }

    #[should_panic]
    #[test]
    fn seat_too_high() {
        seat(99);
    }

    #[should_panic(expected = "seat 99 out of range")]
    #[test]
    fn seat_too_high_expected() {
        seat(99);
    }

    // ─────────────────────── Rung 5: execution control ───────────────────────
    // Four things every Rustacean drives the test runner with. Knobs live AFTER
    // a `--` (they go to the test harness, not cargo):
    //   cargo test --bin testing                      # all (ignored ones skipped)
    //   cargo test --bin testing classify             # only tests whose NAME contains "classify"
    //   cargo test --bin testing -- --ignored         # run ONLY the #[ignore] ones
    //   cargo test --bin testing -- --nocapture       # let println! through (normally captured)
    //   cargo test --bin testing -- --test-threads=1  # run serially, not in parallel
    //
    // (a) #[ignore]: mark `slow_check` with #[ignore] (above #[test]). It should
    //     be SKIPPED in a normal run (look for "1 ignored" in the summary), and
    //     only run when you pass `-- --ignored`. Use it for slow/flaky tests.
    #[ignore]
    #[test]
    fn slow_check() {
        assert!(add(1, 1) == 2);
    }

    // (b) captured output: this test prints. By default `cargo test` SWALLOWS
    //     stdout of passing tests (only shows it on failure). Make this test
    //     pass, then run `cargo test --bin testing noisy -- --nocapture` and
    //     confirm you actually SEE the printed line.
    #[test]
    fn noisy() {
        println!("hello from inside the test harness");
        assert!(true);
    }

    // (c) THE PARALLELISM FOOTGUN (read, then answer in your head):
    //     cargo runs tests on MULTIPLE THREADS at once. So two tests that touch
    //     the SAME global/file/env-var/working-dir can interleave and clobber
    //     each other — passing alone, flaky together. The two fixes:
    //       - isolate: give each test its own state (the real fix), or
    //       - serialize: `-- --test-threads=1` (a crutch; hides the coupling).
    //     Q: the `seat`/`classify`/`add` tests share zero mutable state — that's
    //        exactly why they're safe to run in parallel. Keep it that way.

    // ───────────────────────── Rung 6: assertion footguns ─────────────────────
    // Three ways a "passing" test lies to you.

    // (a) The loose-#[should_panic] footgun. This test CLAIMS to verify that
    //     seat(0) panics. But the setup line panics FIRST (unwrap on a parse
    //     error), so the test goes green WITHOUT EVER CALLING seat(0). A bare
    //     #[should_panic] can't tell a real pass from this accident.
    //     your turn:
    //       1. Run it as-is — it passes (the lie).
    //       2. Add expected = "seat 0 out of range" to the attribute and run —
    //          it now FAILS, exposing that the wrong thing panicked.
    //       3. Fix the test: delete the bogus setup line so seat(0) is the
    //          panic that fires. Keep the `expected`. Now it passes honestly.
    #[should_panic(expected = "seat 0 out of range")]
    #[test]
    fn footgun_loose_should_panic() {
        seat(0);
    }

    // (b) The float-equality footgun. 0.1 + 0.2 is NOT exactly 0.3 in IEEE-754.
    //     your turn:
    //       1. First write `assert_eq!(0.1_f64 + 0.2, 0.3);` and run — watch it
    //          FAIL and read the right-hand value (0.30000000000000004).
    //       2. Replace it with a tolerance check:
    //          assert!((0.1_f64 + 0.2 - 0.3).abs() < 1e-9);
    #[test]
    fn footgun_float_eq() {
        assert!((0.1_f64 + 0.2 - 0.3).abs() < 1e-9);
    }

    // (c) Private reach. `secret_key` has NO `pub` — it's private. This unit
    //     test can still call it, because unit tests live INSIDE the crate.
    //     (Foreshadow rung 7: an integration test in tests/ is a separate
    //     crate and could NOT see this — it only gets the public API.)
    //     your turn: assert_eq! that secret_key() == 42.
    #[test]
    fn footgun_private_reach() {
        assert_eq!(secret_key(), 42);
    }
}

// secret_key: PRIVATE (no `pub`). Reachable from unit tests, not integration.
#[cfg(test)]
fn secret_key() -> u32 {
    42
}

// seat: the function under test for rung 4. Valid seats are 1..=10.
#[cfg(test)]
fn seat(n: u32) -> u32 {
    if !(1..=10).contains(&n) {
        panic!("seat {n} out of range (valid 1..=10)");
    }
    n
}

// parse_pair: the function under test for rung 3. Parses "a,b" into (a, b).
#[cfg(test)]
fn parse_pair(s: &str) -> Result<(i64, i64), Box<dyn std::error::Error>> {
    let (a, b) = s.split_once(',').ok_or("missing ',' separator")?;
    Ok((a.trim().parse()?, b.trim().parse()?))
}

#[cfg(test)]
// classify: the function under test for rung 2.
fn classify(n: i64) -> &'static str {
    if n == 0 {
        "zero"
    } else if n < 0 {
        "negative"
    } else {
        "positive"
    }
}
