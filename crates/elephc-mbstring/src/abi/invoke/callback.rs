//! Purpose:
//! Coordinates ordered PHP arguments and protected replacement callbacks through the shared regex engine.
//!
//! Called from:
//! - Native/eval runtime dispatch through `elephc_mbstring_callback_invoke_v1`.
//!
//! Key details:
//! - Entry encoding precedes coercions; each search samples current request limits.
//! - Host results and resolved callbacks stay in the enclosing explicit cleanup arena.

use super::*;
use elephc_builtin_contract::mbstring_abi::callback::MbCallbackHostV1;
use crate::regex::{CallbackReplacement, Event, Limits, ReplacementError, ReplaceResult};

/// Prepares one PHP callback replacement and transfers only its completed string, null, or false result.
///
/// # Safety
/// `out` is aligned writable empty/released result storage. For valid arity, `args` contains
/// `count` borrowed boxed values, `values_host` obeys elephc_mbstring_invoke_v1, and
/// `callback_host` is a complete immutable V1 table. Both contexts remain live through cleanup;
/// callback boxes are described and released by the value host. No host action unwinds through Rust.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_callback_invoke_v1(args: *const *const c_void, count: u64, strict: u32,
    values_host: *const MbInvokeHostV1, callback_host: *const MbCallbackHostV1, out: *mut MbResultV1) -> i32 {
    unsafe { boundary(out, |host| {
        if strict > 1 { return Err(Status::Fatal); }
        let encoding = REGEX.with(|session| session.encoding());
        let count = usize::try_from(count).map_err(|_| Status::Fatal)?;
        let contract = elephc_builtin_contract::lookup_id(RuntimeBuiltinId::MbEregReplaceCallback.builtin_id()).ok_or(Status::Fatal)?;
        if let Some(bytes) = crate::coercion::arity_error_contract(contract, count) {
            return Ok(Outcome { bytes, ..Outcome::empty(PREPARED_ARGUMENT_COUNT_ERROR) }.into());
        }
        host.initialize_callback_host(callback_host)?;
        let values = match prepare_values(args, count, strict != 0, values_host, contract, host)? {
            Ok(values) => values,
            Err(bytes) => return Ok(Outcome { bytes, ..Outcome::empty(RESULT_TYPE_ERROR) }.into()),
        };
        let (Argument::String(pattern), Argument::Callable, Argument::String(subject)) = (&values[0], &values[1], &values[2])
            else { return Err(Status::Fatal); };
        let options = match values.get(3) {
            None | Some(Argument::Null) => None,
            Some(Argument::String(bytes)) => Some(bytes.as_slice()),
            _ => return Err(Status::Fatal),
        };
        run(CallbackReplacement { pattern, subject, options, encoding }, host).map(Into::into)
    }) }
}

/// Runs callbacks outside request-state borrows and preserves their failure through final arena cleanup.
unsafe fn run(input: CallbackReplacement<'_>, host: &mut Session) -> Result<Outcome, Status> {
    let host = RefCell::new(host);
    let status = std::cell::Cell::new(None);
    let mut exceptions = Vec::new();
    let result = REGEX.with(|session| session.replace_callback(input, || {
        let (stack, retry) = REQUEST.with(|state| state.borrow().ini_regex_limits());
        Limits::from_ini(stack, retry, false)
    }, &mut |event| match event {
        Event::Warning(bytes) if status.get().is_none() && exceptions.is_empty() => {
            if let Err(error) = unsafe { host.borrow().diagnostic(2, &bytes) } { status.set(Some(error)); }
        },
        Event::Warning(_) => {},
        Event::Exception(error) => exceptions.push(error),
    }, |registers| {
        if let Some(status) = status.get() { return Err(status); }
        let graph = super::super::regex::registers(Some(registers));
        if graph.kind != RESULT_ARRAY { return Err(Status::Fatal); }
        unsafe { host.borrow_mut().call_replacement(&graph.bytes) }
    }));
    if let Some(status) = status.get() { return Err(status); }
    if let Some(error) = Outcome::error_chain(exceptions) { return Ok(error); }
    match result {
        Ok(ReplaceResult::InvalidSubject) => Ok(Outcome::empty(RESULT_NULL)),
        Ok(ReplaceResult::Failed) => Ok(Outcome::boolean(false)),
        Ok(ReplaceResult::String(bytes)) => Ok(Outcome::string(Ok(bytes))),
        Err(ReplacementError::Callback(status)) => Err(status),
        Err(ReplacementError::Regex(_)) => Err(Status::Fatal),
    }
}
