//! Purpose:
//! Eval registry entry and implementation for `headers_sent`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct dispatch and the runtime builtin contract.
//!
//! Key details:
//! - The flag itself is process state the compiled side owns, so it is always read through the
//!   versioned `RuntimeBuiltinId::HeadersSent` runtime builtin: answering it locally would report
//!   "not sent" for output the compiled program had already flushed.
//! - The two by-reference out-parameters are bound here, the way `str_replace()` binds its
//!   `$count`: the boxed-cell ABI of the runtime builtin carries no out-parameters. They receive
//!   what the compiled `headers_sent($file, $line)` writes, `""` and `0`, since neither side
//!   records where output started.

use super::super::super::*;
use super::super::*;
use elephc_builtin_contract::RuntimeBuiltinId;

eval_builtin! {
    contract: "headers_sent",
    area: Time,
    direct: Time,
    values: Time,
}

/// Evaluates PHP `headers_sent(&$filename = null, &$line = null)`.
///
/// Symfony's `NativeSessionStorage::start()` asks `headers_sent($file, $line)` before every
/// session start, and the by-reference shape alone was refused as an unsupported builtin, which
/// killed the request.
pub(in crate::interpreter) fn eval_builtin_headers_sent(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut targets = Vec::with_capacity(args.len());
    for arg in args {
        targets.push(eval_preg_matches_target(arg, context, scope, values)?);
    }
    let sent = eval_headers_sent_flag(values)?;
    if let Some(target) = targets.first() {
        let file = values.string("")?;
        eval_write_preg_matches_target(target, file, context, values)?;
    }
    if let Some(target) = targets.get(1) {
        let line = values.int(0)?;
        eval_write_preg_matches_target(target, line, context, values)?;
    }
    Ok(sent)
}

/// Reads the compiled program's headers-sent flag through the shared runtime builtin.
///
/// Also the by-value route (`call_user_func('headers_sent', ...)`), which has no caller
/// variables to bind and so answers the flag alone.
pub(in crate::interpreter) fn eval_headers_sent_flag(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values
        .runtime_builtin_call(RuntimeBuiltinId::HeadersSent, &[])?
        .ok_or(EvalStatus::RuntimeFatal)
}
