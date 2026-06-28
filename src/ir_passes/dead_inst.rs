//! Purpose:
//! Dead instruction elimination over EIR functions. Removes result-producing
//! instructions whose values are not live over the CFG and whose effect metadata
//! says they are pure.
//!
//! Called from:
//! - The fixed-point pass driver in `crate::ir_passes::driver`.
//!
//! Key details:
//! - Instructions are neutralized to `nop` instead of physically removed so the
//!   value table keeps its existing instruction-definition slots valid.
//! - Liveness is initialized from terminator uses plus successor live-in sets,
//!   then each block is walked backward so dead chains collapse within one pass
//!   run; cross-block cascades converge through the fixed-point driver.

use std::collections::HashSet;

use crate::ir::{DataPool, Function, InstId, Instruction, Op, ValueId};

use super::driver::IrPass;
use super::liveness::{compute_liveness, terminator_uses};
use super::rewrite::neutralize_to_nop;

/// CFG-aware dead instruction elimination pass.
pub struct DeadInst;

impl IrPass for DeadInst {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "dead-inst"
    }

    /// Neutralizes dead, result-producing instructions and reports whether any
    /// instruction changed. The literal pool is unused because the pass never
    /// materializes new constants.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let dead = collect_dead_instructions(function);
        if dead.is_empty() {
            return false;
        }
        for inst_id in dead {
            if let Some(inst) = function.instruction_mut(inst_id) {
                neutralize_to_nop(inst);
            }
        }
        true
    }
}

/// Finds all instructions that can be neutralized in the current pass sweep.
fn collect_dead_instructions(function: &Function) -> Vec<InstId> {
    let liveness = compute_liveness(function);
    let mut dead = Vec::new();
    for block in function.blocks.iter().rev() {
        let mut live: HashSet<ValueId> = liveness.live_out_of(block.id).clone();
        if let Some(term) = block.terminator.as_ref() {
            live.extend(terminator_uses(term));
        }

        for &inst_id in block.instructions.iter().rev() {
            let inst = function
                .instruction(inst_id)
                .expect("block references a valid instruction");
            if inst.op == Op::Nop {
                if let Some(result) = inst.result {
                    live.remove(&result);
                }
                continue;
            }
            if instruction_is_dead(inst, &live) {
                dead.push(inst_id);
                continue;
            }
            if let Some(result) = inst.result {
                live.remove(&result);
            }
            live.extend(inst.operands.iter().copied());
        }
    }
    dead
}

/// Returns true when an instruction's result is unused and its effect metadata
/// says the instruction is pure.
fn instruction_is_dead(inst: &Instruction, live: &HashSet<ValueId>) -> bool {
    let Some(result) = inst.result else {
        return false;
    };
    !live.contains(&result) && inst.effects.is_pure()
}
