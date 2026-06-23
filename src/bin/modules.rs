// Modules & visibility — Phase 0
// Run: cargo run --bin modules
//
// Mental model: a crate is a TREE of modules rooted at the crate root.
// `mod` declares a node; it imports nothing. Everything is private by default.
// Privacy is about the tree: an item is visible to its own module and all
// descendants; a parent sees only what a child marks `pub`. Paths walk the
// tree: `crate::` from the root, `super::` up one level, `self::` here.
//
// Ladder (DONE marked):
//   1. Module tree & paths        — nested mod; crate:: / super:: / self::   [x]
//   2. pub opens a door           — expose exactly what's needed             [x]
//   3. Field privacy              — private fields + smart constructor       [x]
//   4. use & pub use              — scope aliases; re-export facade          [x]
//   5. Leaking a private type      — pub fn exposing a less-visible type      [x]
//   6. Restricted visibility      — pub(crate)/pub(super)/pub(in path)       [x]
//   7. Facade pattern             — private internals, curated public API    [x]
//   8. Sealed trait               — private module gates downstream impls    [x]
//   9. Capstone: mini-library     — full module tree with a real front door  [x]

// ----------------------------------------------------------------------------
// Rung 1 — Module tree & paths
//
// Below is a tree:  crate -> kitchen -> { pantry, stove }
// Fill in the three bodies using the RIGHT path prefix each time:
//   - pantry::flour_grams returns 500.
//   - stove::bake calls pantry::flour_grams. pantry is a SIBLING of stove,
//     so reach it by going UP one level then back down (super::...).
//   - crate::oven_temp returns 220 at the crate root; stove::bake should also
//     read it via a path that starts at the ROOT (crate::...).
// Goal: make check_1 pass. Pick prefixes deliberately — don't just guess.
// ----------------------------------------------------------------------------

fn oven_temp() -> u32 {
    220
}

mod kitchen {
    pub mod pantry {
        pub fn flour_grams() -> u32 {
            500
        }
    }

    pub mod stove {
        // bake() should return (flour_grams, oven_temp) as a tuple.
        // Reach flour_grams via `super::` (sibling module) and oven_temp via
        // `crate::` (it lives at the crate root, two levels up).
        pub fn bake() -> (u32, u32) {
            (super::pantry::flour_grams(), crate::oven_temp())
        }
    }
}

fn check_1() {
    assert_eq!(kitchen::pantry::flour_grams(), 500);
    assert_eq!(kitchen::stove::bake(), (500, 220));
    println!("rung 1 ok: walked the module tree with super:: and crate::");
}

// ----------------------------------------------------------------------------
// Rung 2 — pub opens a door
//
// `mod billing` below is a black box: it has an internal helper and a public
// entry point. Right now NOTHING compiles to call from outside because the
// items aren't reachable. Your job is to set visibility precisely:
//
//   - `tax_rate` is an INTERNAL detail. It must stay PRIVATE to billing
//     (callable by total_with_tax, but NOT from outside the module).
//   - `total_with_tax` is the PUBLIC entry point. Make it reachable from main.
//   - The module `billing` itself must be reachable from the crate root.
//
// Fill in total_with_tax's body (price + price*rate, integer math, rate is a
// percent so use price * tax_rate() / 100), and add the RIGHT `pub` markers.
// Then in check_2, the commented line that tries to call the PRIVATE helper
// must STAY commented — uncomment it briefly to SEE the E0603 privacy error,
// then re-comment it. That error is the lesson: privacy is enforced, not advice.
// ----------------------------------------------------------------------------

mod billing {
    // your turn: keep this PRIVATE (no pub). It's an implementation detail.
    fn tax_rate() -> u32 {
        8
    }

    // your turn: make this callable from outside billing.
    pub fn total_with_tax(price: u32) -> u32 {
        price + price * tax_rate() / 100
    }
}

fn check_2() {
    // This must work once you've opened the right doors:
    assert_eq!(billing::total_with_tax(100), 108);
    assert_eq!(billing::total_with_tax(250), 270);

    // Privacy demo: uncomment the next line, run, READ the E0603 error
    // ("function `tax_rate` is private"), then re-comment it.
    // let _ = billing::tax_rate();

    println!("rung 2 ok: pub exposed the entry point, kept the helper private");
}

// ----------------------------------------------------------------------------
// Rung 3 — Field privacy: `pub struct` is NOT pub fields
//
// Marking a struct `pub` only makes the TYPE nameable. Its fields are private
// by default — each field needs its own `pub` to be readable/constructible
// from outside the module. This is the lever for enforcing invariants: hide
// the fields, expose a smart constructor that's the ONLY way to build a valid
// value, and the outside world can't violate the rule.
//
// `mod temperature` defines `Celsius`, which must NEVER hold a value below
// absolute zero (-273). Your job:
//   - Keep the field `degrees` PRIVATE (so nobody can set a bogus value or
//     read the raw field — they go through methods).
//   - Implement `new(d) -> Option<Celsius>`: Some only if d >= -273, else None.
//   - Implement `get(&self) -> i32` returning the inner value.
// In check_3, the commented "struct literal" line must fail to compile if
// uncommented (E0451: field `degrees` is private) — that's the guarantee.
// ----------------------------------------------------------------------------

mod temperature {
    #[derive(Debug, PartialEq)]
    pub struct Celsius {
        // your turn: leave this private. No `pub`.
        degrees: i32,
    }

    impl Celsius {
        pub fn new(d: i32) -> Option<Celsius> {
            if d >= -273 {
                Some(Celsius { degrees: d })
            } else {
                None
            }
        }

        pub fn get(&self) -> i32 {
            self.degrees
        }
    }
}

fn check_3() {
    use temperature::Celsius;

    assert_eq!(Celsius::new(25).map(|c| c.get()), Some(25));
    assert_eq!(Celsius::new(-300), None); // invariant enforced by the constructor
    assert_eq!(Celsius::new(-273).map(|c| c.get()), Some(-273));

    // Try uncommenting this: building one directly bypasses the invariant, so
    // the compiler forbids it (E0451, private field). Re-comment after seeing it.
    // let _bogus = Celsius { degrees: -1000 };

    println!("rung 3 ok: private field + smart constructor guards the invariant");
}

// ----------------------------------------------------------------------------
// Rung 4 — `use` and `pub use`: aliases vs re-exports
//
// `use` is just a LOCAL alias — it shortens a path for the current module and
// does NOT change anyone else's view of the tree. `pub use` is different: it
// re-exports an item, making it reachable through a NEW path. That's how crates
// present a flat, friendly public surface over a deep internal tree.
//
// The real item lives deep: crate::deep::nested::core::Engine. Two jobs:
//
//   (a) In `start_deep()` below, use a `use` alias so you can write `Engine`
//       instead of the full path. (Local convenience only.)
//
//   (b) Add a `pub use` re-export at the crate root (a line right above
//       check_4, OUTSIDE any module) so that `crate::Engine` and
//       `crate::start_deep` both resolve — flattening the deep path into a
//       short public name. check_4 references `Engine` by its SHORT re-exported
//       path; make that path exist.
// ----------------------------------------------------------------------------

mod deep {
    pub mod nested {
        pub mod core {
            #[derive(Debug, PartialEq)]
            pub struct Engine {
                pub power: u32,
            }

            impl Engine {
                pub fn new(power: u32) -> Engine {
                    Engine { power }
                }
            }
        }
    }
}

fn start_deep() -> u32 {
    // your turn (a): add a `use` alias above this body (or inside it) so the
    // line below can say `Engine::new(...)` instead of the full nested path.
    // use crate::deep::nested::core::Engine;
    use crate::deep::nested::core::Engine;
    Engine::new(9000).power
}

// your turn (b): add a `pub use` here that re-exports the deep Engine type to
// the crate root, so `Engine` is reachable as a short top-level name.
// pub use deep::nested::core::Engine;
pub use deep::nested::core::Engine;

fn check_4() {
    // Short re-exported path resolves because of your `pub use`:
    let e = Engine::new(12);
    assert_eq!(e.power, 12);
    assert_eq!(start_deep(), 9000);
    println!("rung 4 ok: use aliased locally, pub use flattened the public path");
}

// ----------------------------------------------------------------------------
// Rung 5 — Leaking a private type through a public API
//
// The "private-in-public" rule: a `pub` function that returns (or takes) a type
// LESS visible than itself is a leak — an outside caller receives a value of a
// type they can't name or use. In a LIBRARY crate this fires the
// `private_interfaces` lint (and historically the hard error E0446). In a
// BINARY like this one, the leak instead bites at the USE SITE: callers can't
// NAME the private type (E0603) and can't read its private fields (E0616).
// Either way the message is the same: if you return it, you must publish it.
//
// `mod widget` has a private struct `Inner` and a pub fn `make` returning it.
// check_5 both names the type (`widget::Inner`) and reads `.id`, so right now
// you'll see E0603/E0616. Fix it by giving the type its HONEST visibility:
//   - Make `Inner` pub and its `id` field pub (it's genuinely part of the
//     public return), and implement `make(n)` to return an Inner with id == n.
// The lesson: the fix isn't "silence the error" — it's "if you return it, you
// must publish it". (Rung 7 shows the OTHER fix: don't return it; hide it.)
// ----------------------------------------------------------------------------

mod widget {
    // your turn: this currently leaks. Decide its visibility honestly — it's
    // returned by a pub fn, so it must be reachable by callers.
    pub struct Inner {
        pub id: u32,
    }

    pub fn make(n: u32) -> Inner {
        Inner { id: n }
    }
}

fn check_5() {
    let w: widget::Inner = widget::make(7); // naming the type requires it be pub (E0603)
    assert_eq!(w.id, 7); // reading the field requires it be pub (E0616)
    println!("rung 5 ok: a pub fn's return type must be at least as public as the fn");
}

// ----------------------------------------------------------------------------
// Rung 6 — Restricted visibility: pub(crate) / pub(super) / pub(in path)
//
// `pub` and private are the two extremes. Between them is a dial: you can make
// an item "public, but only up to HERE". The variants:
//   - pub(crate)      visible anywhere in THIS crate, but not to downstream
//                     crates. The workhorse for "shared internal API".
//   - pub(super)      visible to the PARENT module (and its descendants), no
//                     further up.
//   - pub(in path)    visible within exactly the named ancestor module subtree.
//
// The tree here is: engine -> { electrical, fuel }, plus a crate-root caller.
// Set each item's visibility so that EXACTLY the right callers can reach it:
//
//   - `secret_formula` in `fuel`: must be reachable from its parent `engine`
//     (so engine::diagnostics can call it) but NOT from the crate root.
//     => pub(super).
//   - `VERSION` in `engine`: shared everywhere in the crate (check_6 at the
//     root reads it) but conceptually not part of the downstream public API.
//     => pub(crate).
//   - `calibrate` in `electrical`: should be reachable only within the whole
//     `engine` subtree (engine and any descendant), expressed explicitly.
//     => pub(in crate::engine).
//
// Fill in the three bodies AND the three visibility markers. Then there's a
// "must-stay-broken" probe: a commented line in check_6 that tries to call
// secret_formula from the ROOT. Uncomment it to confirm pub(super) blocks the
// root (privacy error), then re-comment.
// ----------------------------------------------------------------------------

mod engine {
    // your turn: visible across the whole crate, not downstream.
    const VERSION: u32 = 3;

    pub const fn version_for_crate() -> u32 {
        // expose VERSION's value so the root can assert it even though VERSION
        // itself is pub(crate); also lets you keep VERSION's marker honest.
        VERSION
    }

    pub mod fuel {
        // your turn: reachable from parent `engine`, but NOT the crate root.
        pub(super) const fn secret_formula() -> u32 {
            42
        }

        // a tiny public wrapper so diagnostics can drive it from the parent
        pub fn run_formula() -> u32 {
            secret_formula()
        }
    }

    pub mod electrical {
        // your turn: reachable within the whole `engine` subtree, stated
        // explicitly with pub(in ...).
        pub(in crate::engine) const fn calibrate() -> u32 {
            7
        }

        pub fn calibrate_pub() -> u32 {
            calibrate()
        }
    }

    pub fn diagnostics() -> u32 {
        fuel::secret_formula() + electrical::calibrate()
    }
}

fn check_6() {
    assert_eq!(engine::version_for_crate(), 3);
    assert_eq!(engine::diagnostics(), 49); // 42 + 7
    assert_eq!(engine::fuel::run_formula(), 42);
    assert_eq!(engine::electrical::calibrate_pub(), 7);

    // Probe: this must NOT compile (secret_formula is pub(super), so the crate
    // root can't see it). Uncomment, observe the privacy error, re-comment.
    // let _ = engine::fuel::secret_formula();

    println!("rung 6 ok: pub(crate)/pub(super)/pub(in path) scoped visibility precisely");
}

// ----------------------------------------------------------------------------
// Rung 7 — The facade pattern: private internals, a curated public surface
//
// This is how mature crates are actually organized. The implementation lives in
// PRIVATE modules (`mod internal` with NO pub), so nothing inside is reachable
// from outside `api` by its real path. Then a thin facade RE-EXPORTS exactly the
// handful of items that form the public surface via `pub use`. Result: you can
// refactor/rename/move everything inside `internal` freely — as long as the
// re-exported names stay stable, no downstream code breaks. The module
// structure becomes an implementation detail, not part of the API.
//
// Inside `mod api` below:
//   - `mod internal` is PRIVATE (leave it without pub). It has two structs:
//     `Client` (the thing users should get) and `RawSocket` (a guts type users
//     should NOT see). Implement Client::connect and Client::ping.
//   - Curate the surface: add `pub use` lines in `api` that re-export ONLY
//     `Client` (not RawSocket). check_7 reaches `api::Client`; the commented
//     probe reaches `api::internal::RawSocket` and MUST fail (E0603) because
//     `internal` is private — proving the guts are sealed off.
// ----------------------------------------------------------------------------

mod api {
    mod internal {
        #[allow(dead_code)]
        struct RawSocket; // a guts type the public API must NOT expose

        pub struct Client {
            pub name: &'static str,
        }

        impl Client {
            pub fn connect(name: &'static str) -> Self {
                Self { name }
            }
            pub fn ping(&self) -> &'static str {
                "pong"
            }
        }
    }

    // your turn: re-export ONLY Client to form the public surface of `api`.
    // (Do NOT re-export RawSocket — it stays an internal detail.)
    pub use internal::Client;
}

fn check_7() {
    let c = api::Client::connect("db-1");
    assert_eq!(c.name, "db-1");
    assert_eq!(c.ping(), "pong");

    // Probe: `internal` is private, so its real path is unreachable from here.
    // Uncomment to confirm the guts are sealed (E0603), then re-comment.
    // let _ = api::internal::RawSocket;

    println!("rung 7 ok: facade re-exports the curated surface, guts stay private");
}

// ----------------------------------------------------------------------------
// Rung 8 — The sealed trait: a private module gates who can `impl`
//
// You've used privacy to control who can CALL things. This rung uses it to
// control who can IMPLEMENT a trait. The trick: make your public trait require
// a SUPERTRAIT that lives in a PRIVATE module. Outsiders can SEE your trait and
// CALL it, but they can't `impl` it for their own types, because they can't
// name (let alone impl) the private supertrait. Only code inside your module —
// which CAN reach the private supertrait — may add impls. This is exactly how
// std seals traits like `std::error::Error`-adjacent ones and how crates lock
// down extension points so they can evolve a trait without breaking downstream.
//
// Layout below:
//   - a PRIVATE module `sealed` holds `pub trait Sealed {}`. Because `sealed`
//     is private to `format`, downstream code can't name `format::sealed::Sealed`.
//   - the PUBLIC trait `Encoder: sealed::Sealed` requires it as a supertrait.
//   - `Json` and `Xml` are the blessed impls. To impl `Encoder` for them you
//     must FIRST impl the private `Sealed` for them (only possible in here).
//
// Your job:
//   - impl `sealed::Sealed` for both `Json` and `Xml` (empty bodies).
//   - impl `Encoder` for both: `encode` returns "json:<input>" / "xml:<input>".
// The commented probe in check_8 tries to impl Encoder for an OUTSIDER type
// without the sealed bound — it must fail (can't satisfy the private supertrait).
// ----------------------------------------------------------------------------

mod format {
    mod sealed {
        // public trait, but inside a PRIVATE module => unnameable downstream.
        pub trait Sealed {}
    }

    pub trait Encoder: sealed::Sealed {
        fn encode(&self, input: &str) -> String;
    }

    pub struct Json;
    pub struct Xml;

    // your turn: impl sealed::Sealed for Json and Xml (empty), THEN impl Encoder
    // for each so encode returns "json:<input>" and "xml:<input>".
    impl sealed::Sealed for Json {}
    impl sealed::Sealed for Xml {}
    impl Encoder for Json {
        fn encode(&self, input: &str) -> String {
            format!("json:{}", input)
        }
    }
    impl Encoder for Xml {
        fn encode(&self, input: &str) -> String {
            format!("xml:{}", input)
        }
    }
}

fn check_8() {
    use format::Encoder;

    assert_eq!(format::Json.encode("hi"), "json:hi");
    assert_eq!(format::Xml.encode("hi"), "xml:hi");

    // Probe (the seal): an outside type can't join the trait, because it can't
    // satisfy the private `Sealed` supertrait. Uncomment to see it rejected.
    // struct Rogue;
    // impl format::Encoder for Rogue {
    //     fn encode(&self, input: &str) -> String { format!("rogue:{input}") }
    // }
    // ^ error: the trait bound `Rogue: format::sealed::Sealed` is not satisfied
    //   (and you can't impl Sealed for Rogue — its module is private)

    println!("rung 8 ok: private-supertrait seal lets you impl in-crate, blocks downstream");
}

// ----------------------------------------------------------------------------
// Rung 9 — CAPSTONE: build a mini-library `inventory` with a real front door
//
// Put every tool together into one crate-in-a-file. The module tree is:
//
//   inventory                      (the crate-like root of this library)
//   ├── util        (PRIVATE)      shared helper used by other submodules
//   ├── model       (PRIVATE)      the data types + their invariants
//   │     ├── Sku   (pub type, PRIVATE field, smart constructor)
//   │     └── Item  (pub type holding a Sku + qty)
//   ├── store       (PRIVATE)      the warehouse logic over model types
//   │     └── Warehouse
//   └── (facade)    pub use ...    the curated PUBLIC surface
//
// Requirements — make check_9 pass using everything from rungs 1–8:
//
//  (a) util::normalize(code) -> String : trims and uppercases the code.
//      Mark it pub(crate) so OTHER submodules (model) can call it, but it's not
//      part of the public API. (model::Sku::new will call super::super::util or
//      crate::inventory::util — your choice of path.)
//
//  (b) model::Sku : a `code: String` field that MUST stay private. Constructor
//      `new(code: &str) -> Option<Sku>` returns Some only if the NORMALIZED code
//      is at least 3 chars; store the normalized form. Getter `code(&self)->&str`.
//
//  (c) model::Item : pub fields `sku: Sku`, `qty: u32` is fine here (it's a
//      plain data carrier). `new(sku, qty)` constructor.
//
//  (d) store::Warehouse : PRIVATE `items: Vec<Item>`. Methods:
//        - new() -> Warehouse
//        - add(&mut self, sku: Sku, qty: u32)   pushes an Item
//        - total(&self) -> u32                   sum of all qty
//        - quantity_of(&self, code: &str) -> u32 qty for items whose sku.code()
//          matches the NORMALIZED `code` (so "ab-1" and "AB-1 " both match).
//
//  (e) The facade: add `pub use` lines in `inventory` re-exporting ONLY
//      `Sku`, `Item`, and `Warehouse`. The submodules stay private, so the only
//      way in is through these names. check_9 uses inventory::{Sku,Item,Warehouse}.
//
// Two seal probes (commented) confirm the design holds — uncomment each, watch
// it fail, re-comment.
// ----------------------------------------------------------------------------

mod inventory {
    mod util {
        // your turn (a): pub(crate) so model can reach it; not public API.
        pub(crate) fn normalize(code: &str) -> String {
            code.trim().to_uppercase()
        }
    }

    mod model {
        use crate::inventory::util;

        #[derive(Debug, Clone, PartialEq)]
        pub struct Sku {
            // your turn (b): keep this private.
            code: String,
        }

        impl Sku {
            pub fn new(code: &str) -> Option<Sku> {
                if code.len() >= 3 {
                    return Some(Self {
                        code: util::normalize(code),
                    });
                }
                None
            }
            pub fn code(&self) -> &str {
                &self.code
            }
        }

        #[derive(Debug, Clone)]
        pub struct Item {
            pub sku: Sku,
            pub qty: u32,
        }

        impl Item {
            pub fn new(sku: Sku, qty: u32) -> Item {
                Self { sku, qty }
            }
        }
    }

    mod store {
        // your turn: bring the model types into scope here with `use`.
        // use super::model::{Sku, Item};

        use crate::inventory::util;

        pub struct Warehouse {
            // your turn (d): keep items private.
            items: Vec<super::model::Item>,
        }

        impl Warehouse {
            pub fn new() -> Warehouse {
                Self { items: Vec::new() }
            }
            pub fn add(&mut self, sku: super::model::Sku, qty: u32) {
                self.items.push(super::model::Item::new(sku, qty));
            }
            pub fn total(&self) -> u32 {
                self.items.iter().map(|i| i.qty).sum()
            }
            pub fn quantity_of(&self, code: &str) -> u32 {
                let normalized = util::normalize(code);
                self.items
                    .iter()
                    .filter(|i| i.sku.code() == &normalized)
                    .map(|i| i.qty)
                    .sum()
            }
        }
    }

    // your turn (e): re-export ONLY the curated surface.
    pub use model::{Item, Sku};
    pub use store::Warehouse;
}

fn check_9() {
    use inventory::{Item, Sku, Warehouse};

    // invariant: too-short codes are rejected; normalization is applied
    assert_eq!(Sku::new("ab"), None);
    let s = Sku::new("  ab-1 ").expect("valid sku");
    assert_eq!(s.code(), "AB-1"); // trimmed + uppercased

    let mut wh = Warehouse::new();
    wh.add(Sku::new("ab-1").unwrap(), 5);
    wh.add(Sku::new("ab-1").unwrap(), 3);
    wh.add(Sku::new("xy-9").unwrap(), 10);

    assert_eq!(wh.total(), 18);
    // query with messy casing/spacing still matches via normalize:
    assert_eq!(wh.quantity_of(" AB-1 "), 8);
    assert_eq!(wh.quantity_of("xy-9"), 10);
    assert_eq!(wh.quantity_of("nope"), 0);

    // Item is part of the public surface and carries a Sku:
    let it = Item::new(Sku::new("zz-0").unwrap(), 1);
    assert_eq!(it.sku.code(), "ZZ-0");

    // --- Seal probes (uncomment each, confirm it fails, re-comment) ---
    // 1) submodules are private — the real internal path is unreachable:
    // let _ = inventory::store::Warehouse::new();
    // 2) the invariant can't be bypassed — Sku's field is private:
    // let _bad = inventory::Sku { code: String::from("x") };

    println!("rung 9 ok: full module tree — private internals, curated front door");
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
