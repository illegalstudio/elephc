//! Purpose:
//! Declarative eval registry entry for `hash_init`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Direct and evaluated-argument dispatch stay in this leaf.
//! - Runtime resources are stored in the eval context stream table.
//! - Optional HMAC parameters remain metadata-only for current eval behavior.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "hash_init",
    area: String,
    params: [
        algo,
        flags = EvalBuiltinDefaultValue::Int(0),
        key = EvalBuiltinDefaultValue::String(""),
    ],
    direct: HashContext,
    values: HashContext,
}

use super::super::super::*;

/// Evaluates PHP `hash_init($algo)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_hash_init(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [algo] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let algo = eval_expr(algo, context, scope, values)?;
    eval_hash_init_result(algo, context, values)
}

/// Opens an incremental hash context resource.
pub(in crate::interpreter) fn eval_hash_init_result(
    algo: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let algo = values.string_bytes(algo)?;
    match context.stream_resources_mut().open_hash_context(&algo) {
        Some(id) => values.resource(id),
        None => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches evaluated `hash_init()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_hash_init_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [algo] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_hash_init_result(*algo, context, values)
}

/// Converts a runtime resource cell into eval's zero-based hash context id.
pub(in crate::interpreter) fn eval_hash_context_resource_id(
    hash_context: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(hash_context)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(hash_context, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
}
