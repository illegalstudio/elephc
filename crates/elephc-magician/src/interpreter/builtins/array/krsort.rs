//! Purpose:
//! Declarative eval registry entry for `krsort`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.
//! - A by-value callable call warns and changes nothing, so it accepts PHP's optional
//!   `$flags` argument and ignores it rather than failing on the arity.

use super::super::super::*;

eval_builtin! {
    contract: "krsort",
    area: Array,
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `krsort` array mutator.
pub(in crate::interpreter) fn eval_krsort_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let ([array] | [array, _]) = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    super::array_pop::eval_warn_array_by_value("krsort", values)?;
    super::sort::eval_array_sort_value_result(*array, values)
}
