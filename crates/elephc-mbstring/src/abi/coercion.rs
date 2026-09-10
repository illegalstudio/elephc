//! Purpose:
//! Exposes PHP argument-count and parameter-coercion planning through the shared C bridge.
//!
//! Called from:
//! - Native/eval adapters before invoking mbstring operations or protected host callbacks.
//!
//! Key details:
//! - Preparation never accesses request state or dereferences host arrays and objects.
//! - Diagnostics are binary framed records; deferred actions run after their delivery.
//! - Owned outputs use the existing release API, including errors and empty strings.

use std::borrow::Cow;
use elephc_builtin_contract::mbstring_abi::{coercion::*, host::*};
use crate::coercion::{self, Input, Prepared};
use super::*;

/// Validates PHP arity before the caller performs any parameter conversions.
///
/// # Safety
/// `out` must be null or writable, aligned, uninitialized or previously released result storage.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_arity_v1(op: u32, count: u64, out: *mut MbCoercionResultV1) {
    if out.is_null() { return; }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let Some(operation) = RuntimeBuiltinId::from_u32(op).filter(|id| id.is_mbstring()) else { return Outcome::fatal(); };
        let Ok(count) = usize::try_from(count) else { return Outcome::fatal(); };
        match coercion::arity_error(operation, count) {
            Some(bytes) => Outcome { bytes, ..Outcome::empty(PREPARED_ARGUMENT_COUNT_ERROR) },
            None => Outcome::boolean(true),
        }
    })).unwrap_or_else(|_| Outcome::fatal());
    unsafe { out.write(result.into_wire()); }
}

/// Prepares one parameter without invoking PHP, borrowing request state, or traversing host values.
///
/// # Safety
/// `input` must be null or a readable aligned descriptor whose declared byte ranges remain valid
/// throughout the call. The host retains the original value until it consumes a deferred action.
/// `out` must be null or writable, aligned, uninitialized or previously released result storage.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_prepare_v1(
    op: u32, index: u32, input: *const MbCoercionInputV1, strict: u32, out: *mut MbCoercionResultV1,
) {
    if out.is_null() { return; }
    let result = catch_unwind(AssertUnwindSafe(|| {
        if strict > 1 { return Outcome::fatal(); }
        let Some(operation) = RuntimeBuiltinId::from_u32(op) else { return Outcome::fatal(); };
        let Some(descriptor) = (unsafe { input.as_ref() }) else { return Outcome::fatal(); };
        let Some(input) = (unsafe { decode_input(descriptor) }) else { return Outcome::fatal(); };
        let Some(plan) = coercion::prepare(operation, index as usize, input, strict != 0) else { return Outcome::fatal(); };
        let mut output = match plan.value {
            Err(bytes) => Outcome { bytes, ..Outcome::empty(RESULT_TYPE_ERROR) },
            Ok(Prepared::Null) => Outcome::empty(PREPARED_NULL),
            Ok(Prepared::Bool(value)) => Outcome::boolean(value),
            Ok(Prepared::Int(value)) => Outcome::integer(value),
            Ok(Prepared::String(Cow::Borrowed(_))) if matches!(input, Input::String(_)) =>
                Outcome::empty(PREPARED_BORROWED_STRING),
            Ok(Prepared::String(bytes)) => Outcome::string(Ok(bytes.into_owned())),
            Ok(Prepared::Array) => Outcome::empty(PREPARED_ARRAY),
            Ok(Prepared::FormatFloat(bits)) => Outcome { value: bits as i64, ..Outcome::empty(PREPARED_FLOAT_STRING) },
            Ok(Prepared::InvokeStringable) => Outcome::empty(PREPARED_STRINGABLE),
            Ok(Prepared::ResolveCallable) => Outcome::empty(PREPARED_CALLABLE),
        };
        for diagnostic in plan.diagnostics {
            output.diagnostics.extend_from_slice(&(diagnostic.level as u64).to_le_bytes());
            output.diagnostics.extend_from_slice(&(diagnostic.message.len() as u64).to_le_bytes());
            output.diagnostics.extend_from_slice(&diagnostic.message);
        }
        output
    })).unwrap_or_else(|_| Outcome::fatal());
    unsafe { out.write(result.into_wire()); }
}

/// Rejects malformed metadata before constructing a borrowed concrete value for the pure planner.
pub(super) unsafe fn decode_input(input: &MbCoercionInputV1) -> Option<Input<'_>> {
    let bytes = if matches!(input.kind, HOST_STRING | INPUT_OBJECT) {
        if input.len > isize::MAX as u64 { return None; }
        if input.len == 0 { &[] }
        else if input.bytes.is_null() { return None; }
        else { unsafe { std::slice::from_raw_parts(input.bytes, input.len as usize) } }
    } else {
        if !input.bytes.is_null() || input.len != 0 { return None; }
        &[]
    };
    if !matches!(input.kind, INPUT_OBJECT | INPUT_RESOURCE | HOST_INDEXED_ARRAY | HOST_ASSOC_ARRAY | HOST_STRING) && input.flags != 0 { return None; }
    match input.kind {
        HOST_NULL if input.value == 0 => Some(Input::Null),
        HOST_INT => Some(Input::Int(input.value as i64)),
        HOST_FLOAT => Some(Input::Float(input.value)),
        HOST_BOOL if input.value <= 1 => Some(Input::Bool(input.value != 0)),
        HOST_STRING if (input.flags == 0 && input.value == 0)
            || (input.flags == INPUT_INI_IDENTITY && input.value != 0 && input.value <= i64::MAX as u64) => Some(Input::String(bytes)),
        HOST_INDEXED_ARRAY | HOST_ASSOC_ARRAY if input.flags & !INPUT_ENCODING_CATALOG == 0 => Some(Input::Array),
        INPUT_OBJECT if input.flags & !INPUT_STRINGABLE == 0 =>
            Some(Input::Object { class: bytes, stringable: input.flags & INPUT_STRINGABLE != 0 }),
        INPUT_RESOURCE if input.flags & !INPUT_CLOSED_RESOURCE == 0 =>
            Some(Input::Resource { closed: input.flags & INPUT_CLOSED_RESOURCE != 0 }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves argument preparation remains available while request state is exclusively borrowed.
    #[test]
    fn mbstring_coercion_abi_has_no_request_borrow() {
        REQUEST.with(|state| {
            let _state = state.borrow_mut();
            let input = MbCoercionInputV1 { kind: HOST_INT, value: 42, bytes: std::ptr::null(), len: 0, flags: 0 };
            let mut output = MbResultV1::default();
            unsafe { elephc_mbstring_prepare_v1(RuntimeBuiltinId::MbStrlen.as_u32(), 0, &input, 0, &mut output); }
            assert_eq!(output.kind, RESULT_STRING);
            assert_eq!(unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize) }, b"42");
            unsafe { elephc_mbstring_release_v1(&mut output); }
        });
    }
}
