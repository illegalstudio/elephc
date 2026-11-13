//! Purpose:
//! Hoists a loop-invariant `count(json_decode($s, true))` probe into the loop preheader.
//!
//! Called from:
//! - The EIR fixed-point driver after the JSON count sink and loop scalarization.
//!
//! Key details:
//! - The matched loop must execute at least once (an `argc + positive constant` runtime bound
//!   or a positive constant bound) and the decode must be the first observable effect in the
//!   body, so running the probe in the preheader has the same side effects as the first
//!   iteration.
//! - The source must be a stable PHP local that is neither written nor address-escaping in the
//!   loop. A body `load_local` is duplicated in the preheader; a value defined outside the loop
//!   is reused directly.
//! - The body call result is replaced by the preheader probe result and the body call is
//!   neutralized. The probe uses the existing count lowering, so invalid JSON, scalar roots and
//!   numeric/escaped-key fallbacks keep their original errors and fallback behaviour.
//! - Exception handlers, generators, fibers, dynamic eval, nested loops and any earlier body
//!   effect reject the rewrite.

use std::collections::HashMap;

use crate::ir::{
    BlockId, CmpPredicate, DataPool, Function, Immediate, InstId, Instruction, IrType, LocalKind,
    LocalSlotId, Op, Ownership, PassOrigin, RuntimeCallTarget, RuntimeFnId, Terminator, ValueDef,
    ValueId,
};
use crate::types::PhpType;

use super::by_ref_alias::address_escaping_slots;
use super::cfg::has_exception_handlers;
use super::dominance::compute_dominance;
use super::driver::IrPass;
use super::integer_range::main_argc_slot;
use super::loops::{compute_loops, NaturalLoop};
use super::rewrite::{defining_instruction, neutralize_to_nop, replace_all_uses};

/// Hoists loop-invariant count-only JSON decodes into their preheader.
pub struct JsonDecodeCountHoist;

impl IrPass for JsonDecodeCountHoist {
    /// Names the pass in fixed-point validation errors.
    fn name(&self) -> &'static str {
        "json_decode_count_hoist"
    }

    /// Hoists every loop whose count-only decode is provably loop-invariant.
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
        let escaping = address_escaping_slots(function);
        let dominance = compute_dominance(function);
        let loops = compute_loops(function, &dominance);
        if loops.is_empty() {
            return false;
        }
        let mut changed = false;
        for loop_ref in loops.loops() {
            if loop_ref.parent.is_some() {
                continue;
            }
            let Some(plan) = plan_loop(function, loop_ref, &escaping, &dominance) else {
                continue;
            };
            if apply(function, plan) {
                changed = true;
            }
        }
        changed
    }
}

/// The proven probe/cached-call rewrite for one loop.
struct Plan {
    preheader: BlockId,
    call: InstId,
    source: ValueId,
    assoc: ValueId,
    source_load: Option<SourceLoad>,
}

/// A body `load_local` that must be duplicated in the preheader.
struct SourceLoad {
    slot: LocalSlotId,
    ir_type: IrType,
    php_type: PhpType,
    ownership: Ownership,
}

/// Recovers one loop-invariant JSON count decode from a two-block natural loop.
fn plan_loop(
    function: &Function,
    loop_ref: &NaturalLoop,
    escaping: &std::collections::HashSet<LocalSlotId>,
    dominance: &super::dominance::DominanceInfo,
) -> Option<Plan> {
    let preheader = loop_ref.preheader?;
    let header = function.block(loop_ref.header)?;
    if header.params.is_empty() || loop_ref.latches.len() != 1 {
        return None;
    }
    let body = loop_ref
        .blocks
        .iter()
        .copied()
        .find(|&block| block != loop_ref.header)?;
    let Some(Terminator::CondBr {
        cond,
        then_target,
        then_args,
        else_target,
        else_args,
    }) = header.terminator.as_ref()
    else {
        return None;
    };
    if *then_target != body
        || !then_args.is_empty()
        || !else_args.is_empty()
        || !function.block(*else_target)?.params.is_empty()
    {
        return None;
    }
    let condition = defining_instruction(function, *cond)?;
    if condition.op != Op::ICmp
        || condition.immediate != Some(Immediate::CmpPredicate(CmpPredicate::Slt))
        || condition.operands.len() != 2
    {
        return None;
    }
    let counter = condition.operands[0];
    let bound = condition.operands[1];
    if !header.params.contains(&counter) {
        return None;
    }
    let Some(Terminator::Br {
        target,
        args: preheader_args,
    }) = function.block(preheader)?.terminator.as_ref()
    else {
        return None;
    };
    if *target != loop_ref.header || preheader_args.len() != header.params.len() {
        return None;
    }
    let counter_position = header
        .params
        .iter()
        .position(|&param| param == counter)?;
    if constant_i64(function, *preheader_args.get(counter_position)?)? != 0 {
        return None;
    }
    if !bound_is_positive(function, bound, loop_ref, preheader) {
        return None;
    }
    if !dominance.dominates(preheader, *else_target) {
        return None;
    }
    let call = json_count_call(function, body)?;
    let call_inst = function.instruction(call)?;
    if call_inst.operands.len() < 2 || call_inst.result.is_none() {
        return None;
    }
    let source = call_inst.operands[0];
    let assoc = call_inst.operands[1];
    let assoc_inst = defining_instruction(function, assoc)?;
    if assoc_inst.op != Op::ConstBool || assoc_inst.immediate != Some(Immediate::Bool(true)) {
        return None;
    }
    if def_block(function, assoc).is_some_and(|block| loop_ref.contains(block)) {
        return None;
    }
    let source_load = match function.value(source)?.def {
        ValueDef::Instruction { inst, .. } => {
            let load = function.instruction(inst)?;
            if load.op != Op::LoadLocal {
                return None;
            }
            let Some(Immediate::LocalSlot(slot)) = load.immediate else {
                return None;
            };
            let local = function.locals.get(slot.as_raw() as usize)?;
            if local.kind != LocalKind::PhpLocal
                || (slot.as_raw() as usize) < function.params.len()
                || escaping.contains(&slot)
                || slot_written_in_loop(function, loop_ref, slot)
            {
                return None;
            }
            Some(SourceLoad {
                slot,
                ir_type: load.result_type,
                php_type: load.result_php_type.clone(),
                ownership: load.result_ownership,
            })
        }
        ValueDef::BlockParam { block, .. } => {
            if loop_ref.contains(block) || !dominance.dominates(block, preheader) {
                return None;
            }
            None
        }
    };
    if !first_effect_is_call(function, body, call) {
        return None;
    }
    Some(Plan {
        preheader,
        call,
        source,
        assoc,
        source_load,
    })
}

/// Returns the unique count-only JSON decode call in one body block.
fn json_count_call(function: &Function, body: BlockId) -> Option<InstId> {
    let block = function.block(body)?;
    let mut found = None;
    for &id in &block.instructions {
        let inst = function.instruction(id)?;
        if inst.op == Op::RuntimeCall
            && matches!(
                inst.immediate,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::JsonDecodeCount
                )))
            )
        {
            if found.replace(id).is_some() {
                return None;
            }
        }
    }
    found
}

/// Returns true when every instruction before `call` is a local load, constant or nop.
fn first_effect_is_call(function: &Function, body: BlockId, call: InstId) -> bool {
    let Some(block) = function.block(body) else {
        return false;
    };
    for &id in &block.instructions {
        if id == call {
            return true;
        }
        let Some(inst) = function.instruction(id) else {
            return false;
        };
        if !matches!(
            inst.op,
            Op::Nop
                | Op::LoadLocal
                | Op::ConstI64
                | Op::ConstF64
                | Op::ConstBool
                | Op::ConstNull
                | Op::ConstStr
        ) {
            return false;
        }
    }
    false
}

/// Rejects a loop whose source local is written, unset or released.
fn slot_written_in_loop(function: &Function, loop_ref: &NaturalLoop, slot: LocalSlotId) -> bool {
    for &block in &loop_ref.blocks {
        let Some(block) = function.block(block) else {
            return true;
        };
        for &id in &block.instructions {
            let Some(inst) = function.instruction(id) else {
                return true;
            };
            if matches!(
                inst.op,
                Op::StoreLocal | Op::UnsetLocal | Op::ReleaseLocalSlot | Op::ZeroLocalSlot
            ) && super::checked_int_sink::instruction_mentions_slot(inst, slot)
            {
                return true;
            }
        }
    }
    false
}

/// Returns true when the loop's `counter < bound` condition holds on entry.
fn bound_is_positive(
    function: &Function,
    bound: ValueId,
    loop_ref: &NaturalLoop,
    preheader: BlockId,
) -> bool {
    if constant_i64(function, bound).is_some_and(|bound| bound > 0) {
        return true;
    }
    let Some(add) = defining_instruction(function, bound) else {
        return false;
    };
    if add.op != Op::IAdd || add.operands.len() != 2 {
        return false;
    }
    for (argc, offset) in [
        (add.operands[0], add.operands[1]),
        (add.operands[1], add.operands[0]),
    ] {
        let Some(offset) = constant_i64(function, offset) else {
            continue;
        };
        if offset <= 0 {
            continue;
        }
        let Some(load) = defining_instruction(function, argc) else {
            continue;
        };
        let Some(Immediate::LocalSlot(slot)) = load.immediate else {
            continue;
        };
        if load.op == Op::LoadLocal
            && main_argc_slot(function, slot)
            && def_block(function, argc)
                .is_some_and(|block| !loop_ref.contains(block) && (block == function.entry || block == preheader))
        {
            return true;
        }
    }
    false
}

/// Returns the block defining `value`, when instruction-defined.
fn def_block(function: &Function, value: ValueId) -> Option<BlockId> {
    match function.value(value)?.def {
        ValueDef::Instruction { block, .. } | ValueDef::BlockParam { block, .. } => Some(block),
    }
}

/// Reads one integer constant value, if the value is defined by `const_i64`.
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

/// Emits the preheader probe and retargets the body call to the cached helper.
fn apply(function: &mut Function, plan: Plan) -> bool {
    let probe_source = if let Some(load) = plan.source_load {
        let Some(value) = push_instruction(
            function,
            plan.preheader,
            Op::LoadLocal,
            Vec::new(),
            Some(Immediate::LocalSlot(load.slot)),
            load.ir_type,
            load.php_type,
            load.ownership,
            Op::LoadLocal.default_effects(),
            PassOrigin::JsonDecodeCountHoist,
        ) else {
            return false;
        };
        value
    } else {
        plan.source
    };
    let Some(probe) = push_instruction(
        function,
        plan.preheader,
        Op::RuntimeCall,
        vec![probe_source, plan.assoc],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::JsonDecodeCount,
        ))),
        IrType::I64,
        PhpType::Int,
        Ownership::NonHeap,
        RuntimeFnId::JsonDecodeCount.intrinsic_effects(),
        PassOrigin::JsonDecodeCountHoist,
    ) else {
        return false;
    };
    let Some(call_result) = function.instruction(plan.call).and_then(|call| call.result) else {
        return false;
    };
    replace_all_uses(
        function,
        &HashMap::from([(call_result, probe)]),
    );
    if let Some(call) = function.instruction_mut(plan.call) {
        neutralize_to_nop(call);
    }
    true
}

/// Appends one instruction to a block and materializes a result value when typed.
#[allow(clippy::too_many_arguments)]
fn push_instruction(
    function: &mut Function,
    block: BlockId,
    op: Op,
    operands: Vec<ValueId>,
    immediate: Option<Immediate>,
    result_type: IrType,
    result_php_type: PhpType,
    ownership: Ownership,
    effects: crate::ir::Effects,
    origin: PassOrigin,
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
        effects,
        None,
    );
    inst.origin = Some(origin);
    let id = InstId::from_raw(function.instructions.len() as u32);
    function.instructions.push(inst);
    function.blocks[block_index].instructions.push(id);
    result
}
