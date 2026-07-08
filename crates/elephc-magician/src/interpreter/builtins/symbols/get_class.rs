//! Purpose:
//! Declarative eval registry entry for `get_class`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the class-name introspection helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "get_class",
    area: Symbols,
    params: [object = EvalBuiltinDefaultValue::Null],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `get_class` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_get_class_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::super::eval_builtin_get_class(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `get_class` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_get_class_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args { [] => super::super::eval_get_class_no_arg_result(context, values), [object] => super::super::eval_get_class_result(*object, context, values), _ => Err(EvalStatus::RuntimeFatal), }
}
