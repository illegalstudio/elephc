//! Purpose:
//! Replaces a local assigned once from `mb_chr()` and observed only through `strlen()` or
//! `ord($local[0])` with allocation-free integer observation calls.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the other string length sinks.
//!
//! Key details:
//! - The call result may be observed only by the Mixed clone/acquire/store scaffolding that
//!   binds it to one local slot, plus releases of those temporaries.
//! - Every load of that slot must feed exactly one length chain (`Cast` to `Str` then `StrLen`)
//!   or one first-byte chain (`$local[0]` with a constant zero index and warning flag, a `Cast`
//!   to `Str`, then `ord`). Any value observation keeps the boxed `mb_chr()` result.
//! - Length-only locals retarget the call to `MbChrLen`; first-byte-only locals use
//!   `MbChrFirst`; a mixed shape uses `MbChrPacked`, whose `(length << 8) | first_byte` result
//!   lets each terminal observation become a shift or mask without re-evaluating the arguments.
//! - Retargeting preserves the original codepoint and encoding operands, so argument evaluation
//!   order and coercion stay at the assignment site. The observation calls own the explicit-UTF-8
//!   fast path and the bridge fallback for other encodings.
//! - The slot must have exactly one store. Scope cleanup for the now-unused slot is retired with
//!   the rest of the scaffolding.

use std::collections::HashMap;

use crate::ir::{
    BlockId, DataPool, Effects, Function, Immediate, InstId, Instruction, IrType, LocalSlotId, Op,
    Ownership, RuntimeCallTarget, RuntimeFnId, Value, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::dominance::{compute_dominance, DominanceInfo};
use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};
use super::string_len_sink::const_string_value;

/// Rewrites scalar observations of a local `mb_chr()` result.
pub struct MbChrLenLocalSink;

impl IrPass for MbChrLenLocalSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "mb_chr_len_local_sink"
    }

    /// Skips functions with no boxed `mb_chr()` runtime call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_boxed_mb_chr)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let dominance = compute_dominance(function);
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_boxed_mb_chr(inst))
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if specialize(function, candidate, &dominance, data) {
                changed = true;
            }
        }
        changed
    }
}

/// Returns whether `inst` is an explicit two-operand `mb_chr()` producing `string|false`.
fn is_boxed_mb_chr(inst: &crate::ir::Instruction) -> bool {
    matches!(target(inst), Some(RuntimeFnId::MbChr))
        && inst.op == Op::RuntimeCall
        && inst.operands.len() == 2
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Mixed
}

/// Returns the stable runtime target of a direct or profiled call instruction.
fn target(inst: &crate::ir::Instruction) -> Option<RuntimeFnId> {
    match &inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
            | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(*target),
        _ => None,
    }
}

/// One storage-neutral observation of the bound `mb_chr()` local.
enum Observation {
    /// `strlen($local)`.
    Length(LengthObservation),
    /// `ord($local[0])`.
    FirstByte(FirstByteObservation),
}

impl Observation {
    /// Returns the value produced by the terminal observation instruction.
    fn result(&self) -> ValueId {
        match self {
            Observation::Length(observation) => observation.result,
            Observation::FirstByte(observation) => observation.result,
        }
    }
}

/// One `strlen($local)` observation and the scaffolding that feeds it.
struct LengthObservation {
    load: InstId,
    cast: InstId,
    length: InstId,
    result: ValueId,
    neutral: Vec<InstId>,
}

/// One `ord($local[0])` observation and the scaffolding that feeds it.
struct FirstByteObservation {
    load: InstId,
    chars: InstId,
    cast: InstId,
    ord: InstId,
    result: ValueId,
    neutral: Vec<InstId>,
}

/// Specializes one call after proving the complete local use graph.
fn specialize(
    function: &mut Function,
    candidate: InstId,
    dominance: &DominanceInfo,
    data: &DataPool,
) -> bool {
    let Some(call) = function.instruction(candidate) else {
        return false;
    };
    let Some(result) = call.result else {
        return false;
    };
    let Some(call_block) = instruction_block(function, result) else {
        return false;
    };
    let Some((store, slot, mut neutral)) = trace_store(function, result) else {
        return false;
    };
    let Some((observations, cleanup)) = analyze_slot(function, slot, store) else {
        return false;
    };
    if observations.is_empty()
        || observations.iter().any(|observation| {
            instruction_block(function, observation.result())
                .is_none_or(|block| !dominance.dominates(call_block, block))
        })
    {
        return false;
    }

    let has_first_byte = observations
        .iter()
        .any(|observation| matches!(observation, Observation::FirstByte(_)));
    let has_length = observations
        .iter()
        .any(|observation| matches!(observation, Observation::Length(_)));
    let sink = match (has_first_byte, has_length) {
        (true, true) => RuntimeFnId::MbChrPacked,
        (true, false) => RuntimeFnId::MbChrFirst,
        (false, true) => RuntimeFnId::MbChrLen,
        (false, false) => return false,
    };
    let observation_effects = observation_effects(function, data, result, sink);
    let guarded_encoding = function
        .instruction(candidate)
        .and_then(|inst| inst.operands.get(1).copied());

    let Some(call) = function.instruction_mut(candidate) else {
        return false;
    };
    call.immediate = Some(Immediate::RuntimeCall(match &call.immediate {
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
            arguments,
            strict_php,
            strict_types,
            ..
        })) => RuntimeCallTarget::ProfiledFunction {
            target: sink,
            arguments: *arguments,
            strict_php: *strict_php,
            strict_types: *strict_types,
        },
        _ => RuntimeCallTarget::Function(sink),
    }));
    call.result_type = IrType::I64;
    call.result_php_type = PhpType::Int;
    call.result_ownership = Ownership::NonHeap;
    call.effects = observation_effects;
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }

    if sink == RuntimeFnId::MbChrPacked {
        // One packed observation feeds every consumer. Each terminal instruction becomes the
        // tiny extraction of its half, so no second call re-evaluates the argument values (which
        // the original call already consumed and the surrounding cleanup releases).
        for observation in &observations {
            match observation {
                Observation::Length(observation) => {
                    let Some(shift) = insert_const_i64_before(function, observation.length, 8)
                    else {
                        return false;
                    };
                    rewrite_terminal(
                        function,
                        observation.length,
                        observation.result,
                        Op::IShrA,
                        result,
                        shift,
                    );
                    neutral.extend(observation.neutral.iter().copied());
                    neutral.push(observation.load);
                    neutral.push(observation.cast);
                }
                Observation::FirstByte(observation) => {
                    let Some(mask) = insert_const_i64_before(function, observation.ord, 0xFF)
                    else {
                        return false;
                    };
                    rewrite_terminal(
                        function,
                        observation.ord,
                        observation.result,
                        Op::IBitAnd,
                        result,
                        mask,
                    );
                    neutral.extend(observation.neutral.iter().copied());
                    neutral.push(observation.load);
                    neutral.push(observation.chars);
                    neutral.push(observation.cast);
                }
            }
        }
    } else {
        let replacements: HashMap<ValueId, ValueId> = observations
            .iter()
            .filter_map(|observation| match (sink, observation) {
                (RuntimeFnId::MbChrLen, Observation::Length(observation)) => {
                    Some((observation.result, result))
                }
                (RuntimeFnId::MbChrFirst, Observation::FirstByte(observation)) => {
                    Some((observation.result, result))
                }
                _ => None,
            })
            .collect();
        replace_all_uses(function, &replacements);

        for observation in &observations {
            match observation {
                Observation::Length(observation) => {
                    neutral.extend(observation.neutral.iter().copied());
                    neutral.push(observation.load);
                    neutral.push(observation.cast);
                    neutral.push(observation.length);
                }
                Observation::FirstByte(observation) => {
                    neutral.extend(observation.neutral.iter().copied());
                    neutral.push(observation.load);
                    neutral.push(observation.chars);
                    neutral.push(observation.cast);
                    neutral.push(observation.ord);
                }
            }
        }
    }

    if observation_effects.is_pure() {
        if let Some(encoding) = guarded_encoding {
            neutral.extend(guard_scaffold(function, encoding));
        }
    }
    neutral.extend(cleanup);
    neutral.push(store);
    for id in neutral {
        if id == candidate {
            continue;
        }
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    true
}

/// Returns the exception guard/unguard pair protecting one owned encoding argument.
///
/// Lowering wraps a potentially throwing bridge call with these instructions. The fused UTF-8
/// observation is pure, so the guard only adds a global exception-state write per iteration.
/// A guard result with any observer besides its unguard keeps the scaffold.
fn guard_scaffold(function: &Function, encoding: ValueId) -> Vec<InstId> {
    let uses = function.value_use_counts();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if inst.op != Op::RuntimeCall
            || inst.operands.first() != Some(&encoding)
            || !matches!(
                inst.immediate,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned))
            )
        {
            continue;
        }
        let Some(guard_result) = inst.result else {
            return Vec::new();
        };
        if uses.get(&guard_result).copied().unwrap_or(0) != 1 {
            return Vec::new();
        }
        let guard = InstId::from_raw(raw as u32);
        for (raw, unguard) in function.instructions.iter().enumerate() {
            if unguard.op == Op::RuntimeCall
                && unguard.operands.as_slice() == [guard_result]
                && matches!(
                    unguard.immediate,
                    Some(Immediate::RuntimeCall(
                        RuntimeCallTarget::ExceptionUnguardOwned
                    ))
                )
            {
                return vec![guard, InstId::from_raw(raw as u32)];
            }
        }
        return Vec::new();
    }
    Vec::new()
}

/// Returns the precise effects of the fused observation when its arguments prove the fast path.
///
/// A literal UTF-8 encoding and an integer-like codepoint take the allocation-free UTF-8
/// classifier on every target: it only compares and shifts the codepoint, so it cannot warn,
/// throw, allocate, or observe global state. Keeping the conservative target effects in every
/// other case preserves the bridge's diagnostics and lifetime requirements.
fn observation_effects(
    function: &Function,
    data: &DataPool,
    call_result: ValueId,
    sink: RuntimeFnId,
) -> Effects {
    let Some(call) = super::rewrite::defining_instruction(function, call_result) else {
        return sink.intrinsic_effects();
    };
    let Some(encoding) = call.operands.get(1) else {
        return sink.intrinsic_effects();
    };
    let literal_utf8 = const_string_value(function, data, *encoding).is_some_and(|encoding| {
        encoding.eq_ignore_ascii_case("UTF-8") || encoding.eq_ignore_ascii_case("UTF8")
    });
    let integer_like_codepoint = call.operands.first().is_some_and(|&codepoint| {
        function.value(codepoint).is_some_and(|value| {
            matches!(
                value.php_type.codegen_repr(),
                PhpType::Int | PhpType::Bool | PhpType::TaggedScalar
            )
        })
    });
    if literal_utf8 && integer_like_codepoint {
        Effects::PURE
    } else {
        sink.intrinsic_effects()
    }
}

/// Returns the block that defines one instruction result.
fn instruction_block(function: &Function, value: ValueId) -> Option<crate::ir::BlockId> {
    let crate::ir::ValueDef::Instruction { block, .. } = function.value(value)?.def else {
        return None;
    };
    Some(block)
}

/// Traces the single clone/acquire/store chain from the boxed call result to its local slot.
fn trace_store(
    function: &Function,
    result: ValueId,
) -> Option<(InstId, LocalSlotId, Vec<InstId>)> {
    let (clone, mut neutral) = single_consumer(function, result, Op::MixedClone)?;
    let clone_result = function.instruction(clone)?.result?;
    let (acquire, clone_releases) = single_consumer(function, clone_result, Op::Acquire)?;
    neutral.extend(clone_releases);
    let acquire_result = function.instruction(acquire)?.result?;
    let mut store = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&acquire_result) {
            continue;
        }
        if inst.op != Op::StoreLocal || inst.operands.as_slice() != [acquire_result] {
            return None;
        }
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            return None;
        };
        if store.replace((InstId::from_raw(raw as u32), slot)).is_some() {
            return None;
        }
    }
    neutral.push(clone);
    neutral.push(acquire);
    let (store, slot) = store?;
    Some((store, slot, neutral))
}

/// Returns the one consumer with `op`, plus every direct release of the value.
fn single_consumer(
    function: &Function,
    value: ValueId,
    op: Op,
) -> Option<(InstId, Vec<InstId>)> {
    let mut found = None;
    let mut releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&value) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if inst.op == op && inst.operands.as_slice() == [value] {
            if found.replace(id).is_some() {
                return None;
            }
        } else if inst.op == Op::Release && inst.operands.as_slice() == [value] {
            releases.push(id);
        } else {
            return None;
        }
    }
    Some((found?, releases))
}

/// Proves every other slot reference is a length or first-byte observation.
fn analyze_slot(
    function: &Function,
    slot: LocalSlotId,
    store: InstId,
) -> Option<(Vec<Observation>, Vec<InstId>)> {
    let mut observations = Vec::new();
    let mut cleanup = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !matches!(inst.immediate, Some(Immediate::LocalSlot(found)) if found == slot) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id == store {
            continue;
        }
        match inst.op {
            Op::LoadLocal if inst.operands.is_empty() => {
                observations.push(analyze_load(function, id)?);
            }
            Op::ReleaseLocalSlot => cleanup.push(id),
            _ => return None,
        }
    }
    Some((observations, cleanup))
}

/// Classifies the single observation chain fed by one slot load.
fn analyze_load(function: &Function, load: InstId) -> Option<Observation> {
    let load_result = function.instruction(load)?.result?;
    let mut head = None;
    let mut neutral = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&load_result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::Cast
                if inst.operands.as_slice() == [load_result]
                    && inst.result.is_some()
                    && inst.result_php_type.codegen_repr() == PhpType::Str =>
            {
                if head.replace((id, ObservationKind::Length)).is_some() {
                    return None;
                }
            }
            Op::RuntimeCall
                if inst.immediate.is_none()
                    && inst.operands.len() == 3
                    && inst.operands[0] == load_result
                    && is_const_i64(function, inst.operands[1], 0)
                    && is_const_bool(function, inst.operands[2], true)
                    && inst.result.is_some()
                    && inst.result_php_type.codegen_repr() == PhpType::Mixed =>
            {
                if head.replace((id, ObservationKind::FirstByte)).is_some() {
                    return None;
                }
            }
            Op::Release if inst.operands.as_slice() == [load_result] => neutral.push(id),
            _ => return None,
        }
    }
    let (head, kind) = head?;
    match kind {
        ObservationKind::Length => analyze_length(function, load, head, neutral),
        ObservationKind::FirstByte => analyze_first_byte(function, load, head, neutral),
    }
}

/// Which terminal chain starts at one slot load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObservationKind {
    Length,
    FirstByte,
}

/// Returns the single cast and length observation fed by one slot load.
fn analyze_length(
    function: &Function,
    load: InstId,
    cast: InstId,
    mut neutral: Vec<InstId>,
) -> Option<Observation> {
    let cast_result = function.instruction(cast)?.result?;
    let mut length = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&cast_result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::StrLen if inst.operands.as_slice() == [cast_result] && inst.result.is_some() => {
                if length.replace((id, inst.result?)).is_some() {
                    return None;
                }
            }
            Op::Release if inst.operands.as_slice() == [cast_result] => neutral.push(id),
            _ => return None,
        }
    }
    let (length, result) = length?;
    Some(Observation::Length(LengthObservation {
        load,
        cast,
        length,
        result,
        neutral,
    }))
}

/// Returns the single `$local[0]` cast and `ord` observation fed by one slot load.
fn analyze_first_byte(
    function: &Function,
    load: InstId,
    chars: InstId,
    mut neutral: Vec<InstId>,
) -> Option<Observation> {
    let chars_result = function.instruction(chars)?.result?;
    let mut cast = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&chars_result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::Cast
                if inst.operands.as_slice() == [chars_result]
                    && inst.result.is_some()
                    && inst.result_php_type.codegen_repr() == PhpType::Str =>
            {
                if cast.replace(id).is_some() {
                    return None;
                }
            }
            Op::Release if inst.operands.as_slice() == [chars_result] => neutral.push(id),
            _ => return None,
        }
    }
    let cast = cast?;
    let cast_result = function.instruction(cast)?.result?;
    let mut ord = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&cast_result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::RuntimeCall
                if inst.operands.as_slice() == [cast_result]
                    && target(inst) == Some(RuntimeFnId::Ord)
                    && inst.result.is_some() =>
            {
                if ord.replace((id, inst.result?)).is_some() {
                    return None;
                }
            }
            Op::Release if inst.operands.as_slice() == [cast_result] => neutral.push(id),
            _ => return None,
        }
    }
    let (ord, result) = ord?;
    Some(Observation::FirstByte(FirstByteObservation {
        load,
        chars,
        cast,
        ord,
        result,
        neutral,
    }))
}

/// Returns whether `value` is defined by a `const_i64` of `expected`.
fn is_const_i64(function: &Function, value: ValueId, expected: i64) -> bool {
    let Some(ValueDef::Instruction { inst, .. }) = function.value(value).map(|value| value.def)
    else {
        return false;
    };
    function
        .instruction(inst)
        .is_some_and(|inst| inst.op == Op::ConstI64 && inst.immediate == Some(Immediate::I64(expected)))
}

/// Returns whether `value` is defined by a `const_bool` of `expected`.
fn is_const_bool(function: &Function, value: ValueId, expected: bool) -> bool {
    let Some(ValueDef::Instruction { inst, .. }) = function.value(value).map(|value| value.def)
    else {
        return false;
    };
    function.instruction(inst).is_some_and(|inst| {
        inst.op == Op::ConstBool && inst.immediate == Some(Immediate::Bool(expected))
    })
}

/// Inserts a fresh `const_i64` immediately before `before` and returns its value.
fn insert_const_i64_before(
    function: &mut Function,
    before: InstId,
    value: i64,
) -> Option<ValueId> {
    let (block, position) = instruction_position(function, before)?;
    let inst_id = InstId::from_raw(function.instructions.len() as u32);
    let result = ValueId::from_raw(function.values.len() as u32);
    function.values.push(Value {
        ir_type: IrType::I64,
        php_type: PhpType::Int,
        def: ValueDef::Instruction {
            block,
            index: position as u32,
            inst: inst_id,
        },
        ownership: Ownership::NonHeap,
    });
    function.instructions.push(Instruction::new(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(value)),
        Some(result),
        IrType::I64,
        PhpType::Int,
        Ownership::NonHeap,
        Op::ConstI64.default_effects(),
        None,
    ));
    function.blocks[block.as_raw() as usize]
        .instructions
        .insert(position, inst_id);
    let shifted: Vec<InstId> = function.blocks[block.as_raw() as usize].instructions[position + 1..]
        .to_vec();
    for (offset, shifted_id) in shifted.into_iter().enumerate() {
        if let Some(value) = function.instructions[shifted_id.as_raw() as usize].result {
            function.values[value.as_raw() as usize].def = ValueDef::Instruction {
                block,
                index: (position + 1 + offset) as u32,
                inst: shifted_id,
            };
        }
    }
    Some(result)
}

/// Returns the block and instruction-slot position that define `id`'s result.
fn instruction_position(function: &Function, id: InstId) -> Option<(BlockId, usize)> {
    let result = function.instruction(id)?.result?;
    match function.value(result)?.def {
        ValueDef::Instruction { block, index, .. } => Some((block, index as usize)),
        _ => None,
    }
}

/// Rewrites one terminal observation into an integer extraction of the packed result.
fn rewrite_terminal(
    function: &mut Function,
    terminal: InstId,
    result: ValueId,
    op: Op,
    packed: ValueId,
    operand: ValueId,
) {
    if let Some(inst) = function.instruction_mut(terminal) {
        inst.op = op;
        inst.operands = vec![packed, operand];
        inst.immediate = None;
        inst.effects = op.default_effects();
        inst.result_type = IrType::I64;
        inst.result_php_type = PhpType::Int;
        inst.result_ownership = Ownership::NonHeap;
    }
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
}
