//! Purpose:
//! Declarative eval registry entry for `iterator_count`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    name: "iterator_count",
    area: Array,
    params: [iterator],
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `iterator_count` array builtin.
pub(in crate::interpreter) fn eval_iterator_count_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_iterator_count(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `iterator_count` array builtin.
pub(in crate::interpreter) fn eval_iterator_count_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [iterator] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_iterator_count_result(*iterator, values)
}

/// Evaluates PHP `iterator_count()` for eval-supported array iterator inputs.
pub(in crate::interpreter) fn eval_builtin_iterator_count(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [iterator] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let iterator = eval_expr(iterator, context, scope, values)?;
    eval_iterator_count_result(iterator, values)
}

/// Returns the element count for eval-supported array iterator inputs.
pub(in crate::interpreter) fn eval_iterator_count_result(
    iterator: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(iterator)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(iterator)?;
    values.int(i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?)
}
