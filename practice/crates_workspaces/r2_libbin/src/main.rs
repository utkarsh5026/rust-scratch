// Rung 2 — The lib + bin split: one package, TWO crates
//
// A package may hold ONE library crate (root src/lib.rs) plus binary crate(s).
// The binary does NOT see the library's items directly — it depends on the
// library as a SEPARATE crate, referred to by the LIBRARY CRATE NAME
// (= package name, with '-' turned into '_'). Here that name is `r2_libbin`.
//
// Your turn:
//   1. In src/lib.rs, implement `pub fn tax`.
//   2. Here in main, CALL it through the library crate and print:
//          tax(1000) = 80
//      Think: what path leads from the binary crate to the lib crate's `tax`?
//
// Run:  cd practice/crates_workspaces/r2_libbin && cargo run

fn main() {
    let owed: u64 = r2_libbin::tax(1000);
    println!("tax(1000) = {owed}");
}
