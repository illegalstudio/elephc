//! Purpose:
//! Hoists a pure fused `count(array_filter($ints, fn($x) => $x % C === 0))` call out of a
//! loop when its packed-int receiver is immutable in the loop.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the array-sum bounds pass.
//!
//! Key details:
//! - The receiver must be a stable PHP local that is neither written, unset nor loaded
//!   through another observation inside the innermost loop, and it must not escape by
//!   reference; value-passed COW copies cannot mutate the source.
//! - The fused kernel is pure (read-only packed scan, literal nonzero divisor), so moving
//!   it to the preheader preserves every observable result and warning.
//! - A mutated or otherwise observed receiver keeps the per-iteration call.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    BlockId, DataPool, Effects, Function, Immediate, InstId, IrType, LocalKind, LocalSlotId, Op,
    Ownership, RuntimeCallTarget, RuntimeFnId, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::by_ref_alias::address_escaping_slots;
use super::checked_int_sink::instruction_mentions_slot;
use super::dominance::compute_dominance;
use super::driver::IrPass;
use super::loops::compute_loops;
use super::rewrite::replace_all_uses;

/// Hoists loop-invariant fused filter counts.
pub struct ArrayFilterCountHoist;

impl IrPass for ArrayFilterCountHoist {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "array_filter_count_hoist"
    }

    /// Skips functions with no fused filter-count call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_filter_count_call)
    }

    /// Hoists every independently proven call.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if super::local_scope::has_dynamic_eval(function)
            || function.flags.is_generator
            || function.flags.is_fiber_wrapper
        {
            return false;
        }
        let escaping = address_escaping_slots(function);
        let dominance = compute_dominance(function);
        let loops = compute_loops(function, &dominance);
        if loops.is_empty() {
            return false;
        }
        let positions = instruction_positions(function);
        let mut plans = Vec::new();
        for (raw, inst) in function.instructions.iter().enumerate() {
            if !is_filter_count_call(inst) {
                continue;
            }
            let candidate = InstId::from_raw(raw as u32);
            if let Some(plan) = plan_call(function, &loops, &positions, &escaping, candidate) {
                plans.push(plan);
            }
        }
        let mut changed = false;
        for plan in plans {
            if apply(function, plan) {
                changed = true;
            }
        }
        changed
    }
}

/// One loop call that can move to its preheader.
struct Plan {
    call: InstId,
    result: ValueId,
    preheader: BlockId,
    slot: LocalSlotId,
    local_ir: IrType,
    local_php: PhpType,
    divisor: i64,
}

/// Returns whether `inst` is a two-operand fused filter-count call returning int.
fn is_filter_count_call(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ArrayFilterCountMod
            )))
        )
        && inst.operands.len() == 2
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Int
}

/// Returns the block position of each instruction.
fn instruction_positions(function: &Function) -> HashMap<InstId, (BlockId, usize)> {
    let mut positions = HashMap::new();
    for block in &function.blocks {
        for (position, &id) in block.instructions.iter().enumerate() {
            positions.insert(id, (block.id, position));
        }
    }
    positions
}

/// Returns the defining instruction and its id when `value` is instruction-defined.
fn defining_instruction_with_id(function: &Function, value: ValueId) -> Option<(InstId, &crate::ir::Instruction)> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some((inst, function.instruction(inst)?))
}

/// Validates one call and returns the preheader insertion plan.
fn plan_call(
    function: &Function,
    loops: &super::loops::LoopInfo,
    positions: &HashMap<InstId, (BlockId, usize)>,
    escaping: &HashSet<LocalSlotId>,
    candidate: InstId,
) -> Option<Plan> {
    let call = function.instruction(candidate)?;
    let source = *call.operands.first()?;
    let divisor = *call.operands.get(1)?;
    let (load_id, load) = defining_instruction_with_id(function, source)?;
    if load.op != Op::LoadLocal {
        return None;
    }
    let Some(Immediate::LocalSlot(slot)) = load.immediate else {
        return None;
    };
    let local = function.locals.get(slot.as_raw() as usize)?;
    if local.kind != LocalKind::PhpLocal
        || escaping.contains(&slot)
        || !matches!(
            local.php_type.codegen_repr(),
            PhpType::Array(element) if element.codegen_repr() == PhpType::Int
        )
    {
        return None;
    }
    let (_, divisor_inst) = defining_instruction_with_id(function, divisor)?;
    if divisor_inst.op != Op::ConstI64 {
        return None;
    }
    let Some(Immediate::I64(divisor)) = divisor_inst.immediate else {
        return None;
    };
    if divisor == 0 {
        return None;
    }
    let (call_block, _) = *positions.get(&candidate)?;
    let loop_ref = loops.innermost_loop(call_block)?;
    let preheader = loop_ref.preheader?;
    let mut source_loads_in_loop = 0;
    for block in &function.blocks {
        if !loop_ref.contains(block.id) {
            continue;
        }
        for &id in &block.instructions {
            let inst = function.instruction(id)?;
            if !instruction_mentions_slot(inst, slot) {
                continue;
            }
            match inst.op {
                Op::LoadLocal => {
                    if id != load_id {
                        return None;
                    }
                    source_loads_in_loop += 1;
                }
                Op::StoreLocal | Op::UnsetLocal | Op::ReleaseLocalSlot => return None,
                _ => return None,
            }
        }
    }
    let load_block = positions.get(&load_id)?.0;
    if loop_ref.contains(load_block) {
        if source_loads_in_loop != 1 {
            return None;
        }
    } else if source_loads_in_loop != 0 {
        return None;
    }
    Some(Plan {
        call: candidate,
        result: call.result?,
        preheader,
        slot,
        local_ir: local.ir_type,
        local_php: local.php_type.clone(),
        divisor,
    })
}

/// Inserts the preheader call and replaces every use of the loop result.
fn apply(function: &mut Function, plan: Plan) -> bool {
    let Some(array) = push_instruction(
        function,
        plan.preheader,
        Op::LoadLocal,
        Vec::new(),
        Some(Immediate::LocalSlot(plan.slot)),
        plan.local_ir,
        plan.local_php.clone(),
        Ownership::for_php_type(&plan.local_php),
        Op::LoadLocal.default_effects(),
    ) else {
        return false;
    };
    let Some(divisor) = push_instruction(
        function,
        plan.preheader,
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(plan.divisor)),
        IrType::I64,
        PhpType::Int,
        Ownership::NonHeap,
        Op::ConstI64.default_effects(),
    ) else {
        return false;
    };
    let Some(result) = push_instruction(
        function,
        plan.preheader,
        Op::RuntimeCall,
        vec![array, divisor],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::ArrayFilterCountMod,
        ))),
        IrType::I64,
        PhpType::Int,
        Ownership::NonHeap,
        RuntimeFnId::ArrayFilterCountMod.intrinsic_effects(),
    ) else {
        return false;
    };
    replace_all_uses(function, &HashMap::from([(plan.result, result)]));
    if let Some(call) = function.instruction_mut(plan.call) {
        call.op = Op::Nop;
        call.operands.clear();
        call.immediate = None;
        call.effects = Op::Nop.default_effects();
    }
    true
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
    effects: Effects,
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
    let inst = crate::ir::Instruction::new(
        op,
        operands,
        immediate,
        result,
        result_type,
        result_php_type,
        ownership,
        effects,
        None,
    );
    let id = InstId::from_raw(function.instructions.len() as u32);
    function.instructions.push(inst);
    function.blocks[block_index].instructions.push(id);
    result
}
