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
use crate::input::{InputLimit, Query, registration};

/// One parser pass: whether it completed and the `max_input_vars` limit it hit, if any.
struct QueryRun {
    success: bool,
    limit: Option<InputLimit>,
}

/// Applies the neutral signature before touching host pointers or caller reference storage.
pub(super) unsafe fn invoke(operation: RuntimeBuiltinId, args: *const *const c_void, count: u64, strict: u32,
    host: *const MbInvokeHostV1, session: &mut Session, raw: bool) -> Result<Outcome, Status> {
    if strict > 1 { return Err(Status::Fatal); }
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).ok_or(Status::Fatal)?;
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
    unsafe { execute(&source, session, raw) }
}

/// Preserves live host mutations and emits diagnostics only while no PHP exception is pending.
unsafe fn execute(source: &[u8], host: &mut Session, raw: bool) -> Result<Outcome, Status> {
    let (ready, mut pending) = unsafe { host.initialize_capture(1) };
    if !ready { return Err(pending.unwrap_or(Status::Fatal)); }
    match unsafe { parse(source, host, &mut pending, raw) } {
        Ok(run) => finish(run, pending, raw),
        Err(status) => Err(pending.map_or(status, |pending| pending.merge(status))),
    }
}

/// Runs query body stages after successful output initialization, preserving accumulated throws.
unsafe fn parse(source: &[u8], host: &mut Session, pending: &mut Option<Status>, raw: bool) -> Result<QueryRun, Status> {
    let (to, candidates, strict) = REQUEST.with(|state| {
        let state = state.borrow();
        (state.internal_encoding(), state.http_input_encodings().to_vec(), state.strict_detection())
    });
    let (config, status) = unsafe { host.query_configuration(QUERY_CONFIG_ENTRY)? };
    *pending = pending.or(status);
    let (query, limit) = if raw {
        Query::decode_partial(source, &config.separators, config.max_variables)
    } else {
        match Query::decode(source, &config.separators, config.max_variables) {
            Ok(query) => (query, None),
            Err(error) => {
                unsafe { diagnostic(host, pending, error.message().as_bytes(), b"mb_parse_str(): ")?; }
                REQUEST.with(|state| state.borrow_mut().set_http_input_identification(None));
                return Ok(QueryRun { success: false, limit: None });
            },
        }
    };
    let function: &[u8] = if raw { b"parse_str(): " } else { b"mb_parse_str(): " };
    // `parse_str()` is byte-preserving and PHP's implementation does not run mbstring input
    // detection at all: it must not warn and must not publish an identification the later
    // `mb_http_input()` call observes. `mb_parse_str()` keeps both stages.
    let (identified_encoding, identified_warning) = if raw {
        (None, false)
    } else {
        let identified = query.identify(&candidates, strict);
        (identified.encoding, identified.warning)
    };
    if identified_warning { unsafe { diagnostic(host, pending, b"Unable to detect encoding", function)?; } }
    if raw || identified_encoding.is_some() {
        let from = if raw { None } else { identified_encoding };
        for pair in query.into_pairs() {
            let pair = match from {
                Some(from) => REQUEST.with(|state| pair.convert(from, to, &mut state.borrow_mut())),
                None => pair,
            };
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
                    unsafe { diagnostic(host, pending, message.as_bytes(), function)?; }
                }
            }
        }
    }
    if !raw {
        REQUEST.with(|state| state.borrow_mut().set_http_input_identification(identified_encoding));
    }
    // Ordinary `parse_str()` returns the exceeded limit instead of warning here: its prelude
    // publishes the parsed array to the caller first, so an error handler sees the accepted
    // fields even when it throws.
    Ok(QueryRun { success: raw || identified_encoding.is_some(), limit })
}

/// Attaches the PHP function name and retains a thrown warning while allowing later query writes.
unsafe fn diagnostic(host: &Session, pending: &mut Option<Status>, message: &[u8], function: &[u8]) -> Result<(), Status> {
    if pending.is_some() { return Ok(()); }
    let mut bytes = function.to_vec();
    bytes.extend_from_slice(message);
    match unsafe { host.diagnostic(2, &bytes) } {
        Ok(()) => Ok(()),
        Err(Status::Pending) => { *pending = Some(Status::Pending); Ok(()) },
        Err(status) => Err(status),
    }
}

/// Publishes the parser result after a completed query without a retained PHP throwable.
fn finish(run: QueryRun, pending: Option<Status>, raw: bool) -> Result<Outcome, Status> {
    if let Some(status) = pending { return Err(status); }
    Ok(if raw {
        // Zero means "no limit exceeded"; encode an exceeded limit with a positive signal so
        // `max_input_vars=0` still reports its "exceeded 0" warning.
        Outcome::integer(run.limit.map_or(0, |limit| limit.maximum.saturating_add(1)))
    } else {
        Outcome::boolean(run.success)
    })
}
