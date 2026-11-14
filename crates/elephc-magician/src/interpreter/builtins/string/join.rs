//! Purpose:
//! Declarative eval registry entry for `join`, PHP's alias of `implode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Shares `implode()`'s join and diagnostics, reported under the `join()` name.
//! - Unlike `implode($x)`, a direct one-argument `join($x)` is not specialized by Zend, so it
//!   binds through the ordinary separator-or-array overload like every other call form.

eval_builtin! {
    contract: "join",
    area: String,
    direct: StringSplitJoin,
    values: StringSplitJoin,
}

use super::super::super::*;

/// Evaluates PHP `join()` over separator and array expressions.
pub(in crate::interpreter) fn eval_builtin_join(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let operands = match args {
        [array] => vec![array],
        [separator, array] => vec![separator, array],
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    with_eval_operands(&operands, context, scope, values, |args, context, _, values| {
        eval_implode_named_result("join", args, context, values)
    })
}

/// Dispatches evaluated `join()` calls through the shared implode join.
pub(in crate::interpreter) fn eval_join_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_implode_named_result("join", evaluated_args, context, values)
}
