//! Purpose:
//! Coordinates PHP argument preparation, protected host actions, and shared mbstring dispatch.
//!
//! Called from:
//! - Native/eval callers through elephc_mbstring_invoke_v1.
//!
//! Key details:
//! - Value arguments are copied before ordered coercions; capture outputs retain caller identity.
//! - Array snapshots follow outer coercions; no host callback holds the request-state borrow.
//! - Host owners remain in an explicit arena until cleanup, including after Rust panics.

mod host;
mod encoding_list;
mod entities;
mod conversion;
mod regex;
mod capture;
mod query;
mod ini;
mod output;
mod callback;
pub use capture::elephc_mbstring_capture_v1;
pub use output::{elephc_mbstring_output_v1, elephc_mbstring_output_invoke_v1};
pub use callback::elephc_mbstring_callback_invoke_v1;
#[cfg(test)]
mod tests;

use std::ffi::c_void;
use elephc_builtin_contract::{RuntimeBuiltinStatus, mbstring_abi::{coercion::*, host::*, invoke::*}};
use crate::coercion::Prepared;
use super::*;
use host::{Session, Status};

/// Executes a PHP call after shared arity/coercion planning and protected host callbacks.
/// Success transfers an MbResultV1, including engine PHP error messages for host materialization.
/// PendingThrowable transfers no result; RuntimeFatal transfers an empty fatal result.
/// Ordinary operation diagnostics retain their existing result-buffer delivery protocol.
///
/// # Safety
/// `out` must be writable aligned uninitialized or released storage. For valid arity, `args`
/// describes `count` borrowed boxed values, and `host` points to a complete immutable V1 through V5 table.
/// For MbEreg/MbEregi with three arguments, V4 is required and the third argument identifies
/// writable caller reference storage retained by pin_value, not a snapshot of its contents.
/// MbParseStr requires V5 and a pinned writable output at argument index one.
/// Host callbacks obey their non-unwinding and ownership contracts through final cleanup.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_invoke_v1(
    op: u32, args: *const *const c_void, count: u64, strict: u32,
    host: *const MbInvokeHostV1, out: *mut MbResultV1,
) -> i32 {
    unsafe { boundary(out, |session| invoke(op, args, count, strict, host, session)) }
}

/// Contains shared operation panics and always retires its host arena before publishing a result.
unsafe fn boundary(out: *mut MbResultV1, invoke: impl FnOnce(&mut Session) -> Result<Completed, Status>) -> i32 {
    if out.is_null() { return RuntimeBuiltinStatus::RuntimeFatal as i32; }
    unsafe { out.write(MbResultV1::default()); }
    let mut session = Session::default();
    let result = catch_unwind(AssertUnwindSafe(|| invoke(&mut session))).unwrap_or(Err(Status::Fatal));
    let cleanup = unsafe { session.cleanup() };
    let result = match (result, cleanup) {
        (result, Ok(())) => result,
        (Err(status), Err(cleanup)) => Err(status.merge(cleanup)),
        (Ok(_), Err(status)) => Err(status),
    };
    match result {
        Ok(mut outcome) => {
            unsafe { out.write(std::mem::take(&mut outcome.0)); }
            RuntimeBuiltinStatus::Success as i32
        },
        Err(Status::Pending) => RuntimeBuiltinStatus::PendingThrowable as i32,
        Err(Status::Fatal) => {
            unsafe { out.write(Outcome::fatal().into_wire()); }
            RuntimeBuiltinStatus::RuntimeFatal as i32
        },
    }
}

/// Owns a completed wire result until host cleanup permits its transfer to the caller.
struct Completed(MbResultV1);

impl From<Outcome> for Completed {
    /// Publishes ordinary scalar/graph buffers under the same final-cleanup guard as INI leases.
    fn from(outcome: Outcome) -> Self { Self(outcome.into_wire()) }
}

impl Drop for Completed {
    /// Releases buffers and optional INI leases without invoking PHP when host cleanup rejects a result.
    fn drop(&mut self) { unsafe { elephc_mbstring_release_v1(&mut self.0); } }
}

/// Validates arity before touching host inputs, then dispatches only after all callbacks return.
unsafe fn invoke(
    op: u32, args: *const *const c_void, count: u64, strict: u32,
    host: *const MbInvokeHostV1, session: &mut Session,
) -> Result<Completed, Status> {
    if strict > 1 { return Err(Status::Fatal); }
    let operation = RuntimeBuiltinId::from_u32(op).filter(|id| id.is_mbstring()).ok_or(Status::Fatal)?;
    if operation == RuntimeBuiltinId::MbParseStr {
        return unsafe { query::invoke(args, count, strict, host, session) }.map(Into::into);
    }
    if matches!(operation, RuntimeBuiltinId::MbEreg | RuntimeBuiltinId::MbEregi) {
        return unsafe { capture::invoke(operation, args, count, strict, host, session) }.map(Into::into);
    }
    let regex_entry_encoding = matches!(operation, RuntimeBuiltinId::MbEregReplace | RuntimeBuiltinId::MbEregiReplace)
        .then(|| REGEX.with(|session| session.encoding()));
    let count = usize::try_from(count).map_err(|_| Status::Fatal)?;
    if let Some(bytes) = crate::coercion::arity_error(operation, count) {
        return Ok(Outcome { bytes, ..Outcome::empty(PREPARED_ARGUMENT_COUNT_ERROR) }.into());
    }
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).ok_or(Status::Fatal)?;
    let mut values = match unsafe { prepare_values(args, count, strict != 0, host, contract, session)? } {
        Ok(values) => values,
        Err(bytes) => return Ok(Outcome { bytes, ..Outcome::empty(RESULT_TYPE_ERROR) }.into()),
    };
    if operation == RuntimeBuiltinId::SharedIni { return unsafe { ini::invoke(&values, session) }; }
    if matches!(operation, RuntimeBuiltinId::MbEncodeNumericentity | RuntimeBuiltinId::MbDecodeNumericentity) {
        return unsafe { entities::invoke(operation, &values, session) }.map(Into::into);
    }
    if operation == RuntimeBuiltinId::MbConvertEncoding {
        return unsafe { conversion::invoke(values, session) }.map(Into::into);
    }
    for (index, value) in values.iter_mut().enumerate() {
        if let Argument::Array(root, catalog) = value {
            if matches!((operation, index), (RuntimeBuiltinId::MbDetectOrder, 0) | (RuntimeBuiltinId::MbDetectEncoding, 1)) {
                let graph = match unsafe { encoding_list::prepare(session, index, operation)? } {
                    Ok(graph) => graph, Err(error) => return Ok(Outcome::error(error).into()),
                };
                *value = Argument::Snapshot(graph, *catalog);
                continue;
            }
            let graph = unsafe { session.snapshot_argument(index, *root)? };
            *value = Argument::Snapshot(graph.encode(), *catalog);
        }
    }
    let args: Vec<_> = values.iter().map(Argument::wire).collect::<Option<_>>().ok_or(Status::Fatal)?;
    if operation.is_mbregex() {
        let encoding = regex_entry_encoding.unwrap_or_else(|| REGEX.with(|session| session.encoding()));
        return unsafe { regex::invoke(operation, &args, session, encoding) }.map(Into::into);
    }
    REQUEST.with(|state| {
        let mut result = unsafe { dispatch(operation, &args, &mut state.borrow_mut()) };
        if operation == RuntimeBuiltinId::MbListEncodings && session.has_array_values() && result.kind == RESULT_STRING_ARRAY {
            result.kind = RESULT_ENCODING_CATALOG;
        }
        Ok(result.into())
    })
}

/// Copies all value arguments before ordered coercion, keeping ownership in the enclosing session.
unsafe fn prepare_values(args: *const *const c_void, count: usize, strict: bool, host: *const MbInvokeHostV1,
    contract: &elephc_builtin_contract::BuiltinContract, session: &mut Session) -> Result<Result<Vec<Argument>, Vec<u8>>, Status> {
    if count > isize::MAX as usize / std::mem::size_of::<*const c_void>() || (count != 0 && args.is_null()) { return Err(Status::Fatal); }
    unsafe { session.initialize(host, count)?; }
    let args = if count == 0 { &[] } else { unsafe { std::slice::from_raw_parts(args, count) } };
    for (index, &argument) in args.iter().enumerate() { unsafe { session.clone_argument(index, argument)?; } }
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        match unsafe { prepare_argument(session, contract, index, strict)? } {
            Ok(value) => values.push(value), Err(bytes) => return Ok(Err(bytes)),
        }
    }
    Ok(Ok(values))
}

/// Prepares one retained argument through the common planner and protected scalar host actions.
unsafe fn prepare_argument(session: &mut Session, contract: &elephc_builtin_contract::BuiltinContract,
    index: usize, strict: bool) -> Result<Result<Argument, Vec<u8>>, Status> {
    let input = unsafe { session.describe(index)? };
    let decoded = unsafe { super::coercion::decode_input(&input) }.ok_or(Status::Fatal)?;
    let identity = if input.kind == HOST_STRING && input.flags == INPUT_INI_IDENTITY {
        let string = super::ini::lookup_string(input.value).map_err(|_| Status::Fatal)?;
        let crate::coercion::Input::String(bytes) = decoded else { return Err(Status::Fatal); };
        if &*string != bytes { return Err(Status::Fatal); }
        Some(string)
    } else { None };
    let plan = crate::coercion::prepare_contract(contract, index, decoded, strict).ok_or(Status::Fatal)?;
    // Copy borrowed class/string bytes before releasing metadata or invoking any PHP callback.
    let value = plan.value.map(|value| match value {
        Prepared::Null => Argument::Null,
        Prepared::Bool(value) => Argument::Bool(value),
        Prepared::Int(value) => Argument::Int(value),
        Prepared::String(value) => identity.map_or_else(|| Argument::String(value.into_owned()), Argument::IniString),
        Prepared::Array => Argument::Array(MbHostValueV1 { tag: input.kind, lo: input.value, hi: 0 },
            input.flags & INPUT_ENCODING_CATALOG != 0),
        Prepared::FormatFloat(bits) => Argument::Float(bits),
        Prepared::InvokeStringable => Argument::Stringable,
        Prepared::ResolveCallable => Argument::Callable,
    });
    unsafe { session.release_temporary()?; }
    for diagnostic in plan.diagnostics {
        unsafe { session.diagnostic(diagnostic.level, &diagnostic.message)?; }
    }
    Ok(match value {
        Ok(Argument::Float(bits)) => Ok(Argument::String(unsafe { session.format_float(bits)? })),
        Ok(Argument::Stringable) => Ok(Argument::String(unsafe { session.stringable(index)? })),
        Ok(Argument::Callable) => { unsafe { session.resolve_callback(index)?; } Ok(Argument::Callable) },
        other => other,
    })
}

/// Keeps prepared strings and graph buffers owned by Rust until request dispatch returns.
enum Argument {
    Null, Bool(bool), Int(i64), String(Vec<u8>), Snapshot(Vec<u8>, bool),
    IniString(crate::state::IniString),
    Array(MbHostValueV1, bool), Float(u64), Stringable, Callable,
}

impl Argument {
    /// Borrows a completed argument without transferring its Rust buffer to the native allocator.
    fn wire(&self) -> Option<MbArgV1> {
        Some(match self {
            Self::Null => MbArgV1::null(),
            Self::Bool(value) => MbArgV1::boolean(*value),
            Self::Int(value) => MbArgV1::integer(*value),
            Self::String(bytes) => MbArgV1::string(bytes),
            Self::IniString(string) => elephc_builtin_contract::mbstring_abi::ini::string_argument(string.identity()),
            Self::Snapshot(bytes, catalog) => MbArgV1 { value: if *catalog { ARRAY_ENCODING_CATALOG } else { 0 },
                ..MbArgV1::array(bytes) },
            _ => return None,
        })
    }
}
