//! Purpose:
//! Detects canonical counted/conditional loops whose header test can be duplicated
//! at the latch, so the emitted back edge branches directly to the body instead of
//! jumping through the header block.
//!
//! Called from:
//! - `crate::codegen::context::FunctionContext::new` once per function, before block
//!   emission.
//!
//! Key details:
//! - The transformation is machine-level only: the EIR CFG is unchanged. The latch's
//!   edge copies already write the next values into the header parameter homes, so the
//!   duplicated compare reads the current iteration's values.
//! - Only a header whose instructions are `nop`s with unused results plus one `icmp`,
//!   whose condition is used solely by the header `cond_br`, and whose `cond_br` has no
//!   branch arguments is eligible. Compare operands must be header parameters or values
//!   defined outside the header, so skipping the header cannot lose a header-local value.
//! - The header remains the entry test for the first iteration; every latch skips it.

use std::collections::HashMap;

use crate::ir::{
    BlockId, Function, InstId, Op, Terminator, ValueDef, ValueId,
};
use crate::ir_passes::{compute_dominance, compute_loops, has_back_edge};

/// One latch's duplicated loop test.
#[derive(Clone, Copy)]
pub(super) struct RotatedLoopBranch {
    /// The header's integer comparison to re-emit at the latch.
    pub(super) compare: InstId,
    /// The header `cond_br` then target (normally the loop body).
    pub(super) then_target: BlockId,
    /// The header `cond_br` else target (normally the loop exit).
    pub(super) else_target: BlockId,
}

/// Computes the rotated back-edge plans for every eligible latch.
pub(super) fn compute(function: &Function) -> HashMap<BlockId, RotatedLoopBranch> {
    let mut plans = HashMap::new();
    // Compiler-generated bodies are not user hot loops and dominate eval compile time.
    if function.flags.is_internal || function.flags.is_synthetic || !has_back_edge(function) {
        return plans;
    }
    let dominance = compute_dominance(function);
    let loops = compute_loops(function, &dominance);
    let uses = value_uses(function);
    for natural_loop in loops.loops() {
        let header = natural_loop.header;
        let Some(header_block) = function.block(header) else {
            continue;
        };
        let Some(Terminator::CondBr {
            cond,
            then_target,
            then_args,
            else_target,
            else_args,
        }) = header_block.terminator.as_ref()
        else {
            continue;
        };
        if !then_args.is_empty()
            || !else_args.is_empty()
            || *then_target == header
            || *else_target == header
        {
            continue;
        }
        let Some(condition) = function.value(*cond) else {
            continue;
        };
        let ValueDef::Instruction {
            block: compare_block,
            inst: compare_inst,
            ..
        } = condition.def
        else {
            continue;
        };
        if compare_block != header || uses.get(cond).copied().unwrap_or(0) != 1 {
            continue;
        }
        let Some(compare) = function.instruction(compare_inst) else {
            continue;
        };
        if compare.op != Op::ICmp {
            continue;
        }
        if !header_is_rotatable(function, header_block, compare_inst, &uses) {
            continue;
        }
        if !compare_operands_are_entry_values(function, &dominance, header, compare) {
            continue;
        }
        for &latch in &natural_loop.latches {
            if latch == header {
                continue;
            }
            let Some(Terminator::Br { target, args }) =
                function.block(latch).and_then(|block| block.terminator.as_ref())
            else {
                continue;
            };
            if *target != header || args.len() != header_block.params.len() {
                continue;
            }
            plans.insert(
                latch,
                RotatedLoopBranch {
                    compare: compare_inst,
                    then_target: *then_target,
                    else_target: *else_target,
                },
            );
        }
    }
    plans
}

/// Counts every instruction operand and terminator use of each value.
fn value_uses(function: &Function) -> HashMap<ValueId, usize> {
    let mut uses = HashMap::new();
    for inst in &function.instructions {
        for &operand in &inst.operands {
            *uses.entry(operand).or_insert(0) += 1;
        }
    }
    for block in &function.blocks {
        if let Some(term) = &block.terminator {
            for value in term.used_values() {
                *uses.entry(value).or_insert(0) += 1;
            }
        }
    }
    uses
}

/// Returns whether the header can be skipped on a back edge without losing values.
fn header_is_rotatable(
    function: &Function,
    header: &crate::ir::BasicBlock,
    compare: InstId,
    uses: &HashMap<ValueId, usize>,
) -> bool {
    for &inst_id in &header.instructions {
        if inst_id == compare {
            continue;
        }
        let Some(inst) = function.instruction(inst_id) else {
            return false;
        };
        if inst.op != Op::Nop {
            return false;
        }
        if inst
            .result
            .is_some_and(|result| uses.get(&result).copied().unwrap_or(0) != 0)
        {
            return false;
        }
    }
    true
}

/// Returns whether every compare operand is a header parameter or an outside value.
fn compare_operands_are_entry_values(
    function: &Function,
    dominance: &crate::ir_passes::DominanceInfo,
    header: BlockId,
    compare: &crate::ir::Instruction,
) -> bool {
    let Some(header_block) = function.block(header) else {
        return false;
    };
    compare.operands.iter().all(|&operand| {
        if header_block.params.contains(&operand) {
            return true;
        }
        let Some(value) = function.value(operand) else {
            return false;
        };
        let defining_block = match value.def {
            ValueDef::BlockParam { block, .. } | ValueDef::Instruction { block, .. } => block,
        };
        defining_block != header && dominance.dominates(defining_block, header)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        Builder, CmpPredicate, Function, Immediate, Instruction, IrType, Op, Ownership,
        Terminator, ValueId,
    };
    use crate::types::PhpType;

    /// Builds a canonical header-tested integer loop and returns its latch and header.
    fn canonical_loop() -> (Function, BlockId, BlockId) {
        let mut function = Function::new("rotated".to_string(), IrType::I64, PhpType::Int);
        let latch;
        let header;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![]);
            header = builder.create_named_block("header", vec![(IrType::I64, PhpType::Int)]);
            let body = builder.create_named_block("body", vec![]);
            let exit = builder.create_named_block("exit", vec![]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let zero = builder.emit_const_i64(0);
            let limit = builder.emit_const_i64(10);
            builder.terminate(Terminator::Br {
                target: header,
                args: vec![zero],
            });

            builder.position_at_end(header);
            let counter = builder.block_param(header, 0);
            let condition = builder
                .emit(
                    Op::ICmp,
                    vec![counter, limit],
                    Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
                    IrType::I64,
                    PhpType::Bool,
                    Ownership::NonHeap,
                )
                .expect("icmp");
            builder.terminate(Terminator::CondBr {
                cond: condition,
                then_target: body,
                then_args: vec![],
                else_target: exit,
                else_args: vec![],
            });

            builder.position_at_end(body);
            let one = builder.emit_const_i64(1);
            let next = builder
                .emit(
                    Op::IAdd,
                    vec![counter, one],
                    None,
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .expect("increment");
            builder.terminate(Terminator::Br {
                target: header,
                args: vec![next],
            });
            latch = body;

            builder.position_at_end(exit);
            builder.terminate(Terminator::Return { value: Some(counter) });
        }
        (function, latch, header)
    }

    /// The canonical header-tested loop yields one latch plan.
    #[test]
    fn canonical_loop_is_rotated() {
        let (function, latch, _) = canonical_loop();
        assert!(compute(&function).contains_key(&latch));
    }

    /// A header with a real side effect must stay on the back-edge path.
    #[test]
    fn side_effecting_header_is_not_rotated() {
        let (mut function, latch, header) = canonical_loop();
        let slot = crate::ir::LocalSlotId::from_raw(0);
        let counter = function.block(header).unwrap().params[0];
        let inst = function.instructions.len() as u32;
        function.instructions.push(Instruction::new(
            Op::StoreLocal,
            vec![counter],
            Some(Immediate::LocalSlot(slot)),
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::StoreLocal.default_effects(),
            None,
        ));
        function.blocks[header.as_raw() as usize]
            .instructions
            .push(InstId::from_raw(inst));
        assert!(!compute(&function).contains_key(&latch));
    }

    /// A condition observed outside the header cannot be replaced by a latch-local test.
    #[test]
    fn condition_used_in_body_is_not_rotated() {
        let (mut function, latch, header) = canonical_loop();
        let condition_inst = function
            .block(header)
            .unwrap()
            .instructions
            .iter()
            .find(|id| function.instruction(**id).unwrap().op == Op::ICmp)
            .copied()
            .unwrap();
        let condition = function
            .instruction(condition_inst)
            .and_then(|inst| inst.result)
            .expect("icmp result");
        let counter = function.block(header).unwrap().params[0];
        let inst = function.instructions.len() as u32;
        function.instructions.push(Instruction::new(
            Op::IAdd,
            vec![counter, condition],
            None,
            Some(ValueId::from_raw(function.values.len() as u32)),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
            Op::IAdd.default_effects(),
            None,
        ));
        function.blocks[latch.as_raw() as usize]
            .instructions
            .push(InstId::from_raw(inst));
        assert!(!compute(&function).contains_key(&latch));
    }
}
