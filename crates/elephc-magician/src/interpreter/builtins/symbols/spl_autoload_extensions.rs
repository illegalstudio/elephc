//! Purpose:
//! Declarative eval registry entry for `spl_autoload_extensions`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to eval-local autoload extension state.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "spl_autoload_extensions",
    area: Symbols,
    params: [file_extensions = EvalBuiltinDefaultValue::Null],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `spl_autoload_extensions` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_autoload_extensions_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_builtin_symbols_call_impl("spl_autoload_extensions", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `spl_autoload_extensions` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_autoload_extensions_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_symbols_values_result_impl("spl_autoload_extensions", evaluated_args, context, values)
}
