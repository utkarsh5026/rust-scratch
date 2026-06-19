# `Rc<RefCell<T>>` patterns

> Ladder: [`src/bin/rc_refcell.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/rc_refcell.rs) ·
> Run: `cargo run --bin rc_refcell` · Phase 1 · 10 rungs

## TL;DR

`Rc<RefCell<T>>` is the single-threaded "shared mutable state" idiom, built by
stacking two jobs: **`Rc`** gives *many owners* (it lets the value be aliased),
and **`RefCell`** lets you *mutate through a shared `&`* by moving the borrow
check from compile time to **runtime**. The whole tension — and every footgun —
lives in that stack: `Rc` hands out N references to one cell, but `RefCell`
still allows only one `&mut` at a time, so the aliasing `Rc` enables is exactly
what makes `RefCell` panic at runtime. Reach for it only when you genuinely need
*both* shared ownership *and* mutation; the cost is runtime borrow panics and
the ever-present risk of reference-cycle leaks.

## Why it exists (from first principles)

Plain Rust ownership is a tree: one owner, borrows flow down. But some data
shapes are **graphs** — a node pointed at from two places (doubly-linked list,
DOM, a tree with parent pointers), or one piece of state several objects mutate
(an event log, an observer registry).

You can't express "two owners" with `&mut` — that's exclusive. And you can't
mutate through `Rc<T>` alone, because `Rc` only hands out `&T` (shared
references). Neither layer solves the problem on its own:

| Layer alone | What you get | What's missing |
|---|---|---|
| `Rc<T>` | Multiple owners of the same allocation | No mutation — `Rc` gives out only `&T` |
| `RefCell<T>` | Interior mutability behind a `&` | Only one owner — no way to alias the cell |

Stack them: `Rc` provides the shared-ownership topology (multiple handles to one
allocation), and `RefCell` provides the mutation through those shared handles.
The price is that the borrow check moves from compile time to runtime — two
`borrow_mut()` calls on the same cell at the same time will **panic**, not fail
to compile.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | shared cell | one `RefCell`, two `Rc` handles; mutate via A, see it via B; `Rc::ptr_eq` proves one allocation |
| 2 | foundations | shared owners | two structs each hold a clone of one `Rc<RefCell<Vec>>`; `&self` methods mutate via the cell; `strong_count` counts owners |
| 3 | mechanics | counts & cheapness | `&Rc` peeks without owning, only `.clone()` adds an owner, `borrow_mut()` reaches inside; dropping one owner keeps the value alive |
| 4 | footgun | double `borrow_mut` | two live borrows of the **same** cell -> `BorrowMutError` panic; the overlap (guard staying alive) is what triggers it |
| 5 | footgun | borrow across a call | holding a borrow while calling a method that re-borrows the same cell (reentrancy) -> panic; release the guard *before* recursing |
| 6 | footgun | the cycle leak | `a -> b -> a` with strong `Rc`s: each pins the other, `strong_count` never hits 0, **`Drop` never runs** — a silent leak |
| 7 | real-world | `Weak` + tree | down = strong (own), up = weak (observe); `Rc::downgrade` / `Weak::upgrade`; the tree frees cleanly |
| 8 | real-world | observer/subject | a `Subject` co-owns observers and fans one event out to all via `borrow_mut` in a loop |
| 9 | capstone | doubly-linked list | `next: Rc` / `prev: Weak`; push both ends, traverse forward **and** backward, drop with no leak |
| 10 | capstone+ | iterative `Drop` | the default recursive drop of an `Rc`-chained list overflows the stack on long chains; `.take()` each `next` before the node drops to tear down flat |

## The ideas, built up

### The shared-cell "aha"

The fundamental move: make one `RefCell`, wrap it in `Rc`, clone the `Rc`.
Now two handles point at the same underlying cell. A mutation through one
is immediately visible through the other — because there is no copy; both
handles dereference to the same allocation.

```rust
fn shared_cell(start: i32) -> (Rc<RefCell<i32>>, Rc<RefCell<i32>>) {
    let original = Rc::new(RefCell::new(start));
    let cloned = original.clone();
    (original, cloned)
}
```

The check proves this:

```rust
let (a, b) = shared_cell(10);
*a.borrow_mut() += 5;
assert_eq!(*b.borrow(), 15);    // b sees a's mutation
assert!(Rc::ptr_eq(&a, &b));    // same allocation
```

`Rc::ptr_eq` is the definitive test — it compares the raw pointer inside each
`Rc`, confirming they reference the same heap allocation, not just equal values.
This is what "shared ownership" means: not two copies of the data, but two
handles to one copy.

### From loose handles to owned structs

Loose locals sharing a cell is a demo. The real pattern is **separate structs**
each holding a handle to the same shared state, mutating it through `&self`
methods. The type alias makes the intent clear:

```rust
type Log = Rc<RefCell<Vec<String>>>;

struct Logger { log: Log }
struct Auditor { log: Log }
```

Both `Logger::record(&self, msg)` and `Auditor::count(&self)` take `&self` —
no `&mut self` needed, because mutation goes through the `RefCell`, not through
the Rust borrow of `self`:

```rust
impl Logger {
    fn record(&self, msg: &str) {
        self.log.borrow_mut().push(msg.to_string());
    }
}

impl Auditor {
    fn count(&self) -> usize {
        self.log.borrow().len()
    }
}
```

After two `record()` calls, the `Auditor` — a completely separate struct —
sees both entries. And `Rc::strong_count(&log)` reports 3: the original handle,
the `Logger`'s clone, and the `Auditor`'s clone. Three owners, one `Vec`.

This is why the pattern exists: it decouples ownership from mutability. Each
struct holds a shared reference to the cell; the cell enforces exclusive access
at runtime.

### `&Rc` peeks, `.clone()` owns, `borrow_mut()` reaches inside

Three distinct operations, and confusing them creates bugs:

```rust
fn peek_count(h: &Counter) -> usize {
    Rc::strong_count(h)              // reads the count, no new owner
}

fn bump(h: &Counter, n: i32) {
    *h.borrow_mut() += n;            // mutates the inner value, no new owner
}

fn make_sibling(h: &Counter) -> Counter {
    Rc::clone(h)                     // creates a new owner (bumps strong_count)
}
```

Passing `&Rc<RefCell<T>>` lets you read AND mutate the shared value without
changing the owner count. The `&Rc` auto-derefs through `Rc` to reach the
`RefCell`, and `borrow_mut()` is a `&self` method on `RefCell` — so all you
need is a shared reference to the `Rc`.

Only `Rc::clone()` (or the equivalent `.clone()` on an `Rc`) bumps
`strong_count`. The clone is cheap — it copies a pointer and increments a
counter, not the underlying data.

And dropping an owner doesn't kill the value — it decrements `strong_count`.
The value survives as long as at least one `Rc` exists:

```rust
let sib = make_sibling(&h);
assert_eq!(peek_count(&h), 2);   // two owners
drop(sib);
assert_eq!(peek_count(&h), 1);   // back to one; value still alive
assert_eq!(*h.borrow(), 115);    // the value is unaffected
```

### Footgun 1: double `borrow_mut` panics at runtime

This is the defining cost of `Rc<RefCell<T>>`. The compiler can't see that two
`Rc` handles alias the same cell, so it can't reject a double borrow at compile
time. `RefCell` re-imposes the rule at runtime: **one `&mut` XOR many `&`**,
enforced by a panic.

```rust
fn try_double_mut(x: &Counter, y: &Counter, add: i32) -> Result<(), ()> {
    let mut first = x.borrow_mut();                     // holds a &mut to the cell
    let mut second = y.try_borrow_mut().map_err(|_| ())?; // tries ANOTHER &mut
    *first += add;
    *second += add;
    Ok(())
}
```

When `x` and `y` are different cells, both borrows succeed — they're independent
`RefCell`s. When they alias (created via `Rc::clone`), the second borrow finds
the cell already mutably borrowed and fails. The non-panicking
`try_borrow_mut()` returns `Err(BorrowMutError)`; the panicking `borrow_mut()`
would crash the thread.

The check also proves the panic version directly:

```rust
let _first = h2.borrow_mut();
let _second = alias2.borrow_mut();   // BorrowMutError -> panic
```

The key detail: it's the **overlap** that triggers the panic, not the mere
existence of two handles. If you scope the first borrow so it's dropped before
the second one starts, no conflict occurs. The `RefMut` guard returned by
`borrow_mut()` tracks the borrow's lifetime — when it drops, the borrow ends.

### Footgun 2: borrow held across a call (reentrancy)

The rung-4 double borrow was obvious because both borrows were on adjacent
lines. The version that actually bites people in real code is **indirect**:
you hold a borrow, then call a function that — somewhere down the stack —
borrows the same cell again. The cell doesn't know it's "the same logical
operation"; it just sees a second borrow while the first is live, and panics.

The ladder sets up a `Bank` scenario: an `Account` can have a `backup` account
(another `Rc<RefCell<Account>>`). Withdrawal falls through to the backup if the
primary balance is insufficient. If the backup aliases the primary (a
self-referential backup), the naive implementation holds a `borrow_mut()` of the
account while recursing into the backup — which tries to `borrow_mut()` the
same cell again.

The fix: extract what you need from the cell into local variables, **drop the
guard** (by ending its scope), then recurse:

```rust
fn withdraw(acct: &Acct, amount: i32) -> Result<i32, &'static str> {
    let (shortfall, backup) = {
        let mut account = acct.borrow_mut();   // borrow starts
        if account.balance >= amount {
            account.balance -= amount;
            return Ok(account.balance);
        }
        let shortfall = amount - account.balance;
        account.balance = 0;
        (shortfall, account.backup.clone())    // clone the Rc handle out
    };                                          // borrow ENDS here

    // Now the cell is unborrowed — safe to pass to a recursive call
    let Some(backup) = backup else {
        return Err("insufficient");
    };
    if Rc::ptr_eq(acct, &backup) {
        return Err("insufficient");            // self-backup: can't double-spend
    }
    withdraw(&backup, shortfall)?;
    Ok(0)
}
```

The pattern is: **read what you need, drop the guard, then call**. The curly
braces around the borrow block are the mechanism — when the `RefMut` guard
goes out of scope, the borrow ends. The `Rc::ptr_eq` check is an additional
safety net: even after releasing the borrow, recursing into the same cell
would drain an already-zeroed balance, so the function short-circuits.

### Footgun 3: the reference cycle that never frees

The runtime borrow panic is loud — you find it fast. This footgun is **silent**:
it's a memory leak. `Rc` frees its value only when `strong_count` hits 0. If
two nodes hold strong `Rc` handles to each other, each keeps the other's count
at >=1 forever — even after every external handle is gone. Destructors never
run.

The ladder builds a `Node` with a `Drop` impl that logs into a shared `Vec`
when it dies:

```rust
impl Drop for Node {
    fn drop(&mut self) {
        self.dropped.borrow_mut().push(self.name.clone());
    }
}
```

Then it creates a cycle:

```rust
let a = make_node("a", &log);
let b = make_node("b", &log);
link(&a, &b);   // a -> b (strong)
link(&b, &a);   // b -> a (strong) — now it's a cycle
```

Each node starts with 2 strong owners: the local variable and the other node's
link. When the locals go out of scope, each count drops to 1 — but never to 0,
because the cycle holds. Neither `Node::drop` ever fires. The drop log is
empty.

This is safe — Rust prevents use-after-free and double-free, but it does
**not** prevent leaks. `Rc` cycles are the single-threaded equivalent of a
"GC-proof" leak in a garbage-collected language: the objects are unreachable
but never collected.

### Breaking cycles with `Weak`: the parent-pointer tree

The fix for the cycle leak is `Weak<T>` — a non-owning handle. `Weak` does
not increment `strong_count`, so it cannot pin a value alive. To use the
value behind a `Weak`, you must `upgrade()` it to an `Option<Rc<T>>` — and
you get `None` if the value was already dropped.

The ownership rule for avoiding cycles:

> The direction that **owns** uses `Rc` (strong).
> The direction that merely **observes** uses `Weak`.

In a tree: parent -> child is strong (the parent owns its children); child ->
parent is weak (the child can navigate up but must not keep the parent alive).

```rust
fn add_child(parent: &Tree, child: &Tree) {
    parent.borrow_mut().children.push(Rc::clone(child));  // strong down
    child.borrow_mut().parent = Rc::downgrade(parent);     // weak up
}

fn parent_value(child: &Tree) -> Option<i32> {
    child.borrow().parent.upgrade()                        // Option<Rc<...>>
        .map(|parent| parent.borrow().value)
}
```

`Rc::downgrade(&rc)` creates a `Weak` from an `Rc`. `weak.upgrade()` tries
to promote it back to an `Rc`, succeeding only if the target still has at
least one strong owner.

The counts tell the story:

```rust
assert_eq!(Rc::strong_count(&root), 1);   // only the local variable
assert_eq!(Rc::weak_count(&root), 1);     // the child's parent pointer
assert_eq!(Rc::strong_count(&leaf), 2);   // local + parent's children vec
```

The child's weak pointer to the root does **not** bump `strong_count`. When
the locals go out of scope, the root's strong count reaches 0 — it drops,
its children `Vec` drops, the leaf's strong count reaches 0 — it drops too.
Both `TreeNode::drop` implementations fire. The drop log confirms both nodes
freed.

The ladder also proves that you can **mutate the parent through the child's
back-pointer** — shared mutability across the tree, which is the whole reason
`RefCell` is in the stack:

```rust
if let Some(p) = leaf.borrow().parent.upgrade() {
    p.borrow_mut().value = 99;
}
assert_eq!(root.borrow().value, 99);
```

### Real-world pattern: observer / subject fan-out

The other canonical use: one event source ("subject") pushes updates into many
independent observers, each holding its own mutable state. The subject owns a
list of `Rc<RefCell<Observer>>` handles; calling `publish` borrows each one
mutably in a loop:

```rust
impl Subject {
    fn publish(&self, value: i32) {
        for observer in &self.observers {
            let mut observer = observer.borrow_mut();
            observer.seen += 1;
            observer.last = value;
        }
    }
}
```

The callers holding their own `Rc` handles to the same observers see the
mutations — because they're the same cells:

```rust
subject.publish(10);
subject.publish(20);
assert_eq!(a.borrow().seen, 2);    // the caller's handle sees the subject's writes
assert_eq!(a.borrow().last, 20);
```

The borrow discipline from rung 4 matters here: each `borrow_mut()` must be
scoped to one loop iteration. If you held a borrow across iterations and two
observers aliased the same cell, you'd hit the same double-borrow panic.

This is the shape behind event buses, reactive signals, and GUI data-binding
in single-threaded Rust.

### Capstone: a doubly-linked list from scratch

The structure that forces everything from this ladder together. A doubly-linked
list can't be built with plain ownership: a node is pointed at from **both**
directions (its predecessor's `next` and its successor's `prev`), so it needs
shared ownership — and you need to mutate those links after the nodes exist,
so it needs interior mutability.

The rung-7 ownership rule maps perfectly:

| Link | Direction | Ownership | Why |
|------|----------|-----------|-----|
| `next` | forward | `Rc` (strong) | the list owns its nodes going forward |
| `prev` | backward | `Weak` | backward links must not pin nodes, or every adjacent pair forms a rung-6 cycle |

```rust
struct DNode {
    value: i32,
    next: Option<DLink>,           // strong: owns the next node
    prev: Weak<RefCell<DNode>>,    // weak: observes the previous node
    dropped: IntDropLog,
}

struct List {
    head: Option<DLink>,
    tail: Option<DLink>,
    dropped: IntDropLog,
}
```

**`push_back`** appends a node. The wiring is: set the new node's `prev` to a
weak handle of the old tail, then set the old tail's `next` to a strong handle
of the new node. The borrow discipline requires care — you borrow the old tail
mutably to set its `next`, and borrow the new node mutably to set its `prev`,
but they are different cells so no conflict:

```rust
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
```

**Forward traversal** walks `head -> next -> next -> ...`:

```rust
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
```

**Backward traversal** walks `tail -> prev.upgrade() -> prev.upgrade() -> ...`,
proving the `Weak` back-links are correctly wired:

```rust
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
```

The traversal `clone()`s the `Rc` handle to advance the cursor, then borrows
the node to read its value and get the next link. The borrow ends when
`node_ref` goes out of scope at the next iteration — so no borrow overlaps.

The drop test is the proof that the whole structure works: when the `List` is
dropped, its `head` drops node 1, whose `next` drops node 2, and so on — a
cascade of strong-count-reaching-zero. No `prev` link holds anything alive
because they're all `Weak`. The drop log shows all 4 nodes freed in
front-to-back order:

```rust
assert_eq!(dropped, vec![1, 20, 3, 4], "front-to-back drop order");
```

Interior mutability works through the list too — borrowing a node handle and
mutating its value is visible via traversal:

```rust
n2.borrow_mut().value = 20;
assert_eq!(list.to_vec(), vec![1, 20, 3, 4]);
```

## Footguns

- **`Rc` defeats the compile-time aliasing check, so `RefCell` re-imposes it at
  runtime.** Two `borrow_mut()`s on the same cell (reachable via two `Rc`
  handles) panic with `already borrowed: BorrowMutError`. You traded a compile
  error for a possible panic.

- **Borrow held across a call (reentrancy).** The sneaky version: you hold a
  `borrow_mut()`, then call a method that — somewhere down the stack — borrows
  the *same* cell. The cell doesn't know it's "the same logical operation"; it
  panics. Fix: read what you need out of the cell, **drop the guard** (scope it
  in a `{ }` block or `drop(guard)`), *then* make the call.

- **Strong reference cycles leak.** `a` holds a strong `Rc` to `b` and vice
  versa -> neither count reaches 0 -> destructors never run, memory never frees.
  Safe Rust prevents use-after-free and double-free; it does **not** prevent
  leaks. Fix: make one direction `Weak`.

- **The ownership rule for back-pointers:** the direction that *owns* uses `Rc`
  (strong); the direction that merely *observes/navigates back* uses `Weak`.
  Parent -> child strong, child -> parent weak. `next` strong, `prev` weak.

- **`Rc`-chained structures recurse on Drop.** Dropping the head of a long
  `Rc`-linked list drops its `next`, which drops *its* `next`... -> stack overflow
  in the destructor for very long chains. Fix: a manual iterative `Drop` that
  pops nodes in a loop.

## Signatures to know

```rust
type Shared<T> = Rc<RefCell<T>>;

// Rc — shared ownership (no mutation of its contents)
Rc::new(v)            -> Rc<T>
Rc::clone(&rc)        -> Rc<T>     // cheap: bumps strong_count, same allocation
Rc::strong_count(&rc) -> usize
Rc::weak_count(&rc)   -> usize
Rc::ptr_eq(&a, &b)    -> bool      // same allocation?
Rc::downgrade(&rc)    -> Weak<T>   // a non-owning handle

// Weak — non-owning; doesn't keep the value alive
Weak::new()           -> Weak<T>   // points at nothing
weak.upgrade()        -> Option<Rc<T>>  // None if the target was dropped

// RefCell — interior mutability, borrow-checked at RUNTIME
cell.borrow()         -> Ref<'_, T>        // panics if a &mut is out
cell.borrow_mut()     -> RefMut<'_, T>     // panics if ANY borrow is out
cell.try_borrow_mut() -> Result<RefMut, BorrowMutError>  // non-panicking
```

## Real-world patterns

| Pattern | Shape | Example |
|---------|-------|---------|
| **Shared log / registry** | Multiple structs co-own one `Rc<RefCell<Vec>>` | Event logs, metric collectors, DI containers |
| **Observer / subject** | Subject owns `Vec<Rc<RefCell<Observer>>>`; `publish` fans out via `borrow_mut` | Event buses, reactive signals, GUI data-binding |
| **Tree with parent pointers** | Children = `Rc` (owned), parent = `Weak` (observed) | DOM trees, scene graphs, file-system models |
| **Doubly-linked list** | `next` = `Rc`, `prev` = `Weak` | Caches (LRU), undo stacks, playlist navigation |
| **Graph with back-edges** | Forward edges `Rc`, back-edges `Weak` | Dependency graphs, social graphs |

## Explain it back

- Why does `Rc<RefCell<T>>` need *both* layers — what fails if you drop either?
- Two `Rc` handles to one cell, both `borrow_mut()` at once: compile error or
  runtime panic? Why?
- You have a graph traversal that panics with `BorrowMutError` even though it
  "looks single-threaded and sequential." What's the likely cause?
- In the `withdraw` function, why must the borrow end before the recursive call?
  What happens if you remove the inner `{ }` block?
- Why does an `a <-> b` strong cycle leak, and exactly which count stays non-zero?
- In a tree with parent pointers, which link is `Rc` and which is `Weak`, and
  what breaks if you swap them?
- What does `Weak::upgrade()` return, and when is it `None`?
- In the doubly-linked list, why does the drop cascade proceed front-to-back?
  What would happen if `prev` were strong instead of `Weak`?

## See also

- [`Rc` / `Arc`](rc-arc.md) — the shared-ownership layer on its own
- [`Cell` / `RefCell`](cell-refcell.md) — the interior-mutability layer
  and the runtime borrow check
- [`Drop` & Ordering](drop-ordering.md) — why the cycle leak means
  destructors never run; iterative `Drop` for linked structures
- [`Borrow` / `ToOwned`](borrow-toowned.md) — the `MyCow` capstone
  also stacks shared-ownership with interior mutability
