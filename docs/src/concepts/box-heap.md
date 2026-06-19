# Box & the Heap

> Ladder: [`src/bin/box_heap.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/box_heap.rs) ·
> Run: `cargo run --bin box_heap` · Phase 1 · 9 rungs

## TL;DR

`Box<T>` is Rust's simplest heap allocator: a **single owning pointer** to a
value on the heap. It is exactly one pointer wide, implements `Deref<Target = T>`
so you use the inner value transparently, and drops the heap allocation when it
goes out of scope. Box exists because some values **cannot live on the stack** —
recursive types have infinite size without indirection, and trait objects need a
uniform container for heterogeneous values. When you see `Box`, think: "owned
heap value, pointer-sized, cleaned up automatically."

> **Mental model:** `Box<T>` is a `T` that lives on the heap instead of the
> stack. You reach through it with `*`, but auto-deref means you rarely have to.

## Why this exists (from first principles)

Every local variable in Rust lives on the stack, and the compiler must know its
size at compile time. This creates two hard problems:

**Problem 1 — Recursive types.** An `enum List { Cons(i32, List), Nil }` is
infinitely sized: a `List` contains a `List` contains a `List`... The compiler
needs a finite `size_of::<List>()` and can't get one. Wrapping the recursion in
`Box<List>` breaks the cycle — a `Box` is always one pointer wide, regardless of
what it points to.

**Problem 2 — Heterogeneous collections.** A `Vec<Shape>` doesn't work when
`Shape` is a trait — different implementors have different sizes. But
`Vec<Box<dyn Shape>>` does: every element is the same size (a fat pointer), and
the actual data lives on the heap at whatever size it needs.

**Problem 3 — Large values.** Moving a 10 MB array into a function copies 10 MB
on the stack. `Box::new(big_array)` puts it on the heap; moving the `Box` copies
just one pointer.

`Box` is *the* answer to "I need a value whose size isn't known at compile time"
or "I need a value that outlives this stack frame's layout constraints."

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `boxed_sum` | Box is an owning, pointer-sized handle; dereference with `*`. |
| 2 | foundations | `drop_order` | Heap value drops when its Box scope ends, in reverse declaration order. |
| 3 | mechanics | cons list | Recursive types require `Box` to get a finite size (E0072). |
| 4 | mechanics | `greet_len` / `rename` / `unbox` | `Deref`/`DerefMut` + moving the owned value out of a Box. |
| 5 | footgun | `Expr` tree | The infinite-size error — read it, understand it, fix it with Box. |
| 6 | edge cases | `into_parts` / `steal_items` | Moving out of an owned Box vs. `mem::take` through `&mut Box`. |
| 7 | real-world | `Box<dyn Shape>` | Heterogeneous trait objects; fat pointer = data ptr + vtable ptr. |
| 8 | real-world | `Box<dyn Error>` + `Box::leak` | Dynamic error unification; intentional leak for `'static` refs. |
| 9 | capstone | generic `LinkedList<T>` | Owning pointer chain from scratch: push/pop/len/iter + iterative Drop. |

## The ideas, built up

### Box basics: allocate, deref, pointer-sized

A `Box<T>` allocates `T` on the heap and gives you an owning pointer. You read
the heap value by dereferencing:

```rust
fn boxed_sum(b: Box<i64>, n: i64) -> i64 {
    *b + n   // explicit deref — but arithmetic would auto-deref too
}
```

The critical size fact: **a `Box<T>` is always exactly `size_of::<usize>()`
bytes** — one pointer — regardless of how large `T` is. A `Box<[u8; 1024]>` is
8 bytes on a 64-bit machine, same as `Box<i64>`. The heap holds the data; the
stack holds just the address.

```rust
assert_eq!(size_of::<Box<i64>>(), size_of::<usize>());
assert_eq!(size_of::<Box<[u8; 1024]>>(), size_of::<usize>());
```

### Drop timing: Box follows stack rules

When a `Box` goes out of scope, it drops the heap value — exactly like a stack
value's destructor runs at scope end. And when multiple `Box`es are declared in
sequence, they drop in **reverse declaration order** (LIFO), because the `Box`
itself is on the stack and stack drops are LIFO:

```rust
let a = make("a");   // declared first
let b = make("b");   // declared second
drop(b);             // force "b" to drop first
drop(a);             // then "a"
// log == ["b", "a"]
```

Without explicit `drop()` calls, locals drop in reverse order at scope end
anyway. The `Noisy` struct with a shared log (via `Rc<RefCell<Vec<String>>>`)
proves this observationally — the drop impl pushes the label, and you assert the
log matches. The takeaway: **heap ownership follows stack scoping rules; the Box
is on the stack, so it obeys stack destruction order.**

### Recursive types: why Box is required

This is the poster-child use case. A cons list:

```rust
// WRONG — infinite size
enum List { Cons(i32, List), Nil }

// OK — Box breaks the recursion
enum List { Cons(i32, Box<List>), Nil }
```

Without `Box`, the compiler tries to compute `size_of::<List>()` and finds that
`Cons` contains a `List` which contains a `Cons` which contains a `List`...
It's turtles all the way down — error E0072, "recursive type has infinite size."

`Box<List>` is the fix because a `Box` is one pointer regardless of its target.
Now `size_of::<List>() = max(size_of::<i32>() + size_of::<Box<List>>(), 0) + tag`,
which is finite and known. The recursion still exists at runtime — the heap chain
can be arbitrarily long — but each *type-level* nesting is just "a pointer to
more of the same."

Summing the list is straightforward pattern-matching with recursion:

```rust
fn sum_list(list: &List) -> i32 {
    match list {
        List::Nil => 0,
        List::Cons(v, rest) => v + sum_list(rest),
    }
}
```

Auto-deref means `rest` (a `&Box<List>`) coerces to `&List` when passed
recursively — no explicit `*` needed.

### Deref, DerefMut, and moving out

`Box<T>` implements `Deref<Target = T>` and `DerefMut`, which gives three
capabilities:

**Auto-deref lets you access fields directly:**

```rust
fn greet_len(p: &Box<Person>) -> usize {
    p.name.len()   // no *, no (**p).name — auto-deref handles it
}
```

**DerefMut lets you mutate through the box:**

```rust
fn rename(mut p: Box<Person>, new_name: &str) -> Box<Person> {
    p.name = new_name.to_string();   // writes straight through
    p
}
```

**`*box` on an owned Box moves the value out of the heap:**

```rust
fn unbox(p: Box<Person>) -> Person {
    *p   // consumes the Box, returns the owned Person
}
```

This last one is unique to `Box` — you can't `*` a `Rc` or `Arc` to move out,
because they might have other owners. A `Box` is *the sole owner*, so moving
out is always safe. The box is consumed, the heap is freed, and you hold the
value on the stack.

### The infinite-size error: expression trees

The cons list had one recursive field. An expression tree has **two**:

```rust
// WRONG — Add(Expr, Expr) is doubly-infinite
enum Expr { Num(i64), Add(Expr, Expr), Mul(Expr, Expr) }

// OK — box each child
enum Expr {
    Num(i64),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
}
```

The error message (E0072) tells you exactly which type is recursive and
suggests `Box`. The fix is mechanical: every recursive field gets wrapped.

Evaluating the tree is the natural recursive pattern:

```rust
fn eval(e: &Expr) -> i64 {
    match e {
        Expr::Num(n) => *n,
        Expr::Add(a, b) => eval(a) + eval(b),
        Expr::Mul(a, b) => eval(a) * eval(b),
    }
}
```

Why `Box` and not `&Expr`? A reference would need a lifetime and wouldn't *own*
the children. The tree owns its nodes — `Box` is the ownership-preserving
indirection.

### Moving out of a Box and partial moves

When you own a `Box<T>`, you can destructure the contents by moving them out:

```rust
fn into_parts(b: Box<Config>) -> (String, Vec<String>) {
    let Config { name, items } = *b;   // move all fields out, box freed
    (name, items)
}
```

But when you only have `&mut Box<Config>`, you **cannot** move a field out —
that would leave a hole in a value someone else still holds a reference to:

```rust
fn steal_items(b: &mut Box<Config>) -> Vec<String> {
    b.items          // WRONG: "cannot move out of `b.items` which is behind a mutable reference"
}
```

The fix is `std::mem::take`, which **swaps in a default value** as it takes the
old one out — no hole left behind:

```rust
fn steal_items(b: &mut Box<Config>) -> Vec<String> {
    std::mem::take(&mut b.items)   // items becomes Vec::new(), config stays valid
}
```

This is the same `mem::take` / `mem::replace` technique from the `Drop &
ordering` ladder. It shows up constantly with linked structures — any time you
need to rewire a pointer through `&mut`, you swap in a placeholder.

### Box\<dyn Trait\>: heterogeneous collections

A `Vec<Circle>` can only hold circles. A `Vec<Box<dyn Shape>>` can hold
*anything that implements Shape* — circles, squares, triangles, side by side:

```rust
trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> &'static str;
}

let shapes: Vec<Box<dyn Shape>> = vec![
    Box::new(Circle { radius: 1.0 }),
    Box::new(Square { side: 2.0 }),
];
```

Each element is a **fat pointer**: one word for the data on the heap, one word
for the vtable (the table of function pointers for that concrete type's `Shape`
impl). This is **dynamic dispatch** — `s.area()` looks up the right function
pointer at runtime.

The size difference tells the story:

```rust
assert_eq!(size_of::<Box<dyn Shape>>(), 2 * size_of::<usize>());  // fat: data + vtable
assert_eq!(size_of::<Box<Circle>>(),     size_of::<usize>());      // thin: data only
```

A concrete `Box<Circle>` is one pointer — the compiler knows the type statically.
A `Box<dyn Shape>` needs the extra vtable pointer because the type is erased.

Summing areas just iterates and calls through the vtable:

```rust
fn total_area(shapes: &[Box<dyn Shape>]) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}
```

Auto-deref means `s.area()` works directly on `&Box<dyn Shape>` — you don't
need `(**s).area()`.

### Box\<dyn Error\> and Box::leak

**`Box<dyn Error>` unifies error types.** When a function can fail in multiple
unrelated ways (parse error, IO error, custom validation), you need a single
return type. `Box<dyn std::error::Error>` is the catch-all: any type implementing
`Error` converts into it via `?`, and you can build one from a plain string with
`.into()`:

```rust
fn parse_and_double(s: &str) -> Result<i32, Box<dyn Error>> {
    let n = s.parse::<i32>()?;           // ParseIntError -> Box<dyn Error>
    if n < 0 {
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "negative number",            // custom error from string
        )))
    } else {
        Ok(n * 2)
    }
}
```

**`Box::leak` gives you `'static` from runtime data.** It consumes a `Box<T>`
and returns `&'static mut T` by deliberately *never freeing the memory* — an
intentional leak. The use case is turning runtime-built data into a reference
that lives forever (global config, interned strings, etc.):

```rust
fn leak_static(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}
```

`String::into_boxed_str()` gives a `Box<str>`, and `Box::leak` on it yields
`&'static mut str` (which coerces to `&'static str`). The heap allocation will
never be freed — that's the trade. Use it for data that genuinely must live for
the whole program, not as a lifetime escape hatch.

## Footguns

- **Infinite-size enum (E0072).** Every recursive field in an enum needs `Box`
  (or some other indirection). Two recursive fields means two `Box`es — `Vec`
  can't save you when each node has a fixed number of children.

- **Moving out through `&mut`.** You can destructure an *owned* `Box<T>`, but
  through a mutable reference you can't leave a hole. Reach for `mem::take` or
  `mem::replace` to swap in a default.

- **Recursive Drop on long chains.** A linked list where each node's `Drop`
  calls the next node's `Drop` will blow the stack on long chains. You must
  write an **iterative** `Drop` that walks and frees one node at a time.

- **`Box::leak` is permanent.** The memory is never freed. It's correct for
  process-lifetime data, but a leak in a loop is a real memory leak.

## Signatures to know

```rust
// Allocate on the heap
fn Box::new(x: T) -> Box<T>

// Transparent access — Box<T> acts like T
impl<T: ?Sized> Deref for Box<T> {
    type Target = T;
}
impl<T: ?Sized> DerefMut for Box<T> { ... }

// Move the value out of the heap (consumes the Box)
let val: T = *boxed;

// Intentional leak -> 'static reference
fn Box::leak<'a>(b: Box<T>) -> &'a mut T

// String -> Box<str> (for leak or other unsized boxing)
fn String::into_boxed_str(self) -> Box<str>
```

## Real-world patterns

### Trait objects for plugin/strategy patterns

`Box<dyn Trait>` is how Rust does runtime polymorphism. Any time you'd reach for
an interface/abstract class in another language — a logging backend, a storage
driver, a rendering strategy — you store a `Box<dyn Backend>` and dispatch
through the vtable.

### Error propagation with `?`

In application code (not libraries), `Result<T, Box<dyn Error>>` is the quick
path when you don't want to define a custom error enum. Every `?` auto-converts
the concrete error into the box. For libraries, prefer `thiserror` for typed
errors; for applications, `anyhow` wraps this pattern with better ergonomics.

### Recursive data structures

Trees, expression ASTs, and linked lists all use `Box` (or `Rc`/`Arc` for
shared ownership) to break the infinite-size cycle. The pattern is always the
same: `Option<Box<Node<T>>>` for an optional link.

## Capstone insight

Building a generic `LinkedList<T>` from scratch makes every Box idea concrete.
The link type is `Option<Box<Node<T>>>`: `Some(box)` is a node, `None` is the
end. Every operation revolves around `Option::take()` — the same `mem::take`
trick from rung 6, applied to rewire links through `&mut self`:

```rust
fn push(&mut self, elem: T) {
    let old_head = self.head.take();           // take the old head out (leaves None)
    let new_node = Box::new(Node {
        elem,
        next: old_head,                        // old head becomes next
    });
    self.head = Some(new_node);                // new node becomes head
}
```

`pop` is the mirror: take the head, set head to `node.next`, return `node.elem`.

The **iterative Drop** is the non-obvious piece. Without it, dropping a 100,000-
node list recurses 100,000 deep and overflows the stack — each `Box<Node>`
drops its `next` field, which drops *its* next, and so on. The fix is a `while
let` loop that takes each link one at a time:

```rust
impl<T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        let mut current = self.head.take();
        while let Some(mut node) = current {
            current = node.next.take();   // free one node per iteration
        }
    }
}
```

Each iteration drops one `Box<Node>` (the `Some(mut node)` binding owns it and
drops it at the end of the loop body), but crucially that node's `next` has
already been `take()`-ed out to `None`, so no recursive drop fires. The chain
unwinds iteratively, not recursively.

The iterator is the final piece: an `Iter<'a, T>` that holds
`Option<&'a Node<T>>` and advances by following `node.next.as_deref()`. It
borrows the list without consuming it, yielding `&T` references:

```rust
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        self.next.take().map(|node| {
            self.next = node.next.as_deref();
            &node.elem
        })
    }
}
```

Once you've built this, `Box` stops being "the heap pointer" and becomes a
**concrete ownership link** — a chain of sole-owner pointers you can rewire,
iterate, and drop by hand.

## Explain it back

- Why is `Box<T>` exactly one `usize` wide, no matter how big `T` is?
- What error do you get if you write `enum List { Cons(i32, List), Nil }`, and
  why does `Box<List>` fix it?
- How does `*boxed_value` differ from dereferencing an `&T`? (Hint: ownership.)
- Why can you destructure `*owned_box` but not `borrowed_box.field` through
  `&mut`?
- What is the size of `Box<dyn Shape>` vs `Box<Circle>`, and why?
- Why does the linked list need an iterative `Drop`, and what would happen
  without one on 100,000 nodes?
- What does `Box::leak` actually do, and when is it appropriate to use?

## See also

- [Cow](cow.md) — uses `Box` internally for the `Owned` variant's heap
  allocation; `into_boxed_str()` appears in the leak rung.
- [Drop & Ordering](drop-ordering.md) — `mem::take` and `mem::replace`, the
  tools that make Box-based linked structures work through `&mut`.
- [Rc\<RefCell\<T\>\> patterns](rc-refcell.md) — shared ownership via `Rc`
  where `Box`'s sole ownership isn't enough; the doubly-linked list there
  contrasts with this singly-linked capstone.
