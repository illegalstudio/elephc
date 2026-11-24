//! Purpose:
//! Connects curl transfer boundaries to elephc's pay-for-use network monitoring slots.
//!
//! Called from:
//! - `crate::abi` for easy transfers and connection upkeep.
//! - `crate::multi` for multi-handle progress and blocking selection.
//!
//! Key details:
//! - Slot reads are inert unless a monitor capture is active right now.
//! - Trace context is read only through the validated shared contract helper.

use elephc_monitoring_contract::{active_traceparent, EventHooks};

extern "C" {
    static elephc_monitor_event_active_fn: usize;
    static elephc_instr_network_fn: usize;
    static elephc_instr_network_wait_fn: usize;
    static elephc_monitor_active: u64;
    /// 1 while a transaction-monitoring build published the request's trace context.
    static elephc_trace_active: u64;
}

/// Reads the network-event slots published by the compiled runtime.
pub(crate) fn hooks() -> EventHooks {
    EventHooks::new(
        active(),
        unsafe { std::ptr::addr_of!(elephc_monitor_event_active_fn).read() },
        unsafe { std::ptr::addr_of!(elephc_instr_network_fn).read() },
        unsafe { std::ptr::addr_of!(elephc_instr_network_wait_fn).read() },
    )
}

/// Returns whether a monitor capture is active right now.
pub(crate) fn active() -> bool {
    unsafe { std::ptr::addr_of!(elephc_monitor_active).read() != 0 }
}

/// Returns the current validated traceparent only while monitoring is active: an exact
/// capture, or a transaction-monitoring build that published the request's context. An
/// `ELEPHC_TRACEPARENT` merely inherited from the environment is never sent.
pub(crate) fn traceparent() -> Option<String> {
    let published = unsafe { std::ptr::addr_of!(elephc_trace_active).read() != 0 };
    active_traceparent(active() || published)
}

/// Sets the standalone test runtime's exact-monitor activity flag.
#[cfg(all(test, elephc_curl_native))]
pub(crate) fn set_test_active(active: bool) {
    slot_stub::set_active(active);
}

// Standalone bridge tests do not link a compiled elephc runtime, so they provide
// inert slots in their own executable. Production staticlibs never define them.
#[cfg(test)]
mod slot_stub {
    #[no_mangle]
    static elephc_monitor_event_active_fn: usize = 0;
    #[no_mangle]
    static elephc_instr_network_fn: usize = 0;
    #[no_mangle]
    static elephc_instr_network_wait_fn: usize = 0;
    #[no_mangle]
    static mut elephc_monitor_active: u64 = 0;
    #[no_mangle]
    static mut elephc_trace_active: u64 = 0;

    /// Updates the test-only activity symbol read by the production helper.
    #[cfg(elephc_curl_native)]
    pub(super) fn set_active(active: bool) {
        unsafe {
            std::ptr::addr_of_mut!(elephc_monitor_active).write(u64::from(active));
        }
    }

    /// Updates the test-only trace-published symbol.
    pub(super) fn set_trace_published(published: bool) {
        unsafe {
            std::ptr::addr_of_mut!(elephc_trace_active).write(u64::from(published));
        }
    }
}

#[cfg(test)]
mod trace_tests {
    use super::*;

    /// A transaction-monitoring build's published context is sent without a capture on;
    /// the same value merely present in the environment is not.
    #[test]
    fn a_published_context_is_sent_and_an_inherited_one_is_not() {
        let context = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let previous = std::env::var_os("ELEPHC_TRACEPARENT");
        unsafe { std::env::set_var("ELEPHC_TRACEPARENT", context) };
        slot_stub::set_trace_published(false);
        let inherited = traceparent();
        slot_stub::set_trace_published(true);
        let published = traceparent();
        slot_stub::set_trace_published(false);
        match previous {
            Some(value) => unsafe { std::env::set_var("ELEPHC_TRACEPARENT", value) },
            None => unsafe { std::env::remove_var("ELEPHC_TRACEPARENT") },
        }
        assert_eq!(inherited, None);
        assert_eq!(published.as_deref(), Some(context));
    }
}
