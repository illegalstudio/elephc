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
