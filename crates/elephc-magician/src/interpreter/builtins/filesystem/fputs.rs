//! Purpose:
//! Declarative eval registry entry for `fputs`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - `fputs` is `fwrite`'s alias, so both dispatch to the same stream write helper. It needs its
//!   own entry all the same: the parity gate requires every static builtin to be visible to
//!   `eval()` by name.

eval_builtin! {
    contract: "fputs",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `fputs` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fputs_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::fwrite::eval_builtin_fwrite(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fputs` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fputs_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream, data] => super::fwrite::eval_fwrite_result(*stream, *data, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
