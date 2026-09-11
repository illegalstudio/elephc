//! Purpose:
//! Executes mb_ereg/mb_eregi through common coercion and protected caller-output callbacks.
//!
//! Called from:
//! - The typed mbstring invocation dispatcher and the compatible capture-specific C entry.
//!
//! Key details:
//! - The neutral catalog owns both PHP signatures; reference contents are never coerced or copied.
//! - Empty-pattern validation precedes output initialization and destructor-visible state changes.
//! - The common boundary retires writer and argument owners after success, throws, or Rust panics.

use super::*;
use std::cell::Cell;
use crate::regex::{Event, Limits, RegisterKey};
use elephc_builtin_contract::mbstring_abi::array::{ArrayGraph, Key, Value};

/// Searches with common PHP argument preparation and an optional caller-owned capture output.
/// `ignore_case` is zero for mb_ereg or one for mb_eregi. Supplied output arguments require
/// a V4 host; two-argument calls also accept V1 through V3. This entry does not register
/// either PHP builtin in a compiler or eval backend.
///
/// # Safety
/// The argument pointer range, strictness flag, callback table, and result storage obey
/// elephc_mbstring_invoke_v1's contract. The third argument identifies writable caller
/// reference storage retained by pin_value, not a detached value snapshot.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_capture_v1(
    ignore_case: u32, args: *const *const c_void, count: u64, strict: u32,
    host: *const MbInvokeHostV1, out: *mut MbResultV1,
) -> i32 {
    let operation = match ignore_case {
        0 => RuntimeBuiltinId::MbEreg,
        1 => RuntimeBuiltinId::MbEregi,
        _ => return unsafe { boundary(out, |_| Err(Status::Fatal)) },
    };
    unsafe { super::elephc_mbstring_invoke_v1(operation.as_u32(), args, count, strict, host, out) }
}

/// Validates the signature before host access, then prepares values while pinning output identity.
pub(super) unsafe fn invoke(operation: RuntimeBuiltinId, args: *const *const c_void, count: u64, strict: u32,
    host: *const MbInvokeHostV1, session: &mut Session) -> Result<Outcome, Status> {
    if strict > 1 { return Err(Status::Fatal); }
    let ignore_case = match operation {
        RuntimeBuiltinId::MbEreg => false,
        RuntimeBuiltinId::MbEregi => true,
        _ => return Err(Status::Fatal),
    };
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).ok_or(Status::Fatal)?;
    let count = usize::try_from(count).map_err(|_| Status::Fatal)?;
    if let Some(bytes) = crate::coercion::arity_error_contract(contract, count) {
        return Ok(Outcome { bytes, ..Outcome::empty(PREPARED_ARGUMENT_COUNT_ERROR) });
    }
    if args.is_null() { return Err(Status::Fatal); }
    unsafe { session.initialize(host, count)?; }
    if count == 3 && !session.has_capture_output() { return Err(Status::Fatal); }
    let args = unsafe { std::slice::from_raw_parts(args, count) };
    for (index, &argument) in args.iter().enumerate() {
        if index == 2 { unsafe { session.pin_argument(index, argument)?; } }
        else { unsafe { session.clone_argument(index, argument)?; } }
    }
    let mut strings = Vec::with_capacity(2);
    for index in 0..2 {
        match unsafe { prepare_argument(session, contract, index, strict != 0)? } {
            Ok(Argument::String(bytes)) => strings.push(bytes),
            Err(bytes) => return Ok(Outcome { bytes, ..Outcome::empty(RESULT_TYPE_ERROR) }),
            _ => return Err(Status::Fatal),
        }
    }
    unsafe { execute(&strings[0], &strings[1], ignore_case, count == 3, session) }
}

/// Retains pending exceptions while successful initialization permits the PHP body to continue.
unsafe fn execute(pattern: &[u8], subject: &[u8], ignore_case: bool, output: bool,
    host: &mut Session) -> Result<Outcome, Status> {
    let host = RefCell::new(host);
    let status = Cell::new(None);
    let mut exceptions = Vec::new();
    let matched = REGEX.with(|regex| regex.capture(pattern, subject, ignore_case, || {
        if !output { return true; }
        let (ready, error) = unsafe { host.borrow_mut().initialize_capture(2) };
        status.set(error);
        ready
    }, || {
        let (stack, retry) = REQUEST.with(|state| state.borrow().ini_regex_limits());
        Limits::from_ini(stack, retry, false)
    }, &mut |event| match event {
        Event::Warning(bytes) if status.get().is_none() && exceptions.is_empty() => {
            if let Err(error) = unsafe { host.borrow().diagnostic(2, &bytes) } { status.set(Some(error)); }
        },
        Event::Warning(_) => {},
        Event::Exception(error) => exceptions.push(error),
    }));
    let matched = match matched {
        Ok(matched) => matched,
        Err(_) => return Err(status.get().unwrap_or(Status::Fatal)),
    };
    let found = matched.is_some();
    if output && status.get() != Some(Status::Fatal) {
        if let Some(matched) = matched {
            let entries = matched.registers(false).into_iter().map(|(key, value)| {
                let key = match key { RegisterKey::Index(index) => Key::Int(index as i64), RegisterKey::Name(name) => Key::String(name) };
                (key, value.map_or(Value::Bool(false), Value::String))
            }).collect();
            let graph = ArrayGraph::new(0, vec![entries]).ok_or(Status::Fatal)?;
            if let Err(error) = unsafe { host.borrow().fill_capture(&graph.encode()) } {
                status.set(Some(status.get().map_or(error, |previous| previous.merge(error))));
            }
        }
    }
    if let Some(status) = status.get() { return Err(status); }
    Ok(Outcome::error_chain(exceptions).unwrap_or_else(|| Outcome::boolean(found)))
}
