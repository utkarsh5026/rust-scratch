# `Pin` & `Unpin`

> Ladder: [`src/bin/pin_unpin.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/pin_unpin.rs) ·
> Run: `cargo run --bin pin_unpin` · Phase 5 · 9 rungs

## TL;DR

`Pin<P>` is a wrapper around a pointer `P` (like `Box<T>` or `&mut T`) that adds
exactly one promise: **the `T` behind this pointer will never move in memory
again before it is dropped.** It enforces that promise by *withholding* `&mut T`,
because `&mut T` is the capability that lets you move a value out (`mem::swap`,
`mem::replace`).

`Unpin` is an auto-trait meaning *"I don't care if I move — pinning me is
meaningless."* Almost every type is `Unpin`, and for those `Pin` is a no-op you
can freely escape. Pin only has teeth for the rare `!Unpin` type, and the
canonical `!Unpin` type is a **self-referential struct** (a field pointing into
another field). That is exactly what an `async` block compiles into, which is why
`Future::poll` takes `self: Pin<&mut Self>`.

## Why it exists (from first principles)

Rust values move constantly: returning from a function, pushing into a `Vec`,
reassigning a binding — all are byte-for-byte relocations. Normally this is
invisible and free, because Rust values are *location-independent*: nothing inside
them records their own address.

Break that assumption and everything falls apart. Rung 4 builds the smallest type
that does:

```rust
struct SelfRef {
    data: [u8; 4],   // stored INLINE inside the struct
    ptr: *const u8,  // points at &data[0] — into the struct itself
}
```

Wire it up in place, and reading through the self-pointer works:

```rust
let mut a = SelfRef::new([7, 8, 9, 10]);
a.init();                              // ptr = &a.data[0], at a's CURRENT address
assert_eq!(a.ptr_addr(), a.data_addr());
assert_eq!(a.deref_ptr(), 7);          // fine — nothing has moved
```

Now move it one line, and the pointer is poison:

```rust
let moved = a;                         // inline bytes relocate to a NEW address
assert_ne!(moved.ptr_addr(), moved.data_addr());
// moved.deref_ptr() would now read a DANGLING pointer — real UB.
```

`data` moved to a new address; `ptr` still holds the old one. This is a
use-after-free waiting to happen, and the borrow checker cannot see it — the
pointer is a raw `*const u8`, invisible to ownership analysis.

> The problem is not the raw pointer. The problem is that **the type is only valid
> while it stays at one address**, and nothing in the language stops it from
> moving. `Pin` is the missing piece: a type-level "do not move" contract.

Why does this matter beyond a toy? An `async fn` that holds a reference across an
`.await` compiles to a struct where one field borrows another — precisely
`SelfRef`'s shape. Every future you ever write is a potential `SelfRef`. Async in
Rust is *impossible* to make sound without a way to say "this value is now frozen
in place." That way is `Pin`.

## The ladder

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | Construct a pin | `Box::pin` and `Pin::new`; read the value straight through the pin |
| 2 | foundations | `Unpin` = no-op | Safe `get_mut` escapes the pin freely because moving is harmless |
| 3 | mechanics | Two doors to `&mut T` | Safe `get_mut` (Unpin only) vs unsafe `get_unchecked_mut` (any `T`) |
| 4 | footgun | The WHY | Self-referential struct + move ⇒ stale pointer (deref-after-move is UB) |
| 5 | footgun | `PhantomPinned` ⇒ `!Unpin` | `Pin::new` refuses to compile (E0277); `Box::pin` is the door |
| 6 | footgun | No safe move-out | A `!Unpin` pin has no safe `&mut` ⇒ no `mem::swap`/`replace` |
| 7 | real-world | Pin in `Future::poll` | Hand-write `poll(self: Pin<&mut Self>)`, drive it via `Box::pin` |
| 8 | real-world | Pin projection | Structural `map_unchecked_mut` vs non-structural `&mut` (what `pin-project` does) |
| 9 | capstone | Sound self-referential struct | `PhantomPinned` + `Box::pin` + wire-after-pin; moving won't compile; Miri-clean |

## The ideas, built up

### A pin is just a pointer you can still read through

There is nothing scary about the *read* side. `Pin<P>` derefs to `&T`, so you
reach the value normally:

```rust
fn pin_basics(s: String) -> usize {
    let boxed_string = Box::pin(s);    // Pin<Box<String>>
    let len = boxed_string.len();      // read through the pin — free
    let mut n = 0i32;
    let _pinned = Pin::new(&mut n);     // Pin<&mut i32>, the stack flavor
    len
}
```

Two constructors appear here, and the difference between them is the whole topic:

- `Box::pin(value)` — heap-allocates and pins; works for **any** `T`.
- `Pin::new(&mut value)` — the *safe* stack constructor; only available when
  `T: Unpin`.

What `Pin` takes away is *write* access. That is deferred to the mechanics rungs.

### `Unpin`: the opt-out that makes Pin free for everyone else

If Pin locked down every type, it would be unbearable — you could never get `&mut`
back. The escape hatch is `Unpin`, an auto-trait that almost every type
implements structurally. `Unpin` means "moving me is fine, so a pin on me
guarantees nothing and costs nothing."

For an `Unpin` type you can walk straight back out of the pin, safely:

```rust
fn unpin_escape(p: Pin<&mut i32>, delta: i32) {
    *p.get_mut() += delta;             // get_mut: &mut i32, SAFE because i32: Unpin
}
```

`get_mut` **only exists when `T: Unpin`**. That is the type system encoding the
rule: handing out `&mut T` is safe exactly when moving `T` can't hurt anything.

A neat trick from the ladder — prove `Unpin` at compile time with a witness fn:

```rust
fn assert_unpin<T: Unpin>() {}
assert_unpin::<String>();  // compiles ⇒ String is Unpin
```

`String` is `Unpin` even though it owns a heap buffer, because moving a `String`
just copies its 3-word header; the buffer's address never changes and nothing
points back at the header.

### The two doors to `&mut T`

Once you understand `Unpin`, the entire `Pin` mutation API is two methods:

| Method | Safe? | Available when | You must promise |
|--------|-------|----------------|------------------|
| `get_mut(self) -> &mut T` | safe | `T: Unpin` | nothing — the compiler already knows moving is fine |
| `get_unchecked_mut(self) -> &mut T` | **unsafe** | any `T` | not to use the `&mut` to move the value out |

Rung 3 uses both on the same (Unpin) `Widget` to feel the contrast:

```rust
fn bump_widget(mut w: Pin<&mut Widget>) {
    w.as_mut().get_mut().count += 1;   // safe door

    // SAFETY: we only mutate a field in place — we never move Widget out of
    // the pin. (Widget is also Unpin, so pinning isn't load-bearing here.)
    unsafe { w.get_unchecked_mut().label += "!"; }
}
```

Two details that trip everyone up:

- **`get_mut`/`get_unchecked_mut` take `self` by value** — they *consume* the
  `Pin`. To keep using `w`, reborrow a fresh short-lived pin with `w.as_mut()`
  before each call. Without it, the first call moves `w` away.
- **The `// SAFETY:` line is the real work.** `get_unchecked_mut` is unsafe
  because in general the returned `&mut` could be fed to `mem::swap` to move the
  value out. Your job is to state why that won't happen here.

### `PhantomPinned`: opting *out* of `Unpin` to get the guarantee

Rung 4's `SelfRef` was `Unpin` (all its fields are), so nothing stopped the fatal
move. The fix is to opt out. Add a single zero-sized `PhantomPinned` field:

```rust
struct Immovable {
    data: [u8; 4],
    ptr: *const u8,
    _pin: PhantomPinned,  // this one field makes Immovable !Unpin
}
```

`PhantomPinned` is `!Unpin`, and `Unpin` is structural, so any struct containing
it is `!Unpin` too. The instant a type is `!Unpin`, the compiler removes the safe
doors:

```rust
let mut imm = Immovable::new([9, 9, 9, 9]);
let _p = Pin::new(&mut imm);
// error[E0277]: `PhantomPinned` cannot be unpinned
//   the trait bound `PhantomPinned: Unpin` is not satisfied
```

`Pin::new` is gone because it can't prove a stack local will never move again. The
remaining safe way to pin a `!Unpin` value is `Box::pin`:

```rust
fn make_pinned(bytes: [u8; 4]) -> Pin<Box<Immovable>> {
    Box::pin(Immovable::new(bytes))   // heap = a stable address it owns forever
}
```

`Box::pin` works for any `T` because it *owns* the heap allocation: once the value
lives on the heap behind a `Pin<Box<T>>`, there is no safe path to move it out, so
the address is stable for the value's whole life.

### The guarantee in action: you cannot move it out

Rung 6 closes the loop. Once you hold a `Pin<&mut Immovable>`, there is **no safe
way back to `&mut Immovable`** — `get_mut` doesn't exist for `!Unpin`, and without
`&mut` you cannot call `mem::swap`/`mem::replace` to relocate the value:

```rust
// (a) get_mut is not available for a !Unpin type:
let _bad: &mut Immovable = pinned.as_mut().get_mut(); // E0277

// (b) and without &mut you cannot swap two pinned Immovables:
std::mem::swap(pinned.as_mut().get_mut(), other.as_mut().get_mut()); // E0277
```

To edit a field you must go through the unsafe door and promise not to move:

```rust
fn set_first_byte(p: Pin<&mut Immovable>, v: u8) {
    // SAFETY: we only write a field in place, we never move the Immovable.
    unsafe { p.get_unchecked_mut().data[0] = v; }
}
```

That is the entire mechanism: `Pin` makes the corrupting move *unrepresentable in
safe code* for exactly the types that would be corrupted by it.

### Where it all pays off: `Future::poll`

The `Future` trait's method is:

```rust
fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
```

The receiver is `Pin<&mut Self>`, never a plain `&mut Self`, because an `async`
block compiles to a self-referential state machine, and the executor must be
forbidden from moving it between polls. So a runtime pins the future once and only
ever hands `poll` a pinned reference.

The hand-written `Countdown` is `Unpin` (plain fields), so its `poll` can use the
safe door:

```rust
impl Future for Countdown {
    type Output = u32;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
        let this = self.get_mut();          // safe: Countdown is Unpin
        if this.remaining == 0 {
            Poll::Ready(this.total)
        } else {
            this.remaining -= 1;
            cx.waker().wake_by_ref();        // "poll me again"
            Poll::Pending
        }
    }
}
```

The driver shows the executor contract — **pin once, then poll forever without
moving:**

```rust
let mut boxed = Box::pin(fut);              // parked at a fixed heap address
loop {
    match boxed.as_mut().poll(&mut cx) {    // only ever .as_mut(), never by value
        Poll::Ready(v) => return v,
        Poll::Pending => continue,
    }
}
```

A real `async fn` future is `!Unpin`, so its generated `poll` uses
`get_unchecked_mut` internally — same shape, unsafe door.

## Real-world patterns

### Pin projection (what `pin-project` generates)

When a combinator holds an inner future, polling it means turning
`Pin<&mut Composite>` into `Pin<&mut Field>`. This is **projection**, and there are
two kinds — picking wrong is unsound:

```rust
struct Composite {
    child: Child,  // !Unpin field -> STRUCTURAL projection
    tag: u32,      // Unpin field  -> NON-STRUCTURAL projection
}

impl Composite {
    // STRUCTURAL: the child is pinned whenever Composite is; keep it pinned.
    fn child_pin(self: Pin<&mut Self>) -> Pin<&mut Child> {
        // SAFETY: we never move the child out and never expose a bare &mut Child.
        unsafe { self.map_unchecked_mut(|s| &mut s.child) }
    }

    // NON-STRUCTURAL: u32 is Unpin, so a bare &mut is sound.
    fn tag_mut(self: Pin<&mut Self>) -> &mut u32 {
        // SAFETY: tag is Unpin, so pinning is irrelevant to it; we don't move Composite.
        unsafe { &mut self.get_unchecked_mut().tag }
    }
}
```

| | Structural | Non-structural |
|---|-----------|----------------|
| Applies to | `!Unpin` fields you must keep pinned | `Unpin` fields that don't care |
| Projects to | `Pin<&mut Field>` | `&mut Field` |
| API | `map_unchecked_mut` | `get_unchecked_mut` then `&mut field` |
| Rule you owe | never move the field out, never hand out bare `&mut` | none — moving the field is harmless |

Choosing structural-vs-not *per field* is the entire design decision the
`pin-project` crate encodes with a macro so you don't write the `unsafe` by hand.

### `Pin<Box<T>>` as the universal "pin anything" tool

`Box::pin(value)` is the go-to whenever you have a `!Unpin` value and need it
pinned with a `'static`, owned handle — hand-rolled executors, storing a
`Pin<Box<dyn Future>>` in a collection, or any self-referential type. It trades one
heap allocation for a guaranteed-stable address.

## Capstone insight

A *sound* self-referential struct is just the recipe applied in the right order:

```rust
struct SelfReferential {
    value: String,
    ptr: *const String,   // points at &self.value once wired
    _pin: PhantomPinned,
}

fn new(text: &str) -> Pin<Box<SelfReferential>> {
    let mut boxed = Box::pin(SelfReferential {
        value: text.into(),
        ptr: std::ptr::null(),          // 1. null first
        _pin: PhantomPinned,            //    !Unpin
    });
    let self_ptr: *const String = &boxed.value; // 2. address is now FINAL (heap-pinned)
    unsafe {
        // SAFETY: we only set a field in place; never move the struct. The address
        // is permanent because the value is already pinned on the heap.
        boxed.as_mut().get_unchecked_mut().ptr = self_ptr;  // 3. wire AFTER pinning
    }
    boxed
}
```

The load-bearing insight: **you cannot set up the self-pointer in the same step
that produces the value by move.** If `new` built the struct, set `ptr = &value`,
and then returned it by value (rung 4's mistake), the return-move would invalidate
`ptr` before the caller ever saw it. The fix is to reach the value's *final*
address first (`Box::pin`), and only then wire the pointer through the unsafe door.
After that, no safe API hands out `&mut value` or moves the struct, so `ptr` can
never dangle — and moving it out is a compile error:

```rust
let moved = *s;                  // error[E0507]: cannot move out of a Pin<Box<..>> deref
```

Running `cargo miri run --bin pin_unpin` clean is the real proof: Miri exercises
every raw-pointer read and confirms no undefined behavior, no aliasing violation,
across all nine rungs.

## Footguns

- **`get_mut`/`get_unchecked_mut` consume the `Pin`.** They take `self` by value.
  Reborrow with `.as_mut()` before each call, or the pin is gone after the first
  use.
- **Adding `PhantomPinned` silently removes `Pin::new` and `get_mut`.** That is
  the *point*, but the resulting E0277 ("`PhantomPinned` cannot be unpinned") is
  confusing if you don't expect it. `Box::pin` is the door.
- **Dereferencing a self-pointer after a move is real UB**, not just a wrong
  answer. The ladder proves the hazard by comparing addresses and deliberately
  never derefs after moving.
- **Wiring a self-pointer *before* the value reaches its final address** makes it
  stale immediately. Pin (via `Box::pin`) first, wire second.
- **Choosing structural projection for an `Unpin` field, or vice versa, is
  unsound.** Structural means "I promise never to move this field out." Only
  promise that for fields that actually need to stay pinned.
- **`Pin` is a `std` library type, not a language feature.** It guarantees nothing
  on its own — its soundness rests entirely on the `unsafe` code that upholds
  "pinned means never moved." The safe API just makes that contract hard to break.

## Explain it back

- In one sentence, what does `Pin<P>` promise, and *how* does it enforce it?
- Why is almost every type `Unpin`, and why is that a good thing?
- Why does `get_mut` require `T: Unpin` but `get_unchecked_mut` does not?
- Walk through the rung-4 bug: what exactly is stale after the move, and why is
  reading it UB?
- What does adding a `PhantomPinned` field remove, and why can't you use
  `Pin::new` afterward?
- Why does `Future::poll` take `self: Pin<&mut Self>` instead of `&mut self`?
- What is the difference between structural and non-structural pin projection, and
  what does each one obligate you to promise?
- In the capstone, why must the self-pointer be wired *after* `Box::pin`, not
  inside the struct literal?

## See also

- [`Future` trait & `poll`](future-poll.md) — the state machine `Pin` protects;
  where `self: Pin<&mut Self>` comes from.
- [`Box` & the heap](box-heap.md) — `Box::pin` builds on heap allocation for a
  stable address.
- [Marker & auto traits](marker-auto-traits.md) — `Unpin` is an auto-trait;
  `PhantomPinned` opts out the same way `PhantomData<*const ()>` opts out of
  `Send`/`Sync`.
- [`Send` & `Sync` deeply](send-sync.md) — the other place `unsafe impl` and
  `PhantomData` markers steer auto-trait derivation.
