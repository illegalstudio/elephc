//! Purpose:
//! Shares exact scalar ordering between the eval walker and bytecode VM.
//!
//! Called from:
//! - The common already-evaluated binary expression boundary.
//!
//! Key details:
//! - Borrowed operands keep their original tags and never run conversion hooks.
//! - Bytewise strings bypass PHP's numeric-string comparison rules.

use super::*;
use elephc_semantics::{StrictFeature, comparisons::{self as policy, OrderingPlan}};

/// Checks original kinds and handles only ordering whose string semantics differ from PHP.
pub(super) fn binary(op: EvalBinOp, left: RuntimeCellHandle, right: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !context.semantic_profile().contains(StrictFeature::Comparisons) { return Ok(None); }
    if matches!(op, EvalBinOp::LooseEq | EvalBinOp::LooseNotEq) {
        return eval_throw_type_error(policy::EQUALITY_MESSAGE, context, values);
    }
    if !matches!(op, EvalBinOp::Lt | EvalBinOp::LtEq | EvalBinOp::Gt | EvalBinOp::GtEq | EvalBinOp::Spaceship) {
        return Ok(None);
    }
    let left_tag = values.type_tag(left)?;
    let right_tag = values.type_tag(right)?;
    if policy::ordering(numeric_domains::value_kind(left_tag), numeric_domains::value_kind(right_tag)) != OrderingPlan::Exact {
        return eval_throw_type_error(policy::ORDERING_MESSAGE, context, values);
    }
    if left_tag != EVAL_TAG_STRING { return Ok(None); }
    let ordering = values.string_bytes(left)?.cmp(&values.string_bytes(right)?);
    use std::cmp::Ordering;
    match op {
        EvalBinOp::Lt => values.bool_value(ordering == Ordering::Less),
        EvalBinOp::LtEq => values.bool_value(ordering != Ordering::Greater),
        EvalBinOp::Gt => values.bool_value(ordering == Ordering::Greater),
        EvalBinOp::GtEq => values.bool_value(ordering != Ordering::Less),
        EvalBinOp::Spaceship => values.int(match ordering { Ordering::Less => -1, Ordering::Equal => 0, Ordering::Greater => 1 }),
        _ => unreachable!("checked ordering operator"),
    }.map(Some)
}
