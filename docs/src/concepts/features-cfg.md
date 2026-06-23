# Cargo features & `cfg`

> Ladder: [`src/bin/features_cfg.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/features_cfg.rs) ·
> Run: `cargo run --bin features_cfg` · Phase 0 · 9 rungs

## TL;DR

Conditional compilation lets the compiler decide **which code even exists** before
it type-checks anything, based on *cfg predicates* (`target_os`, `feature = "x"`,
`test`, `debug_assertions`, …). Two faces of one idea:

- `#[cfg(...)]` — an **attribute** that includes or excludes an item entirely.
  Excluded code is never compiled. It can be broken Rust and nobody notices until
  the predicate flips.
- `cfg!(...)` — a **macro** that evaluates to a plain `bool` at runtime. *Both*
  branches around it are always compiled.

A **feature** is a named cfg flag you declare in `Cargo.toml [features]` and turn
on with `--features`. The single law that governs all feature design:
**features must be additive** — turning one on may only *add* behavior, never
remove or change it, because Cargo compiles your crate once with the *union* of
every feature anyone in the dependency graph requested.

## Why this exists (from first principles)

One source tree has to compile for many worlds: Linux and Windows, 64-bit and
32-bit, debug and release, "with JSON support" and "without". You could maintain
separate files or `if` everything at runtime, but both are bad: runtime `if`s
still force you to *compile and link* code (and its dependencies) you'll never run
on this target, and some code is literally uncompilable elsewhere (a Windows API
call on Linux).

Conditional compilation solves this by moving the decision **before** type
checking. `#[cfg(target_os = "windows")]` on a function means: on Linux, that
function does not exist — it is never parsed for types, never linked, costs
nothing. The compiler enforces only what survives the cfg filter.

Features generalize this from "facts about the target" to "knobs the user picks".
But features carry a hazard that platform cfgs don't: **they are shared**. If
crate A and crate B both depend on your crate and each asks for a different
feature, Cargo does not build your crate twice. It builds it once with the union.
That single fact is the source of every feature footgun and the reason for the
additivity law.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `cfg!()` macro | Runtime `bool`; both arms compile |
| 2 | foundations | `#[cfg(...)]` attribute | Item exists or not; twin defs would be `E0428` |
| 3 | mechanics | First feature flag | Declare in `[features]`, gate with `#[cfg(feature)]` |
| 4 | mechanics | `cfg_attr` | Conditionally apply an attribute (a `derive`) |
| 5 | footgun | Additivity | Mutually-exclusive features collide under unification |
| 6 | footgun | Missing-symbol trap | `cfg!` keeps both arms → `E0425`; gate the call site |
| 7 | real-world | Optional dependency | `optional = true` + `dep:rand`; off → not compiled |
| 8 | real-world | Feature graphs | `default`, umbrella features, `--no-default-features` |
| 9 | capstone | Mini config module | `cfg(target_os)` + additive renderers + `cfg_attr` + `cfg!` |

## The ideas, built up

### 1. `cfg!(...)` — the macro: a compile-time `bool`

`cfg!(predicate)` evaluates to `true` or `false` at compile time, but it is *used*
at runtime like any other boolean. The key property: it does **not** delete code.
Both arms of an `if cfg!(...)` are fully compiled and type-checked.

```rust
fn build_profile() -> String {
    let mut s = String::new();
    if cfg!(debug_assertions) {      // both the "debug" and "release" arms
        s.push_str("debug");         // are compiled; one is chosen at runtime
    } else {
        s.push_str("release");
    }
    if cfg!(target_os = "linux") {
        s.push_str(" on linux");
    }
    s
}
```

On a debug Linux build this returns `"debug on linux"`. Because both arms compile,
`cfg!` is only safe when *every* arm is valid Rust on *every* target. That's its
limitation and the reason rung 2 exists.

### 2. `#[cfg(...)]` — the attribute: an item either exists or it doesn't

The attribute form is the *exclusion* tool. `#[cfg(pred)]` on an item means the
item is compiled only if `pred` holds; otherwise it vanishes before name
resolution. That lets you write two definitions of the same function gated by
mutually exclusive predicates:

```rust
#[cfg(target_pointer_width = "64")]
fn platform_tag() -> &'static str { "64-bit" }

#[cfg(not(target_pointer_width = "64"))]
fn platform_tag() -> &'static str { "non-64-bit" }
```

There is no "defined multiple times" error because, on any given target, only one
of these exists. The proof: change the second predicate to *also* be
`target_pointer_width = "64"` and you get `E0428: the name 'platform_tag' is
defined multiple times` — the collision only appears once both items survive the
cfg filter. The cfg deletes one before the compiler ever sees a conflict.

> This is the deep difference from `cfg!`: the attribute can guard code that
> *wouldn't even compile* on the other target. The macro cannot — both its arms
> must always be valid.

### 3. Your first feature flag

A feature is a cfg flag you invent. Unlike `target_os`, the name `feature = "demo"`
is unknown until you declare it in `Cargo.toml`:

```toml
[features]
demo = []   # the empty list = "enables no other features / optional deps (yet)"
```

Then `#[cfg(feature = "demo")]` works like any other cfg, and `--features demo`
turns it on:

```rust
#[cfg(feature = "demo")]
fn feature_line() -> &'static str { "demo enabled" }
```

With the feature off, `feature_line` does not exist. The same binary now compiles
two different ways depending on the flag:

```
cargo run --bin features_cfg                 # demo off — fn not compiled in
cargo run --bin features_cfg --features demo # demo on  — fn exists
```

> Since Rust 1.80, an undeclared feature name in a `#[cfg(feature = "...")]`
> triggers the `unexpected_cfgs` lint ("consider adding `demo` as a feature in
> Cargo.toml") instead of silently evaluating to false. That lint catches typo'd
> feature names — a class of bug that used to fail silently.

### 4. `cfg_attr` — conditionally applying another attribute

`#[cfg(...)]` includes or excludes an *item*. `cfg_attr` includes or excludes an
*attribute*:

```rust
#[cfg_attr(PREDICATE, attr1, attr2, ...)]
// reads as: if PREDICATE holds, expand to #[attr1] #[attr2] ...; else nothing
```

The canonical use is gating a `derive`:

```rust
#[derive(Debug, Clone)]                          // always wanted
#[cfg_attr(feature = "demo", derive(PartialEq))] // PartialEq only under `demo`
struct Point { x: i32, y: i32 }
```

Without `demo`, `Point` has `Debug` and `Clone` but no `PartialEq`, so `p == q`
won't compile (and the test that does the comparison is itself behind
`#[cfg(feature = "demo")]`). This is exactly how every serde-supporting crate works:

```rust
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
```

One line; no duplicated struct definition.

### 5. The additivity law (the rung the whole concept exists for)

Cargo compiles your crate **once** with the union of every feature any crate in
the graph requested. Therefore a feature turning on may only *add* behavior. The
anti-pattern is two features that assume they are mutually exclusive:

```rust
// ANTI-PATTERN — collides under unification
#[cfg(feature = "metric")]
fn unit_label() -> &'static str { "meters" }
#[cfg(feature = "imperial")]
fn unit_label() -> &'static str { "feet" }
```

Each builds fine alone. But `--features metric,imperial` produces
`E0428: defined multiple times` — and that combination is exactly what unification
can *force* on you: crate A enables `metric`, crate B enables `imperial`, and your
build breaks through no fault of either. The victim never asked for the conflict.

The fix is to **accumulate** instead of choosing:

```rust
// ADDITIVE — enabling both just yields both
fn enabled_units() -> Vec<&'static str> {
    let mut units = Vec::new();
    if cfg!(feature = "metric")   { units.push("meters"); }
    if cfg!(feature = "imperial") { units.push("feet"); }
    units
}
```

Now `--features metric,imperial` returns `["meters", "feet"]`. The design rules
that fall out of this:

- Never gate the *default* behavior behind `#[cfg(not(feature = "x"))]` — that's a
  feature that *disables* something, which is removal in disguise.
- Never make two features mutually exclusive — unification can turn both on.
- Enabling a feature must never break a consumer who didn't ask for it.

### 6. The missing-symbol trap

If an item is gated behind a feature, **every use of it must be gated too.** This
is where the `cfg!` vs `#[cfg]` distinction becomes a correctness issue, not a
style choice. Consider a function that exists only under `demo`:

```rust
#[cfg(feature = "demo")]
fn pretty_print(label: &str, n: i32) -> String { format!("[ {label} = {n} ]") }
```

The tempting wrong solution uses the macro:

```rust
// WRONG — fails to compile when demo is OFF
fn describe(label: &str, n: i32) -> String {
    if cfg!(feature = "demo") { pretty_print(label, n) }  // E0425: cannot find `pretty_print`
    else { format!("{label}={n}") }
}
```

`cfg!` keeps *both* arms compiled, so the off-build still tries to resolve
`pretty_print`, which doesn't exist → `E0425`. The only correct solution gates the
*call site* with the attribute form, so each build names only what it has:

```rust
// OK — each build compiles only its own arm
fn describe(label: &str, n: i32) -> String {
    #[cfg(feature = "demo")]
    { pretty_print(label, n) }
    #[cfg(not(feature = "demo"))]
    { format!("{label}={n}") }
}
```

The same discipline applies to `use` imports — `use rand::RngExt;` must carry
`#[cfg(feature = "dice")]` or the off-build hits `E0432: unresolved import`. The
rule in one line: **gate the definition AND every reference (calls and imports),
with the attribute, not the macro.**

### 7. Optional dependency = a feature

A dependency you only sometimes need is marked `optional = true`. Cargo then does
not compile it by default; a feature pulls it in via the `dep:` syntax:

```toml
[dependencies]
rand = { version = "0.10", optional = true }

[features]
dice = ["dep:rand"]
```

```rust
#[cfg(feature = "dice")]
fn roll_die() -> u32 { rand::rng().random_range(1..=6) }
```

Two subtleties worth owning:

- An optional dependency *implicitly* creates a same-named feature (`rand`) you
  could enable directly — **unless** you reference it as `dep:rand` somewhere,
  which suppresses that implicit feature and keeps your dependency names out of
  your public feature surface. `dep:` (Rust 1.60+) is the modern idiom.
- In the default build, `rand` is not merely unused — it is **not downloaded or
  compiled at all**. That's the real payoff: feature-gated bloat costs nothing
  when off, both in compile time and binary size.

### 8. Feature graphs

Features form a directed graph, not a flat list. Three mechanisms:

```toml
[features]
default = ["metric"]          # on automatically; --no-default-features strips it
color   = []                  # a leaf feature
full    = ["color", "demo"]   # umbrella: enabling `full` transitively enables both
```

- **`default`** is the set Cargo turns on for a plain `cargo build`.
  `--no-default-features` removes it (common for `no_std`/embedded builds).
- A feature listing **other features** enables them transitively. Turning on
  `full` is guaranteed to also turn on `color` and `demo`.
- The combination `--no-default-features --features full` means "minimal build,
  plus exactly this subtree".

The active set behaves like a closure under "enables":

| build | active features |
|---|---|
| default | `["metric"]` |
| `--no-default-features` | `[]` |
| `--features full` | `["metric", "color", "demo"]` |
| `--no-default-features --features full` | `["color", "demo"]` |

## Footguns

| Trap | What bites | Fix |
|------|-----------|-----|
| Mutually-exclusive features | `E0428` when unification turns both on | Make features additive — accumulate, don't choose |
| Feature that disables behavior | A consumer enabling it silently breaks another | On may only *add*; never gate defaults behind `not(feature)` |
| `cfg!` to guard a gated symbol | `E0425` in the off-build — both `cfg!` arms compile | Use `#[cfg]` on the call site, not `cfg!` |
| Ungated `use` of a gated crate | `E0432: unresolved import` in the off-build | `#[cfg(feature = "...")]` on the `use` too |
| Typo'd feature name | `#[cfg]` silently always-false (pre-1.80) | Declare every feature; heed the `unexpected_cfgs` lint |

## Real-world patterns

- **serde support behind a feature** — the universal
  `#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]`, with `serde`
  as an optional dependency pulled in by `dep:serde`. Costs nothing when off.
- **`std` vs `no_std`** — libraries expose a `std` feature (often in `default`)
  and use `#![cfg_attr(not(feature = "std"), no_std)]` so embedded users can opt
  out via `--no-default-features`.
- **Umbrella `full` feature** — big crates (e.g. tokio's `features = ["full"]`)
  ship one feature that enables a curated subtree, so users don't have to know the
  whole list.
- **Platform shims** — `#[cfg(target_os = "...")]` selects one of several
  same-named functions, so callers write portable code over an OS-specific impl.

## Capstone insight

The capstone builds a `mod config` that exercises the whole matrix at once — the
shape of a real crate's config/output layer:

```rust
#[cfg(target_os = "linux")]
pub fn config_dir() -> &'static str { "/etc/app" }   // platform path (attribute form)
#[cfg(not(target_os = "linux"))]
pub fn config_dir() -> &'static str { "/tmp/app" }

#[derive(Debug)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]  // serde derive only under json
pub struct Report { pub name: &'static str, pub level: u8 }

pub fn render(r: &Report) -> Vec<String> {
    let mut lines = Vec::new();
    #[cfg(feature = "json")]   { lines.push(serde_json::to_string(r).unwrap()); }
    #[cfg(feature = "pretty")] { lines.push(format!("{} @ level {}", r.name, r.level)); }
    #[cfg(not(any(feature = "json", feature = "pretty")))]
    { lines.push(format!("{:?}", r)); }   // fallback when neither is on
    lines
}
```

The "aha" is that all four tools compose cleanly *because each respects its lane*:
`cfg(target_os)` picks exactly one `config_dir`; `cfg_attr` adds the `Serialize`
impl only when the JSON path needs it; the per-branch `#[cfg]` in `render` makes
the output **additive** — `--features json,pretty` emits *both* lines, and the
`not(any(...))` fallback guarantees exactly one line when neither is on. The whole
module is a microcosm of the additivity law: every feature only ever adds a line.

## Explain it back

- What's the difference between `cfg!(...)` and `#[cfg(...)]`, and when does only
  one of them work?
- Why does `if cfg!(feature = "x") { gated_fn() }` fail to compile when the feature
  is off, while `#[cfg(feature = "x")] { gated_fn() }` succeeds?
- Why must features be additive? Construct the two-crate scenario where a
  non-additive feature breaks an innocent consumer.
- What does `dep:rand` do that bare `rand` in a feature list does not? What is the
  *implicit* feature, and why suppress it?
- Predict the active feature set for `--no-default-features --features full` given
  `default = ["metric"]` and `full = ["color", "demo"]`.

## See also

- [Modules & visibility](modules.md) — what gets gated lives in modules; `#[cfg]`
  and `pub` both shape the surface a crate exposes.
- [Newtype & zero-cost wrappers](newtype.md) — `#[cfg_attr(..., derive(...))]` is
  the same conditional-derive machinery newtypes lean on for serde support.
