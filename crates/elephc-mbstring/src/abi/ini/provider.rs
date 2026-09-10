//! Purpose:
//! Owns the process-lifetime PCRE2 MIME provider and its compiled-handle lifetime.
//!
//! Called from:
//! - Native provider registration, shared INI validation, and output MIME selection.
//!
//! Key details:
//! - Provider callbacks are native-only and cannot execute PHP or unwind through Rust.
//! - Every published handle is released, including malformed provider responses.

use std::{ffi::c_void, sync::OnceLock};
use elephc_builtin_contract::mbstring_abi::ini::MbMimeRegexV1;
use crate::state::MimeRegexError;

static PROVIDER: OnceLock<MbMimeRegexV1> = OnceLock::new();

/// Separates normal PCRE2 compile diagnostics from missing or malformed native providers.
pub(super) enum Failure { Compile(MimeRegexError), Fatal }

/// Pins a complete provider, accepting idempotent registration but rejecting a different allocator/function set.
pub(super) fn install(provider: MbMimeRegexV1) -> bool {
    if provider.version != 1 || provider.size as usize != std::mem::size_of::<MbMimeRegexV1>()
        || provider.compile.is_none() || provider.matches.is_none() || provider.free.is_none() || provider.error.is_none() { return false; }
    let current = PROVIDER.get_or_init(|| provider);
    std::ptr::fn_addr_eq(current.compile.unwrap(), provider.compile.unwrap())
        && std::ptr::fn_addr_eq(current.matches.unwrap(), provider.matches.unwrap())
        && std::ptr::fn_addr_eq(current.free.unwrap(), provider.free.unwrap())
        && std::ptr::fn_addr_eq(current.error.unwrap(), provider.error.unwrap())
}

/// Validates one trimmed C-string pattern through the installed native PCRE2 functions.
pub(super) fn validate(pattern: &[u8]) -> Result<(), Failure> {
    compile(pattern).map(drop)
}

/// Matches one accepted MIME expression, treating native match errors as PHP's nonmatch.
pub(in crate::abi) fn matches(pattern: &[u8], subject: &[u8]) -> Result<bool, ()> {
    let compiled = compile(pattern).map_err(|_| ())?;
    let status = unsafe { compiled.provider.matches.unwrap()(compiled.handle, subject.as_ptr(), subject.len() as u64) };
    if status > 1 { return Err(()); }
    Ok(status == 1)
}

/// Keeps a native compiled pattern under its provider's allocator without any PHP destructor.
struct Compiled { provider: &'static MbMimeRegexV1, handle: *mut c_void }

impl Drop for Compiled {
    /// Releases the native handle on validation, matching, and contained Rust failure paths.
    fn drop(&mut self) { unsafe { self.provider.free.unwrap()(self.handle); } }
}

/// Validates provider result framing and transfers only a successful nonnull compiled handle.
fn compile(pattern: &[u8]) -> Result<Compiled, Failure> {
    let provider = PROVIDER.get().ok_or(Failure::Fatal)?;
    let mut handle: *mut c_void = std::ptr::null_mut();
    let mut offset = 0;
    let status = unsafe { provider.compile.unwrap()(&mut handle, pattern.as_ptr(), pattern.len() as u64, &mut offset) };
    let allocated = !handle.is_null();
    if status == 0 && allocated { return Ok(Compiled { provider, handle }); }
    if allocated { unsafe { provider.free.unwrap()(handle); } }
    if status == 0 { return Err(Failure::Fatal); }
    if status < 0 || allocated { return Err(Failure::Fatal); }
    let mut message = [0_u8; 256];
    let length = unsafe { provider.error.unwrap()(status, message.as_mut_ptr(), message.len() as u64) };
    if length < 0 || length as usize >= message.len() || message[length as usize] != 0 { return Err(Failure::Fatal); }
    Err(Failure::Compile(MimeRegexError { offset, message: message[..length as usize].to_vec() }))
}
