# Testing

> Ladder: [`src/bin/testing.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/testing.rs)
> + [`practice/testing_lab/`](https://github.com/utkarsh5026/rust-scratch/tree/master/practice/testing_lab) ·
> Run: `cargo test --bin testing` and `cargo test -p testing_lab` · Phase 0 · 9 rungs

## TL;DR

A test in Rust is just **a function tagged `#[test]`**. The compiler bundles all
of them into a separate test harness binary; `cargo test` runs each one in
isolation and catches its panic. **A test fails iff it panics** — and every
assertion macro is just a fancy `if !cond { panic!(...) }`.

There are three places tests live, distinguished by *scope*:

| Kind | Lives in | Sees | Tests it from |
|---|---|---|---|
| **Unit** | `#[cfg(test)] mod tests` inside the source file | **private** items (white-box) | the inside |
| **Integration** | top-level `tests/` dir (separate crate) | only `pub` items (black-box) | the outside, like a user |
| **Doctest** | `///` examples in doc comments | only `pub` items | the docs, kept honest |

Everything else — `assert_eq!`, `#[should_panic]`, `Result`-returning tests,
`#[ignore]`, doctest fences — is detail layered on those two facts.

## Why this exists (from first principles)

Most languages bolt testing on as a library: import a framework, register test
classes, run a separate tool. Rust builds it into the **compiler and `cargo`**,
and that design choice explains every quirk on this page.

Because the harness is part of the build, a test is an ordinary function the
compiler already type-checks. There is no "test runner reflection" — `#[test]`
is an attribute the compiler collects at compile time. And because the harness
reports a result by observing whether the function *returned* or *panicked*, the
entire assertion vocabulary reduces to "panic on failure." Once you internalize
"failing == panicking," the rest follows: `#[should_panic]` simply *inverts* that
rule, and a `Result`-returning test lets an `Err` stand in for a panic.

The one thing that trips everyone up — why integration tests and doctests need a
**library** — also falls out of the build model. A `tests/foo.rs` file compiles
as *its own crate* that links your code as a dependency. A dependency only
exposes its `pub` surface. Binaries (`src/main.rs`, `src/bin/*.rs`) have no
linkable public API, so there's nothing for an external test crate to call. Hence
this ladder spins up `practice/testing_lab/` (a real `src/lib.rs`) the moment it
needs `tests/` and doctests.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|---|---|---|
| 1 | foundations | First test | `#[cfg(test)] mod tests`, `#[test]`, `assert_eq!`, `use super::*` |
| 2 | foundations | Assertion toolbox | `assert!` / `assert_eq!` / `assert_ne!` + custom messages |
| 3 | mechanics | `Result`-returning test | `fn() -> Result<(), E>` lets you use `?` instead of `.unwrap()` |
| 4 | mechanics | `#[should_panic]` | assert a panic happens; tighten with `expected = "..."` |
| 5 | footgun | Execution control | `#[ignore]`, name filter, `--nocapture`, parallelism |
| 6 | footgun | Assertion footguns | loose `should_panic`, float equality, private reach |
| 7 | real-world | Integration tests | `tests/` = separate crate, public API only, `common/mod.rs` |
| 8 | real-world | Doctests | runnable `///` examples, hidden `#` lines, `?`, fences |
| 9 | capstone | A `Ledger` tested every way | unit + integration + doctests in one suite |

## The ideas, built up

### 1. A test is a `#[test]` fn in a `#[cfg(test)]` module

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_works() {
        assert_eq!(add(2, 3), 5);
    }
}
```

Two attributes carry the whole foundation:

- **`#[cfg(test)]`** is conditional compilation. The module exists *only* when
  building under `cargo test`; a `cargo build --release` compiles it away to
  nothing. That's why you can write as many tests as you like with zero binary
  cost.
- **`#[test]`** marks a function the harness should collect and run. It must take
  no arguments and return `()` (or `Result`, see rung 3).

`use super::*` matters because `mod tests` is a *child* module. Child modules
don't automatically see their parent's items, so you import them. And because the
test module is a child of the very module it tests, it can see **private** items —
the defining privilege of unit tests, which rung 6 and 7 turn into a contrast.

### 2. The assertion toolbox, and why `assert_eq!` beats `assert!`

```rust
assert!(classify(0) == "zero");                       // boolean: prints only "false"
assert_eq!(classify(-4), "negative");                 // prints left AND right on failure
assert_ne!(classify(7), "zero");
assert_eq!(classify(2), "positive", "ctx: {}", 2);    // trailing custom message
```

All three panic on failure. The difference is the **failure message**:

```text
// assert!(a == b)  on failure:
assertion failed: classify(0) == "ZERO"

// assert_eq!(a, b) on failure:
assertion `left == right` failed
  left: "zero"
 right: "ZERO"
```

`assert_eq!` captures both operands and prints them, so a red test tells you *what
the value actually was*, not merely that a boolean came out false. Reach for
`assert_eq!`/`assert_ne!` whenever you're comparing two values; save bare
`assert!` for genuine booleans. Any of them takes a trailing format string for
extra context.

### 3. `Result`-returning tests and `?`

A `#[test]` fn may return `Result<(), E>`. `Ok(())` passes; `Err(e)` fails and
prints `e`. That unlocks `?` in test bodies:

```rust
#[test]
fn parse_pair_ok() -> Result<(), Box<dyn std::error::Error>> {
    let (a, b) = parse_pair("3,4")?;   // ? compiles ONLY because of the return type
    assert_eq!((a, b), (3, 4));
    Ok(())
}
```

The `Box<dyn std::error::Error>` is the catch-all error type any `?`-converted
error can flow into. Why prefer this over `.unwrap()`? Both fail the test on an
unexpected error, but the *intent* differs: `?` says "an error here is a test
failure, surface it," while `.unwrap()` says "this can't happen, panic if it
does." When the error is part of the path you're exercising, `?` reads better and
keeps the happy path uncluttered.

### 4. `#[should_panic]` — inverting the pass/fail rule

Sometimes the *correct* behavior is to panic, and you want to assert it does.
`#[should_panic]` inverts the harness rule: the test passes **iff** the body
panics.

```rust
#[should_panic]                                       // passes on ANY panic
#[test]
fn seat_too_high() { seat(99); }

#[should_panic(expected = "seat 99 out of range")]    // passes only on THIS panic
#[test]
fn seat_too_high_expected() { seat(99); }
```

The runner even labels them: `test tests::seat_too_high - should panic ... ok`.

The bare form is dangerously loose — it can't tell a *correct* panic from an
unrelated one (rung 6 weaponizes this). `expected = "..."` requires the panic
message to *contain* that substring, pinning the test to the panic you actually
mean. **Always add `expected`.**

### 5. Driving the runner

Knobs after a `--` go to the test harness, not to cargo:

```bash
cargo test --bin testing                      # everything (ignored ones skipped)
cargo test --bin testing classify             # only tests whose name contains "classify"
cargo test --bin testing -- --ignored         # run ONLY the #[ignore] tests
cargo test --bin testing noisy -- --nocapture # let println! through
cargo test --bin testing -- --test-threads=1  # run serially, not in parallel
```

- **`#[ignore]`** marks a test skipped by default (slow/flaky/manual). It shows as
  `N ignored` and runs only with `-- --ignored`.
- **Captured output**: `cargo test` swallows the stdout of *passing* tests (it
  only surfaces output on failure, to keep noise down). `-- --nocapture` lets
  `println!` through.
- **Name filter** is a substring match on the full test path.

### 6. Tests run in parallel — the footgun that defines this rung

By default the harness runs tests on **multiple threads at once**. Independent
tests get faster; tests that share mutable state (a global, a file, an env var,
the current working dir) can interleave and clobber each other — *green alone,
flaky in the suite*. Two fixes:

- **Isolate** — give each test its own state. The real fix.
- **Serialize** — `-- --test-threads=1`. A crutch that hides the coupling.

The `add` / `classify` / `seat` tests share zero mutable state, which is precisely
why they're safe in parallel. Keep tests that way by construction.

## Footguns

The whole footgun tier (rung 6) is a catalogue of "passing" tests that lie.

**Loose `#[should_panic]` passes for the wrong reason.** A panic anywhere in the
body satisfies a bare `#[should_panic]`:

```rust
// LIE: the unwrap panics FIRST, so seat(0) is never reached, yet the test is green
#[should_panic]
#[test]
fn footgun_loose() {
    let _bogus: u32 = "not a number".parse::<u32>().unwrap(); // panics HERE
    seat(0);                                                  // never runs
}

// HONEST: pin the message, and make the line under test the one that panics
#[should_panic(expected = "seat 0 out of range")]
#[test]
fn footgun_fixed() { seat(0); }
```

Adding `expected` *fails* the lying version (the parse panic doesn't contain "seat
0 out of range"), exposing the bug. The same trap reappears in doctests: a
`should_panic` doctest still containing a `todo!()` will pass, because `todo!()`
panics.

**Never `assert_eq!` raw floats.** IEEE-754 makes `0.1 + 0.2 != 0.3`:

```rust
// WRONG: fails with right: 0.30000000000000004
assert_eq!(0.1_f64 + 0.2, 0.3);

// OK: compare within a tolerance
assert!((0.1_f64 + 0.2 - 0.3).abs() < 1e-9);
```

**Private reach is a unit-test-only superpower.** A unit test can call a private
`fn secret_key() -> u32` because it lives inside the crate. Hold that thought —
the next rung shows an integration test getting a *compile error* for the same
call.

## Real-world patterns

### Integration tests: `tests/` is a separate crate

The `testing_lab` library exposes `pub fn normalize` backed by a private
`squeeze_spaces`. An integration test links the crate from outside:

```rust
// practice/testing_lab/tests/normalize.rs  — its own crate, black-box
mod common;

#[test]
fn normalizes_messy_input() {
    assert_eq!(testing_lab::normalize("  Hello   WORLD  "), "hello world");
}
```

Try to reach a private helper and the compiler stops you — *this is the wall that
defines black-box testing*:

```text
error[E0603]: function `squeeze_spaces` is private
 --> tests/_probe.rs:3:18
  |
3 |     testing_lab::squeeze_spaces("a  b");
  |                  ^^^^^^^^^^^^^^ private function
```

A unit test reaches privates; an integration test sees only `pub`. Different
scope, different job.

**Shared helpers go in `tests/common/mod.rs`, not `tests/common.rs`.** Every file
*directly* under `tests/` compiles as its own test binary — so `tests/common.rs`
would run as a confusing empty test target. A file in a *subdirectory*
(`tests/common/mod.rs`) is a plain module other test files pull in with
`mod common;`, and is **not** itself a test target:

```rust
// tests/common/mod.rs
use testing_lab::normalize;
pub fn assert_normalizes(input: &str, expected: &str) {
    assert_eq!(normalize(input), expected, "input: {input:?}");
}
```

### Doctests: examples that can't rot

A fenced code block in a `///` comment is compiled and run by `cargo test`. If the
example stops matching the API, the doctest fails — your docs can never silently
drift:

```rust
/// ```
/// use testing_lab::normalize;
/// # let _unused = 1;                       // hidden `#` line: runs, not rendered
/// assert_eq!(normalize("  Hi   THERE "), "hi there");
/// ```
pub fn normalize(s: &str) -> String { /* ... */ }
```

Doctest mechanics worth knowing:

- **Hidden lines.** A line starting with `#` runs but is omitted from the rendered
  docs — perfect for boilerplate setup that would distract the reader.
- **`?` needs a `Result`-returning example.** rustdoc wraps your snippet in a
  `fn main() -> ()`, so `?` won't compile until the example returns a `Result`.
  End it with a hidden `# Ok::<(), SomeError>(())` and rustdoc switches the
  wrapper's return type to match:

  ```rust
  /// let p = parse_port("8080")?;
  /// assert_eq!(p, 8080);
  /// # Ok::<(), std::num::ParseIntError>(())
  ```

- **Fence attributes** change what "pass" means:
  - ` ```should_panic ` — passes only if the example panics.
  - ` ```compile_fail ` — passes only if the example *fails to compile* (great for
    proving an API rejects misuse, e.g. `nth_word(42, 0)` where a `&str` is
    required).
  - ` ```no_run ` — compiles but doesn't execute (network/side-effects).
  - ` ```ignore ` — skip entirely.

## Capstone insight

Rung 9 tests one small library — a money `Ledger` (balance in integer cents, a
private `entries` counter, and a private `check_invariant`) — *every way at once*,
and the structural lesson is how the techniques **partition by scope**:

```text
cargo test -p testing_lab
  unittests src/lib.rs ...  white-box: assert on private `entries`, call check_invariant()
  tests/ledger.rs      ...  black-box: public deposit/withdraw + custom assert_balance helper
  Doc-tests testing_lab...  runnable docs: a basic one, a should_panic, a `?`-using one
```

- A **unit** test inside `mod ledger::tests` asserts `ledger.entries == 3` — a
  field an integration test literally cannot name.
- An **integration** test in `tests/ledger.rs` drives only `deposit`/`withdraw`
  and checks results through a shared `common::assert_balance`, exactly as a
  downstream user would.
- A **doctest** on every public method doubles as documentation *and* a test,
  including `deposit`'s `should_panic` (negative amount) and `withdraw`'s `?`-using
  example.

The "aha": these aren't three competing styles, they're three altitudes. Unit
tests verify internal invariants you can only see from inside; integration tests
verify the contract you ship; doctests verify the contract *as documented*. A
mature crate runs all three from one `cargo test`, and the harness reports each as
its own section.

## Explain it back

- Why does a test "fail"? What single mechanism underlies `assert_eq!`,
  `#[should_panic]`, and a `Result`-returning test?
- What does `#[cfg(test)]` cost a release build, and why?
- Why can a unit test call a private function but an integration test gets E0603
  for the same call?
- Why do integration tests and doctests require a library crate, but unit tests
  don't?
- Your `#[should_panic]` test is green. Name two ways it could be lying, and the
  fix for each.
- Why does `assert_eq!(0.1 + 0.2, 0.3)` fail, and what do you write instead?
- Why is `tests/common/mod.rs` correct for shared helpers but `tests/common.rs`
  wrong?
- A doctest uses `?` and won't compile. What's missing?

## See also

- [Modules & visibility](modules.md) — `pub`, privacy, and the crate boundary that
  decides what an integration test can see.
- [Cargo features & `cfg`](features-cfg.md) — `#[cfg(test)]` is the same
  conditional-compilation machinery as `#[cfg(feature = "...")]`.
- [Error handling architecture](error-arch.md) — `Box<dyn Error>` as the catch-all
  for `?` in tests.
