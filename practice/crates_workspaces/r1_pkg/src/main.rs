// Rung 1 — Package vs crate
//
// A *package* is the thing with a Cargo.toml. It bundles one or more *crates*.
// A *crate* is one compilation unit with a single ROOT file:
//   - a BINARY crate's root is  src/main.rs
//   - a LIBRARY crate's root is src/lib.rs
// Right now this package contains exactly ONE crate: a binary, rooted in THIS file.
//
// Your turn:
//   1. Make `main` print EXACTLY this line (one space between fields, newline at end):
//          package=r1_pkg bin=r1_pkg
//      but don't hard-code the strings — read them from the env vars Cargo injects
//      at build time: CARGO_PKG_NAME and CARGO_BIN_NAME (use the env! macro).
//   2. Fill in the ANSWER line below.
//
// Run:  cd practice/crates_workspaces/r1_pkg && cargo run
//
// ANSWER: this package contains 1 crate; this crate's root file is src/main.rs

fn main() {
    println!(
        "package={} bin={}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_BIN_NAME")
    );
}
