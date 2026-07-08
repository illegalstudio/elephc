//! Purpose:
//! Eval registry entry and implementation for `is_iterable`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Arrays are iterable directly; objects are checked against Traversable-style
//!   relationships in the current eval context.

use super::super::super::*;

eval_builtin! {
    name: "is_iterable",
    area: Types,
    params: [value],
    direct: IsIterable,
    values: IsIterable,
}

/// Evaluates PHP `is_iterable()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_is_iterable(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_is_iterable_result(value, context, values)
}

/// Applies PHP `is_iterable()` to one already evaluated value.
pub(in crate::interpreter) fn eval_is_iterable_result(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    let result = eval_is_iterable_value(tag, value, context, values)?;
    values.bool_value(result)
}

/// Returns PHP's `is_iterable()` result for arrays and Traversable-compatible objects.
fn eval_is_iterable_value(
    tag: u64,
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Ok(true);
    }
    if tag != EVAL_TAG_OBJECT {
        return Ok(false);
    }
    for target in ["Traversable", "Iterator", "IteratorAggregate"] {
        if dynamic_object_is_a(value, target, false, context, values)?
            .map_or_else(|| values.object_is_a(value, target, false), Ok)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}
