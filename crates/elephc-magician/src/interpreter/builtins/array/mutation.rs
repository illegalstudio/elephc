//! Purpose:
//! Shared lvalue binding for source-sensitive array mutator builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::array` mutating builtin owners.
//!
//! Key details:
//! - The helper keeps the by-reference storage target together with the current
//!   array cell so callers can write back replacements after PHP-visible work.

use super::super::super::*;

/// Captures the first by-reference array mutator argument as a writable lvalue.
pub(in crate::interpreter) fn eval_array_mutation_lvalue_arg(
    arg: &EvalCallArg,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, EvalReferenceTarget), EvalStatus> {
    if arg.is_spread() || !matches!(arg.name(), None | Some("array")) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (array, target) = eval_call_arg_value(arg.value(), context, scope, values)?;
    let target = target.ok_or(EvalStatus::RuntimeFatal)?;
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok((array, target))
}
