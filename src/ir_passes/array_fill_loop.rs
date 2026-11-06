//! Purpose:
//! Reserves capacity for a pure fill loop and rewrites its appends to the
//! check-free reserved push.
//!
//! Called from:
//! - The EIR fixed-point driver after the typed-array reduction rewrite.
//!
//! Key details:
//! - The loop must be a top-level `while (i < N) { $data[] = <pure int expr>; i += 1; }`
//!   over a private, non-null, int-shaped array created immediately before it.
//! - `N` is a constant, so the pass can both size the initial `array_new` and prove
//!   that every append has reserved capacity. The value expression is limited to
//!   non-throwing int arithmetic so preallocating cannot change when a failure is
//!   observed.
//! - The array must be the only owner, its only append must be the matched push,
//!   and the induction value must not be observed outside the loop.
//! - The rewritten push stores one int and advances the header length without COW,
//!   growth, metadata checks or a source-local writeback.

use crate::ir::{
    BlockId, CmpPredicate, DataPool, Function, Immediate, InstId, LocalKind, LocalSlotId, Op,
    PassOrigin, Terminator, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::array_get_bounds::slot_is_private_int_array;
use super::array_sum_loop::{induction, induction_has_no_external_uses, Induction};
use super::cfg::has_exception_handlers;
use super::dominance::compute_dominance;
use super::driver::IrPass;
use super::loops::{compute_loops, NaturalLoop};
use super::rewrite::defining_instruction;

/// Rewrites a pure typed-array fill loop to the reserved append path.
pub struct ArrayFillLoop;

impl IrPass for ArrayFillLoop {
    /// Names the pass in fixed-point validation errors.
    fn name(&self) -> &'static str {
        "array_fill_loop"
    }

    /// Reserves capacity and rewrites eligible fill-loop appends.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if has_exception_handlers(function)
            || function.flags.is_generator
            || function.flags.is_fiber_wrapper
            || super::local_scope::has_dynamic_eval(function)
            || function.instructions.iter().any(|inst| {
                inst.op == Op::RuntimeCall && matches!(inst.immediate, Some(Immediate::Data(_)))
            })
        {
            return false;
        }
        let dominance = compute_dominance(function);
        let loops = compute_loops(function, &dominance);
        if loops.is_empty() {
            return false;
        }
        let mut changed = false;
        for loop_ref in loops.loops() {
            if loop_ref.parent.is_some() || loop_ref.blocks.len() != 2 {
                continue;
            }
            let Some((slot, bound, induction)) = fill_bound(function, loop_ref) else {
                continue;
            };
            if !slot_is_private_int_array(function, slot)
                || !induction_has_no_external_uses(function, loop_ref, induction.param)
            {
                continue;
            }
            let Some((array_new, store_block)) = array_new_for_slot(function, slot) else {
                continue;
            };
            let Some(preheader) = loop_ref.preheader else {
                continue;
            };
            if !dominance.dominates(store_block, preheader) {
                continue;
            }
            let Some((push, value)) = fill_push(function, loop_ref, slot) else {
                continue;
            };
            if !value_is_pure_int(function, loop_ref, induction, slot, value) {
                continue;
            }
            let Some(start) = constant_i64(function, induction.start_value) else {
                continue;
            };
            let capacity = bound - start;
            if capacity < 0 || capacity > i64::from(i32::MAX) {
                continue;
            }
            if count_slot_pushes(function, slot) != 1 {
                continue;
            }
            if reserve_array_new(function, array_new, capacity)
                && rewrite_push(function, push)
            {
                changed = true;
            }
        }
        changed
    }
}

/// Returns the matched array slot, the constant exclusive bound and the induction.
fn fill_bound(
    function: &Function,
    loop_ref: &NaturalLoop,
) -> Option<(LocalSlotId, i64, Induction)> {
    let header = function.block(loop_ref.header)?;
    let terminator = header.terminator.as_ref()?;
    let Terminator::CondBr { cond, then_target, else_target, else_args, .. } = terminator else {
        return None;
    };
    if !loop_ref.contains(*then_target)
        || !else_args.is_empty()
        || !function.block(*else_target)?.params.is_empty()
    {
        return None;
    }
    let comparison = defining_instruction(function, *cond)?;
    if !matches!(comparison.op, Op::ICmp | Op::PhpRelCmp)
        || !matches!(
            comparison.immediate,
            Some(Immediate::CmpPredicate(CmpPredicate::Slt))
        )
    {
        return None;
    }
    let param = *comparison.operands.first()?;
    let bound = constant_i64(function, *comparison.operands.get(1)?)?;
    if bound < 0 {
        return None;
    }
    let induction = induction(function, loop_ref, param)?;
    let slot = fill_slot(function, loop_ref)?;
    Some((slot, bound, induction))
}

/// Returns the private array slot whose only loop use is the matched append.
fn fill_slot(function: &Function, loop_ref: &NaturalLoop) -> Option<LocalSlotId> {
    let body = loop_ref
        .blocks
        .iter()
        .copied()
        .find(|&block| block != loop_ref.header)?;
    let mut slot = None;
    for &id in &function.block(body)?.instructions {
        let inst = function.instruction(id)?;
        match inst.op {
            Op::LoadLocal => {
                let Some(Immediate::LocalSlot(candidate)) = inst.immediate else {
                    continue;
                };
                let local = function.locals.get(candidate.as_raw() as usize)?;
                if local.kind != LocalKind::PhpLocal
                    || !matches!(
                        local.php_type.codegen_repr(),
                        PhpType::Array(_) | PhpType::AssocArray { .. }
                    )
                {
                    continue;
                }
                if slot.replace(candidate).is_some() {
                    return None;
                }
            }
            Op::ArrayPush => {}
            _ => {}
        }
    }
    slot
}

/// Returns the single append in the body and its value operand.
fn fill_push(
    function: &Function,
    loop_ref: &NaturalLoop,
    slot: LocalSlotId,
) -> Option<(InstId, ValueId)> {
    let body = loop_ref
        .blocks
        .iter()
        .copied()
        .find(|&block| block != loop_ref.header)?;
    let mut push = None;
    for &id in &function.block(body)?.instructions {
        let inst = function.instruction(id)?;
        if inst.op != Op::ArrayPush {
            continue;
        }
        if push.replace(id).is_some() {
            return None;
        }
        let array = *inst.operands.first()?;
        let load = defining_instruction(function, array)?;
        if load.op != Op::LoadLocal || load.immediate != Some(Immediate::LocalSlot(slot)) {
            return None;
        }
    }
    let push = push?;
    let value = *function.instruction(push)?.operands.get(1)?;
    Some((push, value))
}

/// Counts appends whose receiver is a load of `slot`.
fn count_slot_pushes(function: &Function, slot: LocalSlotId) -> usize {
    function
        .instructions
        .iter()
        .filter(|inst| {
            inst.op == Op::ArrayPush
                && inst.operands.first().is_some_and(|&array| {
                    defining_instruction(function, array).is_some_and(|load| {
                        load.op == Op::LoadLocal
                            && load.immediate == Some(Immediate::LocalSlot(slot))
                    })
                })
        })
        .count()
}

/// Returns the slot's `array_new` and the block of its owning store.
fn array_new_for_slot(
    function: &Function,
    slot: LocalSlotId,
) -> Option<(InstId, BlockId)> {
    let mut found = None;
    let mut stores = 0;
    for block in &function.blocks {
        for &id in &block.instructions {
            let inst = function.instruction(id)?;
            if inst.op != Op::StoreLocal || inst.immediate != Some(Immediate::LocalSlot(slot)) {
                continue;
            }
            stores += 1;
            if stores > 1 {
                return None;
            }
            let value = *inst.operands.first()?;
            let mut current = defining_inst_id(function, value)?;
            for _ in 0..8 {
                let definition = function.instruction(current)?;
                match definition.op {
                    Op::Acquire | Op::Borrow | Op::Move | Op::EnsureOwned => {
                        current = defining_inst_id(function, *definition.operands.first()?)?;
                    }
                    Op::ArrayNew => {
                        found = Some((current, block.id));
                        break;
                    }
                    _ => return None,
                }
            }
        }
    }
    found
}

/// Requires every non-scaffolding body operation to be non-throwing int arithmetic.
fn value_is_pure_int(
    function: &Function,
    loop_ref: &NaturalLoop,
    induction: Induction,
    slot: LocalSlotId,
    value: ValueId,
) -> bool {
    let Some(body) = loop_ref
        .blocks
        .iter()
        .copied()
        .find(|&block| block != loop_ref.header)
    else {
        return false;
    };
    if function
        .value(value)
        .is_none_or(|value| value.php_type.codegen_repr() != PhpType::Int)
    {
        return false;
    }
    for &id in &function.block(body).unwrap().instructions {
        let Some(inst) = function.instruction(id) else {
            return false;
        };
        match inst.op {
            Op::Nop | Op::ConcatReset | Op::ConstI64 | Op::ArrayPush => {}
            Op::LoadLocal => {
                let Some(Immediate::LocalSlot(loaded)) = inst.immediate else {
                    return false;
                };
                let Some(local) = function.locals.get(loaded.as_raw() as usize) else {
                    return false;
                };
                if loaded != slot && local.php_type.codegen_repr() != PhpType::Int {
                    return false;
                }
            }
            Op::ISMod => {
                let Some(&divisor) = inst.operands.get(1) else {
                    return false;
                };
                if constant_i64(function, divisor).is_none_or(|value| value == 0) {
                    return false;
                }
            }
            Op::IAdd | Op::ISub | Op::IMul => {
                if inst.origin != Some(PassOrigin::IntegerRange) && id != induction.latch {
                    return false;
                }
            }
            Op::IBitAnd | Op::IBitOr | Op::IBitXor => {}
            Op::IShl | Op::IShrA => {
                let Some(&shift) = inst.operands.get(1) else {
                    return false;
                };
                if constant_i64(function, shift).is_none_or(|value| !(0..64).contains(&value)) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// Sets the initial array's capacity and stamps it as an int-slot array.
fn reserve_array_new(function: &mut Function, array_new: InstId, capacity: i64) -> bool {
    let Some(inst) = function.instruction_mut(array_new) else {
        return false;
    };
    let Some(result) = inst.result else {
        return false;
    };
    inst.immediate = Some(Immediate::Capacity(capacity as u32));
    inst.result_php_type = PhpType::Array(Box::new(PhpType::Int));
    if let Some(value) = function.value_mut(result) {
        value.php_type = PhpType::Array(Box::new(PhpType::Int));
    }
    true
}

/// Rewrites the append to the reserved, check-free form.
fn rewrite_push(function: &mut Function, push: InstId) -> bool {
    let Some(inst) = function.instruction_mut(push) else {
        return false;
    };
    inst.op = Op::ArrayPushReserved;
    inst.effects = Op::ArrayPushReserved.default_effects();
    inst.origin = Some(PassOrigin::ArrayFillLoop);
    true
}

/// Reads one integer constant, if the value is defined by `const_i64`.
fn constant_i64(function: &Function, value: ValueId) -> Option<i64> {
    let inst = defining_instruction(function, value)?;
    if inst.op != Op::ConstI64 {
        return None;
    }
    match inst.immediate {
        Some(Immediate::I64(value)) => Some(value),
        _ => None,
    }
}

/// Returns the instruction id that defines `value`.
fn defining_inst_id(function: &Function, value: ValueId) -> Option<InstId> {
    match function.value(value)?.def {
        ValueDef::Instruction { inst, .. } => Some(inst),
        ValueDef::BlockParam { .. } => None,
    }
}
