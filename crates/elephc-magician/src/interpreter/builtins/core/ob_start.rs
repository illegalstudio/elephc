//! Purpose:
//! Eval registry entry and implementation for `ob_start`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Shares the runtime output-buffer stack with statically compiled code via the
//! -   `RuntimeValueOps` ob hooks, so eval'd and static output interleave correctly.
//! - User output handlers are unsupported: a non-null `$callback` raises a warning
//! -   and returns false without starting a buffer; `chunk_size`/`flags` are inert.

use super::super::super::*;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "ob_start",
    area: Core,
    params: [
        callback = EvalBuiltinDefaultValue::Null,
        chunk_size = EvalBuiltinDefaultValue::Int(0),
        flags = EvalBuiltinDefaultValue::Int(112)
    ],
    direct: Core,
    values: Core,
}

/// Evaluates PHP `ob_start($callback = null, $chunk_size = 0, $flags = 112)`.
pub(in crate::interpreter) fn eval_builtin_ob_start(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 3 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_ob_start_result(&evaluated, context, values)
}

/// Starts a runtime output buffer, rejecting unsupported handler callbacks.
pub(in crate::interpreter) fn eval_ob_start_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.len() > 3 {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(callback) = evaluated_args.first() {
        if !values.is_null(*callback)? {
            values.warning("ob_start(): output handler callbacks are not supported; pass null")?;
            return values.bool_value(false);
        }
    }
    let started = values.ob_start()?;
    values.bool_value(started)
}
