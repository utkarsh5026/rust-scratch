# Strings & text

> Ladder: [`src/bin/strings_text.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/strings_text.rs) ·
> Run: `cargo run --bin strings_text` · Phase 3 · 9 rungs

## TL;DR

Rust's string zoo is two independent axes multiplied together:

- **Owned vs borrowed.** `String` / `OsString` / `PathBuf` / `CString` own a heap
  buffer. `&str` / `&OsStr` / `&Path` / `&CStr` are *views* — unsized, always behind
  a reference.
- **What the bytes promise.** `str` = valid UTF-8. `OsStr` = whatever the OS uses
  (UTF-8 *not* guaranteed). `CStr` = NUL-terminated, no interior NUL. `[u8]` = no
  promises at all.

The one sentence that unlocks everything: **`String` is `Vec<u8>` + the UTF-8
invariant; `&str` is `&[u8]` + that same invariant.** Every footgun in this topic
is about respecting that invariant, and every conversion method is a gatekeeper for
crossing it.

## Why this exists (from first principles)

Why not have one string type? Because "text" means different things at different
boundaries, and each boundary enforces a different guarantee:

- Your program logic wants **valid Unicode** so iteration, comparison, and display
  behave. That is `str`.
- The **operating system** predates Unicode. A Unix filename is any byte sequence
  except `NUL` and `/`; a Windows path is UTF-16 that may contain unpaired
  surrogates. Neither is guaranteed to be valid UTF-8, so forcing it into `str`
  would either lose data or panic. That is `OsStr`.
- **C** has no length field — a string is "bytes up to the first `NUL`". To hand a
  string to C you must guarantee a terminator *and* no interior `NUL`. That is
  `CStr` / `CString`.
- **Filesystem paths** are `OsStr` plus structure (separators, components,
  extensions). That is `Path`.

If these were all one type, the compiler couldn't stop you from, say, passing a
non-UTF-8 filename where UTF-8 is required, or building a C string with an interior
`NUL` that silently truncates. Separate types turn those bugs into *compile errors*
or *explicit `Result`s at the conversion point*.

## The ladder at a glance

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | `str` vs `String` | owned heap buffer vs borrowed view; take `&str` to accept both |
| 2 | foundations | UTF-8 invariant | `len()` is *bytes*; `bytes` / `chars` / `char_indices` |
| 3 | footgun | slicing bites | `&s[a..b]` panics mid-codepoint; `get` / `is_char_boundary` |
| 4 | mechanics | zero-copy parsing | `lines` / `split_once` / `trim` / `parse` return borrowed slices |
| 5 | real-world | `OsStr` / `OsString` | OS text isn't UTF-8; `to_str` → `Option`, `to_string_lossy` → `Cow` |
| 6 | real-world | `Path` / `PathBuf` | structured paths over string concat |
| 7 | real-world | `CStr` / `CString` | NUL-terminated FFI; interior-NUL is an error |
| 8 | real-world | conversions & validation | `String ↔ Vec<u8>`; `from_utf8` Result vs lossy Cow |
| 9 | capstone | hand-rolled UTF-8 decoder | reimplement `str::chars()` from raw bytes |

## The ideas, built up

### 1. `str` is a view; `String` owns the buffer

`String` owns a growable heap allocation. `&str` is a borrowed window into UTF-8
bytes — a string literal lives in the binary's read-only data, and slicing a
`String` produces a `&str` pointing into its buffer. `&str` is *unsized*, so you
only ever hold it behind a reference.

The idiomatic consequence: **take `&str` as a parameter, not `&String`.** Deref
coercion turns `&String` into `&str` automatically, so one signature accepts both an
owned string and a literal, with no clone:

```rust
fn shout(s: &str) -> String {
    format!("{}!", s.to_uppercase())
}

let owned = String::from("hello");
shout(&owned);    // &String coerces to &str
shout("world");   // literal &str
// `owned` is still usable here — we only borrowed it.
```

Note `to_uppercase()` *returns* a new `String` rather than mutating in place: case
mapping can change the byte length (e.g. `ß` → `SS`), so it cannot be done within
the original buffer.

### 2. The UTF-8 invariant: bytes are not characters

UTF-8 encodes one `char` (a Unicode scalar value) in **1 to 4 bytes**. This single
fact is the source of every surprise:

```rust
fn analyze(s: &str) -> StrStats {
    StrStats {
        byte_len: s.len(),                                  // BYTES, not chars
        char_count: s.chars().count(),                      // decoded scalars
        last_char_offset: s.char_indices().last().map(|(i, _)| i),
    }
}
```

- `s.len()` is the number of **bytes**. `"café".len()` is `5`, not `4` (the `é` is
  two bytes, `0xC3 0xA9`). `"日本語".len()` is `9` (3 bytes each).
- `s.bytes()` yields the raw `u8` encoding.
- `s.chars()` yields decoded `char`s.
- `s.char_indices()` yields `(byte_offset, char)` — the byte where each char
  *starts*. Offsets jump by 1–4, not always by 1.

`char_indices().last().map(|(i, _)| i)` gives the byte offset of the final char and
returns `None` for an empty string for free — `.last()` on an empty iterator is
`None`.

### 3. Slicing bites: byte ranges must hit char boundaries

There is **no `s[i]` char indexing** in Rust. You slice by a *byte range*
`&s[a..b]` — and the endpoints must fall on UTF-8 char boundaries, or it **panics
at runtime**: `&"café"[0..4]` splits the `é` and dies with "byte index 4 is not a
char boundary".

Two tools make slicing safe:

```rust
// OK: non-panicking slice — returns None for out-of-bounds OR mid-codepoint.
fn safe_slice(s: &str, a: usize, b: usize) -> Option<&str> {
    s.get(a..b)
}

// OK: drop the first char, multibyte-aware.
fn behead(s: &str) -> &str {
    match s.chars().next() {
        Some(first) => &s[first.len_utf8()..],
        None => "",
    }
}
```

`s.get(a..b)` is the fallible twin of `&s[a..b]`: same checks, but `None` instead of
a panic. `s.is_char_boundary(i)` answers the boundary question directly.

`behead` shows the subtle point: `&s[1..]` would panic on `"日本"` (first char is 3
bytes), but `&s[first.len_utf8()..]` is safe *even though it's a raw slice* —
`len_utf8()` is exactly the byte width of that first char, so the start index is
guaranteed to land on the next char's boundary. You can use the panicking slice when
you can *prove* the index is a boundary.

### 4. Zero-copy parsing: split/trim/parse return borrowed slices

The everyday string toolkit — `lines`, `trim`, `split_once`, `strip_prefix` — all
return `&str` slices that **borrow the original buffer**. No allocation, no copy.
Only `parse()` produces an owned value.

```rust
fn parse_config(input: &str) -> Vec<(&str, i64)> {
    input
        .lines()
        .filter(|line| !line.is_empty() && !line.trim().starts_with('#'))
        .filter_map(|line| {
            line.split_once('=').map(|(key, value)| {
                (key.trim(), value.trim().parse::<i64>().unwrap())
            })
        })
        .collect()
}
```

The returned `&str` keys point *into* `input` — the elided lifetime ties the output
to the input. You can prove it: `key.as_ptr()` lands inside `input`'s buffer range.
`split_once('=')` splits on the first match and returns `Option<(&str, &str)>`, so a
malformed line (no `=`) becomes `None` and `filter_map` drops it automatically.

### 5. `OsStr` / `OsString`: the OS doesn't promise UTF-8

`std::env::args_os()`, `Path::file_name()`, environment variables — these hand you
`OsStr` / `OsString`, *not* `str`, because the OS may give you bytes that aren't
valid UTF-8. Crossing `OsStr` → `str` can fail, and the API forces you to choose how
to handle that:

```rust
fn describe_os(os: &OsStr) -> String {
    match os.to_str() {                          // -> Option<&str>: None if not UTF-8
        Some(s) => format!("utf8: {s}"),
        None => format!("lossy: {}", os.to_string_lossy()),
    }
}

fn is_cow_borrowed(os: &OsStr) -> bool {
    matches!(os.to_string_lossy(), Cow::Borrowed(_))
}
```

- `to_str()` → `Option<&str>`: `None` when the bytes aren't valid UTF-8.
- `to_string_lossy()` → `Cow<str>`: never fails — replaces bad bytes with the
  replacement char `U+FFFD` (`�`). It returns `Cow::Borrowed` when the input was
  *already* valid UTF-8 (zero-copy) and `Cow::Owned` only when it had to allocate to
  substitute. That `Borrowed`-vs-`Owned` distinction is a free "did anything go
  wrong" signal.

> The lesson: there is **no infallible `OsStr` → `str`**. The type system makes you
> pick a strategy (fail with `to_str`, or substitute with `to_string_lossy`).

The ladder forges an invalid `OsStr` on Unix via `OsStrExt::from_bytes(&[b'a',
0xFF, b'b'])` to exercise the failure path — `0xFF` can never begin a UTF-8
sequence.

### 6. `Path` / `PathBuf`: structure, not string concatenation

`Path` / `PathBuf` wrap an `OsStr` (so they inherit "maybe not UTF-8") and add
filesystem *semantics*. The senior rule: **never build paths with
`format!("{dir}/{file}")`** — it breaks on Windows, doubles separators, and mishandles
edge cases. Use the structured API:

```rust
fn swap_extension(path: &Path, new_ext: &str) -> PathBuf {
    let mut p = path.to_path_buf();
    p.set_extension(new_ext);   // replaces, or adds if none
    p
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()                       // Option<&OsStr> (None for "..")
        .and_then(|os| os.to_str())        // Option<&str>   (None if not UTF-8)
        .map_or(false, |s| s.starts_with('.'))
}
```

Key methods: `Path::new`, `join` (handles separators), `file_name`, `extension`,
`file_stem`, `parent`, `components`. The structured accessors return `&OsStr`, which
compares directly against `&str` literals (`p.extension().unwrap() == "txt"`).

`is_hidden` is a clean two-layer `Option` chain: a path ending in `..` has no
`file_name()`, and a non-UTF-8 name fails `to_str()` — both short-circuit to
`false`.

### 7. `CStr` / `CString`: the FFI string

C strings are NUL-terminated with **no interior NUL** — the string is "everything up
to the first `\0`". Rust's `str` / `String` store a *length* instead and carry no
terminator, so you must convert at the C boundary.

```rust
fn to_c(s: &str) -> Result<CString, NulError> {
    CString::new(s)            // Err if `s` contains an interior NUL
}

fn c_len(s: &str) -> Option<usize> {
    let cs = CString::new(s).ok()?;
    Some(cs.as_bytes().len())  // EXCLUDES the trailing NUL
}

fn from_c_bytes(buf: &[u8]) -> Option<String> {
    CStr::from_bytes_until_nul(buf)         // read up to the first NUL
        .ok()
        .and_then(|cstr| cstr.to_str().ok().map(|s| s.to_owned()))
}
```

The defining footgun: `CString::new` *must* fail on an interior `NUL`, because
otherwise C would see a truncated string. The `Result` return type is the type
system enforcing that.

Two byte views to keep straight:

- `as_bytes()` — the content **without** the terminator (`b"hello"`).
- `as_bytes_with_nul()` — content **including** it (`b"hello\0"`).

Receiving from C: `CStr::from_bytes_until_nul(b"hello\0garbage")` reads `"hello"` and
ignores the rest; with no `NUL` at all it returns an error (`None` after `.ok()`).

### 8. Conversions & validation: validate on the way back

This rung ties the topic together. Going **to** bytes is free and infallible — the
UTF-8 invariant only loosens. Coming **back** must *validate*, which is precisely why
those functions return `Result` (or substitute via lossy).

```rust
// text -> bytes: free
s.as_bytes();      // &str -> &[u8]   (borrowed)
s.into_bytes();    // String -> Vec<u8> (just unwraps the Vec)

// bytes -> text: must validate
fn decode_strict(bytes: &[u8]) -> Result<String, std::str::Utf8Error> {
    std::str::from_utf8(bytes).map(|s| s.to_owned())
}

fn decode_lossy(bytes: &[u8]) -> (String, bool) {
    let cow = String::from_utf8_lossy(bytes);
    (cow.to_string(), matches!(cow, Cow::Owned(_)))  // Owned == it substituted
}
```

The map of conversions:

| From | To | Method | Fallible? |
|------|----|--------|-----------|
| `&str` | `&[u8]` | `as_bytes()` | no (free, borrowed) |
| `String` | `Vec<u8>` | `into_bytes()` | no (free) |
| `&[u8]` | `&str` | `str::from_utf8` | `Result<_, Utf8Error>` (borrowed) |
| `Vec<u8>` | `String` | `String::from_utf8` | `Result<_, FromUtf8Error>` (owned) |
| `&[u8]` | `Cow<str>` | `String::from_utf8_lossy` | never (substitutes `�`) |

Use the *borrowing* `str::from_utf8` when the caller should keep ownership of the
bytes; use `String::from_utf8` when you already own a `Vec<u8>` and want to consume
it. `decode_lossy` reads the `Cow` variant to report whether substitution happened —
no second scan for `�` needed.

## Footguns

| Trap | What happens | Fix |
|------|--------------|-----|
| `s.len()` as "number of characters" | counts bytes; off for any non-ASCII | `s.chars().count()` |
| `&s[0..n]` mid-codepoint | runtime panic | `s.get(0..n)`, check `is_char_boundary`, or slice on a known boundary like `len_utf8()` |
| `s[i]` char indexing | does not compile | iterate `chars()` / `char_indices()` |
| Forcing a filename into `String` | data loss or panic on non-UTF-8 | keep it `OsStr`; `to_str()`/`to_string_lossy()` at the edge |
| `format!("{dir}/{file}")` | breaks cross-platform | `Path::join` / `set_extension` |
| `CString::new` with interior `NUL` | returns `Err` (would truncate in C) | handle the `Result`; never `.unwrap()` on untrusted input |
| `String::from_utf8` on arbitrary bytes | `Err` on invalid UTF-8 | `from_utf8` (handle Result) or `from_utf8_lossy` |

## Real-world patterns

- **Accept `&str`, store `String`.** APIs take `&str` (or `impl AsRef<str>`) for
  flexibility and own a `String` internally.
- **`Cow<str>` for "usually borrowed, sometimes owned."** `to_string_lossy`,
  `from_utf8_lossy`, and many parsers return `Cow` so the common (clean) case is
  zero-copy and only the exceptional case allocates. (See the dedicated `Cow` note.)
- **Stay in `OsStr`/`Path` as long as possible.** Convert to `str` only at the
  boundary where you genuinely need UTF-8 (logging, display, parsing), and decide
  there how to handle non-UTF-8.
- **`CString` lives as long as the C call.** Hold the `CString` in a binding while C
  borrows its pointer; if it drops first, the pointer dangles.

## Capstone insight

The capstone reimplements `str::chars()` from raw bytes, which forces you to *own*
the UTF-8 encoding rather than trust it:

```rust
fn decode_utf8(bytes: &[u8]) -> Option<(char, usize)> {
    let lead = *bytes.first()?;
    let length = match lead {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,   // note: starts at C2, not C0
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,   // note: ends at F4, not F7
        _ => return None,   // continuation byte or invalid lead
    };
    if bytes.len() < length {
        return None;        // truncated
    }
    let mut cp = match length {
        1 => u32::from(lead),
        2 => u32::from(lead & 0b0001_1111),
        3 => u32::from(lead & 0b0000_1111),
        4 => u32::from(lead & 0b0000_0111),
        _ => unreachable!(),
    };
    for &b in &bytes[1..length] {
        if b & 0b1100_0000 != 0b1000_0000 {   // must be 10xxxxxx
            return None;
        }
        cp = (cp << 6) | u32::from(b & 0b0011_1111);
    }
    char::from_u32(cp).map(|ch| (ch, length))
}
```

The encoding, made explicit:

| Bytes | Lead pattern | Payload bits | Code point range |
|-------|--------------|--------------|------------------|
| 1 | `0xxxxxxx` | 7 | `U+0000..U+007F` |
| 2 | `110xxxxx` | 5 + 6 = 11 | `U+0080..U+07FF` |
| 3 | `1110xxxx` | 4 + 12 = 16 | `U+0800..U+FFFF` |
| 4 | `11110xxx` | 3 + 18 = 21 | `U+10000..U+10FFFF` |

Three layers of validation fall out naturally:

1. The lead-byte ranges already exclude the **overlong** 2-byte encodings (`0xC0`/
   `0xC1`) and 4-byte leads beyond `U+10FFFF` (`0xF5..`).
2. Each continuation byte is checked against `10xxxxxx`.
3. `char::from_u32` rejects surrogates (`U+D800..U+DFFF`) and out-of-range values —
   the final gate that *defines* a valid Unicode scalar.

Wrapping it in an iterator is then trivial — and the `?` gives you "stop at end of
input **or** the first invalid byte" in one line:

```rust
impl<'a> Iterator for Utf8Chars<'a> {
    type Item = char;
    fn next(&mut self) -> Option<char> {
        let (ch, n) = decode_utf8(&self.bytes[self.pos..])?;
        self.pos += n;
        Some(ch)
    }
}
```

On valid input it yields exactly what `str::chars()` does — proving the mental model
end to end. (The one gap a fully conformant decoder closes that this one doesn't:
rejecting overlong 3- and 4-byte encodings, which requires a per-length minimum
code-point check.)

## Explain it back

- Why does `"café".len()` return `5`? What returns `4`?
- When does `&s[a..b]` panic, and what are the two safe alternatives?
- Why can't a filename always be a `String`? What two methods cross `OsStr` → `str`,
  and how do they differ?
- Why does `to_string_lossy` return a `Cow`? What does `Cow::Borrowed` tell you?
- Why does `CString::new` return a `Result`? What breaks if it didn't?
- Which direction (text → bytes or bytes → text) is fallible, and why?
- In `decode_utf8`, why does the 2-byte lead range start at `0xC2` instead of
  `0xC0`?

## See also

- [`Cow` (Clone-on-Write)](cow.md) — the return type of the lossy conversions.
- [`Borrow` / `ToOwned`](borrow-toowned.md) — the borrowed/owned pairing (`str` ↔
  `String`) generalized.
- [Collections deep-dive](collections.md) — `HashMap` key lookup via `Borrow<str>`,
  another place the view/owned split shows up.
