//! Purpose:
//! Exposes the stable, panic-free `elephc_iconv_*` C ABI that compiled PHP programs
//! reach through their `__rt_iconv_*` runtime helpers.
//!
//! Called from:
//! - `__rt_iconv_call` and `__rt_iconv_call_bool`, through the `_elephc_iconv_*_fn` slots.
//!
//! Key details:
//! - The whole extension surface goes through one entry point, so the generated runtime
//!   publishes exactly two function pointers.
//! - Panics are caught at the boundary and degrade to PHP `false`; unwinding into
//!   generated assembly is never allowed.
//! - The caller owns the argument block and must release the result block afterwards.

pub mod args;
pub mod dispatch;
pub mod result;

use std::panic::{catch_unwind, AssertUnwindSafe};

pub use args::{IconvArgSlot, IconvCallArgs};
pub use result::IconvResultBlock;

/// Runs one staged iconv operation.
///
/// # Safety
/// `args` must point at a fully staged argument block and `out` at writable storage for
/// one result block. Every present string slot must describe a readable byte range that
/// stays valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_iconv_call(
    args: *const IconvCallArgs,
    out: *mut IconvResultBlock,
) {
    if out.is_null() {
        return;
    }
    IconvResultBlock::reset(out);
    if args.is_null() {
        return;
    }
    let request = &*args;
    // A panic here would unwind into generated assembly, so it degrades to PHP false.
    let _ = catch_unwind(AssertUnwindSafe(|| dispatch::dispatch(request, out)));
}

/// Releases both owned payloads of a result block.
///
/// # Safety
/// `block` must point at a result block filled in by [`elephc_iconv_call`] that has not
/// been released yet.
#[no_mangle]
pub unsafe extern "C" fn elephc_iconv_release(block: *mut IconvResultBlock) {
    if block.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| IconvResultBlock::release(block)));
}
