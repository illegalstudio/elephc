//! Purpose:
//! Caches a loop-invariant `strlen(json_encode($rows))` length behind a depth-two scalar
//! preheader guard.
//!
//! Called from:
//! - The EIR fixed-point pass driver after `json_encode_len_sink` turned the boxed encoder
//!   into the raw string form.
//!
//! Key details:
//! - The matched loop must execute at least once and the source must be a stable PHP local:
//!   never written or address-escaping in the loop, with exactly one load.
//! - The source must be a packed array whose element type is an associative row. The preheader
//!   guard walks the rows and their values; any object, resource, nested container or unknown
//!   tag returns `-1` and the body keeps the per-iteration encoder, preserving
//!   `JsonSerializable` side effects.
//! - A scalar-only guard returns the exact encoded length computed once; the body's
//!   `str_len` becomes `JsonEncodeLenCached`, which returns that length or runs the raw
//!   encoder on the sentinel path. The cached path never materializes the encoded string.
//! - Exception handlers, generators, fibers, dynamic eval and nested loops stay on the generic
//!   path.

use crate::ir::{
    BasicBlock, BlockId, CmpPredicate, DataPool, Function, Immediate, InstId, Instruction,
    IrHeapKind, IrType, LocalKind, LocalSlotId, Op, Ownership, RuntimeCallTarget, RuntimeFnId,
    Terminator, Value, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::json_decode_count_hoist::{bound_is_positive, constant_i64, slot_written_in_loop};
use super::loops::{compute_loops, NaturalLoop};
use super::rewrite::neutralize_to_nop;

/// Caches scalar-only JSON row lengths in the loop preheader.
pub struct JsonEncodeLenCache;

impl IrPass for JsonEncodeLenCache {
    /// Returns the stable pass name used in fixed-point diagnostics.
    fn name(&self) -> &'static str {
        "json_encode_len_cache"
    }

    /// Skips functions with no raw JSON encode call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_raw_encode_call)
    }

    /// Caches every independently proven row-length observation.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if super::cfg::has_exception_handlers(function)
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
        let mut plans = Vec::new();
        for loop_ref in loops.loops() {
            if loop_ref.parent.is_some() {
                continue;
            }
            if let Some(plan) = plan_loop(function, loop_ref, &dominance) {
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

/// Returns whether `inst` is a one-operand raw `json_encode_str` call.
fn is_raw_encode_call(inst: &Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::JsonEncodeStr
            )))
        )
        && inst.operands.len() == 1
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Str
}

/// One loop whose JSON length can be probed in the preheader.
struct Plan {
    preheader: BlockId,
    header: BlockId,
    preheader_args: Vec<ValueId>,
    encode: InstId,
    length: InstId,
    length_result: ValueId,
    source: ValueId,
    source_slot: LocalSlotId,
    source_type: PhpType,
    source_ownership: Ownership,
    releases: Vec<InstId>,
}

/// Recovers one loop-invariant raw JSON length observation.
fn plan_loop(
    function: &Function,
    loop_ref: &NaturalLoop,
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
    let condition = super::rewrite::defining_instruction(function, *cond)?;
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
    let counter_position = header.params.iter().position(|&param| param == counter)?;
    if constant_i64(function, *preheader_args.get(counter_position)?)? != 0 {
        return None;
    }
    if !bound_is_positive(function, bound, loop_ref, preheader) {
        return None;
    }
    if !dominance.dominates(preheader, *else_target) {
        return None;
    }
    let encode = raw_encode_call_in_body(function, body)?;
    let encode_inst = function.instruction(encode)?;
    let Some(encode_result) = encode_inst.result else {
        return None;
    };
    let source = *encode_inst.operands.first()?;
    let source_value = function.value(source)?;
    let source_type = source_value.php_type.clone();
    let source_ownership = source_value.ownership;
    if function.value_use_counts().get(&source).copied().unwrap_or(0) != 1 {
        return None;
    }
    let ValueDef::Instruction {
        inst: source_load, ..
    } = source_value.def
    else {
        return None;
    };
    let source_load_inst = function.instruction(source_load)?;
    if source_load_inst.op != Op::LoadLocal {
        return None;
    }
    let Some(Immediate::LocalSlot(source_slot)) = source_load_inst.immediate else {
        return None;
    };
    let source_local = function.locals.get(source_slot.as_raw() as usize)?;
    if source_local.kind != LocalKind::PhpLocal
        || (source_slot.as_raw() as usize) < function.params.len()
        || slot_written_in_loop(function, loop_ref, source_slot)
        || !only_source_load_in_loop(function, loop_ref, source_slot, source_load)
        || !source_slot_is_not_referenced(function, source_slot)
    {
        return None;
    }
    if !matches!(
        source_type.codegen_repr(),
        PhpType::Array(element) if matches!(element.codegen_repr(), PhpType::AssocArray { .. })
    ) {
        return None;
    }
    let mut length = None;
    let mut releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&encode_result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::StrLen if inst.operands.as_slice() == [encode_result] => {
                if length.replace(id).is_some() {
                    return None;
                }
            }
            Op::Release => releases.push(id),
            _ => return None,
        }
    }
    let length = length?;
    let length_result = function.instruction(length)?.result?;
    if function.instruction(length)?.operands.as_slice() != [encode_result] {
        return None;
    }
    if function.blocks.iter().any(|block| {
        block
            .terminator
            .as_ref()
            .is_some_and(|term| term.used_values().contains(&encode_result))
    }) {
        return None;
    }
    if !first_effect_is_encode(function, body, encode)
        || !loop_has_no_other_call(function, loop_ref, encode)
    {
        return None;
    }
    Some(Plan {
        preheader,
        header: loop_ref.header,
        preheader_args: preheader_args.clone(),
        encode,
        length,
        length_result,
        source,
        source_slot,
        source_type,
        source_ownership,
        releases,
    })
}

/// Returns the unique raw encode call in the loop body.
fn raw_encode_call_in_body(function: &Function, body: BlockId) -> Option<InstId> {
    let block = function.block(body)?;
    let mut found = None;
    for &id in &block.instructions {
        let inst = function.instruction(id)?;
        if is_raw_encode_call(inst) {
            if found.replace(id).is_some() {
                return None;
            }
        }
    }
    found
}

/// Requires the loop's only slot mention to be the source load.
fn only_source_load_in_loop(
    function: &Function,
    loop_ref: &NaturalLoop,
    slot: LocalSlotId,
    load: InstId,
) -> bool {
    for &block in &loop_ref.blocks {
        let Some(block) = function.block(block) else {
            return false;
        };
        for &id in &block.instructions {
            let Some(inst) = function.instruction(id) else {
                return false;
            };
            if !super::checked_int_sink::instruction_mentions_slot(inst, slot) {
                continue;
            }
            if id != load || inst.op != Op::LoadLocal {
                return false;
            }
        }
    }
    true
}

/// Rejects any by-reference binding or foreign slot op on the source local.
///
/// Normal construction writes, loads and cleanup are allowed: everything before the inserted
/// probe is reflected in the probed length, and the loop-local proof above already rejected
/// every non-load mention inside the loop. Reference cells, owner pushes and other alias
/// materialization would let the loop mutate the array without loading the source slot, so
/// they keep the per-iteration encoder.
fn source_slot_is_not_referenced(function: &Function, slot: LocalSlotId) -> bool {
    function.instructions.iter().all(|inst| {
        !super::checked_int_sink::instruction_mentions_slot(inst, slot)
            || matches!(
                inst.op,
                Op::LoadLocal
                    | Op::StoreLocal
                    | Op::ReleaseLocalSlot
                    | Op::ZeroLocalSlot
                    | Op::UnsetLocal
            )
    })
}

/// Returns true when every instruction before `encode` is a load, constant, nop or scratch reset.
fn first_effect_is_encode(function: &Function, body: BlockId, encode: InstId) -> bool {
    let Some(block) = function.block(body) else {
        return false;
    };
    for &id in &block.instructions {
        if id == encode {
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
                | Op::ConcatReset
        ) {
            return false;
        }
    }
    false
}

/// Rejects another call in the loop that could change the global JSON error state.
fn loop_has_no_other_call(function: &Function, loop_ref: &NaturalLoop, encode: InstId) -> bool {
    use Op::*;
    for &block in &loop_ref.blocks {
        let Some(block) = function.block(block) else {
            return false;
        };
        for &id in &block.instructions {
            if id == encode {
                continue;
            }
            let Some(inst) = function.instruction(id) else {
                return false;
            };
            if matches!(
                inst.op,
                RuntimeCall
                    | ExternCall
                    | ClosureCall
                    | ExprCall
                    | MethodCall
                    | StaticMethodCall
                    | EvalFunctionCall
                    | EvalFunctionCallArray
                    | CallableDescriptorInvoke
                    | PipeCall
            ) {
                return false;
            }
        }
    }
    true
}

/// Applies the preheader guard, probe blocks and cached body dispatch.
fn apply(function: &mut Function, plan: Plan) -> bool {
    let source_local = plan.source_slot;
    let preheader_load = emit_inst(
        function,
        plan.preheader,
        Op::LoadLocal,
        Vec::new(),
        Some(Immediate::LocalSlot(source_local)),
        Some((IrType::Heap(IrHeapKind::Array), plan.source_type.clone(), plan.source_ownership)),
        Op::LoadLocal.default_effects(),
    );
    let Some(preheader_load) = preheader_load else {
        return false;
    };
    let guard_target = RuntimeFnId::JsonRowsScalarGuard;
    let Some(guard) = emit_inst(
        function,
        plan.preheader,
        Op::RuntimeCall,
        vec![preheader_load],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(guard_target))),
        Some((IrType::I64, PhpType::Bool, Ownership::NonHeap)),
        guard_target.intrinsic_effects(),
    ) else {
        return false;
    };

    let probe = new_block(function, "json_len_probe");
    let fallback = new_block(function, "json_len_fallback");
    let merge = new_block(function, "json_len_merge");
    let merge_len = add_block_param(function, merge, 0, IrType::I64, PhpType::Int, Ownership::NonHeap);

    if let Some(block) = function.block_mut(plan.preheader) {
        block.terminator = Some(Terminator::CondBr {
            cond: guard,
            then_target: probe,
            then_args: Vec::new(),
            else_target: fallback,
            else_args: Vec::new(),
        });
    } else {
        return false;
    }

    let Some(encode) = emit_inst(
        function,
        probe,
        Op::RuntimeCall,
        vec![preheader_load],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::JsonEncodeStr,
        ))),
        Some((IrType::Str, PhpType::Str, Ownership::Owned)),
        RuntimeFnId::JsonEncodeStr.effects(),
    ) else {
        return false;
    };
    let Some(length) = emit_inst(
        function,
        probe,
        Op::StrLen,
        vec![encode],
        None,
        Some((IrType::I64, PhpType::Int, Ownership::NonHeap)),
        Op::StrLen.default_effects(),
    ) else {
        return false;
    };
    let _ = emit_inst(
        function,
        probe,
        Op::Release,
        vec![encode],
        None,
        None,
        Op::Release.default_effects(),
    );
    if let Some(block) = function.block_mut(probe) {
        block.terminator = Some(Terminator::Br {
            target: merge,
            args: vec![length],
        });
    } else {
        return false;
    }

    let Some(minus_one) = emit_inst(
        function,
        fallback,
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(-1)),
        Some((IrType::I64, PhpType::Int, Ownership::NonHeap)),
        Op::ConstI64.default_effects(),
    ) else {
        return false;
    };
    if let Some(block) = function.block_mut(fallback) {
        block.terminator = Some(Terminator::Br {
            target: merge,
            args: vec![minus_one],
        });
    } else {
        return false;
    }

    if let Some(block) = function.block_mut(merge) {
        block.terminator = Some(Terminator::Br {
            target: plan.header,
            args: plan.preheader_args.clone(),
        });
    } else {
        return false;
    }

    let Some(length_inst) = function.instruction_mut(plan.length) else {
        return false;
    };
    length_inst.op = Op::RuntimeCall;
    length_inst.operands = vec![plan.source, merge_len];
    length_inst.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        RuntimeFnId::JsonEncodeLenCached,
    )));
    length_inst.result_type = IrType::I64;
    length_inst.result_php_type = PhpType::Int;
    length_inst.result_ownership = Ownership::NonHeap;
    length_inst.effects = RuntimeFnId::JsonEncodeLenCached.effects();
    if let Some(value) = function.value_mut(plan.length_result) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    if let Some(encode) = function.instruction_mut(plan.encode) {
        neutralize_to_nop(encode);
    }
    for release in plan.releases {
        if let Some(inst) = function.instruction_mut(release) {
            neutralize_to_nop(inst);
        }
    }
    true
}

/// Emits one instruction with an optional fresh result value at the block tail.
fn emit_inst(
    function: &mut Function,
    block: BlockId,
    op: Op,
    operands: Vec<ValueId>,
    immediate: Option<Immediate>,
    result: Option<(IrType, PhpType, Ownership)>,
    effects: crate::ir::Effects,
) -> Option<ValueId> {
    let block_len = function.block(block)?.instructions.len();
    let inst_id = InstId::from_raw(function.instructions.len() as u32);
    let (result_value, result_type, result_php_type, result_ownership) = match result {
        Some((ir_type, php_type, ownership)) => {
            let value = ValueId::from_raw(function.values.len() as u32);
            function.values.push(Value {
                ir_type,
                php_type: php_type.clone(),
                def: ValueDef::Instruction {
                    block,
                    index: block_len as u32,
                    inst: inst_id,
                },
                ownership,
            });
            (Some(value), ir_type, php_type, ownership)
        }
        None => (None, IrType::Void, PhpType::Void, Ownership::NonHeap),
    };
    function.instructions.push(Instruction::new(
        op,
        operands,
        immediate,
        result_value,
        result_type,
        result_php_type,
        result_ownership,
        effects,
        None,
    ));
    function.block_mut(block)?.instructions.push(inst_id);
    result_value
}

/// Creates one unterminated block and returns its identifier.
fn new_block(function: &mut Function, name: &str) -> BlockId {
    let id = BlockId::from_raw(function.blocks.len() as u32);
    function
        .blocks
        .push(BasicBlock::new(id, name.to_string(), Vec::new()));
    id
}

/// Creates one block parameter and returns its value identifier.
fn add_block_param(
    function: &mut Function,
    block: BlockId,
    index: u16,
    ir_type: IrType,
    php_type: PhpType,
    ownership: Ownership,
) -> ValueId {
    let value = ValueId::from_raw(function.values.len() as u32);
    function.values.push(Value {
        ir_type,
        php_type,
        def: ValueDef::BlockParam { block, index },
        ownership,
    });
    if let Some(block) = function.block_mut(block) {
        block.params.push(value);
    }
    value
}
