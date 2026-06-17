// Rc / Arc — shared ownership via reference counting
// Run: cargo run --bin rc_arc
//
// Ladder (foundations -> mastery):
//   1. [DONE] basics: two owners read one heap String via Rc
//   2. [DONE] strong_count: watch the count rise & fall across scopes
//   3. [DONE] shared diamond DAG: one node owned by two parents
//   4. [DONE] Rc<str> / Rc<[T]> cheap clones + Rc::ptr_eq identity
//   5. [DONE] Rc::make_mut: clone-on-write on shared data
//   6. [DONE] the reference cycle leak: Drop never runs
//   7. [DONE] break the cycle with Weak (parent <-> child tree)
//   8. [DONE] Rc is !Send -> Arc across threads; Arc<Mutex<T>> counter
//   9. [TODO] capstone: implement MyRc<T> from scratch
//
// main() replays every solved rung in order and stops at the first todo!().

use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr::NonNull;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};
use std::thread;

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
    println!("\nAll unlocked rungs passed ✅");
}

// ───────────────────────────────────────────────────────────────────────────
// Rung 1 (foundations): two owners, one heap allocation.
//
// Create an Rc<String> holding `text`, then make a SECOND owner of the SAME
// allocation (do NOT build a new String). Return both. The caller will check
// that both observe the same text AND that they point at the same heap data.
// ───────────────────────────────────────────────────────────────────────────
fn two_owners(text: &str) -> (Rc<String>, Rc<String>) {
    let rc = Rc::new(text.to_string());
    (rc.clone(), rc.clone())
}

fn check_1() {
    let (a, b) = two_owners("shared");
    assert_eq!(*a, "shared");
    assert_eq!(*b, "shared");
    // Same heap allocation, not two copies:
    assert!(
        Rc::ptr_eq(&a, &b),
        "a and b must point at the SAME allocation"
    );
    println!(
        "check_1 ✅  two owners share one String at {:p}",
        Rc::as_ptr(&a)
    );
}

// ───────────────────────────────────────────────────────────────────────────
// Rung 2 (foundations): the count is the whole machine — watch it move.
//
// Given an existing Rc, record `Rc::strong_count` at four moments and return
// them as [a, b, c, d]:
//   a = the count right now (before you touch anything)
//   b = the count after you make ONE more clone in an inner scope
//   c = the count while a SECOND clone is also alive (still in that scope)
//   d = the count AFTER that inner scope ends and those clones are dropped
//
// The lesson: clone() increments, drop (end of scope) decrements, and you can
// observe it. Use a block `{ ... }` to create/destroy the temporary clones.
// ───────────────────────────────────────────────────────────────────────────
fn count_lifecycle(rc: &Rc<String>) -> [usize; 4] {
    let a = Rc::strong_count(rc);

    let (b, c) = {
        let _rc2 = Rc::clone(rc);
        let b = Rc::strong_count(rc);
        let _rc3 = Rc::clone(rc);
        let c = Rc::strong_count(rc);
        (b, c)
    };

    let d = { Rc::strong_count(rc) };

    [a, b, c, d]
}

fn check_2() {
    let original = Rc::new(String::from("counted"));
    let counts = count_lifecycle(&original);
    assert_eq!(
        counts,
        [1, 2, 3, 1],
        "expected [before, +1, +2, after] = [1,2,3,1], got {counts:?}"
    );
    // The temporaries are gone; only `original` remains.
    assert_eq!(Rc::strong_count(&original), 1);
    println!(
        "check_2 ✅  count moved {:?} as clones came and went",
        counts
    );
}

// ───────────────────────────────────────────────────────────────────────────
// Rung 3 (mechanics): a shared diamond — why Rc exists at all.
//
//        top
//       /    \
//   left      right
//       \    /
//       shared      <- ONE node, owned by BOTH left and right
//
// With Box this is impossible (Box = single owner). With Rc, `left` and
// `right` each hold an Rc to the SAME `shared` node.
//
// Build the diamond and return the `top`. Each Node owns its children as
// Rc<Node>. Don't clone the *data* of `shared` — both branches must point at
// the same allocation (check_3 verifies with ptr_eq and strong_count).
// ───────────────────────────────────────────────────────────────────────────
struct Node {
    name: String,
    children: Vec<Rc<Node>>,
}

fn build_diamond() -> Rc<Node> {
    let shared = Rc::new(Node {
        name: "shared".to_string(),
        children: Vec::new(),
    });
    let left = Rc::new(Node {
        name: "left".to_string(),
        children: vec![Rc::clone(&shared)],
    });
    let right = Rc::new(Node {
        name: "right".to_string(),
        children: vec![Rc::clone(&shared)],
    });

    Rc::new(Node {
        name: "top".to_string(),
        children: vec![Rc::clone(&left), Rc::clone(&right)],
    })
}

fn check_3() {
    let top = build_diamond();
    assert_eq!(top.children.len(), 2, "top should have left and right");
    let left = &top.children[0];
    let right = &top.children[1];

    // Each branch points at the shared node...
    let shared_via_left = &left.children[0];
    let shared_via_right = &right.children[0];
    assert!(
        Rc::ptr_eq(shared_via_left, shared_via_right),
        "left and right must point at the SAME shared node, not two copies"
    );
    assert_eq!(shared_via_left.name, "shared");
    assert_eq!(
        Rc::strong_count(shared_via_left),
        2,
        "shared node should be owned by both left and right"
    );
    println!(
        "check_3 ✅  diamond built; shared node has {} owners",
        Rc::strong_count(shared_via_left)
    );
}

// ───────────────────────────────────────────────────────────────────────────
// Rung 4 (mechanics): Rc<str> — sharing an immutable string the cheap way.
//
// Imagine many records tagged with the same category string ("electronics").
// Storing String in each = one heap allocation per record. Instead, intern it
// ONCE as an Rc<str> and hand every record a cheap clone (just a refcount bump
// + copying a fat pointer; the bytes are never re-copied).
//
// Implement `tag_all`: given a category `&str` and how many records `n`, return
// a Vec<Rc<str>> of length n where EVERY element is a clone of the SAME shared
// Rc<str>. (Hint: `Rc<str>` is built with `Rc::from(some_str)` or `.into()`.)
//
// check_4 verifies all n elements share one allocation via Rc::ptr_eq and that
// the strong_count reflects all of them.
// ───────────────────────────────────────────────────────────────────────────
fn tag_all(category: &str, n: usize) -> Vec<Rc<str>> {
    let rc: Rc<str> = Rc::from(category);
    let mut tags = Vec::with_capacity(n);
    for _ in 0..n {
        tags.push(Rc::clone(&rc));
    }
    tags
}

fn check_4() {
    let tags = tag_all("electronics", 4);
    assert_eq!(tags.len(), 4);
    assert!(tags.iter().all(|t| &**t == "electronics"));

    // All four are the SAME allocation, not four separate strings:
    for t in &tags[1..] {
        assert!(
            Rc::ptr_eq(&tags[0], t),
            "every tag must clone the SAME Rc<str>, not allocate a new one"
        );
    }
    // 4 in the Vec; strong_count counts them all.
    assert_eq!(Rc::strong_count(&tags[0]), 4);

    // Bonus proof it's a real str slice, not a String:
    assert_eq!(tags[0].len(), "electronics".len());
    println!(
        "check_4 ✅  {} records share ONE interned Rc<str> at {:p}",
        tags.len(),
        Rc::as_ptr(&tags[0])
    );
}

// ───────────────────────────────────────────────────────────────────────────
// Rung 5 (mechanics): Rc::make_mut — clone-on-write, the Cow you already know.
//
// Rc<T> only hands out shared (&) access — you can't get &mut T while others
// might be looking. `Rc::make_mut(&mut rc)` solves this: it returns a &mut T,
// but FIRST it checks the strong count:
//   - count == 1 (you're the sole owner): hands you &mut to the SAME allocation
//     (mutate in place, no copy).
//   - count  > 1 (shared): clones the inner T into a fresh allocation, points
//     THIS Rc at the clone, and hands you &mut to that — so the OTHER owners
//     never see your mutation. This is the "write" half of clone-on-write.
//
// Implement `push_isolated`: given `&mut Rc<Vec<i32>>` and a value, push the
// value using make_mut. Return nothing. (Vec<i32> is the inner T.)
// ───────────────────────────────────────────────────────────────────────────
fn push_isolated(rc: &mut Rc<Vec<i32>>, value: i32) {
    Rc::make_mut(rc).push(value);
}

fn check_5() {
    // Case A — sole owner: make_mut mutates IN PLACE (same allocation).
    let mut solo = Rc::new(vec![1, 2, 3]);
    let addr_before = Rc::as_ptr(&solo);
    push_isolated(&mut solo, 4);
    assert_eq!(*solo, vec![1, 2, 3, 4]);
    assert_eq!(
        Rc::as_ptr(&solo),
        addr_before,
        "sole owner should mutate in place, not reallocate"
    );

    // Case B — shared: make_mut must COPY so the other owner is untouched.
    let original = Rc::new(vec![1, 2, 3]);
    let mut writer = Rc::clone(&original); // now count == 2
    assert_eq!(Rc::strong_count(&original), 2);

    push_isolated(&mut writer, 99);
    assert_eq!(*writer, vec![1, 2, 3, 99], "writer sees its own push");
    assert_eq!(*original, vec![1, 2, 3], "original must be UNCHANGED");
    assert!(
        !Rc::ptr_eq(&original, &writer),
        "writer should now point at a fresh clone, not the shared allocation"
    );
    // The copy split them: each is now a sole owner again.
    assert_eq!(Rc::strong_count(&original), 1);
    assert_eq!(Rc::strong_count(&writer), 1);

    println!("check_5 ✅  make_mut: in-place when solo, copy-on-write when shared");
}

// ───────────────────────────────────────────────────────────────────────────
// Rung 6 (footgun): the reference cycle that LEAKS. The defining Rc failure.
//
// Rc frees its inner value when strong_count hits 0. But if A holds an Rc to B
// and B holds an Rc to A, then:
//   - while both are alive, each keeps the other's count at >= 1
//   - when you drop your handles, A's count is still 1 (B points at it) and
//     B's count is still 1 (A points at it) — neither ever reaches 0.
//   - => Drop NEVER runs. The memory is leaked for the rest of the program.
//
// We need interior mutability to wire the back-edge after both nodes exist, so
// each Cycle has `link: RefCell<Option<Rc<Cycle>>>`. The DROP_COUNT static lets
// us PROVE whether Drop ran.
//
// Your job: in `make_leaky_cycle`, create two Rc<Cycle> nodes `a` and `b`, then
// point a.link -> b and b.link -> a (a 2-node cycle). Return nothing — let `a`
// and `b` drop at the end of the function. Because of the cycle, their Drop
// will NOT fire, and check_6 asserts exactly that (DROP_COUNT stays 0).
// ───────────────────────────────────────────────────────────────────────────
thread_local! {
    static DROP_COUNT: Cell<usize> = const { Cell::new(0) };
}

struct Cycle {
    name: &'static str,
    link: RefCell<Option<Rc<Cycle>>>,
}

impl Cycle {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            link: RefCell::new(None),
        }
    }
}

impl Drop for Cycle {
    fn drop(&mut self) {
        DROP_COUNT.with(|c| c.set(c.get() + 1));
        println!("    Cycle '{}' dropped", self.name);
    }
}

fn make_leaky_cycle() {
    let a = Rc::new(Cycle::new("a"));
    let b = Rc::new(Cycle::new("b"));
    a.link.borrow_mut().replace(Rc::clone(&b));
    b.link.borrow_mut().replace(Rc::clone(&a));
}

fn check_6() {
    DROP_COUNT.with(|c| c.set(0));
    make_leaky_cycle();
    // a and b went out of scope at the end of make_leaky_cycle. If there were
    // NO cycle, both would have dropped here (count -> 0). The cycle pins them.
    let drops = DROP_COUNT.with(|c| c.get());
    assert_eq!(
        drops, 0,
        "expected the cycle to LEAK (0 drops), but {drops} node(s) dropped — \
         did you actually form a 2-node cycle with strong Rcs?"
    );
    println!("check_6 ✅  cycle leaked as expected: {drops} of 2 nodes ran Drop (memory leaked)");
}

// ───────────────────────────────────────────────────────────────────────────
// Rung 7 (footgun→fix): Weak breaks the cycle. The parent/child tree pattern.
//
// Weak<T> is an Rc that does NOT own: it holds a pointer + bumps the WEAK count,
// never the STRONG count. So a Weak can't keep a value alive, and a chain of
// Weak edges can't form a keep-alive cycle. To use one you must `upgrade()` it:
// that returns Option<Rc<T>> — Some(rc) if the target is still alive (and bumps
// the strong count for as long as you hold it), None if it's already gone.
//
// The canonical tree:
//   parent --( strong Rc )--> child      (parent owns child)
//   child  --(  Weak     )--> parent     (child refers back, but does NOT own)
//
// Now dropping the parent's handle works: nothing strong points UP at it, so
// its strong count hits 0 and it (and then its children) drop. No leak.
//
// Implement:
//   * `link_parent_child(parent, child)`:
//       - push `child` (a strong clone) into parent.children
//       - set child.parent to a Weak handle pointing at `parent`  (Rc::downgrade)
//   * `parent_name(child)`: upgrade the child's Weak parent; return the parent's
//       name if still alive, or "<no parent>" if the Weak is dangling.
// ───────────────────────────────────────────────────────────────────────────
struct TreeNode {
    name: &'static str,
    parent: RefCell<Weak<TreeNode>>,
    children: RefCell<Vec<Rc<TreeNode>>>,
}

impl TreeNode {
    fn new(name: &'static str) -> Rc<TreeNode> {
        Rc::new(TreeNode {
            name,
            parent: RefCell::new(Weak::new()), // starts pointing at nothing
            children: RefCell::new(Vec::new()),
        })
    }
}

impl Drop for TreeNode {
    fn drop(&mut self) {
        DROP_COUNT.with(|c| c.set(c.get() + 1));
        println!("    TreeNode '{}' dropped", self.name);
    }
}

fn link_parent_child(parent: &Rc<TreeNode>, child: &Rc<TreeNode>) {
    parent.children.borrow_mut().push(Rc::clone(child));
    let weak = Rc::downgrade(parent);
    *child.parent.borrow_mut() = weak;
}

fn parent_name(child: &Rc<TreeNode>) -> &'static str {
    child
        .parent
        .borrow()
        .upgrade()
        .map(|p| p.name)
        .unwrap_or("<no parent>")
}

fn check_7() {
    DROP_COUNT.with(|c| c.set(0));

    {
        let root = TreeNode::new("root");
        let leaf = TreeNode::new("leaf");
        link_parent_child(&root, &leaf);

        // Child can find its parent while the parent is alive.
        assert_eq!(parent_name(&leaf), "root");

        // The Weak back-edge does NOT inflate the parent's STRONG count:
        // only `root` (this binding) strongly owns the root node.
        assert_eq!(
            Rc::strong_count(&root),
            1,
            "Weak parent edge must NOT add to the parent's strong count"
        );
        // The child has 2 strong owners: `leaf` here + root.children.
        assert_eq!(Rc::strong_count(&leaf), 2);

        // Drop the parent handle but KEEP the leaf. Now the leaf's Weak parent
        // dangles — upgrade() returns None.
        drop(root);
        assert_eq!(
            parent_name(&leaf),
            "<no parent>",
            "after the parent drops, the child's Weak should fail to upgrade"
        );
    }

    // Both nodes dropped (no leak) — unlike rung 6.
    let drops = DROP_COUNT.with(|c| c.get());
    assert_eq!(
        drops, 2,
        "expected BOTH nodes to drop (no leak), but only {drops} did"
    );
    println!("check_7 ✅  Weak broke the cycle: {drops} of 2 nodes ran Drop (no leak)");
}

// ───────────────────────────────────────────────────────────────────────────
// Rung 8 (real-world): Rc is !Send -> Arc<Mutex<T>> across threads.
//
// Rc's counter is a plain integer: two threads cloning/dropping it could race
// and corrupt the count (double-free or leak). Rust forbids this at compile
// time by making `Rc: !Send` — you literally cannot move one into a thread.
// Arc uses an ATOMIC counter, so it's Send + Sync and crosses threads safely.
//
// But Arc still only gives SHARED (&) access — to MUTATE shared state across
// threads you wrap the data in a Mutex: Arc<Mutex<T>>. Arc shares the lock
// among threads; the Mutex hands out &mut T to one thread at a time.
//
// (FOOTGUN to see for yourself: at the bottom of this function there's a
//  commented-out Rc version. Uncomment it and run `cargo build` to watch the
//  compiler reject it with "`Rc<...>` cannot be sent between threads safely".)
//
// Implement `concurrent_count(n_threads, per_thread)`: spawn `n_threads`
// threads; each one locks the shared counter and does `+= 1`, `per_thread`
// times. Join them all and return the final total (should be n_threads *
// per_thread — no lost updates).
// ───────────────────────────────────────────────────────────────────────────
fn concurrent_count(n_threads: usize, per_thread: usize) -> usize {
    let counter = Arc::new(Mutex::new(0usize));
    let handles = (0..n_threads)
        .map(|_| {
            let counter = Arc::clone(&counter);
            thread::spawn(move || {
                let mut counter = counter.lock().unwrap();
                *counter += per_thread;
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().unwrap();
    }

    *counter.lock().unwrap()
}

fn check_8() {
    let total = concurrent_count(8, 10_000);
    assert_eq!(
        total,
        8 * 10_000,
        "expected 80000 with no lost updates; got {total} — is every += 1 under the lock?"
    );
    println!("check_8 ✅  8 threads × 10000 increments = {total}, no lost updates");
}

// ───────────────────────────────────────────────────────────────────────────
// Rung 9 (CAPSTONE): build MyRc<T> from scratch. Own the mental model.
//
// An Rc is just: ONE heap allocation holding { strong_count, value }, plus a
// pointer to it. Each MyRc is one such pointer and owns ONE unit of the count.
//   - new():    allocate the inner with strong = 1.
//   - clone():  bump strong by 1, return another pointer to the SAME inner.
//   - deref():  hand out &value (shared access only — just like real Rc).
//   - drop():   decrement strong; when it reaches 0, free the inner allocation
//               (which runs T's destructor exactly once).
//
// We store the inner behind a NonNull raw pointer. strong is a Cell<usize> so
// we can mutate the count through a shared &self (interior mutability — Rc is
// !Sync precisely because this counter is non-atomic). PhantomData<MyRcInner<T>>
// tells the compiler we logically OWN a T, so drop-check works correctly.
//
// FOUR holes to fill: MyRc::new, Clone::clone, Deref::deref, Drop::drop.
// Each needs a little `unsafe` to go through the raw pointer. Aim for: clone
// bumps the count, the LAST drop frees once, and no double-free / no leak.
// Verify with `cargo run --bin rc_arc` AND ideally `cargo miri run --bin rc_arc`.
// ───────────────────────────────────────────────────────────────────────────
struct MyRcInner<T> {
    strong: Cell<usize>,
    value: T,
}

struct MyRc<T> {
    ptr: NonNull<MyRcInner<T>>,
    // We conceptually own a MyRcInner<T> (and thus a T) via the raw pointer.
    _marker: PhantomData<MyRcInner<T>>,
}

impl<T> MyRc<T> {
    fn new(value: T) -> MyRc<T> {
        // Heap-allocate a MyRcInner with strong = 1, take a raw NonNull to it,
        // and wrap it. Hint: Box::new(...) then Box::into_raw / NonNull::from.
        let inner = Box::new(MyRcInner {
            strong: Cell::new(1),
            value,
        });
        let ptr = Box::into_raw(inner);
        MyRc {
            ptr: NonNull::new(ptr).unwrap(),
            _marker: PhantomData,
        }
    }

    // Helper: borrow the shared inner. (Already written for you.)
    fn inner(&self) -> &MyRcInner<T> {
        // SAFETY: ptr is always valid while at least one MyRc to it exists,
        // and `self` is one such MyRc.
        unsafe { self.ptr.as_ref() }
    }

    fn strong_count(this: &MyRc<T>) -> usize {
        this.inner().strong.get()
    }
}

impl<T> Clone for MyRc<T> {
    fn clone(&self) -> MyRc<T> {
        self.inner().strong.set(self.inner().strong.get() + 1);
        MyRc {
            ptr: self.ptr,
            _marker: PhantomData,
        }
    }
}

impl<T> Deref for MyRc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // Shared access to the inner value only.
        &self.inner().value
    }
}

impl<T> Drop for MyRc<T> {
    fn drop(&mut self) {
        if self.inner().strong.get() == 1 {
            unsafe {
                drop(Box::from_raw(self.ptr.as_ptr()));
            }
        } else {
            self.inner().strong.set(self.inner().strong.get() - 1);
        }
    }
}

// A value that records when it is dropped, so check_9 can prove the inner T is
// freed exactly once (on the final MyRc drop) — not zero times, not twice.
struct Dropper(&'static str);
impl Drop for Dropper {
    fn drop(&mut self) {
        DROP_COUNT.with(|c| c.set(c.get() + 1));
        println!("    Dropper('{}') dropped", self.0);
    }
}

fn check_9() {
    DROP_COUNT.with(|c| c.set(0));
    {
        let a = MyRc::new(Dropper("payload"));
        assert_eq!(MyRc::strong_count(&a), 1, "fresh MyRc should have count 1");
        assert_eq!(a.0, "payload", "Deref should expose the inner value");

        {
            let b = MyRc::clone(&a);
            assert_eq!(
                MyRc::strong_count(&a),
                2,
                "clone must bump the shared count"
            );
            assert_eq!(MyRc::strong_count(&b), 2, "both handles see the same count");
            assert_eq!(b.0, "payload", "clone derefs to the same value");
            // Same allocation, not a deep copy of Dropper:
            assert_eq!(a.ptr, b.ptr, "clone must point at the SAME inner");
            assert_eq!(
                DROP_COUNT.with(|c| c.get()),
                0,
                "nothing dropped while shared"
            );
        }
        // b dropped: count back to 1, inner still alive (NOT dropped).
        assert_eq!(
            MyRc::strong_count(&a),
            1,
            "dropping a clone decrements the count"
        );
        assert_eq!(
            DROP_COUNT.with(|c| c.get()),
            0,
            "inner must NOT drop while another owner remains"
        );
    }
    // a dropped: count 0 -> inner freed, Dropper runs exactly once.
    assert_eq!(
        DROP_COUNT.with(|c| c.get()),
        1,
        "the LAST drop must free the inner exactly once (no leak, no double-free)"
    );
    println!("check_9 ✅  MyRc works: clone shares, last drop frees exactly once 🎉");
}
