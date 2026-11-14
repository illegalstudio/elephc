//! Purpose:
//! Eval registry entry and implementation for `strval`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Cast behavior is implemented here; shared scalar coercions still flow
//!   through `RuntimeValueOps`.

use super::super::super::*;

eval_builtin! {
    contract: "strval",
    area: Types,
    direct: Strval,
    values: Strval,
}

/// Evaluates PHP `strval()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_strval(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    with_eval_operands(&[value], context, scope, values, |args, context, _, values| {
        eval_strval_result(args[0], context, values)
    })
}

/// Applies PHP `strval()` to one already evaluated value.
pub(in crate::interpreter) fn eval_strval_result(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_cast_value(&EvalCastType::String, value, context, values)
}
