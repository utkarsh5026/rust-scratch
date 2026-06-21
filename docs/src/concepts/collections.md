# Collections deep-dive

> Ladder: [`src/bin/collections.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/collections.rs) ·
> Run: `cargo run --bin collections` · Phase 3 · 9 rungs

## TL;DR

Every std collection is the same idea — store many values — bent around a
different tradeoff between **ordering**, **lookup cost**, and **what the key has
to prove**:

| Collection | Backing | Lookup | Order | Key needs |
|---|---|---|---|---|
| `HashMap<K,V>` | hash table | O(1) avg | none (random) | `Hash + Eq` |
| `BTreeMap<K,V>` | B-tree | O(log n) | sorted, supports `range` | `Ord` |
| `HashSet<T>` | `HashMap<T,()>` | O(1) avg | none | `Hash + Eq` |
| `BTreeSet<T>` | `BTreeMap<T,()>` | O(log n) | sorted | `Ord` |
| `VecDeque<T>` | ring buffer | O(1) at both ends | insertion | — |

`HashMap` is the default. Reach for `BTreeMap` when you need order or range
queries, `VecDeque` when you push/pop at both ends, and swap the *hasher* (not
the map) when SipHash's DoS-resistance isn't worth its cost. The single most
important *technique* is the **`Entry` API**, which collapses the check-then-act
double lookup into one probe.

## Why this exists (from first principles)

You have a pile of values and you want to *find* one again. The naive answer —
a `Vec` you scan linearly — is O(n) per lookup. Collections buy you sub-linear
lookup, but nothing is free, so each one makes you pay in a different currency:

- A **hash table** turns the key into an array index via a hash function. Lookup
  is O(1) on average — but the price is that the keys land in hash order, which
  is *no order at all* to a human. And it only works if the key can be **hashed
  and compared** consistently (`Hash + Eq`).
- A **B-tree** keeps keys *sorted* in shallow, cache-friendly nodes. Lookup is
  O(log n) — slower than a hash, but now iteration is ordered and you can ask
  "give me every key between X and Y," which a hash table structurally cannot
  answer because its keys have no neighbors. The price is the key must be
  **orderable** (`Ord`).
- A **ring buffer** (`VecDeque`) gives up associative lookup entirely but makes
  *both ends* O(1), which a plain `Vec` can't (front insert/remove is O(n)
  because everything shifts).

The compiler enforces the key requirements through trait bounds: you literally
cannot use a type as a `HashMap` key until it implements `Hash + Eq`. That's not
bureaucracy — it's the table refusing to operate without the one guarantee that
makes it correct.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|---|---|---|
| 1 | foundations | `HashMap` basics | `get` returns `Option<&V>`; absent ⇒ `None`, not panic |
| 2 | foundations | `BTreeMap` & ordering | sorted iteration is free; `range(lo..=hi)` queries |
| 3 | mechanics | the `Entry` API | one lookup instead of check-then-insert |
| 4 | mechanics | `HashSet` & set algebra | dedup, `union`/`intersection`/`difference` |
| 5 | mechanics | `VecDeque` | O(1) both ends; sliding window + BFS frontier |
| 6 | footgun | `Borrow` lookup + key hazard | `get("k")` with no alloc; never mutate a key's hash |
| 7 | footgun | custom `Hash`/`Eq` | break `k==k' ⇒ hash==hash'` and silently lose entries |
| 8 | real-world | choosing one + hashers | decision matrix; swap `RandomState` for FNV-1a |
| 9 | capstone | `MyHashMap` from scratch | open addressing: linear probing + tombstones + resize |

## The ideas, built up

### 1. `HashMap`: `get` borrows, and absence is a value

```rust
fn word_count(text: &str) -> HashMap<&str, usize> {
    let mut map = HashMap::new();
    for word in text.split_whitespace() {
        *map.entry(word).or_insert(0) += 1;
    }
    map
}
```

Two things to internalize from the very first rung:

- **`get` returns `Option<&V>`, a borrow.** `wc.get("the")` is `Some(&3)`, not
  `Some(3)`. The map still owns the value; you get a reference into it. And a
  missing key is `None` — absence is an ordinary return value, never a panic.
  (Indexing with `map[k]` *does* panic on a missing key; `get` is the safe form.)
- The `&str` keys borrow from `text` — no `String` is allocated. The lifetime in
  `HashMap<&str, usize>` ties the map to the source string.

`split_whitespace()` (not `split(' ')`) collapses runs of spaces and handles
tabs/newlines, which is almost always what you want.

### 2. `BTreeMap`: order is the feature, `range` is the payoff

A `HashMap` iterates in effectively random order. A `BTreeMap` is *always*
ascending — you don't sort anything, the tree **is** the sort:

```rust
fn sorted_word_count(text: &str) -> Vec<(&str, usize)> {
    let mut map = BTreeMap::new();
    // ... tally ...
    map.into_iter().collect()  // already ascending by key
}
```

The capability a hash map cannot match is the **range query**:

```rust
fn score_range<'a>(scores: &BTreeMap<u32, &'a str>, lo: u32, hi: u32) -> Vec<&'a str> {
    scores.range(lo..=hi).map(|(_, name)| *name).collect()
}
```

`range(lo..=hi)` is two binary-search descents to find the window endpoints, then
an in-order walk — `O(log n + k)` for `k` results. A `HashMap` has no concept of
"the next key," so it can only answer point lookups.

### 3. The `Entry` API: stop looking things up twice

The naive "increment a counter" needs two or three hash lookups:

```rust
// WRONG (double lookup): hashes `k` twice
if map.contains_key(k) {
    *map.get_mut(k).unwrap() += 1;
} else {
    map.insert(k, 1);
}
```

`entry()` hashes the key **once**, returns a handle to that slot (`Occupied` or
`Vacant`), and lets you branch on it:

```rust
// OK (one lookup)
*map.entry(k).or_insert(0) += 1;
```

Two idioms the ladder drills:

```rust
// Group into Vecs: build the empty Vec ONLY when the key is new.
map.entry(word.len()).or_insert_with(Vec::new).push(word);

// Modify-or-insert: floor on first sight, +1 on every later sight.
map.entry(w).and_modify(|c| *c += 1).or_insert(floor);
```

- `or_insert_with(Vec::new)` vs `or_insert(Vec::new())`: the `_with` closure runs
  **only on a vacant slot**. Plain `or_insert` eagerly constructs its argument on
  *every* call, even when the key already exists — a wasted allocation each time.
  (`or_default()` is the same idea for `Default` types.)
- `and_modify(...).or_insert(...)` reads as *"if occupied, run the modify;
  otherwise insert."* The two arms are mutually exclusive by construction —
  `and_modify` returns the `Entry` back so `or_insert` can finalize it. That's why
  a value seen three times with `floor = 10` lands on `12` (insert 10, then +1,
  +1), never double-counted.

### 4. `HashSet`: a `HashMap<T, ()>` that speaks membership

```rust
// insert returns bool: true if the value was NEW. One probe, doubles as a
// "have I seen this?" test.
for &item in items {
    if seen.insert(item) { out.push(item); }   // dedup, preserving first-seen order
}
```

A `HashSet` *destroys* order, so "dedup but keep order" combines a `HashSet`
(the O(1) seen-test) with a `Vec` (the ordered output). The set algebra returns
**lazy iterators of `&T`**, unordered:

```rust
let inter: Vec<i32> = a.intersection(&b).copied().collect();  // in both
let only_a: Vec<i32> = a.difference(&b).copied().collect();   // a \ b
let union_size = a.union(&b).count();                         // |a ∪ b|, deduped
```

`union(&b).count()` already counts each element once, so you never need
`a.len() + b.len() - inter.len()`.

### 5. `VecDeque`: a ring buffer, the engine of BFS

A `Vec` is O(1) at the back but O(n) at the front — `remove(0)` shifts every
other element. A `VecDeque` keeps head and tail indices into a circular array, so
`push_back`/`pop_front`/`push_front`/`pop_back` are all O(1) amortized.

```rust
// Bounded sliding window: push, evict the oldest off the FRONT, record the max.
win.push_back(v);
if win.len() > cap { win.pop_front(); }
out.push(*win.iter().max().unwrap());
```

```rust
// BFS: VecDeque frontier + HashSet visited. Mark visited ON ENQUEUE.
visited.insert(0);
queue.push_back(0);
while let Some(node) = queue.pop_front() {
    order.push(node);
    for &n in &adj[node] {
        if visited.insert(n) { queue.push_back(n); }  // insert==true ⇒ newly seen
    }
}
```

The subtlety: mark a node visited **when you enqueue it**, not when you dequeue.
A node reachable from two parents would otherwise get enqueued twice. The
`insert`-returns-`bool` trick gates the `push_back` in a single probe.

## Footguns

### The `Borrow` lookup (a feature that looks like magic)

Why does `map.get("foo")` work on a `HashMap<String, V>` without building a
`String`? Because `get` is generic over anything the key can be *borrowed as*:

```rust
fn get<Q>(&self, k: &Q) -> Option<&V>
where K: Borrow<Q>, Q: Hash + Eq + ?Sized
```

`String: Borrow<str>`, and the `Borrow` contract guarantees `"foo"` hashes and
compares **identically** whether it's a `str` or a `String`. So you probe with a
borrowed view and allocate nothing:

```rust
// OK: &str query against a HashMap<String, _>, zero allocation
if let Some(v) = map.get(query) { total += v; }

// WRONG: needless allocation per query
if let Some(v) = map.get(&query.to_string()) { total += v; }
```

### Never mutate a key's hash while it's in the map

A `HashMap` files each key into a bucket by `hash(key)` **at insertion time**. If
the key's hash later changes, the entry is stranded in the wrong bucket: lookups
by the new value probe a *different* bucket and find nothing. The entry is leaked
in place — still consuming memory, permanently unreachable.

Rust normally makes this impossible: keys are owned and never lent out as `&mut`.
But **interior mutability is the escape hatch**. The ladder builds a `BadKey`
wrapping a `Cell<u64>` that hashes on its inner value:

```rust
let mut map = HashMap::new();
map.insert(BadKey::new(1), "value");          // filed under hash(1)
map.keys().next().unwrap().inner.set(999);    // mutate the map's OWN key via Cell
map.get(&BadKey::new(999)).is_some()          // false — probes hash(999) bucket, empty
```

`keys()` hands out a shared `&BadKey`, and `Cell::set` mutates through a shared
reference — so you corrupt the real stored key while it sits in its bucket. This
is exactly why `Cell`/`RefCell` keys are a latent bug.

### Break `k == k' ⇒ hash(k) == hash(k')` and you silently lose data

This is the **one law** every map key must obey: *equal keys must hash equal.*
(The converse isn't required — unequal keys may collide.) `#[derive(Hash,
PartialEq, Eq)]` can never break it because it threads the *same fields* through
both. Hand-write them and you can desync — with no error and no panic, just
vanishing entries.

```rust
// GoodKey: case-insensitive, law UPHELD (both fold case)
impl Hash for GoodKey {
    fn hash<H: Hasher>(&self, s: &mut H) { self.0.to_lowercase().hash(s); }
}
impl PartialEq for GoodKey {
    fn eq(&self, o: &Self) -> bool { self.0.to_lowercase() == o.0.to_lowercase() }
}

// BrokenKey: same eq (case-insensitive), but hash reads the RAW bytes
impl Hash for BrokenKey {
    fn hash<H: Hasher>(&self, s: &mut H) { self.0.hash(s); }  // <-- the bug
}
```

With `BrokenKey`, `"Foo" == "foo"` is `true` but they hash differently. Insert
`"foo"`, look up `"FOO"` → the probe lands in the `hash("FOO")` bucket, which
doesn't hold the `"foo"` entry → **miss**, even though the keys are "equal."

> The discipline: **every field `eq` looks at, `hash` must look at too.** Only
> hand-write these for a custom notion of equality (case-folding, normalization,
> a subset of fields); otherwise derive and stay safe.

## Real-world patterns

### Choosing one (the decision matrix)

```text
push/pop at both ends?       -> VecDeque
need sorted iter / range?    -> BTreeMap (k→v) or BTreeSet (membership)
key → value lookup?          -> HashMap
just membership?             -> HashSet
```

`HashMap` is the default; everything else is a deliberate upgrade for a property
you actually need. Note the *priority order* matters — a deque workload wins
regardless of the other flags.

### Custom hashers (swap the hasher, not the map)

std's `HashMap` defaults to `RandomState` (SipHash 1-3) seeded with a
**per-process random key**. That's deliberate DoS protection: an attacker can't
precompute keys that all collide into one bucket and turn your O(1) map into an
O(n) linked list. The cost is speed on small keys and **nondeterministic
iteration order** across runs.

For internal, non-adversarial maps over small keys, crates like `fxhash`/`ahash`
swap in a faster non-cryptographic hasher. The ladder builds the minimal one —
**FNV-1a**:

```rust
struct FnvHasher { state: u64 }
impl Default for FnvHasher {
    fn default() -> Self { Self { state: 0xcbf2_9ce4_8422_2325 } }  // offset basis
}
impl Hasher for FnvHasher {
    fn finish(&self) -> u64 { self.state }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state ^= b as u64;                          // xor first ("1a")
            self.state = self.state.wrapping_mul(0x100000001b3);  // then multiply
        }
    }
}
type FnvMap<K, V> = HashMap<K, V, BuildHasherDefault<FnvHasher>>;
```

- Seeding `state` in `Default` (rather than lazily inside `write`) is the form
  real hasher crates ship: it removes a per-`write` branch and correctly handles
  a key that calls `write` multiple times (a struct hashing field by field), since
  the FNV chain flows continuously across calls.
- `BuildHasher` is the factory: it makes a fresh `Hasher` per key. `BuildHasherDefault<H>`
  is the zero-config version that just calls `H::default()`. The result is a
  **deterministic** map — same key, same hash, every run.
- `Hasher` has default impls for `write_u32`, `write_u64`, etc., all funneling
  into `write(&[u8])` — so implementing `write` + `finish` is enough.

## Capstone insight

`MyHashMap<K, V>` is a working hash map built on **open addressing** — one flat
`Vec<Slot<K,V>>` where `Slot` is `Empty | Deleted | Occupied(K, V)`. (std's table
is this idea plus SIMD probing; this is the readable textbook version.)

**Linear probing.** To place a key: `home = hash(k) % capacity`, then walk
forward with wraparound (`(i + 1) % cap`) until you hit the first `Empty` (key
absent) or a matching `Occupied` (overwrite). Lookups probe the same path and
stop at `Empty` — because if the key were present, insert would have filled that
empty slot before reaching it.

**Tombstones.** On remove you cannot just set the slot `Empty` — that would cut
the probe chain and hide keys inserted *after* it on the same chain. Instead leave
a `Deleted` marker. Lookups skip past tombstones; inserts may reuse them.

> The hardest correctness point: when inserting, **remember the first tombstone
> you pass, but keep probing.** Only commit to reusing it once you reach `Empty`
> (proving the key is genuinely absent). Reusing it eagerly would create a
> duplicate key if that key already lives further down the chain.

**Resize / rehash.** When `(len + tombstones) * 4 >= capacity * 3` (load factor
0.75), allocate a table of double capacity and re-insert every `Occupied` entry —
which also drops all tombstones for free. The crucial realization: `home` depends
on `self.slots.len()`, so after a resize the **same key maps to a different
bucket**. Resize must *rehash*, not memcpy. The recursion (resize calls insert,
insert calls resize) is bounded: after doubling, live entries sit well under the
0.75 threshold of the new table, so the re-inserts never re-trigger a resize.

Two Rust mechanics carry the whole thing:

```rust
// overwrite: move the old V out from behind &mut, drop the new one in
Some(std::mem::replace(v, value))
// remove: swap the whole slot for a tombstone, extract the old value
let old = std::mem::replace(&mut self.slots[i], Slot::Deleted);
```

`mem::replace` is the canonical "move a value out from behind a `&mut`" tool —
you can't move out of a `Vec` element otherwise.

## Explain it back

- Why does `HashMap::get` return `Option<&V>` instead of `Option<V>`, and when
  does `map[k]` panic where `map.get(k)` wouldn't?
- What can a `BTreeMap` do that a `HashMap` structurally cannot, and what does the
  key pay for it (which trait bound)?
- Write the `Entry`-API one-liner for "increment a counter," and explain why
  `or_insert_with(Vec::new)` beats `or_insert(Vec::new())`.
- Why does `map.get("foo")` compile and allocate nothing on a `HashMap<String, V>`?
  Name the trait and the bound on `get`.
- State the `Hash`/`Eq` law in one line. Describe a concrete way to break it and
  exactly what symptom the user sees.
- Why is mutating a key through a `Cell` after insertion a bug, and why does Rust
  normally prevent it?
- Why does std default to SipHash with a random seed, and when would you swap it
  out? What do you give up?
- In an open-addressing map, why can't `remove` set a slot to `Empty`? What goes
  wrong, and what's the fix?
- During insert, why must you keep probing past the first tombstone before reusing
  it?

## See also

- [`Borrow` / `ToOwned`](borrow-toowned.md) — the trait behind no-alloc `get("k")` lookups.
- [`Cell` & `RefCell`](cell-refcell.md) — the interior mutability that powers the key-mutation footgun.
- [`Drop` & Ordering](drop-ordering.md) — `mem::replace`/`mem::take` for moving values out from behind `&mut`.
- [Newtype & zero-cost wrappers](newtype.md) — wrapping a key type to give it a custom `Hash`/`Eq`.
