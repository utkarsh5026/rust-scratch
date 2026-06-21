//! Collections deep-dive — HashMap / BTreeMap / VecDeque / HashSet
//! Run: cargo run --bin collections
//!
//! Ladder (one fn per rung; `main` replays solved rungs and stops at the first todo!):
//!   1. HashMap basics          — word-frequency counter            [foundations]  <- CURRENT
//!   2. BTreeMap & ordering      — sorted iteration + range queries  [foundations]
//!   3. The Entry API            — kill the double-lookup            [mechanics]
//!   4. HashSet & set algebra    — dedup, union/intersection/diff    [mechanics]
//!   5. VecDeque                 — O(1) both ends, BFS queue         [mechanics]
//!   6. Borrow lookup + key hazard — get("k") & don't mutate a key   [footgun]
//!   7. Custom Hash/Eq contract  — break the law, lose entries       [footgun]
//!   8. Choosing one + hashers   — decision matrix, custom hasher    [real-world]
//!   9. Capstone: MyHashMap      — open addressing from scratch      [capstone]

use std::collections::HashMap;

// ── Rung 1: HashMap basics ────────────────────────────────────────────────
// Count how many times each word appears in `text` (split on whitespace).
// Return a HashMap from each word (&str) to its count (usize).
fn word_count(text: &str) -> HashMap<&str, usize> {
    let mut map = HashMap::new();
    for word in text.split_whitespace() {
        *map.entry(word).or_insert(0) += 1;
    }
    map
}

fn check_1() {
    let wc = word_count("the cat sat on the mat the cat");
    assert_eq!(wc.get("the"), Some(&3));
    assert_eq!(wc.get("cat"), Some(&2));
    assert_eq!(wc.get("sat"), Some(&1));
    assert_eq!(wc.get("dog"), None); // absent key -> None, not a panic
    assert_eq!(wc.len(), 5); // the, cat, sat, on, mat
    println!("rung 1 ok: {:?} distinct words", wc.len());
}

// ── Rung 2: BTreeMap & ordering ───────────────────────────────────────────
// A BTreeMap keeps keys in sorted order — iteration is always ascending, and
// you get `range` queries a HashMap structurally cannot do.
//
// (a) `sorted_word_count`: same tally as rung 1, but return the (word, count)
//     pairs in ascending key order as a Vec.
// (b) `score_range`: given a BTreeMap<u32, &str> of scores->names, return all
//     names whose score is in [lo, hi] inclusive, in ascending score order.
use std::collections::BTreeMap;

fn sorted_word_count(text: &str) -> Vec<(&str, usize)> {
    let mut map = BTreeMap::new();
    for word in text.split_whitespace() {
        *map.entry(word).or_insert(0) += 1;
    }
    map.into_iter().collect()
}

fn score_range<'a>(scores: &BTreeMap<u32, &'a str>, lo: u32, hi: u32) -> Vec<&'a str> {
    scores
        .range(lo..=hi)
        .map(|(_, name)| *name)
        .collect::<Vec<&'a str>>()
}

fn check_2() {
    let swc = sorted_word_count("pear apple cherry apple banana pear apple");
    // ascending by word: apple(3), banana(1), cherry(1), pear(2)
    assert_eq!(
        swc,
        vec![("apple", 3), ("banana", 1), ("cherry", 1), ("pear", 2)]
    );

    let mut scores = BTreeMap::new();
    scores.insert(50, "alice");
    scores.insert(70, "bob");
    scores.insert(90, "carol");
    scores.insert(60, "dave");
    // scores in [60, 80]: dave(60), bob(70)  -> ascending by score
    assert_eq!(score_range(&scores, 60, 80), vec!["dave", "bob"]);
    assert_eq!(
        score_range(&scores, 0, 1000),
        vec!["alice", "dave", "bob", "carol"]
    );
    assert_eq!(score_range(&scores, 100, 200), Vec::<&str>::new());
    println!("rung 2 ok: sorted iteration + range queries");
}

// ── Rung 3: The Entry API ─────────────────────────────────────────────────
// `entry()` returns an Entry enum (Occupied | Vacant) and does the hash lookup
// ONCE — you then decide what to do with that slot. This kills the classic
// "contains_key? then get_mut, else insert" triple-lookup dance.
//
// (a) `group_by_len`: group words by their length into a HashMap<usize, Vec<&str>>.
//     Each new length needs a fresh Vec — use or_insert_with(Vec::new) (or
//     or_default) so the empty Vec is only built when the key is actually new.
// (b) `tally_with_floor`: count words, but every word starts at a baseline of
//     `floor` the first time it's seen, then +1 per occurrence after that.
//     Use entry(w).and_modify(|c| *c += 1).or_insert(floor).
//     So 1st sighting -> floor, 2nd -> floor+1, etc.
fn group_by_len<'a>(words: &[&'a str]) -> HashMap<usize, Vec<&'a str>> {
    let mut map = HashMap::new();
    for word in words {
        map.entry(word.len()).or_insert_with(Vec::new).push(*word);
    }
    map
}

fn tally_with_floor<'a>(words: &[&'a str], floor: usize) -> HashMap<&'a str, usize> {
    let mut map = HashMap::new();
    for word in words {
        map.entry(*word).and_modify(|c| *c += 1).or_insert(floor);
    }
    map
}

fn check_3() {
    let g = group_by_len(&["hi", "ok", "yes", "no", "wow", "a"]);
    assert_eq!(g[&1], vec!["a"]);
    assert_eq!(g[&2], vec!["hi", "ok", "no"]);
    assert_eq!(g[&3], vec!["yes", "wow"]);
    assert_eq!(g.len(), 3);

    let t = tally_with_floor(&["x", "y", "x", "x", "y"], 10);
    // x seen 3x: 10 (insert), then +1, +1 -> 12
    // y seen 2x: 10 (insert), then +1       -> 11
    assert_eq!(t[&"x"], 12);
    assert_eq!(t[&"y"], 11);
    println!("rung 3 ok: Entry API one-lookup grouping & modify-or-insert");
}

// ── Rung 4: HashSet & set algebra ─────────────────────────────────────────
// A HashSet<T> is a HashMap<T, ()> with a nicer API: membership, dedup, and
// the algebra of sets. The set operations return *iterators* (lazy), so you
// collect them into whatever you need.
//
// (a) `unique_in_order`: dedup a slice but PRESERVE first-seen order. A HashSet
//     answers "have I seen this?" in O(1); push to the output only on first sight.
//     (Returns Vec<&str> — HashSet alone would lose the order, so combine it
//     with a Vec.)
// (b) `set_ops`: given two slices, return (intersection, only_in_a, union_size)
//     as (sorted Vec<i32>, sorted Vec<i32>, usize). Use HashSet's
//     intersection / difference / union. They yield &T, and they're unordered,
//     so sort before returning.
use std::collections::HashSet;

fn unique_in_order<'a>(items: &[&'a str]) -> Vec<&'a str> {
    let mut set = HashSet::new();
    let mut result = Vec::new();
    for item in items {
        if set.insert(*item) {
            result.push(*item);
        }
    }
    result
}

fn set_ops(a: &[i32], b: &[i32]) -> (Vec<i32>, Vec<i32>, usize) {
    let set_a = a.iter().copied().collect::<HashSet<i32>>();
    let set_b = b.iter().copied().collect::<HashSet<i32>>();

    let mut intersection = set_a.intersection(&set_b).copied().collect::<Vec<i32>>();
    let mut difference = set_a.difference(&set_b).copied().collect::<Vec<i32>>();
    let union_size = set_a.union(&set_b).count();

    intersection.sort();
    difference.sort();

    (intersection, difference, union_size)
}

fn check_4() {
    assert_eq!(
        unique_in_order(&["a", "b", "a", "c", "b", "d"]),
        vec!["a", "b", "c", "d"]
    );

    let (inter, only_a, union_size) = set_ops(&[1, 2, 3, 4], &[3, 4, 5, 6]);
    assert_eq!(inter, vec![3, 4]); // in both
    assert_eq!(only_a, vec![1, 2]); // a \ b
    assert_eq!(union_size, 6); // {1,2,3,4,5,6}
    println!("rung 4 ok: dedup-in-order + set algebra");
}

// ── Rung 5: VecDeque ──────────────────────────────────────────────────────
// A Vec is O(1) at the back but O(n) at the front (everything shifts). A
// VecDeque is a growable ring buffer: push/pop at BOTH ends are O(1) amortized.
// That makes it the natural queue (FIFO) and the backbone of BFS.
//
// (a) `sliding_max`: a fixed-capacity window. Feed values one at a time; keep
//     only the last `cap` of them (drop the oldest off the FRONT when full),
//     and after each push record the current max of the window. Return the Vec
//     of per-step maxima. This is the "keep a bounded history" pattern.
// (b) `bfs_layers`: breadth-first traversal of a graph given as adjacency lists
//     (adj[i] = neighbors of node i). Start from node 0. Return nodes in BFS
//     visitation order. Use a VecDeque as the frontier queue + a HashSet of
//     visited. (Classic: pop_front the queue, push_back unvisited neighbors.)
use std::collections::VecDeque;

fn sliding_max(values: &[i32], cap: usize) -> Vec<i32> {
    if cap == 0 {
        return Vec::new();
    }

    let mut deque: VecDeque<i32> = VecDeque::new();
    let mut maxima = Vec::new();

    for value in values {
        deque.push_back(*value);
        if deque.len() > cap {
            deque.pop_front();
        }

        let max = deque.iter().max().expect("cap > 0 keeps window non-empty");
        maxima.push(*max);
    }

    maxima
}

fn bfs_layers(adj: &[Vec<usize>]) -> Vec<usize> {
    if adj.is_empty() {
        return Vec::new();
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut order = Vec::new();

    visited.insert(0);
    queue.push_back(0);

    while let Some(node) = queue.pop_front() {
        order.push(node);

        for &neighbor in &adj[node] {
            if visited.insert(neighbor) {
                queue.push_back(neighbor);
            }
        }
    }

    order
}

fn check_5() {
    // window cap 3 over [1,3,2,5,4,0]:
    //  [1]->1  [1,3]->3  [1,3,2]->3  [3,2,5]->5  [2,5,4]->5  [5,4,0]->5
    assert_eq!(sliding_max(&[1, 3, 2, 5, 4, 0], 3), vec![1, 3, 3, 5, 5, 5]);

    //   0 ── 1 ── 3
    //   │         │
    //   2 ────────┘
    let adj = vec![
        vec![1, 2], // 0 -> 1, 2
        vec![0, 3], // 1 -> 0, 3
        vec![0, 3], // 2 -> 0, 3
        vec![1, 2], // 3 -> 1, 2
    ];
    // BFS from 0: visit 0, enqueue 1,2; visit 1, enqueue 3; visit 2; visit 3
    assert_eq!(bfs_layers(&adj), vec![0, 1, 2, 3]);
    println!("rung 5 ok: ring-buffer window + BFS queue");
}

// ── Rung 6: Borrow lookup trick & the key-mutation hazard ─────────────────
// Two footguns that define how map keys really work.
//
// FOOTGUN A — the Borrow lookup. A HashMap<String, V> lets you call
// map.get("foo") with a &str, never building a String. That works because
// `get<Q>` is generic: `where String: Borrow<Q>, Q: Hash + Eq`. String borrows
// as str, and str hashes the SAME as the String would. The contract that makes
// this sound: `x.borrow()` must hash and compare identically to `x`.
//
//   (a) `count_lookups`: you're given a HashMap<String, u32> and a list of &str
//       queries. Sum the counts for queries that are present — WITHOUT allocating
//       a String per query. (i.e. call .get(q) with q: &str directly.)
//
// FOOTGUN B — never mutate a key's hash while it lives in the map. If a key's
// hash changes after insertion, it sits in the wrong bucket: lookups miss it and
// it silently "disappears". Rust mostly prevents this (keys are owned & not
// exposed mutably) — but interior mutability (Cell/RefCell) lets you smuggle a
// mutation through a shared &. This rung makes that corruption VISIBLE.
//
//   (b) `demonstrate_key_mutation`: a HashMap<BadKey, &str> where BadKey wraps a
//       Cell<u64> and hashes/compares on that inner value (impls provided). Insert
//       a key, MUTATE its inner value via the Cell (you still hold a handle to it),
//       then look it up by its NEW value. Return whether the lookup found it.
//       Spoiler: it returns false — the entry is lost (wrong bucket). Your job is
//       to write the code that proves it, then explain WHY in the SAFETY-style note.
use std::cell::Cell;
use std::hash::{Hash, Hasher};

#[derive(Clone)]
struct BadKey {
    inner: Cell<u64>,
}
impl BadKey {
    fn new(v: u64) -> Self {
        BadKey {
            inner: Cell::new(v),
        }
    }
}
// Hash and Eq both read the CURRENT inner value — so changing it changes both.
impl Hash for BadKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.get().hash(state);
    }
}
impl PartialEq for BadKey {
    fn eq(&self, other: &Self) -> bool {
        self.inner.get() == other.inner.get()
    }
}
impl Eq for BadKey {}

fn count_lookups(map: &HashMap<String, u32>, queries: &[&str]) -> u32 {
    let mut count = 0;
    for query in queries {
        if map.get(*query).is_some() {
            count += map.get(*query).unwrap();
        }
    }
    count
}

fn demonstrate_key_mutation() -> bool {
    let mut map = HashMap::new();
    let k = BadKey::new(1);
    map.insert(k, "value");
    map.keys().next().unwrap().inner.set(999);
    map.get(&BadKey::new(999)).is_some()
}

fn check_6() {
    let mut m = HashMap::new();
    m.insert("alpha".to_string(), 10u32);
    m.insert("beta".to_string(), 20);
    m.insert("gamma".to_string(), 30);
    // queries are &str — must hit without allocating Strings
    assert_eq!(count_lookups(&m, &["alpha", "gamma", "zeta"]), 40);

    // mutating a key's hash after insertion loses the entry
    assert_eq!(demonstrate_key_mutation(), false);
    println!("rung 6 ok: Borrow lookup (no alloc) + key-mutation corruption witnessed");
}

// ── Rung 7: Custom Hash/Eq + the contract ─────────────────────────────────
// THE LAW every HashMap key must obey:  k1 == k2  ⇒  hash(k1) == hash(k2).
// (Equal keys MUST hash equal. The converse isn't required — unequal keys may
// collide.) `#[derive(Hash, PartialEq, Eq)]` always upholds it because it uses
// the SAME fields for both. Hand-writing them lets you break it — and a broken
// key doesn't error, it silently loses entries.
//
// You'll build the SAME key type two ways and prove one is broken:
//
//   (a) GoodKey: a case-insensitive string key. Eq compares lowercased; Hash
//       hashes the lowercased form. Law holds: "Foo" == "foo" AND they hash the
//       same, so map.get(GoodKey("FOO")) finds an entry inserted as GoodKey("foo").
//
//   (b) BrokenKey: Eq STILL compares case-insensitively, but Hash hashes the
//       RAW bytes (not lowercased). Now "Foo" == "foo" but hash("Foo") !=
//       hash("foo") — the law is VIOLATED. Inserting "foo" then looking up "FOO"
//       will MISS (different bucket), even though the keys are "equal".
//
// `probe(map_inserted_as, lookup_as)` returns whether the lookup hits.
// Implement Hash + PartialEq + Eq for both. Then the checks prove Good hits and
// Broken misses — same logical key, opposite outcome, purely from breaking the law.
#[derive(Clone)]
struct GoodKey(String);
#[derive(Clone)]
struct BrokenKey(String);

impl Hash for GoodKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_lowercase().hash(state);
    }
}

impl PartialEq for GoodKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_lowercase() == other.0.to_lowercase()
    }
}

impl Eq for GoodKey {}

impl Hash for BrokenKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl PartialEq for BrokenKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_lowercase() == other.0.to_lowercase()
    }
}

impl Eq for BrokenKey {}

fn good_probe(inserted_as: &str, lookup_as: &str) -> bool {
    let mut map: HashMap<GoodKey, u32> = HashMap::new();
    map.insert(GoodKey(inserted_as.to_string()), 1);
    map.get(&GoodKey(lookup_as.to_string())).is_some()
}

fn broken_probe(inserted_as: &str, lookup_as: &str) -> bool {
    let mut map: HashMap<BrokenKey, u32> = HashMap::new();
    map.insert(BrokenKey(inserted_as.to_string()), 1);
    map.get(&BrokenKey(lookup_as.to_string())).is_some()
}

fn check_7() {
    // GoodKey upholds the law: insert "foo", find it via "FOO"
    assert!(good_probe("foo", "foo"));
    assert!(good_probe("foo", "FOO")); // case-insensitive hit
    assert!(good_probe("Hello", "hELLO"));

    // BrokenKey: Eq says equal, but Hash disagrees -> lookup lands in wrong bucket
    assert!(broken_probe("foo", "foo")); // same string -> same hash -> still hits
    assert!(!broken_probe("foo", "FOO")); // EQUAL keys, DIFFERENT hash -> MISS
    println!("rung 7 ok: breaking k==k' ⇒ hash==hash' silently loses entries");
}

// ── Rung 8: Choosing one + custom hashers ─────────────────────────────────
// Two real-world skills: (A) picking the right collection from requirements,
// and (B) swapping HashMap's default hasher.
//
// Background on the default hasher: std's HashMap uses `RandomState` (SipHash
// 1-3) seeded with a per-process RANDOM key. That's deliberate DoS protection —
// an attacker can't precompute keys that all collide into one bucket. The cost:
// SipHash is slow for tiny keys, and iteration/hash order is nondeterministic
// across runs. For internal, non-adversarial maps with small keys (u32/u64),
// crates like `fxhash`/`ahash` swap in a faster, NON-cryptographic hasher.
//
//   (A) `choose_collection(spec)` — return the &'static str name of the BEST std
//       collection for the requirement. Decision matrix (first match wins, in
//       this priority): see the Spec fields and the asserts in check_8.
//
//   (B) Implement FNV-1a, a tiny deterministic hasher, and use it in a HashMap.
//       FNV-1a over bytes:  hash = OFFSET; for b in bytes { hash ^= b; hash *= PRIME }
//       (64-bit: OFFSET = 0xcbf29ce484222325, PRIME = 0x100000001b3, wrapping mul.)
//       You implement the `Hasher` trait (write_u8/write + finish). Then
//       `FnvMap<K,V>` is a HashMap with BuildHasherDefault<FnvHasher>, giving a
//       DETERMINISTIC map (same key -> same hash every run, unlike RandomState).
use std::hash::BuildHasherDefault;

struct Spec {
    need_sorted_iteration: bool, // must iterate keys in order / want range queries
    push_pop_both_ends: bool,    // queue/deque workload
    keys_values: bool,           // true = key->value map, false = just membership
}

fn choose_collection(spec: &Spec) -> &'static str {
    let Spec {
        need_sorted_iteration,
        push_pop_both_ends,
        keys_values,
    } = *spec;
    if push_pop_both_ends {
        return "VecDeque";
    }
    if need_sorted_iteration && keys_values {
        return "BTreeMap";
    }
    if need_sorted_iteration && !keys_values {
        return "BTreeSet";
    }
    if keys_values {
        return "HashMap";
    }
    "HashSet"
}

struct FnvHasher {
    state: u64,
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

impl Default for FnvHasher {
    fn default() -> Self {
        Self { state: FNV_OFFSET }
    }
}

impl Hasher for FnvHasher {
    fn finish(&self) -> u64 {
        self.state
    }
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
    }
}

type FnvMap<K, V> = HashMap<K, V, BuildHasherDefault<FnvHasher>>;

fn fnv_roundtrip() -> bool {
    let mut m: FnvMap<&str, i32> = FnvMap::default();
    m.insert("one", 1);
    m.insert("two", 2);
    m.insert("three", 3);
    m.get("two") == Some(&2) && m.get("missing").is_none() && m.len() == 3
}

fn fnv_is_deterministic() -> bool {
    // Same bytes hashed twice -> identical output (RandomState would differ across
    // processes; FNV is pure). Hash "hello" two independent times and compare.
    fn h(s: &str) -> u64 {
        let mut hasher = FnvHasher::default();
        hasher.write(s.as_bytes());
        hasher.finish()
    }
    h("hello") == h("hello") && h("hello") != h("world")
}

fn check_8() {
    assert_eq!(
        choose_collection(&Spec {
            need_sorted_iteration: false,
            push_pop_both_ends: true,
            keys_values: true
        }),
        "VecDeque"
    );
    assert_eq!(
        choose_collection(&Spec {
            need_sorted_iteration: true,
            push_pop_both_ends: false,
            keys_values: true
        }),
        "BTreeMap"
    );
    assert_eq!(
        choose_collection(&Spec {
            need_sorted_iteration: true,
            push_pop_both_ends: false,
            keys_values: false
        }),
        "BTreeSet"
    );
    assert_eq!(
        choose_collection(&Spec {
            need_sorted_iteration: false,
            push_pop_both_ends: false,
            keys_values: true
        }),
        "HashMap"
    );
    assert_eq!(
        choose_collection(&Spec {
            need_sorted_iteration: false,
            push_pop_both_ends: false,
            keys_values: false
        }),
        "HashSet"
    );

    assert!(
        fnv_roundtrip(),
        "custom-hasher map must behave like a normal map"
    );
    assert!(
        fnv_is_deterministic(),
        "FNV-1a must be a pure function of the bytes"
    );
    println!("rung 8 ok: decision matrix + deterministic FNV-1a custom hasher");
}

// ── Rung 9: CAPSTONE — MyHashMap from scratch (open addressing) ───────────
// Build a working hash map to prove you own the whole model. Std uses Swiss-
// table (SIMD) open addressing; you'll build the classic textbook version:
// LINEAR PROBING with TOMBSTONES and load-factor RESIZE.
//
// THE MODEL: one flat `Vec<Slot<K,V>>`. To insert key k:
//   1. home = hash(k) % capacity
//   2. walk forward (wrapping) from `home`: home, home+1, home+2, ...
//   3. stop at the first Empty slot (key absent -> put it there) OR the first
//      slot whose key == k (key present -> overwrite the value).
//   Lookups probe the same way and STOP at the first Empty (an Empty means
//   "k was never here", because insert would have used that slot).
//
// TOMBSTONES: on remove you can't just set the slot Empty — that would cut the
// probe chain and hide keys inserted after it. Mark it `Deleted` (a tombstone):
// lookups SKIP past tombstones (keep probing), inserts may REUSE one.
//
// RESIZE: when (len + tombstones) gets too dense (load factor ~0.75), allocate a
// bigger table and RE-INSERT every live entry (which also clears tombstones).
//
// Implement the 5 TODO methods. Bounds: K: Hash + Eq. Use the default hasher via
// `std::collections::hash_map::DefaultHasher` to turn a key into a u64.
use std::collections::hash_map::DefaultHasher;

enum Slot<K, V> {
    Empty,
    Deleted, // tombstone
    Occupied(K, V),
}

struct MyHashMap<K, V> {
    slots: Vec<Slot<K, V>>,
    len: usize,        // number of Occupied slots
    tombstones: usize, // number of Deleted slots
}

impl<K: Hash + Eq, V> MyHashMap<K, V> {
    fn new() -> Self {
        // start with a small power-of-two capacity (e.g. 8)
        let mut slots = Vec::new();
        slots.resize_with(8, || Slot::Empty);
        MyHashMap {
            slots,
            len: 0,
            tombstones: 0,
        }
    }

    fn len(&self) -> usize {
        self.len
    }

    // Hash a key to a bucket index in [0, capacity).
    fn home(&self, key: &K) -> usize {
        let mut h = DefaultHasher::new();
        key.hash(&mut h);
        (h.finish() as usize) % self.slots.len()
    }

    // Insert or overwrite. Return the OLD value if the key was already present.
    // Steps: maybe resize first; then linear-probe from home(); on Empty OR a
    // reusable Deleted slot, place the entry (len += 1); on a matching Occupied
    // key, swap in the new value and return the old one.
    // (Tip: remember the FIRST tombstone you pass so you can place there if the
    //  key turns out to be absent — but you must still scan ahead to confirm the
    //  key isn't present further down the chain before reusing it.)
    fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.resize_if_needed();

        let mut first_deleted = None;
        let mut i = self.home(&key);

        loop {
            match &mut self.slots[i] {
                Slot::Occupied(k, v) if k == &key => {
                    return Some(std::mem::replace(v, value));
                }
                Slot::Deleted => {
                    if first_deleted.is_none() {
                        first_deleted = Some(i);
                    }
                }
                Slot::Empty => {
                    let target = first_deleted.unwrap_or(i);
                    if first_deleted.is_some() {
                        self.tombstones -= 1;
                    }
                    self.slots[target] = Slot::Occupied(key, value);
                    self.len += 1;
                    return None;
                }
                Slot::Occupied(_, _) => {}
            }

            i = (i + 1) % self.slots.len();
        }
    }

    // Look up a value. Probe from home(); skip Deleted; stop at Empty (-> None);
    // return Some(&v) on a key match.
    fn get(&self, key: &K) -> Option<&V> {
        let mut i = self.home(key);

        loop {
            match &self.slots[i] {
                Slot::Empty => return None,
                Slot::Deleted => {}
                Slot::Occupied(k, v) if k == key => return Some(v),
                Slot::Occupied(_, _) => {}
            }

            i = (i + 1) % self.slots.len();
        }
    }

    // Remove a key. Probe like get; on a match, replace the slot with Deleted,
    // decrement len, increment tombstones, return the old value. Else None.
    fn remove(&mut self, key: &K) -> Option<V> {
        let mut i = self.home(key);

        loop {
            match &self.slots[i] {
                Slot::Empty => return None,
                Slot::Deleted => {}
                Slot::Occupied(k, _) if k == key => {
                    let old_slot = std::mem::replace(&mut self.slots[i], Slot::Deleted);
                    self.len -= 1;
                    self.tombstones += 1;

                    if let Slot::Occupied(_, value) = old_slot {
                        return Some(value);
                    }
                    unreachable!("matched an occupied slot before replacing it")
                }
                Slot::Occupied(_, _) => {}
            }

            i = (i + 1) % self.slots.len();
        }
    }

    // Grow when dense. If (len + tombstones) * 4 >= capacity * 3 (load factor
    // 0.75), build a new table of double capacity and RE-INSERT live entries only
    // (drop tombstones). Call this at the TOP of insert.
    fn resize_if_needed(&mut self) {
        if (self.len + self.tombstones) * 4 < self.slots.len() * 3 {
            return;
        }

        let new_capacity = self.slots.len() * 2;
        let mut new_slots = Vec::new();
        new_slots.resize_with(new_capacity, || Slot::Empty);

        let old_slots = std::mem::replace(&mut self.slots, new_slots);
        self.len = 0;
        self.tombstones = 0;

        for slot in old_slots {
            if let Slot::Occupied(key, value) = slot {
                self.insert(key, value);
            }
        }
    }
}

fn check_9() {
    let mut m: MyHashMap<String, i32> = MyHashMap::new();
    assert_eq!(m.get(&"x".to_string()), None);

    // basic insert / get / overwrite
    assert_eq!(m.insert("a".into(), 1), None); // new key -> None
    assert_eq!(m.insert("b".into(), 2), None);
    assert_eq!(m.get(&"a".into()), Some(&1));
    assert_eq!(m.insert("a".into(), 10), Some(1)); // overwrite -> old value
    assert_eq!(m.get(&"a".into()), Some(&10));
    assert_eq!(m.len(), 2);

    // remove leaves a tombstone but keeps later keys reachable
    assert_eq!(m.remove(&"a".into()), Some(10));
    assert_eq!(m.get(&"a".into()), None);
    assert_eq!(m.get(&"b".into()), Some(&2)); // still found across the tombstone
    assert_eq!(m.remove(&"a".into()), None); // already gone
    assert_eq!(m.len(), 1);

    // force several resizes; everything must survive the rehash
    for i in 0..100 {
        assert_eq!(m.insert(format!("k{i}"), i), None);
    }
    assert_eq!(m.len(), 101); // 100 new + "b"
    for i in 0..100 {
        assert_eq!(m.get(&format!("k{i}")), Some(&i));
    }
    assert_eq!(m.get(&"b".into()), Some(&2));

    // overwrite after growth still returns old, doesn't change len
    let before = m.len();
    assert_eq!(m.insert("k50".into(), 5000), Some(50));
    assert_eq!(m.len(), before);
    assert_eq!(m.get(&"k50".into()), Some(&5000));

    println!("rung 9 ok: MyHashMap — linear probing, tombstones, resize. Capstone done. 🏔️");
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
    println!("all checks passed ✅");
}
