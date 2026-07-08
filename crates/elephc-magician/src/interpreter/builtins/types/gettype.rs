//! Purpose:
//! Eval registry entry and implementation for `gettype`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Runtime tags are mapped to PHP's historical `gettype()` names.

use super::super::super::*;

eval_builtin! {
    name: "gettype",
    area: Types,
    params: [value],
    direct: Gettype,
    values: Gettype,
}

/// Evaluates PHP `gettype()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_gettype(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_gettype_result(value, values)
}

/// Converts one boxed runtime tag into PHP's `gettype()` spelling.
pub(in crate::interpreter) fn eval_gettype_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    values.string(eval_gettype_name(tag))
}
