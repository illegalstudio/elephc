//! Purpose:
//! Shared by-reference `$matches` target helpers for eval preg builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex::match_one`
//! - `crate::interpreter::builtins::regex::match_all`
//!
//! Key details:
//! - Direct preg calls capture writable caller lvalues in source order, then
//!   write the materialized matches array back after regex execution.

use super::super::super::*;
use super::super::*;

/// Captures a writable `$matches` argument target from a direct preg call.
pub(in crate::interpreter) fn eval_preg_matches_target(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalReferenceTarget, EvalStatus> {
    let (value, target) = eval_direct_ref_arg_value(expr, context, scope, values)?;
    let target = target.ok_or(EvalStatus::RuntimeFatal)?;
    eval_preg_define_matches_target(&target, value, context, values)?;
    Ok(target)
}

/// Defines an undefined `$matches` variable or element as null, as binding it by reference does.
///
/// php creates the slot when the call binds it, before the pattern is even compiled, so a
/// rejected pattern -- which leaves `$matches` alone -- still leaves it defined: `$m` reads as
/// null afterwards, never as `Undefined variable $m`. Writing null over a null that already
/// exists is the identity, so only a property target, whose write can run a hook, is skipped.
pub(in crate::interpreter) fn eval_preg_define_matches_target(
    target: &EvalReferenceTarget,
    current: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !matches!(
        target,
        EvalReferenceTarget::Variable { .. }
            | EvalReferenceTarget::ArrayElement { .. }
            | EvalReferenceTarget::NestedArrayElement { .. }
    ) || !values.is_null(current)?
    {
        return Ok(());
    }
    let null = values.null()?;
    eval_write_direct_ref_target(target, null, context, values, Some(ScopeCellOwnership::Owned))
}

/// Writes a preg `$matches` result back to the captured caller lvalue.
pub(in crate::interpreter) fn eval_write_preg_matches_target(
    target: &EvalReferenceTarget,
    matches_array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_write_direct_ref_target(
        target,
        matches_array,
        context,
        values,
        Some(ScopeCellOwnership::Owned),
    )
}
