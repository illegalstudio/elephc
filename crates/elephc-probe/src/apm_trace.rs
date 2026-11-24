//! Purpose:
//! W3C Trace Context for a transaction-monitoring build (`--with-apm`), which carries no
//! profiler: each web request continues the trace its `traceparent` header names, or
//! starts one, with a span of its own, and curl sends that context to the next hop. The
//! trace id is also kept with the request's error, so an error group points at a trace a
//! tracing backend holds.
//!
//! Called from:
//! - The web server, through the runtime's `elephc_instr_trace_fn` slot, which main's
//!   prologue fills with `elephc_probe_trace_begin` when no instrumentation claims it.
//! - `apm_errors::commit`, for the trace id of the request an error ended.
//!
//! Key details:
//! - The context is published as `ELEPHC_TRACEPARENT`, which the bridges read through the
//!   shared monitoring contract, and the runtime's `elephc_trace_active` word says it is
//!   there: curl sends it without the exact profiler's capture being on.
//! - Nothing happens unless the service counts (`--apm`).

use std::sync::atomic::{AtomicU64, Ordering};

/// Address of the runtime's `elephc_trace_active` word, published by main's prologue in a
/// build without instrumentation; 0 otherwise.
#[no_mangle]
pub static mut elephc_probe_trace_active: usize = 0;

thread_local! {
    /// The current request's trace id, 32 lowercase hex digits; empty without one. Per
    /// thread: a request's trace starts, and its error is committed, on the thread that runs
    /// its PHP.
    static CURRENT_TRACE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Whether `value` is a version-00 W3C `traceparent` with non-zero ids.
pub(crate) fn valid_traceparent(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 55 || &bytes[..3] != b"00-" || bytes[35] != b'-' || bytes[52] != b'-' {
        return false;
    }
    let hex = |part: &[u8]| part.iter().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte));
    let (trace, span, flags) = (&bytes[3..35], &bytes[36..52], &bytes[53..55]);
    hex(trace) && hex(span) && hex(flags) && trace.iter().any(|b| *b != b'0') && span.iter().any(|b| *b != b'0')
}

/// `count` random bytes as lowercase hex: unpredictable enough for ids, not for secrets.
fn random_hex(count: usize) -> String {
    use std::hash::{BuildHasher as _, Hasher as _};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let mut out = String::with_capacity(count * 2);
    while out.len() < count * 2 {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(SEQUENCE.fetch_add(1, Ordering::Relaxed));
        hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        hasher.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", hasher.finish()));
    }
    out.truncate(count * 2);
    out
}

/// The context of a request answered under `inbound`: the caller's trace and flags with a
/// span of this service's, or a new sampled trace.
pub(crate) fn context_for(inbound: Option<&str>) -> String {
    let span = random_hex(8);
    match inbound.filter(|value| valid_traceparent(value)) {
        Some(value) => format!("00-{}-{span}-{}", &value[3..35], &value[53..55]),
        None => format!("00-{}-{span}-01", random_hex(16)),
    }
}

/// The current request's trace id; empty without one.
pub(crate) fn current_trace() -> String {
    CURRENT_TRACE.with_borrow(String::clone)
}

/// Starts the trace context of a web request: `traceparent`/`len` is its inbound header
/// (null without one). Same shape as the instrumentation's `elephc_instr_trace_begin`,
/// whose slot it fills; the route is not needed here.
///
/// # Safety
/// A non-null `traceparent` describes bytes valid for this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_trace_begin(traceparent: *const u8, len: usize, _route: *const u8, _route_len: usize) {
    let word = unsafe { std::ptr::addr_of!(elephc_probe_trace_active).read_volatile() };
    if !crate::apm_runtime::APM_ON.load(Ordering::Relaxed) {
        return;
    }
    let inbound = (!traceparent.is_null() && len == 55)
        .then(|| std::str::from_utf8(unsafe { std::slice::from_raw_parts(traceparent, len) }).ok())
        .flatten();
    let context = context_for(inbound);
    CURRENT_TRACE.with_borrow_mut(|trace| *trace = context[3..35].to_string());
    std::env::set_var("ELEPHC_TRACEPARENT", &context);
    if word != 0 {
        unsafe { (*(word as *const AtomicU64)).store(1, Ordering::Relaxed) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An inbound context is continued with a span of our own; anything else starts a
    /// trace; a malformed header is not trusted.
    #[test]
    fn a_request_continues_its_callers_trace_or_starts_one() {
        let inbound = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let ours = context_for(Some(inbound));
        assert!(valid_traceparent(&ours), "{ours}");
        assert!(ours.starts_with("00-4bf92f3577b34da6a3ce929d0e0e4736-") && ours.ends_with("-01"), "{ours}");
        assert!(!ours.contains("00f067aa0ba902b7"), "a span of our own: {ours}");
        let fresh = context_for(None);
        assert!(valid_traceparent(&fresh) && !fresh.contains("4bf92f35"), "{fresh}");
        let forged = context_for(Some("00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01"));
        assert!(!forged.contains("4bf92f35") && !forged.contains("4BF92F35"), "upper case is not W3C: {forged}");
        assert_ne!(context_for(None), context_for(None), "ids differ");
    }
}
