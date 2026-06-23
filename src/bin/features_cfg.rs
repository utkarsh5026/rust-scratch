// Cargo features & `cfg` — conditional compilation
// Run:           cargo run --bin features_cfg
// With features: cargo run --bin features_cfg --features demo
//
// Two faces of the same idea:
//   #[cfg(...)]  = an ATTRIBUTE. Includes/excludes an item entirely. Excluded
//                  code is never compiled (can be broken Rust until the cfg flips).
//   cfg!(...)    = a MACRO. Evaluates to a `bool` at runtime; BOTH arms compile.
// Features = named cfg flags declared in Cargo.toml `[features]`, turned on with
//   --features. The one law: features must be ADDITIVE (on may only add, never
//   remove/change behavior) — Cargo compiles the UNION of all requested features.
//
// Ladder (DONE marks finished rungs):
//   1. cfg!(...) macro — runtime bool, both arms compile          [foundations]
//   2. #[cfg(...)] attribute — gate two impls, only one exists    [foundations]
//   3. first feature flag — Cargo.toml [features] + #[cfg(feature)][mechanics]
//   4. cfg_attr — conditionally apply an attribute (a derive)     [mechanics]
//   5. additivity — a non-additive feature breaks; redesign it    [footgun]
//   6. missing-symbol trap — cfg the caller too (E0425/E0432)     [footgun]
//   7. optional dependency as a feature (serde, dep:)             [real-world]
//   8. feature graphs — default, feature-enables-feature          [real-world]
//   9. capstone: mini platform/config module, full cfg matrix     [capstone]

#[cfg(feature = "dice")]
use rand::RngExt;

// ───────────────────────────── Rung 1 ─────────────────────────────
// cfg!(...) is a MACRO that becomes a plain `bool`. Both branches you write
// around it are always compiled — it just picks one at runtime.
//
// Your turn: return a short String describing the build, using ONLY cfg!:
//   - if cfg!(debug_assertions) -> contains "debug", else "release"
//   - append " on linux" iff cfg!(target_os = "linux")
// e.g. on a debug linux build: "debug on linux"
fn build_profile() -> String {
    let mut s = String::new();
    if cfg!(debug_assertions) {
        s.push_str("debug");
    } else {
        s.push_str("release");
    }
    if cfg!(target_os = "linux") {
        s.push_str(" on linux");
    }
    s
}

fn check_1() {
    let p = build_profile();
    // We can't hardcode the platform, so assert the invariants instead:
    let expect_debug = cfg!(debug_assertions);
    assert_eq!(
        p.contains("debug"),
        expect_debug,
        "debug-ness must match cfg!"
    );
    assert_eq!(
        p.contains("release"),
        !expect_debug,
        "release-ness must match cfg!"
    );
    assert_eq!(
        p.contains("linux"),
        cfg!(target_os = "linux"),
        "linux tag must match cfg!"
    );
    println!("check_1 ✓  build_profile() = {p:?}");
}

// ───────────────────────────── Rung 2 ─────────────────────────────
// Now the ATTRIBUTE form. #[cfg(pred)] decides whether an item EXISTS at all.
// If the predicate is false the item is never compiled — so you can write TWO
// definitions of the same function gated by mutually-exclusive predicates, and
// exactly one survives. The other could even be broken Rust; nobody checks it.
//
// Your turn: write `platform_tag() -> &'static str` TWICE:
//   - one #[cfg(target_pointer_width = "64")] returning "64-bit"
//   - one #[cfg(not(target_pointer_width = "64"))] returning "non-64-bit"
// Only one is compiled, so there's no "duplicate definition" error.
// (Try flipping one predicate to the SAME value as the other and rerun — you'll
//  see E0428 "defined multiple times": that's your proof only one normally exists.)

#[cfg(target_pointer_width = "64")]
fn platform_tag() -> &'static str {
    "64-bit"
}

// TODO: add the second, mutually-exclusive definition here with
#[cfg(not(target_pointer_width = "64"))]
fn platform_tag() -> &'static str {
    "non-64-bit"
}

fn check_2() {
    let tag = platform_tag();
    let expect = if cfg!(target_pointer_width = "64") {
        "64-bit"
    } else {
        "non-64-bit"
    };
    assert_eq!(tag, expect, "platform_tag must match the compiled cfg");
    println!("check_2 ✓  platform_tag() = {tag:?}");
}

// ───────────────────────────── Rung 3 ─────────────────────────────
// A FEATURE is just a cfg flag you declare yourself. Unlike target_os/etc.,
// `feature = "..."` is unknown to Cargo until you list it under [features] in
// Cargo.toml. Then `--features demo` turns it on.
//
// STEP 1 (Cargo.toml): add a section
//     [features]
//     demo = []
//   (empty list = "this feature enables no other features / optional deps yet")
//
// STEP 2 (here): implement `feature_line()` behind #[cfg(feature = "demo")] to
//   return "demo enabled". There is NO non-demo definition — when the feature is
//   off, the function simply does not exist. check_3 below already cfg-guards the
//   call so the file still compiles+runs without the feature.
//
// Verify BOTH builds:
//     cargo run --bin features_cfg                      # demo off
//     cargo run --bin features_cfg --features demo      # demo on

#[cfg(feature = "demo")]
fn feature_line() -> &'static str {
    "demo enabled"
}

fn check_3() {
    #[cfg(feature = "demo")]
    {
        assert_eq!(feature_line(), "demo enabled");
        println!(
            "check_3 ✓  (demo ON)  feature_line() = {:?}",
            feature_line()
        );
    }
    #[cfg(not(feature = "demo"))]
    {
        // feature_line() doesn't even exist here — calling it would be E0425.
        println!("check_3 ✓  (demo OFF) feature_line() not compiled in");
    }
}

// ───────────────────────────── Rung 4 ─────────────────────────────
// cfg_attr CONDITIONALLY APPLIES ANOTHER ATTRIBUTE. Shape:
//     #[cfg_attr(PREDICATE, attr1, attr2, ...)]
// reads as: "if PREDICATE holds, expand to #[attr1] #[attr2] ...; else nothing."
// This is how crates add `#[derive(Serialize)]` ONLY when a serde feature is on,
// without duplicating the whole struct.
//
// Your turn:
//   - Put `#[derive(Debug, Clone)]` on `Point` UNCONDITIONALLY (always wanted).
//   - Then add `#[cfg_attr(feature = "demo", derive(PartialEq))]` so Point gains
//     a PartialEq impl ONLY when the `demo` feature is on.
// (You already declared `demo` in rung 3, so no Cargo.toml change needed.)
//
// check_4 proves it: the equality comparison `p == p` only compiles under demo,
// so it's itself behind #[cfg(feature = "demo")].

// TODO: add the derive + cfg_attr lines on the next line
#[derive(Debug, Clone)]
#[cfg_attr(feature = "demo", derive(PartialEq))]
#[allow(dead_code)]
struct Point {
    x: i32,
    y: i32,
}

fn check_4() {
    let p = Point { x: 1, y: 2 };
    // Debug+Clone are always available:
    let _q = p.clone();
    println!("check_4 ✓  Point = {:?}", p);
    #[cfg(feature = "demo")]
    {
        // PartialEq only exists when demo is on (granted by cfg_attr).
        assert!(p == _q, "PartialEq should hold for cloned Point");
        println!("check_4 ✓  (demo ON) Point: PartialEq available, p == clone");
    }
}

// ───────────────────────────── Rung 5 ─────────────────────────────
// THE ADDITIVITY LAW. Cargo compiles your crate ONCE with the UNION of every
// feature any crate in the graph requested. So a feature must only ADD behavior.
//
// STEP 0 (Cargo.toml): add two more features:
//     metric   = []
//     imperial = []
//
// PART A — the ANTI-PATTERN (mutually exclusive features). Uncomment these two
// and try to build. Each alone is fine; but they ASSUME you never enable both:
//
//     #[cfg(feature = "metric")]
//     fn unit_label() -> &'static str { "meters" }
//     #[cfg(feature = "imperial")]
//     fn unit_label() -> &'static str { "feet" }
//
//   Run the build matrix:
//     cargo run --bin features_cfg --features metric              # ok
//     cargo run --bin features_cfg --features imperial           # ok
//     cargo run --bin features_cfg --features metric,imperial    # 💥 E0428 / breakage
//   That last one is what feature unification can FORCE on you — two crates each
//   enabling one of these, and your build is suddenly broken through no fault of
//   either. Witness the error, then RE-COMMENT the two fns (leave them off).
//
// PART B — the ADDITIVE FIX. Instead of one-or-the-other, ACCUMULATE. Implement
//   `enabled_units() -> Vec<&'static str>` that pushes "meters" when the metric
//   feature is on AND "feet" when imperial is on. Enabling both now just yields
//   BOTH — composition, no conflict. (Use cfg!(feature = "...") to test each.)

fn enabled_units() -> Vec<&'static str> {
    let mut units = Vec::new();
    if cfg!(feature = "metric") {
        units.push("meters");
    }
    if cfg!(feature = "imperial") {
        units.push("feet")
    }
    units
}

fn check_5() {
    let got = enabled_units();
    let mut expect: Vec<&'static str> = Vec::new();
    if cfg!(feature = "metric") {
        expect.push("meters");
    }
    if cfg!(feature = "imperial") {
        expect.push("feet");
    }
    assert_eq!(
        got, expect,
        "enabled_units must additively reflect the on features"
    );
    println!("check_5 ✓  enabled_units() = {got:?}  (additive: both features compose)");
}

// ───────────────────────────── Rung 6 ─────────────────────────────
// THE MISSING-SYMBOL TRAP. If an item is gated behind a feature, every USE of it
// must be gated too — or the no-feature build won't compile (E0425 unresolved
// name / E0432 unresolved import). The lesson: cfg-ing the definition is only
// half the job; the call site is the other half.
//
// Below, `pretty_print` exists only under feature "demo". `describe()` must use
// it when demo is on, but fall back to a plain format when it's off — WITHOUT
// referring to pretty_print in the off-build (where it doesn't exist).
//
// FIRST: do the WRONG thing to feel the trap. Make describe() call pretty_print()
//   unconditionally, then `cargo run --bin features_cfg` (demo OFF) → E0425
//   "cannot find function `pretty_print`". That error IS the lesson.
// THEN fix it: gate the two code paths so each build only names what it has.
//   describe() must return:
//     - demo ON : pretty_print(label, n)   e.g. "[ answer = 42 ]"
//     - demo OFF: format!("{label}={n}")    e.g. "answer=42"

#[cfg(feature = "demo")]
fn pretty_print(label: &str, n: i32) -> String {
    format!("[ {label} = {n} ]")
}

fn describe(label: &str, n: i32) -> String {
    #[cfg(feature = "demo")]
    {
        pretty_print(label, n)
    }
    #[cfg(not(feature = "demo"))]
    {
        format!("{label}={n}")
    }
}

fn check_6() {
    let out = describe("answer", 42);
    let expect = if cfg!(feature = "demo") {
        "[ answer = 42 ]"
    } else {
        "answer=42"
    };
    assert_eq!(out, expect, "describe must adapt to the demo feature");
    println!("check_6 ✓  describe(\"answer\", 42) = {out:?}");
}

// ───────────────────────────── Rung 7 ─────────────────────────────
// OPTIONAL DEPENDENCY AS A FEATURE. A dep you only sometimes need is marked
// `optional = true` (not compiled by default), then pulled in by a feature via
// the `dep:` syntax.
//
// Cargo.toml — your turn:
//   1. Change the rand line to:   rand = { version = "0.10", optional = true }
//   2. Under [features] add:      dice = ["dep:rand"]
//      (`dep:rand` enables the optional crate WITHOUT exposing an implicit
//       `rand` feature on your public surface.)
//
// Here — implement `roll_die()` gated behind #[cfg(feature = "dice")], using
//   rand 0.10 API: rand::rng().random_range(1..=6)  -> a u32 in 1..=6.
//
// Verify:
//   cargo run --bin features_cfg                 # dice off: rand not even compiled
//   cargo run --bin features_cfg --features dice # dice on:  rolls 1..=6

#[cfg(feature = "dice")]
fn roll_die() -> u32 {
    rand::rng().random_range(1..=6)
}

fn check_7() {
    #[cfg(feature = "dice")]
    {
        let r = roll_die();
        assert!((1..=6).contains(&r), "die roll must be in 1..=6, got {r}");
        println!("check_7 ✓  (dice ON)  roll_die() = {r}");
    }
    #[cfg(not(feature = "dice"))]
    {
        println!("check_7 ✓  (dice OFF) rand crate not compiled in at all");
    }
}

// ───────────────────────────── Rung 8 ─────────────────────────────
// FEATURE GRAPHS. Cargo.toml — your turn, under [features]:
//     default = ["metric"]            # on automatically; --no-default-features strips it
//     color   = []                    # a new leaf feature
//     full    = ["color", "demo"]     # umbrella: enabling `full` transitively turns on both
//
// Here — implement `active_features()`: return a Vec<&'static str> of the names
//   among {"metric","color","demo","dice","imperial"} whose feature is on
//   (test each with cfg!(feature = "...")). Order them as listed.
//
// Explore the graph (watch the active set change):
//     cargo run --bin features_cfg                                 # default => metric on
//     cargo run --bin features_cfg --no-default-features           # metric GONE
//     cargo run --bin features_cfg --features full                 # + color + demo (transitive)
//     cargo run --bin features_cfg --no-default-features --features full
// check_8 also ASSERTS the transitive law: if `full` is on, color AND demo must be on.

fn active_features() -> Vec<&'static str> {
    let mut active = Vec::new();
    if cfg!(feature = "metric") {
        active.push("metric");
    }
    if cfg!(feature = "color") {
        active.push("color");
    }
    if cfg!(feature = "demo") {
        active.push("demo");
    }
    if cfg!(feature = "dice") {
        active.push("dice");
    }
    if cfg!(feature = "imperial") {
        active.push("imperial");
    }
    active
}

fn check_8() {
    if cfg!(feature = "full") {
        assert!(
            cfg!(feature = "color"),
            "full must transitively enable color"
        );
        assert!(cfg!(feature = "demo"), "full must transitively enable demo");
        println!("check_8 ✓  full ⇒ {{color, demo}} (transitive enabling holds)");
    }
    let active = active_features();
    // metric is a DEFAULT feature, so a plain build must include it:
    assert_eq!(active.contains(&"metric"), cfg!(feature = "metric"));
    println!("check_8 ✓  active features = {active:?}");
}

// ───────────────────────────── Rung 9 — CAPSTONE ─────────────────────────────
// A mini config/output layer that exercises the WHOLE matrix at once.
//
// Cargo.toml — add two ADDITIVE output features:
//     json   = []
//     pretty = []
//
// Implement everything in `mod config` below:
//
//  (a) config_dir() -> &'static str  — PLATFORM path via the #[cfg(target_os)]
//      ATTRIBUTE form. Provide at least:
//        #[cfg(target_os = "linux")]            => "/etc/app"
//        #[cfg(not(target_os = "linux"))]       => "/tmp/app"   (fallback)
//      (only one is compiled — rung 2).
//
//  (b) Report struct — Debug always; serde Serialize ONLY under `json` via
//      cfg_attr (rung 4):
//        #[cfg_attr(feature = "json", derive(serde::Serialize))]
//
//  (c) render(&Report) -> Vec<String> — ADDITIVE (rung 5). Push a line per
//      enabled format, gating each branch with #[cfg(feature = "...")] so the
//      off-build never names the missing machinery (rung 6):
//        - json on   => serde_json::to_string(r).unwrap()      (valid JSON)
//        - pretty on  => format!("{} @ level {}", r.name, r.level)
//        - if NEITHER => exactly one fallback line: format!("{:?}", r)
//
//  (d) build_report() -> String — cfg!-driven (rung 1): a one-liner naming the
//      platform tag + which output features are active. (Free-form; just non-empty.)
//
// Build matrix to try:
//     cargo run --bin features_cfg
//     cargo run --bin features_cfg --features json
//     cargo run --bin features_cfg --features pretty
//     cargo run --bin features_cfg --features json,pretty   # BOTH lines, additive

mod config {
    #[cfg(target_os = "linux")]
    pub fn config_dir() -> &'static str {
        "/etc/app"
    }

    #[cfg(not(target_os = "linux"))]
    pub fn config_dir() -> &'static str {
        "/tmp/app"
    }

    #[derive(Debug)]
    #[cfg_attr(feature = "json", derive(serde::Serialize))]
    #[allow(dead_code)]
    pub struct Report {
        pub name: &'static str,
        pub level: u8,
    }

    pub fn render(r: &Report) -> Vec<String> {
        let mut lines = Vec::new();

        #[cfg(feature = "json")]
        {
            lines.push(serde_json::to_string(r).unwrap());
        }

        #[cfg(feature = "pretty")]
        {
            lines.push(format!("{} @ level {}", r.name, r.level));
        }

        #[cfg(not(any(feature = "json", feature = "pretty")))]
        {
            lines.push(format!("{:?}", r));
        }

        lines
    }

    pub fn build_report() -> String {
        let platform = if cfg!(target_os = "linux") {
            "linux"
        } else {
            "non-linux"
        };
        let outputs = match (cfg!(feature = "json"), cfg!(feature = "pretty")) {
            (true, true) => "json,pretty",
            (true, false) => "json",
            (false, true) => "pretty",
            (false, false) => "debug",
        };
        format!("{platform} outputs={outputs}")
    }
}

fn check_9() {
    use config::*;

    // (a) platform path
    let dir = config_dir();
    let expect_dir = if cfg!(target_os = "linux") {
        "/etc/app"
    } else {
        "/tmp/app"
    };
    assert_eq!(
        dir, expect_dir,
        "config_dir must match the compiled target_os"
    );

    // (c) additive rendering
    let r = Report {
        name: "svc",
        level: 3,
    };
    let lines = render(&r);
    let mut expected = 0;
    if cfg!(feature = "json") {
        expected += 1;
    }
    if cfg!(feature = "pretty") {
        expected += 1;
    }
    if expected == 0 {
        expected = 1;
    } // plain fallback
    assert_eq!(
        lines.len(),
        expected,
        "render must emit one line per enabled output feature"
    );

    // when json is on, one line must be parseable JSON carrying the data
    #[cfg(feature = "json")]
    {
        let json_line = lines
            .iter()
            .find(|l| l.trim_start().starts_with('{'))
            .expect("json feature on => a JSON line");
        let v: serde_json::Value = serde_json::from_str(json_line).expect("valid JSON");
        assert_eq!(v["name"], "svc");
        assert_eq!(v["level"], 3);
    }

    // (d) build report is non-empty
    let rep = build_report();
    assert!(!rep.is_empty(), "build_report must say something");

    println!("check_9 ✓  dir={dir:?}  lines={lines:?}");
    println!("check_9 ✓  build_report() = {rep}");
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
