//! Pin & Unpin — self-referential types, why `Pin` exists, `Box::pin`.
//!
//! Run: `cargo run --bin pin_unpin`
//!
//! Mental model: `Pin<P>` wraps a pointer `P` (e.g. `Box<T>`, `&mut T`) and
//! promises the `T` behind it will never move again before it drops. It keeps
//! that promise by WITHHOLDING `&mut T` (the thing that lets you `mem::swap`
//! a value out). `Unpin` = "I don't care if I move, pinning me is a no-op" —
//! almost every type is `Unpin`. Pin only matters for `!Unpin` types, and the
//! canonical `!Unpin` type is a self-referential struct — which is what async
//! blocks compile into, hence `poll(self: Pin<&mut Self>)`.
//!
//! Ladder (DONE marks finished rungs):
//!   1. [x] Foundations — construct a pin: `Box::pin` / `Pin::new`, `.as_mut()`
//!   2. [x] Foundations — Unpin means pinning is a no-op (escape freely)
//!   3. [x] Mechanics   — get_mut (safe, Unpin) vs get_unchecked_mut (unsafe)
//!   4. [x] Footgun     — self-referential struct + move => dangling pointer (the WHY)
//!   5. [x] Footgun     — PhantomPinned => !Unpin => Pin::new won't compile (E0277)
//!   6. [x] Footgun     — can't move out of a !Unpin pin (get_mut/swap rejected)
//!   7. [x] Real-world  — Pin in Future::poll, drive via Box::pin
//!   8. [x] Real-world  — pin projection by hand (map_unchecked_mut vs &mut)
//!   9. [x] Capstone    — a correct self-referential struct (Box::pin ctor, safe API)

use std::marker::PhantomPinned;
use std::pin::Pin;

// ───────────────────────────── Rung 1 ─────────────────────────────
// Foundations: a `Pin<P>` is just a pointer with a promise. Build one two ways
// and reach the value through it.
//
// TODO(you): implement `pin_basics` so `check_1` passes.
//  - Heap-pin the string with `Box::pin(...)` -> `Pin<Box<String>>`.
//    Read its length through the pin (Pin derefs to &T) and return it.
//  - Also demonstrate `Pin::new(&mut n)` on a stack i32 (i32 is Unpin so this
//    is allowed), then read it back. You don't have to return that part — the
//    point is just to construct both flavors of pin.
fn pin_basics(s: String) -> usize {
    let boxed_string = Box::pin(s);
    let len = boxed_string.len();
    let mut i32 = 0;
    let _pinned_i32 = Pin::new(&mut i32);
    len
}

fn check_1() {
    let len = pin_basics(String::from("pinned"));
    assert_eq!(len, 6);
    println!("check_1 ok: constructed Pin<Box<T>> and Pin<&mut T>, read through the pin");
}

// ───────────────────────────── Rung 2 ─────────────────────────────
// Foundations: `Unpin` means "moving me is fine, so pinning me is meaningless."
// Almost every type is Unpin, and for an Unpin type you can *escape* the Pin
// freely and safely — the promise costs nothing.
//
// TODO(you): implement `unpin_escape` so `check_2` passes.
//  - Take the pinned counter `p: Pin<&mut i32>`.
//  - Because i32: Unpin, you can call `p.get_mut()` — the SAFE method that
//    hands back `&mut i32` (it only exists when the pointee is Unpin). Use it
//    to add `delta` to the value.
//  - Return nothing; the caller reads the mutated i32 afterwards.
//
// Also fill in the assert: is `String` Unpin? Set EXPECT_STRING_UNPIN to the
// right bool (there's a compile-checked witness in check_2 that proves it).
const EXPECT_STRING_UNPIN: bool = true;

fn unpin_escape(p: Pin<&mut i32>, delta: i32) {
    // get_mut takes `self` by value, so call it once and mutate through that &mut.
    *p.get_mut() += delta;
}

// Compile-time witness: this fn only accepts T: Unpin, so calling it *proves*
// the type is Unpin. (Don't edit this helper.)
fn assert_unpin<T: Unpin>() {}

fn check_2() {
    let mut n = 40;
    let pinned = Pin::new(&mut n);
    unpin_escape(pinned, 2);
    assert_eq!(n, 42);

    // Prove String is Unpin at compile time, and that our expectation matches.
    assert_unpin::<String>();
    assert!(
        EXPECT_STRING_UNPIN,
        "you said String is not Unpin, but assert_unpin::<String>() compiled"
    );
    println!("check_2 ok: Unpin lets you escape the Pin safely (get_mut); String is Unpin");
}

// ───────────────────────────── Rung 3 ─────────────────────────────
// Mechanics: the Pin API has two doors to `&mut T`.
//   - `get_mut(self) -> &mut T`             SAFE, exists ONLY when T: Unpin.
//   - `get_unchecked_mut(self) -> &mut T`   UNSAFE, exists for ANY T; YOU
//        promise not to use the &mut to move the value out.
// Here `Widget` is Unpin (all-plain-fields), so you have both doors. The lesson
// is feeling the difference: mutate one field via the safe door, another via
// the unsafe door (with a SAFETY note), through the SAME pin.
//
// TODO(you): implement `bump_widget` so `check_3` passes.
//  - `w: Pin<&mut Widget>`.
//  - Increment `w.count` using the SAFE `get_mut` (Widget: Unpin).
//  - Then append '!' to `w.label` using `get_unchecked_mut` inside an
//    `unsafe { ... }` block. Write the `// SAFETY:` line yourself: why is
//    taking &mut here sound? (hint: what do we do with the &mut, and is Widget
//    even a type that cares about moving?)
//  Note: after the first get_mut consumes the pin, re-borrow with `.as_mut()`
//  BEFORE calling — i.e. call get_mut on `w.as_mut()` so you still have `w`.
#[derive(Debug)]
struct Widget {
    count: u32,
    label: String,
}

fn bump_widget(mut w: Pin<&mut Widget>) {
    // as_mut() reborrows so get_mut doesn't consume `w`.
    w.as_mut().get_mut().count += 1;

    // SAFETY: we only mutate a field in place — we never move Widget out of
    // the pin. (Widget is also Unpin, so pinning isn't load-bearing here.)
    unsafe {
        w.get_unchecked_mut().label += "!";
    }
}

fn check_3() {
    let mut widget = Widget {
        count: 0,
        label: String::from("hi"),
    };
    let pinned = Pin::new(&mut widget);
    bump_widget(pinned);
    assert_eq!(widget.count, 1);
    assert_eq!(widget.label, "hi!");
    println!("check_3 ok: safe get_mut vs unsafe get_unchecked_mut through one pin");
}

// ───────────────────────────── Rung 4 ─────────────────────────────
// THE WHY. A self-referential struct: `data` is bytes stored INLINE, and `ptr`
// points AT `data[0]` — i.e. into the struct itself. This is the shape async
// blocks generate (a local that borrows another local across an `.await`). It's
// fine to READ while it sits still — but MOVE the struct and `data` relocates
// to a new address while `ptr` still holds the OLD one. Dangling pointer.
// NO Pin here yet: the whole job of this rung is to reproduce the bug Pin was
// invented to prevent — and to see WHY returning-by-value from `new()` can't
// safely set up the self-pointer.
//
// Data is inline (`[u8; 4]`) so that moving the struct really relocates it.
struct SelfRef {
    data: [u8; 4],
    ptr: *const u8, // points at &data[0] once initialized (else null)
}

impl SelfRef {
    // Note: we DON'T set `ptr` here. If we set ptr = &self.data[0] and then
    // returned self by value, the return-move would instantly invalidate it.
    // So new() leaves ptr null; you initialize it in-place afterwards.
    // TODO(you): construct with the given bytes and a null ptr.
    fn new(bytes: [u8; 4]) -> SelfRef {
        SelfRef {
            data: bytes,
            ptr: std::ptr::null(),
        }
    }

    // Wire up the self-pointer AFTER the struct is parked at its final address.
    // TODO(you): set self.ptr to the address of self.data[0].
    fn init(&mut self) {
        self.ptr = &self.data[0] as *const u8;
    }

    // Read the byte behind self.ptr. Only valid if init() ran AND the struct has
    // not moved since. TODO(you): unsafe-read *self.ptr and add a // SAFETY line.
    fn deref_ptr(&self) -> u8 {
        // SAFETY: we only read the byte at the address stored in `self.ptr`.
        // The address is valid because `self.init()` was called and the struct
        // has not moved since.
        unsafe { *self.ptr }
    }

    fn ptr_addr(&self) -> usize {
        self.ptr as usize
    }

    fn data_addr(&self) -> usize {
        &self.data[0] as *const u8 as usize
    }
}

fn check_4() {
    let mut a = SelfRef::new([7, 8, 9, 10]);
    a.init(); // now ptr points at a.data[0], in a's current location
    assert_eq!(
        a.ptr_addr(),
        a.data_addr(),
        "after init, ptr should point at data[0]"
    );
    assert_eq!(
        a.deref_ptr(),
        7,
        "reading through the self-pointer sees data[0]"
    );

    // Now MOVE the struct into a new binding. Its inline bytes relocate, but the
    // stored ptr keeps the OLD address => it is now STALE. We prove the hazard
    // by comparing addresses; we deliberately do NOT call deref_ptr() after the
    // move, because that would be reading a dangling pointer (real UB).
    let moved = a;
    assert_ne!(
        moved.ptr_addr(),
        moved.data_addr(),
        "expected the self-pointer to be STALE after moving — this is the corruption Pin prevents",
    );
    println!(
        "check_4 ok: reproduced the self-referential dangling-pointer hazard (moving corrupts the self-pointer; deref-after-move would be UB)"
    );
}

// ───────────────────────────── Rung 5 ─────────────────────────────
// Footgun → fix. In rung 4 nothing stopped the move. Now we opt OUT of Unpin by
// adding a `PhantomPinned` field. A `!Unpin` type LOSES the safe `Pin::new`
// constructor and the safe `get_mut` — the compiler now refuses the operations
// that could move it. The remaining safe way to pin a `!Unpin` value is
// `Box::pin(value)`: it allocates on the heap (a stable address) and hands you a
// `Pin<Box<T>>` that owns the value, so nothing can move it out.
//
// TODO(you): implement `new` and `make_pinned` so `check_5` passes.
struct Immovable {
    data: [u8; 4],
    ptr: *const u8,
    _pin: PhantomPinned, // ← this field makes Immovable !Unpin
}

impl Immovable {
    // TODO(you): build it with ptr = null and _pin = PhantomPinned.
    fn new(bytes: [u8; 4]) -> Immovable {
        Self {
            data: bytes,
            ptr: std::ptr::null(),
            _pin: PhantomPinned,
        }
    }

    // TODO(you): pin it on the heap. One line: Box::pin(Immovable::new(bytes)).
    fn make_pinned(bytes: [u8; 4]) -> Pin<Box<Immovable>> {
        Box::pin(Immovable::new(bytes))
    }
}

fn check_5() {
    let pinned: Pin<Box<Immovable>> = Immovable::make_pinned([1, 2, 3, 4]);
    // Read through the pin (Pin<Box<T>> derefs to &T): first byte is 1.
    assert_eq!(pinned.data[0], 1);
    // `ptr` is the self-ref slot from rung 4; `new` leaves it null until wired.
    assert!(pinned.ptr.is_null());

    // WITNESS THE COMPILE ERROR (do this once, then re-comment):
    // Uncomment the two lines below. `Immovable` is !Unpin, so `Pin::new` is
    // NOT available (E0277: PhantomPinned is not Unpin). This is the compiler
    // slamming the door that let rung 4's bug happen.
    //
    // let mut imm = Immovable::new([9, 9, 9, 9]);
    // let _p = Pin::new(&mut imm); // ← E0277: the trait bound `PhantomPinned: Unpin` is not satisfied

    // Compile-time proof that Immovable is NOT Unpin: if it WERE, this line would
    // compile — it must be left commented. (assert_unpin from rung 2.)
    // assert_unpin::<Immovable>(); // ← would be E0277 too

    println!(
        "check_5 ok: PhantomPinned => !Unpin => no Pin::new / no get_mut; Box::pin is the door"
    );
}

// ───────────────────────────── Rung 6 ─────────────────────────────
// Footgun: once you hold a `Pin<&mut Immovable>` (Immovable is !Unpin), there is
// NO safe way back to `&mut Immovable`. `get_mut` doesn't exist for !Unpin, and
// without `&mut` you can't `mem::swap`/`mem::replace` the value out. That's the
// pin guarantee doing its job: it forbids exactly the moves that would corrupt a
// self-pointer. To mutate a FIELD in place you must go through the unsafe door
// and promise not to move the whole value.
//
// TODO(you): implement `set_first_byte` so `check_6` passes.
//  - `p: Pin<&mut Immovable>`, set `data[0] = v` IN PLACE.
//  - `get_mut` won't compile here (try it to feel the wall) — use
//    `get_unchecked_mut` inside `unsafe`, and write the `// SAFETY:` line: we
//    only write a field in place, we never move the Immovable.
fn set_first_byte(p: Pin<&mut Immovable>, v: u8) {
    // SAFETY: we only write a field in place, we never move the Immovable.
    unsafe {
        p.get_unchecked_mut().data[0] = v;
    }
}

fn check_6() {
    let mut pinned: Pin<Box<Immovable>> = Immovable::make_pinned([1, 2, 3, 4]);
    set_first_byte(pinned.as_mut(), 99);
    assert_eq!(pinned.data[0], 99);

    // WITNESS THE WALL (uncomment once, then re-comment):
    // (a) get_mut is not available for a !Unpin type:
    //   let _bad: &mut Immovable = pinned.as_mut().get_mut(); // E0277: Immovable: Unpin not satisfied
    // (b) and without &mut you cannot mem::swap two pinned Immovables:
    //   let mut other = Immovable::make_pinned([5, 6, 7, 8]);
    //   std::mem::swap(pinned.as_mut().get_mut(), other.as_mut().get_mut()); // same E0277

    println!(
        "check_6 ok: a !Unpin pin has no safe &mut => no swap/replace; only unsafe in-place edits"
    );
}

// ───────────────────────────── Rung 7 ─────────────────────────────
// Real-world: THE reason Pin exists. `Future::poll` takes `self: Pin<&mut Self>`
// — never a plain `&mut Self` — because an `async` block compiles to a possibly
// self-referential state machine, and the executor must be forbidden from moving
// it between polls. So a runtime always parks the future at a fixed address
// (here: `Box::pin`) and only ever hands `poll` a pinned reference.
//
// `Countdown` is a hand-written future. It is Unpin (plain fields), so inside
// poll you may reach the fields via the SAFE `self.get_mut()`.
//
// TODO(you): implement `Future::poll` for Countdown so `check_7` passes.
//  - Signature is fixed by the trait: `poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<u32>`.
//  - Get `&mut Self` via `self.get_mut()` (OK: Countdown: Unpin).
//  - If `remaining == 0`, return `Poll::Ready(self.total)`.
//  - Otherwise decrement `remaining`, ask to be polled again: call
//    `cx.waker().wake_by_ref()` then return `Poll::Pending`.
use std::future::Future;
use std::task::{Context, Poll};

struct Countdown {
    remaining: u32,
    total: u32,
}

impl Future for Countdown {
    type Output = u32;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.remaining == 0 {
            Poll::Ready(this.total)
        } else {
            this.remaining -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

// A tiny blocking driver: pins the future on the heap and polls in a loop until
// Ready, using a no-op waker. (Provided — you don't edit this.)
fn block_on<F: Future>(fut: F) -> F::Output {
    use std::task::{RawWaker, RawWakerVTable, Waker};
    fn noop_raw() -> RawWaker {
        fn no(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            noop_raw()
        }
        RawWaker::new(std::ptr::null(), &RawWakerVTable::new(clone, no, no, no))
    }
    let waker = unsafe { Waker::from_raw(noop_raw()) };
    let mut cx = Context::from_waker(&waker);
    let mut boxed = Box::pin(fut); // <- future parked at a fixed heap address
    loop {
        match boxed.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => continue,
        }
    }
}

fn check_7() {
    let out = block_on(Countdown {
        remaining: 3,
        total: 42,
    });
    assert_eq!(out, 42);
    println!("check_7 ok: hand-written Future::poll(self: Pin<&mut Self>), driven via Box::pin");
}

// ───────────────────────────── Rung 8 ─────────────────────────────
// Real-world: PIN PROJECTION. A combinator holds an inner future; to poll it,
// `Pin<&mut Combinator>` must become `Pin<&mut inner>`. There are two kinds of
// projection, and choosing wrong is unsound:
//   - STRUCTURAL (for a !Unpin field): the field is pinned whenever the parent
//     is. Project with `map_unchecked_mut` -> `Pin<&mut Field>`. You promise
//     never to move that field out and never to expose a `&mut Field`.
//   - NON-STRUCTURAL (for an Unpin field): pinning the parent says nothing about
//     the field, so you may hand out a plain `&mut Field` freely.
// This is precisely what the `pin-project` crate generates for you.
//
// `Child` is !Unpin (PhantomPinned). `Composite` holds a `child: Child`
// (project structurally) and a `tag: u32` (project non-structurally).
struct Child {
    n: u32,
    _pin: PhantomPinned,
}

impl Child {
    fn bump(self: Pin<&mut Self>) {
        // SAFETY: we mutate a field in place, never moving Child out of its pin.
        unsafe { self.get_unchecked_mut().n += 1 }
    }

    fn get(&self) -> u32 {
        self.n
    }
}

struct Composite {
    child: Child, // !Unpin field  -> structural projection
    tag: u32,     // Unpin field   -> non-structural projection
}

impl Composite {
    // TODO(you): STRUCTURAL projection to the !Unpin child.
    // Return `Pin<&mut Child>` using `self.map_unchecked_mut(|s| &mut s.child)`
    // inside `unsafe`. Write the `// SAFETY:` line: child is pinned whenever
    // Composite is, we never move it out, and we never hand out a bare &mut Child.
    fn child_pin(self: Pin<&mut Self>) -> Pin<&mut Child> {
        // SAFETY: we only read a field in place, we never move Composite out of its pin.
        unsafe { self.map_unchecked_mut(|s| &mut s.child) }
    }

    // TODO(you): NON-STRUCTURAL projection to the Unpin tag.
    // Return `&mut u32`. Since u32 is Unpin, it's sound to expose a bare &mut.
    // Get there via `self.get_unchecked_mut()` (unsafe) then `&mut ...tag`, and
    // write the SAFETY line: tag is Unpin so pinning is irrelevant to it, and we
    // don't move Composite.
    fn tag_mut(self: Pin<&mut Self>) -> &mut u32 {
        // SAFETY: we only read a field in place, we never move Composite out of its pin.
        unsafe { &mut self.get_unchecked_mut().tag }
    }
}

fn check_8() {
    let mut c: Pin<Box<Composite>> = Box::pin(Composite {
        child: Child {
            n: 0,
            _pin: PhantomPinned,
        },
        tag: 0,
    });

    // Structural: project to the pinned child and advance it through its pin.
    c.as_mut().child_pin().bump();
    c.as_mut().child_pin().bump();

    // Non-structural: project to the Unpin tag and set it via a plain &mut.
    *c.as_mut().tag_mut() = 7;

    assert_eq!(c.child.get(), 2);
    assert_eq!(c.tag, 7);
    println!(
        "check_8 ok: structural (map_unchecked_mut) vs non-structural (&mut) pin projection by hand"
    );
}

// ───────────────────────────── Rung 9 (capstone) ─────────────────────────────
// Build a CORRECT self-referential type — the thing rung 4 got wrong. `value` is
// a String; `ptr` points AT the `value` field (self-reference). The recipe that
// makes it sound:
//   1. PhantomPinned  => the type is !Unpin (moving is a compile error).
//   2. Box::pin       => a stable heap address the value will live at forever.
//   3. wire `ptr` AFTER pinning, via get_unchecked_mut — so the address it
//      records is the final resting place, never invalidated by a later move.
//   4. safe accessors that read through the pin; no safe API ever hands out a
//      `&mut value` or moves the struct, so `ptr` can never dangle.
//
// TODO(you): implement `new`, `deref_via_ptr`, and fill the SAFETY lines.
//
// Nudge: run `cargo miri run --bin pin_unpin` after this rung — Miri will flag
// any pointer misuse. Getting a clean Miri run is the real proof of soundness.
struct SelfReferential {
    value: String,
    ptr: *const String, // points at &self.value once wired (else null)
    _pin: PhantomPinned,
}

impl SelfReferential {
    // Construct pinned, THEN wire the self-pointer in place.
    // TODO(you):
    //   1. let mut boxed = Box::pin(SelfReferential { value: text.into(),
    //          ptr: null, _pin: PhantomPinned });
    //   2. compute the address of the value field:
    //          let self_ptr: *const String = &boxed.value;
    //   3. store it via the unsafe door (write the SAFETY line):
    //          unsafe { boxed.as_mut().get_unchecked_mut().ptr = self_ptr; }
    //   4. return boxed.
    fn new(text: &str) -> Pin<Box<SelfReferential>> {
        let mut boxed = Box::pin(SelfReferential {
            value: text.into(),
            ptr: std::ptr::null(),
            _pin: PhantomPinned,
        });
        let self_ptr: *const String = &boxed.value;
        unsafe {
            boxed.as_mut().get_unchecked_mut().ptr = self_ptr;
        }
        boxed
    }

    // Read the String through the SELF-POINTER (not through &self.value directly)
    // to prove the pointer is valid and aims at our own field.
    // TODO(you): unsafe-deref self.ptr and return its &str; add a // SAFETY line
    // stating: ptr was wired to &self.value after pinning and the struct, being
    // !Unpin + pinned, has not moved, so the pointer is still valid.
    fn deref_via_ptr(&self) -> &str {
        // SAFETY: ptr was wired to &self.value after pinning and the struct, being !Unpin + pinned, has not moved, so the pointer is still valid.
        unsafe { (*self.ptr).as_str() }
    }

    fn value(&self) -> &str {
        &self.value
    }
    fn ptr_addr(&self) -> usize {
        self.ptr as usize
    }
    fn value_addr(&self) -> usize {
        &self.value as *const String as usize
    }
}

fn check_9() {
    let s = SelfReferential::new("hello");

    // Direct read and read-through-the-self-pointer agree:
    assert_eq!(s.value(), "hello");
    assert_eq!(s.deref_via_ptr(), "hello");

    // Prove it is genuinely self-referential: ptr points AT the value field.
    assert_eq!(
        s.ptr_addr(),
        s.value_addr(),
        "ptr must point at our own value field"
    );

    // The payoff vs rung 4: moving `s` out of the pin is a COMPILE error, so the
    // self-pointer can never go stale. Uncomment to witness (then re-comment):
    //   let moved = *s;            // E0507: cannot move out of a Pin<Box<..>> deref
    //   let inner = Pin::into_inner(s); // unavailable: SelfReferential: Unpin not satisfied

    println!(
        "check_9 ok: a sound self-referential struct (PhantomPinned + Box::pin + wire-after-pin); moving it won't compile"
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
    println!("\nall implemented checks passed ✅");
}
