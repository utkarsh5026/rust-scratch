// API evolution & semver  —  what breaks downstream, and how to future-proof.
// Run:  cargo run --bin semver
//
// Mental model: a change is BREAKING (needs a major bump pre-1.0: 0.x.0; post-1.0:
// X.0.0) if ANY valid downstream crate could stop compiling/linking or change
// behavior after a `cargo update` inside the same major. Rust's exhaustiveness,
// type inference, and auto-traits make that set much bigger than "I deleted a fn".
// Authoritative rules: https://doc.rust-lang.org/cargo/reference/semver.html
//
// Ladder (mark DONE as you go):
//   1. [DONE] Foundations  — required_bump(&Change): encode the canonical rules
//   2. [DONE] Foundations  — struct-literal break: adding a pub field
//   3. [DONE] Mechanics    — #[non_exhaustive] structs
//   4. [DONE] Mechanics    — adding an enum variant; #[non_exhaustive] enums
//   5. [DONE] Footgun      — trait evolution: required method vs defaulted; .into() ambiguity
//   6. [DONE] Footgun      — sealed traits (Sealed supertrait pattern)
//   7. [DONE] Real-world   — auto-trait leakage (Send/Sync silently dropped)
//   8. [DONE] Real-world   — generic bounds & params: loosen vs tighten, defaulted params
//   9. [TODO] Capstone     — ApiChange->Bump engine + a fully future-proofed module

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
    println!("all checks passed ✔");
}

// ───────────────────────────── Rung 1 — the three numbers ──────────────────────
//
// Semantic versioning: MAJOR.MINOR.PATCH.
//   - MAJOR  bump = a BREAKING change (downstream may fail to compile).
//   - MINOR  bump = backwards-compatible ADDITION (new API, nothing breaks).
//   - PATCH  bump = backwards-compatible bug fix (no API surface change).
//
// Your turn: implement `required_bump` so each Change maps to the smallest
// version bump that is actually required. These six are the canonical,
// unambiguous cases — the subtle ones come in later rungs.

#[derive(Debug, PartialEq, Eq)]
enum Bump {
    Major,
    Minor,
    Patch,
}

#[derive(Debug)]
enum Change {
    /// Fixed an internal bug; no public item changed.
    BugFixInternal,
    /// Made an existing function faster; same signature, same result.
    PerfImprovement,
    /// Added a brand-new public function.
    AddPublicFunction,
    /// Added `#[deprecated]` to a function that still exists and works.
    DeprecatePublicFunction,
    /// Removed a public function entirely.
    RemovePublicFunction,
    /// Renamed a public function (old name gone, new name added).
    RenamePublicFunction,
}

fn required_bump(change: &Change) -> Bump {
    match change {
        Change::BugFixInternal => Bump::Patch,
        Change::PerfImprovement => Bump::Patch,
        Change::AddPublicFunction => Bump::Minor,
        Change::DeprecatePublicFunction => Bump::Minor,
        Change::RemovePublicFunction => Bump::Major,
        Change::RenamePublicFunction => Bump::Major,
    }
}

fn check_1() {
    assert_eq!(required_bump(&Change::BugFixInternal), Bump::Patch);
    assert_eq!(required_bump(&Change::PerfImprovement), Bump::Patch);
    assert_eq!(required_bump(&Change::AddPublicFunction), Bump::Minor);
    assert_eq!(required_bump(&Change::DeprecatePublicFunction), Bump::Minor);
    assert_eq!(required_bump(&Change::RemovePublicFunction), Bump::Major);
    assert_eq!(required_bump(&Change::RenamePublicFunction), Bump::Major);
    println!("rung 1 ✔  semver bumps classified");
}

// ──────────────────────── Rung 2 — the struct-literal landmine ──────────────────
//
// THE FOOTGUN. Imagine your v1.0 library shipped this:
//
//     pub struct RgbColor { pub r: u8, pub g: u8, pub b: u8 }
//
// Downstream code is free to build it with a struct literal and to *exhaustively*
// destructure it:
//
//     let c = RgbColor { r: 255, g: 0, b: 0 };
//     let RgbColor { r, g, b } = c;
//
// Now v1.1 wants an alpha channel, so you add `pub a: u8`. That single addition is
// a BREAKING change: every downstream struct literal now fails with E0063 (missing
// field `a`) and every exhaustive destructure fails with E0027 (pattern missing
// field `a`). A "minor" feature just forced a major bump.
//
// STEP A — feel the pain (do this, then undo it): temporarily uncomment the
// `pub a: u8` line in `BadColor` below and the literal in `feel_the_pain`, run,
// and read the E0063 / E0027 errors. Then re-comment them.
//
// STEP B — the fix (this is the rung): the future-proof design makes the fields
// PRIVATE and hands out a constructor + accessor. With no public fields, downstream
// *cannot* write a fragile literal or exhaustive destructure, so you can add fields
// forever without breaking anyone. Implement `RgbColor::rgb` and `channels`.

#[allow(dead_code)]
pub struct BadColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    // pub a: u8,   // STEP A: uncomment to watch downstream break
}

#[allow(dead_code)]
fn feel_the_pain() {
    let _c = BadColor {
        r: 255,
        g: 0,
        b: 0, /*, a: 255 */
    };
    // let BadColor { r, g, b } = _c;   // STEP A: exhaustive destructure also breaks
}

// v1.1-ready: private fields, so no downstream literal/destructure can exist.
pub struct RgbColor {
    r: u8,
    g: u8,
    b: u8,
    #[allow(dead_code)]
    a: u8,
    // Later you could add `a: u8` here and NOT break `check_2` — try it once green.
}

impl RgbColor {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn channels(&self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }
}

fn check_2() {
    // "Downstream" code. Note it goes through rgb()/channels(), never a literal —
    // that indirection is exactly what makes adding a field non-breaking.
    let c = RgbColor::rgb(255, 0, 0);
    assert_eq!(c.channels(), (255, 0, 0));
    println!("rung 2 ✔  private fields keep struct growth non-breaking");
}

// ──────────────────────── Rung 3 — #[non_exhaustive] structs ────────────────────
//
// Rung 2's fix (all-private fields) costs you ergonomics: downstream must call a
// getter for every read. Sometimes you want users to read `cfg.timeout_ms` directly
// AND keep the freedom to add fields later. `#[non_exhaustive]` is that contract.
//
// On a struct, `#[non_exhaustive]` tells OTHER crates: you may read the public
// fields, but you may NOT
//   (a) build it with a struct literal  ->  `ClientConfig { .. }`  is forbidden
//   (b) destructure it exhaustively      ->  must end the pattern with `..`
// so the only way for them to construct one is a constructor YOU provide. That means
// adding a field later never breaks them — there's no literal/exhaustive-pattern to
// invalidate. (Key subtlety: these restrictions apply to *foreign* crates only.
// Inside THIS crate the attribute is inert — you can still literal-construct and
// exhaustively match. So you won't see an error here; you're learning the *design*.
// Next rung you'll feel the real compiler version via a std non_exhaustive type.)
//
// Your turn:
//   1. Add `#[non_exhaustive]` above `ClientConfig`.
//   2. Implement `ClientConfig::new()` returning sensible defaults — this is the
//      ONLY construction path you're giving downstream.
//   3. Implement `with_retries`, a builder-style setter, so users can still
//      customize without a struct literal.

// TODO: add #[non_exhaustive] here
#[non_exhaustive]
pub struct ClientConfig {
    pub timeout_ms: u32,
    pub retries: u8,
}

impl ClientConfig {
    pub fn new() -> Self {
        Self {
            timeout_ms: 30_000,
            retries: 3,
        }
    }

    pub fn with_retries(mut self, retries: u8) -> Self {
        self.retries = retries;
        self
    }
}

fn check_3() {
    // Downstream reads public fields directly (allowed) but constructs via new()
    // (the only door), then customizes with a builder method — never a literal.
    let cfg = ClientConfig::new().with_retries(5);
    assert_eq!(cfg.timeout_ms, 30_000); // direct field read: fine cross-crate
    assert_eq!(cfg.retries, 5);
    println!("rung 3 ✔  #[non_exhaustive] struct: read fields, but construct via a door");
}

// ─────────────── Rung 4 — adding an enum variant; #[non_exhaustive] enums ────────
//
// Enums are the classic semver landmine. Add a variant to a normal public enum and
// every downstream `match` that lacked a `_` arm breaks with E0004 (non-exhaustive
// patterns). So "add a variant" defaults to a MAJOR change.
//
// PART A — feel the REAL compiler version, in this very file. `std::io::ErrorKind`
// is itself `#[non_exhaustive]`, and from this bin's point of view it lives in a
// "foreign" crate (std). So the compiler FORCES you to add a `_` arm — exactly the
// discipline non_exhaustive imposes on downstream.
//   Experiment: write the match in `describe_io_error` WITHOUT a `_` arm first, run,
//   and read the E0004 error + the note "`ErrorKind` marked as non-exhaustive".
//   Then add the `_` arm to make it compile and satisfy check_4.
//
// PART B — the rule, as code. If your enum was `#[non_exhaustive]` from day one,
// downstream was always forced to write `_`, so adding a variant later is only a
// MINOR change. If it was a plain enum, adding a variant is MAJOR. Implement
// `add_variant_bump` to encode that.

use std::io::ErrorKind;

fn describe_io_error(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::NotFound => "missing",
        ErrorKind::PermissionDenied => "denied",
        _ => "other",
    }
}

fn add_variant_bump(enum_was_non_exhaustive: bool) -> Bump {
    if enum_was_non_exhaustive {
        Bump::Minor
    } else {
        Bump::Major
    }
}

fn check_4() {
    assert_eq!(describe_io_error(ErrorKind::NotFound), "missing");
    assert_eq!(describe_io_error(ErrorKind::PermissionDenied), "denied");
    assert_eq!(describe_io_error(ErrorKind::OutOfMemory), "other");

    assert_eq!(add_variant_bump(true), Bump::Minor); // was non_exhaustive
    assert_eq!(add_variant_bump(false), Bump::Major); // was a plain enum
    println!("rung 4 ✔  enum variants: plain = breaking, non_exhaustive forces `_`");
}

// ──────────────── Rung 5 — trait evolution: methods & impls ─────────────────────
//
// A public trait is a contract you make with everyone who `impl`s it. Two opposite
// moves on a trait have opposite semver costs:
//
//   * Add a REQUIRED method (no default body)  -> BREAKING. Every downstream
//     `impl Plugin for TheirType` now fails with E0046 ("not all trait items
//     implemented"), because they wrote their impl before the method existed.
//   * Add a method WITH a default body          -> non-breaking (MINOR). Existing
//     impls inherit the default; they don't have to know it appeared.
//
// PART A — feel it. `MyPlugin` below is a "downstream" impl written against v1.0 of
// `Plugin` (just `name`). Your v1.1 needs a `version` method.
//   STEP 1: add `fn version(&self) -> u32;` to the trait WITHOUT a body, run, and
//           watch `impl Plugin for MyPlugin` break with E0046. That's every user of
//           your crate breaking at once.
//   STEP 2: give `version` a DEFAULT body (`{ 1 }`) instead. Now MyPlugin compiles
//           untouched — the addition is non-breaking. Leave it this way for check_5.
//
// PART B — the rules as code. Also classify impl-side changes:
//   * Adding a NON-blanket impl (`impl Trait for ConcreteType`) -> MINOR. (It can
//     technically perturb downstream type inference, but the reference treats it as
//     a minor change.)
//   * Adding a BLANKET impl (`impl<T> Trait for T`)            -> MAJOR. It can
//     collide with downstream impls (coherence / E0119) — a true breaking change.
// Implement `trait_change_bump`.

pub trait Plugin {
    fn name(&self) -> &str;

    fn version(&self) -> u32 {
        1
    }
}

struct MyPlugin; // downstream impl, written against v1.0 (only `name` existed)
impl Plugin for MyPlugin {
    fn name(&self) -> &str {
        "demo"
    }
}

#[derive(Debug)]
enum TraitChange {
    AddRequiredMethodNoDefault,
    AddDefaultedMethod,
    AddNonBlanketImpl,
    AddBlanketImpl,
}

fn trait_change_bump(change: &TraitChange) -> Bump {
    match change {
        TraitChange::AddRequiredMethodNoDefault => Bump::Major,
        TraitChange::AddDefaultedMethod => Bump::Minor,
        TraitChange::AddNonBlanketImpl => Bump::Minor,
        TraitChange::AddBlanketImpl => Bump::Major,
    }
}

fn check_5() {
    let p = MyPlugin;
    assert_eq!(p.name(), "demo");
    assert_eq!(p.version(), 1); // inherited from the DEFAULT body you added

    use TraitChange::*;
    assert_eq!(trait_change_bump(&AddRequiredMethodNoDefault), Bump::Major);
    assert_eq!(trait_change_bump(&AddDefaultedMethod), Bump::Minor);
    assert_eq!(trait_change_bump(&AddNonBlanketImpl), Bump::Minor);
    assert_eq!(trait_change_bump(&AddBlanketImpl), Bump::Major);
    println!("rung 5 ✔  trait evolution: defaults & non-blanket impls are safe, the rest break");
}

// ───────────────────── Rung 6 — sealed traits as a semver tool ──────────────────
//
// Rung 5's lesson was scary: adding a required method to a public trait breaks every
// downstream impl. But what if NO downstream impl can exist? Then there's nothing to
// break — you can add methods, change defaults, even add supertraits, all as MINOR
// changes. That's what SEALING a trait buys you. (It's how `serde`, `bytes`, and many
// std traits stay evolvable.)
//
// The pattern: put a marker trait in a PRIVATE module and make your public trait
// require it as a supertrait. Downstream can call your trait's methods, but cannot
// `impl` it, because they can't name — let alone implement — the private `Sealed`.
//
//     mod sealed { pub trait Sealed {} }          // module is private to the crate
//     pub trait Format: sealed::Sealed { ... }     // public, but gated by Sealed
//
// Only code INSIDE this crate can `impl sealed::Sealed for X`, so only you can add
// `impl Format`. A foreign crate that writes `impl Format for TheirType` gets E0277
// ("the trait bound `TheirType: Sealed` is not satisfied") and cannot fix it — they
// can't reach into your private `sealed` module. (Same as rungs 3-4, the *block* is
// cross-crate, so you won't see that error here — you're building the machine.)
//
// Your turn:
//   1. Inside `mod sealed`, declare `pub trait Sealed {}`.
//   2. Make `Format` require it: `pub trait Format: sealed::Sealed`.
//   3. For BOTH `Json` and `Yaml`: `impl sealed::Sealed` (empty) AND `impl Format`
//      with `extension` returning "json" / "yaml".
// Bonus to convince yourself of the payoff: add a second method to `Format` WITH a
// default (`fn is_binary(&self) -> bool { false }`) — note you didn't have to touch
// Json/Yaml, and no foreign impl could possibly break, because none can exist.

mod sealed {
    pub trait Sealed {}
}

pub trait Format: sealed::Sealed {
    fn extension(&self) -> &str;
}

pub struct Json;

impl sealed::Sealed for Json {}

impl Format for Json {
    fn extension(&self) -> &str {
        "json"
    }
}

pub struct Yaml;

impl sealed::Sealed for Yaml {}

impl Format for Yaml {
    fn extension(&self) -> &str {
        "yaml"
    }
}

fn describe<F: Format>(f: &F) -> String {
    format!(".{}", f.extension())
}

fn check_6() {
    assert_eq!(describe(&Json), ".json");
    assert_eq!(describe(&Yaml), ".yaml");
    println!("rung 6 ✔  sealed trait: no foreign impls => safe to evolve the trait");
}

// ───────────────── Rung 7 — auto-trait leakage (Send/Sync) ──────────────────────
//
// The sneakiest breaking change in Rust: you change a PRIVATE field's type and the
// public signature doesn't move one character — yet downstream stops compiling.
//
// Why: `Send` and `Sync` are AUTO traits. The compiler derives them structurally
// from a type's fields. A `Job` holding a `Vec<u8>` is `Send + Sync`; swap that
// field to an `Rc<u8>` and `Job` silently becomes `!Send + !Sync` — because `Rc`'s
// non-atomic refcount isn't thread-safe. Any downstream `thread::spawn(move || ...)`
// that captured a `Job` now fails with E0277 ("`Rc<u8>` cannot be sent between
// threads safely"). You shipped it as a patch; it was actually a MAJOR break.
// (Same hazard with `-> impl Trait` returns: the hidden opaque type leaks the auto
// traits of whatever you built it from.)
//
// The professional defense: a compile-time regression guard. A `const` block that
// calls `assert_send::<Job>` / `assert_sync::<Job>` turns "did I accidentally drop
// Send?" into a BUILD ERROR, so you can never ship the leak unknowingly.
//
// Your turn:
//   1. Implement `assert_send<T: Send>()` and `assert_sync<T: Sync>()` (empty bodies;
//      the BOUND is the whole test — they only compile if T really is Send/Sync).
//   2. Keep `Job` as-is (Send + Sync) so the const guard below compiles.
//   3. FEEL THE LEAK: temporarily change `data: Vec<u8>` to `data: std::rc::Rc<u8>`,
//      run, and read the E0277 from the const guard. Then change it back.

fn assert_send<T: Send>() {}

fn assert_sync<T: Sync>() {}

pub struct Job {
    #[allow(dead_code)]
    data: Vec<u8>, // STEP 3: swap to std::rc::Rc<u8> to watch Send/Sync vanish
}

// Compile-time regression guard: if Job ever loses Send or Sync, THIS fails to
// build. `const _` runs the closure's type-check at compile time without any runtime.
const _: () = {
    let _guard: fn() = || {
        assert_send::<Job>();
        assert_sync::<Job>();
    };
};

fn check_7() {
    // Runtime witness too: a Job genuinely crosses a thread boundary.
    let job = Job {
        data: vec![1, 2, 3],
    };
    let handle = std::thread::spawn(move || job.data.len());
    assert_eq!(handle.join().unwrap(), 3);
    println!("rung 7 ✔  auto-trait leakage: a private field can silently break Send/Sync");
}

// ──────────────── Rung 8 — generic bounds & parameters ──────────────────────────
//
// Generic bounds have an ASYMMETRY that's the whole semver story here:
//   * LOOSENING a bound (removing a requirement) is NON-breaking. Anyone who met the
//     old, stricter bound automatically meets the looser one. You let MORE callers in.
//   * TIGHTENING a bound (adding a requirement) is BREAKING. Callers whose type
//     doesn't satisfy the new requirement get E0277 and are locked out.
// Rule of thumb: ask for the MINIMUM your body actually needs. Over-constraining isn't
// just inelegant — every extra bound is a wall some future caller will hit.
//
// PART A — feel it. `NoClone` derives `Debug` but NOT `Clone`. The "downstream"
// call in check_8 is `process(&[NoClone, NoClone])`. The body of `process` only
// needs to `{:?}`-format its items, so the MINIMAL bound is `T: Debug`.
//   STEP 1: fill the bound with exactly what the body needs (`std::fmt::Debug`).
//   STEP 2 (the break): add `+ Clone` to the bound, run, and read E0277
//           ("`NoClone: Clone` is not satisfied") — that's a caller locked out by an
//           over-tight bound. Remove the `+ Clone` again to go green.
//
// PART B — the rules as code. Implement `bound_change_bump`:
//   * LoosenBound        -> Minor   (remove a requirement; compatible)
//   * TightenBound       -> Major   (add a requirement; locks callers out)
//   * AddTypeParamNoDefault -> Major (changes arity; turbofish & some calls break)
//   * AddTypeParamWithDefault -> Minor (existing uses keep working via the default)

#[derive(Debug)]
struct NoClone;

fn process<T: std::fmt::Debug>(items: &[T]) -> usize {
    let _rendered: Vec<String> = items.iter().map(|x| format!("{x:?}")).collect();
    items.len()
}

#[derive(Debug)]
enum BoundChange {
    LoosenBound,
    TightenBound,
    AddTypeParamNoDefault,
    AddTypeParamWithDefault,
}

fn bound_change_bump(change: &BoundChange) -> Bump {
    match change {
        BoundChange::LoosenBound => Bump::Minor,
        BoundChange::TightenBound => Bump::Major,
        BoundChange::AddTypeParamNoDefault => Bump::Major,
        BoundChange::AddTypeParamWithDefault => Bump::Minor,
    }
}

fn check_8() {
    assert_eq!(process(&[NoClone, NoClone]), 2); // compiles only if bound is loose enough

    use BoundChange::*;
    assert_eq!(bound_change_bump(&LoosenBound), Bump::Minor);
    assert_eq!(bound_change_bump(&TightenBound), Bump::Major);
    assert_eq!(bound_change_bump(&AddTypeParamNoDefault), Bump::Major);
    assert_eq!(bound_change_bump(&AddTypeParamWithDefault), Bump::Minor);
    println!("rung 8 ✔  bounds: loosen = safe, tighten = break; type params need defaults");
}

// ═══════════════════ Rung 9 — CAPSTONE ══════════════════════════════════════════
//
// Two parts. PART A is the "semver brain": one classifier that encodes every rule
// from rungs 1-8. PART B is the "semver hands": a library module engineered so a
// realistic v1.1 release is a MINOR bump — its downstream consumer compiles untouched.
//
// ───────── PART A — the classification engine ─────────
//
// Implement `classify(&ApiChange) -> Bump`, using match guards on the bool fields.
// Each variant encodes a lesson you proved earlier; the comments say which rung.
// The full rule set:

#[derive(Debug)]
enum ApiChange {
    /// rung 1: internal bug fix, no public surface touched.
    BugFix,
    /// rung 1: brand-new public item (fn/type/module) added.
    AddPublicItem,
    /// rung 1: a public item removed or renamed.
    RemovePublicItem,
    /// rung 2-3: add a field to a struct. Safe ONLY if downstream could never
    /// write a struct literal — i.e. it already had a private field OR is
    /// #[non_exhaustive]. Otherwise it breaks every literal/exhaustive destructure.
    AddStructField { sealed_from_literals: bool },
    /// rung 4: add an enum variant. Safe only if the enum is #[non_exhaustive].
    AddEnumVariant { non_exhaustive: bool },
    /// rung 5: add a method to a public trait. Safe iff it has a default body...
    AddTraitMethod { has_default: bool },
    /// rung 6: ...OR the trait is sealed (no foreign impls can exist to break).
    AddMethodToSealedTrait,
    /// rung 5: add a blanket `impl<T> Trait for T` (coherence collisions).
    AddBlanketImpl,
    /// rung 8: tighten a generic bound (add a requirement).
    TightenBound,
    /// rung 8: loosen a generic bound (remove a requirement).
    LoosenBound,
    /// rung 7: change a private field's type. Breaking iff it drops Send/Sync
    /// (or any auto trait) from a public type.
    ChangeInternals { keeps_auto_traits: bool },
}

fn classify(change: &ApiChange) -> Bump {
    match change {
        ApiChange::BugFix => Bump::Patch,
        ApiChange::AddPublicItem => Bump::Minor,
        ApiChange::RemovePublicItem => Bump::Major,
        ApiChange::AddStructField {
            sealed_from_literals: true,
        } => Bump::Minor,
        ApiChange::AddStructField {
            sealed_from_literals: false,
        } => Bump::Major,
        ApiChange::AddEnumVariant {
            non_exhaustive: true,
        } => Bump::Minor,
        ApiChange::AddEnumVariant {
            non_exhaustive: false,
        } => Bump::Major,
        ApiChange::AddTraitMethod { has_default: true } => Bump::Minor,
        ApiChange::AddTraitMethod { has_default: false } => Bump::Major,
        ApiChange::AddMethodToSealedTrait => Bump::Minor,
        ApiChange::AddBlanketImpl => Bump::Major,
        ApiChange::TightenBound => Bump::Major,
        ApiChange::LoosenBound => Bump::Minor,
        ApiChange::ChangeInternals {
            keeps_auto_traits: true,
        } => Bump::Patch,
        ApiChange::ChangeInternals {
            keeps_auto_traits: false,
        } => Bump::Major,
    }
}

// ───────── PART B — a future-proofed library (the `lib` module) ─────────
//
// This module is your v1.0 crate. Engineer it so the v1.1 additions described at
// the bottom are all MINOR. You must combine: a #[non_exhaustive] config built
// through a constructor (rung 3), and a SEALED trait (rung 6) so you can grow it.
//
// Your turn, inside `mod lib`:
//   1. Mark `Settings` `#[non_exhaustive]` and implement `Settings::new()`
//      (level = 1). This is downstream's only construction path.
//   2. Seal `Codec`: declare `pub trait Sealed {}` in the private `sealed` module,
//      add `: sealed::Sealed` to `Codec`, and impl both `Sealed` and `Codec` for
//      `Gzip` (`name` -> "gzip").

mod lib {
    pub mod sealed {
        pub trait Sealed {}
    }

    #[non_exhaustive]
    pub struct Settings {
        pub level: u8,
    }

    impl Settings {
        // TODO 1: construct with level = 1.
        pub fn new() -> Self {
            Settings { level: 1 }
        }
    }

    // TODO 2: seal this trait with `: sealed::Sealed`.
    pub trait Codec: sealed::Sealed {
        fn name(&self) -> &str;
    }

    pub struct Gzip;
    impl sealed::Sealed for Gzip {}
    impl Codec for Gzip {
        fn name(&self) -> &str {
            "gzip"
        }
    }
}

// "Downstream" consumer, written against v1.0. The whole point: after the v1.1
// additions below, NOT ONE character of this function needs to change.
fn use_library() -> String {
    use lib::*;
    let settings = Settings::new();
    let codec = Gzip;
    format!("{} @ level {}", codec.name(), settings.level)
}

fn check_9() {
    // Part A: the engine.
    use ApiChange::*;
    assert_eq!(classify(&BugFix), Bump::Patch);
    assert_eq!(classify(&AddPublicItem), Bump::Minor);
    assert_eq!(classify(&RemovePublicItem), Bump::Major);
    assert_eq!(
        classify(&AddStructField {
            sealed_from_literals: true
        }),
        Bump::Minor
    );
    assert_eq!(
        classify(&AddStructField {
            sealed_from_literals: false
        }),
        Bump::Major
    );
    assert_eq!(
        classify(&AddEnumVariant {
            non_exhaustive: true
        }),
        Bump::Minor
    );
    assert_eq!(
        classify(&AddEnumVariant {
            non_exhaustive: false
        }),
        Bump::Major
    );
    assert_eq!(classify(&AddTraitMethod { has_default: true }), Bump::Minor);
    assert_eq!(
        classify(&AddTraitMethod { has_default: false }),
        Bump::Major
    );
    assert_eq!(classify(&AddMethodToSealedTrait), Bump::Minor);
    assert_eq!(classify(&AddBlanketImpl), Bump::Major);
    assert_eq!(classify(&TightenBound), Bump::Major);
    assert_eq!(classify(&LoosenBound), Bump::Minor);
    assert_eq!(
        classify(&ChangeInternals {
            keeps_auto_traits: true
        }),
        Bump::Patch
    );
    assert_eq!(
        classify(&ChangeInternals {
            keeps_auto_traits: false
        }),
        Bump::Major
    );

    // Part B: the future-proofed library works for the consumer.
    assert_eq!(use_library(), "gzip @ level 1");

    println!("rung 9 ✔  CAPSTONE: classifier engine + a v1.1-ready library. Semver mastered.");
    println!();
    println!("   ── Now prove the design (optional but worth it) ──");
    println!("   Simulate a v1.1 release and confirm `use_library` still compiles UNTOUCHED:");
    println!("     • add a field to Settings (works: it's #[non_exhaustive] + built via new())");
    println!("     • add a defaulted method to Codec (works: sealed trait, you own all impls)");
    println!("   Both are MINOR. That's the entire payoff of this ladder.");
}
