//! Purpose:
//! Provides explicit no-sampler probe exports on targets without Unix SIGPROF primitives.
//!
//! Called from:
//! - Generated `--probe` runtime hooks when a non-Unix artifact is linked.
//! - The core runtime's optional monitoring function-pointer slots.
//!
//! Key details:
//! - Sampling and local endpoint activation are unavailable; they cannot silently collect nothing.
//! - Event hooks are inert because a Windows build cannot enter a sampled window.
//! - The handshake/fingerprint API remains in `crate::handshake`, independent of these stubs.

use std::sync::atomic::{AtomicBool, Ordering};

/// Compiler-embedded symbol-table entry retained for the generated ABI.
#[repr(C)]
pub struct SymtabEntry {
    pub address: u64,
    pub name_ptr: u64,
    pub name_len: u64,
}

static UNAVAILABLE_REPORTED: AtomicBool = AtomicBool::new(false);

/// Emits one explicit diagnostic if a Windows binary reaches a Unix-only sampler hook.
fn report_unavailable() {
    if !UNAVAILABLE_REPORTED.swap(true, Ordering::Relaxed) {
        eprintln!("elephc-probe: SIGPROF sampling is unavailable on this target");
    }
}

/// A Windows binary cannot enter a sampled event window.
#[no_mangle]
pub extern "C" fn elephc_probe_event_active() -> u32 {
    0
}

/// Keeps the optional event ABI callable while no sampled window can exist.
#[no_mangle]
pub extern "C" fn elephc_probe_note_io() {}

/// Keeps the optional event ABI callable while no sampled window can exist.
#[no_mangle]
pub extern "C" fn elephc_probe_note_wait(_nanoseconds: u64) {}

/// Keeps the optional event ABI callable while no sampled window can exist.
#[no_mangle]
pub extern "C" fn elephc_probe_note_network() {}

/// Keeps the optional event ABI callable while no sampled window can exist.
#[no_mangle]
pub extern "C" fn elephc_probe_note_network_wait(_nanoseconds: u64) {}

/// Refuses sampler initialization instead of installing a non-equivalent timer.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_init(
    _table: *const SymtabEntry,
    _len: usize,
    _key: *const u8,
) {
    report_unavailable();
}

/// Leaves routing inactive because no sample can consume its route tag.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_set_route(_route: *const u8, _len: usize) {}

/// Refuses a query rather than authenticating a request for a nonexistent sampler.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_verify_query(_ptr: *const u8, _len: usize) -> u32 {
    report_unavailable();
    0
}

/// Reports that a SIGPROF timer cannot be rearmed on this target.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_rearm() {
    report_unavailable();
}

/// Keeps generated shutdown hooks safe while making lack of sampler explicit.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_disarm() {}

/// Keeps generated shutdown hooks safe while making lack of sampler explicit.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_dump() {}

/// There is no sampled profile to expose on this target.
pub fn current_folded_profile() -> Option<String> {
    None
}

/// There is no sampled profile to answer on this target.
pub fn sampled_answer() -> String {
    "elephc-probe: SIGPROF sampling is unavailable on this target\n".to_string()
}

/// There is no sampled event report on this target.
pub fn event_report(_base: usize) -> String {
    String::new()
}
