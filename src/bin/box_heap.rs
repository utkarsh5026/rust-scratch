// Concept: Box<T> & the heap — std::boxed::Box
//
// All problems for this concept live in THIS file. Each problem is one function
// plus a `check_N` that asserts it. `main` runs them in order and stops at the
// first unimplemented one (todo! panic).
//
// Run with: cargo run --bin box_heap
// (the bin is `box_heap` because `box` is a reserved keyword)
//
// Ladder:
//   1. Box basics: allocate, deref, pointer-sized   (DONE)
//   2. Box vs stack & Drop timing                   (DONE)
//   3. recursive types (cons list) — why Box is required  (DONE)
//   4. Box::new vs Deref/*, moving the value out     (DONE)
//   5. the infinite-size error (footgun)             (DONE)
//   6. moving out of a Box & partial moves           (DONE)
//   7. Box<dyn Trait> — heterogeneous trait objects  (DONE)
//   8. Box<dyn Error> & Box::leak for 'static        (DONE)
//   9. capstone: generic singly-linked List<T> from scratch  (DONE)

// ---------------------------------------------------------------------------
// Problem 1 — Box basics
//
// Task: implement `boxed_sum`. Take ownership of a Box<i64> and a plain i64,
// and return their sum (i64). You must read the heap value THROUGH the box.
//
// Goal: feel that a Box<T> is an owning pointer — you reach the T by
// dereferencing (`*b`), and arithmetic/auto-deref does the rest.
// ---------------------------------------------------------------------------
fn boxed_sum(b: Box<i64>, n: i64) -> i64 {
    *b + n
}

fn check_1() {
    let b = Box::new(40);
    assert_eq!(boxed_sum(b, 2), 42);

    // A Box<T> is just one pointer wide, no matter how big T is.
    use std::mem::size_of;
    assert_eq!(size_of::<Box<i64>>(), size_of::<usize>());
    assert_eq!(size_of::<Box<[u8; 1024]>>(), size_of::<usize>());

    println!("✅ problem 1: box is an owning, pointer-sized handle to the heap");
}

// ---------------------------------------------------------------------------
// Problem 2 — Box vs stack & Drop timing
//
// A `Noisy` pushes its label into a shared log when it is dropped. We use that
// log to PROVE two things about Box ownership:
//   (a) the heap value is dropped exactly when its owning Box goes out of scope,
//   (b) dropping order is the reverse of declaration order, same as stack values
//       — the Box is on the stack, so it follows stack drop rules.
//
// Task: implement `drop_order`. Given two labels, build them so that the LOG
// ends up as ["b", "a"]:
//   - create a Noisy("a") owned by a Box,
//   - create a Noisy("b") owned by a Box,
//   - then force "b" to drop FIRST, "a" SECOND, and return the log's contents.
//
// You may use `std::mem::drop` to drop a value early. Think about which order
// gets you ["b", "a"]. Return the recorded log as a Vec<String>.
// ---------------------------------------------------------------------------
use std::cell::RefCell;
use std::rc::Rc;

struct Noisy {
    label: String,
    log: Rc<RefCell<Vec<String>>>,
}

impl Drop for Noisy {
    fn drop(&mut self) {
        self.log.borrow_mut().push(self.label.clone());
    }
}

fn drop_order() -> Vec<String> {
    let log: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    // Helper so you don't repeat yourself.
    let make = |label: &str| {
        Box::new(Noisy {
            label: label.to_string(),
            log: Rc::clone(&log),
        })
    };

    let a = make("a");
    let b = make("b");
    drop(b);
    drop(a);
    log.borrow().to_vec()
}

fn check_2() {
    assert_eq!(drop_order(), vec!["b".to_string(), "a".to_string()]);
    println!("✅ problem 2: heap value drops when its Box scope ends, in reverse order");
}

// ---------------------------------------------------------------------------
// Problem 3 — recursive types (a cons list) — why Box is REQUIRED
//
// A cons list is either Nil, or a value Cons'd onto another list:
//     Cons(1, Cons(2, Cons(3, Nil)))
//
// The trap: `enum List { Cons(i32, List), Nil }` does NOT compile — a List
// would contain a List would contain a List... infinite size. The compiler
// must know the size of every type at compile time. Box<List> breaks the
// recursion: a Box is ONE pointer wide regardless of what it points to, so
// List now has a finite, known size.
//
// The enum is already correct below: the tail is Box<List>, NOT List. That one
// Box is the whole trick — it makes List finite-sized. (In rung 5 you'll remove
// it and read the exact compiler error this prevents.)
//
// Task: implement `sum_list` to add up every i32 in the list.
//
// Hint: match on the list; for Cons pull out the value and the rest; recurse on
// the rest (it's a &Box<List> — auto-deref lets you pass it where &List is wanted).
// ---------------------------------------------------------------------------
enum List {
    Cons(i32, Box<List>),
    Nil,
}

fn sum_list(list: &List) -> i32 {
    match list {
        List::Nil => 0,
        List::Cons(v, rest) => v + sum_list(rest),
    }
}

fn check_3() {
    use List::*;
    let list = Cons(1, Box::new(Cons(2, Box::new(Cons(3, Box::new(Nil))))));
    assert_eq!(sum_list(&list), 6);
    println!("✅ problem 3: Box gives a recursive type a finite size");
}

// ---------------------------------------------------------------------------
// Problem 4 — Box::new vs Deref/*, and moving the value out
//
// Box<T> implements Deref<Target = T> and DerefMut. This is why you can:
//   - call T's methods directly on a Box<T> (auto-deref),
//   - read/write the inner value with *box,
//   - and MOVE the owned T out of the box with `*box` (this consumes the box).
//
// `Person` is NOT Copy and NOT Clone on purpose — so the only way to return an
// OWNED Person from a Box<Person> is to MOVE it out, not copy it.
//
// Task: implement the three functions:
//   - `greet_len`: take &Box<Person> and return the length of its name. Reach
//     the field through the box WITHOUT writing `*` (auto-deref does it).
//   - `rename`:    take a Box<Person> and a new name; mutate the name in place
//     via the box, then return the (still boxed) Person.
//   - `unbox`:     take a Box<Person> and return an OWNED Person (move it out of
//     the heap). Exactly one small expression.
// ---------------------------------------------------------------------------
struct Person {
    name: String,
}

fn greet_len(p: &Box<Person>) -> usize {
    p.name.len()
}

fn rename(mut p: Box<Person>, new_name: &str) -> Box<Person> {
    p.name = new_name.to_string();
    p
}

fn unbox(p: Box<Person>) -> Person {
    *p
}

fn check_4() {
    let p = Box::new(Person {
        name: "Ada".to_string(),
    });
    assert_eq!(greet_len(&p), 3);

    let p = rename(p, "Grace");
    assert_eq!(greet_len(&p), 5);

    let owned: Person = unbox(p); // now a plain stack Person, box is gone
    assert_eq!(owned.name, "Grace");

    println!("✅ problem 4: Deref/DerefMut + moving the owned value out of a Box");
}

// ---------------------------------------------------------------------------
// Problem 5 — the infinite-size error (footgun) — THIS FILE WON'T COMPILE YET
//
// `Expr` is an arithmetic expression tree: a number, or two sub-expressions
// added/multiplied. As written below it has TWO recursive fields and NO
// indirection — so it has infinite size and the compiler REJECTS it.
//
// Step 1: run `cargo run --bin box_heap` and READ the error. You should see
//         E0072 "recursive type `Expr` has infinite size". Make sure you can
//         say WHY in one sentence (what would the size of Add(Expr, Expr) be?).
//
// Step 2: fix `Expr` so it compiles, using the minimal heap indirection. Note
//         Vec can't save you here the way it could for a flat list — each Add
//         /Mul has exactly two children, so box each child: Box<Expr>.
//
// Step 3: implement `eval` to compute the expression's value.
//
// (Why not &Expr instead of Box<Expr>? A reference would need a lifetime and
//  wouldn't OWN the children — the tree owns its nodes, so Box is the fit.)
// ---------------------------------------------------------------------------
enum Expr {
    Num(i64),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
}

fn eval(e: &Expr) -> i64 {
    match e {
        Expr::Num(n) => *n,
        Expr::Add(a, b) => eval(a) + eval(b),
        Expr::Mul(a, b) => eval(a) * eval(b),
    }
}

fn check_5() {
    use Expr::*;
    // (2 + 3) * 4  == 20
    let expr = Mul(
        Box::new(Add(Box::new(Num(2)), Box::new(Num(3)))),
        Box::new(Num(4)),
    );
    assert_eq!(eval(&expr), 20);
    println!("✅ problem 5: added Box to break the infinite-size cycle, eval works");
}

// ---------------------------------------------------------------------------
// Problem 6 — moving out of a Box & partial moves
//
// `Config` has two non-Copy fields. Two scenarios:
//
//  (a) You OWN the box. You can destructure the whole thing by moving it out of
//      the heap and binding the fields:  let Config { name, items } = *b;
//      That consumes the box and hands you the owned fields.
//
//  (b) You only have a &mut Box<Config> (you must leave the config usable). Now
//      the borrow checker BITES: `let v = b.items;` fails with
//      "cannot move out of `b.items` which is behind a mutable reference".
//      You can't leave a hole where `items` was. The escape hatch is to swap a
//      value IN as you take the old one OUT: std::mem::take / std::mem::replace.
//
// Task:
//   - `into_parts(b: Box<Config>) -> (String, Vec<String>)`: destructure-move.
//   - `steal_items(b: &mut Box<Config>) -> Vec<String>`: take `items` out,
//     leaving an empty Vec behind, config still valid. (Try the naive
//     `b.items` first to SEE the error, then reach for std::mem.)
// ---------------------------------------------------------------------------
struct Config {
    name: String,
    items: Vec<String>,
}

fn into_parts(b: Box<Config>) -> (String, Vec<String>) {
    let Config { name, items } = *b;
    (name, items)
}

fn steal_items(b: &mut Box<Config>) -> Vec<String> {
    std::mem::take(&mut b.items)
}

fn check_6() {
    let mut b = Box::new(Config {
        name: "app".to_string(),
        items: vec!["a".to_string(), "b".to_string()],
    });

    let stolen = steal_items(&mut b);
    assert_eq!(stolen, vec!["a".to_string(), "b".to_string()]);
    assert!(b.items.is_empty()); // hole was filled with an empty Vec
    assert_eq!(b.name, "app"); // config still fully usable

    let (name, items) = into_parts(b);
    assert_eq!(name, "app");
    assert!(items.is_empty());

    println!("✅ problem 6: destructure-move out of an owned Box; mem::take through &mut");
}

// ---------------------------------------------------------------------------
// Problem 7 — Box<dyn Trait>: heterogeneous trait objects
//
// A Vec<Circle> can only hold Circles. But a Vec<Box<dyn Shape>> can hold ANY
// type that implements Shape, side by side — the canonical use of Box. Each
// element is a "fat pointer": one word to the data on the heap + one word to
// the type's vtable (so `area()` dispatches to the right impl at runtime).
//
// Task:
//   - impl Shape for Circle and Square (area + name).
//   - implement `total_area(shapes: &[Box<dyn Shape>]) -> f64`: sum every area.
//     (Auto-deref lets you call `s.area()` straight on a &Box<dyn Shape>.)
//
// The check also proves Box<dyn Shape> is TWO words (fat) while Box<Circle> is
// ONE word (thin) — dynamic dispatch costs you the extra vtable pointer.
// ---------------------------------------------------------------------------
trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> &'static str;
}

struct Circle {
    radius: f64,
}
struct Square {
    side: f64,
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.radius * self.radius
    }
    fn name(&self) -> &'static str {
        "circle"
    }
}

impl Shape for Square {
    fn area(&self) -> f64 {
        self.side * self.side
    }
    fn name(&self) -> &'static str {
        "square"
    }
}

fn total_area(shapes: &[Box<dyn Shape>]) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}

fn check_7() {
    use std::mem::size_of;

    let shapes: Vec<Box<dyn Shape>> = vec![
        Box::new(Circle { radius: 1.0 }),
        Box::new(Square { side: 2.0 }),
    ];
    assert_eq!(shapes[0].name(), "circle");
    assert_eq!(shapes[1].name(), "square");

    let expected = std::f64::consts::PI * 1.0 + 4.0;
    assert!((total_area(&shapes) - expected).abs() < 1e-9);

    // A trait-object box is FAT (data ptr + vtable ptr); a concrete box is thin.
    assert_eq!(size_of::<Box<dyn Shape>>(), 2 * size_of::<usize>());
    assert_eq!(size_of::<Box<Circle>>(), size_of::<usize>());

    println!("✅ problem 7: Box<dyn Trait> = heterogeneous collection + fat pointer");
}

// ---------------------------------------------------------------------------
// Problem 8 — Box<dyn Error> (the idiomatic dynamic error) & Box::leak
//
// (a) Box<dyn Error>: a function that can fail in MULTIPLE unrelated ways wants
//     a single error type. `Box<dyn std::error::Error>` is it — any concrete
//     error converts into it, so `?` "just works" across error kinds, and you
//     can also build one from a string with `.into()`.
//
//     `parse_and_double(s)`:
//       - parse s as i32 with `?`  (a ParseIntError auto-converts into the box),
//       - if the number is negative, return an error built from a &str message
//         containing the word "negative"  (use `"...".into()` or `Err(...)`),
//       - otherwise return Ok(n * 2).
//
// (b) Box::leak: consumes a Box and returns a plain &mut to the heap value with
//     ANY lifetime you want — including 'static — by deliberately NEVER freeing
//     it (an intentional leak). Used to turn runtime-built data into a 'static
//     reference (config, interned strings, etc.).
//
//     `leak_static(s)`: take an owned String and return a &'static str backed by
//     that heap allocation.  Hint: `String::into_boxed_str()` gives a Box<str>,
//     and `Box::leak(...)` on it yields a &'static mut str (coerces to &str).
// ---------------------------------------------------------------------------
use std::error::Error;

fn parse_and_double(s: &str) -> Result<i32, Box<dyn Error>> {
    let n = s.parse::<i32>()?;
    if n < 0 {
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "negative number",
        )))
    } else {
        Ok(n * 2)
    }
}

fn leak_static(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn check_8() {
    assert_eq!(parse_and_double("21").unwrap(), 42);
    assert!(parse_and_double("abc").is_err()); // ParseIntError path
    let neg = parse_and_double("-5").unwrap_err();
    assert!(neg.to_string().contains("negative")); // custom string path

    let s: &'static str = leak_static(format!("hi-{}", 1 + 1));
    assert_eq!(s, "hi-2");

    println!("✅ problem 8: Box<dyn Error> unifies error kinds; Box::leak gives 'static");
}

// ---------------------------------------------------------------------------
// Problem 9 — CAPSTONE: a generic singly-linked List<T> from scratch
//
// This is the classic "owning pointer chain". Each node owns the next one:
//     head -> Box(Node{elem, next}) -> Box(Node{...}) -> None
// The link type is Option<Box<Node<T>>>: Some(box) is a node, None is the end.
// (Box can't be null, so the "no next node" case is None, not a null pointer.)
//
// The KEY tool you'll lean on is Option::take(): it swaps the Option out for
// None and hands you the old value — exactly the mem::take trick from rung 6,
// and the only sane way to rewire links through &mut self.
//
// Implement every `todo!`:
//   - new()                    -> empty list (head: None)
//   - push(&mut self, elem)    -> new node whose `next` is the OLD head; takes
//                                 the old head with self.head.take()
//   - pop(&mut self) -> Option<T>  -> take the head; if Some(node), set head to
//                                 node.next and return node.elem (move it out
//                                 of the Box). LIFO: last pushed pops first.
//   - len(&self) -> usize      -> walk the links counting (use .as_deref()/.as_ref()).
//   - iter(&self) -> Iter<T>   -> start an iterator at the head.
//   - Iter::next               -> yield &elem and advance to the next node.
//   - Drop for List            -> ITERATIVE drop. The auto-generated recursive
//                                 drop would recurse node→node and blow the
//                                 stack on a long list. Walk the chain in a
//                                 loop, take()-ing each `next`, so each Box is
//                                 freed one at a time. (check_9 builds 100k
//                                 nodes — a recursive drop would overflow.)
// ---------------------------------------------------------------------------
type Link<T> = Option<Box<Node<T>>>;

struct Node<T> {
    elem: T,
    next: Link<T>,
}

struct LinkedList<T> {
    head: Link<T>,
}

impl<T> LinkedList<T> {
    fn new() -> Self {
        Self { head: None }
    }

    fn push(&mut self, elem: T) {
        let head = self.head.take();
        match head {
            None => self.head = Some(Box::new(Node { elem, next: None })),
            Some(node) => {
                let new_node = Box::new(Node {
                    elem,
                    next: Some(node),
                });
                self.head = Some(new_node);
            }
        }
    }

    fn pop(&mut self) -> Option<T> {
        let head = self.head.take();
        match head {
            None => None,
            Some(node) => {
                self.head = node.next;
                Some(node.elem)
            }
        }
    }

    fn len(&self) -> usize {
        let mut size = 0;
        let mut current = self.head.as_ref();
        loop {
            match current {
                None => break,
                Some(node) => {
                    size += 1;
                    current = node.next.as_ref();
                }
            }
        }
        size
    }

    fn iter(&self) -> Iter<'_, T> {
        Iter {
            next: self.head.as_deref(),
        }
    }
}

impl<T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        let mut current = self.head.take();
        while let Some(mut node) = current {
            current = node.next.take();
        }
    }
}

struct Iter<'a, T> {
    next: Option<&'a Node<T>>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        let next = self.next.take();
        match next {
            None => None,
            Some(node) => {
                self.next = node.next.as_deref();
                Some(&node.elem)
            }
        }
    }
}

fn check_9() {
    let mut list: LinkedList<i32> = LinkedList::new();
    assert_eq!(list.len(), 0);
    assert_eq!(list.pop(), None);

    list.push(1);
    list.push(2);
    list.push(3);
    assert_eq!(list.len(), 3);

    // iter borrows, yields in LIFO order without consuming
    let collected: Vec<i32> = list.iter().copied().collect();
    assert_eq!(collected, vec![3, 2, 1]);
    assert_eq!(list.len(), 3); // iter didn't consume

    // pop is LIFO
    assert_eq!(list.pop(), Some(3));
    assert_eq!(list.pop(), Some(2));
    assert_eq!(list.pop(), Some(1));
    assert_eq!(list.pop(), None);

    // works generically
    let mut words: LinkedList<String> = LinkedList::new();
    words.push("a".to_string());
    words.push("b".to_string());
    assert_eq!(words.pop(), Some("b".to_string()));

    // the iterative Drop must survive a huge list without overflowing the stack
    let mut big = LinkedList::new();
    for i in 0..100_000 {
        big.push(i);
    }
    assert_eq!(big.len(), 100_000);
    drop(big); // <- recursive drop would stack-overflow here

    println!("✅ problem 9: hand-rolled generic linked list — push/pop/len/iter + iterative Drop");
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
