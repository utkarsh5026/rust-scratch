//! `Rc<RefCell<T>>` patterns — shared *mutable* state in single-threaded Rust,
//! and its costs.
//!
//! Run: `cargo run --bin rc_refcell`
//!
//! The stack: `Rc` = many owners (aliasing). `RefCell` = mutate behind `&`,
//! borrow-checked at *runtime*. The tension between them is the whole lesson.
//!
//! Ladder (DONE marked):
//!   1. [x] foundations — shared cell: mutate via handle A, read via handle B
//!   2. [x] foundations — two owner structs share one Rc<RefCell<Vec>>
//!   3. [x] mechanics   — strong_count, ptr_eq, &Rc vs clone, reach-inside
//!   4. [x] footgun     — double borrow_mut on the same cell -> BorrowMutError
//!   5. [x] footgun     — borrow held across a call / reentrancy -> panic
//!   6. [x] footgun     — Rc<RefCell> cycle: Drop never runs (the leak)
//!   7. [x] real-world  — Weak fix + tree with parent (Weak) / children (Rc)
//!   8. [x] real-world  — observer: Subject notifies Rc<RefCell> observers
//!   9. [x] capstone    — doubly-linked list (Rc next / Weak prev), clean Drop
//!  10. [x] capstone+   — iterative Drop: don't stack-overflow on a long chain

use std::cell::RefCell;
use std::rc::Rc;

// ───────────────────────── Rung 1: the shared-cell "aha" ─────────────────────
//
// Build ONE value that two handles share, then prove a mutation through one
// handle is visible through the other. This is the entire point of the idiom.
//
// Return a tuple of two handles `(a, b)` that both point at the *same* cell
// holding the i32 `start`. (Hint: make one Rc<RefCell<i32>>, then clone it.)
fn shared_cell(start: i32) -> (Rc<RefCell<i32>>, Rc<RefCell<i32>>) {
    let original = Rc::new(RefCell::new(start));
    let cloned = original.clone();
    (original, cloned)
}

fn check_1() {
    let (a, b) = shared_cell(10);

    // mutate THROUGH a...
    *a.borrow_mut() += 5;
    // ...and see it THROUGH b — same underlying cell.
    assert_eq!(*b.borrow(), 15);

    // and the reverse direction
    *b.borrow_mut() = 100;
    assert_eq!(*a.borrow(), 100);

    // they really are the same allocation, not two copies
    assert!(Rc::ptr_eq(&a, &b));
    println!("rung 1 ok: one cell, two handles, shared mutation 👀");
}

// ──────────────── Rung 2: two owner structs share one cell ───────────────────
//
// In rung 1 the two handles were loose locals. The real pattern is: separate
// *owners* each hold a handle to the same shared state. Here a `Logger` and an
// `Auditor` both record into ONE shared event log.
//
// `Log` is just a type alias for the shared handle. Implement:
//   - `Logger::new(log)` / `Auditor::new(log)` — store the handle (clone-share).
//   - `Logger::record(&self, msg)` — push `msg` onto the shared Vec.
//   - `Auditor::count(&self)` — return how many events the shared log holds.
// Note both methods take `&self` — mutation goes through the RefCell, not `&mut`.

type Log = Rc<RefCell<Vec<String>>>;

struct Logger {
    log: Log,
}

struct Auditor {
    log: Log,
}

impl Logger {
    fn new(log: &Log) -> Self {
        Self { log: log.clone() }
    }

    fn record(&self, msg: &str) {
        self.log.borrow_mut().push(msg.to_string());
    }
}

impl Auditor {
    fn new(log: &Log) -> Self {
        Self { log: log.clone() }
    }
    fn count(&self) -> usize {
        self.log.borrow().len()
    }
}

fn check_2() {
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let logger = Logger::new(&log);
    let auditor = Auditor::new(&log);

    logger.record("user logged in");
    logger.record("user clicked buy");

    // the Auditor — a totally separate object — sees what the Logger wrote
    assert_eq!(auditor.count(), 2);

    // and the original handle sees it too: 3 owners of one Vec
    assert_eq!(log.borrow().len(), 2);
    assert_eq!(Rc::strong_count(&log), 3);
    println!("rung 2 ok: separate owners, one shared log 📒");
}

// ─────────────── Rung 3: mechanics — counts, ptr_eq, &Rc vs clone ────────────
//
// Time to be precise about what's cheap and what aliases what. You'll write two
// helpers that take the shared handle *by reference* (no clone needed just to
// peek) and one that deliberately makes a sharing clone.
//
//   - `peek_count(h)`  -> the current strong_count WITHOUT adding an owner.
//   - `bump(h)`        -> add `n` to the shared i32, through the cell.
//   - `make_sibling(h)`-> return a NEW handle to the SAME cell (a sharing clone).
//
// The lesson: passing `&Rc<RefCell<T>>` lets you read/mutate the shared value
// without changing the owner count; only `.clone()` creates a new owner. And
// `borrow_mut()` "reaches inside" — the Rc layer is just refcounting, the
// RefCell layer is where the mutation actually happens.
type Counter = Rc<RefCell<i32>>;

fn peek_count(h: &Counter) -> usize {
    Rc::strong_count(h)
}

fn bump(h: &Counter, n: i32) {
    *h.borrow_mut() += n;
}

fn make_sibling(h: &Counter) -> Counter {
    Rc::clone(h)
}

fn check_3() {
    let h: Counter = Rc::new(RefCell::new(0));
    assert_eq!(peek_count(&h), 1); // sole owner

    // borrowing the handle to mutate does NOT add an owner
    bump(&h, 10);
    bump(&h, 5);
    assert_eq!(peek_count(&h), 1);
    assert_eq!(*h.borrow(), 15);

    // a sibling is a second owner of the same cell
    let sib = make_sibling(&h);
    assert_eq!(peek_count(&h), 2);
    assert!(Rc::ptr_eq(&h, &sib));

    // mutate via the sibling, observe via the original — same cell
    bump(&sib, 100);
    assert_eq!(*h.borrow(), 115);

    // drop the sibling -> back to one owner; the value survives (h still owns it)
    drop(sib);
    assert_eq!(peek_count(&h), 1);
    assert_eq!(*h.borrow(), 115);
    println!("rung 3 ok: &Rc peeks, clone owns, borrow_mut reaches inside 🔬");
}

// ───────── Rung 4: footgun — two live borrows of the same cell PANIC ─────────
//
// THE defining footgun of this idiom. `Rc` hands you N handles to one cell, so
// nothing at COMPILE time stops you from borrowing it twice at once. But
// `RefCell` still enforces "one &mut XOR many &" — at RUNTIME, by panicking.
//
// `try_double_mut` receives two handles `x` and `y` that MAY alias the same
// cell. It must attempt to hold a mutable borrow of BOTH at the same time and
// add `add` through each. Use `try_borrow_mut()` (the non-panicking form) so we
// can observe the failure as a value instead of a crash:
//   - return Ok(()) if it genuinely managed two simultaneous &mut (x and y are
//     different cells), having applied both additions;
//   - return Err(()) if the second borrow was refused (x and y alias).
//
// Key: you must hold the FIRST borrow alive (in a binding) while taking the
// SECOND — that overlap is what triggers the conflict. If you scope them so they
// don't overlap, nothing fails and you'll learn nothing.
fn try_double_mut(x: &Counter, y: &Counter, add: i32) -> Result<(), ()> {
    let mut first = x.borrow_mut();
    let mut second = y.try_borrow_mut().map_err(|_| ())?;

    *first += add;
    *second += add;
    Ok(())
}

fn check_4() {
    // Distinct cells: two independent &mut succeed.
    let a: Counter = Rc::new(RefCell::new(0));
    let b: Counter = Rc::new(RefCell::new(0));
    assert_eq!(try_double_mut(&a, &b, 5), Ok(()));
    assert_eq!(*a.borrow(), 5);
    assert_eq!(*b.borrow(), 5);

    // Aliased cell: the SAME RefCell borrowed mutably twice -> refused.
    let h: Counter = Rc::new(RefCell::new(0));
    let alias = Rc::clone(&h);
    assert_eq!(try_double_mut(&h, &alias, 5), Err(()));
    // and because the second borrow failed, no partial mutation leaked through
    // in a way that double-counts: the value is unchanged (the fn bailed).
    assert_eq!(*h.borrow(), 0);

    // Sanity: the classic version really does panic. We catch it to prove it.
    let h2: Counter = Rc::new(RefCell::new(0));
    let alias2 = Rc::clone(&h2);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _first = h2.borrow_mut();
        let _second = alias2.borrow_mut(); // <- BorrowMutError, panics
    }))
    .is_err();
    assert!(panicked, "two live borrow_mut on the same cell must panic");
    println!("rung 4 ok: Rc aliases freely, RefCell panics at runtime 💥");
}

// ──────── Rung 5: footgun — borrow held across a call (reentrancy) ───────────
//
// Rung 4's double-borrow was obvious because both borrows were right there. The
// version that actually bites people in real code is INDIRECT: you hold a
// borrow, then call a method that — somewhere down the line — borrows the SAME
// cell again. The cell doesn't know it's "the same logical operation"; it just
// sees a second borrow while the first is live, and panics.
//
// Here's a `Bank` of accounts, each an `Rc<RefCell<Account>>`. An account can be
// `linked` to an "overdraft backup" account (another handle, possibly aliasing).
//
//   struct Account { balance: i32, backup: Option<Acct> }
//
// Implement `withdraw(acct, amount)`:
//   - borrow the account mutably,
//   - if balance >= amount: subtract and return Ok(new_balance);
//   - else if it has a backup: try to pull the shortfall from the backup by
//     calling `withdraw(&backup, shortfall)` RECURSIVELY, then ...
//
// THE TRAP: the naive way holds the `borrow_mut()` of `acct` alive across the
// recursive `withdraw(&backup, ...)` call. If `backup` aliases `acct` (a self-
// referential backup, which the check sets up), that recursive call tries to
// borrow the same cell again -> panic.
//
// Your job has TWO parts:
//   (a) Write the NAIVE version first and SEE it panic on the self-backup case
//       (comment in check_5 walks you through it).
//   (b) Then fix it so the borrow of `acct` is RELEASED before the recursive
//       call. Read what you need out of the cell, DROP the guard, then recurse.
//   Return Err("insufficient") if neither the account nor its backup can cover.
type Acct = Rc<RefCell<Account>>;

struct Account {
    balance: i32,
    backup: Option<Acct>,
}

impl Account {
    fn new(balance: i32) -> Acct {
        Rc::new(RefCell::new(Account {
            balance,
            backup: None,
        }))
    }
}

fn withdraw(acct: &Acct, amount: i32) -> Result<i32, &'static str> {
    let (shortfall, backup) = {
        let mut account = acct.borrow_mut();

        if account.balance >= amount {
            account.balance -= amount;
            return Ok(account.balance);
        }

        let shortfall = amount - account.balance;
        account.balance = 0;
        (shortfall, account.backup.clone())
    };

    let Some(backup) = backup else {
        return Err("insufficient");
    };

    if Rc::ptr_eq(acct, &backup) {
        return Err("insufficient");
    }

    withdraw(&backup, shortfall)?;
    Ok(0)
}

fn check_5() {
    // Simple case: enough funds, no backup needed.
    let a = Account::new(100);
    assert_eq!(withdraw(&a, 30), Ok(70));
    assert_eq!(a.borrow().balance, 70);

    // Backup chain: `main` has 50, backup `reserve` has 100.
    let reserve = Account::new(100);
    let main_acct = Account::new(50);
    main_acct.borrow_mut().backup = Some(Rc::clone(&reserve));

    // withdraw 120: 50 from main (-> 0), shortfall 70 pulled from reserve.
    assert_eq!(withdraw(&main_acct, 120), Ok(0));
    assert_eq!(main_acct.borrow().balance, 0);
    assert_eq!(reserve.borrow().balance, 30);

    // THE REENTRANCY CASE: an account whose backup is ITSELF (alias). The naive
    // "hold borrow across recursive call" version panics here. The fixed version
    // must NOT panic — it should just find the cell already drained and report
    // insufficient (it can't double-spend the same balance).
    let weird = Account::new(40);
    weird.borrow_mut().backup = Some(Rc::clone(&weird)); // self-backup alias
    let did_not_panic =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| withdraw(&weird, 100)));
    assert!(
        did_not_panic.is_ok(),
        "withdraw must release its borrow before recursing, or self-backup panics"
    );
    // 40 available, asked 100 -> can't cover even via self -> Err.
    assert_eq!(did_not_panic.unwrap(), Err("insufficient"));
    println!("rung 5 ok: don't hold a borrow across a call that re-borrows 🔁");
}

// ─────────── Rung 6: footgun — the reference cycle that never frees ──────────
//
// The runtime borrow panic is loud — you find it fast. THIS footgun is silent:
// it's a memory leak. `Rc` frees its value only when strong_count hits 0. If two
// nodes hold `Rc` handles to EACH OTHER, each keeps the other's count at >=1
// forever — even after every external handle is gone. Destructors never run.
//
// `Node` carries a `name`, a `Drop` impl that records the drop into a shared
// log, and a `links: Vec<Link>` of strong handles to other nodes.
//
//   struct Node { name: String, links: Vec<Link>, dropped: DropLog }
//
// Implement just TWO tiny things:
//   - `Node::new(name, log)` -> a fresh `Link` (Rc<RefCell<Node>>) with no links.
//   - `link(a, b)` -> push a STRONG clone of `b` into a's `links` (a points to b).
//
// Then check_6 builds a -> b -> a, drops the external handles, and asserts that
// NOTHING was dropped — proving the leak. (DropLog is a shared Vec<String> the
// Drop impl pushes the node's name into, so we can observe who got freed.)
type Link = Rc<RefCell<Node>>;
type DropLog = Rc<RefCell<Vec<String>>>;

struct Node {
    name: String,
    links: Vec<Link>,
    dropped: DropLog,
}

impl Drop for Node {
    fn drop(&mut self) {
        self.dropped.borrow_mut().push(self.name.clone());
    }
}

fn make_node(name: &str, log: &DropLog) -> Link {
    Rc::new(RefCell::new(Node {
        name: name.to_string(),
        links: Vec::new(),
        dropped: Rc::clone(log),
    }))
}

fn link(a: &Link, b: &Link) {
    a.borrow_mut().links.push(Rc::clone(b));
}

fn check_6() {
    let log: DropLog = Rc::new(RefCell::new(Vec::new()));

    {
        let a = make_node("a", &log);
        let b = make_node("b", &log);

        link(&a, &b); // a -> b
        link(&b, &a); // b -> a   (now it's a cycle)

        // Each node is kept alive by TWO owners: the local + the other node's link.
        assert_eq!(Rc::strong_count(&a), 2);
        assert_eq!(Rc::strong_count(&b), 2);
        // locals a, b dropped here at end of scope...
    }

    // ...but the cycle keeps both counts at 1, so NEITHER Node::drop ever ran.
    // The allocation is leaked: unreachable, but never freed.
    assert!(
        log.borrow().is_empty(),
        "cycle should leak: no Drop should have fired, but got {:?}",
        log.borrow()
    );
    println!("rung 6 ok: a<->b strong cycle leaks — Drop never fires 🕳️");
}

// ────────── Rung 7: real-world — Weak breaks the cycle (parent tree) ─────────
//
// The fix for rung 6, applied to the most common real shape: a TREE where a
// parent owns its children, and each child can navigate back up to its parent.
//
// The ownership rule that makes it leak-free:
//   - parent -> child  : STRONG (Rc).  The parent OWNS the child; the child must
//     live as long as the parent references it.
//   - child  -> parent : WEAK (Weak).  The child OBSERVES its parent; it must
//     NOT keep the parent alive, or you recreate the rung-6 cycle.
//
// A `Weak<T>` is a non-owning handle: it doesn't bump strong_count, and you must
// `upgrade()` it (-> Option<Rc<T>>) to use it, which yields None if the target
// was already dropped.
//
//   struct TreeNode { value, parent: Weak<RefCell<TreeNode>>, children: Vec<Tree>, dropped }
//
// Implement:
//   - `tree_node(value, log) -> Tree` : leaf with an EMPTY weak parent
//     (Weak::new()) and no children.
//   - `add_child(parent, child)` : push `child` (strong) into parent.children,
//     AND set child.parent to a WEAK handle pointing at `parent`.
//       (hint: Rc::downgrade(parent) turns an &Rc into a Weak.)
//   - `parent_value(child) -> Option<i32>` : upgrade the child's weak parent and
//     read its `value`; None if there's no live parent.
use std::rc::Weak;

type Tree = Rc<RefCell<TreeNode>>;

struct TreeNode {
    value: i32,
    parent: Weak<RefCell<TreeNode>>,
    children: Vec<Tree>,
    dropped: DropLog,
}

impl Drop for TreeNode {
    fn drop(&mut self) {
        self.dropped.borrow_mut().push(self.value.to_string());
    }
}

fn tree_node(value: i32, log: &DropLog) -> Tree {
    Rc::new(RefCell::new(TreeNode {
        value,
        parent: Weak::new(),
        children: Vec::new(),
        dropped: Rc::clone(log),
    }))
}

fn add_child(parent: &Tree, child: &Tree) {
    parent.borrow_mut().children.push(Rc::clone(child));
    child.borrow_mut().parent = Rc::downgrade(parent);
}

fn parent_value(child: &Tree) -> Option<i32> {
    child
        .borrow()
        .parent
        .upgrade()
        .map(|parent| parent.borrow().value)
}

fn check_7() {
    let log: DropLog = Rc::new(RefCell::new(Vec::new()));

    {
        let root = tree_node(1, &log);
        let leaf = tree_node(2, &log);
        add_child(&root, &leaf);

        // Down: root owns leaf (strong). Up: leaf sees root (weak, doesn't own).
        assert_eq!(root.borrow().children.len(), 1);
        assert_eq!(parent_value(&leaf), Some(1));

        // The crucial counts: root has ONE strong owner (the local `root`);
        // leaf's weak->root link did NOT bump it. leaf has TWO strong owners
        // (local `leaf` + root.children), and root holds a weak referrer.
        assert_eq!(Rc::strong_count(&root), 1);
        assert_eq!(Rc::weak_count(&root), 1); // the leaf's parent pointer
        assert_eq!(Rc::strong_count(&leaf), 2);

        // mutate the parent THROUGH the child's back-pointer — shared mutability
        // across the tree, the whole reason RefCell is in here.
        if let Some(p) = leaf.borrow().parent.upgrade() {
            p.borrow_mut().value = 99;
        }
        assert_eq!(root.borrow().value, 99);
        // scope ends: locals drop, and with no strong cycle, both nodes free.
    }

    // Unlike rung 6, BOTH nodes were dropped (no strong cycle to pin them).
    let dropped = log.borrow();
    assert_eq!(dropped.len(), 2, "both nodes must free; got {:?}", dropped);
    assert!(dropped.contains(&"99".to_string()) && dropped.contains(&"2".to_string()));
    println!("rung 7 ok: Weak parent / Rc children — tree frees cleanly 🌳");
}

// ───────── Rung 8: real-world — observer / subject (shared mutation fan-out) ─
//
// The other canonical use: one event source ("subject") that pushes updates into
// many independent observers, each holding its own mutable state. The subject
// owns a list of handles to observers; calling `publish` mutates ALL of them
// through their shared cells. This is how event buses, reactive signals, and
// GUI data-binding are wired in single-threaded Rust.
//
// An observer just tallies how many events it has seen and remembers the last:
//   struct Observer { id: u32, seen: u32, last: i32 }
//
// `Subject` keeps `observers: Vec<Rc<RefCell<Observer>>>`. Implement:
//   - `Subject::new()` -> empty.
//   - `subscribe(&mut self, obs)` : store a SHARED handle to `obs` (clone in).
//     Return nothing.
//   - `publish(&self, value)` : for every observer, borrow_mut and do
//     `seen += 1; last = value;`.  Reads through the SAME cells elsewhere must
//     see the update (that's the test).
//
// Footgun to respect (you've met it twice now): publish borrows each observer
// mutably; make sure no other borrow of the same observer is live during the
// loop. Keep each borrow_mut scoped to one iteration.
struct Observer {
    id: u32,
    seen: u32,
    last: i32,
}

type Obs = Rc<RefCell<Observer>>;

struct Subject {
    observers: Vec<Obs>,
}

impl Observer {
    fn new(id: u32) -> Obs {
        Rc::new(RefCell::new(Observer {
            id,
            seen: 0,
            last: 0,
        }))
    }
}

impl Subject {
    fn new() -> Self {
        Self {
            observers: Vec::new(),
        }
    }

    fn subscribe(&mut self, obs: &Obs) {
        self.observers.push(Rc::clone(obs));
    }

    fn publish(&self, value: i32) {
        for observer in &self.observers {
            let mut observer = observer.borrow_mut();
            observer.seen += 1;
            observer.last = value;
        }
    }
}

fn check_8() {
    let a = Observer::new(1);
    let b = Observer::new(2);

    let mut subject = Subject::new();
    subject.subscribe(&a);
    subject.subscribe(&b);

    // We still hold `a` and `b` outside the subject — both are shared, so the
    // subject's publish mutates the very same cells we can read here.
    subject.publish(10);
    subject.publish(20);

    assert_eq!(a.borrow().seen, 2);
    assert_eq!(a.borrow().last, 20);
    assert_eq!(b.borrow().seen, 2);
    assert_eq!(b.borrow().last, 20);

    // observers are independently owned: each has 2 strong owners
    // (our local handle + the subject's Vec).
    assert_eq!(Rc::strong_count(&a), 2);

    // A third observer can join late and only sees events after it subscribed.
    let c = Observer::new(3);
    subject.subscribe(&c);
    subject.publish(30);
    assert_eq!(c.borrow().seen, 1); // missed the first two
    assert_eq!(a.borrow().seen, 3); // a saw all three
    assert_eq!(a.borrow().id, 1); // (id untouched, sanity)
    println!("rung 8 ok: subject fans one event out to many shared observers 📡");
}

// ─────────── Rung 9: CAPSTONE — a doubly-linked list from scratch ────────────
//
// The structure that forces EVERYTHING from this ladder together. A doubly-
// linked list can't be built with plain ownership: a node is pointed at from
// BOTH directions (its predecessor's `next` and its successor's `prev`), so it
// needs shared ownership — and you need to mutate those links after the nodes
// exist, so it needs interior mutability. Hence `Rc<RefCell<Node>>`.
//
// The leak-avoidance rule you learned in rung 7 maps perfectly:
//   - `next` : STRONG (Rc).   The list owns its nodes going forward.
//   - `prev` : WEAK (Weak).   Backward links must NOT pin nodes, or every
//     adjacent pair forms a rung-6 cycle and the whole list leaks.
//
//   struct Node { value: i32, next: Option<Link>, prev: Weak<RefCell<Node>>, dropped }
//   struct List { head: Option<Link>, tail: Option<Link> }   // tail = strong here
//
// Implement on `List`:
//   - `new()` -> empty.
//   - `push_back(&mut self, value)` : append a node. Wire new.prev = downgrade(old
//     tail); old_tail.next = Some(new); update head/tail. Handle the empty case.
//   - `push_front(&mut self, value)`: prepend. Wire new.next = old head; old_head.
//     prev = downgrade(new); update head/tail. Handle the empty case.
//   - `to_vec(&self) -> Vec<i32>` : walk `head -> next -> …`, collecting values.
//   - `to_vec_rev(&self) -> Vec<i32>`: walk `tail -> prev.upgrade() -> …` BACKWARD.
//     This is the proof your prev-links are correct.
//
// Mind the borrow discipline from rungs 4–5: never hold a borrow of one node
// while you borrow_mut the same node again. Clone the Rc handles you need out of
// a borrow, end the borrow, THEN wire the other side. (`as_ref()`, `.clone()`,
// and small scoped blocks are your friends.)
// DropLog for the list is over i32 here for convenience.
type IntDropLog = Rc<RefCell<Vec<i32>>>;
type DLink = Rc<RefCell<DNode>>;

struct DNode {
    value: i32,
    next: Option<DLink>,
    prev: Weak<RefCell<DNode>>,
    dropped: IntDropLog,
}

impl DNode {
    fn new(value: i32, dropped: &IntDropLog) -> Self {
        Self {
            value,
            next: None,
            prev: Weak::new(),
            dropped: Rc::clone(dropped),
        }
    }
}

struct List {
    head: Option<DLink>,
    tail: Option<DLink>,
    dropped: IntDropLog,
}

impl Drop for DNode {
    fn drop(&mut self) {
        self.dropped.borrow_mut().push(self.value);
    }
}

impl List {
    fn new(dropped: &IntDropLog) -> Self {
        Self {
            head: None,
            tail: None,
            dropped: Rc::clone(dropped),
        }
    }

    fn push_back(&mut self, value: i32) {
        let new_node = Rc::new(RefCell::new(DNode::new(value, &self.dropped)));

        match self.tail.take() {
            None => {
                self.head = Some(Rc::clone(&new_node));
                self.tail = Some(new_node);
            }
            Some(old_tail) => {
                new_node.borrow_mut().prev = Rc::downgrade(&old_tail);
                old_tail.borrow_mut().next = Some(Rc::clone(&new_node));
                self.tail = Some(new_node);
            }
        }
    }

    fn push_front(&mut self, value: i32) {
        let new_node = Rc::new(RefCell::new(DNode::new(value, &self.dropped)));

        match self.head.take() {
            None => {
                self.head = Some(Rc::clone(&new_node));
                self.tail = Some(new_node);
            }
            Some(old_head) => {
                old_head.borrow_mut().prev = Rc::downgrade(&new_node);
                new_node.borrow_mut().next = Some(old_head);
                self.head = Some(new_node);
            }
        }
    }

    fn to_vec(&self) -> Vec<i32> {
        let mut values = Vec::new();
        let mut current = self.head.clone();

        while let Some(node) = current {
            let node_ref = node.borrow();
            values.push(node_ref.value);
            current = node_ref.next.clone();
        }

        values
    }

    fn to_vec_rev(&self) -> Vec<i32> {
        let mut values = Vec::new();
        let mut current = self.tail.clone();

        while let Some(node) = current {
            let node_ref = node.borrow();
            values.push(node_ref.value);
            current = node_ref.prev.upgrade();
        }

        values
    }
}

fn check_9() {
    let log: IntDropLog = Rc::new(RefCell::new(Vec::new()));

    {
        let mut list = List::new(&log);
        list.push_back(2);
        list.push_back(3);
        list.push_front(1); // list is now 1 <-> 2 <-> 3
        list.push_back(4); //  1 <-> 2 <-> 3 <-> 4

        // forward and backward traversal must agree (reversed)
        assert_eq!(list.to_vec(), vec![1, 2, 3, 4]);
        assert_eq!(list.to_vec_rev(), vec![4, 3, 2, 1]);

        // prev links are real Weak: a node's strong_count counts only its single
        // predecessor's `next` (+ head/tail handle for the ends), never `prev`.
        // The middle node "2" is owned by node-1's `next` only -> strong_count 1.
        let n2 = list.head.as_ref().unwrap().borrow().next.clone().unwrap();
        assert_eq!(n2.borrow().value, 2);
        assert_eq!(Rc::strong_count(&n2), 2);

        // mutate through a node handle, observe via traversal — interior mut.
        n2.borrow_mut().value = 20;
        assert_eq!(list.to_vec(), vec![1, 20, 3, 4]);
        // list dropped here
    }

    // No prev-cycle, so the whole chain frees. head owns node1 owns node2 ...,
    // so drops cascade front-to-back as each `next` Rc hits zero.
    let dropped = log.borrow().clone();
    assert_eq!(dropped.len(), 4, "all nodes must free; got {:?}", dropped);
    assert_eq!(dropped, vec![1, 20, 3, 4], "front-to-back drop order");
    println!("rung 9 ok: hand-rolled doubly-linked list — Rc next / Weak prev 🎉");
}

// ───────── Rung 10 (bonus): iterative Drop — don't blow the stack ────────────
//
// The wart of any Rc-chained structure. In rung 9 each node OWNS the next via a
// strong `Rc`, so the DEFAULT (compiler-generated) drop is recursive: dropping
// the head drops its `next` Rc, whose refcount hits 0, which runs that node's
// destructor, which drops ITS `next`... one stack frame per node. A few hundred
// thousand nodes and you get a stack overflow — in the destructor, where it's
// horrible to debug. (Same lesson as the hand-rolled list in `box_heap`.)
//
// `DropList` is a minimal singly-linked chain (head + strong `next`) with NO
// custom Drop yet — so right now it relies on the recursive default. Your job:
// write `impl Drop for DropList` that tears the list down ITERATIVELY, so no
// matter how long the chain is, drop runs in O(1) stack space.
//
// The trick: walk the chain and `.take()` each node's `next` BEFORE that node is
// dropped. If a node's `next` is already `None` when it drops, its destructor
// has nothing to recurse into — so each node is freed flat, one loop turn each.
//
//   pattern:
//     let mut cur = self.head.take();          // own the first node, unlink head
//     while let Some(node) = cur {
//         cur = <take node's next out of the cell>;   // hand the chain forward
//         // `node` drops here with its `next` already None -> no recursion
//     }
//
// (Remember: `next` lives inside the RefCell, so taking it needs a borrow_mut.
//  Scope that borrow so it ends before `node` drops at the end of the loop body.)
struct DropList {
    head: Option<DLink>,
    #[allow(dead_code)]
    dropped: IntDropLog,
}

impl DropList {
    fn of_len(n: usize, dropped: &IntDropLog) -> Self {
        // Build a chain of `n` nodes by prepending — O(1) per node, no recursion.
        let mut head: Option<DLink> = None;
        for value in (0..n as i32).rev() {
            let node = Rc::new(RefCell::new(DNode::new(value, dropped)));
            node.borrow_mut().next = head.take();
            head = Some(node);
        }
        DropList {
            head,
            dropped: Rc::clone(dropped),
        }
    }
}

impl Drop for DropList {
    fn drop(&mut self) {
        let mut cur = self.head.take();
        while let Some(node) = cur {
            cur = node.borrow_mut().next.take();
        }
    }
}

fn check_10() {
    let log: IntDropLog = Rc::new(RefCell::new(Vec::new()));

    // Long enough that the *recursive* default drop would overflow the stack.
    // With a correct iterative Drop, this tears down flat and completes.
    let n = 300_000;
    {
        let list = DropList::of_len(n, &log);
        // sanity: the chain really is n nodes long (cheap forward walk, no recursion)
        let mut count = 0usize;
        let mut cur = list.head.clone();
        while let Some(node) = cur {
            count += 1;
            cur = node.borrow().next.clone();
        }
        assert_eq!(count, n);
        // `list` dropped here — your iterative Drop must run without overflowing.
    }

    // every node's destructor ran exactly once
    assert_eq!(log.borrow().len(), n, "all {n} nodes must be dropped");
    println!("rung 10 ok: iterative Drop tears down {n} nodes, no stack overflow 🧹");
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
    check_10();
}
