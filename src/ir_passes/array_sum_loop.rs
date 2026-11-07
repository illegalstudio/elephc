//! Purpose:
//! Replaces a typed int-array reduction loop with one runtime helper call.
//!
//! Called from:
//! - The EIR fixed-point driver after integer-range specialization and the
//!   typed array-bounds proof.
//!
//! Key details:
//! - The matched loop is `while (i < count($data)) { $sum += $data[$i]; $i += 1; }`
//!   where the array is a private, dense `array<int>` and `count` was hoisted.
//! - The body must be exactly the load/unchecked-read/add/acquire/release/store
//!   scaffold plus the unit latch; any other instruction or observation rejects
//!   the rewrite.
//! - `__rt_array_sum_int` reproduces PHP's left-to-right addition with int-to-float
//!   promotion, so the loop's value semantics, seed, empty range and overflow are
//!   unchanged. The induction value must not be observed after the loop.
//! - The preheader branches straight to the loop exit; the now-unreachable loop
//!   blocks are neutralized by the later branch simplification pass.

use crate::ir::{
    BlockId, DataPool, Function, Immediate, InstId, Instruction, IrHeapKind, IrType, LocalKind,
    LocalSlotId, MixedNumericOp, Op, Ownership, PassOrigin, Terminator, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::array_get_bounds::{loop_bound, slot_is_private_int_array, LoopBound};
use super::cfg::has_exception_handlers;
use super::dominance::compute_dominance;
use super::driver::IrPass;
use super::licm::build_def_block;
use super::loops::{compute_loops, NaturalLoop};
use super::rewrite::defining_instruction;

/// Rewrites a proven typed int-array reduction loop into a helper call.
pub struct ArraySumLoop;

impl IrPass for ArraySumLoop {
    /// Names the pass in fixed-point validation errors.
    fn name(&self) -> &'static str {
        "array_sum_loop"
    }

    /// Rewrites every loop whose body is exactly the typed-array reduction scaffold.
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
        let def_block = build_def_block(function);
        let mut changed = false;
        for loop_ref in loops.loops() {
            if loop_ref.blocks.len() != 2 {
                continue;
            }
            let Some(bound) = loop_bound(function, loop_ref, &def_block) else {
                continue;
            };
            if !slot_is_private_int_array(function, bound.slot) {
                continue;
            }
            let Some(preheader) = loop_ref.preheader else {
                continue;
            };
            if def_block.get(&bound.count) != Some(&preheader) {
                continue;
            }
            let Some(induction) = induction(function, loop_ref, bound.param) else {
                continue;
            };
            let Some(plan) = reduction_plan(function, loop_ref, bound, induction) else {
                continue;
            };
            if !super::numeric_domain::inputs(function, &[plan.binop_result]) {
                continue;
            }
            if !induction_has_no_external_uses(function, loop_ref, bound.param) {
                continue;
            }
            apply(function, preheader, bound, induction, plan);
            changed = true;
        }
        changed
    }
}

/// The proven unit induction of one loop.
#[derive(Clone, Copy)]
pub(super) struct Induction {
    /// The header block parameter advanced by the latch.
    pub(super) param: ValueId,
    /// Value passed into the header from the preheader.
    pub(super) start_value: ValueId,
    /// Instruction id of the latch's `i + 1`.
    pub(super) latch: InstId,
}

/// The reduction scaffold recovered from the loop body.
#[derive(Clone, Copy)]
struct ReductionPlan {
    binop_result: ValueId,
    sum_slot: LocalSlotId,
    exit: BlockId,
}

/// Requires a non-negative constant start and a `i + 1` latch.
pub(super) fn induction(
    function: &Function,
    loop_ref: &NaturalLoop,
    param: ValueId,
) -> Option<Induction> {
    let header = function.block(loop_ref.header)?;
    let position = header.params.iter().position(|&value| value == param)?;
    let preheader = loop_ref.preheader.and_then(|id| function.block(id))?;
    let Some(Terminator::Br { target, args }) = preheader.terminator.as_ref() else {
        return None;
    };
    if *target != loop_ref.header || loop_ref.latches.len() != 1 {
        return None;
    }
    let start_value = *args.get(position)?;
    if constant_i64(function, start_value).is_none_or(|start| start < 0) {
        return None;
    }
    let latch = function.block(loop_ref.latches[0])?;
    let Some(Terminator::Br { target, args }) = latch.terminator.as_ref() else {
        return None;
    };
    if *target != loop_ref.header {
        return None;
    }
    let next = *args.get(position)?;
    let add = defining_instruction(function, next)?;
    if add.op != Op::IAdd || !matches!(add.immediate, None) {
        return None;
    }
    let left = *add.operands.first()?;
    let right = *add.operands.get(1)?;
    let step = if left == param {
        right
    } else if right == param {
        left
    } else {
        return None;
    };
    if constant_i64(function, step) != Some(1) {
        return None;
    }
    Some(Induction { param, start_value, latch: defining_inst_id(function, next)? })
}

/// Returns the instruction id that defines `value`.
fn defining_inst_id(function: &Function, value: ValueId) -> Option<InstId> {
    match function.value(value)?.def {
        ValueDef::Instruction { inst, .. } => Some(inst),
        ValueDef::BlockParam { .. } => None,
    }
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

/// Recovers the exact reduction scaffold from the loop's single body block.
fn reduction_plan(
    function: &Function,
    loop_ref: &NaturalLoop,
    bound: LoopBound,
    induction: Induction,
) -> Option<ReductionPlan> {
    let body = loop_ref
        .blocks
        .iter()
        .copied()
        .find(|&block| block != loop_ref.header)?;
    let block = function.block(body)?;
    let Some(Terminator::CondBr { then_target, else_target, else_args, .. }) =
        function.block(loop_ref.header)?.terminator.as_ref()
    else {
        return None;
    };
    if *then_target != body || !else_args.is_empty() || !function.block(*else_target)?.params.is_empty() {
        return None;
    }
    let mut get = None;
    let mut binop = None;
    let mut clone = None;
    let mut acquire = None;
    let mut release_slot = None;
    let mut store = None;
    let mut releases = Vec::new();
    let mut add = None;
    let mut loads = Vec::new();
    for &id in &block.instructions {
        let inst = function.instruction(id)?;
        match inst.op {
            Op::Nop | Op::ConcatReset => {}
            Op::LoadLocal => loads.push(id),
            Op::ArrayGetUnchecked => {
                if get.replace(id).is_some() {
                    return None;
                }
            }
            Op::MixedNumericBinop => {
                if binop.replace(id).is_some() {
                    return None;
                }
            }
            Op::MixedClone => {
                if clone.replace(id).is_some() {
                    return None;
                }
            }
            Op::Acquire => {
                if acquire.replace(id).is_some() {
                    return None;
                }
            }
            Op::ReleaseLocalSlot => {
                if release_slot.replace(id).is_some() {
                    return None;
                }
            }
            Op::StoreLocal => {
                if store.replace(id).is_some() {
                    return None;
                }
            }
            Op::Release => releases.push(id),
            Op::IAdd => {
                if add.replace(id).is_some() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    let (get, binop, acquire, release_slot, store, add) =
        (get?, binop?, acquire?, release_slot?, store?, add?);
    if add != induction.latch {
        return None;
    }
    let get_inst = function.instruction(get)?;
    if get_inst.operands.len() != 2 || get_inst.operands[1] != bound.param {
        return None;
    }
    let array_value = get_inst.operands[0];
    let array_load = defining_instruction(function, array_value)?;
    if array_load.op != Op::LoadLocal
        || array_load.immediate != Some(Immediate::LocalSlot(bound.slot))
    {
        return None;
    }
    let get_result = get_inst.result?;
    let binop_inst = function.instruction(binop)?;
    if !matches!(binop_inst.immediate, Some(Immediate::MixedNumericOp(MixedNumericOp::Add)))
        || binop_inst.operands.len() != 2
    {
        return None;
    }
    let (sum_load, _) = if binop_inst.operands[0] == get_result {
        (binop_inst.operands[1], binop_inst.operands[0])
    } else if binop_inst.operands[1] == get_result {
        (binop_inst.operands[0], binop_inst.operands[1])
    } else {
        return None;
    };
    let sum_def = defining_instruction(function, sum_load)?;
    if sum_def.op != Op::LoadLocal {
        return None;
    }
    let Some(Immediate::LocalSlot(sum_slot)) = sum_def.immediate else {
        return None;
    };
    let sum_local = function.locals.get(sum_slot.as_raw() as usize)?;
    if sum_local.kind != LocalKind::PhpLocal
        || sum_local.ir_type != IrType::Heap(IrHeapKind::Mixed)
        || sum_local.php_type != PhpType::Mixed
        || (sum_slot.as_raw() as usize) < function.params.len()
    {
        return None;
    }
    let binop_result = binop_inst.result?;
    let clone_result = match clone {
        Some(clone) => {
            let clone_inst = function.instruction(clone)?;
            if clone_inst.operands.as_slice() != [binop_result] {
                return None;
            }
            clone_inst.result
        }
        None => None,
    };
    let value_owner = clone_result.unwrap_or(binop_result);
    let acquire_inst = function.instruction(acquire)?;
    if acquire_inst.operands.as_slice() != [value_owner]
        && acquire_inst.operands.as_slice() != [binop_result]
    {
        return None;
    }
    let expected = acquire_inst.result?;
    let stored = *function.instruction(store)?.operands.first()?;
    if stored != expected
        || function.instruction(store)?.immediate != Some(Immediate::LocalSlot(sum_slot))
        || function.instruction(release_slot)?.immediate != Some(Immediate::LocalSlot(sum_slot))
    {
        return None;
    }
    for release in releases {
        let released = *function.instruction(release)?.operands.first()?;
        if released != binop_result && Some(released) != clone_result {
            return None;
        }
    }
    // Every load in the body must be one of the two matched producer loads.
    let matched_loads: Vec<InstId> = [array_value, sum_load]
        .into_iter()
        .filter_map(|value| defining_inst_id(function, value))
        .collect();
    if loads.iter().any(|load| !matched_loads.contains(load)) {
        return None;
    }
    // The receiver load feeds only the get, and the get/sum loads feed only the add.
    let array_users = function
        .instructions
        .iter()
        .filter(|inst| inst.operands.contains(&array_value))
        .count();
    if array_users != 1 {
        return None;
    }
    for inst in &function.instructions {
        if inst.operands.contains(&get_result) && inst.op != Op::MixedNumericBinop {
            return None;
        }
        if inst.operands.contains(&sum_load) && inst.op != Op::MixedNumericBinop {
            return None;
        }
    }
    Some(ReductionPlan { binop_result, sum_slot, exit: *else_target })
}

/// Rejects a loop whose induction value is observed outside the loop.
pub(super) fn induction_has_no_external_uses(
    function: &Function,
    loop_ref: &NaturalLoop,
    param: ValueId,
) -> bool {
    for block in &function.blocks {
        let inside = loop_ref.contains(block.id);
        for &id in &block.instructions {
            let Some(inst) = function.instruction(id) else {
                return false;
            };
            if inst.operands.contains(&param) && !inside {
                return false;
            }
        }
        if let Some(term) = block.terminator.as_ref() {
            if term.used_values().contains(&param) && !inside {
                return false;
            }
        }
    }
    true
}

/// Emits the helper call in the preheader and redirects the loop to its exit.
fn apply(
    function: &mut Function,
    preheader: BlockId,
    bound: LoopBound,
    induction: Induction,
    plan: ReductionPlan,
) {
    let Some(count_receiver) = defining_instruction(function, bound.count).and_then(|count| {
        count.operands.first().copied()
    }) else {
        return;
    };
    let seed = push_instruction(
        function,
        preheader,
        Op::LoadLocal,
        vec![],
        Some(Immediate::LocalSlot(plan.sum_slot)),
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
        Ownership::MaybeOwned,
    );
    let Some(seed) = seed else {
        return;
    };
    let Some(sum) = push_instruction(
        function,
        preheader,
        Op::ArraySumInt,
        vec![count_receiver, induction.start_value, bound.count, seed],
        None,
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
        Ownership::Owned,
    ) else {
        return;
    };
    let _ = push_instruction(
        function,
        preheader,
        Op::ReleaseLocalSlot,
        vec![],
        Some(Immediate::LocalSlot(plan.sum_slot)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    let _ = push_instruction(
        function,
        preheader,
        Op::StoreLocal,
        vec![sum],
        Some(Immediate::LocalSlot(plan.sum_slot)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    if let Some(block) = function.block_mut(preheader) {
        block.terminator = Some(Terminator::Br {
            target: plan.exit,
            args: vec![],
        });
    }
}

/// Appends one instruction to a block and materializes a result value when typed.
fn push_instruction(
    function: &mut Function,
    block: BlockId,
    op: Op,
    operands: Vec<ValueId>,
    immediate: Option<Immediate>,
    result_type: IrType,
    result_php_type: PhpType,
    ownership: Ownership,
) -> Option<ValueId> {
    let block_index = block.as_raw() as usize;
    let position = function.blocks.get(block_index)?.instructions.len();
    let result = (!result_type.is_void()).then(|| {
        let value = ValueId::from_raw(function.values.len() as u32);
        function.values.push(crate::ir::Value {
            ir_type: result_type,
            php_type: result_php_type.clone(),
            def: ValueDef::Instruction {
                block,
                index: position as u32,
                inst: InstId::from_raw(function.instructions.len() as u32),
            },
            ownership,
        });
        value
    });
    let mut inst = Instruction::new(
        op,
        operands,
        immediate,
        result,
        result_type,
        result_php_type,
        ownership,
        op.default_effects(),
        None,
    );
    inst.origin = Some(PassOrigin::ArraySumLoop);
    let id = InstId::from_raw(function.instructions.len() as u32);
    function.instructions.push(inst);
    function.blocks[block_index].instructions.push(id);
    result
}
