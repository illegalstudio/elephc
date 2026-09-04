//! Purpose:
//! Defines the typed monitoring policy shared by the compiler and bridge crates,
//! plus dependency-free helpers for calling runtime event slots safely.
//!
//! Called from:
//! - `crate::linker::bridges` and `crate::ir::runtime_fn` for CI-audited policy metadata.
//! - Bridge crates that report database or network operations to `elephc monitor`.
//!
//! Key details:
//! - Bridges supply slot addresses, so this crate never creates unresolved runtime symbols.
//! - Event consumers gate their own capture windows; timing reads the clock only while active.

use std::time::Instant;

/// The externally visible I/O category attributed by the monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoKind {
    /// Database statements, kept separate for query budgets and N+1 analysis.
    Database,
    /// Outgoing network transfers such as curl requests.
    Network,
}

/// Whether an I/O boundary propagates the active W3C trace context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceContextPolicy {
    /// The operation has no outgoing trace-context boundary.
    NotApplicable,
    /// User code may propagate the context, but the runtime does not inject it.
    Manual,
    /// The runtime injects the active context unless the user supplied one.
    Automatic,
}

/// Whether an I/O boundary reports its blocked duration separately from self time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitPolicy {
    /// Generic function timing is the only duration signal.
    GenericTiming,
    /// The bridge reports the actual blocking span through a monitor event slot.
    Measured,
}

/// Required observability decision for a bridge or typed runtime operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitoringPolicy {
    /// No policy has been reviewed. Audits reject this on bridge-backed and I/O operations.
    Unspecified,
    /// Ordinary function enter/exit timing fully describes this operation.
    GenericTiming,
    /// The operation reports typed I/O events and declares its trace behavior.
    Io {
        /// Category kept distinct in monitor reports and budgets.
        kind: IoKind,
        /// Whether blocked time is measured explicitly.
        wait: WaitPolicy,
        /// W3C Trace Context behavior at the external boundary.
        trace_context: TraceContextPolicy,
    },
    /// Compiler/runtime infrastructure rather than a PHP-visible work boundary.
    Infrastructure {
        /// Machine-visible justification required by the policy audit.
        reason: &'static str,
    },
}

impl MonitoringPolicy {
    /// Returns whether the policy records a reviewed monitoring decision.
    pub const fn is_reviewed(self) -> bool {
        !matches!(self, Self::Unspecified)
    }

    /// Returns whether the policy emits typed I/O events.
    pub const fn is_evented(self) -> bool {
        matches!(self, Self::Io { .. })
    }
}

/// Runtime function-pointer slots used by one bridge-side event category.
#[derive(Debug, Clone, Copy)]
pub struct EventHooks {
    /// Whether bridge-side timing and trace work is active right now.
    active: bool,
    /// Optional consumer callback for a monitoring window not represented by `active`.
    active_fn: usize,
    operation_fn: usize,
    wait_fn: usize,
}

impl EventHooks {
    /// Builds hooks from runtime slot addresses already read by the bridge.
    pub const fn new(
        active: bool,
        active_fn: usize,
        operation_fn: usize,
        wait_fn: usize,
    ) -> Self {
        Self {
            active,
            active_fn,
            operation_fn,
            wait_fn,
        }
    }

    /// Returns whether a monitor capture is active right now.
    pub fn is_active(self) -> bool {
        if self.active {
            return true;
        }
        if self.active_fn == 0 {
            return false;
        }
        let function = unsafe {
            std::mem::transmute::<usize, unsafe extern "C" fn() -> u32>(self.active_fn)
        };
        unsafe { function() != 0 }
    }

    /// Sends one category-specific operation to the installed event consumer.
    ///
    /// The consumer owns its capture-window gate. This callback must therefore
    /// remain reachable while bridge-side exact timing is dormant, because the
    /// sampled probe tracks its active window in shared process state instead.
    pub fn note_operation(self) {
        if self.operation_fn == 0 {
            return;
        }
        let function = unsafe {
            std::mem::transmute::<usize, unsafe extern "C" fn()>(self.operation_fn)
        };
        unsafe { function() };
    }

    /// Sends one measured wait duration to the installed event consumer.
    ///
    /// Callers use this when part of a larger timed boundary must be excluded,
    /// such as PHP callback execution nested inside a curl transfer.
    pub fn note_wait(self, ns: u64) {
        if self.wait_fn == 0 {
            return;
        }
        let function = unsafe {
            std::mem::transmute::<usize, unsafe extern "C" fn(u64)>(self.wait_fn)
        };
        unsafe { function(ns) };
    }

    /// Reports a measured boundary after removing nested user-code time.
    pub fn note_wait_excluding(self, elapsed_ns: u64, user_code_ns: u64) {
        self.note_wait(elapsed_ns.saturating_sub(user_code_ns));
    }

    /// Runs `body` and reports its elapsed nanoseconds only while monitoring is active.
    pub fn timed<T>(self, body: impl FnOnce() -> T) -> T {
        if self.wait_fn == 0 || !self.is_active() {
            return body();
        }
        let started = Instant::now();
        let output = body();
        let elapsed = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        self.note_wait(elapsed);
        output
    }
}

/// Returns the active, syntactically valid W3C `traceparent` value for an outgoing request.
pub fn active_traceparent(active: bool) -> Option<String> {
    if !active {
        return None;
    }
    let value = std::env::var("ELEPHC_TRACEPARENT").ok()?;
    valid_traceparent(&value).then_some(value)
}

/// Validates the strict lowercase W3C traceparent shape emitted by the monitor runtime.
pub fn valid_traceparent(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 55 || bytes[2] != b'-' || bytes[35] != b'-' || bytes[52] != b'-' {
        return false;
    }
    if &bytes[..2] != b"00" || &bytes[53..] == b"00" {
        return false;
    }
    let trace_id = &bytes[3..35];
    let span_id = &bytes[36..52];
    trace_id.iter().all(u8::is_ascii_hexdigit)
        && span_id.iter().all(u8::is_ascii_hexdigit)
        && bytes[53..].iter().all(u8::is_ascii_hexdigit)
        && trace_id.iter().any(|byte| *byte != b'0')
        && span_id.iter().any(|byte| *byte != b'0')
        && bytes.iter().all(|byte| !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static OPERATIONS: AtomicU64 = AtomicU64::new(0);
    static WAIT: AtomicU64 = AtomicU64::new(0);
    static EXCLUDED_WAIT: AtomicU64 = AtomicU64::new(0);
    static WINDOW_ACTIVE: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    /// Reports the fixture's consumer-owned monitoring window state.
    unsafe extern "C" fn window_active() -> u32 {
        u32::from(WINDOW_ACTIVE.load(Ordering::Relaxed))
    }

    /// Records one fixture operation through the same C ABI shape as the runtime slot.
    unsafe extern "C" fn note() {
        OPERATIONS.fetch_add(1, Ordering::Relaxed);
    }

    /// Records one fixture wait duration through the runtime slot shape.
    unsafe extern "C" fn wait(ns: u64) {
        WAIT.fetch_add(ns, Ordering::Relaxed);
    }

    /// Records wait for the callback-exclusion fixture without sharing test state.
    unsafe extern "C" fn excluded_wait(ns: u64) {
        EXCLUDED_WAIT.fetch_add(ns, Ordering::Relaxed);
    }

    /// Verifies inactive hooks still forward events but avoid timed callbacks.
    #[test]
    fn dormant_hooks_leave_window_gating_to_the_event_consumer() {
        OPERATIONS.store(0, Ordering::Relaxed);
        WAIT.store(0, Ordering::Relaxed);
        let hooks = EventHooks::new(
            false,
            0,
            note as *const () as usize,
            wait as *const () as usize,
        );
        hooks.note_operation();
        assert_eq!(hooks.timed(|| 7), 7);
        assert_eq!(OPERATIONS.load(Ordering::Relaxed), 1);
        assert_eq!(WAIT.load(Ordering::Relaxed), 0);
    }

    /// Verifies active hooks report one operation and one nonzero timed span.
    #[test]
    fn active_hooks_report_operation_and_wait() {
        OPERATIONS.store(0, Ordering::Relaxed);
        WAIT.store(0, Ordering::Relaxed);
        let hooks = EventHooks::new(
            true,
            0,
            note as *const () as usize,
            wait as *const () as usize,
        );
        hooks.note_operation();
        assert_eq!(hooks.timed(|| 11), 11);
        assert_eq!(OPERATIONS.load(Ordering::Relaxed), 1);
        assert!(WAIT.load(Ordering::Relaxed) > 0);
    }

    /// Verifies a consumer-owned window enables timing without the exact-capture flag.
    #[test]
    fn consumer_window_enables_timing() {
        WAIT.store(0, Ordering::Relaxed);
        WINDOW_ACTIVE.store(true, Ordering::Relaxed);
        let hooks = EventHooks::new(
            false,
            window_active as *const () as usize,
            0,
            wait as *const () as usize,
        );
        assert_eq!(hooks.timed(|| 13), 13);
        assert!(WAIT.load(Ordering::Relaxed) > 0);
        WINDOW_ACTIVE.store(false, Ordering::Relaxed);
    }

    /// User callback time is removed without allowing an oversized value to wrap.
    #[test]
    fn wait_exclusion_is_saturating() {
        EXCLUDED_WAIT.store(0, Ordering::Relaxed);
        let hooks = EventHooks::new(false, 0, 0, excluded_wait as *const () as usize);
        hooks.note_wait_excluding(1_000, 400);
        assert_eq!(EXCLUDED_WAIT.load(Ordering::Relaxed), 600);
        hooks.note_wait_excluding(100, 200);
        assert_eq!(EXCLUDED_WAIT.load(Ordering::Relaxed), 600);
    }

    /// Verifies traceparent validation rejects injection and zero-identity shapes.
    #[test]
    fn traceparent_validation_accepts_only_monitor_shape() {
        assert!(valid_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        ));
        assert!(!valid_traceparent(
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01"
        ));
        assert!(!valid_traceparent(
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01"
        ));
        assert!(!valid_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01"
        ));
        assert!(!valid_traceparent("00-good\r\nx-bad: yes"));
    }
}
