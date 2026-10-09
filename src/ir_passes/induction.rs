//! Purpose:
//! Recognizes basic scalar induction variables on canonical natural-loop CFGs.
//!
//! Called from:
//! - Integer range analysis and canonical loop optimization.
//!
//! Key details:
//! - Every latch must supply the same nonzero constant-step recurrence.
//! - Recognition alone does not prove that a checked update cannot overflow.

use crate::ir::{BlockId, Function, IrType, Op, Terminator, ValueId};
use crate::types::PhpType;
use super::cfg::successor_edges;
use super::loops::NaturalLoop;
use super::rewrite::defining_instruction;

/// One header parameter initialized outside the loop and updated on every back edge.
pub(super) struct BasicInduction {
    /// Scalar header parameter carrying the current iteration's value.
    pub parameter: ValueId,
    /// Value supplied by the unique preheader.
    pub initial: ValueId,
    /// Signed nonzero increment common to every latch.
    pub step: i64,
    /// Values supplied on back edges, in the loop's latch order.
    pub updates: Vec<ValueId>,
}

/// Finds constant-step recurrences using the caller's immutable integer evaluator.
pub(super) fn basic_inductions(
    function: &Function,
    lp: &NaturalLoop,
    mut constant: impl FnMut(ValueId) -> Option<i64>,
) -> Vec<BasicInduction> {
    let Some(initial) = lp.preheader.and_then(|pre| branch_args_to(function, pre, lp.header)) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for (index, &parameter) in function.blocks[lp.header.as_raw() as usize].params.iter().enumerate() {
        if !function.value(parameter).is_some_and(|value| {
            value.ir_type == IrType::I64 && value.php_type == PhpType::Int
        }) {
            continue;
        }
        let mut updates = Vec::new();
        let mut common = None;
        let recognized = lp.latches.iter().all(|&latch| {
            let Some(args) = branch_args_to(function, latch, lp.header) else { return false; };
            let Some(&update) = args.get(index) else { return false; };
            let Some(inst) = defining_instruction(function, update) else { return false; };
            let [left, right] = inst.operands.as_slice() else { return false; };
            let step = match inst.op {
                Op::IAdd | Op::ICheckedAddToInt if *left == parameter => constant(*right),
                Op::IAdd | Op::ICheckedAddToInt if *right == parameter => constant(*left),
                Op::ISub | Op::ICheckedSubToInt if *left == parameter => {
                    constant(*right).and_then(i64::checked_neg)
                }
                _ => None,
            };
            let Some(step) = step.filter(|step| *step != 0) else { return false; };
            if common.is_some_and(|previous| previous != step) { return false; }
            common = Some(step);
            updates.push(update);
            true
        });
        if recognized {
            if let Some((&initial, step)) = initial.get(index).zip(common) {
                result.push(BasicInduction { parameter, initial, step, updates });
            }
        }
    }
    result
}

/// Returns arguments only when all parallel edges to a target agree.
pub(super) fn branch_args_to(function: &Function, from: BlockId, target: BlockId) -> Option<Vec<ValueId>> {
    let mut term: Terminator = function.block(from)?.terminator.clone()?;
    let mut edges = successor_edges(&mut term).into_iter().filter(|(to, _)| **to == target);
    let args = edges.next()?.1.clone();
    edges.all(|(_, other)| *other == args).then_some(args)
}
