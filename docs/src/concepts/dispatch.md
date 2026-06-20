# Static vs dynamic dispatch

> Ladder: [`src/bin/dispatch.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/dispatch.rs) ·
> Run: `cargo run --bin dispatch` · Phase 2 · 9 rungs

## TL;DR

When you call a trait method, *which* concrete implementation runs has to be
decided somewhere. Rust gives you two places to decide it:

- **Static dispatch** (`<T: Trait>`, `impl Trait`): the compiler knows the concrete
  type at the call site. It stamps out a specialized copy of the code per type
  (**monomorphization**) and can inline. Fast, zero indirection — but code size
  grows and the set of types is fixed at compile time.
- **Dynamic dispatch** (`dyn Trait`): the concrete type is **erased** behind a fat
  pointer `(data, vtable)`. The method is looked up at runtime through the vtable.
  One copy of the code, and it unlocks runtime flexibility (heterogeneous
  collections, types chosen by runtime values) — but each call is an indirection
  that usually can't be inlined.

Every design choice in this area is picking which side of that trade you want. And
there's a third option for closed sets — an `enum` + `match` — that gets much of
the best of both.

## Why this exists (from first principles)

A trait is a promise: "this type has a `hello()` method." But `hello()` for
`English` and `hello()` for `French` are *different functions* at different machine
addresses. When you write `g.hello()`, the generated code needs an address to jump
to. The whole topic is: **how does the compiler find that address, and when?**

Two answers:

1. **At compile time.** If the compiler can see the concrete type of `g` right
   here, it just bakes in the correct address. To make that true for generic code,
   it *duplicates* the function once per concrete type used — monomorphization.
   Calls become direct, inlinable, free.

2. **At runtime.** If the concrete type isn't known until the program runs (you
   chose it from user input, or you stuffed many different types into one list),
   the compiler can't bake an address in. Instead it attaches a **vtable** — a
   little table of function pointers — to the value, and emits "load the address
   out of the vtable, then call it." That indirection is the cost of not knowing
   the type until runtime.

Neither is "better." They solve different problems, and a lot of Rust API design is
about recognizing which problem you have.

## The ladder at a glance

| #  | Tier        | Rung               | The lesson                                                        |
|----|-------------|--------------------|------------------------------------------------------------------|
| 1  | foundations | `stamp_vs_dyn`     | Same method through `<T: Trait>` vs `&dyn Trait`                  |
| 2  | foundations | `impl_trait`       | `impl Trait` in arg position (sugar) vs return position (one type)|
| 3  | mechanics   | `monomorph_proof`  | `type_name::<T>()` proves a separate copy is stamped per type     |
| 4  | footgun     | `return_branch`    | Returning 1 of 2 types: `impl Trait` fails, `Box<dyn>` works      |
| 5  | footgun     | `hetero_collection`| `Vec<T>` can't mix types; `Vec<Box<dyn>>` can                     |
| 6  | footgun     | `returns_self`     | `-> Self`: fine under generics, forbidden behind `dyn`           |
| 7  | real-world  | `closure_pipeline` | One closure = generic `F: Fn`; many = `Vec<Box<dyn Fn>>`         |
| 8  | real-world  | `enum_dispatch`    | Closed-set third way: `enum` + `match`, inline, no vtable         |
| 9  | capstone    | `pipeline_both_ways`| Same pipeline three ways — static, dynamic, enum — one result   |

## The ideas, built up

### 1. The same method, two dispatch strategies

Start with one trait and write the identical logic twice — once generic, once `dyn`:

```rust
trait Greet {
    fn hello(&self) -> String;
}

fn greet_static<T: Greet>(g: &T) -> String { g.hello() } // monomorphized per T
fn greet_dynamic(g: &dyn Greet) -> String  { g.hello() } // one fn, vtable lookup
```

The bodies are byte-for-byte the same. The difference is invisible in the source
and lives entirely in *how the call compiles*:

- `greet_static` is generic. The compiler produces a distinct machine-code copy for
  `English` and another for `French`. Each call jumps straight to a known address.
- `greet_dynamic` is **one** function. `&dyn Greet` is a fat pointer
  `(data_ptr, vtable_ptr)`, and `g.hello()` reads the method address out of the
  vtable at runtime.

That second form is what lets you do this — pick the concrete type at runtime and
still have a single static type for the variable:

```rust
let who: &dyn Greet = if condition { &en } else { &fr };
greet_dynamic(who);
```

### 2. `impl Trait` means two different things by position

`impl Trait` is one syntax with two opposite meanings depending on where it appears:

```rust
fn loudest(g: impl Greet) -> String { g.hello().to_uppercase() } // ARGUMENT
fn default_greeter() -> impl Greet  { French }                   // RETURN
```

- **Argument position** is pure sugar for a generic bound:
  `fn loudest(g: impl Greet)` is exactly `fn loudest<T: Greet>(g: T)`. Static
  dispatch, monomorphized per call site. **The caller picks the type.**
- **Return position** means "I return one specific concrete type that I'm not
  naming." Still static — the compiler knows the real type (`French`), the caller
  just can't name it. **The callee picks**, and it must be a *single* type across
  all return paths.

That "single type" rule is quiet here but becomes a wall in rung 4.

> Scaffolding note from the file: a bare `todo!()` in a `-> impl Greet` function
> won't even compile. The inferred return type would be `!`, and `!: Greet` is
> false. Return-position `impl Trait` demands a real concrete type at compile time.

### 3. Seeing monomorphization with your own eyes

"Monomorphization" stays abstract until you prove it. This function reports the name
of its own type parameter, with **no value argument at all**:

```rust
fn tag<T>() -> &'static str { std::any::type_name::<T>() }

tag::<English>();     // ".../dispatch::English"
tag::<i32>();         // "i32"
tag::<Vec<String>>(); // "alloc::vec::Vec<alloc::string::String>"
```

If there were only one compiled `tag`, it couldn't possibly know which `T` it was
called with — it takes no runtime input. It knows because the compiler stamped a
**separate copy of `tag` per `T`**, each with its own type name baked in. That's
monomorphization made visible. A `&dyn` parameter erases the type, so a single
function literally cannot recover it this way.

### 4. The first wall: a type chosen at runtime

Now ask for one of two types based on a runtime flag:

```rust
// WRONG — does not compile:
fn broken_pick(french: bool) -> impl Greet {
    if french { French } else { English } // `if` and `else` have incompatible types
}
```

`-> impl Greet` promised *one* concrete type, but the type now depends on a runtime
value. There is no single type the compiler can fill in. Static dispatch is out of
road.

The fix is to **erase** the type behind a trait object, giving both branches the
same type — `Box<dyn Greet>`:

```rust
// OK:
fn pick_greeter(french: bool) -> Box<dyn Greet> {
    if french { Box::new(French) } else { Box::new(English) }
}
```

The cost: a heap allocation plus a vtable lookup per `.hello()`. The payoff: a type
decided at runtime. **The moment the type is a runtime decision, you reach for
`dyn`.**

### 5. Heterogeneous collections: the headline feature of `dyn`

A `Vec<T>` is monomorphic — *every* element is the exact same `T`:

```rust
// WRONG — different types in one Vec:
let bad = vec![English, French]; // expected `English`, found `French`
```

To hold a mixed bag of "things that implement `Greet`," erase each element:

```rust
// OK:
fn build_crowd() -> Vec<Box<dyn Greet>> {
    vec![Box::new(English), Box::new(French), Box::new(English)]
}
```

Now every slot has the same type — a fat pointer — even though the values underneath
differ. A list of differently-typed things behind one shared interface is simply
impossible with pure static dispatch. This is the single biggest thing dynamic
dispatch buys you.

### 6. The mirror: `-> Self` is the thing only static can do

Rung 5 showed what `dyn` can do that generics can't. Rung 6 is the reverse — a trait
generics handle fine but that *cannot* become a `dyn` at all:

```rust
trait Doubler {
    fn doubled(&self) -> Self; // returns Self -> NOT object-safe
}

fn twice<T: Doubler>(x: T) -> T { x.doubled() } // totally fine
```

```rust
// WRONG — does not compile:
let obj: Box<dyn Doubler> = Box::new(21_i32);
// "the trait `Doubler` cannot be made into an object because method `doubled`
//  references the `Self` type"
```

Why? A `dyn Doubler` erases the concrete type, but `doubled(&self) -> Self` returns
a value *of that erased type*. A vtable can't describe "returns something whose
size and layout is the type we just threw away." Under `<T: Doubler>`, the concrete
type is known at each instantiation, so `-> Self` is no problem.

> This is exactly why there is no `dyn Clone`: `clone(&self) -> Self` references
> `Self` by value. Generic methods and by-value `Self` are the other common
> object-safety blockers.

So rungs 5 and 6 bracket the trade:

| Static dispatch can…            | Dynamic dispatch can…                  |
|---------------------------------|----------------------------------------|
| Return `Self`, take `Self` by value | Store mixed types in one collection |
| Have generic methods            | Choose the concrete type at runtime    |
| Inline, monomorphize            | Keep code size flat (one copy)         |

### 7. Closures: where everyone meets this decision

Every closure has its own unique, **unnameable** type — even two closures with
identical signatures are different types. So the dispatch choice shows up the moment
you handle closures:

```rust
fn apply_static<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 { f(x) } // one closure, inlined
```

```rust
// WRONG — two closures, two different types, one Vec:
let steps = vec![|x: i32| x + 1, |x: i32| x * 2];
```

To store many closures together (a callback registry, an event table, a pipeline),
erase them:

```rust
fn build_pipeline(add: i32) -> Vec<Box<dyn Fn(i32) -> i32>> {
    vec![
        Box::new(|x| x + 1),
        Box::new(|x| x * 2),
        Box::new(move |x| x + add), // captures `add` -> distinct type again
    ]
}

fn run_pipeline(steps: &[Box<dyn Fn(i32) -> i32>], start: i32) -> i32 {
    steps.iter().fold(start, |acc, step| step(acc))
}
```

The everyday rule: **take a closure → generic `F: Fn` (fast, inlined); store a
collection of closures → `Box<dyn Fn>` (flexible, one indirection each).** It's
exactly why `Iterator::map` is generic but a vector of event handlers is boxed.

### 8. Enum dispatch: the closed-set third way

`dyn` buys heterogeneity but costs an allocation and a vtable hop per element.
Generics are free but can't store mixed types. When your set of types is **closed**
(you know all of them at compile time), an `enum` + `match` gets most of both:

```rust
enum Shape {
    Circle { r: f64 },
    Rect { w: f64, h: f64 },
}

impl Shape {
    fn area(&self) -> f64 {
        match self {
            Shape::Circle { r } => std::f64::consts::PI * r * r,
            Shape::Rect { w, h } => w * h,
        }
    }
}
```

A `Vec<Shape>` holds circles *and* rects — heterogeneous like rung 5 — but:

- **No `Box`, no heap allocation per element.** Each value lives inline in the Vec.
- **Static dispatch.** `match` compiles to a jump on the discriminant; arms can
  inline. No vtable pointer chase.
- **The trade:** the set is closed. Adding a variant means editing the enum and
  every `match` (the compiler enforces exhaustiveness — a feature here). And every
  element is sized to the *largest* variant.

The size contrast is concrete and worth internalizing:

```rust
std::mem::size_of::<Box<dyn Greet>>(); // 16 — a fat pointer (data + vtable)
std::mem::size_of::<Shape>();          // 24 — 16-byte Rect payload + discriminant,
                                       //      rounded up to 8-byte alignment
```

This is why `serde_json::Value`, AST nodes, and state machines are enums, not
`Vec<Box<dyn Node>>` — and what the `enum_dispatch` crate automates.

## Footguns

| Trap | What you see | Fix |
|------|--------------|-----|
| Return one of two types by a runtime flag | `if` and `else` have incompatible types | Erase to `Box<dyn Trait>` |
| Mixed concrete types in one `Vec<T>` | expected `A`, found `B` | `Vec<Box<dyn Trait>>` (or an enum) |
| `Box<dyn Trait>` where the trait has `-> Self` / generic method | "cannot be made into an object" | Keep it generic, or split the method behind `where Self: Sized` |
| `Vec` of two same-signature closures | different closure types | Box them as `dyn Fn`, or use one generic `F` |
| Bare `todo!()` in a `-> impl Trait` fn | `!: Trait` is not satisfied | Return a real concrete value |

## Real-world patterns

- **`Iterator::map`, `Option::map`, sort keys** take `F: FnMut(...)` — generic, so
  the closure inlines and the iterator pipeline fuses to tight code.
- **Plugin / handler registries** are `HashMap<String, Box<dyn Handler>>` or
  `Vec<Box<dyn Fn(...)>>` — the set of handlers isn't known at compile time, so the
  type must be erased.
- **`Box<dyn Error>`** is dynamic dispatch for the same reason: a function can fail
  in many ways and you want one return type.
- **`serde_json::Value`, syntax trees, VM opcodes, state machines** are enums —
  closed sets where inline storage and exhaustive `match` win.
- **Returning iterators/futures** uses `-> impl Iterator` / `-> impl Future`: static,
  no allocation, the concrete (often unnameable) type stays hidden.

A useful decision tree:

> Is the set of types **closed** and known at compile time? → **enum + match.**
> Is it **open**, or chosen at runtime, or a heterogeneous collection? → **`dyn`.**
> Is it a **single** type flowing through generic code? → **`<T>` / `impl Trait`.**

## Capstone insight

The capstone builds the *same* pipeline — `Add(3) → Mul(2) → Neg` — three ways and
proves they compute the same result (`-16` for input `5`):

```rust
// (A) STATIC: the whole pipeline is ONE type, fully inlinable, shape fixed forever.
struct Compose<A, B>(A, B);
impl<A: Transform, B: Transform> Transform for Compose<A, B> {
    fn apply(&self, x: i32) -> i32 { self.1.apply(self.0.apply(x)) }
}
fn run_static(start: i32) -> i32 {
    Compose(Add(3), Compose(Mul(2), Neg)).apply(start) // type: Compose<Add, Compose<Mul, Neg>>
}

// (B) DYNAMIC: pipeline assembled at runtime, any length/order; box + vtable per stage.
fn run_dynamic(start: i32) -> i32 {
    let pipe: Vec<Box<dyn Transform>> =
        vec![Box::new(Add(3)), Box::new(Mul(2)), Box::new(Neg)];
    pipe.iter().fold(start, |acc, t| t.apply(acc))
}

// (C) ENUM: runtime-built like (B), closed set, inline storage, match dispatch.
fn run_enum(start: i32) -> i32 {
    let pipe = vec![Op::Add(3), Op::Mul(2), Op::Neg];
    pipe.iter().fold(start, |acc, op| op.apply(acc))
}
```

The "aha" is in the static version's *type*: `Compose<Add, Compose<Mul, Neg>>`. The
entire pipeline — its stages and their order — is encoded in the type itself. That's
why the compiler can inline it end to end and allocate nothing… and also why its
shape is frozen at compile time. The dynamic and enum versions move that structure
out of the type and into runtime data (a `Vec`), trading inlinability for the
freedom to build the pipeline on the fly. Same computation, three encodings of
"where does the structure live: in the type, or in the data?"

## Explain it back

- What does monomorphization actually duplicate, and how would you *prove* it
  happened without looking at assembly?
- `impl Trait` in argument vs return position — who picks the concrete type in each,
  and what's the one-type constraint on the return form?
- Why does returning one of two types by a runtime flag force `Box<dyn>`?
- Name two things a trait can have that make it **not** object-safe, and say why a
  vtable can't express them.
- A closure captures a variable. Why does that change its type, and why does it
  matter for putting closures in a `Vec`?
- You have a fixed set of message types to dispatch on. Why might an `enum` beat both
  `Vec<Box<dyn Msg>>` and a generic? What do you give up?
- In the capstone, where does the pipeline's "structure" live in each of the three
  versions?

## See also

- [Associated types vs generic params](assoc-vs-generic.md) — the other axis of
  trait design, and where object safety (`dyn Iterator<Item=…>`) reappears.
- [Blanket impls & coherence](blanket-coherence.md) — how `impl Trait for T`
  interacts with the monomorphized world.
- [`Box` & the heap](box-heap.md) — `Box<dyn Trait>` and fat pointers up close.
