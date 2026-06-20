//! Custom error types — Phase 3
//! Run: cargo run --bin custom_errors
//!
//! Sibling to `error_arch` (which was about thiserror-vs-anyhow architecture).
//! This ladder is about the *machinery* underneath, built BY HAND, no derive
//! macros: what does it actually take to be a "real" error in Rust?
//!
//! The contract:
//!   impl Display  -> a human-readable message
//!   impl Error    -> the it's-an-error marker + an OPTIONAL source() cause
//! Everything else (`?`, Box<dyn Error>, downcasting, chains, anyhow) is built
//! on those two impls + From. The key idea is the *source chain*: each error can
//! point at the lower-level error that caused it — a linked list you walk from
//! "what failed" down to "why".
//!
//! Ladder:
//!   1. [x] foundations  — Display + Error for a minimal struct error
//!   2. [x] foundations  — an error enum: variants with data, per-variant Display
//!   3. [x] mechanics    — source(): wrap a lower-level error, expose the cause
//!   4. [x] mechanics    — From + `?` by hand (what #[from] generates)
//!   5. [x] footgun      — Box<dyn Error> bounds: Send + Sync + 'static
//!   6. [x] footgun      — downcasting: recover the concrete type, find root cause
//!   7. [x] real-world   — capture a Backtrace inside your error
//!   8. [x] real-world   — layered library error + anyhow-style {:#} chain printer
//!   9. [x] capstone     — build a source()-iterator + Report reporter from scratch

use std::error::Error;
use std::fmt;

// ─────────────────────────────────────────────────────────────────────────
// Problem 1 (foundations): the Error trait contract.
//
// Define a custom error type `TooLong` that represents "a username was longer
// than the allowed limit". It should carry the offending length and the limit.
//
// To be a *real* Rust error it must implement TWO things:
//   - `std::fmt::Display`  — e.g. "username too long: 42 chars (max 16)"
//   - `std::error::Error`  — the marker trait. For now the default methods are
//                            fine; an empty `impl Error for TooLong {}` works
//                            BECAUSE Display + Debug are supertraits of Error.
//
// Goal: make `validate_username` return `Result<(), TooLong>`, erroring when the
// name exceeds `max`. Implement Display + Error so check_1 passes.
//
// Note the supertrait bound: `trait Error: Debug + Display`. That's why you also
// need `#[derive(Debug)]` on the struct — Error literally cannot be implemented
// without Debug.
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct TooLong {
    len: usize,
    max: usize,
}

impl fmt::Display for TooLong {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "username too long: {} chars (max {})",
            self.len, self.max
        )
    }
}

// TODO: impl std::error::Error for TooLong {}   (an empty body is enough here)
impl std::error::Error for TooLong {}

fn validate_username(name: &str, max: usize) -> Result<(), TooLong> {
    if name.len() <= max {
        Ok(())
    } else {
        Err(TooLong {
            len: name.len(),
            max,
        })
    }
}

fn check_1() {
    assert!(validate_username("alice", 16).is_ok());

    let err = validate_username("this_name_is_way_too_long", 16).unwrap_err();
    // Display produces a useful message
    let msg = err.to_string();
    assert!(msg.contains("25"), "should mention the length, got: {msg}");
    assert!(msg.contains("16"), "should mention the max, got: {msg}");

    // The payoff of impl Error: it coerces into the universal Box<dyn Error>.
    let _boxed: Box<dyn Error> = Box::new(err);

    println!("check_1 ✓  Display + Error = a real custom error type");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 2 (foundations): a real error type is usually an ENUM.
//
// One struct = one failure mode. Real validators fail several ways. The
// idiomatic shape is one enum with a variant per failure, each carrying the
// data that variant needs, and a Display arm that renders each variant well.
//
// Define `enum ValidationError` with THREE variants:
//   - TooShort { len: usize, min: usize }
//   - TooLong  { len: usize, max: usize }
//   - BadChar  { ch: char }          // an illegal character was found
//
// Then implement Display (match on self, one message per arm) and the empty
// `impl Error`. Implement `validate` to apply the rules in order:
//   1. reject if shorter than `min`
//   2. reject if longer than `max`
//   3. reject if any char is not alphanumeric or '_'  (BadChar with that char)
//
// Goal: check_2 passes. Notice you now have ONE error type the caller can
// `match` on to handle each case differently — that's the whole point of a
// typed custom error over a stringly `Box<dyn Error>`.
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum ValidationError {
    TooShort { len: usize, min: usize },
    TooLong { len: usize, max: usize },
    BadChar { ch: char },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { len, min } => {
                write!(f, "username too short: {len} chars (min {min})")
            }
            Self::TooLong { len, max } => {
                write!(f, "username too long: {len} chars (max {max})")
            }
            Self::BadChar { ch } => write!(f, "username contains illegal character: {ch:?}"),
        }
    }
}

// TODO: impl std::error::Error for ValidationError {}
impl std::error::Error for ValidationError {}

fn validate(name: &str, min: usize, max: usize) -> Result<(), ValidationError> {
    let len = name.len();
    if len < min {
        return Err(ValidationError::TooShort { len, min });
    }

    if len > max {
        return Err(ValidationError::TooLong { len, max });
    }

    if let Some(ch) = name.chars().find(|&ch| !ch.is_alphanumeric() && ch != '_') {
        return Err(ValidationError::BadChar { ch });
    }

    Ok(())
}

fn check_2() {
    assert!(validate("alice_01", 3, 16).is_ok());

    match validate("ab", 3, 16) {
        Err(ValidationError::TooShort { len: 2, min: 3 }) => {}
        other => panic!("expected TooShort, got {other:?}"),
    }
    match validate("ab", 3, 16).unwrap_err().to_string() {
        s if s.contains("short") || s.contains("2") => {}
        s => panic!("TooShort message unhelpful: {s}"),
    }

    let long = "x".repeat(20);
    assert!(matches!(
        validate(&long, 3, 16),
        Err(ValidationError::TooLong { len: 20, max: 16 })
    ));

    match validate("bad name!", 3, 16) {
        // the first illegal char is the space
        Err(ValidationError::BadChar { ch: ' ' }) => {}
        other => panic!("expected BadChar(' '), got {other:?}"),
    }

    println!("check_2 ✓  one enum, many failure modes the caller can match on");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 3 (mechanics): source() — the cause underneath.
//
// So far our errors are "leaf" errors — they originate the failure. But most
// errors WRAP a lower-level one: "failed to load config" *because* "failed to
// parse integer". The `Error` trait models this with ONE optional method:
//
//     fn source(&self) -> Option<&(dyn Error + 'static)> { None }   // default
//
// Override it to return the error underneath. That builds the *source chain* —
// a linked list you (or anyhow, or a logger) can walk to print "X, caused by Y,
// caused by Z".
//
// Scenario: parse a config line like "port=8080" into a u16. Two failure modes:
//   - Malformed: the line has no '=' (a LEAF error — no underlying cause)
//   - BadPort:   the value after '=' didn't parse as u16. Here the REAL cause is
//                a std::num::ParseIntError — STORE it and return it from source().
//
// Define:
//   #[derive(Debug)]
//   enum ConfigError {
//       Malformed { line: String },
//       BadPort   { source: std::num::ParseIntError },   // <- keep the cause
//   }
//
// - Display: a high-level message for each. IMPORTANT: do NOT paste the parse
//   error's text into BadPort's Display — that's source()'s job. Just say e.g.
//   "invalid port number". (Duplicating the cause in Display is a classic
//   anti-pattern; the chain printer in rung 8 would print it twice.)
// - Error: override `source()` to return Some(&the ParseIntError) for BadPort,
//   and None for Malformed (the default).
//
// Implement `parse_config_line(&str) -> Result<u16, ConfigError>`:
//   - split once on '='. No '=' -> Malformed.
//   - parse the right side as u16; on Err, wrap it in BadPort.
//
// Goal: check_3 passes — it asserts the chain is reachable via source().
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum ConfigError {
    Malformed { line: String },
    BadPort { source: std::num::ParseIntError },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed { line } => {
                write!(f, "malformed config line: {line}")
            }
            Self::BadPort { .. } => {
                write!(f, "invalid port number")
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::BadPort { source } => Some(source),
            Self::Malformed { .. } => None,
        }
    }
}

fn parse_config_line(line: &str) -> Result<u16, ConfigError> {
    let value = line.split_once('=');
    match value {
        None => Err(ConfigError::Malformed {
            line: line.to_owned(),
        }),
        Some((_, port)) => port
            .parse::<u16>()
            .map_err(|source| ConfigError::BadPort { source }),
    }
}

fn check_3() {
    assert_eq!(parse_config_line("port=8080").unwrap(), 8080);

    // Malformed: a leaf error, no source.
    let e = parse_config_line("garbage").unwrap_err();
    assert!(e.source().is_none(), "Malformed should have no cause");

    // BadPort: its source() should be the underlying ParseIntError.
    let e = parse_config_line("port=99999").unwrap_err(); // > u16::MAX
    let src = e.source().expect("BadPort should expose its cause");
    // the cause is reachable and is the std parse error
    assert!(
        src.is::<std::num::ParseIntError>(),
        "source should be ParseIntError"
    );
    // top-level Display should NOT have swallowed the cause's text
    assert!(
        !e.to_string().contains("number too large"),
        "don't duplicate the source's message in Display"
    );

    println!("check_3 ✓  source() exposes the cause underneath -> a chain");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 4 (mechanics): make `?` convert INTO your error — From by hand.
//
// In rung 3 you wrapped the cause manually (`.map_err(|e| BadPort { source: e })`
// or an explicit match). The `?` operator can do that conversion for you — but
// only if it knows HOW. The desugaring of `expr?` is roughly:
//
//     match expr {
//         Ok(v)  => v,
//         Err(e) => return Err(From::from(e)),   // <- the magic line
//     }
//
// So `?` calls `From::from` on the error. If you implement
// `From<TheLowLevelError> for YourError`, then `?` will silently convert and
// propagate. THIS is exactly what thiserror's `#[from]` generates.
//
// Build a tiny "load a user record" pipeline that can fail two ways, each from a
// different std error:
//
//   #[derive(Debug)]
//   enum LoadError {
//       Io(std::io::Error),                 // reading failed
//       Parse(std::num::ParseIntError),     // the contents weren't a number
//   }
//
// Implement:
//   - Display + Error (with source() returning the wrapped error for each arm —
//     reuse what you learned in rung 3).
//   - From<std::io::Error>            for LoadError  -> LoadError::Io
//   - From<std::num::ParseIntError>   for LoadError  -> LoadError::Parse
//
// Then write `load_count` so its BODY USES `?` on both an io::Result and a parse
// Result with NO .map_err — the From impls + ? do the conversion:
//
//   fn load_count(raw: &str) -> Result<u64, LoadError> {
//       // simulate I/O: if raw is empty, produce an io::Error and `?` it
//       // otherwise parse it as u64 and `?` it
//   }
//
// To fabricate an io::Error: std::io::Error::new(std::io::ErrorKind::Other, "empty input")
// (or ErrorKind::UnexpectedEof). The point is `?` turning it into LoadError::Io.
//
// Goal: check_4 passes — and notice load_count has zero explicit conversions.
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum LoadError {
    Io(std::io::Error),
    Parse(std::num::ParseIntError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Parse(e) => write!(f, "parse error: {e}"),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Parse(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<std::num::ParseIntError> for LoadError {
    fn from(e: std::num::ParseIntError) -> Self {
        Self::Parse(e)
    }
}

fn load_count(raw: &str) -> Result<u64, LoadError> {
    if raw.is_empty() {
        return Err(LoadError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "empty input",
        )));
    }
    let parse = raw.parse::<u64>()?;
    Ok(parse)
}

fn check_4() {
    assert_eq!(load_count("42").unwrap(), 42);

    // empty -> the io error path, converted by `?` via From
    let e = load_count("").unwrap_err();
    assert!(
        matches!(e, LoadError::Io(_)),
        "empty should be Io, got {e:?}"
    );
    assert!(e.source().is_some(), "Io arm should expose its cause");

    // non-numeric -> the parse path, also converted by `?`
    let e = load_count("seven").unwrap_err();
    assert!(
        matches!(e, LoadError::Parse(_)),
        "non-numeric should be Parse, got {e:?}"
    );
    assert!(e.source().unwrap().is::<std::num::ParseIntError>());

    println!("check_4 ✓  From + `?` auto-convert — this is what #[from] generates");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 5 (footgun): the bounds hiding inside Box<dyn Error>.
//
// You've been writing `Box<dyn Error>`. But the type you ACTUALLY want to return
// from most APIs — and what `fn main() -> Result<(), Box<dyn Error>>` and most of
// the ecosystem use — is the SEND + SYNC + 'static flavor:
//
//     Box<dyn Error + Send + Sync + 'static>
//
// Why? Because an error you can't move to another thread (`Send`) or share
// (`Sync`) is useless to multi-threaded code, async runtimes, etc. anyhow's
// `anyhow::Error` REQUIRES Send + Sync for exactly this reason.
//
// This rung makes the bound VISIBLE by making it fail. Below:
//   - `BoxedSendSync` is the alias most code wants.
//   - `NotThreadSafe` is an error that holds an `Rc<str>`. `Rc` is !Send + !Sync.
//   - `boxed_plain`  boxes it as plain `Box<dyn Error>`  -> compiles fine.
//   - `boxed_shared` tries to box it as `BoxedSendSync`   -> WON'T COMPILE.
//
// Your job:
//   1. Try to write `boxed_shared` returning the alias and `Box::new(err)`-ing a
//      NotThreadSafe. Read the compiler error: it will say `Rc<str>` cannot be
//      sent/shared between threads safely, so NotThreadSafe isn't Send/Sync, so
//      it can't coerce to `dyn Error + Send + Sync`.
//   2. Then FIX it the real-world way: change `NotThreadSafe` to hold an
//      `Arc<str>` instead of `Rc<str>` (Arc IS Send + Sync). Now both functions
//      compile. Leave a one-line comment explaining what changed and why.
//
// The lesson: `+ Send + Sync` isn't noise — it's a promise about thread-mobility
// that the payload types must actually keep.
// ─────────────────────────────────────────────────────────────────────────

use std::sync::Arc;

type BoxedSendSync = Box<dyn Error + Send + Sync + 'static>;

#[derive(Debug)]
struct NotThreadSafe {
    detail: Arc<str>,
}

impl fmt::Display for NotThreadSafe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not-thread-safe error: {}", self.detail)
    }
}
impl std::error::Error for NotThreadSafe {}

fn boxed_plain() -> Box<dyn Error> {
    Box::new(NotThreadSafe {
        detail: "boom".into(),
    })
}

fn boxed_shared() -> BoxedSendSync {
    Box::new(NotThreadSafe {
        detail: "boom".into(),
    })
}

fn check_5() {
    let e = boxed_plain();
    assert!(e.to_string().contains("boom"));

    let e: BoxedSendSync = boxed_shared();
    assert!(e.to_string().contains("boom"));

    let handle = std::thread::spawn(move || e.to_string());
    assert!(handle.join().unwrap().contains("boom"));

    println!("check_5 ✓  Box<dyn Error + Send + Sync> demands thread-safe payloads");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 6 (footgun): downcasting — get the concrete type back out.
//
// `Box<dyn Error>` erases the type. Sometimes you need it BACK: "if the root
// cause was specifically a ParseIntError, retry; otherwise give up." The Error
// trait supports this via `dyn Error`'s inherent methods (built on Any):
//
//     err.is::<T>()              -> bool        (is the concrete type T?)
//     err.downcast_ref::<T>()    -> Option<&T>  (borrow it as T if so)
//
// These work because `Error: 'static`, so the type carries a TypeId.
//
// Two tasks:
//
// (a) `describe_root` — given a `&(dyn Error)`, WALK the source chain to the very
//     bottom (keep calling `.source()` until it returns None) and return the
//     Display string of that deepest error. This is "find the root cause".
//
// (b) `root_is_parse_error` — walk the same chain and return true iff the ROOT
//     cause's concrete type is `std::num::ParseIntError` (use `is::<...>()` on the
//     deepest error). This is the "downcast to decide" pattern.
//
// Reuse your ConfigError from rung 3: `parse_config_line("port=99999")` gives a
// ConfigError::BadPort whose source() is a ParseIntError — a 2-level chain.
//
// Hint for the walk: start with `let mut cur = top;` then
// `while let Some(next) = cur.source() { cur = next; }` leaves `cur` at the root.
//
// Goal: check_6 passes.
// ─────────────────────────────────────────────────────────────────────────

fn describe_root(top: &(dyn Error + 'static)) -> String {
    let mut cur = top;
    while let Some(next) = cur.source() {
        cur = next;
    }
    cur.to_string()
}

fn root_is_parse_error(top: &(dyn Error + 'static)) -> bool {
    let mut cur = top;
    while let Some(next) = cur.source() {
        cur = next;
    }
    cur.is::<std::num::ParseIntError>()
}

fn check_6() {
    // a leaf error: it IS its own root
    let leaf = TooLong { len: 20, max: 16 };
    assert_eq!(describe_root(&leaf), leaf.to_string());
    assert!(!root_is_parse_error(&leaf));

    // a 2-level chain: ConfigError::BadPort -> ParseIntError
    let chained = parse_config_line("port=99999").unwrap_err();
    let root = describe_root(&chained);
    // the root is the ParseIntError, whose message mentions the overflow
    assert!(
        root.contains("too large") || root.contains("invalid"),
        "root should be the parse error, got: {root}"
    );
    assert!(
        root_is_parse_error(&chained),
        "root cause should downcast to ParseIntError"
    );

    println!("check_6 ✓  downcast + chain-walk recovers the concrete root cause");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 7 (real-world): capture a Backtrace at the point of failure.
//
// A source chain tells you the "logical" why (X because Y because Z). A
// BACKTRACE tells you the "physical" where — the call stack at the moment the
// error was created. anyhow attaches one automatically; thiserror exposes a
// `#[backtrace]` field. Here you wire it by hand with `std::backtrace::Backtrace`.
//
// Two capture APIs, and the difference MATTERS:
//   - Backtrace::capture()        -> respects the RUST_BACKTRACE / RUST_LIB_BACKTRACE
//                                    env vars. If unset, you get a cheap *disabled*
//                                    backtrace (status() == Disabled). This is what
//                                    you want in real libs: zero cost unless the user
//                                    opts in.
//   - Backtrace::force_capture()  -> ALWAYS walks the stack, ignoring env. Expensive;
//                                    use when you truly always want it.
//
// Build an error that carries a backtrace captured WHERE IT WAS CREATED:
//
//   #[derive(Debug)]
//   struct TracedError {
//       msg: String,
//       backtrace: Backtrace,
//   }
//
// - A constructor `TracedError::new(msg)` that captures `Backtrace::force_capture()`
//   into the field (force_, so the test is deterministic regardless of env).
// - Display: just the msg (a backtrace is NOT part of the human message; it's
//   diagnostic data you print separately / on demand).
// - impl Error: override `fn backtrace(&self)`? NOTE: `Error::backtrace` is still
//   UNSTABLE on stable Rust, so DON'T override it. Instead expose your own
//   inherent getter `fn backtrace(&self) -> &Backtrace { &self.backtrace }`.
//
// Goal: check_7 passes — it checks the captured backtrace has status `Captured`
// and that its rendering is non-empty.
// ─────────────────────────────────────────────────────────────────────────

use std::backtrace::{Backtrace, BacktraceStatus};

#[derive(Debug)]
struct TracedError {
    msg: String,
    backtrace: Backtrace,
}

impl TracedError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            msg: msg.into(),
            backtrace: Backtrace::force_capture(),
        }
    }

    fn backtrace(&self) -> &Backtrace {
        &self.backtrace
    }
}

impl fmt::Display for TracedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for TracedError {}

fn check_7() {
    let e = TracedError::new("disk on fire");
    assert_eq!(e.to_string(), "disk on fire");

    // force_capture() always captures, so status is Captured regardless of env.
    assert_eq!(e.backtrace().status(), BacktraceStatus::Captured);

    // and it actually rendered some frames
    let rendered = format!("{}", e.backtrace());
    assert!(
        !rendered.is_empty(),
        "a captured backtrace should render frames"
    );

    println!("check_7 ✓  backtrace captures WHERE the error was born");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 8 (real-world): a layered library error + a chain printer.
//
// This is the shape a real library ships: ONE public error enum whose variants
// each wrap a DIFFERENT lower-level error, a correct Error impl exposing every
// cause via source(), and a way to render the whole chain like anyhow does.
//
// The domain: loading app config from a file.
//   read the file        -> can fail with std::io::Error
//   parse a "port=N" line -> can fail with your ConfigError (rung 3), which
//                            itself wraps a ParseIntError. So this is a THREE-level
//                            chain:  AppError -> ConfigError -> ParseIntError.
//
// (a) Define the public error:
//   #[derive(Debug)]
//   enum AppError {
//       Read { path: String, source: std::io::Error },
//       Config { source: ConfigError },
//   }
//   - Display: a HIGH-LEVEL message only ("failed to read config file `foo`",
//     "invalid configuration") — do NOT restate the source (you learned why in
//     rung 3: the chain printer would duplicate it).
//   - Error::source(): return Some(&the wrapped error) for each variant.
//
// (b) Write `format_chain(err: &dyn Error) -> String` — the anyhow `{:#}` style:
//   render the top error, then ": " + each successive source, walked to the root.
//   e.g.  "invalid configuration: invalid port number: number too large to fit in target type"
//   (top Display, then each source's Display, joined by ": ").
//
//   Walk it the same way as rung 6, but COLLECT each level's Display:
//     start a String with top.to_string(); then
//     let mut cur = top.source(); while let Some(e) = cur { push ": "+e; cur = e.source(); }
//
// (c) Implement `load_app_config(path, contents)`:
//   - if `path` ends with ".missing", simulate a read failure: build an
//     io::Error (NotFound) and return AppError::Read { path, source }.
//   - otherwise feed `contents` to parse_config_line; on Err wrap into
//     AppError::Config { source }.  (use `?` + a From impl, OR map_err — your call)
//   - on success return the u16 port.
//
// Goal: check_8 passes — it asserts the 3-level chain renders correctly AND that
// the top-level Display alone does NOT contain the root's text (no duplication).
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum AppError {
    Read {
        path: String,
        source: std::io::Error,
    },
    Config {
        source: ConfigError,
    },
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Read { path, .. } => write!(f, "failed to read config file {path}"),
            AppError::Config { .. } => write!(f, "invalid configuration"),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            AppError::Read { source, .. } => Some(source),
            AppError::Config { source } => Some(source),
        }
    }
}

fn format_chain(err: &dyn Error) -> String {
    let mut chain = err.to_string();
    let mut cur = err.source();
    while let Some(next) = cur {
        chain.push_str(&format!(": {}", next.to_string()));
        cur = next.source();
    }
    chain
}

fn load_app_config(path: &str, contents: &str) -> Result<u16, AppError> {
    if path.ends_with(".missing") {
        Err(AppError::Read {
            path: path.to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        })
    } else {
        parse_config_line(contents).map_err(|e| AppError::Config { source: e })
    }
}

fn check_8() {
    assert_eq!(load_app_config("app.conf", "port=8080").unwrap(), 8080);

    // read-failure path: 2-level chain  AppError::Read -> io::Error
    let e = load_app_config("app.missing", "port=8080").unwrap_err();
    assert!(matches!(e, AppError::Read { .. }));
    let chain = format_chain(&e);
    assert!(chain.starts_with("failed to read"), "got: {chain}");
    assert!(
        chain.contains(": "),
        "should join the io cause with ': ', got: {chain}"
    );

    // config path: 3-level chain  AppError::Config -> ConfigError::BadPort -> ParseIntError
    let e = load_app_config("app.conf", "port=99999").unwrap_err();
    assert!(matches!(e, AppError::Config { .. }));
    let chain = format_chain(&e);
    // all three levels present, in order, joined by ": "
    let colons = chain.matches(": ").count();
    assert!(colons >= 2, "expected a 3-level chain, got: {chain}");
    assert!(
        chain.contains("too large") || chain.contains("invalid digit"),
        "root parse error should be the tail, got: {chain}"
    );
    // and the TOP display alone must not already contain the root's text
    assert!(
        !e.to_string().contains("too large"),
        "top-level Display must not duplicate the root cause"
    );

    println!("check_8 ✓  layered error + anyhow-style {{:#}} chain rendering");
}

// ─────────────────────────────────────────────────────────────────────────
// Problem 9 (capstone): build the core of anyhow's reporter from scratch.
//
// You've used `.source()` walks three times now. anyhow/eyre package that into
// two reusable pieces. Build both yourself — this proves you own the model.
//
// PART A — `Chain<'a>`, an Iterator over a source chain.
//   std added `Error::sources()` (still unstable); you're building the same
//   thing. Each `next()` yields the current error, then advances to its source.
//
//   struct Chain<'a> { next: Option<&'a (dyn Error + 'static)> }
//
//   impl<'a> Iterator for Chain<'a> {
//       type Item = &'a (dyn Error + 'static);
//       fn next(&mut self) -> Option<Self::Item> {
//           // take the current; set self.next to current.source(); return current
//       }
//   }
//
//   Hint: `let cur = self.next.take()?;` then `self.next = cur.source();` then
//   `Some(cur)`. The `?` on the Option ends iteration when the chain is exhausted.
//
//   Provide a constructor `fn chain(err: &dyn Error) -> Chain<'_>`.
//
// PART B — `Report<'a>`, a Display wrapper that renders anyhow's multi-line form.
//   For a chain [top, c1, c2] it prints EXACTLY:
//
//       top
//
//       Caused by:
//           0: c1
//           1: c2
//
//   Rules:
//     - first line: the top error's Display.
//     - if there are NO further sources, stop there (no "Caused by:" block).
//     - otherwise a blank line, then "Caused by:", then each subsequent error on
//       its own line indented "    {i}: {err}" where i starts at 0 for the FIRST
//       source (not the top). No trailing newline.
//
//   struct Report<'a>(&'a (dyn Error + 'static));
//   impl fmt::Display for Report<'a> { ... }   // BUILD ON your Chain iterator
//
//   Use your Chain to get all links. links[0] is the top; links[1..] are causes.
//
// Goal: check_9 passes — exact-match on both the single-error and 3-level forms.
// ─────────────────────────────────────────────────────────────────────────

struct Chain<'a> {
    next: Option<&'a (dyn Error + 'static)>,
}

impl<'a> Iterator for Chain<'a> {
    type Item = &'a (dyn Error + 'static);
    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next.take()?;
        self.next = current.source();
        Some(current)
    }
}

fn chain<'a>(err: &'a (dyn Error + 'static)) -> Chain<'a> {
    Chain { next: Some(err) }
}

struct Report<'a>(&'a (dyn Error + 'static));

impl fmt::Display for Report<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.to_string())?;
        if self.0.source().is_some() {
            write!(f, "\n\nCaused by:")?;
            for (i, source) in chain(self.0).skip(1).enumerate() {
                write!(f, "\n    {i}: {}", source.to_string())?;
            }
        }
        Ok(())
    }
}

fn check_9() {
    // single error, no sources -> just its Display, no "Caused by:"
    let leaf = TooLong { len: 20, max: 16 };
    let report = format!("{}", Report(&leaf));
    assert_eq!(report, "username too long: 20 chars (max 16)");

    // Chain iterator length: leaf chain has exactly 1 link
    assert_eq!(chain(&leaf).count(), 1);

    // 3-level chain via rung 8's AppError -> ConfigError -> ParseIntError
    let e = load_app_config("app.conf", "port=99999").unwrap_err();
    assert_eq!(
        chain(&e).count(),
        3,
        "AppError->ConfigError->ParseIntError = 3 links"
    );

    let report = format!("{}", Report(&e));
    let mut lines = report.lines();
    assert_eq!(lines.next().unwrap(), "invalid configuration");
    assert_eq!(lines.next().unwrap(), "");
    assert_eq!(lines.next().unwrap(), "Caused by:");
    // first cause is index 0
    let l = lines.next().unwrap();
    assert_eq!(l, "    0: invalid port number", "got: {l:?}");
    // second cause (the root ParseIntError) is index 1
    let l = lines.next().unwrap();
    assert!(l.starts_with("    1: "), "got: {l:?}");
    assert!(
        l.contains("too large"),
        "root cause on the last line, got: {l:?}"
    );

    println!("check_9 ✓  CAPSTONE: hand-built Chain iterator + anyhow-style Report");
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
