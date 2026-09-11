//! Purpose:
//! Models protected exact-key graph reads and identity leases for independent V3 ABI tests.
//!
//! Called from:
//! - The invocation fixture's version-three callback table.
//!
//! Key details:
//! - Fixture arguments already live for the call; explicit lease owners exercise failure cleanup.
//! - Values preserve shared Rc graph identities and dereference current reference cells lazily.

use super::*;

/// Publishes a separately releasable identity lease before returning an injected pin failure.
pub(super) unsafe extern "C" fn pin(context: *mut c_void, input: *const c_void, out: *mut *mut c_void) -> i32 {
    let host = unsafe { &mut *context.cast::<Host>() };
    let fault = host.enter("pin");
    let value = if input.is_null() { Php::Null } else { unsafe { &*input.cast::<Php>() }.clone() };
    unsafe { *out = host.own(value); }
    fault.map_or(0, |fault| fault.status)
}

/// Reads the current reference value and publishes both value and identity owners before status.
pub(super) unsafe extern "C" fn graph_value(
    context: *mut c_void, array: *const MbArraySourceV2, cursor: *mut u64, out: *mut MbArrayEntryV3,
) -> i32 {
    let host = unsafe { &mut *context.cast::<Host>() };
    let fault = host.enter("graph_value");
    let Php::Array(values) = (unsafe { &*(*array).retained.cast::<Php>() }) else { return 1; };
    let index = unsafe { *cursor as usize };
    if let Some(value) = values.get(index) {
        let value = by_value(value);
        unsafe {
            (*out).kind = ITER_ENTRY;
            (*out).owner = host.own(value.clone());
            (*out).original = host.own(value);
            (*out).key = MbHostValueV1 { tag: HOST_INT, lo: index as u64, hi: 0 };
            if !fault.is_some_and(|fault| fault.malformed) { *cursor += 1; }
        }
    } else if fault.is_some_and(|fault| fault.malformed) {
        unsafe {
            (*out).owner = host.own(Php::Null);
            (*out).original = host.own(Php::Null);
        }
    }
    fault.map_or(0, |fault| fault.status)
}
