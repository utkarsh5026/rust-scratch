// This file is the ROOT of the LIBRARY crate `r2_libbin`.
// It is a *different crate* from the binary in main.rs, even though both
// live in the same package and share this one Cargo.toml.
//
// Only `pub` items are visible to the binary crate that depends on this lib.

/// 8% tax on an amount in integer cents, rounded down.
pub fn tax(cents: u64) -> u64 {
    cents * 8 / 100
}
