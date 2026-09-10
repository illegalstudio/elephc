//! Purpose:
//! Connects native header and terminal-output events to the shared response MIME state.
//!
//! Called from:
//! - Header/stdout runtime emitters and MbOutputHostV1 metadata reads.
//!
//! Key details:
//! - Protected warning callbacks execute after all response-state borrows end.
//! - Header results own their normalized wire bytes; metadata snapshots borrow request strings.
//! - Header lists and status codes remain the host's responsibility.

use super::*;
use std::ffi::c_void;
use elephc_builtin_contract::mbstring_abi::{ini::MbIniHostV1, output_handler::MbOutputInfoV1};

/// Validates a header, updates accepted MIME state, and returns its owned wire bytes or false.
/// Origin zero labels header() diagnostics; origin one labels mb_output_handler() diagnostics.
/// The host forwards successful bytes to its actual header sink before releasing the result.
///
/// # Safety
/// Input bytes are readable for `length`, with null allowed only for zero. `host` is a complete
/// immutable V1 diagnostic table whose context and non-unwinding callback live through return.
/// `out` is aligned writable empty/released result storage with normal mbstring release ownership.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_response_header_v1(input: *const u8, length: u64, origin: u32,
    host: *const MbIniHostV1, out: *mut MbResultV1) -> i32 {
    if out.is_null() { return 1; }
    unsafe { out.write(MbResultV1::default()); }
    let result = catch_unwind(AssertUnwindSafe(|| {
        if origin > 1 || length > isize::MAX as u64 || (length != 0 && input.is_null()) { return Err(1); }
        let host = unsafe { host.as_ref() }.ok_or(1)?;
        if host.version != 1 || host.size as usize != std::mem::size_of::<MbIniHostV1>() { return Err(1); }
        let diagnostic = host.diagnostic.ok_or(1)?;
        let line = if length == 0 { &[] } else { unsafe { std::slice::from_raw_parts(input, length as usize) } };
        let result = REQUEST.with(|state| state.borrow_mut().response.header(line));
        match result {
            Ok(Some(bytes)) => Ok(Outcome { bytes, ..Outcome::empty(RESULT_STRING) }),
            Ok(None) => Ok(Outcome::boolean(false)),
            Err(warning) => {
                let caller = if origin == 0 { "header" } else { "mb_output_handler" };
                let message = format!("{caller}(): {warning}");
                match unsafe { diagnostic(host.context, 2, message.as_ptr(), message.len() as u64) } {
                    0 => Ok(Outcome::boolean(false)), 2 => Err(2), _ => Err(1),
                }
            },
        }
    }));
    match result {
        Ok(Ok(outcome)) => { unsafe { out.write(outcome.into_wire()); } 0 },
        Ok(Err(2)) => 2,
        _ => 1,
    }
}

/// Publishes request MIME/default metadata together with the caller's live output-handler flag.
///
/// # Safety
/// `out` is aligned writable MbOutputInfoV1 storage. Optional `context` points to a live aligned
/// u64 handler flag, or is null to select false. Returned string ranges borrow this thread's
/// request until its next mutation; callers copy them before another host callback or reset.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_response_info_v1(context: *mut c_void, out: *mut MbOutputInfoV1) -> i32 {
    if out.is_null() { return 1; }
    unsafe { out.write(MbOutputInfoV1::default()); }
    catch_unwind(AssertUnwindSafe(|| REQUEST.with(|state| {
        let state = state.borrow();
        let response = &state.response;
        unsafe { out.write(MbOutputInfoV1 {
            mimetype: response.mimetype.as_ref().map_or(std::ptr::null(), |value| value.as_ptr()),
            mimetype_len: response.mimetype.as_ref().map_or(0, |value| value.len() as u64),
            default_mimetype: response.default_mimetype.as_ptr(), default_mimetype_len: response.default_mimetype.len() as u64,
            send_default_content_type: u64::from(response.send_default_content_type),
            in_handler: u64::from(!context.is_null() && *context.cast::<u64>() != 0),
        }); }
        0
    }))).unwrap_or(1)
}

/// Commits MIME defaults only when nonempty bytes reach the terminal response sink.
#[no_mangle]
pub extern "C" fn elephc_mbstring_response_commit_v1(length: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| REQUEST.with(|state| {
        state.borrow_mut().response.commit(length);
        0
    }))).unwrap_or(1)
}
