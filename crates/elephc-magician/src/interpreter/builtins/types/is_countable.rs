//! Purpose:
//! Eval registry entry and implementation for `is_countable`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Arrays are countable directly; objects are countable when they satisfy PHP's
//!   `Countable` interface in the current eval context, the same check `count()` uses.

use super::super::super::*;

eval_builtin! {
    contract: "is_countable",
    area: Types,
    direct: IsCountable,
    values: IsCountable,
}

/// Evaluates PHP `is_countable()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_is_countable(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_is_countable_result(value, context, values)
}

/// Applies PHP `is_countable()` to one already evaluated value.
pub(in crate::interpreter) fn eval_is_countable_result(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = match values.type_tag(value)? {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => true,
        EVAL_TAG_OBJECT => dynamic_object_is_a(value, "Countable", false, context, values)?
            .map_or_else(|| values.object_is_a(value, "Countable", false), Ok)?,
        _ => false,
    };
    values.bool_value(result)
}
