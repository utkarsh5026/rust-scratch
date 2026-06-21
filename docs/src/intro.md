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
| `Cow` | [✅ note](concepts/cow.md) | `src/bin/cow.rs` |
| `Box` & the heap | [✅ note](concepts/box-heap.md) | `src/bin/box_heap.rs` |
| `Rc` / `Arc` | [✅ note](concepts/rc-arc.md) | `src/bin/rc_arc.rs` |
| `Cell` / `RefCell` | [✅ note](concepts/cell-refcell.md) | `src/bin/cell_refcell.rs` |
| `Rc<RefCell<T>>` patterns | [✅ note](concepts/rc-refcell.md) | `src/bin/rc_refcell.rs` |
| Conversion traits | [✅ note](concepts/conversions.md) | `src/bin/conversions.rs` |
| Lifetimes in depth | [✅ note](concepts/lifetimes-depth.md) | `src/bin/lifetimes_depth.rs` |
| HRTB — `for<'a>` | [✅ note](concepts/hrtb.md) | `src/bin/hrtb.rs` |
| `Borrow` / `ToOwned` | [✅ note](concepts/borrow-toowned.md) | `src/bin/borrow_toowned.rs` |
| `Drop` & ordering | [✅ note](concepts/drop-ordering.md) | `src/bin/drop_ordering.rs` |
| Associated types vs generic params | [✅ note](concepts/assoc-vs-generic.md) | `src/bin/assoc_vs_generic.rs` |
| Generic bounds & `where` clauses | [✅ note](concepts/generic-bounds.md) | `src/bin/generic_bounds.rs` |
| Blanket impls & coherence | [✅ note](concepts/blanket-coherence.md) | `src/bin/blanket_coherence.rs` |
| Static vs dynamic dispatch | [✅ note](concepts/dispatch.md) | `src/bin/dispatch.rs` |
| Error handling architecture | [✅ note](concepts/error-arch.md) | `src/bin/error_arch.rs` |
| Custom error types | [✅ note](concepts/custom-errors.md) | `src/bin/custom_errors.rs` |
| Builder pattern | [✅ note](concepts/builder.md) | `src/bin/builder.rs` |
| The typestate pattern | [✅ note](concepts/typestate.md) | `src/bin/typestate.rs` |
| Newtype & zero-cost wrappers | [✅ note](concepts/newtype.md) | `src/bin/newtype.rs` |
| Threads & scoped threads | [✅ note](concepts/threads.md) | `src/bin/threads.rs` |
| `Send` & `Sync` deeply | [✅ note](concepts/send-sync.md) | `src/bin/send_sync.rs` |

New notes get added under **Concepts** as each ladder is finished — see
[Adding a new note](meta/adding-a-note.md).
