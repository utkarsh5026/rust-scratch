// Concept: The typestate pattern — encode a state machine in the TYPE, so invalid
//          states/transitions become compile errors instead of runtime bugs.
// Run: cargo run --bin typestate
//
// Mental model:
//   Put the STATE into a type parameter: `Door<Open>` and `Door<Closed>` are
//   DIFFERENT TYPES, not one type with an `is_open: bool`. Methods that only make
//   sense in a given state live in `impl Door<Open>` / `impl Door<Closed>`, so
//   calling `.close()` on a closed door is a COMPILE error, not a runtime check.
//   A transition CONSUMES `self` and returns a value of the NEW state type — the
//   old handle is moved away, so you can't keep using a stale state. The state
//   marker is a zero-sized `PhantomData<State>`, so all of this vanishes at runtime.
//
// Ladder (DONE marks finished rungs):
//   1. door_basics          - Door<State> w/ ZST markers + PhantomData; build Closed [DONE]
//   2. state_methods        - open() only on Closed, close() only on Open            [DONE]
//   3. consuming_transitions- self-by-value transitions thread a payload through     [DONE]
//   4. zst_and_sealed       - size_of unchanged (zero-cost) + sealed State trait     [DONE]
//   5. phantom_required     - drop PhantomData -> E0392; understand & fix            [DONE]
//   6. runtime_boundary     - erase to an enum when state is runtime-chosen          [DONE]
//   7. typestate_builder    - required fields in the type; build() only when ready   [DONE]
//   8. generic_over_state   - impl<S: State> for behavior common to every state      [DONE]
//   9. protocol_capstone    - full connection state machine from scratch             [DONE]

use std::marker::PhantomData;

// ---------------------------------------------------------------------------
// Rung 1: door_basics
// State markers are zero-sized types. `Door<State>` carries the state only in a
// PhantomData field — there is no runtime `state` value at all.
// ---------------------------------------------------------------------------

struct Open;
struct Closed;

struct Door<State> {
    // The door's actual data could live here later; for now the only "field"
    // is the phantom marker that ties this Door to a particular State type.
    _state: PhantomData<State>,
}

impl Door<Closed> {
    // your turn: return a brand-new CLOSED door.
    // Hint: you can't store a `Closed` value (there's nowhere to put it) — you
    // store `PhantomData` and let the `<Closed>` in the return type do the work.
    fn new() -> Door<Closed> {
        Self {
            _state: PhantomData,
        }
    }
}

fn check_1() {
    let _closed: Door<Closed> = Door::<Closed>::new();
    // A Door is just its phantom marker, so it must be zero-sized.
    assert_eq!(std::mem::size_of::<Door<Closed>>(), 0);
    println!("rung 1 ok: built a zero-sized Door<Closed>");
}

// ---------------------------------------------------------------------------
// Rung 2: state_methods
// The whole point: a method lives in the `impl` block for the state it belongs
// to. `open()` is only meaningful on a CLOSED door, so it goes in `impl Door<Closed>`.
// `close()` only on an OPEN door, so `impl Door<Open>`. The compiler then makes
// `closed_door.close()` a type error — there is no such method on Door<Closed>.
// ---------------------------------------------------------------------------

impl Door<Closed> {
    // your turn: an open door is just the SAME door re-tagged as Open.
    // Signature is intentionally `self` by value (rung 3 explains why) — take the
    // closed door and hand back a `Door<Open>`.
    fn open(self) -> Door<Open> {
        Door {
            _state: PhantomData::<Open>,
        }
    }
}

impl Door<Open> {
    // your turn: the mirror image — consume the open door, return Door<Closed>.
    fn close(self) -> Door<Closed> {
        Door {
            _state: PhantomData::<Closed>,
        }
    }
}

fn check_2() {
    let closed = Door::<Closed>::new();
    let opened: Door<Open> = closed.open();
    let _closed_again: Door<Closed> = opened.close();

    // The lesson is what must NOT compile. Uncomment EITHER line below and run —
    // the compiler should reject it ("no method named `close` found for ...").
    // Read the error, then re-comment it so the rung passes.
    //
    //   let d = Door::<Closed>::new();
    //   d.close();        // <- closing an already-closed door: compile error
    //
    //   let d = Door::<Closed>::new().open();
    //   d.open();         // <- opening an already-open door: compile error

    println!("rung 2 ok: open()/close() are state-gated at compile time");
}

// ---------------------------------------------------------------------------
// Rung 3: consuming_transitions
// Now the door carries DATA that must survive transitions. A `File<State>` holds
// a path (always) and, when Open, a buffer of bytes. Two things to feel here:
//
//   (a) Transitions take `self` BY VALUE. That's not a style choice — moving
//       `self` means the OLD handle is consumed. After `let f = f.open()`, the
//       old closed `f` is gone; you literally cannot use a stale state. Try it.
//
//   (b) You must thread the shared data across the transition by MOVING the
//       fields out of the old state into the new one (no Clone needed).
// ---------------------------------------------------------------------------

struct File<State> {
    path: String,
    // bytes written so far while open; meaningless when closed, so it's only
    // populated in the Open state. We keep the field on both for simplicity and
    // let the type gate the METHODS that touch it.
    buffer: Vec<u8>,
    _state: PhantomData<State>,
}

impl File<Closed> {
    fn new(path: impl Into<String>) -> File<Closed> {
        File {
            path: path.into(),
            buffer: Vec::new(),
            _state: PhantomData,
        }
    }

    // your turn: open the file. Consume `self`, MOVE its data into a File<Open>.
    // The borrow checker won't let you partially move out of `self` while also
    // building a struct from it unless you take the fields out explicitly.
    fn open(self) -> File<Open> {
        File {
            path: self.path,
            buffer: self.buffer,
            _state: PhantomData::<Open>,
        }
    }
}

impl File<Open> {
    // write() is ONLY available while open. Append the bytes to the buffer.
    // Take `&mut self` here (you're mutating in place, not transitioning).
    fn write(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    // your turn: close the file, returning a File<Closed>. The buffer is
    // "flushed", so the closed file starts with an empty buffer again — but the
    // path must survive. Return how many bytes were flushed alongside the file.
    fn close(self) -> (File<Closed>, usize) {
        (
            File {
                path: self.path,
                buffer: Vec::new(),
                _state: PhantomData::<Closed>,
            },
            self.buffer.len(),
        )
    }
}

fn check_3() {
    let f = File::<Closed>::new("/tmp/log.txt");
    let mut f = f.open();
    f.write(b"hello ");
    f.write(b"world");
    let (closed, flushed) = f.close();

    assert_eq!(closed.path, "/tmp/log.txt"); // path survived both transitions
    assert_eq!(closed.buffer.len(), 0); // buffer was flushed on close
    assert_eq!(flushed, 11); // "hello world" == 11 bytes

    // Prove (a) for yourself: uncomment and watch it fail with
    //   "borrow of moved value" / "use of moved value: `g`".
    //
    //   let g = File::<Closed>::new("x").open();
    //   let _ = g.close();
    //   g.write(b"!");   // <- g was consumed by close(); stale handle is gone

    println!("rung 3 ok: data threaded through moves; stale state is unusable");
}

// ---------------------------------------------------------------------------
// Rung 4: zst_and_sealed
// Two senior-Rustacean refinements to the door:
//
//   (a) ZERO COST, proven. The two `File<State>` types differ only in a phantom
//       marker, so they must have IDENTICAL layout — same size, and the state tag
//       contributes nothing at runtime. You'll assert that.
//
//   (b) SEALED state set. Right now ANY type could be written as `Door2<i32>` or
//       `Door2<String>` — the marker `<State>` is unconstrained. We want exactly
//       two legal states and no way for outside code to add a third. The trick:
//       a `State` trait that is a SUPERTRAIT of a PRIVATE `Sealed` trait. You can
//       only impl `State` if you can impl `Sealed`, and `Sealed` is private to this
//       module — so no downstream type can ever satisfy it. Then bound the struct
//       (and constructors) with `State`.
// ---------------------------------------------------------------------------

mod door_sealed {
    use std::marker::PhantomData;

    // your turn: a PRIVATE module-internal trait. Nothing outside `door_sealed`
    // can name or impl it. (It's `pub(self)` by default — just don't `pub` it.)
    trait Sealed {}

    // your turn: the public State trait, gated behind Sealed as a supertrait.
    // `pub trait State: Sealed {}` — outsiders can SEE it but can't IMPL it,
    // because they can't impl the private `Sealed`.
    #[allow(dead_code)]
    pub trait State: Sealed {}

    pub struct Open2;
    pub struct Closed2;

    // your turn: impl BOTH `Sealed` and `State` for Open2 and Closed2.
    // (Four tiny impls. Without the Sealed impl, the State impl won't satisfy the
    //  supertrait bound.)
    impl Sealed for Open2 {}
    impl Sealed for Closed2 {}
    impl State for Open2 {}
    impl State for Closed2 {}

    // The struct now CONSTRAINS its parameter: only real states allowed.
    // your turn: add the `S: State` bound (replace the bare `<S>`).
    pub struct Door2<S: State> {
        _state: PhantomData<S>,
    }

    impl Door2<Closed2> {
        pub fn new() -> Door2<Closed2> {
            Door2 {
                _state: PhantomData,
            }
        }
    }
}

fn check_4() {
    use door_sealed::{Closed2, Door2, Open2};

    #[allow(unreachable_code)]
    {
        // (a) zero-cost: state is purely a compile-time tag.
        assert_eq!(
            std::mem::size_of::<File<Closed>>(),
            std::mem::size_of::<File<Open>>()
        );
        assert_eq!(std::mem::size_of::<Door2<Closed2>>(), 0);

        let _d = Door2::<Closed2>::new();

        // Force the compiler to confirm both legal states satisfy the bound.
        // (These require `impl State for Open2/Closed2` to exist.)
        fn assert_state<S: door_sealed::State>() {}
        assert_state::<Open2>();
        assert_state::<Closed2>();

        // The seal in action — uncomment to watch it FAIL to compile:
        //   `i32` is not a State, so the bound rejects it:
        //   let _bad: Door2<i32> = todo!();   // E0277: `i32: State` not satisfied
        //
        // And outside this file nobody can even write `impl State for MyType`,
        // because the `Sealed` supertrait is private. That's the seal.

        println!("rung 4 ok: zero-cost layout + sealed state set");
    }
}

// ---------------------------------------------------------------------------
// Rung 5: phantom_required
// THE defining footgun of typestate. You want a state parameter `S`, but if `S`
// never appears in any field, the compiler refuses the struct:
//
//   error[E0392]: type parameter `S` is never used
//   help: consider removing `S`, referring to it in a field, or using
//         `PhantomData`
//
// Why does Rust even care? Because an unused type/lifetime parameter changes
// nothing about the type's LAYOUT but DOES change its identity, variance, drop
// behavior, and auto-trait (Send/Sync) reasoning — and Rust refuses to guess
// which you meant. `PhantomData<S>` is how you say "pretend this struct owns an
// S" without storing one. (Variance is a Phase-5 topic; here just feel the rule.)
// ---------------------------------------------------------------------------

// This struct is DELIBERATELY broken right now (S is unused). Your job:
//   1. Run it once, READ the E0392 error, then come back.
//   2. Fix it by giving `Lock` a PhantomData field so `S` is "used".
//
// your turn: add the field that makes `S` used.
struct Lock<S> {
    held_by: String,
    _state: PhantomData<S>,
}

#[allow(dead_code)]
struct Locked;
struct Unlocked;

impl Lock<Unlocked> {
    fn new(owner: impl Into<String>) -> Lock<Unlocked> {
        Lock {
            held_by: owner.into(),
            _state: PhantomData,
        }
    }
}

fn check_5() {
    let l: Lock<Unlocked> = Lock::new("alice");
    assert_eq!(l.held_by, "alice");
    assert_eq!(
        std::mem::size_of::<Lock<Unlocked>>(),
        std::mem::size_of::<String>()
    );

    // Bonus understanding (no code needed): the help text lists THREE fixes —
    // remove S, use it in a real field, or PhantomData. We use PhantomData
    // because we want the type tag WITHOUT a runtime value. That's the whole
    // reason PhantomData exists.

    println!("rung 5 ok: PhantomData satisfies 'parameter never used' (E0392)");
}

// ---------------------------------------------------------------------------
// Rung 6: runtime_boundary
// The hard limit of typestate: the state must be known AT COMPILE TIME. A value's
// type can't depend on a runtime `if`. So what do you do when the state comes from
// a config file, a network byte, or user input?
//
//   Answer: typestate lives in the STATICALLY-KNOWN core; at the dynamic boundary
//   you ERASE the state into a runtime enum, and to RE-ENTER typestate you match
//   the enum and branch into the correct typed value.
//
// Below: a `Valve<Open>` / `Valve<Closed>` typestate, plus an `AnyValve` enum that
// erases the state. You implement the two-way bridge.
// ---------------------------------------------------------------------------

struct Valve<S> {
    _state: PhantomData<S>,
}

impl Valve<Closed> {
    fn new() -> Valve<Closed> {
        Valve {
            _state: PhantomData,
        }
    }
}

// The runtime-erased form: a normal enum that can hold EITHER typed valve.
enum AnyValve {
    Open(Valve<Open>),
    Closed(Valve<Closed>),
}

impl AnyValve {
    // ERASE: this is the easy direction — you HAVE a typed value, wrap it.
    // (Done for you so you can see the shape.)
    #[allow(dead_code)]
    fn from_open(v: Valve<Open>) -> AnyValve {
        AnyValve::Open(v)
    }
    fn from_closed(v: Valve<Closed>) -> AnyValve {
        AnyValve::Closed(v)
    }

    // your turn: ENTER from runtime data. Given a string like "open"/"closed"
    // (imagine it came from a config file), produce the right `AnyValve`.
    // Return an Err for anything else. THIS is where a runtime value chooses the
    // state — impossible to do purely in the type system.
    fn parse(s: &str) -> Result<AnyValve, String> {
        match s {
            "open" => Ok(AnyValve::Open(Valve {
                _state: PhantomData,
            })),
            "closed" => Ok(AnyValve::Closed(Valve {
                _state: PhantomData,
            })),
            _ => Err(format!("invalid valve state: {}", s)),
        }
    }

    // your turn: a label, by matching the enum. (Shows you've re-entered the
    // typed world: inside each arm you hold a concrete Valve<Open>/Valve<Closed>.)
    fn state_name(&self) -> &'static str {
        match self {
            AnyValve::Open(_) => "open",
            AnyValve::Closed(_) => "closed",
        }
    }
}

fn check_6() {
    // Runtime input decides the state — exactly what typestate alone can't do.
    let from_config = AnyValve::parse("open").unwrap();
    assert_eq!(from_config.state_name(), "open");

    let v2 = AnyValve::parse("closed").unwrap();
    assert_eq!(v2.state_name(), "closed");

    assert!(AnyValve::parse("banana").is_err());

    // And the erase direction still works for statically-known values:
    let typed = Valve::<Closed>::new();
    let erased = AnyValve::from_closed(typed);
    assert_eq!(erased.state_name(), "closed");

    println!("rung 6 ok: enum bridges runtime<->typestate at the boundary");
}

// ---------------------------------------------------------------------------
// Rung 7: typestate_builder
// The killer app. Two REQUIRED fields (url, method) and one optional (body). We
// track "has this required field been set yet?" IN THE TYPE with two parameters:
//   U in {No, Yes}  — url set?
//   M in {No, Yes}  — method set?
// `build()` is implemented ONLY for ReqBuilder<Yes, Yes>. Forgetting a required
// field isn't a runtime error — the method literally doesn't exist on your type.
//
// The subtle part you'll implement: each setter must FLIP ITS OWN parameter while
// PRESERVING THE OTHER. So `url()` is generic over `M` and returns <Yes, M> — it
// keeps whatever method-state you already had. Threading the *type* params is the
// mirror image of threading the *data* you did in rung 3.
// ---------------------------------------------------------------------------

struct Yes;
struct No;

struct ReqBuilder<U, M> {
    url: Option<String>,
    method: Option<String>,
    body: Option<String>, // optional: no type-state tracking needed
    _u: PhantomData<U>,
    _m: PhantomData<M>,
}

#[derive(Debug, PartialEq)]
struct Request {
    url: String,
    method: String,
    body: Option<String>,
}

impl ReqBuilder<No, No> {
    // your turn: a fresh builder with nothing set yet (both params = No).
    fn new() -> ReqBuilder<No, No> {
        ReqBuilder {
            url: None,
            method: None,
            body: None,
            _u: PhantomData,
            _m: PhantomData,
        }
    }
}

// Setters live on ANY state (you can set url whether or not method is set yet),
// so they're generic over BOTH params and flip exactly one in the return type.
impl<U, M> ReqBuilder<U, M> {
    // your turn: record the url, return a builder whose U is now `Yes`,
    // leaving M untouched. You must rebuild the struct, moving the other fields.
    fn url(self, url: impl Into<String>) -> ReqBuilder<Yes, M> {
        ReqBuilder {
            url: Some(url.into()),
            method: self.method,
            body: self.body,
            _u: PhantomData::<Yes>,
            _m: self._m,
        }
    }

    // your turn: same idea, flip M -> Yes, keep U.
    fn method(self, method: impl Into<String>) -> ReqBuilder<U, Yes> {
        ReqBuilder {
            url: self.url,
            method: Some(method.into()),
            body: self.body,
            _u: self._u,
            _m: PhantomData::<Yes>,
        }
    }

    // body is optional — it never changes the type-state, just the data.
    // (Take &mut-free consuming style for chaining; return Self.)
    fn body(self, body: impl Into<String>) -> ReqBuilder<U, M> {
        ReqBuilder {
            url: self.url,
            method: self.method,
            body: Some(body.into()),
            _u: self._u,
            _m: self._m,
        }
    }
}

// build() EXISTS ONLY when both required fields are set. This is the whole point.
impl ReqBuilder<Yes, Yes> {
    // your turn: the Options are guaranteed Some here BY THE TYPE — it's sound to
    // unwrap them. (A senior touch: this is the rare place .unwrap() is provably
    // infallible, because the typestate is the proof.)
    fn build(self) -> Request {
        Request {
            url: self.url.unwrap(),
            method: self.method.unwrap(),
            body: self.body,
        }
    }
}

fn check_7() {
    // Full chain, any order — types track completion regardless of order.
    let req = ReqBuilder::new()
        .url("https://example.com")
        .method("GET")
        .body("hi")
        .build();
    assert_eq!(
        req,
        Request {
            url: "https://example.com".into(),
            method: "GET".into(),
            body: Some("hi".into()),
        }
    );

    // Order doesn't matter; optional body can be skipped:
    let req2 = ReqBuilder::new().method("POST").url("/x").build();
    assert_eq!(req2.method, "POST");
    assert_eq!(req2.body, None);

    // THE PAYOFF — uncomment any of these and watch build() not exist:
    //
    //   ReqBuilder::new().build();                 // no url, no method
    //   ReqBuilder::new().url("/x").build();       // method missing
    //   ReqBuilder::new().method("GET").build();   // url missing
    //
    // The error is "method `build` not found for ReqBuilder<No, Yes>" etc.
    // A missing required field is a COMPILE error. No runtime validation needed.

    println!("rung 7 ok: build() exists only when all required fields are set");
}

// ---------------------------------------------------------------------------
// Rung 8: generic_over_state
// Two complementary tools to the per-state impls you've been writing:
//
//   (a) impl<S: ConnState> Conn<S> { ... } — ONE impl block covering EVERY state,
//       for behavior that exists regardless of state (here: id(), state_name()).
//
//   (b) An ASSOCIATED CONST on the state trait. Each state type carries its own
//       metadata (`NAME`), and generic code reads it as `S::NAME` — no match, no
//       runtime field. The type literally carries a compile-time string per state.
//
//   (c) A generic TRANSITION: `reset()` goes from ANY state to Disconnected,
//       written once for all S instead of one impl per state.
// ---------------------------------------------------------------------------

trait ConnState {
    // your turn: give this a per-state name. Declare it WITHOUT a default here:
    //   const NAME: &'static str;
    // (declared with a default below only so the file compiles before you do the
    //  impls — replace the default with a bare declaration once your impls set it.)
    const NAME: &'static str = "<unset>";
}

struct Connecting;
struct Connected;
struct Disconnected;

// your turn: set NAME for each state ("connecting" / "connected" / "disconnected").
impl ConnState for Connecting {
    const NAME: &'static str = "connecting";
}
impl ConnState for Connected {
    const NAME: &'static str = "connected";
}
impl ConnState for Disconnected {
    const NAME: &'static str = "disconnected";
}

struct Conn<S: ConnState> {
    id: u32,
    _s: PhantomData<S>,
}

impl Conn<Connecting> {
    fn new(id: u32) -> Conn<Connecting> {
        Conn {
            id,
            _s: PhantomData,
        }
    }
    // a state-SPECIFIC transition, for contrast with the generic one below.
    fn established(self) -> Conn<Connected> {
        Conn {
            id: self.id,
            _s: PhantomData,
        }
    }
}

// (a)+(b)+(c): everything here works for ALL states at once.
impl<S: ConnState> Conn<S> {
    // your turn: return the id (common to every state).
    fn id(&self) -> u32 {
        self.id
    }

    // your turn: read the per-state associated const. Note you never stored a
    // name — it comes from the TYPE via `S::NAME`.
    fn state_name(&self) -> &'static str {
        S::NAME
    }

    // your turn: a generic transition — from ANY state to Disconnected, keeping id.
    fn reset(self) -> Conn<Disconnected> {
        Conn {
            id: self.id,
            _s: PhantomData,
        }
    }
}

fn check_8() {
    let c = Conn::<Connecting>::new(42);
    assert_eq!(c.id(), 42);
    assert_eq!(c.state_name(), "connecting"); // read from S::NAME, not a field

    let c = c.established();
    assert_eq!(c.state_name(), "connected");
    assert_eq!(c.id(), 42); // id() available in the new state too, same impl

    // generic transition works from any state:
    let d = c.reset();
    assert_eq!(d.state_name(), "disconnected");
    assert_eq!(d.id(), 42);

    // even a freshly-connecting conn can reset (proves reset() is truly generic):
    let d2 = Conn::<Connecting>::new(7).reset();
    assert_eq!(d2.id(), 7);

    println!("rung 8 ok: one impl<S> + associated const serve every state");
}

// ---------------------------------------------------------------------------
// Rung 9 (capstone): protocol_capstone
// A small TCP-like connection lifecycle, every typestate tool combined:
//
//   Idle --connect--> Handshaking --synack--> Established --close--> Closed
//
//   * SEALED state set (rung 4): only these 4 types are protocol states.
//   * ASSOCIATED CONST per state (rung 8): each carries its NAME.
//   * DATA threaded through transitions (rung 3): peer + bytes_sent survive.
//   * Per-state methods (rung 2): send() only while Established, etc.
//   * GENERIC accessors (rung 8): peer()/state_name()/bytes_sent() for all S.
//   * RUNTIME BOUNDARY (rung 6): AnyConn enum + step(event) drives the machine
//     from runtime strings, since a real server gets events at runtime.
//
// The plumbing (sealed trait, 4 state markers w/ NAME, struct, Idle::new) is
// pre-wired — you've proven that twice. YOUR job is the machine: the typed
// transitions, the generic accessors, and the AnyConn event loop.
// ---------------------------------------------------------------------------

mod tcp {
    use std::marker::PhantomData;

    mod sealed {
        pub trait Sealed {}
    }

    pub trait Protocol: sealed::Sealed {
        const NAME: &'static str;
    }

    pub struct Idle;
    pub struct Handshaking;
    pub struct Established;
    pub struct Closed;

    macro_rules! protocol_state {
        ($t:ty, $name:literal) => {
            impl sealed::Sealed for $t {}
            impl Protocol for $t {
                const NAME: &'static str = $name;
            }
        };
    }
    protocol_state!(Idle, "idle");
    protocol_state!(Handshaking, "handshaking");
    protocol_state!(Established, "established");
    protocol_state!(Closed, "closed");

    pub struct Conn<S: Protocol> {
        peer: String,
        bytes_sent: usize,
        _s: PhantomData<S>,
    }

    impl Conn<Idle> {
        pub fn new() -> Conn<Idle> {
            Conn {
                peer: String::new(),
                bytes_sent: 0,
                _s: PhantomData,
            }
        }

        // your turn: connect to a peer. Record the peer address, move to
        // Handshaking. bytes_sent starts at 0.
        pub fn connect(self, peer: impl Into<String>) -> Conn<Handshaking> {
            Conn {
                peer: peer.into(),
                bytes_sent: 0,
                _s: PhantomData,
            }
        }
    }

    impl Conn<Handshaking> {
        // your turn: the peer ACKed our SYN — advance to Established, threading
        // peer + bytes_sent across.
        pub fn synack(self) -> Conn<Established> {
            Conn {
                peer: self.peer,
                bytes_sent: self.bytes_sent,
                _s: PhantomData,
            }
        }
    }

    impl Conn<Established> {
        // your turn: send some bytes. Only valid while Established. Mutate in
        // place (&mut self) and bump bytes_sent by data.len().
        pub fn send(&mut self, data: &[u8]) {
            self.bytes_sent += data.len();
        }

        // your turn: tear down. Established -> Closed, returning the total bytes
        // sent over the connection's life.
        pub fn close(self) -> (Conn<Closed>, usize) {
            (
                Conn {
                    peer: self.peer,
                    bytes_sent: self.bytes_sent,
                    _s: PhantomData,
                },
                self.bytes_sent,
            )
        }
    }

    // Generic over EVERY state: accessors that always make sense.
    impl<S: Protocol> Conn<S> {
        // your turn: return the current state's NAME (from the associated const).
        pub fn state_name(&self) -> &'static str {
            S::NAME
        }
        pub fn peer(&self) -> &str {
            &self.peer
        }
        pub fn bytes_sent(&self) -> usize {
            self.bytes_sent
        }
    }

    // -- Runtime boundary: erase the state so we can drive the machine from
    //    runtime events (rung 6 pattern, applied to a 4-state machine). --
    pub enum AnyConn {
        Idle(Conn<Idle>),
        Handshaking(Conn<Handshaking>),
        Established(Conn<Established>),
        Closed(Conn<Closed>),
    }

    impl AnyConn {
        pub fn state_name(&self) -> &'static str {
            match self {
                AnyConn::Idle(c) => c.state_name(),
                AnyConn::Handshaking(c) => c.state_name(),
                AnyConn::Established(c) => c.state_name(),
                AnyConn::Closed(c) => c.state_name(),
            }
        }

        pub fn bytes_sent(&self) -> usize {
            match self {
                AnyConn::Idle(c) => c.bytes_sent(),
                AnyConn::Handshaking(c) => c.bytes_sent(),
                AnyConn::Established(c) => c.bytes_sent(),
                AnyConn::Closed(c) => c.bytes_sent(),
            }
        }

        // your turn: the heart of the capstone. Given a runtime event string,
        // advance the machine and return the new erased state. Rules:
        //   "connect:<peer>"  : Idle        -> Handshaking
        //   "synack"          : Handshaking -> Established
        //   "send:<data>"     : Established  stays Established, sends the bytes
        //   "close"           : Established -> Closed
        // ANY event that doesn't match the current state is IGNORED — return self
        // unchanged (a real server drops out-of-state packets, doesn't crash).
        //
        // Hint: `match (self, event)` and parse the "key:value" with split_once(':').
        // Inside each arm you hold the concrete typed Conn and can call its real
        // typed transitions — this is where you RE-ENTER typestate from runtime data.
        pub fn step(self, event: &str) -> AnyConn {
            match self {
                Self::Idle(c) => match event.split_once(':') {
                    Some(("connect", peer)) => Self::Handshaking(c.connect(peer)),
                    _ => Self::Idle(c),
                },
                Self::Handshaking(c) => match event {
                    "synack" => Self::Established(c.synack()),
                    _ => Self::Handshaking(c),
                },
                Self::Established(mut c) => match event.split_once(':') {
                    Some(("send", data)) => {
                        c.send(data.as_bytes());
                        Self::Established(c)
                    }
                    _ if event == "close" => {
                        let (c, _) = c.close();
                        Self::Closed(c)
                    }
                    _ => Self::Established(c),
                },
                Self::Closed(c) => Self::Closed(c),
            }
        }
    }
}

fn check_9() {
    use tcp::{AnyConn, Conn, Established, Idle};

    // ---- Path 1: the fully-typed happy path (compile-time enforced order) ----
    let c = Conn::<Idle>::new();
    assert_eq!(c.state_name(), "idle");

    let c = c.connect("10.0.0.1:80");
    assert_eq!(c.state_name(), "handshaking");
    assert_eq!(c.peer(), "10.0.0.1:80");

    let mut c = c.synack();
    assert_eq!(c.state_name(), "established");
    c.send(b"GET / HTTP/1.1");
    c.send(b"\r\n\r\n");
    let (c, total) = c.close();
    assert_eq!(c.state_name(), "closed");
    assert_eq!(total, 18);
    assert_eq!(c.bytes_sent(), 18); // generic accessor still works post-teardown

    // What MUST NOT compile (the guarantee). Uncomment to confirm:
    //   Conn::<Idle>::new().synack();      // can't synack before connecting
    //   Conn::<Idle>::new().send(b"x");    // can't send while Idle
    //   let mut z = Conn::<Idle>::new().connect("x").synack();
    //   z.close(); z.send(b"y");           // can't send after close (z moved)

    // ---- Path 2: drive the SAME machine from a runtime event stream ----
    let events = [
        "connect:1.2.3.4",
        "synack",
        "send:hello",
        "send:world",
        "close",
    ];
    let mut conn = AnyConn::Idle(Conn::<Idle>::new());
    for ev in events {
        conn = conn.step(ev);
    }
    assert_eq!(conn.state_name(), "closed");
    assert_eq!(conn.bytes_sent(), 10); // "hello"+"world"

    // out-of-state events are ignored, not fatal:
    let mut conn = AnyConn::Idle(Conn::<Idle>::new());
    conn = conn.step("synack"); // invalid from Idle -> ignored
    assert_eq!(conn.state_name(), "idle");
    conn = conn.step("send:nope"); // invalid from Idle -> ignored
    assert_eq!(conn.state_name(), "idle");
    conn = conn.step("connect:host"); // valid
    assert_eq!(conn.state_name(), "handshaking");

    // keep the `Established` import meaningful for readers of the typed path:
    fn _typed_only(_c: &Conn<Established>) {}

    println!("rung 9 ok: full typestate protocol — typed core + runtime-driven boundary");
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
