//! Purpose:
//! Rewrites a typed indexed read into its unchecked form when the enclosing loop
//! condition and the array's private construction prove it cannot miss.
//!
//! Called from:
//! - The EIR fixed-point driver immediately after loop-invariant count hoisting.
//!
//! Key details:
//! - Only a `0 <= i`-seeded induction parameter advanced by a positive constant and
//!   bounded by a typed-array `count()` in the loop condition qualifies.
//! - The receiver slot must be a non-parameter PHP local built only from `array_new`
//!   and int-valued appends, and every load of it must reach a read-only consumer.
//! - The array is loop-invariant, so the hoisted count and the header condition hold
//!   on every path to the body; the read's index is the header induction value itself.
//! - The rewritten result is a raw `Int`, and its only consumers may be the
//!   null-as-zero `TaggedScalar -> Int` casts that the bounds proof makes redundant.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    BlockId, CmpPredicate, DataPool, Function, Immediate, InstId, Instruction, IrHeapKind,
    IrType, LocalKind, LocalSlotId, Op, Ownership, PassOrigin, Terminator, ValueId,
};
use crate::types::PhpType;

use super::cfg::has_exception_handlers;
use super::count_hoist::{defining_id, is_single_argument_count, slot_is_loop_invariant};
use super::driver::IrPass;
use super::licm::build_def_block;
use super::loops::{compute_loops, NaturalLoop};
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Rewrites provably in-bounds typed indexed reads to `array_get_unchecked`.
pub struct ArrayGetBounds;

impl IrPass for ArrayGetBounds {
    /// Names the pass in fixed-point validation errors.
    fn name(&self) -> &'static str {
        "array_get_bounds"
    }

    /// Rewrites every read whose loop bound and receiver construction prove it safe.
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
        let dominance = super::dominance::compute_dominance(function);
        let loops = compute_loops(function, &dominance);
        if loops.is_empty() {
            return false;
        }
        let def_block = build_def_block(function);
        let mut changed = false;
        let mut order: Vec<usize> = (0..loops.loops().len()).collect();
        order.sort_by_key(|&index| std::cmp::Reverse(loops.loops()[index].depth));
        for index in order {
            let loop_ref = &loops.loops()[index];
            let Some(bound) = loop_bound(function, loop_ref, &def_block) else {
                continue;
            };
            if !slot_is_private_int_array(function, bound.slot)
                || !induction_is_nonnegative(function, loop_ref, bound)
            {
                continue;
            }
            let candidates = loop_read_candidates(function, loop_ref, bound);
            for (read, result, casts) in candidates {
                if rewrite_read(function, read, result) {
                    retire_narrowing_casts(function, result, &casts);
                    changed = true;
                }
            }
        }
        changed
    }
}

/// One loop's proven count bound and the induction value it constrains.
#[derive(Clone, Copy)]
pub(super) struct LoopBound {
    /// Typed array/hash local read by the condition's `count()`.
    pub(super) slot: LocalSlotId,
    /// Header block parameter compared against the count.
    pub(super) param: ValueId,
    /// Hoisted `count()` result used by the header comparison.
    pub(super) count: ValueId,
}

/// Recognizes `header: cond_br (i < count(load slot))` inside the loop.
pub(super) fn loop_bound(
    function: &Function,
    loop_ref: &NaturalLoop,
    def_block: &HashMap<ValueId, BlockId>,
) -> Option<LoopBound> {
    let header = function.block(loop_ref.header)?;
    let Terminator::CondBr { cond, then_target, .. } = header.terminator.as_ref()? else {
        return None;
    };
    if !loop_ref.contains(*then_target) {
        return None;
    }
    let comparison = super::rewrite::defining_instruction(function, *cond)?;
    if !matches!(comparison.op, Op::ICmp | Op::PhpRelCmp)
        || !matches!(
            comparison.immediate,
            Some(Immediate::CmpPredicate(CmpPredicate::Slt))
        )
    {
        return None;
    }
    let param = *comparison.operands.first()?;
    let count_value = *comparison.operands.get(1)?;
    if !header.params.contains(&param)
        || function.value(param)?.ir_type != IrType::I64
        || function.value(param)?.php_type != PhpType::Int
    {
        return None;
    }
    let count = super::rewrite::defining_instruction(function, count_value)?;
    if !is_single_argument_count(count) || count.origin != Some(PassOrigin::CountHoist) {
        return None;
    }
    let load_id = defining_id(function, *count.operands.first()?)?;
    let load = function.instruction(load_id)?;
    if load.op != Op::LoadLocal {
        return None;
    }
    let Some(Immediate::LocalSlot(slot)) = load.immediate else {
        return None;
    };
    let Some(preheader) = loop_ref.preheader else {
        return None;
    };
    if !matches!(
        def_block.get(&count_value),
        Some(&block) if block == preheader || block == loop_ref.header
    ) {
        return None;
    }
    if !slot_is_loop_invariant(function, loop_ref, slot) {
        return None;
    }
    Some(LoopBound { slot, param, count: count_value })
}

/// Requires a non-negative seed and one positive constant step on the header parameter.
fn induction_is_nonnegative(function: &Function, loop_ref: &NaturalLoop, bound: LoopBound) -> bool {
    let Some(header) = function.block(loop_ref.header) else {
        return false;
    };
    let Some(position) = header.params.iter().position(|&param| param == bound.param) else {
        return false;
    };
    let Some(preheader) = loop_ref.preheader.and_then(|id| function.block(id)) else {
        return false;
    };
    let Some(Terminator::Br { target, args }) = preheader.terminator.as_ref() else {
        return false;
    };
    if *target != loop_ref.header
        || !args.get(position).is_some_and(|&seed| nonnegative_const(function, seed))
        || loop_ref.latches.len() != 1
    {
        return false;
    }
    let Some(latch) = function.block(loop_ref.latches[0]) else {
        return false;
    };
    let Some(Terminator::Br { target, args }) = latch.terminator.as_ref() else {
        return false;
    };
    if *target != loop_ref.header {
        return false;
    }
    let Some(next) = args.get(position).and_then(|&value| defining_id(function, value)) else {
        return false;
    };
    let Some(step) = function.instruction(next) else {
        return false;
    };
    match step.op {
        Op::IAdd | Op::ICheckedAdd => {
            let Some(&left) = step.operands.first() else {
                return false;
            };
            let Some(&right) = step.operands.get(1) else {
                return false;
            };
            let step_value = if left == bound.param {
                right
            } else if right == bound.param {
                left
            } else {
                return false;
            };
            positive_const(function, step_value)
        }
        Op::StrIncDec => {
            step.operands.first() == Some(&bound.param)
                && matches!(step.immediate, Some(Immediate::I64(1)))
        }
        _ => false,
    }
}

/// Returns whether `value` is an integer constant greater than or equal to zero.
fn nonnegative_const(function: &Function, value: ValueId) -> bool {
    matches!(constant_i64(function, value), Some(value) if value >= 0)
}

/// Returns whether `value` is an integer constant greater than zero.
fn positive_const(function: &Function, value: ValueId) -> bool {
    matches!(constant_i64(function, value), Some(value) if value > 0)
}

/// Reads one integer constant, if the value is defined by `const_i64`.
fn constant_i64(function: &Function, value: ValueId) -> Option<i64> {
    let inst = super::rewrite::defining_instruction(function, value)?;
    if inst.op != Op::ConstI64 {
        return None;
    }
    match inst.immediate {
        Some(Immediate::I64(value)) => Some(value),
        _ => None,
    }
}

/// Returns the reads in this loop that use the bound induction value on the bound slot.
fn loop_read_candidates(
    function: &Function,
    loop_ref: &NaturalLoop,
    bound: LoopBound,
) -> Vec<(InstId, ValueId, Vec<InstId>)> {
    let mut candidates = Vec::new();
    for &block_id in &loop_ref.blocks {
        if block_id == loop_ref.header {
            continue;
        }
        let Some(block) = function.block(block_id) else {
            continue;
        };
        for &id in &block.instructions {
            let Some(inst) = function.instruction(id) else {
                continue;
            };
            if inst.op != Op::ArrayGet
                || inst.operands.len() != 2
                || inst.operands[1] != bound.param
            {
                continue;
            }
            let Some(load_id) = defining_id(function, inst.operands[0]) else {
                continue;
            };
            let Some(load) = function.instruction(load_id) else {
                continue;
            };
            if load.op != Op::LoadLocal
                || load.immediate != Some(Immediate::LocalSlot(bound.slot))
            {
                continue;
            }
            let Some(operand) = function.value(inst.operands[0]) else {
                continue;
            };
            if !matches!(
                operand.php_type.codegen_repr(),
                PhpType::Array(element) if element.codegen_repr() == PhpType::Int
            ) {
                continue;
            }
            let Some(result) = inst.result else {
                continue;
            };
            let Some(casts) = int_cast_uses(function, result) else {
                continue;
            };
            candidates.push((id, result, casts));
        }
    }
    candidates
}

/// Returns the int-narrowing casts that consume `value`, or `None` for any other use.
fn int_cast_uses(function: &Function, value: ValueId) -> Option<Vec<InstId>> {
    let mut casts = Vec::new();
    for (index, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&value) {
            continue;
        }
        let cast = inst.op == Op::Cast
            && inst.operands.as_slice() == [value]
            && inst.result_php_type.codegen_repr() == PhpType::Int
            && matches!(inst.immediate, Some(Immediate::CastTarget(IrType::I64)));
        if !cast {
            return None;
        }
        casts.push(InstId::from_raw(index as u32));
    }
    Some(casts)
}

/// Replaces one `array_get` with the unchecked op and retypes its result to raw `Int`.
fn rewrite_read(function: &mut Function, read: InstId, result: ValueId) -> bool {
    let Some(inst) = function.instruction_mut(read) else {
        return false;
    };
    inst.op = Op::ArrayGetUnchecked;
    inst.result_type = IrType::I64;
    inst.result_php_type = PhpType::Int;
    inst.result_ownership = Ownership::NonHeap;
    inst.effects = Op::ArrayGetUnchecked.default_effects();
    inst.origin = Some(PassOrigin::ArrayGetBounds);
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    true
}

/// Redirects the now-redundant int casts to the raw read and neutralizes them.
fn retire_narrowing_casts(function: &mut Function, result: ValueId, casts: &[InstId]) {
    let mut replacements = HashMap::new();
    for &cast in casts {
        if let Some(cast_result) = function.instruction(cast).and_then(|inst| inst.result) {
            replacements.insert(cast_result, result);
        }
    }
    replace_all_uses(function, &replacements);
    for &cast in casts {
        if let Some(inst) = function.instruction_mut(cast) {
            neutralize_to_nop(inst);
        }
    }
}

/// Proves the slot only ever holds a private, non-null, int-shaped packed array.
pub(super) fn slot_is_private_int_array(function: &Function, slot: LocalSlotId) -> bool {
    let Some(local) = function.locals.get(slot.as_raw() as usize) else {
        return false;
    };
    if local.kind != LocalKind::PhpLocal
        || local.ir_type != IrType::Heap(IrHeapKind::Array)
        || (slot.as_raw() as usize) < function.params.len()
    {
        return false;
    }
    let element_ok = matches!(
        local.php_type.codegen_repr(),
        PhpType::Array(element)
            if matches!(
                element.codegen_repr(),
                PhpType::Int | PhpType::Never | PhpType::Void
            )
    );
    if !element_ok {
        return false;
    }
    let mut loads = HashSet::new();
    for block in &function.blocks {
        for &id in &block.instructions {
            let Some(inst) = function.instruction(id) else {
                return false;
            };
            if inst.immediate != Some(Immediate::LocalSlot(slot)) {
                continue;
            }
            match inst.op {
                Op::LoadLocal => {
                    let Some(result) = inst.result else {
                        return false;
                    };
                    loads.insert(result);
                }
                Op::StoreLocal => {
                    let Some(&value) = inst.operands.first() else {
                        return false;
                    };
                    if !store_keeps_int_array(function, value, slot) {
                        return false;
                    }
                }
                Op::ReleaseLocalSlot => {}
                _ => return false,
            }
        }
    }
    if loads.is_empty() {
        return false;
    }
    for block in &function.blocks {
        for &id in &block.instructions {
            let Some(inst) = function.instruction(id) else {
                return false;
            };
            for &operand in &inst.operands {
                if loads.contains(&operand) && !read_only_int_array_consumer(function, inst, operand) {
                    return false;
                }
            }
        }
        // A load forwarded as a block/branch/return value is an alias outside this proof.
        if block
            .terminator
            .as_ref()
            .is_some_and(|term| term.used_values().iter().any(|value| loads.contains(value)))
        {
            return false;
        }
    }
    true
}

/// Accepts the initial empty-array store and writebacks of the same slot's load.
///
/// Lowering stores an `acquire` of the freshly created array and releases the producer, so
/// transparent ownership wrappers are followed back to their source before classifying it.
fn store_keeps_int_array(function: &Function, value: ValueId, slot: LocalSlotId) -> bool {
    let mut current = value;
    for _ in 0..8 {
        let Some(def) = super::rewrite::defining_instruction(function, current) else {
            return false;
        };
        match def.op {
            Op::Acquire | Op::Borrow | Op::Move | Op::EnsureOwned => {
                let Some(&source) = def.operands.first() else {
                    return false;
                };
                current = source;
            }
            Op::ArrayNew => return array_is_int_or_empty(&def.result_php_type),
            Op::LoadLocal => {
                return def.immediate == Some(Immediate::LocalSlot(slot))
                    && array_is_int_or_empty(&def.result_php_type);
            }
            _ => return false,
        }
    }
    false
}

/// Returns whether an array type holds only raw int slots or is still empty.
fn array_is_int_or_empty(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(element)
            if matches!(
                element.codegen_repr(),
                PhpType::Int | PhpType::Never | PhpType::Void
            )
    )
}

/// Allows only length/count/element reads and raw-int appends to consume a load.
fn read_only_int_array_consumer(
    function: &Function,
    inst: &Instruction,
    value: ValueId,
) -> bool {
    match inst.op {
        Op::RuntimeCall if is_single_argument_count(inst) => {
            inst.operands.first() == Some(&value)
        }
        Op::ArrayLen
        | Op::ArrayGet
        | Op::ArrayGetSilent
        | Op::ArrayGetUnchecked
        | Op::ArrayIsset
        | Op::ArrayKeyExists
        | Op::ArraySumInt
        | Op::ArrayPushReserved => inst.operands.first() == Some(&value),
        Op::ArrayPush => {
            inst.operands.first() == Some(&value)
                && inst.operands.get(1).is_some_and(|&pushed| {
                    function
                        .value(pushed)
                        .is_some_and(|pushed| pushed.php_type.codegen_repr() == PhpType::Int)
                })
        }
        _ => false,
    }
}
