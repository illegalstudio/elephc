//! Purpose:
//! Plans scalar constant recipes and profitable register retention for EIR.
//!
//! Called from:
//! - Register allocation and codegen integer immediate selection.
//!
//! Key details:
//! - Recipes preserve float bits and the raw null sentinel. Heap values and
//!   two-word tagged scalars never participate.
//! - Only register-requiring uses contribute cost. Loop frequency is estimated
//!   conservatively, and long live ranges pay a register-pressure surcharge.

use std::collections::HashMap;

use crate::codegen::platform::Target;
use crate::codegen_support::abi::immediate::{
    int_materialization_cost, select_int_immediate, ImmediateOp, IntImmediate,
};
use crate::ir::{Function, Immediate, Instruction, IrType, Op, Ownership, ValueDef, ValueId};
use crate::types::PhpType;

use super::intervals::LiveInterval;
use super::liveness::terminator_uses;
use super::{compute_dominance, compute_loops};

/// Bit-exact single-register scalar recipe, independent of an allocated home.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarConstant {
    Integer(i64),
    Float(u64),
}

/// Reads a scalar definition without folding expressions or looking through mutable storage.
pub(crate) fn scalar_constant(func: &Function, id: ValueId) -> Option<ScalarConstant> {
    let value = func.value(id)?;
    if value.ownership != Ownership::NonHeap { return None; }
    let ValueDef::Instruction { inst, .. } = value.def else { return None; };
    let inst = func.instruction(inst)?;
    match (inst.op, &inst.immediate, value.ir_type) {
        (Op::ConstI64, Some(Immediate::I64(n)), IrType::I64) => Some(ScalarConstant::Integer(*n)),
        (Op::ConstBool, Some(Immediate::Bool(b)), IrType::I64) => Some(ScalarConstant::Integer(i64::from(*b))),
        (Op::ConstNull, _, IrType::I64) if value.php_type.codegen_repr() != PhpType::TaggedScalar => {
            Some(ScalarConstant::Integer(crate::codegen_support::sentinels::NULL_SENTINEL))
        }
        (Op::ConstF64, Some(Immediate::F64(f)), IrType::F64) => Some(ScalarConstant::Float(f.to_bits())),
        _ => None,
    }
}

/// A chosen immediate use and the other operand that still needs a register.
pub(crate) struct ImmediateUse {
    pub constant_index: usize,
    pub other: ValueId,
    pub encoding: IntImmediate,
}

/// Selects one constant operand, swapping commutative operations or comparison predicates later.
pub(crate) fn immediate_use(func: &Function, inst: &Instruction, target: Target) -> Option<ImmediateUse> {
    let (op, commutative) = match inst.op {
        Op::IAdd | Op::ICheckedAddToInt => (ImmediateOp::Add, true),
        Op::ISub | Op::ICheckedSubToInt => (ImmediateOp::Sub, false),
        Op::IMul | Op::ICheckedMulToInt => (ImmediateOp::Mul, true),
        Op::IBitAnd => (ImmediateOp::And, true),
        Op::IBitOr => (ImmediateOp::Or, true),
        Op::IBitXor => (ImmediateOp::Xor, true),
        Op::ICmp => (ImmediateOp::Compare, true),
        _ => return None,
    };
    if inst.operands.len() != 2 { return None; }
    for index in [1, 0] {
        if index == 0 && !commutative { continue; }
        let id = inst.operands[index];
        // Null's stored sentinel is not its arithmetic value (zero), and is
        // deliberately left to the existing PHP coercion path.
        if !matches!(func.value(id)?.php_type, PhpType::Int | PhpType::Bool) { continue; }
        let Some(ScalarConstant::Integer(n)) = scalar_constant(func, id) else { continue; };
        if let Some(encoding) = select_int_immediate(target.arch, op, n) {
            return Some(ImmediateUse { constant_index: index, other: inst.operands[1 - index], encoding });
        }
    }
    None
}

/// Collects all recipes, including constants in implicit handler blocks.
pub(crate) fn constants(func: &Function) -> HashMap<ValueId, ScalarConstant> {
    func.values.iter().enumerate().filter_map(|(index, _)| {
        let id = ValueId::from_raw(index as u32);
        scalar_constant(func, id).map(|constant| (id, constant))
    }).collect()
}

/// Removes cheap recipes from allocation and weights costly ones by avoided reconstruction work.
pub(crate) fn price_intervals(
    func: &Function,
    target: Target,
    recipes: &HashMap<ValueId, ScalarConstant>,
    intervals: &mut Vec<LiveInterval>,
) {
    if recipes.is_empty() { return; }
    // One-instruction reconstruction is no dearer than a register move, so
    // ordinary small-integer functions need no additional CFG analysis.
    let costs: HashMap<_, _> = recipes.iter().map(|(&id, recipe)| {
        let cost = match recipe {
            ScalarConstant::Integer(n) => int_materialization_cost(target.arch, *n),
            ScalarConstant::Float(bits) => match target.arch {
                crate::codegen::platform::Arch::X86_64 => 2,
                crate::codegen::platform::Arch::AArch64 => int_materialization_cost(target.arch, *bits as i64) + 1,
            },
        };
        (id, cost)
    }).collect();
    intervals.retain(|iv| costs.get(&iv.value) != Some(&1));
    if !intervals.iter().any(|iv| costs.contains_key(&iv.value)) { return; }
    let loops = compute_loops(func, &compute_dominance(func));
    let frequency = |block| 8u32.pow(loops.loop_depth(block).min(3));
    let mut uses: HashMap<ValueId, u32> = HashMap::new();
    for block in &func.blocks {
        let weight = frequency(block.id);
        for id in &block.instructions {
            let inst = func.instruction(*id).expect("valid instruction");
            let immediate = immediate_use(func, inst, target);
            for (index, operand) in inst.operands.iter().enumerate() {
                if immediate.as_ref().is_some_and(|usage| usage.constant_index == index) { continue; }
                if recipes.contains_key(operand) {
                    let count = uses.entry(*operand).or_default();
                    *count = count.saturating_add(weight);
                }
            }
        }
        if let Some(term) = &block.terminator {
            for operand in terminator_uses(term) {
                if recipes.contains_key(&operand) {
                    let count = uses.entry(operand).or_default();
                    *count = count.saturating_add(weight);
                }
            }
        }
    }
    intervals.retain_mut(|iv| {
        let Some(&cost) = costs.get(&iv.value) else { return true; };
        let ValueDef::Instruction { block, .. } = func.value(iv.value).expect("value exists").def else {
            unreachable!("constant definition");
        };
        let saving = uses.get(&iv.value).copied().unwrap_or(0).saturating_mul(cost - 1);
        let setup = (cost + 1) * frequency(block) + if iv.call_free { 0 } else { 2 };
        let pressure = ((iv.end - iv.start) / 32).min(8);
        if saving <= setup + pressure { return false; }
        iv.weight = saving - setup - pressure;
        true
    });
}
