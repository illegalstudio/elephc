//! Purpose:
//! Eval registry entry and implementation for `spl_object_id`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - `spl_object_hash()` shares the same object-identity implementation.

eval_builtin! {
    name: "spl_object_id",
    area: Symbols,
    params: [object],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `spl_object_id(...)` calls.
pub(in crate::interpreter) fn eval_spl_object_id_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_spl_object_identity("spl_object_id", args, context, scope, values)
}

/// Evaluates materialized `spl_object_id(...)` arguments.
pub(in crate::interpreter) fn eval_spl_object_id_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [object] => eval_spl_object_identity_result("spl_object_id", *object, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP's SPL object identity builtins over one eval object expression.
pub(in crate::interpreter) fn eval_builtin_spl_object_identity(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let object = eval_expr(object, context, scope, values)?;
    eval_spl_object_identity_result(name, object, values)
}

/// Returns the unboxed object-payload identity in the native SPL builtin spelling.
pub(in crate::interpreter) fn eval_spl_object_identity_result(
    name: &str,
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let identity = values.object_identity(object)? as i64;
    match name {
        "spl_object_id" => values.int(identity),
        "spl_object_hash" => values.string(&identity.to_string()),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}
