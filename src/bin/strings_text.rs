//! Strings & text — `str`/`String`/`OsStr`/`CStr`/`Path`, UTF-8 invariants.
//!
//! Run: `cargo run --bin strings_text`
//!
//! Mental model: two axes.
//!   owned vs borrowed:  String / OsString / PathBuf / CString   own a buffer
//!                       &str  / &OsStr   / &Path   / &CStr      are views
//!   what the bytes promise:  str = valid UTF-8,  OsStr = OS-native (maybe not
//!                            UTF-8),  CStr = NUL-terminated no interior NUL,
//!                            [u8] = nothing.
//!   And: `String` is `Vec<u8>` + UTF-8 invariant; `&str` is `&[u8]` + same.
//!
//! Ladder:
//!   1. str vs String — owned heap buffer vs borrowed view        [foundations] DONE
//!   2. UTF-8 invariant — len() is bytes; bytes/chars/char_indices [foundations] DONE
//!   3. slicing bites — panic mid-codepoint; is_char_boundary/get  [footgun] DONE
//!   4. zero-copy parsing — split/trim/parse return &str slices    [mechanics] DONE
//!   5. OsStr/OsString — non-UTF-8 filenames; to_str/lossy         [real-world] DONE
//!   6. Path/PathBuf — extension/file_name/join/components         [real-world] DONE
//!   7. CStr/CString — NUL-terminated FFI; interior-NUL error      [real-world] DONE
//!   8. conversions & validation — String<->Vec<u8>, from_utf8     [real-world] DONE
//!   9. capstone — hand-rolled UTF-8 decoder + Utf8Chars iterator  [capstone] <- YOU ARE HERE

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

// ---------------------------------------------------------------------------
// Rung 1 — str vs String
//
// `String` owns a growable heap buffer. `&str` is a borrowed view into UTF-8
// bytes (a literal lives in the binary; a slice of a String points into it).
//
// The idiomatic rule: take `&str` as an argument so callers can pass BOTH a
// `&String` (auto-derefs to `&str`) and a string literal `&str` for free.
//
// Implement `shout`: take a borrowed string view and return a NEW owned String
// that is the input uppercased with "!" appended.
//   shout("hi")  ->  "HI!"
// It must accept both a literal and a &String without the caller cloning.
// ---------------------------------------------------------------------------

fn shout(s: &str) -> String {
    format!("{}!", s.to_uppercase())
}

fn check_1() {
    let owned: String = String::from("hello");
    // Passing &String where &str is wanted — deref coercion does the work.
    assert_eq!(shout(&owned), "HELLO!");
    // Passing a literal &str.
    assert_eq!(shout("world"), "WORLD!");
    // `owned` is still usable: we only borrowed it.
    assert_eq!(owned, "hello");
    println!("rung 1 ✅  str (view) vs String (owned)");
}

// ---------------------------------------------------------------------------
// Rung 2 — the UTF-8 invariant: bytes vs chars vs char_indices
//
// `str` is UTF-8, so a single `char` (a Unicode scalar value) may take 1–4
// BYTES. This is the source of every surprise in this topic:
//   - `s.len()` is the number of BYTES, not characters.
//   - `s.bytes()`        yields u8   (the raw encoding)
//   - `s.chars()`        yields char (decoded scalar values)
//   - `s.char_indices()` yields (byte_offset, char) — the byte where each
//                        char starts. Offsets jump by 1–4, not always by 1.
//
// Implement `analyze(s) -> StrStats`:
//   byte_len  = number of bytes              (hint: .len())
//   char_count= number of chars              (hint: .chars().count())
//   last_char_offset = the BYTE OFFSET at which the LAST char begins
//                      (None if the string is empty)
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
struct StrStats {
    byte_len: usize,
    char_count: usize,
    last_char_offset: Option<usize>,
}

fn analyze(s: &str) -> StrStats {
    let len = s.len();
    let char_count = s.chars().count();
    let last_char_offset = s.char_indices().last().map(|(i, _)| i);
    StrStats {
        byte_len: len,
        char_count,
        last_char_offset,
    }
}

fn check_2() {
    // "café" — the é is U+00E9, encoded as 2 bytes (0xC3 0xA9).
    // bytes: c a f [é é]  -> 5 bytes, but 4 chars.
    let s = "café";
    let stats = analyze(s);
    assert_eq!(
        stats,
        StrStats {
            byte_len: 5,
            char_count: 4,
            last_char_offset: Some(3)
        }
    );

    // "日本語" — three CJK chars, 3 bytes each = 9 bytes, last starts at 6.
    let jp = "日本語";
    assert_eq!(
        analyze(jp),
        StrStats {
            byte_len: 9,
            char_count: 3,
            last_char_offset: Some(6)
        }
    );

    // ASCII only: bytes == chars, last char at len-1.
    assert_eq!(
        analyze("abc"),
        StrStats {
            byte_len: 3,
            char_count: 3,
            last_char_offset: Some(2)
        }
    );

    // Empty: no last char.
    assert_eq!(
        analyze(""),
        StrStats {
            byte_len: 0,
            char_count: 0,
            last_char_offset: None
        }
    );
    println!("rung 2 ✅  len() is bytes; chars/char_indices decode UTF-8");
}

// ---------------------------------------------------------------------------
// Rung 3 — slicing bites: byte ranges must land on char boundaries
//
// You can't index a string by char: `s[2]` does NOT compile. You slice by a
// BYTE RANGE: `&s[a..b]`. The catch — the range ends MUST fall on UTF-8 char
// boundaries, or you get a runtime PANIC ("byte index N is not a char
// boundary"). `&"café"[0..4]` panics: byte 4 is the middle of `é`.
//
// Two tools to slice safely:
//   - `s.is_char_boundary(i)` -> bool : is byte index i a valid boundary?
//   - `s.get(a..b)`           -> Option<&str> : the non-panicking slice
//                               (returns None instead of panicking)
//
// Part A — implement `safe_slice(s, a, b) -> Option<&str>`:
//   return the substring of bytes [a, b) ONLY if it's a valid slice
//   (in bounds AND on char boundaries). Otherwise None. No panics ever.
//
// Part B — implement `behead(s) -> &str`:
//   drop the FIRST char and return the rest as a borrowed &str. Must work for
//   multibyte first chars (so you can't just do &s[1..]). Empty -> "".
//   (hint: how many bytes does the first char occupy? .chars().next() knows
//    its .len_utf8(); or char_indices gives you the offset of the 2nd char.)
//
// Note both return &str borrowing FROM the input — slicing never copies.
// ---------------------------------------------------------------------------

fn safe_slice(s: &str, a: usize, b: usize) -> Option<&str> {
    s.get(a..b)
}

fn behead(s: &str) -> &str {
    match s.chars().next() {
        Some(first_char) => &s[first_char.len_utf8()..],
        None => "",
    }
}

fn check_3() {
    let s = "café"; // bytes: [c|a|f|é é]  indices 0,1,2,3,(4 is mid-é),5=end

    // Valid boundary slices.
    assert_eq!(safe_slice(s, 0, 3), Some("caf"));
    assert_eq!(safe_slice(s, 0, 5), Some("café"));
    assert_eq!(safe_slice(s, 3, 5), Some("é"));
    // Byte 4 splits the é -> not a boundary -> None (NOT a panic).
    assert_eq!(safe_slice(s, 0, 4), None);
    // Out of bounds -> None.
    assert_eq!(safe_slice(s, 0, 99), None);

    // Sanity: the raw slice that safe_slice rejects really would panic.
    assert!(!s.is_char_boundary(4));

    // behead drops the first char, multibyte-aware.
    assert_eq!(behead("café"), "afé");
    assert_eq!(behead("日本"), "本"); // first char is 3 bytes
    assert_eq!(behead("x"), "");
    assert_eq!(behead(""), "");
    println!("rung 3 ✅  slice by byte range, but only on char boundaries");
}

// ---------------------------------------------------------------------------
// Rung 4 — zero-copy parsing: split / trim / parse return borrowed slices
//
// The everyday string-processing toolkit. Crucial point: `split`, `trim`,
// `splitn`, `lines`, `strip_prefix` etc. return `&str` slices that BORROW the
// original buffer — no allocation, no copy. `parse::<T>()` turns a &str into a
// real value via FromStr, returning Result.
//
// Implement `parse_config(input) -> Vec<(&str, i64)>`:
//   The input is INI-like, one entry per line:
//       width = 80
//       # this is a comment, skip it
//          height=24       <- note ragged whitespace
//
//       depth = 7          <- skip blank lines too
//   Rules:
//     - split into lines
//     - skip blank lines and lines that (after trimming) start with '#'
//     - split each entry on the FIRST '=' into key and value
//     - TRIM whitespace off both key and value
//     - parse the value as i64
//     - the returned &str keys must BORROW from `input` (lifetime elision ties
//       the output to the input — that's the zero-copy part)
//   You may assume every non-comment, non-blank line is well-formed.
//
//   Helpful methods: .lines(), .trim(), .starts_with('#'), .is_empty(),
//   .split_once('=')  (splits on the first match -> Option<(&str,&str)>),
//   .parse::<i64>()   (Result; .unwrap() is fine here).
// ---------------------------------------------------------------------------

fn parse_config(input: &str) -> Vec<(&str, i64)> {
    input
        .lines()
        .filter(|line| !line.is_empty() && !line.trim().starts_with('#'))
        .filter_map(|line| {
            line.split_once('=').map(|(key, value)| {
                let key = key.trim();
                let value = value.trim();
                (key, value.parse::<i64>().unwrap())
            })
        })
        .collect::<Vec<_>>()
}

fn check_4() {
    let input = "\
width = 80
# a comment
   height=24

depth = 7
";
    let cfg = parse_config(input);
    assert_eq!(cfg, vec![("width", 80), ("height", 24), ("depth", 7)]);

    let (k0, _) = cfg[0];
    let base = input.as_ptr() as usize;
    let kptr = k0.as_ptr() as usize;
    assert!(
        kptr >= base && kptr < base + input.len(),
        "key should point INTO the input buffer (zero-copy)"
    );

    println!("rung 4 ✅  split/trim/parse — borrowed &str slices, no copies");
}

// ---------------------------------------------------------------------------
// Rung 5 — OsStr / OsString: the OS doesn't promise UTF-8
//
// `str` is ALWAYS valid UTF-8. But the operating system disagrees: on Unix a
// filename is an arbitrary sequence of bytes (anything but NUL and '/'); on
// Windows it's UTF-16 that may contain unpaired surrogates. Neither is
// guaranteed to be valid UTF-8. So std uses a SEPARATE type for OS-provided
// text: `OsStr` (borrowed) / `OsString` (owned). `std::env::args_os()`,
// `Path::file_name()`, env vars, etc. hand you these.
//
// Crossing OsStr -> str can FAIL, so the API forces you to acknowledge it:
//   - os.to_str()           -> Option<&str>   (None if not valid UTF-8)
//   - os.to_string_lossy()  -> Cow<str>       (replaces bad bytes with U+FFFD
//                              '�'; Borrowed if already UTF-8 = zero-copy,
//                              Owned only if it had to substitute)
//
// Implement `describe_os(os: &OsStr) -> String`:
//   - if it IS valid UTF-8, return  format!("utf8: {s}")
//   - if it is NOT, return          format!("lossy: {s}")  using the lossy form
// The point: you must go through to_str()/to_string_lossy() — there is no
// infallible OsStr -> str. (Use to_str() to branch; lossy for the fallback.)
//
// Then `is_cow_borrowed(os) -> bool`: return true iff to_string_lossy()
// returned a Cow::Borrowed (i.e. no replacement was needed — zero-copy view).
// ---------------------------------------------------------------------------

use std::borrow::Cow;
use std::ffi::OsStr;

fn describe_os(os: &OsStr) -> String {
    match os.to_str() {
        Some(s) => format!("utf8: {s}"),
        None => format!("lossy: {}", os.to_string_lossy()),
    }
}

fn is_cow_borrowed(os: &OsStr) -> bool {
    matches!(os.to_string_lossy(), Cow::Borrowed(_))
}

fn check_5() {
    // A normal, valid-UTF-8 OsStr (most filenames you'll meet).
    let good: &OsStr = OsStr::new("photo.png");
    assert_eq!(describe_os(good), "utf8: photo.png");
    assert!(
        is_cow_borrowed(good),
        "valid UTF-8 should be a zero-copy Borrowed Cow"
    );

    // Now forge an OsStr with INVALID UTF-8 bytes (0xFF can't start a UTF-8
    // sequence). On Unix, OsStr is just bytes, so we can build one directly.
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let raw = [b'a', 0xFF, b'b']; // 0xFF is not valid UTF-8
        let bad: &OsStr = OsStr::from_bytes(&raw);

        assert_eq!(bad.to_str(), None, "0xFF is not valid UTF-8");
        // lossy turns 0xFF into the replacement char '�' (U+FFFD).
        assert_eq!(describe_os(bad), "lossy: a\u{FFFD}b");
        assert!(!is_cow_borrowed(bad), "invalid bytes force an Owned Cow");
    }

    println!("rung 5 ✅  OsStr: OS text isn't guaranteed UTF-8 (to_str/lossy)");
}

// ---------------------------------------------------------------------------
// Rung 6 — Path / PathBuf: structured filesystem paths, not strings
//
// `Path` (borrowed) / `PathBuf` (owned) wrap an `OsStr` — so they share the
// "not guaranteed UTF-8" property, but add filesystem SEMANTICS. The rule a
// senior Rustacean follows: NEVER build paths with string concatenation
// (`format!("{dir}/{file}")` breaks on Windows, double-separators, etc.).
// Use the structured API:
//   - Path::new(s)            borrow a &str/&OsStr as a &Path
//   - p.join(x)               append a component -> PathBuf (handles separators)
//   - p.file_name()           -> Option<&OsStr>  (last component)
//   - p.extension()           -> Option<&OsStr>  (after the final '.', no dot)
//   - p.file_stem()           -> Option<&OsStr>  (file_name minus extension)
//   - p.parent()              -> Option<&Path>   (everything but last component)
//   - p.components()          iterate the structured parts
//
// Implement `swap_extension(path, new_ext) -> PathBuf`:
//   return a path identical to `path` but with its extension replaced by
//   `new_ext`. "docs/report.txt" + "md" -> "docs/report.md".
//   If there's no extension, ADD one: "README" + "md" -> "README.md".
//   (hint: PathBuf has a `set_extension` method that does exactly this — start
//    from an owned copy via `path.to_path_buf()`.)
//
// Implement `is_hidden(path) -> bool`:
//   true iff the FILE NAME (last component) starts with a '.'  e.g. ".bashrc".
//   "/home/me/.config" -> true,  "/home/me/notes.txt" -> false.
//   (hint: file_name() -> OsStr -> to_str() -> starts_with('.'). A path ending
//    in ".." has no file_name(), so handle the None case as not-hidden.)
// ---------------------------------------------------------------------------

use std::path::{Path, PathBuf};

fn swap_extension(path: &Path, new_ext: &str) -> PathBuf {
    let mut path_buf = path.to_path_buf();
    path_buf.set_extension(new_ext);
    path_buf
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|os| os.to_str())
        .map_or(false, |s| s.starts_with('.'))
}

fn check_6() {
    // join handles the separator for you — no manual '/' concatenation.
    let p = Path::new("docs").join("report.txt");
    assert_eq!(p, Path::new("docs/report.txt"));

    // Structured accessors return OsStr (compare against OsStr via "...").
    assert_eq!(p.file_name().unwrap(), "report.txt");
    assert_eq!(p.extension().unwrap(), "txt");
    assert_eq!(p.file_stem().unwrap(), "report");
    assert_eq!(p.parent().unwrap(), Path::new("docs"));

    // swap_extension replaces (or adds) the extension.
    assert_eq!(swap_extension(&p, "md"), PathBuf::from("docs/report.md"));
    assert_eq!(
        swap_extension(Path::new("README"), "md"),
        PathBuf::from("README.md")
    );

    // is_hidden looks only at the final component.
    assert!(is_hidden(Path::new("/home/me/.config")));
    assert!(!is_hidden(Path::new("/home/me/notes.txt")));
    assert!(!is_hidden(Path::new("..")));

    println!("rung 6 ✅  Path: structured fs paths over string concat");
}

// ---------------------------------------------------------------------------
// Rung 7 — CStr / CString: the FFI string with a NUL terminator
//
// C strings are NUL-terminated: the string is "everything up to the first
// 0x00 byte", and there must be NO interior NUL. Rust models this with:
//   - CString (owned)  : owns a heap buffer that is guaranteed to END in a
//                        single NUL and contain none in the middle.
//   - CStr (borrowed)  : a view over a NUL-terminated byte run (what you'd get
//                        back from C as a `*const c_char`).
// This is the bridge to C. Rust's `str`/`String` do NOT store a terminator
// (they carry a length), so you must convert at the boundary.
//
// The defining footgun: building a CString from data that contains a 0x00 in
// the MIDDLE must FAIL — otherwise C would see a truncated string.
//   CString::new(bytes) -> Result<CString, NulError>
//
// Part A — `to_c(s: &str) -> Result<CString, NulError>`:
//   wrap a Rust &str as a CString. Forward the error if `s` has an interior
//   NUL. (Literally `CString::new(s)`, but write the signature so you see the
//   Result come through.)
//
// Part B — `c_len(s: &str) -> Option<usize>`:
//   how many BYTES would the C string occupy NOT counting the terminator?
//   = s.len() if the conversion succeeds, None if it has an interior NUL.
//   Build the CString, then use `.as_bytes()` (which EXCLUDES the trailing NUL;
//   `.as_bytes_with_nul()` would include it — verify you understand which).
//
// Part C — `from_c_bytes(buf: &[u8]) -> Option<String>`:
//   simulate receiving bytes from C: `buf` is "hello\0garbage". Read the CStr
//   up to the first NUL with `CStr::from_bytes_until_nul`, then decode to an
//   owned String (assume valid UTF-8 here; .to_str().ok() then .to_owned()).
//   Return None if there's no NUL at all.
// ---------------------------------------------------------------------------

use std::ffi::{CStr, CString, NulError};

fn to_c(s: &str) -> Result<CString, NulError> {
    CString::new(s)
}

fn c_len(s: &str) -> Option<usize> {
    let cstring = CString::new(s).ok()?;
    Some(cstring.as_bytes().len())
}

fn from_c_bytes(buf: &[u8]) -> Option<String> {
    CStr::from_bytes_until_nul(buf)
        .ok()
        .and_then(|cstr| cstr.to_str().ok().map(|s| s.to_owned()))
}

fn check_7() {
    // Normal conversion round-trips, terminator is added for you.
    let cs = to_c("hello").unwrap();
    assert_eq!(cs.as_bytes(), b"hello"); // no trailing NUL here
    assert_eq!(cs.as_bytes_with_nul(), b"hello\0"); // ...but it IS stored

    // Interior NUL is rejected — this is the whole point of the type.
    assert!(to_c("a\0b").is_err());

    // c_len = byte length without the terminator.
    assert_eq!(c_len("hello"), Some(5));
    assert_eq!(c_len("café"), Some(5)); // bytes, not chars (é = 2 bytes)
    assert_eq!(c_len("a\0b"), None);

    // Receiving from C: read up to the first NUL, ignore the rest.
    assert_eq!(from_c_bytes(b"hello\0garbage"), Some("hello".to_string()));
    assert_eq!(from_c_bytes(b"\0"), Some(String::new())); // empty C string
    assert_eq!(from_c_bytes(b"no terminator"), None); // no NUL -> None

    println!("rung 7 ✅  CStr/CString: NUL-terminated, no interior NUL (FFI)");
}

// ---------------------------------------------------------------------------
// Rung 8 — conversions & validation: crossing the UTF-8 boundary explicitly
//
// `String` IS a `Vec<u8>` + the UTF-8 invariant. So you can drop into raw bytes
// and back, but the way back has to VALIDATE — that's where the invariant is
// (re)established. Know the four moves:
//   - s.into_bytes()              String   -> Vec<u8>   (free; just unwraps)
//   - s.as_bytes()                &str     -> &[u8]     (free; borrowed)
//   - String::from_utf8(vec)      Vec<u8>  -> Result<String, FromUtf8Error>
//   - str::from_utf8(&buf)        &[u8]    -> Result<&str, Utf8Error>  (borrowed)
//   - String::from_utf8_lossy(&b) &[u8]    -> Cow<str>  (never fails; '�' for
//                                                        bad bytes)
//
// The lesson: bytes -> text can FAIL, so it returns Result (or substitutes via
// lossy). text -> bytes is always free (the invariant only loosens).
//
// Part A — `decode_strict(bytes: &[u8]) -> Result<String, std::str::Utf8Error>`:
//   validate `bytes` as UTF-8 and return an OWNED String. Use the BORROWING
//   validator `std::str::from_utf8` (gives &str + a precise Utf8Error), then
//   .to_owned() on success. (Note: String::from_utf8 takes Vec by value and its
//   error type differs — we want the borrowing one here so callers keep `bytes`.)
//
// Part B — `decode_lossy(bytes: &[u8]) -> (String, bool)`:
//   return (decoded_text, had_replacement). Use from_utf8_lossy; the bool is
//   true iff a replacement char was needed. Detect that WITHOUT re-scanning the
//   string for '�' — the Cow tells you: Borrowed = clean, Owned = it allocated
//   to substitute. (Reuse the Cow::Borrowed insight from rung 5.)
// ---------------------------------------------------------------------------

fn decode_strict(bytes: &[u8]) -> Result<String, std::str::Utf8Error> {
    std::str::from_utf8(bytes).map(|s| s.to_owned())
}

fn decode_lossy(bytes: &[u8]) -> (String, bool) {
    let cow = String::from_utf8_lossy(bytes);
    (cow.to_string(), matches!(cow, Cow::Owned(_)))
}

fn check_8() {
    // text -> bytes is free and infallible.
    let s = String::from("café");
    assert_eq!(s.as_bytes(), &[b'c', b'a', b'f', 0xC3, 0xA9]);
    assert_eq!(s.clone().into_bytes(), vec![b'c', b'a', b'f', 0xC3, 0xA9]);

    // Strict decode: valid bytes round-trip, invalid bytes are an Err.
    assert_eq!(decode_strict(b"hello").unwrap(), "hello");
    assert_eq!(
        decode_strict(&[b'c', b'a', b'f', 0xC3, 0xA9]).unwrap(),
        "café"
    );
    assert!(decode_strict(&[b'a', 0xFF, b'b']).is_err()); // 0xFF invalid

    // Lossy decode: clean input reports false; bad bytes report true + '�'.
    let (clean, had_rep) = decode_lossy(b"hello");
    assert_eq!(clean, "hello");
    assert!(!had_rep);

    let (fixed, had_rep) = decode_lossy(&[b'a', 0xFF, b'b']);
    assert_eq!(fixed, "a\u{FFFD}b");
    assert!(had_rep);

    println!("rung 8 ✅  bytes<->text: validate on the way back (Result/lossy)");
}

// ---------------------------------------------------------------------------
// Rung 9 — CAPSTONE: hand-roll the UTF-8 decoder (reimplement str::chars)
//
// You've trusted `chars()` all ladder. Now BUILD it. UTF-8 encodes one Unicode
// scalar value (a `char`, 0..=0x10FFFF excluding surrogates) into 1–4 bytes.
// The LEADING byte's high bits announce the length; CONTINUATION bytes all
// start with the bits 10:
//
//   bytes  lead pattern   continuation(s)   payload bits   code point range
//   1      0xxxxxxx       (none)            7              U+0000 ..U+007F
//   2      110xxxxx       10xxxxxx          5+6 = 11       U+0080 ..U+07FF
//   3      1110xxxx       10xxxxxx x2       4+12 = 16      U+0800 ..U+FFFF
//   4      11110xxx       10xxxxxx x3       3+18 = 21      U+10000..U+10FFFF
//
// To decode: read the lead byte, decide how many bytes (n) from its top bits,
// extract its low payload bits, then for each of the (n-1) continuation bytes
// verify it matches 10xxxxxx and shift in its low 6 bits:
//     cp = lead_payload
//     for each cont: cp = (cp << 6) | (cont & 0b0011_1111)
// Finally turn the u32 into a char with `char::from_u32(cp)` (this rejects
// surrogates and out-of-range values for you — the last line of validation).
//
// Part A — `decode_utf8(bytes: &[u8]) -> Option<(char, usize)>`:
//   decode the FIRST code point; return (the char, how many bytes it used).
//   Return None if: empty, the lead byte is itself a continuation/invalid
//   (0x80..=0xBF or 0xF8..=0xFF), the slice is truncated (not enough
//   continuation bytes), a continuation byte isn't 10xxxxxx, or char::from_u32
//   rejects the result.
//   (You don't need to reject overlong encodings for this rung — real decoders
//    do; note it as a known gap.)
//
// Part B — make `Utf8Chars` an Iterator<Item = char> that walks a byte slice by
//   repeatedly calling decode_utf8 and advancing. Stop (return None) at the end
//   OR on the first invalid sequence. On valid UTF-8 it must exactly match
//   what the real `.chars()` yields.
// ---------------------------------------------------------------------------

fn decode_utf8(bytes: &[u8]) -> Option<(char, usize)> {
    let lead_byte = *bytes.first()?;
    let length = match lead_byte {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return None,
    };

    if bytes.len() < length {
        return None;
    }

    let mut code_point = match length {
        1 => u32::from(lead_byte),
        2 => u32::from(lead_byte & 0b0001_1111),
        3 => u32::from(lead_byte & 0b0000_1111),
        4 => u32::from(lead_byte & 0b0000_0111),
        _ => unreachable!(),
    };

    for &byte in &bytes[1..length] {
        if byte & 0b1100_0000 != 0b1000_0000 {
            return None;
        }
        code_point = (code_point << 6) | u32::from(byte & 0b0011_1111);
    }

    char::from_u32(code_point).map(|ch| (ch, length))
}

struct Utf8Chars<'a> {
    bytes: &'a [u8],
    pos: usize,
}

fn utf8_chars(bytes: &[u8]) -> Utf8Chars<'_> {
    Utf8Chars { bytes, pos: 0 }
}

impl<'a> Iterator for Utf8Chars<'a> {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        let (char, bytes_used) = decode_utf8(&self.bytes[self.pos..])?;
        self.pos += bytes_used;
        Some(char)
    }
}

fn check_9() {
    // Single code points of every length.
    assert_eq!(decode_utf8(b"A"), Some(('A', 1)));
    assert_eq!(decode_utf8("é".as_bytes()), Some(('é', 2))); // C3 A9
    assert_eq!(decode_utf8("日".as_bytes()), Some(('日', 3))); // E6 97 A5
    assert_eq!(decode_utf8("🦀".as_bytes()), Some(('🦀', 4))); // F0 9F A6 80

    // It decodes only the FIRST char and reports the right width.
    assert_eq!(decode_utf8("é日".as_bytes()), Some(('é', 2)));

    // Rejections.
    assert_eq!(decode_utf8(b""), None); // empty
    assert_eq!(decode_utf8(&[0xFF]), None); // invalid lead
    assert_eq!(decode_utf8(&[0x80]), None); // lead is a continuation byte
    assert_eq!(decode_utf8(&[0xC3]), None); // truncated (needs 1 more)
    assert_eq!(decode_utf8(&[0xC3, 0x00]), None); // 2nd byte not 10xxxxxx

    // The iterator must agree with the real str::chars() on valid input.
    let s = "a é 日本 🦀!";
    let mine: Vec<char> = utf8_chars(s.as_bytes()).collect();
    let theirs: Vec<char> = s.chars().collect();
    assert_eq!(mine, theirs);

    // And it stops cleanly at an invalid sequence (yields the good prefix).
    let mut buf = b"ok".to_vec();
    buf.push(0xFF); // garbage after "ok"
    let decoded: String = utf8_chars(&buf).collect();
    assert_eq!(decoded, "ok");

    println!("rung 9 ✅  CAPSTONE: hand-rolled UTF-8 decoder == str::chars()");
}
