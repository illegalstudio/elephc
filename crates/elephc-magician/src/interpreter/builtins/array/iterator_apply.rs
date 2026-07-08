//! Purpose:
//! Declarative eval registry entry for `iterator_apply`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::spec::EvalBuiltinDefaultValue;

use super::super::super::*;

eval_builtin! {
    name: "iterator_apply",
    area: Array,
    params: [iterator, callback, args = EvalBuiltinDefaultValue::Null],
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `iterator_apply` array builtin.
pub(in crate::interpreter) fn eval_iterator_apply_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_iterator_apply(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `iterator_apply` array builtin.
pub(in crate::interpreter) fn eval_iterator_apply_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [iterator, callback] => {
            let callback = eval_callable(*callback, context, values)?;
            eval_iterator_apply_result(*iterator, &callback, Vec::new(), context, values)
        }
        [iterator, callback, args] => {
            let callback = eval_callable(*callback, context, values)?;
            let callback_args = eval_iterator_apply_arg_values(*args, context, values)?;
            eval_iterator_apply_result(*iterator, &callback, callback_args, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
