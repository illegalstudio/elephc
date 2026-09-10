//! Purpose:
//! Delivers mbregex diagnostics through the protected shared invocation host.
//!
//! Called from:
//! - The mbstring invocation coordinator after argument coercion completes.
//!
//! Key details:
//! - No text or regex state borrow survives warning callbacks, which can reenter AOT or eval.
//! - A pending callback throwable outranks later native transport failures and successful false results.

use super::*;
use crate::regex::{Event, Limits};

/// Executes a regex operation with shared state and protected warnings at their observable boundaries.
pub(super) unsafe fn invoke(operation: RuntimeBuiltinId, args: &[MbArgV1], host: &Session,
    entry_encoding: crate::regex::RegexEncoding) -> Result<Outcome, Status> {
    let arguments = unsafe { Arguments::new(operation, args) }.ok_or(Status::Fatal)?;
    let (stack, retry) = REQUEST.with(|state| state.borrow().ini_regex_limits());
    let mut status = None;
    let mut exceptions = Vec::new();
    let result = super::super::regex::run(operation, &arguments, entry_encoding,
        Limits::from_ini(stack, retry, operation == RuntimeBuiltinId::MbEregMatch), &mut |event| match event {
            Event::Warning(bytes) if status.is_none() && exceptions.is_empty() => {
                if let Err(error) = unsafe { host.diagnostic(2, &bytes) } { status = Some(error); }
            },
            Event::Warning(_) => {},
            Event::Exception(error) => exceptions.push(error),
        });
    if let Some(status) = status { return Err(status); }
    if let Some(error) = Outcome::error_chain(exceptions) { return Ok(error); }
    result.map_err(|_| Status::Fatal)
}
