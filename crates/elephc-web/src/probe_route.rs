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

#[cfg(not(test))]
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
}

#[cfg(test)]
#[no_mangle]
static elephc_probe_route_fn: usize = 0;
#[cfg(test)]
#[no_mangle]
static elephc_probe_rearm_fn: usize = 0;
#[cfg(test)]
#[no_mangle]
static elephc_instr_trace_fn: usize = 0;
#[cfg(test)]
#[no_mangle]
static elephc_instr_request_fn: usize = 0;
#[cfg(test)]
#[no_mangle]
static elephc_probe_verify_fn: usize = 0;

type SetRouteFn = unsafe extern "C" fn(*const u8, usize);
type RearmFn = unsafe extern "C" fn();
type TraceFn = unsafe extern "C" fn(*const u8, usize, *const u8, usize);
type RequestFn = unsafe extern "C" fn(u32);
type VerifyFn = unsafe extern "C" fn(*const u8, usize) -> u32;

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
pub fn route_label(method: &str, path: &str) -> String {
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
