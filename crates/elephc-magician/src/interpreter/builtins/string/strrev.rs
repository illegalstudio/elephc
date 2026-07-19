//! Purpose:
//! Declarative eval registry entry for `strrev`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing string-reversal hook.

eval_builtin! {
    name: "strrev",
    area: String,
    params: [string],
    direct: Strrev,
    values: Strrev,
}

use super::super::super::*;

/// Evaluates PHP's `strrev(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_strrev(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.strrev(value)
}

/// Reverses one converted eval string using the runtime string helper.
pub(in crate::interpreter) fn eval_strrev_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.strrev(value)
}
