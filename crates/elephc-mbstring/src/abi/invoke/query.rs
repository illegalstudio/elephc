//! Purpose:
//! Coordinates mb_parse_str coercion, output initialization, live query writes, and request state.
//!
//! Called from:
//! - The typed mbstring invocation dispatcher with a protected V5 host.
//!
//! Key details:
//! - Output identity is pinned; only the source argument is copied and coerced by value.
//! - Initialization precedes settings capture, and aggregate identification is committed last.
//! - PHP bodies continue after pending callback exceptions; fatal host failures stop safely.

use super::*;
use crate::input::{Query, registration};

/// Applies the neutral signature before touching host pointers or caller reference storage.
pub(super) unsafe fn invoke(args: *const *const c_void, count: u64, strict: u32,
    host: *const MbInvokeHostV1, session: &mut Session) -> Result<Outcome, Status> {
    if strict > 1 { return Err(Status::Fatal); }
    let contract = elephc_builtin_contract::lookup_id(RuntimeBuiltinId::MbParseStr.builtin_id()).ok_or(Status::Fatal)?;
    let count = usize::try_from(count).map_err(|_| Status::Fatal)?;
    if let Some(bytes) = crate::coercion::arity_error_contract(contract, count) {
        return Ok(Outcome { bytes, ..Outcome::empty(PREPARED_ARGUMENT_COUNT_ERROR) });
    }
    if args.is_null() { return Err(Status::Fatal); }
    unsafe { session.initialize(host, count)?; }
    if !session.has_query_output() { return Err(Status::Fatal); }
    let args = unsafe { std::slice::from_raw_parts(args, count) };
    unsafe { session.clone_argument(0, args[0])?; session.pin_argument(1, args[1])?; }
    let source = match unsafe { prepare_argument(session, contract, 0, strict != 0)? } {
        Ok(Argument::String(bytes)) => bytes,
        Err(bytes) => return Ok(Outcome { bytes, ..Outcome::empty(RESULT_TYPE_ERROR) }),
        _ => return Err(Status::Fatal),
    };
    unsafe { execute(&source, session) }
}

/// Preserves live host mutations and emits diagnostics only while no PHP exception is pending.
unsafe fn execute(source: &[u8], host: &mut Session) -> Result<Outcome, Status> {
    let (ready, mut pending) = unsafe { host.initialize_capture(1) };
    if !ready { return Err(pending.unwrap_or(Status::Fatal)); }
    match unsafe { parse(source, host, &mut pending) } {
        Ok(result) => finish(result, pending),
        Err(status) => Err(pending.map_or(status, |pending| pending.merge(status))),
    }
}

/// Runs query body stages after successful output initialization, preserving accumulated throws.
unsafe fn parse(source: &[u8], host: &mut Session, pending: &mut Option<Status>) -> Result<bool, Status> {
    let (to, candidates, strict) = REQUEST.with(|state| {
        let state = state.borrow();
        (state.internal_encoding(), state.http_input_encodings().to_vec(), state.strict_detection())
    });
    let (config, status) = unsafe { host.query_configuration(QUERY_CONFIG_ENTRY)? };
    *pending = pending.or(status);
    let query = match Query::decode(source, &config.separators, config.max_variables) {
        Ok(query) => query,
        Err(error) => {
            unsafe { diagnostic(host, pending, error.message().as_bytes())?; }
            REQUEST.with(|state| state.borrow_mut().set_http_input_identification(None));
            return Ok(false);
        },
    };
    let identified = query.identify(&candidates, strict);
    if identified.warning { unsafe { diagnostic(host, pending, b"Unable to detect encoding")?; } }
    if let Some(from) = identified.encoding {
        for pair in query.into_pairs() {
            let pair = REQUEST.with(|state| pair.convert(from, to, &mut state.borrow_mut()));
            let (value, status) = unsafe { host.query_filter(&pair.name, pair.value)? };
            *pending = pending.or(status);
            let Some(value) = value else { continue; };
            let (config, status) = unsafe { host.query_configuration(QUERY_CONFIG_FIELD)? };
            *pending = pending.or(status);
            let plan = registration(&pair.name, config.max_nesting);
            if plan.is_empty() { continue; }
            let (exceeded, status) = unsafe { host.query_register(&plan, &value)? };
            *pending = pending.or(status);
            if exceeded {
                let (policy, status) = unsafe { host.query_configuration(QUERY_CONFIG_DIAGNOSTIC)? };
                *pending = pending.or(status);
                if !policy.display_errors {
                    let message = format!("Input variable nesting level exceeded {}. To increase the limit change max_input_nesting_level in php.ini.", config.max_nesting);
                    unsafe { diagnostic(host, pending, message.as_bytes())?; }
                }
            }
        }
    }
    REQUEST.with(|state| state.borrow_mut().set_http_input_identification(identified.encoding));
    Ok(identified.encoding.is_some())
}

/// Attaches the PHP function name and retains a thrown warning while allowing later query writes.
unsafe fn diagnostic(host: &Session, pending: &mut Option<Status>, message: &[u8]) -> Result<(), Status> {
    if pending.is_some() { return Ok(()); }
    let mut bytes = b"mb_parse_str(): ".to_vec();
    bytes.extend_from_slice(message);
    match unsafe { host.diagnostic(2, &bytes) } {
        Ok(()) => Ok(()),
        Err(Status::Pending) => { *pending = Some(Status::Pending); Ok(()) },
        Err(status) => Err(status),
    }
}

/// Publishes a boolean only after a completed query without a retained PHP throwable.
fn finish(result: bool, pending: Option<Status>) -> Result<Outcome, Status> {
    pending.map_or_else(|| Ok(Outcome::boolean(result)), Err)
}
