//! Purpose:
//! Transfers fresh checked-numeric boxes into assignment copies without allocating
//! a second Mixed cell or releasing the transferred owner.
//!
//! Called from:
//! - `driver::optimize_module()` after the ordinary optimization fixed point.
//!
//! Key details:
//! - Only checked integer add/sub/mul results are known independent numeric cells.
//! - The source must be used only by the clone and its adjacent release.
//! - No integer-sink specialization runs after this ownership-only rewrite.

use std::collections::{HashMap, HashSet};

use crate::ir::{DataPool, Function, IrHeapKind, IrType, Op, Ownership, ValueDef};

use super::driver::IrPass;
use super::rewrite::{count_value_uses, neutralize_to_nop, replace_all_uses};

/// Removes redundant copies of independently allocated numeric Mixed cells.
pub struct FreshMixedTransfer;

impl IrPass for FreshMixedTransfer {
    /// Names this ownership finalization pass for validation diagnostics.
    fn name(&self) -> &'static str {
        "fresh-mixed-transfer"
    }

    /// Keeps ownership transfer separate from checked integer sink discovery.
    fn run_after_fixed_point(&self) -> bool {
        true
    }

    /// Avoids use counting when the function has no assignment clone.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| inst.op == Op::MixedClone)
    }

    /// Replaces the copied owner only when the original cannot escape or survive.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let uses = count_value_uses(function);
        let mut replacements = HashMap::new();
        let mut removed = Vec::new();

        for block in &function.blocks {
            let mut preceding = HashSet::new();
            for (position, &clone_id) in block.instructions.iter().enumerate() {
                preceding.insert(clone_id);
                let clone = &function.instructions[clone_id.as_raw() as usize];
                if clone.op != Op::MixedClone || clone.operands.len() != 1
                    || clone.immediate.is_some()
                {
                    continue;
                }
                let source = clone.operands[0];
                let Some(result) = clone.result else { continue };
                let Some(source_value) = function.value(source) else { continue };
                let Some(result_value) = function.value(result) else { continue };
                if source_value.ownership != Ownership::Owned
                    || result_value.ownership != Ownership::Owned
                    || source_value.ir_type != IrType::Heap(IrHeapKind::Mixed)
                    || source_value.ir_type != result_value.ir_type
                    || uses.get(&source) != Some(&2)
                {
                    continue;
                }
                let ValueDef::Instruction { inst: producer_id, .. } = source_value.def else {
                    continue;
                };
                let producer = &function.instructions[producer_id.as_raw() as usize];
                if !matches!(producer.op, Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul)
                    || !preceding.contains(&producer_id)
                {
                    continue;
                }
                let next = block.instructions[position + 1..].iter().copied().find(|id| {
                    function.instructions[id.as_raw() as usize].op != Op::Nop
                });
                let Some(release_id) = next else { continue };
                let release = &function.instructions[release_id.as_raw() as usize];
                if release.op != Op::Release || release.operands != [source]
                    || release.immediate.is_some()
                {
                    continue;
                }
                replacements.insert(result, source);
                removed.extend([clone_id, release_id]);
            }
        }

        if replacements.is_empty() {
            return false;
        }
        replace_all_uses(function, &replacements);
        for id in removed {
            neutralize_to_nop(&mut function.instructions[id.as_raw() as usize]);
        }
        true
    }
}
