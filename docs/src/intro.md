# Rust Mastery Notes

This is my **learn-by-doing** knowledge base — the distilled lessons from each
Rust concept I drill in [`rust-scratch`](https://github.com/utkarsh5026/rust-scratch).

For every concept I finish a **ladder of 7–9 problems** (foundations → mechanics →
footguns → real-world patterns → a build-it-from-scratch capstone), living in
`src/bin/<concept>.rs`. Once a ladder is done, I write the lasting takeaways here —
the mental model, the signatures worth memorizing, the footguns I hit, and the
"explain it back" prompts that prove I actually own it.

> The code is the gym. **This is the notebook.**

## How to read a note

Each concept page follows the same shape:

| Section | What's in it |
|---|---|
| **TL;DR** | The one-paragraph mental model. |
| **Why it exists** | The problem this concept solves. |
| **The ladder** | The 7–9 rungs I worked, what each one taught. |
| **Signatures to know** | The std types/bounds worth memorizing. |
| **Footguns** | The traps I hit (or that the ladder deliberately set). |
| **Explain it back** | Questions I should be able to answer cold. |

## Completed concepts

| Concept | Note | Source ladder |
|---|---|---|
| `Cow` | _todo_ | `src/bin/cow.rs` |
| `Box` & the heap | _todo_ | `src/bin/box_heap.rs` |
| `Rc` / `Arc` | _todo_ | `src/bin/rc_arc.rs` |
| `Cell` / `RefCell` | _todo_ | `src/bin/cell_refcell.rs` |
| Conversion traits | _todo_ | `src/bin/conversions.rs` |
| Lifetimes in depth | _todo_ | `src/bin/lifetimes_depth.rs` |
| `Borrow` / `ToOwned` | [✅ note](concepts/borrow-toowned.md) | `src/bin/borrow_toowned.rs` |

New notes get added under **Concepts** as each ladder is finished — see
[Adding a new note](meta/adding-a-note.md).
