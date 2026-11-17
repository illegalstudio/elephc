//! Purpose:
//! Bridge to the sampling probe and the exact profiler, shared by every web mode.
//!
//! Called from:
//! - `crate::worker`, for the default prefork model.
//! - `crate::handler_broker::process::handler`, for pool- and request-isolated
//!   children, where PHP runs in a handler child rather than in the worker.
//!
//! Key details:
//! - The compiled program's core runtime always defines the `.comm` pointer slots
//!   (zero by default); under `--probe` / `--instrument` the compiler stores the
//!   real entry points into them. Reading a slot and calling through it when
//!   non-null keeps monitoring pay-for-use, with no dlsym and no compile-time
//!   coupling to the probe crate.
//! - Every entry point here is a no-op when its slot is zero, so a binary built
//!   without monitoring pays one load per call site and nothing else.
//! - This lives in its own module because it used to be private to `worker`,
//!   which is precisely why `--web-isolation=pool|request` ran no monitoring at
//!   all: the isolated path never goes through `worker`.

extern "C" {
    /// Runtime `.comm` slot holding `elephc_probe_set_route` under `--probe`,
    /// else zero. Mangled per target like the other runtime externs.
    static elephc_probe_route_fn: usize;
    /// Runtime `.comm` slot holding `elephc_probe_rearm` under `--probe`, else
    /// zero — the worker re-arm the fork disarmed.
    static elephc_probe_rearm_fn: usize;
    /// Runtime `.comm` slot holding `elephc_instr_trace_begin` under
    /// `--instrument`, else zero — opens this request's W3C trace context.
    static elephc_instr_trace_fn: usize;
    /// Runtime `.comm` slot holding `elephc_instr_request` under
    /// `--instrument`, else zero — brackets one request's own profile.
    static elephc_instr_request_fn: usize;
    /// Runtime `.comm` slot holding `elephc_probe_verify_query`, else zero —
    /// checks that a profiling request is signed by the build key.
    static elephc_probe_verify_fn: usize;
    /// Runtime `.comm` slot holding `elephc_probe_apm_enable` under `--probe`,
    /// else zero — turns transaction counting on for `--apm`.
    static elephc_probe_apm_fn: usize;
    /// Runtime `.comm` slot holding `elephc_probe_note_transaction` under
    /// `--probe`, else zero — records one completed request.
    static elephc_probe_txn_fn: usize;
    /// Runtime `.comm` slot holding `elephc_probe_reap` under `--probe`, else
    /// zero — counts the request a reaped worker died serving.
    static elephc_probe_reap_fn: usize;
    /// Runtime `.comm` slot holding `elephc_probe_settle_request` under
    /// `--probe`, else zero — settles an isolated handler's pending work.
    static elephc_probe_settle_fn: usize;
    /// Runtime `.comm` slot holding `elephc_probe_request_begin` under
    /// `--probe`, else zero — marks an isolated web worker's request in flight.
    static elephc_probe_begin_fn: usize;
    /// Runtime `.comm` slot holding `elephc_probe_request_end` under `--probe`,
    /// else zero — ends that mark.
    static elephc_probe_end_fn: usize;
}

type SetRouteFn = unsafe extern "C" fn(*const u8, usize);
type RearmFn = unsafe extern "C" fn();
type TraceFn = unsafe extern "C" fn(*const u8, usize, *const u8, usize);
type RequestFn = unsafe extern "C" fn(u32);
type VerifyFn = unsafe extern "C" fn(*const u8, usize) -> u32;
type ApmEnableFn = unsafe extern "C" fn(u32) -> u32;
type TransactionFn = unsafe extern "C" fn(*const u8, usize, u32, u64);

/// Whether this server counts its transactions (`--apm`).
///
/// Set once by the master before it forks, so every worker and handler child
/// inherits it; read on every request to decide whether to take a clock reading
/// at all. A service started without `--apm` therefore pays one relaxed load.
static APM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Turns transaction counting on, for a server started with `--apm`.
///
/// Returns false when the binary cannot count — it was built without
/// `--with-monitoring`, or its shared region failed to map — so the caller can
/// refuse to start rather than serve as if monitored while recording nothing.
///
/// `workers_run_php` is true for `--web-isolation=worker`, where the master
/// reaps the very processes that run requests and can count one that died
/// mid-request.
pub fn apm_enable(workers_run_php: bool) -> bool {
    let addr = unsafe { std::ptr::addr_of!(elephc_probe_apm_fn).read() };
    if addr == 0 {
        return false;
    }
    let enable = unsafe { std::mem::transmute::<usize, ApmEnableFn>(addr) };
    let on = unsafe { enable(u32::from(workers_run_php)) } != 0;
    APM.store(on, std::sync::atomic::Ordering::Relaxed);
    on
}

/// Records requests when the probe counts already: a binary built with
/// `--apm-collector` counts from its first instruction and sends the counts,
/// so a service started without `--apm` records too, rather than sending
/// snapshots without a single request (review). False, and nothing turned
/// on, otherwise.
pub fn apm_follow(workers_run_php: bool) -> bool {
    /// The probe's `APM_FOLLOW` flag.
    const APM_FOLLOW: u32 = 2;
    let addr = unsafe { std::ptr::addr_of!(elephc_probe_apm_fn).read() };
    if addr == 0 {
        return false;
    }
    let enable = unsafe { std::mem::transmute::<usize, ApmEnableFn>(addr) };
    let on = unsafe { enable(u32::from(workers_run_php) | APM_FOLLOW) } != 0;
    APM.store(on, std::sync::atomic::Ordering::Relaxed);
    on
}

/// Whether requests should be timed and recorded.
#[inline]
pub fn apm_active() -> bool {
    APM.load(std::sync::atomic::Ordering::Relaxed)
}

/// Tells the probe a worker process ended, so a request it was serving when
/// it died is counted as a server error. A no-op without `--apm`.
pub fn note_dead_worker(pid: i32) {
    if !apm_active() {
        return;
    }
    let addr = unsafe { std::ptr::addr_of!(elephc_probe_reap_fn).read() };
    if addr == 0 {
        return;
    }
    let reap = unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(i32)>(addr) };
    unsafe { reap(pid) };
}

/// A request an isolated web worker waits on, marked in flight in the probe so
/// the master counts it as a server error should this web worker die before
/// answering. Given back when dropped — answered, or abandoned because the
/// client went away — so it never outlives its request.
///
/// A request the client abandons is counted as it goes, with
/// `CLIENT_CLOSED_REQUEST`: hyper drops the request's future when the client
/// closes the connection, so neither its answer nor its rejection was ever
/// recorded, and it vanished from the counts while a prefork worker counts the
/// same request (review).
pub struct InFlight {
    token: u64,
    /// The route the request is counted under; empty for an inert mark.
    route: String,
    started: std::time::Instant,
    answered: std::sync::atomic::AtomicBool,
}

/// The status a request the client abandoned is counted with, as nginx logs
/// it: a 4xx, the client's doing.
pub const CLIENT_CLOSED_REQUEST: u16 = 499;

impl InFlight {
    /// Marks the request as in flight under `route`; an inert mark without
    /// `--apm` or without room in the probe's in-flight table.
    pub fn begin(route: &str, started: std::time::Instant) -> Self {
        let mut mark = Self { token: 0, route: String::new(), started, answered: std::sync::atomic::AtomicBool::new(false) };
        if !apm_active() {
            return mark;
        }
        mark.route = route.to_string();
        let addr = unsafe { std::ptr::addr_of!(elephc_probe_begin_fn).read() };
        if addr != 0 {
            let begin = unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(*const u8, usize) -> u64>(addr) };
            mark.token = unsafe { begin(route.as_ptr(), route.len()) };
        }
        mark
    }

    /// Says the request is being counted, right before its answer is
    /// recorded: a death from then on must not count it a second time.
    pub fn answering(&self) {
        self.answered.store(true, std::sync::atomic::Ordering::Relaxed);
        self.end(1);
    }

    /// Calls the probe's `elephc_probe_request_end` with this mark's token.
    fn end(&self, answering: u32) {
        if self.token == 0 {
            return;
        }
        let addr = unsafe { std::ptr::addr_of!(elephc_probe_end_fn).read() };
        if addr != 0 {
            let end = unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(u64, u32)>(addr) };
            unsafe { end(self.token, answering) };
        }
    }
}

impl Drop for InFlight {
    /// Counts the request when nothing did — the client went away — and gives
    /// the mark back.
    fn drop(&mut self) {
        if !self.route.is_empty() && !self.answered.load(std::sync::atomic::Ordering::Relaxed) {
            self.answering();
            note_transaction(Some(&self.route), CLIENT_CLOSED_REQUEST, self.started);
        }
        self.end(0);
    }
}

/// Settles the work of a request this process ran but does not record — an
/// isolated handler child, whose web worker records it by name — on the row
/// that answer will be counted under. Call it before `clear`. A no-op without
/// `--apm`.
pub fn settle_request(status: u16) {
    if !apm_active() {
        return;
    }
    let addr = unsafe { std::ptr::addr_of!(elephc_probe_settle_fn).read() };
    if addr == 0 {
        return;
    }
    let settle = unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(u32)>(addr) };
    unsafe { settle(u32::from(status)) };
}

/// Records one completed request's status and wall time.
///
/// `route` is `None` for a request whose route `set` already stamped in this
/// process — the common case, and a lookup-free one — and the shaped label for
/// a request recorded elsewhere: one answered before PHP ran, or one whose PHP
/// ran in an isolated handler child that stamped its own copy.
pub fn note_transaction(route: Option<&str>, status: u16, started: std::time::Instant) {
    if !apm_active() {
        return;
    }
    let addr = unsafe { std::ptr::addr_of!(elephc_probe_txn_fn).read() };
    if addr == 0 {
        return;
    }
    let record = unsafe { std::mem::transmute::<usize, TransactionFn>(addr) };
    let elapsed = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    let (ptr, len) = route.map_or((std::ptr::null(), 0), |route| (route.as_ptr(), route.len()));
    unsafe { record(ptr, len, u32::from(status), elapsed) };
}

/// Flag on the status `elephc_probe_note_transaction` receives: the server
/// answered before any PHP ran (a body too large or too slow), so the path
/// says nothing about a route. Mirrors `ANSWERED_BEFORE_PHP` in the probe.
const ANSWERED_BEFORE_PHP: u32 = 1 << 31;

/// Records a request the server answered itself, before PHP ran: a 408 or a
/// 413. Any client can send one for any path, so it keeps the row of a route
/// the table already has and otherwise goes to one `<rejected>` row — it never
/// adds a route (review: 255 oversized POSTs to made-up paths filled the
/// table).
pub fn note_rejected(route: &str, status: u16, started: std::time::Instant) {
    if !apm_active() {
        return;
    }
    let addr = unsafe { std::ptr::addr_of!(elephc_probe_txn_fn).read() };
    if addr == 0 {
        return;
    }
    let record = unsafe { std::mem::transmute::<usize, TransactionFn>(addr) };
    let elapsed = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    unsafe { record(route.as_ptr(), route.len(), u32::from(status) | ANSWERED_BEFORE_PHP, elapsed) };
}

/// Whether the sampling probe is linked into this binary.
///
/// Callers use this to skip building a route label at all: shaping the path is
/// pure overhead when no probe will consume it, and a production binary has no
/// probe.
pub fn active() -> bool {
    resolve().is_some()
}

/// The route-tagging function, or `None` when the slot is empty because the
/// binary was built without the probe.
fn resolve() -> Option<SetRouteFn> {
    let addr = unsafe { std::ptr::addr_of!(elephc_probe_route_fn).read() };
    if addr == 0 {
        None
    } else {
        Some(unsafe { std::mem::transmute::<usize, SetRouteFn>(addr) })
    }
}

/// Re-arms the probe timer in this worker, if the probe is linked.
pub fn rearm() {
    let addr = unsafe { std::ptr::addr_of!(elephc_probe_rearm_fn).read() };
    if addr != 0 {
        let rearm = unsafe { std::mem::transmute::<usize, RearmFn>(addr) };
        unsafe { rearm() };
    }
}

/// Marker for a path segment that is an identifier rather than a route.
const SHAPE_ID: &str = "{id}";
/// Marker for a segment that is a UUID.
const SHAPE_UUID: &str = "{uuid}";
/// Marker for a segment that is a long hex digest — a content hash, an ETag, a
/// cache-busting suffix.
const SHAPE_HASH: &str = "{hash}";
/// How many hex characters make a segment a digest rather than a word. Twelve
/// is the shortest truncated hash in common use; below it, ordinary words made
/// only of `a`–`f` letters would be collapsed.
const HASH_MIN_LEN: usize = 12;

/// The methods HTTP defines (RFC 9110, and PATCH from RFC 5789), kept as sent.
const STANDARD_METHODS: [&str; 9] = ["GET", "HEAD", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "TRACE", "PATCH"];
/// What any other method token reads as in a label.
const OTHER_METHOD: &str = "OTHER";

/// Builds the label this request is profiled under: the method, then the path
/// with its variable parts replaced by their shape.
///
/// The route table in the probe is a fixed 256 entries with no eviction, and
/// the label used to be the raw path — so a service with ordinary dynamic URLs
/// spent its whole table on `GET /users/1`, `GET /users/2`, ... and the endpoint
/// an operator actually wanted to see arrived to a full table. The docs told the
/// caller to "pass route patterns (`/users/{id}`), not raw paths", which nothing
/// in the server could do: the framework's routing table lives in PHP, and by
/// the time a request reaches here the only thing known about the path is its
/// text. Deriving the shape from the text is what IS available.
///
/// Deliberately conservative — three rules, each one a thing that cannot be a
/// route name:
/// - a segment of only digits (`/users/123`),
/// - a UUID,
/// - twelve or more hex characters (`/assets/app.9f8c2b1a4d5e.js`, where each
///   dot-separated part is judged on its own).
///
/// Anything else is kept verbatim, because over-collapsing is the worse error:
/// a table entry that merges two real endpoints reports a number that is true
/// of neither, while one that fails to merge two ids costs a slot and is
/// visible as such.
///
/// The method is the client's token, so one outside HTTP's standard set reads
/// `OTHER`: every made-up token was otherwise a row of its own (review).
pub fn route_label(method: &str, path: &str) -> String {
    let method = if STANDARD_METHODS.contains(&method) { method } else { OTHER_METHOD };
    let mut label = String::with_capacity(method.len() + path.len() + 1);
    label.push_str(method);
    label.push(' ');
    for (index, segment) in path.split('/').enumerate() {
        if index > 0 {
            label.push('/');
        }
        push_shaped(&mut label, segment);
    }
    label
}

/// Appends one path segment, shaped. A segment with dots is shaped part by
/// part, so a hashed asset name keeps its stem and extension.
fn push_shaped(label: &mut String, segment: &str) {
    if let Some(shape) = shape_of(segment) {
        label.push_str(shape);
        return;
    }
    if !segment.contains('.') {
        label.push_str(segment);
        return;
    }
    for (index, part) in segment.split('.').enumerate() {
        if index > 0 {
            label.push('.');
        }
        label.push_str(shape_of(part).unwrap_or(part));
    }
}

/// The shape of one path part, or `None` when it is a name to keep.
fn shape_of(part: &str) -> Option<&'static str> {
    if part.is_empty() {
        return None;
    }
    if part.bytes().all(|b| b.is_ascii_digit()) {
        return Some(SHAPE_ID);
    }
    if is_uuid(part) {
        return Some(SHAPE_UUID);
    }
    if part.len() >= HASH_MIN_LEN && part.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Some(SHAPE_HASH);
    }
    None
}

/// Whether `part` is a canonical 8-4-4-4-12 hex UUID.
fn is_uuid(part: &str) -> bool {
    part.len() == 36
        && part.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

/// Stamps `route` onto samples taken until `clear`.
pub fn set(route: &str) {
    if let Some(set_route) = resolve() {
        unsafe { set_route(route.as_ptr(), route.len()) };
    }
}

/// Clears the active route so idle-worker samples are untagged.
pub fn clear() {
    if let Some(set_route) = resolve() {
        unsafe { set_route(std::ptr::null(), 0) };
    }
}

/// Whether a profiling request carries a signature this binary accepts.
///
/// Profiling costs the request real time and reveals the shape of the code,
/// so asking for it is privileged: only a holder of the build key can. An
/// unsigned trigger would let anyone who can set a header profile production.
/// A binary without the tooling answers no, which is also the right answer.
pub fn query_is_authentic(value: &str) -> bool {
    let addr = unsafe { std::ptr::addr_of!(elephc_probe_verify_fn).read() };
    if addr == 0 {
        return false;
    }
    let verify = unsafe { std::mem::transmute::<usize, VerifyFn>(addr) };
    (unsafe { verify(value.as_ptr(), value.len()) }) != 0
}

/// Whether the exact profiler's trace hook is linked.
pub fn trace_available() -> bool {
    let addr = unsafe { std::ptr::addr_of!(elephc_instr_trace_fn).read() };
    addr != 0
}

/// Whether the signed profiling-query verifier is linked.
pub fn query_available() -> bool {
    let addr = unsafe { std::ptr::addr_of!(elephc_probe_verify_fn).read() };
    addr != 0
}

/// Starts or ends this request's own profile, when the binary carries hooks.
///
/// A production binary can ship instrumented and dormant (`ELEPHC_INSTR_OFF=1`),
/// costing a load and a branch per call, and still answer the exact same
/// question a dev build answers — for the one request that asked. That is the
/// difference between "profiling is a dev thing" and "profiling is a thing you
/// can do where the problem actually is".
///
/// The argument says WHY rather than only whether: `1` this request was
/// authorized by a signed header, `2` offer it — start
/// a slice only if something is waiting for one — and `0` end whatever
/// started. The bridge cannot answer "is anyone waiting": that state lives in
/// the instrumentation, so it reports what it knows and lets the other side
/// decide.
pub fn profile_request_kind(kind: u32) {
    let addr = unsafe { std::ptr::addr_of!(elephc_instr_request_fn).read() };
    if addr == 0 {
        return;
    }
    let bracket = unsafe { std::mem::transmute::<usize, RequestFn>(addr) };
    unsafe { bracket(kind) };
}

/// Ends this request's profile, handing over its HTTP status so a captured
/// slice says whether its request failed (`0` ends without one).
pub fn profile_request_end(status: u16) {
    profile_request_kind(u32::from(status) << 8);
}

/// Opens the exact profiler's trace context for this request from the
/// inbound W3C `traceparent` (absent → a new trace is started). Pass the
/// header value, or `None`. No-op unless `--instrument` filled the slot.
/// `route` is passed alongside so an exact capture can be broken down per
/// endpoint, the way the sampler's route tagging already allows.
pub fn trace_begin(traceparent: Option<&str>, route: &str) {
    let addr = unsafe { std::ptr::addr_of!(elephc_instr_trace_fn).read() };
    if addr == 0 {
        return;
    }
    let begin = unsafe { std::mem::transmute::<usize, TraceFn>(addr) };
    let (tp, tp_len) = match traceparent {
        Some(value) => (value.as_ptr(), value.len()),
        None => (std::ptr::null(), 0),
    };
    unsafe { begin(tp, tp_len, route.as_ptr(), route.len()) };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Identifiers become shapes; names stay names.
    ///
    /// The second half is the one that decides the rule set. Collapsing too
    /// eagerly merges two real endpoints into a row that describes neither,
    /// which is a wrong profile rather than a coarse one — so a segment is only
    /// replaced when it cannot be a route name.
    #[test]
    fn a_route_label_keeps_names_and_replaces_identifiers() {
        for (method, path, expected) in [
            ("GET", "/users/123", "GET /users/{id}"),
            ("POST", "/users/123/orders/9", "POST /users/{id}/orders/{id}"),
            (
                "GET",
                "/orders/6f9619ff-8b86-d011-b42d-00c04fc964ff",
                "GET /orders/{uuid}",
            ),
            (
                "GET",
                "/assets/app.9f8c2b1a4d5e.js",
                "GET /assets/app.{hash}.js",
            ),
            ("GET", "/d41d8cd98f00b204e9800998ecf8427e", "GET /{hash}"),
            // Kept: a route name, a version, a word that happens to be short
            // hex, and a slug with digits in it.
            ("GET", "/", "GET /"),
            ("GET", "/checkout", "GET /checkout"),
            ("GET", "/api/v2/health", "GET /api/v2/health"),
            ("GET", "/cafe/beef", "GET /cafe/beef"),
            ("GET", "/posts/10-things-i-know", "GET /posts/10-things-i-know"),
        ] {
            assert_eq!(route_label(method, path), expected, "{method} {path}");
        }
    }

    /// The point of the shaping: a table of 256 entries is no longer spent by
    /// ordinary traffic before anyone asks for a profile.
    #[test]
    fn dynamic_urls_collapse_to_one_table_entry() {
        let labels: std::collections::BTreeSet<String> =
            (0..300).map(|n| route_label("GET", &format!("/users/{n}"))).collect();
        assert_eq!(
            labels.len(),
            1,
            "300 distinct URLs must not be 300 distinct routes"
        );
        assert_eq!(labels.iter().next().unwrap(), "GET /users/{id}");
    }

    /// The method is the client's token: one outside HTTP's set reads `OTHER`,
    /// so made-up methods cannot each take a row (review); the standard ones
    /// are kept as sent.
    #[test]
    fn made_up_methods_share_one_label() {
        let labels: std::collections::BTreeSet<String> = (0..300).map(|n| route_label(&format!("M{n}X"), "/")).collect();
        assert_eq!(labels.into_iter().collect::<Vec<_>>(), vec!["OTHER /".to_string()]);
        for method in STANDARD_METHODS {
            assert_eq!(route_label(method, "/a"), format!("{method} /a"));
        }
    }

    /// Both web models build their label with this function.
    ///
    /// The rule above is a pure function, and a pure function passes whatever
    /// the callers do: put `format!("{method} {path}")` back in either call site
    /// and every assertion above stays green while the table fills exactly as
    /// before. Only the call sites can say the shaping is reached, and there are
    /// two of them because PHP runs in a different process under
    /// `--web-isolation=pool|request`.
    #[test]
    fn every_web_model_labels_through_the_shaper() {
        for (source, what) in [
            (include_str!("worker.rs"), "the default prefork worker"),
            (
                include_str!("handler_broker/process/handler.rs"),
                "the isolated handler child",
            ),
        ] {
            assert!(
                source.contains("probe_route::route_label("),
                "{what} must shape its route label, not format the raw path"
            );
        }
    }
}
