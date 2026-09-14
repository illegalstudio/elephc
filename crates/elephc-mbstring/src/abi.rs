//! Purpose:
//! Exposes the shared mbstring engine through a panic-contained, versioned C ABI.
//!
//! Called from:
//! - Generated mbstring runtime helpers for both AOT and Magician calls.
//!
//! Key details:
//! - No host callbacks run while Rust owns the request-state borrow.
//! - PHP exceptions are returned as owned messages, never unwound through Rust.
//! - Only this static library owns thread-local state; Magician does not embed it.

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};

use elephc_builtin_contract::mbstring_abi::*;
use elephc_builtin_contract::RuntimeBuiltinId;

use crate::error::MbError;
use crate::state::State;

mod arguments;
mod check;
mod conversion;
mod detect;
mod coercion;
mod encodings;
mod entities;
mod mime;
mod info;
mod http_input;
mod ini;
mod invoke;
mod response;
mod settings;
mod snapshot;
mod restore;
mod capture_apply;
mod query_apply;
mod regex;
mod exception;
pub use exception::elephc_mbstring_exception_at_v1;
mod text;
pub use restore::{elephc_mbstring_restore_v1, elephc_mbstring_restore_ini_v1};
pub use capture_apply::elephc_mbstring_capture_apply_v1;
pub use query_apply::elephc_mbstring_query_apply_v1;
pub use snapshot::elephc_mbstring_snapshot_v1;
pub use coercion::{elephc_mbstring_arity_v1, elephc_mbstring_prepare_v1};
pub use invoke::{elephc_mbstring_invoke_v1, elephc_mbstring_capture_v1, elephc_mbstring_output_v1, elephc_mbstring_output_invoke_v1};
pub use invoke::elephc_mbstring_callback_invoke_v1;
pub use response::{elephc_mbstring_response_header_v1, elephc_mbstring_response_info_v1, elephc_mbstring_response_commit_v1};
pub use regex::{elephc_mbstring_regex_available_v1, elephc_mbstring_regex_provider_v1};
pub use ini::{elephc_mbstring_ini_v1, elephc_mbstring_configure_v1, elephc_mbstring_core_encoding_v1, elephc_mbstring_mime_provider_v1,
    elephc_mbstring_core_ini_v1, elephc_mbstring_query_configuration_v1,
    elephc_mbstring_ini_string_retain_v1, elephc_mbstring_ini_string_release_v1,
    elephc_mbstring_native_string_bind_v1, elephc_mbstring_native_string_lookup_v1,
    elephc_mbstring_native_string_fresh_v1, elephc_mbstring_native_string_literal_v1,
    elephc_mbstring_native_string_persist_v1, elephc_mbstring_native_string_resolve_v1,
    elephc_mbstring_native_string_copy_v1, elephc_mbstring_native_string_forget_v1, elephc_mbstring_native_string_reset_v1};
#[cfg(test)]
mod tests;
use arguments::Arguments;

thread_local! { static REQUEST: RefCell<State> = RefCell::new(ini::initial_state()); }
thread_local! { static REGEX: crate::regex::Session = regex::initial_session(); }

/// Executes one operation over borrowed arguments and transfers result ownership to `out`.
///
/// # Safety
/// `out` must be writable, aligned, uninitialized or previously released storage.
/// `args` must describe `count` valid slots; string payloads must remain readable
/// throughout the call. Null pointers are allowed only for empty ranges.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_call_v1(op: u32, args: *const MbArgV1, count: u64, out: *mut MbResultV1) {
    if out.is_null() { return; }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let Some(operation) = RuntimeBuiltinId::from_u32(op) else { return Outcome::unsupported(); };
        if !operation.is_mbstring() || !operation.supports_arity(count as usize) {
            return Outcome::unsupported();
        }
        if args.is_null() && count != 0 { return Outcome::fatal(); }
        let args = if count == 0 { &[] } else { unsafe { std::slice::from_raw_parts(args, count as usize) } };
        REQUEST.with(|state| unsafe { dispatch(operation, args, &mut state.borrow_mut()) })
    })).unwrap_or_else(|_| Outcome::fatal());
    unsafe { out.write(result.into_wire()); }
}

/// Releases every bridge-owned payload and leaves an empty result safe to release again.
///
/// # Safety
/// `out` must be null or point to a live result produced by this ABI. Copying a
/// result does not duplicate ownership, so copies must never be released separately.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_release_v1(out: *mut MbResultV1) {
    if out.is_null() { return; }
    let result = unsafe { out.replace(MbResultV1::default()) };
    unsafe { ini::release_result(&result); reclaim(result.bytes, result.len); reclaim(result.diagnostics, result.diagnostics_len); }
}

/// Resets this thread's request settings and caches before the host starts a fresh request.
#[no_mangle]
pub extern "C" fn elephc_mbstring_reset_v1() {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        REQUEST.with(|state| *state.borrow_mut() = ini::initial_state());
        REGEX.with(|regex| {
            regex.configure_encoding(ini::initial_state().ini_regex_encoding());
            regex.reset_request();
        });
    }));
}

/// Holds Rust ownership until an entire operation has completed without panicking.
struct Outcome { kind: u64, value: i64, bytes: Vec<u8>, diagnostics: Vec<u8> }

impl Outcome {
    /// Creates a result with no owned payloads.
    fn empty(kind: u64) -> Self { Self { kind, value: 0, bytes: Vec::new(), diagnostics: Vec::new() } }

    /// Rejects operations outside this ABI's implemented surface without a PHP value.
    fn unsupported() -> Self { Self::empty(RESULT_UNSUPPORTED) }

    /// Reports internal failure without disguising it as a successful integer result.
    fn fatal() -> Self { Self::empty(RESULT_FATAL) }

    /// Preserves the engine's exception class and complete binary message.
    fn error(error: MbError) -> Self {
        let (kind, bytes) = match error {
            MbError::Value(message) => (RESULT_VALUE_ERROR, message.into_bytes()),
            MbError::ValueBytes(message) => (RESULT_VALUE_ERROR, message),
            MbError::Runtime(message) => (RESULT_ERROR, message.into_bytes()),
        };
        Self { bytes, ..Self::empty(kind) }
    }

    /// Creates an integer result without owned payload storage.
    fn integer(value: i64) -> Self { Self { value, ..Self::empty(RESULT_INT) } }

    /// Preserves the distinction between false, zero, and an empty string.
    fn boolean(value: bool) -> Self { Self { value: value as i64, ..Self::empty(RESULT_BOOL) } }

    /// Transfers a successful string or preserves its exception class and bytes.
    fn string(result: crate::error::MbResult<Vec<u8>>) -> Self {
        match result {
            Ok(bytes) => Self { bytes, ..Self::empty(RESULT_STRING) },
            Err(error) => Self::error(error),
        }
    }

    /// Packs independent string-array elements into one releasable wire buffer.
    fn strings(result: crate::error::MbResult<Vec<Vec<u8>>>) -> Self {
        match result {
            Ok(values) => Self { value: values.len() as i64, bytes: encode_string_array(&values),
                ..Self::empty(RESULT_STRING_ARRAY) },
            Err(error) => Self::error(error),
        }
    }

    /// Transfers exact-capacity byte buffers to the runtime's explicit release contract.
    fn into_wire(self) -> MbResultV1 {
        let (bytes, len) = leak(self.bytes);
        let (diagnostics, diagnostics_len) = leak(self.diagnostics);
        MbResultV1 { kind: self.kind, value: self.value, bytes, len, diagnostics, diagnostics_len }
    }
}

/// Borrows validated argument slots and delegates operation semantics to the shared engine.
unsafe fn dispatch(operation: RuntimeBuiltinId, args: &[MbArgV1], state: &mut State) -> Outcome {
    let Some(arguments) = (unsafe { Arguments::new(operation, args) }) else { return Outcome::fatal(); };
    match operation {
        RuntimeBuiltinId::MbParseStr | RuntimeBuiltinId::SharedIni => Outcome::fatal(),
        operation if operation.is_mbregex() => regex::dispatch(operation, &arguments, state),
        RuntimeBuiltinId::MbRegexEncoding | RuntimeBuiltinId::MbRegexSetOptions =>
            regex::settings(operation, &arguments),
        RuntimeBuiltinId::MbGetInfo => info::dispatch(&arguments, state),
        RuntimeBuiltinId::MbHttpInput => http_input::dispatch(&arguments, state),
        RuntimeBuiltinId::MbEncodeMimeheader => mime::encode(&arguments, state),
        RuntimeBuiltinId::MbDecodeMimeheader => Outcome::string(Ok(crate::mime::decode_header(
            arguments.string(0), state.internal_encoding()))),
        RuntimeBuiltinId::MbEncodeNumericentity | RuntimeBuiltinId::MbDecodeNumericentity =>
            entities::dispatch(operation, &arguments, state),
        RuntimeBuiltinId::MbCheckEncoding => check::dispatch(&arguments, state),
        RuntimeBuiltinId::MbConvertEncoding => conversion::dispatch(&arguments, state),
        RuntimeBuiltinId::MbDetectEncoding => detect::dispatch(&arguments, state),
        RuntimeBuiltinId::MbListEncodings => Outcome::strings(Ok(crate::encoding::Encoding::all()
            .map(|encoding| encoding.name().as_bytes().to_vec()).collect())),
        RuntimeBuiltinId::MbEncodingAliases | RuntimeBuiltinId::MbPreferredMimeName =>
            encodings::dispatch(operation, &arguments, state),
        RuntimeBuiltinId::MbLanguage | RuntimeBuiltinId::MbInternalEncoding | RuntimeBuiltinId::MbHttpOutput
        | RuntimeBuiltinId::MbSubstituteCharacter | RuntimeBuiltinId::MbDetectOrder =>
            settings::dispatch(operation, &arguments, state),
        _ => text::dispatch(operation, &arguments, state),
    }
}

/// Reads string or packed-array bytes after rejecting malformed range metadata.
unsafe fn bytes(slot: &MbArgV1) -> Option<&[u8]> {
    if !matches!(slot.kind, ARG_STRING | ARG_ARRAY) || slot.len > isize::MAX as u64 { return None; }
    if slot.len == 0 { return Some(&[]); }
    if slot.bytes.is_null() { return None; }
    Some(unsafe { std::slice::from_raw_parts(slot.bytes, slot.len as usize) })
}

/// Moves an owned buffer across the ABI without exporting Rust's vector capacity.
fn leak(bytes: Vec<u8>) -> (*mut u8, u64) {
    if bytes.is_empty() { return (std::ptr::null_mut(), 0); }
    let bytes = bytes.into_boxed_slice();
    let len = bytes.len() as u64;
    (Box::into_raw(bytes).cast(), len)
}

/// Reclaims one exact-capacity buffer previously transferred by this bridge.
unsafe fn reclaim(bytes: *mut u8, len: u64) {
    if !bytes.is_null() {
        unsafe { drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(bytes, len as usize))); }
    }
}
