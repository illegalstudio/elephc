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

/// Returns the current validated traceparent only while monitoring is active.
pub(crate) fn traceparent() -> Option<String> {
    active_traceparent(active())
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

    /// Updates the test-only activity symbol read by the production helper.
    #[cfg(elephc_curl_native)]
    pub(super) fn set_active(active: bool) {
        unsafe {
            std::ptr::addr_of_mut!(elephc_monitor_active).write(u64::from(active));
        }
    }
}
