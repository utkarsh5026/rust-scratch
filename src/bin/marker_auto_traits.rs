// Marker & auto traits — Send, Sync, Sized, Copy; ?Sized; negative reasoning
// Run: cargo run --bin marker_auto_traits
//
// Ladder:
//   1. Marker trait as a permission tag                 [foundations]  DONE
//   2. Copy is a marker (move vs copy, why it needs Clone)  [foundations]  DONE
//   3. Sized & ?Sized (the invisible bound)             [mechanics]  DONE
//   4. Auto traits compose structurally                 [mechanics]  DONE
//   5. Rc is !Send across threads                       [footgun]  DONE
//   6. Negative reasoning & opt-out (PhantomData<*const ()>) [footgun]  DONE
//   7. PhantomData as a marker (typed IDs / units)      [real-world]  DONE
//   8. unsafe impl Send done right                      [real-world]  DONE
//   9. Capstone: type-state machine with marker traits  [capstone]  DONE

// ---------------------------------------------------------------------------
// Rung 1 — Marker trait as a permission tag
//
// A marker trait has NO methods. Its only job is to TAG a set of types so a
// generic function can require the tag as a bound. The tag IS the permission.
//
// Your turn:
//   - Implement the empty marker trait `Approved` for `Admin` and `Editor`,
//     but DELIBERATELY NOT for `Guest`.
//   - Implement `can_publish` so it returns true. Note its bound `T: Approved`
//     — that bound is the whole lesson: only tagged types may be passed.
//
// Then (optional, to feel the point): uncomment the `can_publish(&Guest)` line
// in check_1 and watch it fail to compile — Guest isn't Approved.
// ---------------------------------------------------------------------------

trait Approved {}

struct Admin;
struct Editor;

#[allow(dead_code)]
struct Guest;

// TODO: impl Approved for Admin and Editor (one line each). Leave Guest out.
impl Approved for Admin {}
impl Approved for Editor {}

fn can_publish<T: Approved>(_user: &T) -> bool {
    true
}

fn check_1() {
    assert!(can_publish(&Admin));
    assert!(can_publish(&Editor));
    // Uncomment to see the marker gate reject an untagged type:
    // assert!(can_publish(&Guest)); // ERROR: the trait bound `Guest: Approved` is not satisfied
    println!("check_1 ok: marker trait gated can_publish to Approved types only");
}

// ---------------------------------------------------------------------------
// Rung 2 — Copy is a marker
//
// `Copy` is the most famous marker trait. It has no methods (the one "method",
// Clone::clone, lives on its supertrait Clone). It just TELLS the compiler:
// "duplicate me bit-for-bit on assignment instead of moving me." So the marker
// changes language semantics, not behavior you call.
//
// Three things to feel here:
//   (a) Without Copy, assigning/​passing a value MOVES it (original unusable).
//   (b) Derive Copy and the same code suddenly leaves the original valid.
//   (c) Copy REQUIRES Clone (it's `Copy: Clone`), and you can't be Copy if any
//       field isn't Copy (e.g. a String field blocks it).
//
// Your turn:
//   - Make `Point` Copy by adding the right derive(s). (Hint: Copy needs Clone.)
//   - Implement `sum_uses_original`: take a Point by value into a helper, then
//     STILL read the original afterward. This only compiles if Point is Copy.
//
// Bonus understanding (no code): `Blob` below has a String field on purpose —
// try adding `#[derive(Copy)]` to it and read why the compiler refuses.
// ---------------------------------------------------------------------------

// TODO: add the derive that makes Point a Copy type.
#[derive(Copy, Clone)]
struct Point {
    x: i32,
    y: i32,
}

struct Blob {
    #[allow(dead_code)]
    name: String, // a String is NOT Copy — so Blob can never be Copy. (Don't fix this.)
}

fn manhattan(p: Point) -> i32 {
    p.x.abs() + p.y.abs()
}

fn sum_uses_original() -> i32 {
    let p = Point { x: 3, y: -4 };
    let d = manhattan(p); // p is COPIED in here (because Point: Copy)...
    d + p.x + p.y
}

fn check_2() {
    let p = Point { x: 1, y: 2 };
    let q = p; // copy, not move
    // Both p and q are valid because Point is Copy:
    assert_eq!(p.x + q.x, 2);
    assert_eq!(sum_uses_original(), 7 + 3 + (-4)); // manhattan(3,-4)=7, plus x+y
    let _ = Blob { name: "ok".into() };
    println!("check_2 ok: Copy is a marker that flips move semantics into copy");
}

// ---------------------------------------------------------------------------
// Rung 3 — Sized & ?Sized (the invisible bound)
//
// `Sized` is a marker the compiler auto-implements for every type whose size is
// known at compile time (i32, Point, Vec<T>, ...). It is NOT implemented for
// "dynamically sized types" (DSTs): `str`, `[T]`, `dyn Trait`. You can never
// hold a bare DST by value — only behind a pointer (&str, Box<str>, &[T]).
//
// The twist that defines this rung: EVERY generic `<T>` has a SILENT `T: Sized`
// bound the compiler inserts for you. So `fn f<T>(x: T)` secretly means
// `fn f<T: Sized>(x: T)`. To accept DSTs you must OPT OUT with the special
// relaxed bound `?Sized` ("maybe sized") — the only place `?` appears on a bound.
//
// Once T may be unsized, you can no longer take it BY VALUE (unknown size on the
// stack) — you must take it behind a reference: `&T`, `Box<T>`, etc.
//
// Your turn:
//   - `last_byte` should work for ordinary sized types AND for `str`/`[u8]`.
//     Right now its bound rejects `str`. Relax the generic so it accepts unsized
//     types, and keep the parameter as `&T` (you can't take an unsized T by value).
//   - Implement the body: return the last byte of the value's bytes, or None if
//     empty. (Hint below in the helper — you're given a `as_bytes`-ish view.)
//
// Keep the trait `Bytes` as-is; it just gives every type a byte view so the
// function has something uniform to look at.
// ---------------------------------------------------------------------------

trait Bytes {
    fn view(&self) -> &[u8];
}

impl Bytes for str {
    fn view(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl Bytes for [u8] {
    fn view(&self) -> &[u8] {
        self
    }
}

impl Bytes for Point {
    fn view(&self) -> &[u8] {
        &[] // Point has no meaningful bytes here; treat as empty
    }
}

// TODO: relax the bound so `str` (a DST) is accepted, and keep `value: &T`.
fn last_byte<T: Bytes + ?Sized>(value: &T) -> Option<u8> {
    value.view().last().copied()
}

fn check_3() {
    // Sized type:
    assert_eq!(last_byte(&Point { x: 0, y: 0 }), None);
    // DSTs — only compile if T: ?Sized:
    let s: &str = "hi";
    assert_eq!(last_byte(s), Some(b'i'));
    let bytes: &[u8] = &[1u8, 2, 3];
    assert_eq!(last_byte(bytes), Some(3));
    println!("check_3 ok: ?Sized opened the generic to DSTs (str, [u8])");
}

// ---------------------------------------------------------------------------
// Rung 4 — Auto traits compose structurally
//
// Now the real "auto" traits: `Send` (safe to MOVE to another thread) and
// `Sync` (safe to SHARE &T across threads). You almost never `impl` these — the
// compiler AUTO-implements them for a type *if and only if* all of its fields
// are Send/Sync. They propagate structurally, like Copy did, but for thread
// safety. This is the essence of an "auto trait": opt-out, not opt-in.
//
// The classic way to PROVE a type is Send/Sync at compile time is a zero-cost
// witness function: `fn assert_send<T: Send>() {}`. Calling `assert_send::<Foo>()`
// compiles only if Foo: Send. No runtime code — it's a pure type-level check.
//
// Your turn:
//   - Implement the two witness functions `assert_send` and `assert_sync`. Their
//     BODIES are empty `{}` — all the work is in the BOUND you put on them.
//   - In check_4, the calls already assert that `Wrapper` (all-Send/Sync fields)
//     passes. Then add ONE field of type `Rc<i32>` to `Poisoned` and observe in
//     a comment what happens (you'll actually break it in rung 5; here just make
//     `Poisoned` compile as all-Send by using thread-safe fields).
//
// Goal: make assert_send::<Wrapper>() and assert_sync::<Wrapper>() compile, and
// assert_send::<Poisoned>() compile (Poisoned must be built from Send fields).
// ---------------------------------------------------------------------------

use std::sync::Arc;

#[allow(dead_code)]
struct Wrapper {
    id: u64,
    name: String,
    tags: Vec<u8>,
}

struct Poisoned {
    // TODO: give Poisoned at least one field. Use a THREAD-SAFE shared pointer
    // so it stays Send+Sync (hint: Arc, not Rc — Rc is rung 5's footgun).
    #[allow(dead_code)]
    shared: Arc<i32>,
}

// TODO: implement the two witnesses. The lesson is entirely in the bound.
fn assert_send<T>()
where
    T: Send,
{
    {}
}
fn assert_sync<T>()
where
    T: Sync,
{
    {}
}

fn check_4() {
    // All fields are Send + Sync, so the compiler auto-derived both for Wrapper:
    assert_send::<Wrapper>();
    assert_sync::<Wrapper>();
    // Built only from Send/Sync fields, so this auto-derives too:
    assert_send::<Poisoned>();
    assert_sync::<Poisoned>();
    // Witness types so the structs aren't "never constructed":
    let _ = Wrapper {
        id: 1,
        name: "x".into(),
        tags: vec![0],
    };
    let _ = Poisoned {
        shared: Arc::new(7),
    };
    println!("check_4 ok: Send/Sync auto-derived structurally from the fields");
}

// ---------------------------------------------------------------------------
// Rung 5 — Rc is !Send across threads (the defining footgun)
//
// This is THE auto-trait error every Rustacean meets. `Rc<T>` uses a plain,
// non-atomic reference count. If two threads cloned/dropped the same Rc at once,
// the count would race and you'd get a use-after-free. So the standard library
// marks Rc as NOT Send and NOT Sync (`impl !Send for Rc`). Because Send is an
// auto trait, that one `!Send` POISONS anything containing an Rc — structurally,
// exactly like rung 4 but in reverse.
//
// `thread::spawn(move || ...)` requires its closure to be `Send` (it captures
// your data and moves it to another thread). Capture an Rc and the closure
// becomes !Send → compile error.
//
// Your turn — TWO parts:
//   (a) FEEL the error: uncomment the `broken()` call in check_5, run it, read
//       the "`Rc<i32>` cannot be sent between threads safely" error, then
//       re-comment it. (The body is already written for you to trigger it.)
//   (b) FIX it for real: implement `parallel_sum` using the THREAD-SAFE sibling
//       `Arc<i32>` instead. Spawn N threads, each reads the shared value, sum
//       their contributions, and return the total. Arc IS Send+Sync (atomic
//       count), so the same shape compiles.
//
// Why Arc works where Rc doesn't: same API, but the refcount is atomic. That
// atomicity is the invariant that earns the auto trait back.
// ---------------------------------------------------------------------------

use std::thread;

// FOOTGUN demo — kept COMMENTED so the file still compiles (a fn body is
// type-checked even when never called). To FEEL the error: uncomment this whole
// fn AND the `broken()` call in check_5, run, read
// "`Rc<i32>` cannot be sent between threads safely", then re-comment both.
//
// fn broken() {
//     let data = std::rc::Rc::new(41);
//     let handle = thread::spawn(move || {
//         *data + 1 // captures `data: Rc<i32>` → closure is !Send → ERROR
//     });
//     let _ = handle.join();
// }

// Your turn: make this compile and return n_threads * value, using Arc.
fn parallel_sum(value: i32, n_threads: usize) -> i32 {
    let data = Arc::new(value);
    let handles = (0..n_threads)
        .map(|_| {
            let data = Arc::clone(&data);
            thread::spawn(move || *data)
        })
        .collect::<Vec<_>>();

    let mut sum = 0;
    for handle in handles {
        sum += handle.join().unwrap();
    }
    sum
}

fn check_5() {
    // (a) Uncomment to witness the !Send error, then re-comment:
    // broken();

    // (b) The Arc fix:
    assert_eq!(parallel_sum(10, 4), 40);
    assert_eq!(parallel_sum(-3, 3), -9);
    println!("check_5 ok: Rc is !Send (poisons the closure); Arc's atomic count fixes it");
}

// ---------------------------------------------------------------------------
// Rung 6 — Negative reasoning & opting OUT
//
// Auto traits are reasoned about NEGATIVELY: a type is Send "unless something in
// it isn't." So how do you make YOUR OWN type !Send when all its real fields are
// perfectly Send? You add a zero-sized field whose TYPE is !Send, and the auto
// trait propagation does the rest. The canonical "make me thread-unsafe" token
// is `PhantomData<*const ()>` — a raw pointer is !Send AND !Sync, and
// PhantomData lets you carry that property with zero runtime size/cost.
//
// Why is a raw pointer !Send/!Sync? The compiler can't verify what it points to
// or who else touches it, so it conservatively refuses the auto traits. (On
// nightly you could write `impl !Send for T {}` explicitly; on stable, the
// PhantomData-of-a-raw-pointer trick is how everyone does it.)
//
// Real-world use: a handle tied to ONE thread (e.g. a `*mut` into a thread-local
// GUI context, or a `MutexGuard`) must not cross threads. Marking it !Send makes
// the compiler enforce that for you.
//
// Your turn:
//   - `ThreadBound` should be a !Send, !Sync handle even though its only real
//     data is a plain `id: u32`. Give it the right PhantomData field so the
//     compiler refuses to send it across threads.
//   - Implement `new` and `id`.
//   - The compile-time proof: `assert_not_send` uses a trick (an autoref-based
//     specialization) to check !Send WITHOUT failing compilation. It's provided
//     — just make `ThreadBound` actually !Send so it returns false.
//
// Sanity check after: `Wrapper` from rung 4 is still Send, so the helper returns
// true for it. Only ThreadBound flips to false.
// ---------------------------------------------------------------------------

use std::marker::PhantomData;

struct ThreadBound {
    id: u32,
    phantom: PhantomData<*const ()>,
}

impl ThreadBound {
    fn new(id: u32) -> Self {
        Self {
            id,
            phantom: PhantomData,
        }
    }
    fn id(&self) -> u32 {
        self.id
    }
}

// --- Provided: a stable-Rust "is T Send?" runtime probe via autoref-style
// specialization. It only resolves correctly at a CONCRETE type, so it's exposed
// as a macro (a generic fn wrapper would erase the Send info and always say
// false). At a concrete type, the inherent `is_send` (gated on T: Send) wins
// method resolution when it applies; otherwise the trait default (false) is used.
struct Probe<T>(PhantomData<T>);
trait NotSend {
    fn is_send(&self) -> bool {
        false
    }
}
impl<T> NotSend for Probe<T> {}
impl<T: Send> Probe<T> {
    fn is_send(&self) -> bool {
        true
    }
}
macro_rules! is_send {
    ($t:ty) => {{ Probe::<$t>(PhantomData).is_send() }};
}

fn check_6() {
    // Real, Send types still report true:
    assert!(is_send!(Wrapper));
    assert!(is_send!(i32));
    // Our deliberately thread-bound handle is !Send:
    assert!(!is_send!(ThreadBound));

    let h = ThreadBound::new(7);
    assert_eq!(h.id(), 7);
    println!("check_6 ok: PhantomData<*const ()> opted ThreadBound OUT of Send/Sync");
}

// ---------------------------------------------------------------------------
// Rung 7 — PhantomData as a marker: typed IDs, and CHOOSING the marker type
//
// PhantomData<T> lets a type "carry" a type parameter T it never actually
// stores. The classic use is a typed ID: a u64 key tagged with WHICH entity it
// belongs to, so `Id<User>` and `Id<Post>` are DIFFERENT types — mixing them is
// a compile error, with zero runtime cost (it's still just a u64).
//
// The deep part of THIS rung is that the marker type you put inside PhantomData
// controls auto-trait + variance behavior:
//   - PhantomData<T>          → "acts like it OWNS a T": inherits T's Send/Sync,
//                               participates in drop check. (Use when you really
//                               do conceptually own a T.)
//   - PhantomData<fn() -> T>  → "acts like a FUNCTION producing T": ALWAYS
//                               Send + Sync + Copy regardless of T, covariant.
//                               (Use for a pure tag — you don't own a T.)
//   - PhantomData<*const T>   → !Send + !Sync (rung 6's thread-binding token).
//
// A typed ID is a PURE TAG — you don't own a User just because you hold its id.
// So a `UserId` should stay Send/Sync/Copy even if `User` itself is !Send. The
// right marker is `PhantomData<fn() -> T>`.
//
// Your turn:
//   - Implement `Id<T>` as a u64 + the RIGHT PhantomData so that:
//       * Id<T> is Copy, and Send/Sync even when T is !Send (use fn() -> T).
//       * T is never stored.
//   - Implement `new(raw: u64)` and `raw(&self) -> u64`.
//   - Implement the bodies of the HAND-WRITTEN Clone and PartialEq impls below.
//     Note their bound is on nothing — `impl<T> Clone for Id<T>` with NO
//     `T: Clone` — so the impl works for ALL tags including non-Clone ones. This
//     is exactly the bound `#[derive(Clone)]` would get WRONG: derive would emit
//     `impl<T: Clone> Clone for Id<T>`, needlessly requiring the tag to be Clone.
//     Hand-writing puts the requirement where it belongs: on the u64, not on T.
//   - The PhantomData marker is already chosen as `fn() -> T` — see why below.
//
// `Ghost` below is a deliberately !Send entity type used only as a tag.
// ---------------------------------------------------------------------------

// A !Send "entity" — exists only to be used as a type tag for Id<Ghost>.
#[allow(dead_code)]
struct Ghost(PhantomData<*const ()>);

struct User;
struct Post;

// The marker `fn() -> T` makes Id a PURE TAG: always Send+Sync+Copy, never
// owning a T. (Swap to PhantomData<T> later and watch `is_send!(Id<Ghost>)` flip
// to false — that's the marker-choice lesson made concrete.)
struct Id<T> {
    raw: u64,
    _tag: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    fn new(raw: u64) -> Self {
        Self {
            raw,
            _tag: PhantomData,
        }
    }

    fn raw(&self) -> u64 {
        self.raw
    }
}

// Hand-written so the bound lands on u64, not on the tag T. Fill the bodies.
impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        Self::new(self.raw)
    }
}
impl<T> std::fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Id").field(&self.raw).finish()
    }
}
impl<T> Copy for Id<T> {} // Copy is a marker — no body. Valid because raw: u64 is Copy.
impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

fn fetch_user(id: Id<User>) -> u64 {
    id.raw()
}

fn check_7() {
    let u1: Id<User> = Id::new(1);
    let u2: Id<User> = Id::new(1);
    let p1: Id<Post> = Id::new(1);

    // Same tag → comparable; copy works:
    assert_eq!(u1, u2);
    assert_eq!(fetch_user(u1), 1);
    assert_eq!(u1.raw(), 1); // u1 still usable → Id is Copy
    assert_eq!(p1.raw(), 1);

    // Type-level safety (uncomment to SEE the error, then re-comment):
    // let _ = fetch_user(p1); // ERROR: expected Id<User>, found Id<Post>
    // let _ = (u1 == p1);     // ERROR: can't compare Id<User> with Id<Post>

    // The marker-choice payoff: Id stays Send even when its tag type is !Send.
    assert!(is_send!(Id<Ghost>));
    assert!(is_send!(Id<User>));
    println!("check_7 ok: Id<T> is a zero-cost typed tag; fn()->T keeps it Send+Copy");
}

// ---------------------------------------------------------------------------
// Rung 8 — unsafe impl Send done right
//
// Sometimes auto-derivation is TOO conservative. A type built on a raw pointer
// is automatically !Send and !Sync (rung 6's rule), but you, the author, may
// KNOW it's actually safe to send — and you take responsibility by writing
// `unsafe impl Send`. This is the manual opt-IN, the mirror of rung 6's opt-OUT.
//
// `MyBox<T>` is a minimal Box: it uniquely owns a heap T through a `*mut T`. The
// raw pointer makes the compiler refuse Send/Sync. But a uniquely-owned heap
// value is exactly as thread-safe as the T inside it — just like the real Box,
// which IS `Send if T: Send` and `Sync if T: Sync`. So we re-grant the auto
// traits, but ONLY under the matching bound on T. That bound is the safety
// contract: assert too much (e.g. `unsafe impl<T> Send` with no bound) and you
// could send a `MyBox<Rc<_>>` across threads → UB.
//
// Your turn:
//   - Implement `new`, `get`, and `Drop` (raw-pointer mechanics — each unsafe
//     block has a `// SAFETY:` slot you MUST fill in with the invariant).
//   - Write the two `unsafe impl` lines that re-grant Send and Sync, each gated
//     on the matching bound (`T: Send` for Send, `T: Sync` for Sync), each with
//     a `// SAFETY:` justification.
//
// After it's green, run `cargo miri run --bin marker_auto_traits` to check the
// raw-pointer code is UB-free (sets the miri_clean flag for this rung).
// ---------------------------------------------------------------------------

struct MyBox<T> {
    ptr: *mut T,
}

impl<T> MyBox<T> {
    fn new(value: T) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(value)),
        }
    }
    fn get(&self) -> &T {
        // SAFETY: self.ptr was created by Box::into_raw in MyBox::new, so it is
        // non-null, aligned, and points to an initialized T. This MyBox uniquely owns
        // that allocation until Drop, and &self guarantees we only create a shared &T.
        unsafe { &*self.ptr }
    }
}

impl<T> Drop for MyBox<T> {
    fn drop(&mut self) {
        // SAFETY: self.ptr was created by Box::into_raw in MyBox::new, and this MyBox
        // is the unique owner of that allocation. Drop runs at most once for this value,
        // so Box::from_raw reconstructs exactly one Box<T>, which is then immediately
        // dropped to free the allocation and drop T.
        unsafe {
            drop(Box::from_raw(self.ptr));
        }
    }
}

// TODO: re-grant the auto traits the raw pointer took away. Match real Box:
//   - Send only when T: Send,  Sync only when T: Sync.
//   - Each needs `unsafe impl` and a `// SAFETY:` line.

// SAFETY: MyBox<T> uniquely owns its heap allocation and only moves ownership
// between threads. Sending it is therefore safe exactly when the owned T is Send.
unsafe impl<T: Send> Send for MyBox<T> {}

// SAFETY: &MyBox<T> only exposes shared access to T through get(), so sharing
// references to MyBox<T> between threads is safe exactly when shared T is Sync.
unsafe impl<T: Sync> Sync for MyBox<T> {}

fn check_8() {
    // It's now Send for Send payloads:
    assert!(is_send!(MyBox<i32>));
    // ...but the bound still bites: Rc is !Send, so MyBox<Rc<_>> stays !Send.
    assert!(!is_send!(MyBox<std::rc::Rc<i32>>));

    // Local read works through get:
    let b2 = MyBox::new(String::from("heap"));
    assert_eq!(b2.get(), "heap");

    // PROOF it's really sendable — uncomment AFTER you've written the unsafe impl
    // (it won't compile until MyBox<i32> is Send):
    // let b = MyBox::new(99);
    // let handle = std::thread::spawn(move || *b.get());
    // assert_eq!(handle.join().unwrap(), 99);
    println!("check_8 ok: unsafe impl<T: Send> Send re-granted Send under the right bound");
}

// ---------------------------------------------------------------------------
// Rung 9 — CAPSTONE: a type-state machine built from markers
//
// Pull every thread of this ladder together into one real pattern: a network
// `Conn<S>` whose STATE is a type parameter. The states are zero-sized MARKER
// structs (rung 1/2), tagged onto Conn via PhantomData (rung 6/7). A SEALED
// trait `State` (private `Sealed` supertrait) is the marker that says "this ZST
// is a legal state" — and being sealed, no downstream crate can invent new
// states. Transitions CONSUME self and hand back a `Conn<NextState>`, so the old
// handle is moved away and illegal operations are COMPILE errors, not runtime
// checks. Because every state is a ZST and the payload is Send, `Conn<S>` is
// auto-Send (rung 4) — verify it with is_send!.
//
// Lifecycle:   Disconnected --connect--> Connected --authenticate--> Authenticated
//              Authenticated --logout--> Connected --disconnect--> Disconnected
//   send() exists ONLY on Conn<Authenticated>. status() works in EVERY state.
//
// What's given: the sealed trait, the State trait, the three ZST states, and the
// Conn struct (with its PhantomData<S> field — omit that and you'd get E0392
// "parameter S is never used", the typestate footgun). The State impls have
// PLACEHOLDER names "" you must fix.
//
// Your turn:
//   - Fill each `const NAME` with the state's real name ("Disconnected", etc.)
//     so status() reports correctly.
//   - Implement the constructor + every transition body. Each transition moves
//     `self.peer`/`self.log` forward into the new Conn and returns Conn<Next>.
//   - Implement `send` (only on Conn<Authenticated>): push a "SEND: {msg}" line
//     to the log and return the message's byte length.
//   - Implement the generic `status` (on Conn<S> for ALL S: State) using S::NAME.
//
// After green, prove the gates in check_9's commented block: send() on a
// Disconnected conn, or connect() on a Connected conn, must NOT compile.
// ---------------------------------------------------------------------------

mod sealed {
    pub trait Sealed {}
}

// `State` is a sealed marker: implementable only here (Sealed is private), so the
// set of legal states is closed. The const is per-state data carried by the tag.
trait State: sealed::Sealed {
    const NAME: &'static str;
}

struct Disconnected;
struct Connected;
struct Authenticated;

impl sealed::Sealed for Disconnected {}
impl sealed::Sealed for Connected {}
impl sealed::Sealed for Authenticated {}

// TODO: replace the placeholder "" names with the real state names.
impl State for Disconnected {
    const NAME: &'static str = "Disconnected";
}
impl State for Connected {
    const NAME: &'static str = "Connected";
}
impl State for Authenticated {
    const NAME: &'static str = "Authenticated";
}

// The PhantomData<S> field is what makes S "used" (avoids E0392) at zero cost.
struct Conn<S: State> {
    peer: String,
    log: Vec<String>,
    _state: PhantomData<S>,
}

impl Conn<Disconnected> {
    fn new(peer: impl Into<String>) -> Self {
        Self {
            peer: peer.into(),
            log: Vec::new(),
            _state: PhantomData,
        }
    }
    fn connect(self) -> Conn<Connected> {
        let mut log = self.log;
        log.push("CONNECT".to_string());
        Conn {
            peer: self.peer,
            log,
            _state: PhantomData,
        }
    }
}

impl Conn<Connected> {
    fn authenticate(self, token: &str) -> Conn<Authenticated> {
        let mut log = self.log;
        log.push(format!("AUTH:{token}"));
        Conn {
            peer: self.peer,
            log,
            _state: PhantomData,
        }
    }
    fn disconnect(self) -> Conn<Disconnected> {
        let mut log = self.log;
        log.push("DISCONNECT".to_string());
        Conn {
            peer: self.peer,
            log,
            _state: PhantomData,
        }
    }
}

impl Conn<Authenticated> {
    // send() exists ONLY in the Authenticated state — the type system enforces it.
    fn send(&mut self, msg: &str) -> usize {
        self.log.push(format!("SEND:{msg}"));
        msg.len()
    }
    fn logout(self) -> Conn<Connected> {
        let mut log = self.log;
        log.push("LOGOUT".to_string());
        Conn {
            peer: self.peer,
            log,
            _state: PhantomData,
        }
    }
}

// Behavior common to EVERY state: one impl bounded by the State marker.
impl<S: State> Conn<S> {
    fn status(&self) -> &'static str {
        S::NAME
    }
    fn log(&self) -> &[String] {
        &self.log
    }
}

fn check_9() {
    // Conn is auto-Send: ZST state tag + Send payload (callback to rung 4).
    assert!(is_send!(Conn<Disconnected>));
    assert!(is_send!(Conn<Authenticated>));

    let c = Conn::new("10.0.0.1:443");
    assert_eq!(c.status(), "Disconnected");

    let c = c.connect();
    assert_eq!(c.status(), "Connected");

    let mut c = c.authenticate("hunter2");
    assert_eq!(c.status(), "Authenticated");
    assert_eq!(c.send("hello"), 5);
    assert_eq!(c.send("world!"), 6);

    let c = c.logout();
    assert_eq!(c.status(), "Connected");

    let c = c.disconnect();
    assert_eq!(c.status(), "Disconnected");

    assert_eq!(
        c.log(),
        &[
            "CONNECT".to_string(),
            "AUTH:hunter2".to_string(),
            "SEND:hello".to_string(),
            "SEND:world!".to_string(),
            "LOGOUT".to_string(),
            "DISCONNECT".to_string(),
        ]
    );

    // COMPILE-TIME GATES — uncomment each to confirm the type system rejects it:
    // let bad = Conn::new("x");
    // bad.send("nope");        // ERROR: no method `send` on Conn<Disconnected>
    // let c2 = Conn::new("y").connect();
    // c2.connect();            // ERROR: no method `connect` on Conn<Connected>
    //
    // SEALED proof — a foreign state can't join the machine:
    // struct Rogue;
    // impl State for Rogue { const NAME: &'static str = "rogue"; } // ERROR: Sealed not satisfied

    println!("check_9 ok: sealed-marker type-state machine — illegal ops don't compile");
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
