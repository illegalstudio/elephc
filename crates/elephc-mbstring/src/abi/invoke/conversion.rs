//! Purpose:
//! Orders protected source-list callbacks before shared string and array conversion.
//!
//! Called from:
//! - The mbstring invocation coordinator after outer PHP parameter coercions.
//!
//! Key details:
//! - Destination selection is fixed before candidate Stringable callbacks run.
//! - Input arrays are snapshotted after source preparation, preserving callback side effects.
//! - PHP callbacks and reader calls never hold a Rust request-state borrow.

use super::{Argument, Arguments, Outcome, RuntimeBuiltinId, REQUEST, encoding_list, host::{Session, Status}};
use super::super::conversion;

/// Prepares source encodings in PHP order and converts retained input with current request policy.
pub(super) unsafe fn invoke(mut values: Vec<Argument>, session: &mut Session) -> Result<Outcome, Status> {
    let operation = RuntimeBuiltinId::MbConvertEncoding;
    let Argument::String(to) = &values[1] else { return Err(Status::Fatal); };
    let resolved = match REQUEST.with(|state| state.borrow_mut().resolve_encoding(Some(to), "mb_convert_encoding", 2, "to_encoding")) {
        Ok(resolved) => resolved, Err(error) => return Ok(Outcome::error(error)),
    };
    if let Some(message) = resolved.deprecation {
        unsafe { session.diagnostic(8192, format!("mb_convert_encoding(): {message}").as_bytes())?; }
    }
    if matches!(values.get(2), Some(Argument::Array(_, _))) {
        let graph = match unsafe { encoding_list::prepare(session, 2, operation)? } {
            Ok(graph) => graph, Err(error) => return Ok(Outcome::error(error)),
        };
        values[2] = Argument::Snapshot(graph, false);
    }
    // Resolve sources before traversing input, using a scalar placeholder for an array input.
    let mut source_args = values.iter().map(Argument::wire).collect::<Vec<_>>();
    source_args[0] = Some(elephc_builtin_contract::mbstring_abi::MbArgV1::string(b""));
    let source_args: Vec<_> = source_args.into_iter().collect::<Option<_>>().ok_or(Status::Fatal)?;
    let args = unsafe { Arguments::new(operation, &source_args) }.ok_or(Status::Fatal)?;
    let sources = match REQUEST.with(|state| conversion::sources(&args, &state.borrow())) {
        Ok(sources) => sources, Err(error) => return Ok(Outcome::error(error)),
    };
    if let Argument::Array(root, _) = &values[0] {
        let graph = unsafe { session.snapshot_argument(0, *root)? };
        values[0] = Argument::Snapshot(graph.encode(), false);
    }
    let wire: Vec<_> = values.iter().map(Argument::wire).collect::<Option<_>>().ok_or(Status::Fatal)?;
    let args = unsafe { Arguments::new(operation, &wire) }.ok_or(Status::Fatal)?;
    Ok(REQUEST.with(|state| conversion::convert(&args, &mut state.borrow_mut(), resolved.encoding, &sources)))
}
