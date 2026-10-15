//! Purpose:
//! Chooses the machine emission order of one function's EIR blocks and finds integer
//! compares that can be fused into their block's conditional branch.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_blocks()`.
//!
//! Key details:
//! - The first table block stays first: the prologue falls through into it.
//! - Blocks that can only end the activation abnormally (`unreachable`, `fatal`, `throw`)
//!   are cold. They move after every other block, in their original relative order, so a
//!   loop body's hot path is laid out contiguously and branches over no cold code.
//! - Every terminator lowers to explicit jumps except a jump to the block laid out next,
//!   which is what makes any order semantically safe.

use std::collections::HashMap;

use crate::ir::{BasicBlock, Function, InstId, Op, Terminator, ValueId};
use crate::ir_passes::{successors, terminator_uses};

/// Returns block-table indices in emission order: hot blocks first, cold blocks last.
pub(super) fn emission_order(function: &Function) -> Vec<usize> {
    let mut hot = Vec::with_capacity(function.blocks.len());
    let mut cold = Vec::new();
    for (index, block) in function.blocks.iter().enumerate() {
        if index > 0 && is_cold(block) {
            cold.push(index);
        } else {
            hot.push(index);
        }
    }
    hot.extend(cold);
    hot
}

/// Returns true for a block whose terminator never continues normal control flow.
fn is_cold(block: &BasicBlock) -> bool {
    matches!(
        block.terminator,
        Some(Terminator::Unreachable | Terminator::Fatal { .. } | Terminator::Throw { .. })
    )
}

/// Returns true when some edge targets the same or an earlier block of the table, which every
/// loop needs (a retreating edge). Straight-line generated code returns false.
pub(super) fn has_retreating_edge(function: &Function) -> bool {
    function.blocks.iter().enumerate().any(|(index, block)| {
        block.terminator.as_ref().is_some_and(|term| {
            successors(term)
                .iter()
                .any(|target| (target.as_raw() as usize) <= index)
        })
    })
}

/// Counts every use of each value across instruction operands and terminators, once per function.
pub(super) fn value_use_counts(function: &Function) -> HashMap<ValueId, usize> {
    let mut counts: HashMap<ValueId, usize> = HashMap::new();
    for operand in function.instructions.iter().flat_map(|inst| inst.operands.iter()) {
        *counts.entry(*operand).or_insert(0) += 1;
    }
    for term in function.blocks.iter().filter_map(|block| block.terminator.as_ref()) {
        for value in terminator_uses(term) {
            *counts.entry(value).or_insert(0) += 1;
        }
    }
    counts
}

/// Returns the integer compare that `block`'s `cond_br` can consume as flags, if any.
///
/// The compare must be the block's last emitting instruction (only `nop`s may follow), and its
/// boolean must have no use besides the branch condition (`use_counts` from
/// [`value_use_counts`]), so skipping its materialization is unobservable and no instruction
/// runs between the flag-setting compare and the branch.
pub(super) fn fusible_compare(
    function: &Function,
    block: &BasicBlock,
    use_counts: &HashMap<ValueId, usize>,
) -> Option<InstId> {
    let Some(Terminator::CondBr { cond, .. }) = &block.terminator else {
        return None;
    };
    let compare = block
        .instructions
        .iter()
        .rev()
        .copied()
        .find(|id| function.instruction(*id).is_some_and(|inst| inst.op != Op::Nop))?;
    let inst = function.instruction(compare)?;
    if inst.op != Op::ICmp || inst.result != Some(*cond) {
        return None;
    }
    (use_counts.get(cond) == Some(&1)).then_some(compare)
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for machine block ordering and compare-branch fusion eligibility.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Functions are built directly with the EIR builder so CFG shapes are exact.

    use super::{emission_order, fusible_compare, value_use_counts};
    use crate::ir::{
        Builder, CmpPredicate, Function, Immediate, IrType, Op, Ownership, Terminator,
    };
    use crate::types::PhpType;

    /// Builds `entry -> cold(unreachable) | hot -> exit` with an icmp feeding the branch.
    fn diamond(extra_use: bool) -> Function {
        let mut function = Function::new("f".to_string(), IrType::I64, PhpType::Int);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        let cold = builder.create_named_block("cold", Vec::new());
        let hot = builder.create_named_block("hot", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let a = builder.emit_const_i64(1);
        let b = builder.emit_const_i64(2);
        let cond = builder
            .emit(
                Op::ICmp,
                vec![a, b],
                Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
                IrType::I64,
                PhpType::Bool,
                Ownership::NonHeap,
            )
            .expect("icmp result");
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: hot,
            then_args: Vec::new(),
            else_target: cold,
            else_args: Vec::new(),
        });
        builder.position_at_end(cold);
        builder.terminate(Terminator::Unreachable);
        builder.position_at_end(hot);
        let returned = if extra_use { cond } else { a };
        builder.terminate(Terminator::Return { value: Some(returned) });
        function
    }

    /// Verifies a loop-free diamond has no retreating edge.
    #[test]
    fn diamond_has_no_retreating_edge() {
        assert!(!super::has_retreating_edge(&diamond(false)));
    }

    /// Verifies cold blocks move after hot ones while the first block stays first.
    #[test]
    fn cold_blocks_are_laid_out_last() {
        let function = diamond(false);
        assert_eq!(emission_order(&function), vec![0, 2, 1]);
    }

    /// Verifies a single-use icmp feeding the branch is fusible, and a reused one is not.
    #[test]
    fn only_single_use_branch_compares_fuse() {
        let function = diamond(false);
        let counts = value_use_counts(&function);
        assert!(fusible_compare(&function, &function.blocks[0], &counts).is_some());
        let function = diamond(true);
        let counts = value_use_counts(&function);
        assert!(fusible_compare(&function, &function.blocks[0], &counts).is_none());
    }
}
