//! Purpose:
//! Declarative eval registry entry and implementation for `pfsockopen`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//! - `crate::interpreter::expressions::eval_call()` through the fsockopen path.
//!
//! Key details:
//! - Eval has no persistent socket table, so runtime behavior delegates to `fsockopen`.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "pfsockopen",
    area: Filesystem,
    params: [
        hostname,
        port,
        error_code: by_ref = EvalBuiltinDefaultValue::Null,
        error_message: by_ref = EvalBuiltinDefaultValue::Null,
        timeout = EvalBuiltinDefaultValue::Null
    ],
    by_ref: [error_code, error_message],
    direct: none,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates a positional `pfsockopen()` call without writable error outputs.
pub(in crate::interpreter) fn eval_pfsockopen_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=5).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let host = eval_expr(&args[0], context, scope, values)?;
    let port = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        eval_expr(arg, context, scope, values)?;
    }
    super::fsockopen::eval_fsockopen_by_value_ref_warnings("pfsockopen", args.len(), values)?;
    super::fsockopen::eval_fsockopen_result(host, port, context, values)
}

/// Evaluates a by-value `pfsockopen()` call from already evaluated arguments.
pub(in crate::interpreter) fn eval_pfsockopen_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=5).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    super::fsockopen::eval_fsockopen_by_value_ref_warnings(
        "pfsockopen",
        evaluated_args.len(),
        values,
    )?;
    super::fsockopen::eval_fsockopen_result(evaluated_args[0], evaluated_args[1], context, values)
}
