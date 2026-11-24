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
use crate::runtime_hooks::owner_frames::StagedRuntimeOwners;

/// Captures the first by-reference array mutator argument as a writable lvalue.
pub(in crate::interpreter) fn eval_array_mutation_lvalue_arg(
    arg: &EvalCallArg,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
    owners: &StagedRuntimeOwners,
) -> Result<(RuntimeCellHandle, EvalReferenceTarget), EvalStatus> {
    if arg.is_spread() || !matches!(arg.name(), None | Some("array")) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (array, mut target) = eval_call_arg_value(arg.value(), context, scope, values)?;
    owners.stage(array);
    let mut reference_owners = Vec::new();
    crate::interpreter::dynamic_functions::capture_abandoned_reference_owners(
        target.as_mut(), &mut reference_owners,
    );
    for owner in reference_owners {
        owners.stage(owner);
    }
    let target = target.ok_or(EvalStatus::RuntimeFatal)?;
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if array.is_borrowed() {
        owners.stage(values.retain(array)?);
    }
    Ok((array.borrowed(), target))
}
