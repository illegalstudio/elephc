//! Purpose:
//! Declarative eval registry entry for `interface_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the interface-existence probe.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "interface_exists",
    area: Symbols,
    params: [interface, autoload = EvalBuiltinDefaultValue::Bool(true)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `interface_exists` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_interface_exists_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::class_names::eval_builtin_interface_exists(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `interface_exists` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_interface_exists_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::class_names::eval_interface_exists_result(evaluated_args, context, values)
}
