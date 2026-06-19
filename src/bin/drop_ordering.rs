// Drop & ordering — destructors, drop order, drop flags, ManuallyDrop, mem::forget/take/replace
// Run: cargo run --bin drop_ordering
//
// Ladder (✅ = done, 👉 = current):
//   Foundations
//     1. ✅ Drop fires at scope end — impl Drop for Noisy, watch it run
//     2. ✅ Local drop order — locals drop in REVERSE declaration order
//   Mechanics
//     3. ✅ Struct & nested order — fields drop in declaration order; nested inside-out
//     4. ✅ Early drop — std::mem::drop ends a value early; why x.drop() won't compile
//   Footguns & edge cases
//     5. ✅ Drop flags & partial moves — move a field out, prove no double-drop
//     6. ✅ forget / take / replace — leak with mem::forget; pull value out of &mut self
//   Real-world
//     7. ✅ RAII scope guard — runs a closure on drop, with .cancel()
//     8. ✅ ManuallyDrop — manual control; drop fields in a custom order
//   Synthesis
//     9. ✅ Capstone — rollback-on-drop Transaction (Drop + drop flag + forget)

use std::cell::RefCell;

// A handy shared log so checks can assert the EXACT order things dropped in.
thread_local! {
    static LOG: RefCell<Vec<String>> = RefCell::new(Vec::new());
}
fn log(msg: impl Into<String>) {
    LOG.with(|l| l.borrow_mut().push(msg.into()));
}
fn take_log() -> Vec<String> {
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

// ----------------------------------------------------------------------------
// Rung 1 — Drop fires at scope end
//
// Goal: make `Noisy` print/record when it is dropped. Implement the Drop trait
// so that dropping a Noisy with name "x" pushes the string "drop x" to the log.
//
// Then check_1 creates a Noisy inside an inner scope and asserts that by the
// time the scope ends, "drop a" was logged.
// ----------------------------------------------------------------------------

struct Noisy {
    name: String,
}

impl Noisy {
    fn new(name: &str) -> Self {
        Noisy {
            name: name.to_string(),
        }
    }
}

// TODO(rung 1): implement Drop for Noisy.
// It should call: log(format!("drop {}", self.name));
impl Drop for Noisy {
    fn drop(&mut self) {
        log(format!("drop {}", self.name));
    }
}

fn check_1() {
    take_log(); // clear
    {
        let _a = Noisy::new("a");
        // _a is still alive here; nothing dropped yet
        assert!(take_log().is_empty(), "Noisy dropped too early");
    } // <- _a goes out of scope here, Drop runs
    let events = take_log();
    assert_eq!(events, vec!["drop a"], "expected exactly one drop of `a`");
    println!("check_1 ✅  Drop runs automatically at end of scope");
}

// ----------------------------------------------------------------------------
// Rung 2 — Local drop order
//
// Within a single scope, locals drop in REVERSE order of declaration (LIFO,
// like a stack). The last variable you declared is the first to be cleaned up.
//
// Goal: implement `make_order` so that it creates three Noisy locals named
// "first", "second", "third" (declared in that order) and returns the log of
// the order they actually dropped in. Don't reorder anything by hand — just
// declare them in order and let the scope end. The point is to PREDICT the
// result before you run it.
//
// Predict on paper first, then implement and check.
// ----------------------------------------------------------------------------

fn make_order() -> Vec<String> {
    take_log();
    {
        let _first = Noisy::new("first");
        let _second = Noisy::new("second");
        let _third = Noisy::new("third");
    }
    take_log()
}

fn check_2() {
    let order = make_order();
    assert_eq!(
        order,
        vec!["drop third", "drop second", "drop first"],
        "locals drop in REVERSE declaration order (LIFO)"
    );
    println!("check_2 ✅  locals drop in reverse declaration order (LIFO)");
}

// ----------------------------------------------------------------------------
// Rung 3 — Struct & nested order
//
// Here's the twist that catches people: STRUCT FIELDS drop in *declaration*
// order (top to bottom) — the OPPOSITE of locals (which are LIFO). And a struct
// that owns other Drop values drops its own body first, THEN its fields.
//
// So for `Pair { id, a, b }` you get: the Pair's own Drop runs, THEN a drops,
// THEN b. Each field, being a Noisy, logs its own "drop <name>".
//
// Goal:
//   - impl Drop for Pair so it logs format!("drop pair {}", self.id).
//   - leave the fields alone: the compiler drops a then b for you AFTER your
//     drop() body returns. That automatic field-drop IS the lesson.
//
// PREDICT the three lines and their order before running. Ask yourself:
//   1. does the container's own drop() run before or after its fields?
//   2. do fields a and b drop in declaration order or reverse?
// ----------------------------------------------------------------------------

struct Pair {
    id: String,
    #[allow(unused)]
    a: Noisy,
    #[allow(unused)]
    b: Noisy,
}

// TODO(rung 3): impl Drop for Pair. Log: format!("drop pair {}", self.id).
impl Drop for Pair {
    fn drop(&mut self) {
        log(format!("drop pair {}", self.id));
    }
}

fn nested_order() -> Vec<String> {
    take_log();
    {
        let _p = Pair {
            id: "P".to_string(),
            a: Noisy::new("a"),
            b: Noisy::new("b"),
        };
    }
    take_log()
}

fn check_3() {
    let order = nested_order();
    assert_eq!(
        order,
        vec!["drop pair P", "drop a", "drop b"],
        "container drops first, then fields in DECLARATION order (a before b)"
    );
    println!("check_3 ✅  container drops first; fields drop in declaration order");
}

// ----------------------------------------------------------------------------
// Rung 4 — Early drop, and why you can't call .drop() yourself
//
// Sometimes you want a value gone BEFORE its scope ends (release a lock early,
// free a buffer before a long computation). The tool is `std::mem::drop(x)` — a
// function that takes x BY VALUE, so ownership moves in and the value dies at
// the end of that tiny function. After `drop(x)`, x is moved-from: using it is
// a compile error, which is exactly what stops a double free.
//
// You will ALSO prove why `x.drop()` (calling the trait method directly) is
// forbidden. Uncomment the line in `the_forbidden_call` and read the compiler
// error, then re-comment it. (Leaving it uncommented breaks the build for every
// rung, so comment it back out before moving on.)
//
// Goal — implement `early_drop` so the log comes out in THIS exact order:
//   ["drop early", "between", "drop late"]
// i.e.
//   - make a Noisy "early" and a Noisy "late"
//   - force "early" to drop NOW (before "late")
//   - call log("between")
//   - let "late" drop naturally at scope end
// ----------------------------------------------------------------------------

fn early_drop() -> Vec<String> {
    take_log();
    {
        let early = Noisy::new("early");
        let _late = Noisy::new("late");

        drop(early);
        log("between");
    }
    take_log()
}

// Read-only thought experiment. Keep this commented; uncomment ONLY to see the
// error, then comment it again.
#[allow(dead_code)]
fn the_forbidden_call() {
    let _n = Noisy::new("nope");
    // _n.drop(); // <-- uncomment to see E0040: "explicit use of destructor method"
}

fn check_4() {
    let order = early_drop();
    assert_eq!(
        order,
        vec!["drop early", "between", "drop late"],
        "early must drop before the `between` log; late drops at scope end"
    );
    println!("check_4 ✅  std::mem::drop ends a value early (and x.drop() is illegal)");
}

// ----------------------------------------------------------------------------
// Rung 5 — Drop flags: conditional moves, tracked at RUNTIME
//
// Until now drop placement looked purely static (the compiler inserts drops at
// scope end). But what if a value is moved out on SOME paths and not others?
// The compiler can't know at compile time which path ran. So it stashes a hidden
// boolean — a "drop flag" — on the stack next to the value: "does this still
// need dropping?" At scope end it checks the flag and drops only if true. That's
// how Rust guarantees a moved-from value is NOT dropped a second time.
//
// `consume(n)` below takes a Noisy BY VALUE and logs "consumed <name>" — so the
// Noisy dies inside consume (logging "drop <name>" right after).
//
// Goal — implement `conditional_move(take_it)` so that:
//   - take_it == true:  x is moved into consume(x) inside the `if`.
//   - take_it == false: x is left alone and drops at scope end.
// In BOTH cases "drop x" must appear EXACTLY ONCE (never zero, never twice).
//
//   conditional_move(true)  => ["consumed x", "drop x"]
//   conditional_move(false) => ["drop x"]
//
// The magic: when take_it is true, the drop flag is cleared after the move, so
// the scope-end drop is SKIPPED — no double free. You don't write the flag; the
// compiler does. Your job is just to do the conditional move and trust it.
// ----------------------------------------------------------------------------

fn consume(n: Noisy) {
    log(format!("consumed {}", n.name));
    // n drops here at end of consume -> logs "drop <name>"
}

fn conditional_move(take_it: bool) -> Vec<String> {
    take_log();
    {
        let x = Noisy::new("x");
        if take_it {
            consume(x);
        }
    }
    take_log()
}

fn check_5() {
    assert_eq!(
        conditional_move(true),
        vec!["consumed x", "drop x"],
        "when moved, x drops inside consume and NOT again at scope end"
    );
    assert_eq!(
        conditional_move(false),
        vec!["drop x"],
        "when not moved, x drops once at scope end"
    );
    println!("check_5 ✅  drop flags: a conditionally-moved value drops exactly once");
}

// ----------------------------------------------------------------------------
// Rung 6 — forget / take / replace: skipping and relocating destructors
//
// Two `std::mem` tools that bend drop behavior. Both matter constantly in real
// code (and especially inside Drop impls).
//
// (a) mem::forget(x) — moves x in and then DOES NOT drop it. The destructor is
//     skipped; the value leaks. It's safe (leaking memory isn't UB), and it's
//     how you hand ownership to something that will clean up later (FFI, ManuallyDrop).
//
// (b) mem::replace(&mut dst, new) — moves `new` into the place behind a mutable
//     borrow and RETURNS the old value to you. mem::take is the same but leaves
//     Default::default() behind. This is the ONLY way to move a non-Copy value
//     out of `&mut self`: you can't write `let v = self.field;` (that would move
//     out of a borrow — E0507). You must swap something in to take something out.
//
// Goal:
//   - `forget_it`: create Noisy "leaked", then forget it. Returns the log, which
//     must be EMPTY (drop never ran).
//   - `Slot::swap_in`: use mem::replace to install `replacement` into self.inner
//     and return the PREVIOUS Noisy to the caller — WITHOUT dropping it.
//
// Predicted combined log from replace_demo: ["swapped", "drop old", "drop new"].
// ----------------------------------------------------------------------------

fn forget_it() -> Vec<String> {
    take_log();
    {
        let x = Noisy::new("leaked");
        // TODO(rung 6a): forget x so its Drop is SKIPPED (the value leaks).
        let _ = std::mem::forget(x);
    }
    take_log() // must be empty: "drop leaked" should NEVER appear
}

struct Slot {
    inner: Noisy,
}

impl Slot {
    // Put `replacement` into self.inner and return the value that was there.
    // You cannot move self.inner out directly (it's behind &mut self).
    fn swap_in(&mut self, replacement: Noisy) -> Noisy {
        std::mem::replace(&mut self.inner, replacement)
    }
}

fn replace_demo() -> Vec<String> {
    take_log();
    let mut slot = Slot {
        inner: Noisy::new("old"),
    };
    {
        let returned = slot.swap_in(Noisy::new("new"));
        // "old" now lives in `returned`; "new" lives in slot.inner. Nothing dropped yet.
        log("swapped");
        drop(returned); // drops "old" here
    }
    drop(slot); // Slot has no Drop impl, so this drops its field -> "drop new"
    take_log()
}

fn check_6() {
    assert_eq!(
        forget_it(),
        Vec::<String>::new(),
        "mem::forget skips Drop entirely — 'drop leaked' must never appear"
    );
    assert_eq!(
        replace_demo(),
        vec!["swapped", "drop old", "drop new"],
        "replace moves old out (no drop) until we explicitly drop it; new stays in slot"
    );
    println!("check_6 ✅  mem::forget skips Drop; mem::replace relocates a value out of &mut self");
}

// ----------------------------------------------------------------------------
// Rung 7 — RAII scope guard (the reason Drop exists)
//
// The killer app for Drop: tie a cleanup ACTION to a scope, so it runs no matter
// how you leave — normal return, early return, or panic. This is exactly what
// MutexGuard, File, and the `scopeguard` crate do. You'll build a tiny version.
//
// A Guard owns a closure and runs it on drop. But there's a real puzzle: drop()
// only gets &mut self, and calling an FnOnce closure CONSUMES it (you can only
// call it once, by value). You cannot move `self.action` out of &mut self... so
// you reach for the rung-6 trick: store it as Option<F> and `.take()` it (which
// is mem::replace with None) to get an owned F you can call.
//
// Also add `.cancel()`: consume the guard WITHOUT running the action (e.g. the
// operation succeeded, so skip the rollback).
//
// Goal:
//   - impl Drop for Guard: if the action is still present, take it and call it.
//   - impl Guard::cancel(self): disarm the guard so drop() runs nothing.
//
// Expected:
//   guard_runs()      => ["work", "cleanup"]   (action fires at scope end)
//   guard_cancelled() => ["work"]              (cancel disarmed it)
// ----------------------------------------------------------------------------

struct Guard<F: FnOnce()> {
    action: Option<F>,
}

impl<F: FnOnce()> Guard<F> {
    fn new(f: F) -> Self {
        Guard { action: Some(f) }
    }

    // Disarm: after this, dropping the guard must NOT run the action.
    fn cancel(mut self) {
        // TODO(rung 7b): make this guard not fire on drop. Think about what
        // `drop` checks below, and how to make that check find "nothing to run".
        self.action = None;
    }
}

impl<F: FnOnce()> Drop for Guard<F> {
    fn drop(&mut self) {
        if let Some(action) = self.action.take() {
            action();
        }
    }
}

fn guard_runs() -> Vec<String> {
    take_log();
    {
        let _g = Guard::new(|| log("cleanup"));
        log("work");
    } // _g drops here -> action runs
    take_log()
}

fn guard_cancelled() -> Vec<String> {
    take_log();
    {
        let g = Guard::new(|| log("cleanup"));
        log("work");
        g.cancel(); // disarm: cleanup must NOT run
    }
    take_log()
}

fn check_7() {
    assert_eq!(
        guard_runs(),
        vec!["work", "cleanup"],
        "guard must run its action on drop"
    );
    assert_eq!(
        guard_cancelled(),
        vec!["work"],
        "cancelled guard must NOT run its action"
    );
    println!("check_7 ✅  RAII scope guard runs cleanup on drop; cancel() disarms it");
}

// ----------------------------------------------------------------------------
// Rung 8 — ManuallyDrop: take the wheel from the compiler
//
// `ManuallyDrop<T>` is a wrapper that SUPPRESSES T's automatic drop. The
// compiler will not drop what's inside — YOU must, by calling the (unsafe)
// `ManuallyDrop::drop(&mut md)` exactly once, or it leaks (like forget).
//
// Why it exists: it's the only way to override the fixed field-drop order, and
// it's how containers like Vec manage element drops by hand. Here you'll flip
// the default order: fields `a, b` would normally drop a-then-b (rung 3), but
// you'll make them drop b-then-a.
//
// Two things to build:
//   (a) suppressed(): wrap a Noisy "ghost" in ManuallyDrop, never drop it ->
//       log stays EMPTY (the wrapper ate the destructor).
//   (b) impl Drop for Custom: manually drop b FIRST, then a.
//       SAFETY you must uphold: drop each field exactly once, never use them
//       afterwards. (This is genuine `unsafe` — a double ManuallyDrop::drop is UB.)
//
// Expected:
//   suppressed()   => []
//   custom_order() => ["drop b", "drop a"]
// ----------------------------------------------------------------------------

use std::mem::ManuallyDrop;

fn suppressed() -> Vec<String> {
    take_log();
    {
        let _ghost = ManuallyDrop::new(Noisy::new("ghost"));
        // TODO(rung 8a): ...actually, do NOTHING here. The point is that a
        // ManuallyDrop that is never manually dropped leaks. Just confirm the
        // log is empty. (Leave this block as-is; no code needed.)
    }
    take_log()
}

struct Custom {
    a: ManuallyDrop<Noisy>,
    b: ManuallyDrop<Noisy>,
}

impl Drop for Custom {
    fn drop(&mut self) {
        // SAFETY: we are dropping the fields exactly once.
        unsafe {
            ManuallyDrop::drop(&mut self.b);
            ManuallyDrop::drop(&mut self.a);
        }
    }
}

fn custom_order() -> Vec<String> {
    take_log();
    {
        let _c = Custom {
            a: ManuallyDrop::new(Noisy::new("a")),
            b: ManuallyDrop::new(Noisy::new("b")),
        };
    }
    take_log()
}

fn check_8() {
    assert_eq!(
        suppressed(),
        Vec::<String>::new(),
        "a ManuallyDrop never manually dropped leaks — no drop logged"
    );
    assert_eq!(
        custom_order(),
        vec!["drop b", "drop a"],
        "custom Drop dropped b before a, overriding default field order"
    );
    println!("check_8 ✅  ManuallyDrop suppresses auto-drop; lets you choose the order by hand");
}

// ----------------------------------------------------------------------------
// Rung 9 — CAPSTONE: rollback-on-drop Transaction
//
// Build the pattern every database driver, every "temp file unless kept", every
// "undo on error" uses: an RAII transaction that AUTO-ROLLS-BACK unless you
// explicitly commit(). This is the synthesis of the whole ladder:
//   - Drop runs the rollback (rung 1) ...
//   - on EVERY exit path including panic-unwind (rung 7) ...
//   - unless a `committed` drop-flag disarms it (rungs 5 & 7) ...
//   - and it mutates state behind &mut, which it holds across its lifetime.
//
// The "database" is just a Vec<String> of committed rows. A Transaction borrows
// it mutably, stages inserts, and on drop either keeps them (if committed) or
// removes exactly the rows it added (if not).
//
// Build ALL of this:
//   - begin(db):    start a transaction over db, 0 rows added, not committed.
//   - insert(row):  push row onto db, remember you added one, log "insert <row>".
//   - commit(self): disarm rollback and log "commit".
//   - Drop:         if NOT committed, pop the `added` rows back off and log
//                   "rollback"; if committed, do nothing.
//
// Expected behavior (asserted in check_9):
//   committed txn  -> rows kept,    log ["insert a", "insert b", "commit"]
//   dropped txn    -> rows removed, log ["insert a", "insert b", "rollback"]
//   PANIC mid-txn  -> rows removed (rollback still fires while unwinding)
// ----------------------------------------------------------------------------

struct Transaction<'a> {
    db: &'a mut Vec<String>,
    added: usize,
    committed: bool,
}

impl<'a> Transaction<'a> {
    fn begin(db: &'a mut Vec<String>) -> Self {
        Self {
            db,
            added: 0,
            committed: false,
        }
    }

    fn insert(&mut self, row: &str) {
        self.db.push(row.to_owned());
        self.added += 1;
        log(format!("insert {}", row));
    }

    fn commit(mut self) {
        self.committed = true;
        log("commit");
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            for _ in 0..self.added {
                self.db.pop();
            }
            log("rollback");
        }
    }
}

fn check_9() {
    // 1) commit keeps the rows
    take_log();
    let mut db: Vec<String> = Vec::new();
    {
        let mut txn = Transaction::begin(&mut db);
        txn.insert("a");
        txn.insert("b");
        txn.commit();
    }
    assert_eq!(db, vec!["a", "b"], "committed rows must be kept");
    assert_eq!(take_log(), vec!["insert a", "insert b", "commit"]);

    // 2) drop without commit rolls back, preserving pre-existing rows
    take_log();
    let mut db: Vec<String> = vec!["existing".to_string()];
    {
        let mut txn = Transaction::begin(&mut db);
        txn.insert("a");
        txn.insert("b");
        // no commit -> Drop rolls back
    }
    assert_eq!(
        db,
        vec!["existing"],
        "uncommitted txn must roll back its own rows only"
    );
    assert_eq!(take_log(), vec!["insert a", "insert b", "rollback"]);

    // 3) rollback even fires on PANIC (RAII during unwinding)
    take_log();
    let mut db: Vec<String> = vec!["existing".to_string()];
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut txn = Transaction::begin(&mut db);
        txn.insert("a");
        panic!("boom mid-transaction");
        #[allow(unreachable_code)]
        txn.commit();
    }));
    assert!(result.is_err(), "the closure should have panicked");
    assert_eq!(db, vec!["existing"], "rollback must run during unwind too");
    assert_eq!(take_log(), vec!["insert a", "rollback"]);

    println!(
        "check_9 ✅  CAPSTONE: rollback-on-drop transaction (commit disarms; panic still rolls back)"
    );
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
    println!("\nall checks passed 🎉");
}
