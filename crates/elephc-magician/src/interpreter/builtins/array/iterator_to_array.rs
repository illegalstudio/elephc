//! Purpose:
//! Declarative eval registry entry for `iterator_to_array`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::spec::EvalBuiltinDefaultValue;

use super::super::super::*;

eval_builtin! {
    name: "iterator_to_array",
    area: Array,
    params: [iterator, preserve_keys = EvalBuiltinDefaultValue::Bool(true)],
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `iterator_to_array` array builtin.
pub(in crate::interpreter) fn eval_iterator_to_array_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_iterator_to_array(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `iterator_to_array` array builtin.
pub(in crate::interpreter) fn eval_iterator_to_array_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [iterator] => eval_iterator_to_array_result(*iterator, true, values),
        [iterator, preserve_keys] => {
            let preserve_keys = values.truthy(*preserve_keys)?;
            eval_iterator_to_array_result(*iterator, preserve_keys, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
