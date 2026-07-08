//! Purpose:
//! Declarative eval registry entry for `spl_autoload_register`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the SPL autoload registration stub.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "spl_autoload_register",
    area: Symbols,
    params: [
        callback = EvalBuiltinDefaultValue::Null,
        throw = EvalBuiltinDefaultValue::Bool(true),
        prepend = EvalBuiltinDefaultValue::Bool(false),
    ],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `spl_autoload_register` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_autoload_register_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_builtin_symbols_call_impl("spl_autoload_register", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `spl_autoload_register` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_autoload_register_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_symbols_values_result_impl("spl_autoload_register", evaluated_args, context, values)
}
