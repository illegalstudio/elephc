//! Purpose:
//! Runtime helper implementations for numeric builtins whose metadata lives in
//! per-builtin declaration files.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::direct`.
//!
//! Key details:
//! - Helpers evaluate direct-call `EvalExpr` arguments before delegating to
//!   runtime numeric operations.

use super::super::super::*;

/// Evaluates PHP `abs(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_abs(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.abs(value)
}
