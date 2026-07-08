//! Purpose:
//! Declarative eval registry entry for `spl_autoload`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the SPL autoload stub.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "spl_autoload",
    area: Symbols,
    params: [class, file_extensions = EvalBuiltinDefaultValue::Null],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `spl_autoload` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_autoload_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::super::eval_builtin_spl_autoload_void("spl_autoload", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `spl_autoload` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_autoload_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::super::eval_spl_autoload_void_result("spl_autoload", evaluated_args, values)
}
