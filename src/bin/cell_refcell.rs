// Concept: Cell<T> and RefCell<T> — interior mutability
// Run: cargo run --bin cell_refcell
//
// Mental model: normally Rust enforces "many &T XOR one &mut T" at COMPILE time.
// Interior mutability lets you mutate through a shared &T, upholding the rule a
// different way:
//   - Cell<T>:    no references handed out — only copy values in/out. No runtime cost.
//   - RefCell<T>: hands out real &/&mut, but checks the borrow rule at RUNTIME (panics).
//
// Ladder (DONE marks finished rungs):
//   1. cell_basics        - mutate a Copy value through & with get/set            [DONE]
//   2. refcell_basics     - borrow()/borrow_mut() to push into a Vec through &    [DONE]
//   3. cell_toolbox       - replace, take, into_inner, update; Cell<Option<T>>    [DONE]
//   4. refcell_toolbox    - &self methods, many coexisting borrows, try_borrow    [DONE]
//   5. borrow_panic       - overlap borrow_mut with a borrow -> runtime panic     [DONE]
//   6. sync_and_reentrancy- !Sync, and a Ref held across a callback panics        [DONE]
//   7. rc_refcell_graph   - Rc<RefCell<T>> shared mutable state                   [DONE]
//   8. ref_map_projection - Ref::map to borrow a single field                     [DONE]
//   9. my_refcell         - build RefCell from scratch (UnsafeCell + flag)        [DONE] <-- capstone

use std::cell::{Cell, Ref, RefCell, RefMut};

// ── Rung 1: Cell basics ───────────────────────────────────────────────────────
// `bump` takes a SHARED reference (&Cell<u32>) yet must mutate the value inside.
// Use Cell's get/set to read the current value, add `by`, and store it back.
// Note the signature: &Cell, not &mut Cell — that's the whole point.
fn bump(counter: &Cell<u32>, by: u32) {
    counter.set(counter.get() + by);
}

fn check_1() {
    let counter = Cell::new(0u32);
    let r1 = &counter; // two shared refs...
    let r2 = &counter;
    bump(r1, 5);
    bump(r2, 3); // ...both can drive a mutation. No &mut anywhere.
    assert_eq!(counter.get(), 8);
    println!("rung 1 ok: cell now = {}", counter.get());
}

// ── Rung 2: RefCell basics ────────────────────────────────────────────────────
// Cell only moves whole values in/out — useless for a Vec you want to push into.
// RefCell hands out real references via borrow() (-> Ref<T>) and borrow_mut()
// (-> RefMut<T>), and checks the borrow rules at runtime instead of compile time.
//
// `log` takes a SHARED &RefCell<Vec<String>> and must push `msg` onto the Vec.
fn log(entries: &RefCell<Vec<String>>, msg: &str) {
    entries.borrow_mut().push(msg.to_string());
}

fn check_2() {
    let entries = RefCell::new(Vec::<String>::new());
    let r = &entries; // again: shared ref, yet we mutate through it
    log(r, "boot");
    log(&entries, "ready");
    // borrow() hands out a Ref<Vec<String>>; deref it to read the Vec.
    assert_eq!(entries.borrow().len(), 2);
    assert_eq!(entries.borrow()[0], "boot");
    println!("rung 2 ok: entries = {:?}", entries.borrow());
}

// ── Rung 3: Cell toolbox ──────────────────────────────────────────────────────
// `get()` needs T: Copy, so it can't move a String out of a Cell. The toolbox
// solves that by swapping values instead of copying:
//   - replace(new) -> old      : store `new`, hand back the old value
//   - take()       -> T        : store T::default(), hand back the old (needs Default)
//   - update(f)                : set(f(get())) — only on Copy types (stable since 1.88)
//   - into_inner() -> T        : consume the Cell, get the value out
//
// (a) `rotate`: store `new` in the slot and RETURN the previous value.
fn rotate(slot: &Cell<i32>, new: i32) -> i32 {
    slot.replace(new)
}

// (b) `steal`: move the String OUT of the cell, leaving None behind, and return it.
//     Cell<Option<T>> is the classic trick to move a non-Copy value out through &.
fn steal(slot: &Cell<Option<String>>) -> Option<String> {
    slot.take()
}

fn check_3() {
    let slot = Cell::new(10);
    let old = rotate(&slot, 99);
    assert_eq!(old, 10);
    assert_eq!(slot.get(), 99);

    // update: read-modify-write a Copy value in place.
    slot.update(|v| v + 1);
    assert_eq!(slot.get(), 100);

    let name = Cell::new(Some(String::from("ferris")));
    assert_eq!(steal(&name), Some(String::from("ferris")));
    assert_eq!(steal(&name), None); // it was left as None
    assert!(name.into_inner().is_none()); // consume the Cell
    println!("rung 3 ok: toolbox (replace/take/update/into_inner)");
}

// ── Rung 4: RefCell toolbox & the "&self that mutates" pattern ────────────────
// The real reason RefCell exists: it lets a type expose methods that take &self
// (look read-only to callers) but mutate internal state. Think caches, loggers,
// lazy init. Here you build a tiny Stats with all-`&self` methods.
//
// Also: borrow() can be called MANY times at once (many readers OK); try_borrow /
// try_borrow_mut return a Result instead of panicking when the rule would break.
struct Stats {
    samples: RefCell<Vec<i32>>,
}

impl Stats {
    fn new() -> Self {
        Stats {
            samples: RefCell::new(Vec::new()),
        }
    }

    // Each of these takes &self — note: NOT &mut self.
    fn add(&self, n: i32) {
        self.samples.borrow_mut().push(n);
    }

    fn len(&self) -> usize {
        self.samples.borrow().len()
    }

    fn sum(&self) -> i32 {
        self.samples.borrow().iter().sum()
    }
}

fn check_4() {
    let s = Stats::new();
    s.add(3);
    s.add(4);
    s.add(5);
    assert_eq!(s.len(), 3);
    assert_eq!(s.sum(), 12);

    // Many simultaneous read borrows are fine — both Refs alive at once:
    let a = s.samples.borrow();
    let b = s.samples.borrow();
    assert_eq!(a.len(), b.len());

    // While a read borrow (`a`) is alive, a mutable borrow would break the rule.
    // try_borrow_mut returns Err instead of panicking — proof the flag is live.
    assert!(s.samples.try_borrow_mut().is_err());
    drop(a);
    drop(b);
    // now that all read borrows are gone, a mutable borrow succeeds:
    assert!(s.samples.try_borrow_mut().is_ok());
    println!("rung 4 ok: &self methods + coexisting borrows + try_borrow");
}

// ── Rung 5: the defining footgun — runtime borrow panic ───────────────────────
// The whole bargain of RefCell: the borrow check moves to RUNTIME. Break the
// "one writer XOR many readers" rule and you don't get a compile error — you get
// a panic: "already borrowed" / "already mutably borrowed".
//
// (a) WITNESS IT. Make `trigger_panic` actually panic by holding a read borrow
//     alive in a `let` binding and then asking for borrow_mut() while it lives.
//     Hint shape:
//         let r = v.borrow();        // Ref alive for the rest of the scope
//         v.borrow_mut().push(...);  // BOOM: still borrowed by `r`
//     (check_5 runs this inside catch_unwind and asserts it panicked.)
fn trigger_panic(v: &RefCell<Vec<i32>>) {
    let _r = v.borrow();
    v.borrow_mut().push(1);
}

// (b) FIX THE SHAPE. `duplicate_first` should append a copy of the first element.
//     The naive version panics for the same reason. Make it NOT panic by ending
//     the read borrow before you take the write borrow (copy the value out first,
//     or scope the borrow with { } so the Ref is dropped).
fn duplicate_first(v: &RefCell<Vec<i32>>) {
    let first = v.borrow()[0];
    v.borrow_mut().push(first);
}

fn check_5() {
    // (a) prove the panic happens
    let v = RefCell::new(vec![1, 2, 3]);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        trigger_panic(&v);
    }))
    .is_err();
    assert!(
        panicked,
        "trigger_panic should have panicked from overlapping borrows"
    );

    // (b) the fixed version must run cleanly and duplicate the first element
    let w = RefCell::new(vec![10, 20]);
    duplicate_first(&w);
    assert_eq!(*w.borrow(), vec![10, 20, 10]);
    println!("rung 5 ok: saw the borrow panic, then fixed it by scoping the borrow");
}

// ── Rung 6: !Sync, and re-entrant borrow through a callback ───────────────────
// Two edges where RefCell bites:
//
// (1) RefCell<T> is !Sync — its borrow flag is a plain Cell with no atomics, so
//     sharing &RefCell across threads is forbidden AT COMPILE TIME. Uncomment the
//     block below to see the error ("`RefCell<i32>` cannot be shared between
//     threads safely"). This is why threads use Mutex/RwLock, not RefCell.
//
//     let shared = RefCell::new(0);
//     std::thread::scope(|s| {
//         s.spawn(|| { *shared.borrow_mut() += 1; }); // <-- compile error
//     });
//
// (2) Re-entrancy: a read borrow held across a CALLBACK that re-borrows mutably.
//     The two borrows aren't adjacent — the mutable one is buried in the closure.
//
// (a) Implement `each`: iterate the Vec and call f(x) for each element. The
//     natural impl holds a read borrow (v.borrow()) alive for the whole loop —
//     leave it that way; that's the hazard we want.
fn each<F: FnMut(i32)>(v: &RefCell<Vec<i32>>, mut f: F) {
    for &x in v.borrow().iter() {
        f(x);
    }
}

// Provided: this calls `each`, and the closure tries to mutate the SAME RefCell.
// Because `each` is still holding the read borrow, this re-entrant borrow_mut panics.
fn double_into_buggy(v: &RefCell<Vec<i32>>) {
    each(v, |x| {
        v.borrow_mut().push(x * 2); // re-entrant: panics while `each`'s borrow lives
    });
}

// (b) Implement `double_into_fixed`: append the double of every existing element,
//     WITHOUT a re-entrant borrow. Snapshot what you need (release the read borrow),
//     THEN mutate.
fn double_into_fixed(v: &RefCell<Vec<i32>>) {
    let doubles = v.borrow().iter().map(|x| x * 2).collect::<Vec<_>>();
    v.borrow_mut().extend(doubles);
}

fn check_6() {
    // (a) the re-entrant version must panic
    let v = RefCell::new(vec![1, 2, 3]);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        double_into_buggy(&v);
    }))
    .is_err();
    assert!(
        panicked,
        "double_into_buggy should panic on re-entrant borrow"
    );

    // (b) the snapshot version runs clean
    let w = RefCell::new(vec![1, 2, 3]);
    double_into_fixed(&w);
    assert_eq!(*w.borrow(), vec![1, 2, 3, 2, 4, 6]);

    // sanity: `each` itself works for a read-only callback
    let mut total = 0;
    each(&w, |x| total += x);
    assert_eq!(total, 1 + 2 + 3 + 2 + 4 + 6);
    println!("rung 6 ok: !Sync (see comment) + re-entrant borrow panic, then fixed");
}

// ── Rung 7: Rc<RefCell<T>> — shared, mutable state ────────────────────────────
// The combo you came for. Rc gives MANY owners; RefCell gives mutation through a
// shared &. Together: multiple handles to the same data, any of which can mutate
// it. This is how you build graphs/trees/observer state in safe single-threaded
// Rust (the threaded analogue is Arc<Mutex<T>>).
use std::rc::Rc;

#[derive(Debug)]
struct Node {
    value: i32,
    children: Vec<Rc<RefCell<Node>>>,
}

// (a) make a leaf node wrapped for sharing+mutation. Return type says it all.
fn new_node(value: i32) -> Rc<RefCell<Node>> {
    Rc::new(RefCell::new(Node {
        value,
        children: vec![],
    }))
}

// (b) push `child` into `parent`'s children. parent is a shared handle (&Rc<...>),
//     so reach the Node via borrow_mut().
fn add_child(parent: &Rc<RefCell<Node>>, child: Rc<RefCell<Node>>) {
    parent.borrow_mut().children.push(child);
}

// (c) recursively sum this node's value plus all descendants' values.
fn sum_tree(node: &Rc<RefCell<Node>>) -> i32 {
    let node = node.borrow();
    let value = node.value;
    let children = node
        .children
        .iter()
        .map(|child| sum_tree(child))
        .sum::<i32>();
    value + children
}

fn check_7() {
    let root = new_node(1);
    let a = new_node(2);
    let b = new_node(3);
    add_child(&root, Rc::clone(&a)); // root now co-owns `a`...
    add_child(&root, Rc::clone(&b));

    assert_eq!(sum_tree(&root), 1 + 2 + 3);

    // The payoff: mutate the node through the SEPARATE handle `a`, and the tree
    // reached via `root` sees it — same underlying RefCell, two owners.
    a.borrow_mut().value = 20;
    assert_eq!(sum_tree(&root), 1 + 20 + 3);

    // ...and Rc proves the shared ownership: `a` is owned by both `a` and root.
    assert_eq!(Rc::strong_count(&a), 2);
    println!(
        "rung 7 ok: Rc<RefCell<Node>> shared mutable tree, sum = {}",
        sum_tree(&root)
    );
}

// ── Rung 8: Ref::map / RefMut::map — projecting a borrow ──────────────────────
// Problem: you want a function that hands back a borrow of ONE FIELD of a
// RefCell's contents. You can't return `&str`/`&mut u32` directly — the Ref guard
// would drop at the end of the function and the borrow flag would reset, so the
// reference would dangle (it won't even compile).
//
// Solution: Ref::map turns a Ref<T> into a Ref<U> by projecting through a closure,
// KEEPING the borrow guard alive. The returned Ref<U> still holds the flag down.
// (RefMut::map does the same for the mutable side.)
struct Config {
    name: String,
    retries: u32,
}

// (a) return a read borrow of just the `name`, as Ref<'_, str>.
//     Ref::map(guard, |cfg| ...project to &cfg.name...).
fn borrow_name(c: &RefCell<Config>) -> Ref<'_, str> {
    Ref::map(c.borrow(), |cfg| cfg.name.as_str())
}

// (b) return a MUTABLE borrow of just `retries`, as RefMut<'_, u32>.
fn borrow_retries_mut(c: &RefCell<Config>) -> RefMut<'_, u32> {
    RefMut::map(c.borrow_mut(), |cfg| &mut cfg.retries)
}

fn check_8() {
    let cfg = RefCell::new(Config {
        name: "prod".to_string(),
        retries: 3,
    });

    {
        let name = borrow_name(&cfg); // a Ref<str> — borrow flag is held down here
        assert_eq!(&*name, "prod");
        // proof the projected Ref still locks the cell: a write must fail right now
        assert!(cfg.try_borrow_mut().is_err());
    } // name dropped -> flag released

    {
        let mut r = borrow_retries_mut(&cfg);
        *r += 1; // mutate just the projected field
    }
    assert_eq!(cfg.borrow().retries, 4);
    println!("rung 8 ok: Ref::map / RefMut::map projected a single field");
}

// ── Rung 9 (CAPSTONE): build MyRefCell<T> from scratch ────────────────────────
// Now reimplement RefCell's machinery. Three pieces:
//   1. UnsafeCell<T>      — the ONLY legal way to get a *mut T from a shared &.
//                           (&T -> &mut T any other way is instant UB.)
//   2. a borrow flag      — a Cell<isize> tracking the borrow state:
//                              0  = free
//                             >0  = that many shared borrows are out
//                             -1  = one mutable borrow is out
//   3. RAII guard types   — MyRef / MyRefMut. Deref to the data; their Drop
//                           restores the flag. THIS is why borrows auto-release.
//
// Implement every todo!() below. The flag rules you must enforce:
//   - borrow():     panic if flag < 0 (a writer is out), else flag += 1.
//   - borrow_mut(): panic if flag != 0 (anyone is out), else flag = -1.
//   - MyRef::drop:     flag -= 1.
//   - MyRefMut::drop:  flag = 0.
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};

struct MyRefCell<T> {
    value: UnsafeCell<T>,
    flag: Cell<isize>,
}

struct MyRef<'a, T> {
    cell: &'a MyRefCell<T>,
}

struct MyRefMut<'a, T> {
    cell: &'a MyRefCell<T>,
}

impl<T> MyRefCell<T> {
    fn new(value: T) -> Self {
        MyRefCell {
            value: UnsafeCell::new(value),
            flag: Cell::new(0),
        }
    }

    fn borrow(&self) -> MyRef<'_, T> {
        if self.flag.get() < 0 {
            panic!("already mutably borrowed");
        }
        self.flag.set(self.flag.get() + 1);
        MyRef { cell: self }
    }

    fn borrow_mut(&self) -> MyRefMut<'_, T> {
        if self.flag.get() != 0 {
            panic!("already borrowed");
        }
        self.flag.set(-1);
        MyRefMut { cell: self }
    }
}

// Reads go through the *const from UnsafeCell::get(). Safe here because the flag
// guarantees no &mut is out while this MyRef lives.
impl<T> Deref for MyRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.cell.value.get() }
    }
}

impl<T> Deref for MyRefMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &mut *self.cell.value.get() }
    }
}

impl<T> DerefMut for MyRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.cell.value.get() }
    }
}

// The heart of auto-release: dropping a guard fixes the flag back up.
impl<T> Drop for MyRef<'_, T> {
    fn drop(&mut self) {
        self.cell.flag.set(self.cell.flag.get() - 1);
    }
}

impl<T> Drop for MyRefMut<'_, T> {
    fn drop(&mut self) {
        self.cell.flag.set(0);
    }
}

fn check_9() {
    let c = MyRefCell::new(vec![1, 2, 3]);

    // many shared borrows coexist
    {
        let a = c.borrow();
        let b = c.borrow();
        assert_eq!(a.len(), 3);
        assert_eq!(b[0], 1);
        // a writer must be refused while readers are out
        let blew = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _w = c.borrow_mut();
        }))
        .is_err();
        assert!(blew, "borrow_mut should panic while shared borrows are out");
    } // a, b dropped -> flag back to 0

    // now a mutable borrow works and actually mutates the data
    {
        let mut m = c.borrow_mut();
        m.push(4);
        // while the writer is out, a reader must be refused
        let blew = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _r = c.borrow();
        }))
        .is_err();
        assert!(blew, "borrow should panic while a mutable borrow is out");
    } // m dropped -> flag back to 0

    assert_eq!(*c.borrow(), vec![1, 2, 3, 4]);
    println!("rung 9 ok: hand-rolled MyRefCell — UnsafeCell + flag + RAII guards 🎉");
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
