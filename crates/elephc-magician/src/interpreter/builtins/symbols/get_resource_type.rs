//! Purpose:
//! Eval registry entry for `get_resource_type`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Resource introspection implementation is shared with `get_resource_id()`.

eval_builtin! {
    name: "get_resource_type",
    area: Symbols,
    params: [resource],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `get_resource_type(...)` calls through the `get_resource_id` owner.
pub(in crate::interpreter) fn eval_get_resource_type_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::get_resource_id::eval_builtin_resource_introspection(
        "get_resource_type",
        args,
        context,
        scope,
        values,
    )
}

/// Evaluates materialized `get_resource_type(...)` arguments through the `get_resource_id` owner.
pub(in crate::interpreter) fn eval_get_resource_type_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [resource] => super::get_resource_id::eval_resource_introspection_result(
            "get_resource_type",
            *resource,
            values,
        ),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
