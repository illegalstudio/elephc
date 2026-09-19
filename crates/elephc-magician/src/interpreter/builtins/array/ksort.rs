//! Purpose:
//! Declarative eval registry entry for `ksort`.
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
    contract: "ksort",
    area: Array,
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `ksort` array mutator.
pub(in crate::interpreter) fn eval_ksort_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let ([array] | [array, _]) = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    super::array_pop::eval_warn_array_by_value("ksort", values)?;
    super::sort::eval_array_sort_value_result(*array, values)
}
