//! Purpose:
//! Shared numeric helper algorithms used by per-builtin math home files.
//!
//! Called from:
//! - `crate::interpreter::builtins::math::{min,max}`.
//!
//! Key details:
//! - Runtime cells remain opaque and ordering stays delegated to
//!   `RuntimeValueOps::compare`.

use super::super::super::*;

/// Selects the smallest or largest evaluated cell using runtime comparison hooks.
pub(in crate::interpreter) fn eval_min_max_selected(
    evaluated_args: &[RuntimeCellHandle],
    op: EvalBinOp,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((&first, rest)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let mut selected = first;
    for candidate in rest {
        if eval_comparison_condition(op, *candidate, selected, values)? {
            selected = *candidate;
        }
    }
    Ok(selected)
}
