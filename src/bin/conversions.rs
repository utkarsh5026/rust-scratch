// Concept: Conversion traits — From / Into, TryFrom / TryInto, AsRef / AsMut
// Run: cargo run --bin conversions
//
// Mental model: a family of traits for turning one type into another, split on
// two questions — CAN IT FAIL?  and  CONSUME OR BORROW?
//
//                  infallible              fallible
//   take ownership  From / Into            TryFrom / TryInto
//   just borrow     AsRef / AsMut          —
//
// The unlock: you ONLY implement `From` (and `TryFrom`). `Into`/`TryInto` are
// handed to you for free by a blanket impl. And the `?` operator converts error
// types through `From`. Almost everything here falls out of those two facts.
//
// Ladder (DONE marks finished rungs):
//   1. from_basics            - impl From<Celsius> for Fahrenheit; .into() free   [DONE]
//   2. into_ergonomic         - `impl Into<String>` bounds for flexible APIs      [DONE]
//   3. from_for_errors        - From powers `?`'s error conversion                [DONE]
//   4. tryfrom_basics         - TryFrom with an Error for validated construction  [DONE]
//   5. reflexive_and_coherence- From<T> for T, why not impl Into, orphan rule     [DONE]
//   6. tryinto_and_lossy      - TryInto bounds; `as` truncation vs TryFrom        [DONE]
//   7. asref_str              - accept impl AsRef<str> / AsRef<[u8]> like stdlib  [DONE]
//   8. asref_path_asmut       - AsRef<Path> (File::open trick) + AsMut            [DONE]
//   9. json_value             - mini Value: From in, TryFrom out, AsRef          [DONE] <-- capstone

// ── Rung 1: From basics ───────────────────────────────────────────────────────
// Implement `From<Celsius> for Fahrenheit`. The formula is F = C * 9/5 + 32.
//
// The point of this rung: once you implement `From`, you get `.into()` for FREE.
// You never write `impl Into<Fahrenheit> for Celsius` — the stdlib's blanket impl
// (`impl<T, U: From<T>> Into<U> for T`) gives it to you automatically.
struct Celsius(f64);
struct Fahrenheit(f64);

impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self {
        Fahrenheit(c.0 * 9.0 / 5.0 + 32.0)
    }
}

fn check_1() {
    // Two ways to trigger the SAME From impl:
    let f1 = Fahrenheit::from(Celsius(100.0)); // explicit From
    let f2: Fahrenheit = Celsius(0.0).into(); // .into() — free, type-driven
    assert!(
        (f1.0 - 212.0).abs() < 1e-9,
        "100C should be 212F, got {}",
        f1.0
    );
    assert!((f2.0 - 32.0).abs() < 1e-9, "0C should be 32F, got {}", f2.0);
    println!("rung 1 ok: implemented From<Celsius> for Fahrenheit — .into() came free 🎉");
}

// ── Rung 2: Into bounds for ergonomic APIs ────────────────────────────────────
// The real reason `Into` exists: it makes function arguments flexible. A param of
// type `impl Into<String>` accepts a String, a &str, a Cow, a char... anything
// that knows how to become a String. The function converts ONCE, at the boundary.
//
// `Tag::new` should accept any value convertible into a String and store the
// owned String inside. Implement it using the `name: impl Into<String>` param.
//
// Rule of thumb you're learning: prefer `T: From<X>` / `impl Into<T>` on the
// CALLER side of a generic boundary; you implement `From`, callers enjoy `Into`.
#[derive(Debug, PartialEq)]
struct Tag {
    name: String,
}

impl Tag {
    fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

fn check_2() {
    // All three call sites hit the SAME function — different arg types, no clones
    // written by the caller:
    let a = Tag::new("literal"); // &'static str
    let b = Tag::new(String::from("owned")); // String (no-op conversion)
    let c = Tag::new('x'); // char -> String!
    assert_eq!(
        a,
        Tag {
            name: "literal".to_string()
        }
    );
    assert_eq!(
        b,
        Tag {
            name: "owned".to_string()
        }
    );
    assert_eq!(
        c,
        Tag {
            name: "x".to_string()
        }
    );
    println!("rung 2 ok: `impl Into<String>` makes one fn accept &str, String, and char");
}

// ── Rung 3: From powers the `?` operator ──────────────────────────────────────
// THIS is why `From` matters more than any other trait here. When you write `?`
// on a Result whose error type doesn't match the function's return error type,
// the compiler inserts `.map_err(From::from)` for you. So you make heterogeneous
// errors flow into ONE error type just by implementing `From` for each source.
//
// `parse_config` calls two stdlib operations that fail with DIFFERENT error types:
//   - `s.parse::<i32>()`     -> Result<i32, std::num::ParseIntError>
//   - a manual range check   -> you return ConfigError::OutOfRange yourself
// Both must surface as `ConfigError`. Your job: implement `From<ParseIntError>
// for ConfigError` so the `?` on the parse call compiles.
//
// Do NOT change parse_config's body — it's already written with `?`. Make it
// compile by writing the missing `From` impl. (The OutOfRange arm is handled.)
use std::num::ParseIntError;

#[derive(Debug, PartialEq)]
enum ConfigError {
    NotANumber(ParseIntError),
    OutOfRange(i32),
}

impl From<ParseIntError> for ConfigError {
    fn from(error: ParseIntError) -> Self {
        ConfigError::NotANumber(error)
    }
}

fn parse_config(s: &str) -> Result<i32, ConfigError> {
    let n: i32 = s.parse()?; // <- `?` needs From<ParseIntError> for ConfigError
    if !(0..=100).contains(&n) {
        return Err(ConfigError::OutOfRange(n));
    }
    Ok(n)
}

fn check_3() {
    assert_eq!(parse_config("42"), Ok(42));
    // a parse failure becomes NotANumber via your From impl + `?`:
    assert!(matches!(
        parse_config("nope"),
        Err(ConfigError::NotANumber(_))
    ));
    // a range failure becomes OutOfRange (returned explicitly):
    assert_eq!(parse_config("999"), Err(ConfigError::OutOfRange(999)));
    println!("rung 3 ok: `?` auto-converted ParseIntError -> ConfigError via From");
}

// ── Rung 4: TryFrom for validated construction ────────────────────────────────
// When a conversion CAN fail, `From` is wrong — `From::from` can't return an error.
// That's what `TryFrom` is for: `fn try_from(v) -> Result<Self, Self::Error>`.
// And just like From→Into, implementing `TryFrom` gives you `try_into()` for free.
//
// Build a `Percent` newtype that only accepts 0..=100. Implement
// `TryFrom<i32> for Percent` with `type Error = PercentError;`
//   - in range  -> Ok(Percent(v))
//   - out of range -> Err(PercentError::OutOfRange(v))
//
// You define the associated Error type AND the try_from body.
#[derive(Debug, PartialEq)]
struct Percent(i32);

#[derive(Debug, PartialEq)]
enum PercentError {
    OutOfRange(i32),
}

impl TryFrom<i32> for Percent {
    type Error = PercentError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        if value < 0 || value > 100 {
            return Err(PercentError::OutOfRange(value));
        }
        Ok(Percent(value))
    }
}

fn check_4() {
    // explicit TryFrom:
    assert_eq!(Percent::try_from(50), Ok(Percent(50)));
    assert_eq!(Percent::try_from(150), Err(PercentError::OutOfRange(150)));
    assert_eq!(Percent::try_from(-1), Err(PercentError::OutOfRange(-1)));

    // try_into() came FREE from your TryFrom impl — note we annotate the target so
    // the compiler knows which TryInto to pick:
    let p: Result<Percent, _> = 100.try_into();
    assert_eq!(p, Ok(Percent(100)));

    // and it composes with `?` in a fallible fn (same From-based machinery):
    fn make(n: i32) -> Result<Percent, PercentError> {
        let p = Percent::try_from(n)?; // ? works: error type already matches
        Ok(p)
    }
    assert_eq!(make(0), Ok(Percent(0)));
    assert!(make(101).is_err());
    println!("rung 4 ok: TryFrom validates; try_into() came free; composes with ?");
}

// ── Rung 5: reflexivity & the orphan rule (the coherence footgun) ─────────────
// Two facts that bite people:
//
// (a) REFLEXIVE impl: the stdlib provides `impl<T> From<T> for T`. Every type can
//     "convert" to itself — a no-op. THIS is why `?` works even when the error
//     types already match (it calls From::from, which is identity here), and why
//     `impl Into<String>` happily accepts a String at zero cost.
//
// (b) THE ORPHAN RULE (coherence): you may implement a trait for a type only if
//     the TRAIT or the TYPE is local to your crate. So you CANNOT write
//         impl From<u64> for std::time::Duration   // both foreign -> REJECTED
//     The fix every Rustacean reaches for: wrap it in a NEWTYPE you own.
//     (Also why you implement `From`, never `Into`: the blanket impl gives Into
//     for free, and historically you couldn't even impl Into for a foreign type.)
//
// Your task: make `secs_to_timeout` compile by giving it a LOCAL type to target.
//   5a. define `struct Timeout(Duration);`  — a newtype you own
//   5b. `impl From<u64> for Timeout`         — treat the u64 as whole seconds
//       (Duration::from_secs). Then `secs.into()` below resolves to your impl.
use std::time::Duration;

// TODO rung 5a: define `struct Timeout(Duration);`
// TODO rung 5b: impl From<u64> for Timeout { ... Duration::from_secs(secs) ... }

struct Timeout(Duration);

impl From<u64> for Timeout {
    fn from(secs: u64) -> Self {
        Timeout(Duration::from_secs(secs))
    }
}

// ↓↓↓ The orphan-rule violation, left commented. Uncomment to SEE the error
// (E0117 "only traits defined in the current crate..."), read it, then re-comment.
// You CANNOT make this compile from this crate — that's the whole lesson.
// impl From<u64> for Duration {
//     fn from(secs: u64) -> Self { Duration::from_secs(secs) }
// }

fn secs_to_timeout(secs: u64) -> Timeout {
    secs.into() // resolves to your From<u64> for Timeout
}

fn check_5() {
    let t = secs_to_timeout(30);
    assert_eq!(t.0, Duration::from_secs(30));

    // reflexive identity: u64 -> u64 is a REAL (no-op) From impl from the stdlib
    let same = u64::from(42u64);
    assert_eq!(same, 42);

    println!("rung 5 ok: newtype dodges the orphan rule; From<T> for T is the identity conversion");
}

// ── Rung 6: TryInto bounds + the `as` truncation footgun ──────────────────────
// `as` casts between numeric types NEVER fail — they silently truncate/wrap. That
// is a notorious bug source: `300i32 as u8` is 44, no warning, no panic. The safe,
// checked counterpart is `TryFrom`/`TryInto`, which returns Err when the value
// doesn't fit.
//
// Write a generic `narrow` that accepts ANY value which can *try* to become a u8
// and returns the checked result. Bound it on `T: TryInto<u8>` and call
// `value.try_into()`. The error type is the trait's associated type `T::Error`.
//
//   fn narrow<T: TryInto<u8>>(value: T) -> Result<u8, T::Error>
//
// (This is exactly the shape stdlib uses: `u8::try_from(x)` and `x.try_into()`
// are how you downcast integers safely.)
fn narrow<T: TryInto<u8>>(value: T) -> Result<u8, T::Error> {
    value.try_into()
}

fn check_6() {
    // THE FOOTGUN: `as` silently truncates. 300 doesn't fit in a u8 (max 255),
    // so it wraps around to 300 - 256 = 44. No error, no warning. Bugs live here.
    let truncated = 300i32 as u8;
    assert_eq!(truncated, 44, "as-cast wrapped 300 -> 44 silently");

    // THE SAFE PATH: your generic narrow() catches the overflow as an Err...
    assert!(narrow(300i32).is_err(), "300 must NOT fit in u8");
    // ...and passes values that fit, for multiple input types (the bound is generic):
    assert_eq!(narrow(200i32), Ok(200u8));
    assert_eq!(narrow(200u32), Ok(200u8));
    assert_eq!(narrow(0i64), Ok(0u8));
    assert!(narrow(-1i32).is_err(), "negative must NOT fit in u8");

    println!("rung 6 ok: TryInto<u8> bound narrows safely; `as` would have wrapped silently");
}

// ── Rung 7: AsRef — cheap reference conversions (the borrow-accepting API) ─────
// From/Into CONSUME a value and usually ALLOCATE. But often a function only needs
// to *read* the data — it shouldn't demand ownership or force a clone. That's
// `AsRef<T>`: a zero-cost "give me a &T view of yourself". `&str`, `String`,
// `&String`, `Box<str>` all impl `AsRef<str>`, so one bound accepts all of them
// BY REFERENCE — no allocation, no ownership taken.
//
// This is the pattern behind `Path::new`, `str` methods, etc. Contrast with
// rung 2: `impl Into<String>` was right when you needed to STORE an owned String;
// `impl AsRef<str>` is right when you only need to LOOK at the text.
//
// Implement both with their AsRef bound already in place:
//   - `shout`: uppercase the string view  -> s.as_ref().to_uppercase()
//   - `byte_len`: length of the byte view  -> b.as_ref().len()
fn shout<S: AsRef<str>>(s: S) -> String {
    s.as_ref().to_uppercase()
}

fn byte_len<B: AsRef<[u8]>>(b: B) -> usize {
    b.as_ref().len()
}

fn check_7() {
    // ONE function, many borrowed forms — none of these clone to call it:
    assert_eq!(shout("hi"), "HI"); // &str
    assert_eq!(shout(String::from("hi")), "HI"); // String (moved, but not cloned by us)
    let owned = String::from("hi");
    assert_eq!(shout(&owned), "HI"); // &String -> &str view
    assert_eq!(owned, "hi"); // still usable: shout only borrowed it

    // AsRef<[u8]> unifies &str, String, &[u8], Vec<u8>, arrays... as a byte view:
    assert_eq!(byte_len("abc"), 3); // &str
    assert_eq!(byte_len(String::from("abcd")), 4); // String
    assert_eq!(byte_len(vec![1u8, 2, 3]), 3); // Vec<u8>
    assert_eq!(byte_len([0u8; 5]), 5); // [u8; 5]

    println!("rung 7 ok: AsRef<str>/AsRef<[u8]> accept many types BY REFERENCE — no alloc");
}

// ── Rung 8: AsRef<Path> (the File::open trick) + AsMut ────────────────────────
// THE most famous AsRef in the stdlib: `fn open<P: AsRef<Path>>(path: P)`. That
// single bound is why you can call `File::open("f.txt")`, `File::open(string)`,
// `File::open(path_buf)` — &str, String, PathBuf, &Path ALL impl AsRef<Path>.
// You write the generic bound once; callers pass whatever path-like thing they hold.
//
// AsMut is the mutable mirror of AsRef: `as_mut()` hands back a `&mut T` view, so
// one function can mutate a Vec, an array, or a &mut slice in place.
//
//   8a. `extension`: return the file extension as an owned String, if any.
//       hint: p.as_ref().extension() -> Option<&OsStr>; .and_then(|e| e.to_str())
//             then .map(String::from)
//   8b. `double_all`: multiply every i32 in the mutable slice view by 2, return it.
//       hint: for x in data.as_mut() { *x *= 2; }  then return data
use std::path::{Path, PathBuf};

fn extension<P: AsRef<Path>>(p: P) -> Option<String> {
    p.as_ref()
        .extension()
        .and_then(|e| e.to_str())
        .map(String::from)
}

fn double_all<T: AsMut<[i32]>>(mut data: T) -> T {
    data.as_mut().iter_mut().for_each(|x| *x *= 2);
    data
}

fn check_8() {
    // AsRef<Path>: one fn, many path-like inputs, all borrowed (no alloc to call):
    assert_eq!(extension("file.rs"), Some("rs".to_string())); // &str
    assert_eq!(extension(String::from("a.txt")), Some("txt".to_string())); // String
    assert_eq!(
        extension(PathBuf::from("/tmp/x.log")),
        Some("log".to_string())
    ); // PathBuf
    assert_eq!(extension("noext"), None); // no extension

    // AsMut<[i32]>: mutate different containers in place through one bound:
    assert_eq!(double_all(vec![1, 2, 3]), vec![2, 4, 6]); // Vec<i32>
    assert_eq!(double_all([10, 20]), [20, 40]); // [i32; 2]

    println!("rung 8 ok: AsRef<Path> = the File::open trick; AsMut gives a mutable view");
}

// ── Rung 9 (CAPSTONE): a mini JSON `Value` wired with the whole family ─────────
// Build a small dynamic value type — like serde_json::Value — and connect every
// conversion trait you've learned:
//   • From<T> for Value   → ergonomic construction: `42.into()`, `"hi".into()`
//   • AsRef<str> bound     → flexible key lookup on objects (&str OR String keys)
//   • TryFrom<Value> for T → typed, FALLIBLE extraction back out (wrong type = Err)
//
// THE DESIGN INSIGHT you're proving you own: data flows INTO the dynamic type
// infallibly (From — a bool always makes a valid Value), but flows OUT fallibly
// (TryFrom — a Value might not be the type you asked for). That asymmetry is the
// whole reason both traits exist.
//
// Your tasks (fill every `todo!`):
//   9a. From<bool>, From<i64>, From<f64>, From<&str>, From<String>, From<Vec<Value>>
//       for Value. (i64 and f64 both become Value::Num(f64).)
//   9b. `Value::get<S: AsRef<str>>(&self, key) -> Option<&Value>` — if self is an
//       Object, return the value whose key matches (compare via key.as_ref()).
//   9c. TryFrom<Value> for f64    (Ok if Num, else Err(WrongType))
//       TryFrom<Value> for String (Ok if Str, else Err(WrongType))
#[derive(Debug, PartialEq, Clone)]
enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

#[derive(Debug, PartialEq)]
struct WrongType;

// 9a — values flow IN, infallibly:
impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Num(n as f64)
    }
}
impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Num(n)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Str(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Str(s)
    }
}
impl From<Vec<Value>> for Value {
    fn from(items: Vec<Value>) -> Self {
        Value::Array(items)
    }
}

impl Value {
    // 9b — AsRef<str> lets callers pass &str OR String as the key:
    fn get<S: AsRef<str>>(&self, key: S) -> Option<&Value> {
        let key = key.as_ref();
        if let Value::Object(object) = self {
            object.iter().find(|(k, _)| k == key).map(|(_, v)| v)
        } else {
            None
        }
    }
}

// 9c — values flow OUT, fallibly:
impl TryFrom<Value> for f64 {
    type Error = WrongType;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        if let Value::Num(n) = v {
            Ok(n)
        } else {
            Err(WrongType)
        }
    }
}

impl TryFrom<Value> for String {
    type Error = WrongType;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        if let Value::Str(s) = v {
            Ok(s)
        } else {
            Err(WrongType)
        }
    }
}

fn check_9() {
    // From: ergonomic construction — values flow IN via .into()
    let n: Value = 42i64.into();
    assert_eq!(n, Value::Num(42.0));
    assert_eq!(Value::from("hi"), Value::Str("hi".to_string()));
    assert_eq!(Value::from(true), Value::Bool(true));

    // a nested array built from heterogeneous .into() values:
    let arr: Value = vec![1i64.into(), "two".into(), true.into()].into();
    assert_eq!(
        arr,
        Value::Array(vec![
            Value::Num(1.0),
            Value::Str("two".to_string()),
            Value::Bool(true),
        ])
    );

    // AsRef<str> lookup: same get() accepts a &str key AND a String key
    let obj = Value::Object(vec![
        ("name".to_string(), "ada".into()),
        ("age".to_string(), 36i64.into()),
    ]);
    assert_eq!(obj.get("name"), Some(&Value::Str("ada".to_string()))); // &str key
    assert_eq!(obj.get(String::from("age")), Some(&Value::Num(36.0))); // String key
    assert_eq!(obj.get("missing"), None);
    assert_eq!(Value::Null.get("x"), None); // not an object -> None

    // TryFrom: typed extraction OUT, fallible
    let name = String::try_from(obj.get("name").unwrap().clone()).unwrap();
    assert_eq!(name, "ada");
    let age: f64 = obj.get("age").unwrap().clone().try_into().unwrap(); // try_into for free
    assert_eq!(age, 36.0);
    assert_eq!(f64::try_from(Value::Bool(true)), Err(WrongType)); // wrong type -> Err

    println!("rung 9 ok: mini Value — From in, AsRef<str> lookup, TryFrom out 🎉 CAPSTONE");
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
