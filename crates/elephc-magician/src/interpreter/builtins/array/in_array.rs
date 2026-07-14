//! Purpose:
//! Declarative eval registry entry for `in_array`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-search hook.

use super::super::spec::EvalBuiltinDefaultValue;

use super::super::super::*;

eval_builtin! {
    name: "in_array",
    area: Array,
    params: [needle, haystack, strict = EvalBuiltinDefaultValue::Bool(false)],
    direct: ArraySearch,
    values: ArraySearch,
}
/// Dispatches direct eval calls for the `in_array` array builtin.
pub(in crate::interpreter) fn eval_in_array_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::array_search::eval_builtin_array_search("in_array", args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `in_array` array builtin.
pub(in crate::interpreter) fn eval_in_array_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [needle, array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    super::array_search::eval_array_search_result("in_array", *needle, *array, values)
}
