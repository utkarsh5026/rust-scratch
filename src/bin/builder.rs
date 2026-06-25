//! Builder pattern — ergonomic, evolvable construction.
//!
//! Run: `cargo run --bin builder`
//!
//! Ladder (Phase 3 — API & error design):
//!   1. Owned/consuming builder        — self-by-value chain, build() -> T            [ DONE ]
//!   2. &mut self builder              — chain on &mut, build(&self) -> T             [ IN PROGRESS ]
//!   3. Optionals & defaults          — Option fields + Default + unwrap_or          [ DONE ]
//!   4. Fallible build                — missing required => build() -> Result         [ DONE ]
//!   5. The &mut chain footgun        — temporary-drop trap, #[must_use]              [ DONE ]
//!   6. Collection / repeatable setters — accumulate into Vec/HashMap, Into bounds    [ DONE ]
//!   7. Typestate builder             — missing field is a COMPILE error              [ DONE ]
//!   8. Capstone: real config builder — consume + optional + repeat + validate        [ DONE ]

// ---------------------------------------------------------------------------
// Rung 1 — Owned / consuming builder
// ---------------------------------------------------------------------------
// The classic shape. A `HttpRequest` is built up field-by-field through a
// fluent chain, then `.build()` produces the finished value.
//
// The defining trait of this style: each setter takes `self` BY VALUE, mutates
// one field, and returns `self`. Because it consumes and returns the builder,
// you can only ever chain it: `builder.method(..).method(..).build()`.

#[derive(Debug, PartialEq)]
struct HttpRequest {
    method: String,
    url: String,
    body: String,
}

struct HttpRequestBuilder {
    method: String,
    url: String,
    body: String,
}

impl HttpRequestBuilder {
    fn new() -> Self {
        HttpRequestBuilder {
            method: String::from("GET"),
            url: String::new(),
            body: String::new(),
        }
    }

    fn method(self, m: &str) -> Self {
        HttpRequestBuilder {
            method: m.to_string(),
            ..self
        }
    }

    fn url(self, u: &str) -> Self {
        HttpRequestBuilder {
            url: u.to_string(),
            ..self
        }
    }

    fn body(self, b: &str) -> Self {
        HttpRequestBuilder {
            body: b.to_string(),
            ..self
        }
    }

    fn build(self) -> HttpRequest {
        HttpRequest {
            method: self.method,
            url: self.url,
            body: self.body,
        }
    }
}

fn check_1() {
    let req = HttpRequestBuilder::new()
        .method("POST")
        .url("https://example.com")
        .body("hello")
        .build();

    assert_eq!(
        req,
        HttpRequest {
            method: "POST".to_string(),
            url: "https://example.com".to_string(),
            body: "hello".to_string(),
        }
    );

    // Defaults survive when you don't override them.
    let req2 = HttpRequestBuilder::new().url("https://x.com").build();
    assert_eq!(req2.method, "GET");
    assert_eq!(req2.body, "");

    println!("rung 1 ✔  consuming builder chains and builds");
}

// ---------------------------------------------------------------------------
// Rung 2 — &mut self builder
// ---------------------------------------------------------------------------
// Same finished type, opposite ownership choice. Here each setter borrows
// `&mut self`, mutates one field, and returns `&mut Self`. `build` takes
// `&self` and produces a fresh value WITHOUT consuming the builder — so the
// builder is still alive afterwards and could build again.
//
// Why bother? Two reasons you'll feel later:
//   - You can hold a builder in a variable and conditionally set fields:
//       let mut b = ...; if verbose { b.method("TRACE"); }  b.build();
//   - The builder is reusable (build twice, slightly tweaked).
// The cost: the ergonomics of the *fluent chain* get subtle — that's rung 5.

struct ReqBuilder {
    method: String,
    url: String,
    body: String,
}

impl ReqBuilder {
    fn new() -> Self {
        ReqBuilder {
            method: String::from("GET"),
            url: String::new(),
            body: String::new(),
        }
    }

    fn method(&mut self, m: &str) -> &mut Self {
        self.method = m.to_string();
        self
    }

    fn url(&mut self, u: &str) -> &mut Self {
        self.url = u.to_string();
        self
    }

    fn body(&mut self, b: &str) -> &mut Self {
        self.body = b.to_string();
        self
    }

    fn build(&self) -> HttpRequest {
        HttpRequest {
            method: self.method.clone(),
            url: self.url.clone(),
            body: self.body.clone(),
        }
    }
}

fn check_2() {
    // Fluent chain still works on a fresh temporary...
    let req = ReqBuilder::new()
        .method("PUT")
        .url("https://api.test")
        .body("data")
        .build();
    assert_eq!(req.method, "PUT");
    assert_eq!(req.url, "https://api.test");

    // ...but the real payoff: build the SAME builder twice, after tweaking it.
    let mut b = ReqBuilder::new();
    b.url("https://reuse.test").method("GET");
    let r1 = b.build();
    b.method("DELETE");
    let r2 = b.build();
    assert_eq!(r1.method, "GET");
    assert_eq!(r2.method, "DELETE");
    assert_eq!(r1.url, r2.url); // url carried across both builds

    println!("rung 2 ✔  &mut self builder chains AND is reusable");
}

// ---------------------------------------------------------------------------
// Rung 3 — Optionals & defaults
// ---------------------------------------------------------------------------
// The builder's job is to let callers set ONLY what they care about. Model that
// directly: every field in the builder is an `Option`, starting `None`. A setter
// stores `Some(value)`. `build()` resolves each `None` to a default.
//
// Note the finished `ServerOpts` has plain (non-Option) fields — the Option-ness
// lives only in the builder, and `build()` is where "unset" collapses into a
// concrete default. This is the standard split.

#[derive(Debug, PartialEq)]
struct ServerOpts {
    host: String,
    port: u16,
    max_conns: u32,
    tls: bool,
}

#[derive(Default)]
struct ServerOptsBuilder {
    host: Option<String>,
    port: Option<u16>,
    max_conns: Option<u32>,
    tls: Option<bool>,
}

impl ServerOptsBuilder {
    fn new() -> Self {
        // Hint: you derived Default above. Use it.
        Self::default()
    }

    // TODO(rung 3): add four setters. Use the &mut self style from rung 2.
    // Each stores Some(value):  self.port = Some(p); self
    //   fn host(&mut self, h: &str) -> &mut Self
    //   fn port(&mut self, p: u16) -> &mut Self
    //   fn max_conns(&mut self, n: u32) -> &mut Self
    //   fn tls(&mut self, on: bool) -> &mut Self
    fn host(&mut self, h: &str) -> &mut Self {
        self.host = Some(h.to_string());
        self
    }

    fn port(&mut self, p: u16) -> &mut Self {
        self.port = Some(p);
        self
    }

    fn max_conns(&mut self, n: u32) -> &mut Self {
        self.max_conns = Some(n);
        self
    }

    fn tls(&mut self, on: bool) -> &mut Self {
        self.tls = Some(on);
        self
    }
    // TODO(rung 3): build() resolves each Option to a default. Defaults:
    //   host "127.0.0.1", port 8080, max_conns 100, tls false.
    // Reach for `Option::unwrap_or` / `unwrap_or_else` / `unwrap_or_default`.
    // (Which one avoids allocating "127.0.0.1" when host WAS set? Think about it.)
    fn build(&self) -> ServerOpts {
        ServerOpts {
            host: self.host.clone().unwrap_or_else(|| "127.0.0.1".to_string()),
            port: self.port.unwrap_or(8080),
            max_conns: self.max_conns.unwrap_or(100),
            tls: self.tls.unwrap_or(false),
        }
    }
}

fn check_3() {
    // Set nothing => all defaults.
    let s = ServerOptsBuilder::new().build();
    assert_eq!(
        s,
        ServerOpts {
            host: "127.0.0.1".to_string(),
            port: 8080,
            max_conns: 100,
            tls: false,
        }
    );

    // Set some, leave the rest defaulted.
    let s2 = ServerOptsBuilder::new()
        .host("0.0.0.0")
        .port(9000)
        .max_conns(250)
        .tls(true)
        .build();
    assert_eq!(s2.host, "0.0.0.0"); // overridden
    assert_eq!(s2.port, 9000); // overridden
    assert_eq!(s2.tls, true); // overridden
    assert_eq!(s2.max_conns, 250); // overridden

    println!("rung 3 ✔  optionals collapse to defaults at build time");
}

// ---------------------------------------------------------------------------
// Rung 4 — Fallible build
// ---------------------------------------------------------------------------
// Defaults cover fields that HAVE a sensible default. But `name` here is
// genuinely required, and some values are invalid (port 0). The builder can't
// stop a caller from leaving `name` unset — so `build()` becomes the single
// checkpoint that returns Result<T, BuildError>. All validation lives here.

#[derive(Debug, PartialEq)]
struct Connection {
    name: String, // REQUIRED — no default
    port: u16,    // must be non-zero
    retries: u32, // optional, default 3
}

#[derive(Debug, PartialEq)]
enum BuildError {
    MissingName,
    InvalidPort, // port 0
}

#[derive(Default)]
struct ConnectionBuilder {
    name: Option<String>,
    port: Option<u16>,
    retries: Option<u32>,
}

impl ConnectionBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn name(&mut self, n: &str) -> &mut Self {
        self.name = Some(n.to_string());
        self
    }

    fn port(&mut self, p: u16) -> &mut Self {
        self.port = Some(p);
        self
    }

    fn retries(&mut self, r: u32) -> &mut Self {
        self.retries = Some(r);
        self
    }

    fn build(&self) -> Result<Connection, BuildError> {
        let port = self.port.unwrap_or(8080);
        if port == 0 {
            return Err(BuildError::InvalidPort);
        }

        Ok(Connection {
            name: self.name.clone().ok_or(BuildError::MissingName)?,
            port,
            retries: self.retries.unwrap_or(3),
        })
    }
}

fn check_4() {
    // Happy path.
    let c = ConnectionBuilder::new()
        .name("db")
        .port(5432)
        .build()
        .expect("valid");
    assert_eq!(
        c,
        Connection {
            name: "db".to_string(),
            port: 5432,
            retries: 3, // defaulted
        }
    );

    // Missing required name.
    let e = ConnectionBuilder::new().port(5432).build();
    assert_eq!(e, Err(BuildError::MissingName));

    // Invalid port (explicitly set to 0).
    let e2 = ConnectionBuilder::new().name("db").port(0).build();
    assert_eq!(e2, Err(BuildError::InvalidPort));

    // name set, port unset => default 8080, valid.
    let c2 = ConnectionBuilder::new().name("cache").build().unwrap();
    assert_eq!(c2.port, 8080);
    assert_eq!(c2.retries, 3);

    println!("rung 4 ✔  build() validates and returns Result");
}

// ---------------------------------------------------------------------------
// Rung 5 — The &mut chain footgun (temporary-drop trap)
// ---------------------------------------------------------------------------
// The &mut self builder (rung 2/4) chains fine in ONE expression, because the
// temporary builder lives until the end of the statement. The trap appears when
// you try to capture a partially-built &mut builder in a `let`, or build across
// statements. We reuse the rung-4 ConnectionBuilder.
//
// STEP A — see the error. Uncomment this block and run `cargo run --bin builder`:
//
//     let builder = ConnectionBuilder::new().name("db").port(5432);
//     let conn = builder.build().unwrap();
//
// You'll get E0716 "temporary value dropped while borrowed". WHY: `new()` makes
// a temporary ConnectionBuilder; `.name().port()` return `&mut` references INTO
// that temporary; at the `;` the temporary is freed, so `builder` would be a
// reference to freed memory. Read the error, then RE-COMMENT it so the file
// compiles again, and write your understanding in the SAFETY-style note below.
//
//   WHY IT FAILS: <your turn: one sentence — what does `builder` end up pointing at?>
//
// STEP B — implement the fix below.

// Build a Connection, conditionally enabling extra retries. This forces the
// across-statements pattern that the consuming builder of rung 1 can't do
// ergonomically but the &mut builder does — IF you bind the builder to an owner.
fn build_conn(many_retries: bool) -> Result<Connection, BuildError> {
    // TODO(rung 5): the fix for the temporary-drop trap is to give the builder
    // an OWNING binding first, then call setters on that binding:
    //     let mut b = ConnectionBuilder::new();
    //     b.name(...); b.port(...);
    //     if many_retries { b.retries(...); }
    //     b.build()
    // Set name "svc", port 5432; if many_retries, set retries to 10.

    let mut b = ConnectionBuilder::new();
    b.name("svc");
    b.port(5432);
    if many_retries {
        b.retries(10);
    }
    b.build()
}

fn check_5() {
    let c = build_conn(false).unwrap();
    assert_eq!(c.name, "svc");
    assert_eq!(c.port, 5432);
    assert_eq!(c.retries, 3); // default — many_retries was false

    let c2 = build_conn(true).unwrap();
    assert_eq!(c2.retries, 10); // conditionally overridden

    println!("rung 5 ✔  owning-binding pattern dodges the temporary-drop trap");
}

// ---------------------------------------------------------------------------
// Rung 6 — Collection / repeatable setters + Into bounds
// ---------------------------------------------------------------------------
// Two ergonomics tricks every real builder uses:
//
//  (a) REPEATABLE setters that accumulate. `.header(k, v)` called N times pushes
//      N entries — it does NOT overwrite. This is how reqwest's RequestBuilder
//      collects headers. The field is a Vec/HashMap; the setter inserts.
//
//  (b) `impl Into<String>` arguments, so callers pass &str OR String without
//      writing .to_string() at the call site. Inside the setter you call
//      `.into()` once to normalize to the owned type.

use std::collections::HashMap;

#[derive(Debug, PartialEq)]
struct Email {
    to: Vec<String>,                  // repeatable: every .to(..) appends
    headers: HashMap<String, String>, // repeatable: every .header(k,v) inserts
    subject: String,
}

#[derive(Default)]
struct EmailBuilder {
    to: Vec<String>,
    headers: HashMap<String, String>,
    subject: String,
}

impl EmailBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn to(&mut self, addr: impl Into<String>) -> &mut Self {
        self.to.push(addr.into());
        self
    }

    fn header(&mut self, k: impl Into<String>, v: impl Into<String>) -> &mut Self {
        self.headers.insert(k.into(), v.into());
        self
    }

    fn subject(&mut self, s: impl Into<String>) -> &mut Self {
        self.subject = s.into();
        self
    }

    // build clones the accumulated collections out (we only have &self).
    fn build(&self) -> Email {
        Email {
            to: self.to.clone(),
            headers: self.headers.clone(),
            subject: self.subject.clone(),
        }
    }
}

fn check_6() {
    let owned_addr = String::from("c@x.com"); // proves String args work too

    let mut b = EmailBuilder::new();
    b.to("a@x.com") // &str
        .to("b@x.com") // appends, not replaces
        .to(owned_addr) // String — Into<String> handles both
        .header("X-Trace", "1")
        .header("X-Env", "prod")
        .subject("hello");
    let email = b.build();

    assert_eq!(email.to, vec!["a@x.com", "b@x.com", "c@x.com"]);
    assert_eq!(email.headers.len(), 2);
    assert_eq!(email.headers.get("X-Trace").map(String::as_str), Some("1"));
    assert_eq!(email.headers.get("X-Env").map(String::as_str), Some("prod"));
    assert_eq!(email.subject, "hello");

    println!("rung 6 ✔  repeatable setters accumulate; Into accepts &str and String");
}

// ---------------------------------------------------------------------------
// Rung 7 — Typestate builder (missing required field = COMPILE error)
// ---------------------------------------------------------------------------
// Two REQUIRED fields: endpoint and token. We track "is it set?" in the TYPE
// using two marker type-params E and T, each either `No` or `Yes`. Each required
// setter CONSUMES self and returns a builder with that marker flipped to `Yes`.
// `build()` is implemented ONLY for ApiCallBuilder<Yes, Yes> — so you literally
// cannot call it until both are set. The check at rung 4 moves from runtime to
// the type system.
//
// This is consuming-builder style (self by value) — natural here, because each
// transition produces a builder of a DIFFERENT type, so `..self` can't be used
// (the source and target are different types); you move each field explicitly.

use std::marker::PhantomData;

struct Yes;
struct No;

#[derive(Debug, PartialEq)]
struct ApiCall {
    endpoint: String,
    token: String,
    timeout_ms: u64, // optional, default 30_000
}

struct ApiCallBuilder<E, T> {
    endpoint: Option<String>,
    token: Option<String>,
    timeout_ms: Option<u64>,
    _state: PhantomData<(E, T)>,
}

// Start state: nothing required is set yet.
impl ApiCallBuilder<No, No> {
    fn new() -> Self {
        ApiCallBuilder {
            endpoint: None,
            token: None,
            timeout_ms: None,
            _state: PhantomData,
        }
    }
}

// Setters are available in ANY state (generic over E, T), but the REQUIRED ones
// transition the marker. timeout does NOT change state.
impl<E, T> ApiCallBuilder<E, T> {
    // TODO(rung 7a): endpoint() consumes self and returns ApiCallBuilder<Yes, T>.
    // Because the return type differs from Self, you must rebuild the struct by
    // hand — move token & timeout_ms across, set endpoint to Some, _state: PhantomData.
    fn endpoint(self, e: &str) -> ApiCallBuilder<Yes, T> {
        ApiCallBuilder {
            endpoint: Some(e.to_string()),
            token: self.token,
            timeout_ms: self.timeout_ms,
            _state: PhantomData,
        }
    }

    // TODO(rung 7b): token() consumes self and returns ApiCallBuilder<E, Yes>.
    fn token(self, t: &str) -> ApiCallBuilder<E, Yes> {
        ApiCallBuilder {
            endpoint: self.endpoint,
            token: Some(t.to_string()),
            timeout_ms: self.timeout_ms,
            _state: PhantomData,
        }
    }

    // TODO(rung 7c): timeout() is optional — it returns Self (same state).
    // Take `mut self`, set timeout_ms, return self.
    fn timeout(self, ms: u64) -> Self {
        ApiCallBuilder {
            endpoint: self.endpoint,
            token: self.token,
            timeout_ms: Some(ms),
            _state: PhantomData,
        }
    }
}

// build() EXISTS ONLY when both markers are Yes.
impl ApiCallBuilder<Yes, Yes> {
    // TODO(rung 7d): build() -> ApiCall. The type guarantees endpoint & token are
    // Some, so `.unwrap()` here is provably safe (the type system already checked
    // what rung 4 checked at runtime). timeout_ms defaults to 30_000.
    fn build(self) -> ApiCall {
        ApiCall {
            endpoint: self.endpoint.unwrap(),
            token: self.token.unwrap(),
            timeout_ms: self.timeout_ms.unwrap_or(30_000),
        }
    }
}

fn check_7() {
    // Both required fields set, in either order — build() compiles.
    let call = ApiCallBuilder::new()
        .endpoint("https://api.test/v1")
        .token("secret")
        .build();
    assert_eq!(
        call,
        ApiCall {
            endpoint: "https://api.test/v1".to_string(),
            token: "secret".to_string(),
            timeout_ms: 30_000,
        }
    );

    // Order doesn't matter, and the optional timeout works.
    let call2 = ApiCallBuilder::new()
        .token("secret")
        .timeout(5_000)
        .endpoint("https://api.test/v2")
        .build();
    assert_eq!(call2.timeout_ms, 5_000);

    // COMPILE-ERROR CHECK (uncomment to prove it — then re-comment):
    //     let bad = ApiCallBuilder::new().endpoint("x").build();
    // => E0599 "no method named `build` found for ApiCallBuilder<Yes, No>".
    // The missing token is now a TYPE error, not a runtime Err.

    println!("rung 7 ✔  typestate: build() only exists once both required fields are set");
}

// ---------------------------------------------------------------------------
// Rung 8 — Capstone: a real config builder
// ---------------------------------------------------------------------------
// Synthesize the whole ladder into one idiomatic API:
//   - entry point `ServerConfig::builder()` (rung 1 consuming style)
//   - `impl Into<String>` args (rung 6)
//   - optional fields with defaults (rung 3)
//   - repeatable setters: `.route(..)` appends, `.env(k,v)` inserts (rung 6)
//   - fallible `build() -> Result<ServerConfig, ConfigError>` with all validation
//     funneled through one checkpoint (rung 4)
//
// Style choice: CONSUMING builder (self by value, returns Self). It chains in a
// single expression and ends in `.build()?`, the shape real crates expose.

#[derive(Debug, PartialEq)]
struct ServerConfig {
    bind_addr: String,            // required
    port: u16,                    // default 8080, must be nonzero
    workers: usize,               // default 4, must be >= 1
    routes: Vec<String>,          // repeatable, must have >= 1
    env: HashMap<String, String>, // repeatable, optional
}

#[derive(Debug, PartialEq)]
enum ConfigError {
    MissingBindAddr,
    ZeroPort,
    ZeroWorkers,
    NoRoutes,
}

#[derive(Default)]
struct ServerConfigBuilder {
    bind_addr: Option<String>,
    port: Option<u16>,
    workers: Option<usize>,
    routes: Vec<String>,
    env: HashMap<String, String>,
}

impl ServerConfig {
    // Conventional entry point: `ServerConfig::builder()`.
    fn builder() -> ServerConfigBuilder {
        ServerConfigBuilder::default()
    }
}

impl ServerConfigBuilder {
    fn bind_addr(self, a: impl Into<String>) -> Self {
        ServerConfigBuilder {
            bind_addr: Some(a.into()),
            port: self.port,
            workers: self.workers,
            routes: self.routes,
            env: self.env,
        }
    }
    fn port(self, p: u16) -> Self {
        ServerConfigBuilder {
            bind_addr: self.bind_addr,
            port: Some(p),
            workers: self.workers,
            routes: self.routes,
            env: self.env,
        }
    }
    fn workers(self, n: usize) -> Self {
        ServerConfigBuilder {
            bind_addr: self.bind_addr,
            port: self.port,
            workers: Some(n),
            routes: self.routes,
            env: self.env,
        }
    }
    fn route(self, r: impl Into<String>) -> Self {
        let mut routes = self.routes;
        routes.push(r.into());
        ServerConfigBuilder {
            bind_addr: self.bind_addr,
            port: self.port,
            workers: self.workers,
            routes: routes,
            env: self.env,
        }
    }
    fn env(self, k: impl Into<String>, v: impl Into<String>) -> Self {
        let mut env = self.env;
        env.insert(k.into(), v.into());
        ServerConfigBuilder {
            bind_addr: self.bind_addr,
            port: self.port,
            workers: self.workers,
            routes: self.routes,
            env: env,
        }
    }

    fn build(self) -> Result<ServerConfig, ConfigError> {
        let bind_addr = self.bind_addr.ok_or(ConfigError::MissingBindAddr)?;
        let port = self.port.unwrap_or(8080);
        let workers = self.workers.unwrap_or(4);

        if port == 0 {
            return Err(ConfigError::ZeroPort);
        }
        if workers == 0 {
            return Err(ConfigError::ZeroWorkers);
        }
        if self.routes.is_empty() {
            return Err(ConfigError::NoRoutes);
        }
        Ok(ServerConfig {
            bind_addr,
            port,
            workers,
            routes: self.routes,
            env: self.env,
        })
    }
}

fn check_8() {
    // Full happy path — fluent chain ending in build()?
    let cfg = ServerConfig::builder()
        .bind_addr("0.0.0.0")
        .port(9090)
        .workers(8)
        .route("/health")
        .route("/metrics")
        .env("LOG", "info")
        .build()
        .expect("valid config");

    assert_eq!(cfg.bind_addr, "0.0.0.0");
    assert_eq!(cfg.port, 9090);
    assert_eq!(cfg.workers, 8);
    assert_eq!(cfg.routes, vec!["/health", "/metrics"]);
    assert_eq!(cfg.env.get("LOG").map(String::as_str), Some("info"));

    // Minimal valid: defaults fill in port/workers; one route required.
    let cfg2 = ServerConfig::builder()
        .bind_addr("127.0.0.1")
        .route("/")
        .build()
        .unwrap();
    assert_eq!(cfg2.port, 8080); // default
    assert_eq!(cfg2.workers, 4); // default

    // Each error path, exercised independently:
    assert_eq!(
        ServerConfig::builder().route("/").build(),
        Err(ConfigError::MissingBindAddr),
    );
    assert_eq!(
        ServerConfig::builder().bind_addr("x").build(),
        Err(ConfigError::NoRoutes),
    );
    assert_eq!(
        ServerConfig::builder()
            .bind_addr("x")
            .route("/")
            .port(0)
            .build(),
        Err(ConfigError::ZeroPort),
    );
    assert_eq!(
        ServerConfig::builder()
            .bind_addr("x")
            .route("/")
            .workers(0)
            .build(),
        Err(ConfigError::ZeroWorkers),
    );

    println!("rung 8 ✔  CAPSTONE: consuming + Into + optionals + repeatable + validated build");
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
}
