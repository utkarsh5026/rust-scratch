# Summary

[Introduction](intro.md)

---

# Concepts

## Phase 0 — Tooling, project structure & testing

- [Modules & visibility](concepts/modules.md)
- [Cargo features & `cfg`](concepts/features-cfg.md)

## Phase 1 — Ownership, conversions & the type system

- [Cow — Clone-on-Write](concepts/cow.md)
- [Box & the Heap](concepts/box-heap.md)
- [Cell & RefCell](concepts/cell-refcell.md)
- [Conversion traits](concepts/conversions.md)
- [Borrow / ToOwned](concepts/borrow-toowned.md)
- [Drop & Ordering](concepts/drop-ordering.md)
- [Lifetimes in depth](concepts/lifetimes-depth.md)
- [HRTB — for<'a>](concepts/hrtb.md)
- [Rc / Arc](concepts/rc-arc.md)
- [Rc<RefCell<T>> patterns](concepts/rc-refcell.md)

## Phase 2 — Traits & generics like a library author

- [Associated types vs generic params](concepts/assoc-vs-generic.md)
- [Generic bounds & where clauses](concepts/generic-bounds.md)
- [Blanket impls & coherence](concepts/blanket-coherence.md)
- [Static vs dynamic dispatch](concepts/dispatch.md)
- [Closures & Fn/FnMut/FnOnce](concepts/closures.md)
- [impl Trait & RPIT](concepts/impl-trait.md)

## Phase 3 — API & error design

- [Error handling architecture](concepts/error-arch.md)
- [Custom error types](concepts/custom-errors.md)
- [Builder pattern](concepts/builder.md)
- [The typestate pattern](concepts/typestate.md)
- [Newtype & zero-cost wrappers](concepts/newtype.md)
- [API evolution & semver](concepts/semver.md)
- [Collections deep-dive](concepts/collections.md)
- [Strings & text](concepts/strings-text.md)
- [Iterators end-to-end](concepts/iterators.md)

## Phase 4 — Concurrency

- [Threads & scoped threads](concepts/threads.md)
- [`Send` & `Sync` deeply](concepts/send-sync.md)
- [`Mutex` / `RwLock`](concepts/mutex-rwlock.md)
- [Channels](concepts/channels.md)
- [Data parallelism with `rayon`](concepts/rayon-parallel.md)

---

# Meta

- [Adding a new note](meta/adding-a-note.md)
- [Note template](meta/template.md)
