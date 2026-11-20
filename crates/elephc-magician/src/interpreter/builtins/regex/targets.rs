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
//! - The write can release the old `$matches` and run a destructor whose nested
//!   `preg_*` call resets the error state. php-src empties `$matches` before matching,
//!   so the outer call's own outcome is what `preg_last_error()` reports afterwards;
//!   the writer restores it, like the AOT `emit_push_last_error`/`emit_pop_last_error`.

use super::super::super::*;
use super::super::*;
use super::*;

/// Writes a preg `$matches` result back to the captured caller lvalue.
pub(in crate::interpreter) fn eval_write_preg_matches_target(
    target: &EvalReferenceTarget,
    matches_array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let outcome = eval_preg_last_error_code();
    eval_write_direct_ref_target(
        target,
        matches_array,
        context,
        values,
        Some(ScopeCellOwnership::Owned),
    )?;
    eval_preg_set_last_error(outcome);
    Ok(())
}
