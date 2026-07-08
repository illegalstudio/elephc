//! Purpose:
//! Declarative eval registry entry for `enum_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the class-like existence probe.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "enum_exists",
    area: Symbols,
    params: [r#enum, autoload = EvalBuiltinDefaultValue::Bool(true)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `enum_exists` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_enum_exists_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_builtin_symbols_call_impl("enum_exists", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `enum_exists` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_enum_exists_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_symbols_values_result_impl("enum_exists", evaluated_args, context, values)
}
