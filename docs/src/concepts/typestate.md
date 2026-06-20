# The typestate pattern

> Ladder: [`src/bin/typestate.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/typestate.rs) ·
> Run: `cargo run --bin typestate` · Phase 3 · 9 rungs

## TL;DR

**Typestate** moves a value's *state* out of its runtime fields and into its
*type*. Instead of one `Door { is_open: bool }` you check at runtime, you have
two distinct types — `Door<Open>` and `Door<Closed>` — and you write the methods
that only make sense in one state inside that state's own `impl` block. Calling
`.close()` on a `Door<Closed>` is then not a runtime error or a panic: it is a
**compile error**, because the method literally does not exist on that type.

Three mechanical pillars hold it up:

1. **State as a type parameter**, carried by a zero-sized `PhantomData<State>`
   field — so the whole scheme costs **zero bytes** at runtime.
2. **Transitions consume `self` by value** and return the new state type, so the
   old handle is *moved away* and a stale state is unusable.
3. **`impl Type<ThisState>`** gates each method to the state where it's valid.

The payoff: an entire class of "wrong order" and "wrong state" bugs becomes
unrepresentable. The cost: states must be known at compile time, so at runtime
boundaries you bridge through an `enum`.

## Why this exists (from first principles)

Start with the bug we want to delete. A connection with a runtime flag:

```rust
struct Conn { state: State, /* ... */ }
enum State { Idle, Established, Closed }

impl Conn {
    fn send(&mut self, data: &[u8]) {
        // Is this even legal right now?
        if self.state != State::Established {
            panic!("send() called on a {:?} connection", self.state); // runtime!
        }
        // ...
    }
}
```

Everything about correctness here is **deferred to runtime**:

- `send()` on a closed connection compiles fine. It only blows up when that line
  actually executes — maybe in production, maybe in a rare branch your tests miss.
- Every method has to re-check the flag, and every check is a place to forget.
- The type `Conn` claims to support `send`, `connect`, and `close` *all the
  time*, which is a lie — each is valid only in some states.

Typestate's move is to make the compiler the enforcer. If `send` only exists on
`Conn<Established>`, then code holding a `Conn<Closed>` *cannot name* `send` —
there's nothing to call, nothing to check at runtime, nothing to test. The
illegal program doesn't compile, which is the strongest guarantee Rust offers.

> The mental shift: **stop storing the state as data; start encoding it as a
> type.** A `bool` has two values you check; two types have two *vocabularies* of
> methods the compiler enforces.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | Foundations | `door_basics` | `Door<State>` with ZST markers + `PhantomData`; building a `Door<Closed>` |
| 2 | Foundations | `state_methods` | `open()` on `Closed` only, `close()` on `Open` only; the wrong call won't compile |
| 3 | Mechanics | `consuming_transitions` | `self`-by-value transitions thread a data payload through; the stale handle is moved away |
| 4 | Mechanics | `zst_and_sealed` | `size_of` proves zero cost; a **sealed** `State` trait closes the state set |
| 5 | Footgun | `phantom_required` | omit `PhantomData` ⇒ **E0392** "parameter never used"; why Rust insists |
| 6 | Footgun | `runtime_boundary` | typestate is compile-time only; erase to an `enum` and re-enter via `match` |
| 7 | Real-world | `typestate_builder` | required fields tracked in the type; `build()` exists only when complete |
| 8 | Real-world | `generic_over_state` | `impl<S: State>` + associated `const` for behavior shared by every state |
| 9 | Capstone | `protocol_capstone` | a TCP-like state machine: sealed states, typed transitions, runtime event loop |

## The ideas, built up

### 1. The state lives in the type, not a field

A state marker is just a zero-sized struct. The stateful type carries it only as
a phantom:

```rust
struct Open;    // marker — zero fields, zero bytes
struct Closed;

struct Door<State> {
    _state: PhantomData<State>,   // the ONLY "field"
}

impl Door<Closed> {
    fn new() -> Door<Closed> {
        Self { _state: PhantomData }
    }
}
```

There is no `Closed` *value* anywhere — you can't store one, there's nothing to
store. The `<Closed>` in the return type is what fixes `State = Closed`; the
`PhantomData` is the placeholder that satisfies the field. The proof it's free:

```rust
assert_eq!(std::mem::size_of::<Door<Closed>>(), 0);
```

`Door<Open>` and `Door<Closed>` are **different types** that happen to have
identical (empty) layout. That difference is invisible at runtime and total at
compile time.

### 2. Gate methods by writing them in the state's `impl`

The whole pattern is this asymmetry: a method goes in the `impl` block for the
state where it's valid.

```rust
impl Door<Closed> {
    fn open(self) -> Door<Open> { Door { _state: PhantomData } }
}

impl Door<Open> {
    fn close(self) -> Door<Closed> { Door { _state: PhantomData } }
}
```

`open` exists only on `Door<Closed>`; `close` only on `Door<Open>`. So:

```rust
let d = Door::<Closed>::new();
d.close();   // WRONG: error[E0599] no method named `close` found for `Door<Closed>`
```

That error *is* the pattern. Not a panic, not an `Err` — the program that closes
a closed door is rejected before it can run.

### 3. Transitions consume `self` — and that's the safety, not a style choice

Look at the signature: `fn open(self, ...) -> Door<Open>`. Taking `self` **by
value** means the transition *moves* the old door. After it returns, the old
handle is gone. This is what makes a *stale* state impossible to use, which
matters the moment a value carries data:

```rust
struct File<State> {
    path: String,
    buffer: Vec<u8>,
    _state: PhantomData<State>,
}

impl File<Closed> {
    fn open(self) -> File<Open> {
        File { path: self.path, buffer: self.buffer, _state: PhantomData } // MOVE the data across
    }
}

impl File<Open> {
    fn write(&mut self, bytes: &[u8]) { self.buffer.extend_from_slice(bytes); } // &mut: mutate, not transition
    fn close(self) -> (File<Closed>, usize) {
        let flushed = self.buffer.len();
        (File { path: self.path, buffer: Vec::new(), _state: PhantomData }, flushed)
    }
}
```

Two things to internalize:

- **Data is threaded by moving fields**, not cloning. You own `self`, so
  `path: self.path` moves the `String` into the new state for free. A transition
  is "same data, new type tag."
- **A consumed handle can't be revived:**

  ```rust
  let g = File::<Closed>::new("x").open();
  let _ = g.close();   // g moved here
  g.write(b"!");       // WRONG: error[E0382] use of moved value: `g`
  ```

  Use-after-close is the same compile error as use-after-free. The type system's
  move semantics are doing state-machine enforcement for free.

> Note the receiver choice encodes intent: **`self` for a transition** (you
> become a new state), **`&mut self` for an in-state mutation** (`write` keeps
> you `Open`).

### 4. Close the set of legal states with a *sealed* trait

Bare `Door<State>` lets anyone write `Door<i32>` or `Door<String>`. To say "there
are exactly these states and no others," bound the parameter with a trait — and
**seal** that trait so downstream code can't implement it:

```rust
mod door_sealed {
    trait Sealed {}                 // PRIVATE to this module
    pub trait State: Sealed {}      // public, but requires the private Sealed

    pub struct Open2;
    pub struct Closed2;

    impl Sealed for Open2 {}    impl State for Open2 {}
    impl Sealed for Closed2 {}  impl State for Closed2 {}

    pub struct Door2<S: State> {    // only real states allowed
        _state: PhantomData<S>,
    }
}
```

The mechanism: to implement the public `State`, a type must also satisfy the
supertrait `Sealed` — but `Sealed` is private to `door_sealed`, so **no code
outside this module can ever impl it.** Outsiders can *name* `State` (e.g. to
write `fn f<S: State>()`) but can never *add* a new one. Now:

```rust
let _bad: Door2<i32> = /* ... */;   // WRONG: error[E0277] the trait bound `i32: State` is not satisfied
```

This is the **sealed trait pattern**, and it's exactly how `clap`, `tokio`, and
many stdlib traits keep an "internal only" set extensible by the author but
closed to users. The compiler even warns you the seal is working:
`warning: trait Sealed is more private than the item State` — that asymmetry is
the whole point.

### 5. `impl<S: State>` for what every state shares — plus associated consts

Per-state `impl`s gate state-specific methods. For methods that make sense in
*every* state, write one generic block, and let an **associated const** carry
per-state data:

```rust
trait ConnState { const NAME: &'static str; }

struct Connecting;   impl ConnState for Connecting   { const NAME: &str = "connecting"; }
struct Connected;    impl ConnState for Connected    { const NAME: &str = "connected"; }
struct Disconnected; impl ConnState for Disconnected { const NAME: &str = "disconnected"; }

impl<S: ConnState> Conn<S> {
    fn id(&self) -> u32 { self.id }
    fn state_name(&self) -> &'static str { S::NAME }       // read the type's const
    fn reset(self) -> Conn<Disconnected> {                  // a transition valid from ANY state
        Conn { id: self.id, _s: PhantomData }
    }
}
```

`state_name` returns a string it never stored — it reads `S::NAME` off the type
parameter. **The type is the lookup table.** And `reset` is a single generic
transition usable from every state, instead of one copy per state.

## Footguns

### `PhantomData` is not optional — E0392

If you declare a state parameter `S` but no field mentions it, the compiler flatly
rejects the struct:

```rust
struct Lock<S> { held_by: String }   // WRONG
// error[E0392]: type parameter `S` is never used
// help: consider removing `S`, referring to it in a field, or using `PhantomData`
```

Why does Rust care, when `S` changes nothing about the layout? Because an unused
parameter still affects the type's **identity, variance, drop-check, and
auto-trait (`Send`/`Sync`) reasoning** — and the compiler refuses to silently
guess which meaning you intended. `PhantomData<S>` is the explicit answer: "treat
this as if it owns an `S`," at zero byte cost.

```rust
struct Lock<S> { held_by: String, _state: PhantomData<S> }   // OK
// size_of::<Lock<Unlocked>>() == size_of::<String>()  — the tag is free
```

### Typestate can't choose a state at runtime

A value's type is fixed at compile time. It **cannot** depend on a runtime `if`:

```rust
// There is no way to write this:
let valve = if config_says_open { Valve::<Open> } else { Valve::<Closed> }; // types differ — won't compile
```

When the state comes from a config file, a network byte, or user input, you must
leave the type world at that boundary. Erase the state into an `enum`:

```rust
enum AnyValve { Open(Valve<Open>), Closed(Valve<Closed>) }

impl AnyValve {
    fn parse(s: &str) -> Result<AnyValve, String> {        // ENTER from runtime data
        match s {
            "open"   => Ok(AnyValve::Open(Valve { _state: PhantomData })),
            "closed" => Ok(AnyValve::Closed(Valve { _state: PhantomData })),
            _        => Err(format!("invalid valve state: {s}")),
        }
    }
    fn state_name(&self) -> &'static str {                 // RE-ENTER: each arm is a concrete typed value
        match self { AnyValve::Open(_) => "open", AnyValve::Closed(_) => "closed" }
    }
}
```

The senior mental model is a **sandwich**: enums at the I/O edges, strong
typestate in the middle. `parse` erases runtime input into the enum; `match`
re-enters the typed core where each arm holds a concrete `Valve<Open>` /
`Valve<Closed>` and can call its real typed methods. Typestate doesn't *replace*
enums — it complements them.

## Real-world patterns

### The typestate builder: required fields enforced at compile time

The flagship application. Track "has this required field been set?" in a type
parameter per field, and implement `build()` *only* for the all-set combination:

```rust
struct Yes; struct No;

struct ReqBuilder<U, M> {       // U = url set?  M = method set?
    url: Option<String>, method: Option<String>, body: Option<String>,
    _u: PhantomData<U>, _m: PhantomData<M>,
}

impl<U, M> ReqBuilder<U, M> {
    fn url(self, url: impl Into<String>) -> ReqBuilder<Yes, M> {   // flip U, KEEP M
        ReqBuilder { url: Some(url.into()), method: self.method, body: self.body,
                     _u: PhantomData, _m: self._m }
    }
    fn method(self, m: impl Into<String>) -> ReqBuilder<U, Yes> { /* flip M, keep U */ }
}

impl ReqBuilder<Yes, Yes> {     // build() EXISTS ONLY here
    fn build(self) -> Request {
        Request { url: self.url.unwrap(), method: self.method.unwrap(), body: self.body }
    }
}
```

Two insights that make this click:

- **Setters are generic over the *other* parameter.** `url()` returns
  `ReqBuilder<Yes, M>` — it flips `U` to `Yes` but *preserves whatever `M` you
  already had*. That's why the chain works in any order: each setter touches only
  its own axis. This is the type-level mirror of "thread the data through a
  transition" from rung 3.
- **`unwrap()` in `build()` is provably infallible.** The `<Yes, Yes>` type *is*
  the proof that `url` and `method` are `Some`. This is one of the rare,
  legitimate uses of `unwrap` — the typestate discharges the panic.

And the payoff:

```rust
ReqBuilder::new().url("/x").build();
// WRONG: error[E0599] no method named `build` found for `ReqBuilder<Yes, No>`
```

A forgotten required field is a **compile error**, with no runtime validation and
no `Result`. This is what the `typed-builder` crate's derive macro generates for
you; here you've built it by hand.

## Capstone insight

The capstone wires every tool into one small TCP-like lifecycle:

```text
Idle --connect--> Handshaking --synack--> Established --close--> Closed
```

- **Sealed `Protocol` trait** with a per-state `const NAME`, generated by a tiny
  `macro_rules!` — a peek at how real crates erase the four-line
  `impl Sealed + impl Trait` boilerplate per state.
- **Typed transitions** (`connect`, `synack`, `close`) that consume `self` and
  thread `peer`/`bytes_sent` across, plus a `send(&mut self)` valid only while
  `Established`.
- **Generic accessors** (`state_name`, `peer`, `bytes_sent`) in one
  `impl<S: Protocol>` block.
- **A runtime event loop** that erases the state into `AnyConn` and drives the
  machine from strings:

```rust
pub fn step(self, event: &str) -> AnyConn {
    match self {
        AnyConn::Idle(c) => match event.split_once(':') {
            Some(("connect", peer)) => AnyConn::Handshaking(c.connect(peer)),
            _ => AnyConn::Idle(c),                    // out-of-state event: ignored
        },
        AnyConn::Handshaking(c) => match event {
            "synack" => AnyConn::Established(c.synack()),
            _ => AnyConn::Handshaking(c),
        },
        AnyConn::Established(mut c) => match event.split_once(':') {
            Some(("send", data)) => { c.send(data.as_bytes()); AnyConn::Established(c) }
            _ if event == "close" => { let (c, _) = c.close(); AnyConn::Closed(c) }
            _ => AnyConn::Established(c),
        },
        AnyConn::Closed(c) => AnyConn::Closed(c),
    }
}
```

The structural "aha": **match on the state first, the event second.** Each state's
catch-all arm (`_ => self unchanged`) handles "drop out-of-state packets" without
enumerating every bad combination — a real server silently ignores a `SYN` on an
established connection, it doesn't crash. Inside each arm you hold the concrete
typed `Conn<...>` and call its *real* typed transition: the enum is just the
runtime carrier, and the moment you `match` you're back in the strongly-typed
world. That is the typestate sandwich at full size — a statically-verified core
wrapped in a thin dynamic boundary.

The `Established(mut c)` binding is the one subtlety: `send` takes `&mut self` but
you own `c` by value, so you bind it `mut`, mutate in place, and re-wrap it in the
same `AnyConn::Established` variant.

## Explain it back

- Why is `Door<Open>` and `Door<Closed>` better than `Door { is_open: bool }`?
  What error does the bad call become, and *when*?
- Why must transitions take `self` by value? What bug does the resulting move
  prevent?
- What is `PhantomData<S>` for, and what exact error appears without it? Why does
  the compiler refuse to just ignore an unused parameter?
- How does a *sealed* trait close the set of states, and why can't a downstream
  crate add one? What's the role of the private supertrait?
- Why can't typestate pick a state from runtime input, and what's the standard
  bridge? Describe the "enum at the boundary, types in the middle" sandwich.
- In the typestate builder, why is `url()` generic over `M`? Why is the `unwrap()`
  in `build()` actually safe?

## See also

- [Builder pattern](builder.md) — rung 7 there is the same typestate-builder idea
  in its native habitat.
- [Blanket impls & coherence](blanket-coherence.md) — the sealed-trait pattern and
  why downstream impls are (or aren't) allowed.
- [Generic bounds & `where` clauses](generic-bounds.md) — conditional `impl`s and
  bounding the method, not the struct, which gates state-specific methods.
- [`Drop` & ordering](drop-ordering.md) — RAII guards, the other side of
  "the type enforces a protocol."
