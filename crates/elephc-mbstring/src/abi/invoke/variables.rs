//! Purpose:
//! Runs mb_convert_variables over live caller roots with one shared encoding choice.
//!
//! Called from:
//! - The protected mbstring invocation coordinator for the variadic reference operation.
//!
//! Key details:
//! - Only the two encoding arguments are copied; the caller keeps root storage live.
//! - Deprecations are delivered before source-list callbacks and any variable mutation.
//! - Request accounting is committed after host callbacks without retaining a state borrow.

use super::*;
use crate::encoding::EncodingList;
use crate::variables::{LiveFailure, VariablePlan, hash_child, host::{HostAdapter, HostError}};
use elephc_builtin_contract::mbstring_abi::variables::{MbInvokeHostV6, MbVariableHandleV1};

unsafe extern "C" {
    fn elephc_mbstring_variable_clone_v1(value: *const u8) -> *mut u8;
    fn elephc_mbstring_variable_box_v1(tag: u64, low: u64, high: u64) -> *mut u8;
    fn elephc_mbstring_variable_reference_new_v1(tag: u64) -> *mut u8;
    fn elephc_mbstring_variable_release_v1(value: *mut u8);
}

/// Owns temporary by-value callable roots until conversion and all host callbacks finish.
struct TemporaryRoots(Vec<*mut u8>);

impl Drop for TemporaryRoots {
    fn drop(&mut self) {
        for reference in self.0.drain(..) {
            unsafe { elephc_mbstring_variable_release_v1(reference); }
        }
    }
}

/// Expands the AOT variadic reference container while leaving eval's flat arguments untouched.
///
/// # Safety
/// The V6 host and all argument pointers must refer to live invocation storage. In packed mode,
/// the fourth input is the compiler-owned indexed or associative variadic container.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_variables_native_invoke_v1(
    op: u32, args: *const *const c_void, count: u64, strict: u32,
    host: *const MbInvokeHostV1, out: *mut MbResultV1,
) -> i32 {
    if out.is_null() { return RuntimeBuiltinStatus::RuntimeFatal as i32; }
    unsafe { out.write(MbResultV1::default()); }
    let Some(table) = (unsafe { (host as *const MbInvokeHostV6).as_ref() }) else {
        return RuntimeBuiltinStatus::RuntimeFatal as i32;
    };
    let context = table.base.base.base.base.base.context as *const u64;
    if context.is_null() { return RuntimeBuiltinStatus::RuntimeFatal as i32; }
    let packed = unsafe { context.add(1).read_unaligned() };
    if packed == 0 {
        return unsafe { elephc_mbstring_invoke_v1(op, args, count, strict, host, out) };
    }
    if packed != 1 || count != 4 || args.is_null() {
        return RuntimeBuiltinStatus::RuntimeFatal as i32;
    }
    let supplied = unsafe { std::slice::from_raw_parts(args, 4) };
    let tail = supplied[3].cast::<u8>();
    if tail.is_null() { return RuntimeBuiltinStatus::RuntimeFatal as i32; }
    let kind = unsafe { tail.sub(8).cast::<u64>().read_unaligned() } & 0xff;
    if kind != 2 && kind != 3 { return RuntimeBuiltinStatus::RuntimeFatal as i32; }
    let length = unsafe { tail.cast::<u64>().read_unaligned() };
    let Ok(length) = usize::try_from(length) else { return RuntimeBuiltinStatus::RuntimeFatal as i32; };
    if kind == 2 {
        let Some(bytes) = length.checked_mul(8).and_then(|bytes| bytes.checked_add(24)) else {
            return RuntimeBuiltinStatus::RuntimeFatal as i32;
        };
        if bytes > isize::MAX as usize { return RuntimeBuiltinStatus::RuntimeFatal as i32; }
    }
    let mut expanded = Vec::new();
    if expanded.try_reserve_exact(length.saturating_add(3)).is_err() {
        return RuntimeBuiltinStatus::RuntimeFatal as i32;
    }
    let mut temporary = TemporaryRoots(Vec::new());
    if temporary.0.try_reserve_exact(length).is_err() {
        return RuntimeBuiltinStatus::RuntimeFatal as i32;
    }
    expanded.extend_from_slice(&supplied[..3]);
    for index in 0..length {
        let (marker, tag, low, high) = if kind == 2 {
            let marker = unsafe { tail.add(24 + index * 8).cast::<*const u64>().read_unaligned() };
            if marker.is_null() { return RuntimeBuiltinStatus::RuntimeFatal as i32; }
            (marker, unsafe { marker.read_unaligned() }, unsafe { marker.add(1).read_unaligned() },
                unsafe { marker.add(2).read_unaligned() })
        } else {
            let Some(Some(child)) = (unsafe { hash_child(tail.cast_mut(), index) }) else {
                return RuntimeBuiltinStatus::RuntimeFatal as i32;
            };
            let marker = child.words[0] as usize as *const u64;
            (marker, child.words[2], unsafe { marker.read_unaligned() }, unsafe { marker.add(1).read_unaligned() })
        };
        if tag == 11 {
            let reference = low as usize as *const c_void;
            if reference.is_null() { return RuntimeBuiltinStatus::RuntimeFatal as i32; }
            expanded.push(reference);
            continue;
        }
        if kind == 2 && tag == 7 && high == 1 {
            expanded.push(marker.cast());
            continue;
        }
        if kind == 3 && tag == 7 {
            let boxed = low as usize as *const u64;
            if boxed.is_null() { return RuntimeBuiltinStatus::RuntimeFatal as i32; }
            if unsafe { boxed.read_unaligned() } == 11 {
                let reference = unsafe { boxed.add(1).read_unaligned() } as usize as *const c_void;
                if reference.is_null() { return RuntimeBuiltinStatus::RuntimeFatal as i32; }
                expanded.push(reference);
                continue;
            }
            if unsafe { boxed.read_unaligned() } == 7 && unsafe { boxed.add(2).read_unaligned() } == 1 {
                expanded.push(boxed.cast());
                continue;
            }
        }
        let value = if kind == 2 || tag == 7 {
            unsafe { elephc_mbstring_variable_clone_v1((if kind == 2 { marker } else { low as usize as *const u64 }).cast()) }
        } else {
            unsafe { elephc_mbstring_variable_box_v1(tag, low, high) }
        };
        if value.is_null() { return RuntimeBuiltinStatus::RuntimeFatal as i32; }
        let reference = unsafe { elephc_mbstring_variable_reference_new_v1(7) };
        if reference.is_null() {
            unsafe { elephc_mbstring_variable_release_v1(value); }
            return RuntimeBuiltinStatus::RuntimeFatal as i32;
        }
        unsafe { reference.cast::<u64>().write_unaligned(value as usize as u64); }
        temporary.0.push(reference);
        expanded.push(reference.cast());
    }
    unsafe { elephc_mbstring_invoke_v1(op, expanded.as_ptr(), expanded.len() as u64, strict, host, out) }
}

/// Applies arity and outer coercion before selecting V6 storage capabilities.
pub(super) unsafe fn invoke(
    args: *const *const c_void, count: u64, strict: u32,
    host: *const MbInvokeHostV1, session: &mut Session,
) -> Result<Outcome, Status> {
    if strict > 1 { return Err(Status::Fatal); }
    let operation = RuntimeBuiltinId::MbConvertVariables;
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).ok_or(Status::Fatal)?;
    let count = usize::try_from(count).map_err(|_| Status::Fatal)?;
    if let Some(bytes) = crate::coercion::arity_error_contract(contract, count) {
        return Ok(Outcome { bytes, ..Outcome::empty(PREPARED_ARGUMENT_COUNT_ERROR) });
    }
    if args.is_null() || count > isize::MAX as usize / std::mem::size_of::<*const c_void>() {
        return Err(Status::Fatal);
    }
    unsafe { session.initialize(host, count)?; }
    let table = session.variable_host().ok_or(Status::Fatal)?;
    let args = unsafe { std::slice::from_raw_parts(args, count) };
    unsafe { session.clone_argument(0, args[0])?; session.clone_argument(1, args[1])?; }
    let to = match unsafe { prepare_argument(session, contract, 0, strict != 0)? } {
        Ok(Argument::String(bytes)) => bytes,
        Err(bytes) => return Ok(Outcome { bytes, ..Outcome::empty(RESULT_TYPE_ERROR) }),
        _ => return Err(Status::Fatal),
    };
    let resolved = match REQUEST.with(|state| state.borrow_mut().resolve_encoding(
        Some(&to), contract.name, 1, "to_encoding",
    )) {
        Ok(resolved) => resolved,
        Err(error) => return Ok(Outcome::error(error)),
    };
    if let Some(message) = resolved.deprecation {
        unsafe { session.diagnostic(8192, format!("mb_convert_variables(): {message}").as_bytes())?; }
    }
    let from = match unsafe { prepare_argument(session, contract, 1, strict != 0)? } {
        Ok(value) => value,
        Err(bytes) => return Ok(Outcome { bytes, ..Outcome::empty(RESULT_TYPE_ERROR) }),
    };
    let plan = match from {
        Argument::String(bytes) => REQUEST.with(|state| VariablePlan::prepare_with_destination(
            &state.borrow(), resolved, EncodingList::CommaSeparated(&bytes), true,
        )),
        Argument::Array(_, catalog) => {
            let names = match unsafe { encoding_list::prepare_names(session, 1, operation)? } {
                Ok(names) => names.into_iter().map(|name| name.name().as_bytes().to_vec()).collect::<Vec<_>>(),
                Err(error) => return Ok(Outcome::error(error)),
            };
            REQUEST.with(|state| VariablePlan::prepare_with_destination(
                &state.borrow(), resolved, EncodingList::Array(&names), !catalog,
            ))
        },
        _ => return Err(Status::Fatal),
    };
    let plan = match plan { Ok(plan) => plan, Err(error) => return Ok(Outcome::error(error)) };
    let roots = args[2..].iter().map(|&argument| {
        MbVariableHandleV1::root(argument).ok_or(Status::Fatal)
    }).collect::<Result<Vec<_>, _>>()?;
    let mut adapter = HostAdapter::new(&table).ok_or(Status::Fatal)?;
    let (converted, errors) = plan.convert(&mut adapter, &roots);
    REQUEST.with(|state| state.borrow_mut().record_illegal_chars(errors));
    match converted {
        Ok(source) => Ok(Outcome::string(Ok(source.name().as_bytes().to_vec()))),
        Err(LiveFailure::Recursive) => Ok(warning(b"Cannot handle recursive references")),
        Err(LiveFailure::Undetectable) => Ok(warning(b"Unable to detect encoding")),
        Err(LiveFailure::Host(HostError::Fatal)) => Err(Status::Fatal),
        Err(LiveFailure::Host(HostError::Pending)) => Err(Status::Pending),
    }
}

/// Returns PHP false with an ordinary mbstring warning after a completed traversal.
fn warning(message: &[u8]) -> Outcome {
    let mut outcome = Outcome::boolean(false);
    outcome.diagnostics.extend_from_slice(b"Warning: mb_convert_variables(): ");
    outcome.diagnostics.extend_from_slice(message);
    outcome.diagnostics.push(b'\n');
    outcome
}
