//! Error handling architecture — Phase 3
//! Run: cargo run --bin error_arch
//!
//! The big idea: errors are *values* of type `Result<T, E>`. `?` is sugar for
//! "if Err, convert via `From` and return early." The whole architecture is
//! about choosing `E`: typed enums (thiserror, for libs) vs erased opaque
//! errors (anyhow, for apps), glued together by `?` + `From`.
//!
//! Ladder:
//!   1. [x] foundations  — `?` across error kinds with `Box<dyn Error>`
//!   2. [x] foundations  — hand-rolled enum: Display + Error + manual From
//!   3. [x] mechanics    — thiserror derive (#[error], #[from], #[source])
//!   4. [x] mechanics    — anyhow: context, with_context, bail!, anyhow!
//!   5. [x] footgun      — source chains & downcasting
//!   6. [x] footgun      — `?` won't convert (E0277) + String-error anti-pattern
//!   7. [x] real-world   — library (thiserror) / app (anyhow) boundary
//!   8. [x] real-world   — error classification: non_exhaustive, is_retryable
//!   9. [x] capstone     — build a mini-anyhow from scratch

use std::error::Error;

// ─────────────────────────────────────────────────────────────────────────
// Problem 1 (foundations): `?` across different error kinds.
//
// `parse_and_double` takes a &str, parses it as an i32, and returns double it.
// Parsing can fail with std::num::ParseIntError. We ALSO want to reject the
// number 13 (unlucky) with our own ad-hoc error message.
//
// Goal: make this compile and pass check_1 by returning
// `Result<i32, Box<dyn Error>>` and using `?` to propagate the parse error,
// plus returning an error for 13. `Box<dyn Error>` is the "I don't care about
// the exact type, just bubble it" quick-and-dirty app error.
//
// Hint: a parse failure should propagate with `?`. For the 13 case, you can
// build a boxed error from a string with `.into()` or `Box::<dyn Error>::from`.
// ─────────────────────────────────────────────────────────────────────────

fn parse_and_double(s: &str) -> Result<i32, Box<dyn Error>> {
    let n = s.parse::<i32>()?;
    if n == 13 {
        return Err("13 is unlucky".into());
    }
    Ok(n * 2)
}

fn check_1() {
    assert_eq!(parse_and_double("21").unwrap(), 42);
    // a non-numeric string -> the ParseIntError bubbles up as Box<dyn Error>
    assert!(parse_and_double("oops").is_err());
    // our own rule
    assert!(parse_and_double("13").is_err());
    println!("check_1 ✓  `?` propagates heterogeneous errors as Box<dyn Error>");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 2 (foundations): hand-roll a typed error enum.
//
// `Box<dyn Error>` is fine for an app, but a *library* should hand its caller
// a typed error they can match on. The contract for being a "real" error in
// Rust is: implement `std::fmt::Display` (human message) AND
// `std::error::Error` (the marker trait that unlocks `?` -> Box<dyn Error>,
// source chains, etc.).
//
// `ConfigError` has two variants:
//   - Missing(String)   — a required key was absent (the String is the key)
//   - Parse(ParseIntError) — a value failed to parse as a number
//
// Implement, BY HAND (no thiserror yet — that's rung 3):
//   1. `impl Display for ConfigError`  — Missing => "missing key: <key>",
//      Parse => "invalid number: <inner>"
//   2. `impl Error for ConfigError`    — you can leave source() defaulting for
//      now, OR override it to return the inner ParseIntError for the Parse
//      variant (we'll lean on source() hard in rung 5).
//   3. `impl From<ParseIntError> for ConfigError`  — so that `?` on a parse
//      call inside `read_port` auto-converts into ConfigError::Parse.
//
// Then `read_port` looks a key up in a tiny config map and parses it.
// ─────────────────────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
enum ConfigError {
    Missing(String),
    Parse(ParseIntError),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(k) => write!(f, "missing key: {k}"),
            ConfigError::Parse(e) => write!(f, "invalid number: {e}"),
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConfigError::Parse(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ParseIntError> for ConfigError {
    fn from(e: ParseIntError) -> Self {
        ConfigError::Parse(e)
    }
}

fn read_port(cfg: &HashMap<String, String>) -> Result<u16, ConfigError> {
    if let Some(port) = cfg.get("port") {
        return port.parse::<u16>().map_err(ConfigError::from);
    }
    Err(ConfigError::Missing("port".to_string()))
}

fn check_2() {
    let mut cfg = HashMap::new();
    cfg.insert("port".to_string(), "8080".to_string());
    assert_eq!(read_port(&cfg).unwrap(), 8080);

    // missing key
    let empty = HashMap::new();
    match read_port(&empty) {
        Err(ConfigError::Missing(k)) => assert_eq!(k, "port"),
        other => panic!("expected Missing, got {other:?}"),
    }

    // unparseable value -> the ? converts ParseIntError into ConfigError::Parse
    let mut bad = HashMap::new();
    bad.insert("port".to_string(), "notanumber".to_string());
    assert!(matches!(read_port(&bad), Err(ConfigError::Parse(_))));

    // Display works, and it's usable as &dyn Error
    let e: ConfigError = ConfigError::Missing("port".to_string());
    assert_eq!(e.to_string(), "missing key: port");
    let _as_err: &dyn Error = &e; // proves Error is implemented

    println!("check_2 ✓  hand-rolled Display + Error + From (what thiserror generates)");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 3 (mechanics): let `thiserror` generate rung 2 for you.
//
// Everything you wrote by hand in rung 2 (Display, Error, From, source) is
// pure boilerplate. `thiserror` derives all of it from attributes. This is the
// idiomatic way a *library* defines its error type.
//
// Define `LoadError` as a derived thiserror enum with THREE variants:
//   - Io(std::io::Error)        — tag with #[from] so `?` on an io call works,
//                                 and #[error("io error: {0}")]
//   - BadNumber(ParseIntError)  — tag with #[from] too,
//                                 #[error("bad number: {0}")]
//   - Empty                     — a unit variant, #[error("input was empty")]
//
// Key attributes:
//   #[derive(Debug, thiserror::Error)]   on the enum
//   #[error("...")]                      on each variant -> Display impl.
//                                        {0} is the tuple field; for the
//                                        wrapped-source variants you can also
//                                        use {0} to print the inner error.
//   #[from]                              on a field -> generates From<that type>
//                                        AND makes it the source() automatically.
//
// `first_number` reads the FIRST line of `text`, trims it, and:
//   - returns LoadError::Empty if there are no lines / it's blank
//   - otherwise parses it as i64, letting `?` convert a ParseIntError
//     into LoadError::BadNumber via the #[from].
// (We include an Io variant just to prove #[from] composes; first_number
//  itself won't produce one.)
// ─────────────────────────────────────────────────────────────────────────

// TODO rung 3: define `LoadError` with #[derive(Debug, thiserror::Error)]
//   enum LoadError { Io(...), BadNumber(...), Empty }

#[derive(Debug, thiserror::Error)]
enum LoadError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("bad number: {0}")]
    BadNumber(#[from] ParseIntError),
    #[error("input was empty")]
    Empty,
}

fn first_number(text: &str) -> Result<i64, LoadError> {
    let mut lines = text.lines();
    if let Some(line) = lines.next() {
        let line = line.trim();
        if line.is_empty() {
            return Err(LoadError::Empty);
        }
        return Ok(line.parse::<i64>()?);
    }
    Err(LoadError::Empty)
}

fn check_3() {
    assert_eq!(first_number("42\n99").unwrap(), 42);
    assert!(matches!(first_number(""), Err(LoadError::Empty)));
    assert!(matches!(first_number("   "), Err(LoadError::Empty)));
    assert!(matches!(first_number("nope"), Err(LoadError::BadNumber(_))));

    // Display comes from #[error("...")]
    assert_eq!(
        first_number("nope").unwrap_err().to_string()[..12].to_string(),
        "bad number: "
    );

    // #[from] gives us a From impl + source() for free
    let io = LoadError::from(std::io::Error::new(std::io::ErrorKind::Other, "x"));
    assert!(std::error::Error::source(&io).is_some());

    println!("check_3 ✓  thiserror derives Display+Error+From+source from attributes");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 4 (mechanics): the application side — `anyhow`.
//
// An *application's* top layer rarely wants to match on error variants — it
// just wants "did it work? if not, give me a good report and bubble to main."
// `anyhow::Error` is one opaque type that ANY error (anything : Error + Send +
// Sync + 'static) converts into via `?`. Its superpower is *context*: adding
// human breadcrumbs as the error travels up.
//
// `anyhow::Result<T>` is just `Result<T, anyhow::Error>`.
//
// Implement `load_user(dir, id)`:
//   - builds a "path" string  format!("{dir}/{id}.txt")
//   - if `dir` is "missing", simulate a not-found by returning an error built
//     with the `anyhow!` macro: anyhow!("no such dir: {dir}")
//   - otherwise it tries to parse `id` as a u32 user id with `?`, BUT attach
//     context so a parse failure reads nicely. Use `.with_context(|| ...)`
//     (closure form, lazy) saying  format!("parsing user id {id:?}")
//   - on success return the doubled id as u64 (stand-in for "loaded user")
//
// You'll need:  use anyhow::{anyhow, Context};   (Context is the trait that
// adds `.context()` / `.with_context()` onto Result and Option).
// `bail!(...)` is also available = `return Err(anyhow!(...))`; try it for the
// missing-dir case if you like.
// ─────────────────────────────────────────────────────────────────────────

// TODO rung 4: bring anyhow items into scope (anyhow!, Context, maybe bail!)

use anyhow::{Context, anyhow};

fn load_user(dir: &str, id: &str) -> anyhow::Result<u64> {
    if dir == "missing" {
        return Err(anyhow!("no such dir: {dir}"));
    }
    let id = id
        .parse::<u32>()
        .with_context(|| format!("parsing user id {id:?}"))?;
    Ok(id as u64 * 2)
}

fn check_4() {
    assert_eq!(load_user("data", "21").unwrap(), 42);

    // missing dir -> anyhow! message
    let e = load_user("missing", "21").unwrap_err();
    assert_eq!(e.to_string(), "no such dir: missing");

    // bad id -> the context message is the OUTER display; the real parse error
    // is preserved underneath as the source (we'll walk that chain in rung 5).
    let e = load_user("data", "xyz").unwrap_err();
    assert_eq!(e.to_string(), r#"parsing user id "xyz""#);
    // the underlying ParseIntError is still there, one level down:
    assert!(e.source().is_some());

    println!("check_4 ✓  anyhow: opaque error + context() breadcrumbs");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 5 (footgun/edge): source chains & downcasting.
//
// An error is really a *linked list*: each error's `.source()` points at the
// lower-level cause it wrapped. anyhow's `.context()` builds exactly such a
// chain. Two skills every senior reaches for:
//   (a) walk the whole chain for a full report ("X: caused by Y: caused by Z")
//   (b) DOWNCAST an erased anyhow::Error back to a concrete type, to recover
//       typed info you can branch on even after it's been erased.
//
// Part A — `error_chain(err)`: given a &(dyn Error), return a Vec<String> of
//   every error's Display in the chain, starting with `err` itself, following
//   `.source()` until None.
//   e.g. for the load_user("data","xyz") error it should be roughly
//        ["parsing user id \"xyz\"", "invalid digit found in string"]
//
// Part B — `classify(err: &anyhow::Error) -> &'static str`:
//   anyhow::Error has `.downcast_ref::<T>()`. Build an error that wraps a
//   *typed* LoadError inside anyhow with context, then recover it:
//     - if the chain contains a LoadError::Empty      => "empty"
//     - else if it contains any LoadError             => "load"
//     - else                                          => "other"
//   Hint: `err.downcast_ref::<LoadError>()` returns Option<&LoadError>;
//   anyhow walks the chain for you when downcasting.
// ─────────────────────────────────────────────────────────────────────────

fn error_chain(err: &dyn Error) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = err;
    chain.push(current.to_string());
    while let Some(source) = current.source() {
        chain.push(source.to_string());
        current = source;
    }
    chain
}

fn classify(err: &anyhow::Error) -> &'static str {
    if let Some(load_error) = err.downcast_ref::<LoadError>() {
        match load_error {
            LoadError::Empty => "empty",
            LoadError::BadNumber(_) => "load",
            _ => "other",
        }
    } else {
        "other"
    }
}

fn check_5() {
    // Part A: anyhow context chain has 2 links
    let e = load_user("data", "xyz").unwrap_err();
    let chain = error_chain(e.as_ref()); // anyhow::Error -> &dyn Error via as_ref
    assert_eq!(
        chain.len(),
        2,
        "expected context + underlying parse error, got {chain:?}"
    );
    assert_eq!(chain[0], r#"parsing user id "xyz""#);
    assert!(chain[1].contains("invalid digit"));

    // hand-rolled ConfigError chain: Parse -> ParseIntError (source we overrode)
    let mut bad = HashMap::new();
    bad.insert("port".to_string(), "nope".to_string());
    let ce = read_port(&bad).unwrap_err();
    let chain = error_chain(&ce);
    assert_eq!(chain.len(), 2, "ConfigError + its ParseIntError source");

    // Part B: downcast an erased anyhow back to the typed LoadError
    let typed: anyhow::Error = anyhow::Error::new(LoadError::Empty).context("while loading config");
    assert_eq!(classify(&typed), "empty");

    let typed2: anyhow::Error =
        anyhow::Error::new(LoadError::BadNumber("z".parse::<i64>().unwrap_err()))
            .context("while loading config");
    assert_eq!(classify(&typed2), "load");

    let plain = anyhow::anyhow!("just a string error");
    assert_eq!(classify(&plain), "other");

    println!("check_5 ✓  walked source() chain + downcast erased error back to type");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 6 (footgun): the `?`-won't-convert wall + the String-error trap.
//
// PART A — E0277, "the trait `From<X>` is not satisfied".
//   `?` only works if the error type you're producing has a `From` impl for
//   the error type at the call site. When you invent a custom enum and forget
//   the From impls, `?` refuses to compile. This IS the most common real error.
//
//   `run_pipeline` runs two stages that fail with DIFFERENT error types:
//     - stage_a(n) -> Result<i32, ParseIntError-like>   (we reuse ParseIntError)
//     - stage_b(n) -> Result<i32, std::num::TryFromIntError>
//   and wants to return its own `PipelineError`. To make `?` work you must
//   add `From<ParseIntError>` and `From<TryFromIntError>` for PipelineError.
//
//   >>> EXPERIENCE THE ERROR FIRST <<<
//   Before adding the From impls, write the body using `?` and run it. Read the
//   E0277 message carefully ("the trait bound `PipelineError: From<...>` is not
//   satisfied"). THEN add the two From impls and watch it compile. Articulating
//   what the compiler asked for is the lesson.
//
// PART B — the `Result<T, String>` anti-pattern.
//   A lazy API returns its error as a plain String. The trap: `String` does
//   NOT implement `std::error::Error`, so a String "error" has no source chain,
//   can't be downcast, can't be matched — you've thrown away all structure and
//   kept only a sentence. `adapt_legacy` takes such a stringly-typed result and
//   converts it into a proper typed `PipelineError::Legacy(String)` so the rest
//   of the system gets a real Error again.
// ─────────────────────────────────────────────────────────────────────────

use std::num::TryFromIntError;

#[derive(Debug, thiserror::Error)]
enum PipelineError {
    #[error("stage a failed: {0}")]
    StageA(#[from] ParseIntError),

    #[error("stage b failed: {0}")]
    StageB(#[from] TryFromIntError),

    #[error("legacy: {0}")]
    Legacy(String),
}

fn stage_a(s: &str) -> Result<i64, ParseIntError> {
    s.parse::<i64>()
}

// stage_b narrows an i64 down to a u8 — fails with TryFromIntError if too big.
fn stage_b(n: i64) -> Result<u8, TryFromIntError> {
    u8::try_from(n)
}

fn run_pipeline(s: &str) -> Result<u8, PipelineError> {
    let n = stage_a(s)?;
    let n = stage_b(n)?;
    Ok(n)
}

// A legacy function that stringly-types its error (the anti-pattern).
fn legacy_op(ok: bool) -> Result<i32, String> {
    if ok {
        Ok(7)
    } else {
        Err("legacy blew up".to_string())
    }
}

fn adapt_legacy(ok: bool) -> Result<i32, PipelineError> {
    legacy_op(ok).map_err(PipelineError::Legacy)
}

fn check_6() {
    assert_eq!(run_pipeline("200").unwrap(), 200u8);
    assert!(matches!(run_pipeline("xyz"), Err(PipelineError::StageA(_)))); // parse fail
    assert!(matches!(
        run_pipeline("99999"),
        Err(PipelineError::StageB(_))
    )); // > u8::MAX

    assert_eq!(adapt_legacy(true).unwrap(), 7);
    match adapt_legacy(false) {
        Err(PipelineError::Legacy(msg)) => assert_eq!(msg, "legacy blew up"),
        other => panic!("expected Legacy, got {other:?}"),
    }
    // Proof of the payoff: once wrapped, it's a real Error again (Display works,
    // it's a &dyn Error, it could carry a source). A bare String never could.
    let e = adapt_legacy(false).unwrap_err();
    let _: &dyn Error = &e;
    assert_eq!(e.to_string(), "legacy: legacy blew up");

    println!("check_6 ✓  added From for ? (E0277 fixed) + wrapped a stringly error into a type");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 7 (real-world): the library / application boundary.
//
// THE canonical architecture. Two layers:
//
//   * `mod store` is a LIBRARY. It exposes a typed `thiserror` error so its
//     callers can match on / recover from specific failures. A library must
//     never force `anyhow` on its users.
//
//   * the APPLICATION layer (load_setting) consumes the library, adds human
//     `.context()`, and returns the opaque `anyhow::Error` that just bubbles
//     up to main. The app doesn't care about variants — until it does, in
//     which case it downcasts (rung 5) back to the library's typed error.
//
// Implement:
//   1. In `mod store`: finish `StoreError` (thiserror) with variants
//        NotFound { key: String }   #[error("key not found: {key}")]
//        Parse(ParseIntError)       #[error("not a number: {0}")] + #[from]
//      and implement `get_number(key)`:
//        - look up key in DB (provided). Missing -> StoreError::NotFound{key}.
//        - present -> parse as i64 with `?` (the #[from] converts).
//
//   2. `load_setting(key)` (app layer, returns anyhow::Result<i64>):
//        call store::get_number(key) and attach
//        `.with_context(|| format!("loading setting {key:?}"))`, then `?`.
//        Because anyhow::Error: From<E: Error+Send+Sync+'static>, the typed
//        StoreError converts in automatically AND stays recoverable underneath.
// ─────────────────────────────────────────────────────────────────────────

mod store {
    use super::ParseIntError;

    // a tiny fake database
    fn db(key: &str) -> Option<&'static str> {
        match key {
            "port" => Some("8080"),
            "timeout" => Some("not-a-number"),
            _ => None,
        }
    }

    #[derive(Debug, thiserror::Error)]
    pub enum StoreError {
        #[error("key not found: {key}")]
        NotFound { key: String },
        #[error("not a number: {0}")]
        Parse(#[from] ParseIntError),
    }

    pub fn get_number(key: &str) -> Result<i64, StoreError> {
        if let Some(value) = db(key) {
            value.parse::<i64>().map_err(StoreError::Parse)
        } else {
            Err(StoreError::NotFound {
                key: key.to_string(),
            })
        }
    }
}

fn load_setting(key: &str) -> anyhow::Result<i64> {
    store::get_number(key).with_context(|| format!("loading setting {key:?}"))
}

fn check_7() {
    // library layer: caller gets a TYPED error they can match on
    assert_eq!(store::get_number("port").unwrap(), 8080);
    match store::get_number("missing") {
        Err(store::StoreError::NotFound { key }) => assert_eq!(key, "missing"),
        other => panic!("expected typed NotFound, got {other:?}"),
    }
    assert!(matches!(
        store::get_number("timeout"),
        Err(store::StoreError::Parse(_))
    ));

    // app layer: opaque anyhow + context on the OUTSIDE...
    let e = load_setting("missing").unwrap_err();
    assert_eq!(e.to_string(), r#"loading setting "missing""#);
    // ...but the library's typed error is still recoverable UNDERNEATH:
    let typed = e.downcast_ref::<store::StoreError>();
    assert!(
        matches!(typed, Some(store::StoreError::NotFound { .. })),
        "the typed StoreError must survive under the anyhow context"
    );

    assert_eq!(load_setting("port").unwrap(), 8080);

    println!("check_7 ✓  thiserror lib + anyhow app: typed error survives under context");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 8 (real-world): error classification — recoverable vs fatal + retry.
//
// A mature error type doesn't just say *what* went wrong — it lets callers
// classify *how to react*. Two idioms:
//
//   * `#[non_exhaustive]` on a public error enum: tells downstream crates "more
//     variants may appear in future versions, so your `match` MUST have a `_`
//     arm." It future-proofs the library's error type against breaking changes.
//     (The forced-wildcard effect only bites OTHER crates, not this file — but
//     adding the attribute is the habit every library author needs.)
//
//   * a classification method like `is_retryable()` so callers (and a generic
//     retry loop) can branch on recoverable-vs-fatal without matching every
//     variant.
//
// Implement:
//   1. `#[non_exhaustive]` on `ApiError` (already a thiserror enum below).
//   2. `ApiError::is_retryable(&self) -> bool`:
//        RateLimited{..}, Timeout, ServiceUnavailable => true
//        NotFound{..}, Unauthorized                   => false
//   3. `ApiError::retry_after(&self) -> Option<u64>`:
//        Some(secs) only for RateLimited { retry_after_secs }, else None.
//   4. `run_with_retry(max_attempts, op)`: call `op()`; on Ok return it; on Err
//        retry (up to max_attempts total calls) ONLY while the error
//        `is_retryable()`. A non-retryable error returns immediately. If
//        attempts run out, return the last error.
// ─────────────────────────────────────────────────────────────────────────

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("request timed out")]
    Timeout,
    #[error("service unavailable")]
    ServiceUnavailable,
    #[error("not found: {resource}")]
    NotFound { resource: String },
    #[error("unauthorized")]
    Unauthorized,
}

impl ApiError {
    fn is_retryable(&self) -> bool {
        match self {
            ApiError::RateLimited { .. } | ApiError::Timeout | ApiError::ServiceUnavailable => true,
            ApiError::NotFound { .. } | ApiError::Unauthorized => false,
        }
    }

    fn retry_after(&self) -> Option<u64> {
        match self {
            ApiError::RateLimited { retry_after_secs } => Some(*retry_after_secs),
            _ => None,
        }
    }
}

// A generic retry loop driven purely by the error's classification.
fn run_with_retry<T, F>(max_attempts: u32, mut op: F) -> Result<T, ApiError>
where
    F: FnMut() -> Result<T, ApiError>,
{
    for attempt in 0..max_attempts {
        match op() {
            Ok(value) => return Ok(value),
            Err(error) => {
                if !error.is_retryable() || attempt + 1 == max_attempts {
                    return Err(error);
                }
                if let Some(retry_after) = error.retry_after() {
                    std::thread::sleep(std::time::Duration::from_secs(retry_after));
                }
            }
        }
    }
    Err(ApiError::ServiceUnavailable)
}

fn check_8() {
    assert!(ApiError::Timeout.is_retryable());
    assert!(ApiError::ServiceUnavailable.is_retryable());
    assert!(
        ApiError::RateLimited {
            retry_after_secs: 5
        }
        .is_retryable()
    );
    assert!(!ApiError::Unauthorized.is_retryable());
    assert!(
        !ApiError::NotFound {
            resource: "x".into()
        }
        .is_retryable()
    );

    assert_eq!(
        ApiError::RateLimited {
            retry_after_secs: 5
        }
        .retry_after(),
        Some(5)
    );
    assert_eq!(ApiError::Timeout.retry_after(), None);

    // retryable: fails twice with Timeout, succeeds on the 3rd call
    let mut calls = 0u32;
    let res = run_with_retry(3, || {
        calls += 1;
        if calls < 3 {
            Err(ApiError::Timeout)
        } else {
            Ok(calls)
        }
    });
    assert_eq!(res.unwrap(), 3);
    assert_eq!(calls, 3, "should have retried up to success");

    // non-retryable: must STOP immediately, not burn all attempts
    let mut calls = 0u32;
    let res = run_with_retry(5, || {
        calls += 1;
        Err::<(), _>(ApiError::Unauthorized)
    });
    assert!(matches!(res, Err(ApiError::Unauthorized)));
    assert_eq!(calls, 1, "non-retryable error must not be retried");

    // retryable but never recovers: exhausts attempts, returns last error
    let mut calls = 0u32;
    let res = run_with_retry(2, || {
        calls += 1;
        Err::<(), _>(ApiError::ServiceUnavailable)
    });
    assert!(matches!(res, Err(ApiError::ServiceUnavailable)));
    assert_eq!(calls, 2, "should have used exactly max_attempts");

    println!("check_8 ✓  classification (is_retryable) drives a generic retry loop");
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
    println!("\nAll unlocked checks passed.");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 9 (CAPSTONE): build a mini-anyhow from scratch.
//
// You've used anyhow; now build its core so the magic disappears. Three pieces:
//
//   (1) `MyError` — an opaque wrapper around `Box<dyn Error + Send + Sync>`.
//       Crucially it implements Display + Debug but NOT std::error::Error.
//       (Why not? If MyError: Error, the blanket `From<E: Error>` below would
//        overlap with std's reflexive `From<MyError> for MyError` — coherence
//        conflict. anyhow::Error makes the exact same choice. State this in a
//        comment when you write it.)
//
//   (2) blanket `impl<E: Error + Send + Sync + 'static> From<E> for MyError`
//       — THIS is what makes `?` accept any std error and erase it. The single
//       most important impl in the whole crate.
//
//   (3) a `Context` extension trait adding `.context(msg)` to any
//       `Result<T, E>`, which on Err wraps the original error as the SOURCE of
//       a new `ContextError { msg, source }`. That's how anyhow stacks a
//       human message on top while preserving the real cause underneath.
//
// Implement every TODO below. No unsafe needed. When done, `chain()` walking
// the .source() links should show your context message on top of the original.
// ─────────────────────────────────────────────────────────────────────────

pub struct MyError(Box<dyn Error + Send + Sync + 'static>);

impl fmt::Debug for MyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for MyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<E: Error + Send + Sync + 'static> From<E> for MyError {
    fn from(e: E) -> Self {
        MyError(Box::new(e))
    }
}

// TODO rung 9 (2): blanket From impl that erases any std error into MyError.
//   impl<E: Error + Send + Sync + 'static> From<E> for MyError { ... }

impl MyError {
    // Walk the source chain, top-first, like anyhow's `{:#}` / .chain().
    fn chain(&self) -> Vec<String> {
        let mut out = vec![self.0.to_string()];
        let mut cur = self.0.source();
        while let Some(e) = cur {
            out.push(e.to_string());
            cur = e.source();
        }
        out
    }
}

// The error type that `.context()` builds: a message whose SOURCE is the
// original error. This is the link that grows the chain.
#[derive(Debug)]
struct ContextError {
    msg: String,
    source: Box<dyn Error + Send + Sync + 'static>,
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl Error for ContextError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source)
    }
}

// Named WrapErr (not Context) to avoid clashing with the anyhow::Context
// trait imported at the top of the file. The method is still `.context(..)`.
trait WrapErr<T> {
    fn context<C: fmt::Display>(self, ctx: C) -> Result<T, MyError>;
}

impl<T, E: Error + Send + Sync + 'static> WrapErr<T> for Result<T, E> {
    fn context<C: fmt::Display>(self, ctx: C) -> Result<T, MyError> {
        // On Ok -> pass through. On Err(e) -> build a ContextError whose source
        // is `e`, then wrap that in MyError.
        self.map_err(|e| {
            MyError(Box::new(ContextError {
                msg: ctx.to_string(),
                source: Box::new(e),
            }))
        })
    }
}

impl<T> WrapErr<T> for Result<T, MyError> {
    fn context<C: fmt::Display>(self, ctx: C) -> Result<T, MyError> {
        self.map_err(|e| {
            MyError(Box::new(ContextError {
                msg: ctx.to_string(),
                source: e.0,
            }))
        })
    }
}

fn parse_positive(s: &str) -> Result<i32, MyError> {
    // `?` here exercises your blanket From<ParseIntError> for MyError.
    let n: i32 = s.parse()?;
    Ok(n)
}

fn check_9() {
    // (2) blanket From + `?` erases a ParseIntError into MyError
    assert_eq!(parse_positive("42").unwrap(), 42);
    let e = parse_positive("abc").unwrap_err();
    assert!(
        e.to_string().contains("invalid digit"),
        "Display delegates to inner"
    );

    // (3) .context() stacks a message; the original survives as the source
    let e = WrapErr::context(parse_positive("abc"), "while parsing the port").unwrap_err();
    let chain = e.chain();
    assert_eq!(
        chain.len(),
        2,
        "context message + underlying error, got {chain:?}"
    );
    assert_eq!(chain[0], "while parsing the port");
    assert!(chain[1].contains("invalid digit"));

    // context composes onto a fresh io error too (proves the blanket bound)
    let io: Result<(), std::io::Error> =
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "boom"));
    // fully-qualified: io::Error satisfies BOTH anyhow::Context and our WrapErr,
    // so we must say which trait's `context` we mean.
    let e = WrapErr::context(io, "loading file").unwrap_err();
    assert_eq!(e.chain()[0], "loading file");
    assert_eq!(e.chain()[1], "boom");

    println!("check_9 ✓  CAPSTONE: rebuilt anyhow's erase-via-From + context chain");
}
