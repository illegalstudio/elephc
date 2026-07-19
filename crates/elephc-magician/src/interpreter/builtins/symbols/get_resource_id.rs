//! Purpose:
//! Eval registry entry and implementation for `get_resource_id`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Resource id/type support is shared with `get_resource_type()`.

eval_builtin! {
    name: "get_resource_id",
    area: Symbols,
    params: [resource],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `get_resource_id(...)` calls.
pub(in crate::interpreter) fn eval_get_resource_id_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_resource_introspection("get_resource_id", args, context, scope, values)
}

/// Evaluates materialized `get_resource_id(...)` arguments.
pub(in crate::interpreter) fn eval_get_resource_id_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [resource] => eval_resource_introspection_result("get_resource_id", *resource, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates `get_resource_type(...)` and `get_resource_id(...)` over one eval value.
pub(in crate::interpreter) fn eval_builtin_resource_introspection(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [resource] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let resource = eval_expr(resource, context, scope, values)?;
    eval_resource_introspection_result(name, resource, values)
}

/// Evaluates a materialized resource introspection builtin argument.
pub(in crate::interpreter) fn eval_resource_introspection_result(
    name: &str,
    resource: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(resource)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    match name {
        "get_resource_type" => values.string("stream"),
        "get_resource_id" => values.cast_int(resource),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}
