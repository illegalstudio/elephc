//! Purpose:
//! Replaces a `strlen()` of a length-only string producer with its exact length
//! when the produced bytes themselves are never observed.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the integer cast sinks.
//!
//! Key details:
//! - The inner call must receive an already-materialized PHP string, so it cannot throw,
//!   coerce, or invoke user code after its operand has been evaluated.
//! - The inner result may be observed only by one `str_len` and release scaffolding.
//! - `md5()` always returns 32 hex characters; `base64_encode()` returns
//!   `4 * ceil(len / 3)` characters; ASCII case conversion and the three-argument
//!   `strtr()` translation preserve byte length; a constant `str_repeat()` multiplies a
//!   literal source length by its count. The operand evaluation and its side effects
//!   remain.

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget,
    RuntimeFnId, UnaryStringRuntime, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{count_value_uses, defining_instruction, neutralize_to_nop};

/// Largest folded repeat length; larger results keep the real allocation path.
const MAX_FOLDED_REPEAT: usize = 16 * 1024 * 1024;

/// Rewrites length-only hash and base64 calls.
pub struct StringLenSink;

impl IrPass for StringLenSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "string_len_sink"
    }

    /// Skips functions with no length-only string producer call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_length_observable_call)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_length_observable_call(inst))
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if specialize(function, candidate, data) {
                changed = true;
            }
        }
        changed
    }
}

/// Returns whether `inst` is a length-only string producer call.
fn is_length_observable_call(inst: &crate::ir::Instruction) -> bool {
    if inst.op != Op::RuntimeCall
        || inst.result.is_none()
        || inst.result_php_type.codegen_repr() != PhpType::Str
    {
        return false;
    }
    match inst.immediate {
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::Md5,
        )))
        | Some(Immediate::RuntimeCall(RuntimeCallTarget::UnaryString(
            UnaryStringRuntime::Base64Encode
                | UnaryStringRuntime::BinToHex
                | UnaryStringRuntime::StrToLower
                | UnaryStringRuntime::StrToUpper,
        ))) => inst.operands.len() == 1,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::StrRepeat,
        ))) => inst.operands.len() == 2,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::Strtr,
        ))) => inst.operands.len() == 3,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::Ucfirst | RuntimeFnId::Lcfirst | RuntimeFnId::Ucwords,
        ))) => matches!(inst.operands.len(), 1 | 2),
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::MbStrtoupper
                | RuntimeFnId::MbStrtolower
                | RuntimeFnId::MbUcfirst
                | RuntimeFnId::MbLcfirst
                | RuntimeFnId::MbConvertCase,
        )))
        | Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::MbTrim
                | RuntimeFnId::MbLtrim
                | RuntimeFnId::MbRtrim
                | RuntimeFnId::MbStrPad
                | RuntimeFnId::MbSubstr
                | RuntimeFnId::MbStrcut,
        )))
        | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
            target:
                RuntimeFnId::MbStrtoupper
                | RuntimeFnId::MbStrtolower
                | RuntimeFnId::MbUcfirst
                | RuntimeFnId::MbLcfirst
                | RuntimeFnId::MbConvertCase
                | RuntimeFnId::MbTrim
                | RuntimeFnId::MbLtrim
                | RuntimeFnId::MbRtrim
                | RuntimeFnId::MbStrPad
                | RuntimeFnId::MbSubstr
                | RuntimeFnId::MbStrcut,
            ..
        })) => matches!(inst.operands.len(), 2 | 3 | 4 | 5),
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::ChunkSplit,
        ))) => inst.operands.len() == 3,
        _ => false,
    }
}

/// Specializes one candidate after proving its operand and complete use graph.
fn specialize(function: &mut Function, candidate: InstId, data: &DataPool) -> bool {
    let Some(call) = function.instruction(candidate) else {
        return false;
    };
    let Some(result) = call.result else {
        return false;
    };
    let Some(&operand) = call.operands.first() else {
        return false;
    };
    if function
        .value(operand)
        .is_none_or(|value| value.php_type.codegen_repr() != PhpType::Str)
    {
        return false;
    }
    let is_md5 = matches!(
        call.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::Md5
        )))
    );
    let is_bin2hex = matches!(
        call.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::UnaryString(
            UnaryStringRuntime::BinToHex
        )))
    );
    let is_case_conversion = matches!(
        call.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::UnaryString(
            UnaryStringRuntime::StrToLower | UnaryStringRuntime::StrToUpper
        )))
    ) || matches!(
        call.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::Ucfirst | RuntimeFnId::Lcfirst | RuntimeFnId::Ucwords
        )))
    );
    let is_pairwise_strtr = matches!(
        call.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::Strtr
        )))
    ) && call.operands.len() == 3;
    let is_chunk_split = matches!(
        call.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::ChunkSplit
        )))
    ) && call.operands.len() == 3;
    let is_mb_case = matches!(
        call.immediate,
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(
                RuntimeFnId::MbStrtoupper
                    | RuntimeFnId::MbStrtolower
                    | RuntimeFnId::MbUcfirst
                    | RuntimeFnId::MbLcfirst
                    | RuntimeFnId::MbConvertCase
            ) | RuntimeCallTarget::ProfiledFunction {
                target:
                    RuntimeFnId::MbStrtoupper
                    | RuntimeFnId::MbStrtolower
                    | RuntimeFnId::MbUcfirst
                    | RuntimeFnId::MbLcfirst
                    | RuntimeFnId::MbConvertCase,
                ..
            }
        ))
    );
    let mb_case_length = if is_mb_case {
        let Some(encoding_operand) = call.operands.last().copied() else {
            return false;
        };
        let Some(encoding) = const_string_value(function, data, encoding_operand) else {
            return false;
        };
        if !encoding.eq_ignore_ascii_case("UTF-8") {
            return false;
        }
        let Some((subject, _load)) = const_local_string(function, data, operand) else {
            return false;
        };
        if !subject.is_ascii() {
            return false;
        }
        Some(subject.len() as i64)
    } else {
        None
    };
    let chunk_split_operands = is_chunk_split.then(|| call.operands.clone());
    let is_repeat = matches!(
        call.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::StrRepeat
        )))
    );
    let repeat_length = if is_repeat {
        let Some(&count_operand) = call.operands.get(1) else {
            return false;
        };
        let Some(source) = const_str_literal(function, data, operand) else {
            return false;
        };
        let Some(count) = const_i64(function, count_operand) else {
            return false;
        };
        if count < 0 {
            return false;
        }
        let Some(length) = source.len().checked_mul(count as usize) else {
            return false;
        };
        if length > MAX_FOLDED_REPEAT {
            return false;
        }
        Some(length as i64)
    } else {
        None
    };
    let is_mb_trim = matches!(
        call.immediate,
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(
                RuntimeFnId::MbTrim | RuntimeFnId::MbLtrim | RuntimeFnId::MbRtrim
            ) | RuntimeCallTarget::ProfiledFunction {
                target: RuntimeFnId::MbTrim | RuntimeFnId::MbLtrim | RuntimeFnId::MbRtrim,
                ..
            }
        ))
    );
    let mb_trim_length = if is_mb_trim && call.operands.len() == 3 {
        let Some(encoding) = const_string_value(function, data, call.operands[2]) else {
            return false;
        };
        if !encoding.eq_ignore_ascii_case("UTF-8") {
            return false;
        }
        let Some((subject, _)) = const_local_string(function, data, operand) else {
            return false;
        };
        let Some((mask, _)) = const_local_string(function, data, call.operands[1]) else {
            return false;
        };
        if !subject.is_ascii() || !mask.is_ascii() {
            return false;
        }
        let trim_left = matches!(
            call.immediate,
            Some(Immediate::RuntimeCall(
                RuntimeCallTarget::Function(RuntimeFnId::MbTrim | RuntimeFnId::MbLtrim)
                    | RuntimeCallTarget::ProfiledFunction {
                        target: RuntimeFnId::MbTrim | RuntimeFnId::MbLtrim,
                        ..
                    }
            ))
        );
        let trim_right = matches!(
            call.immediate,
            Some(Immediate::RuntimeCall(
                RuntimeCallTarget::Function(RuntimeFnId::MbTrim | RuntimeFnId::MbRtrim)
                    | RuntimeCallTarget::ProfiledFunction {
                        target: RuntimeFnId::MbTrim | RuntimeFnId::MbRtrim,
                        ..
                    }
            ))
        );
        Some(trimmed_ascii_len(subject.as_bytes(), mask.as_bytes(), trim_left, trim_right))
    } else {
        None
    };
    let is_mb_pad = matches!(
        call.immediate,
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(RuntimeFnId::MbStrPad)
                | RuntimeCallTarget::ProfiledFunction {
                    target: RuntimeFnId::MbStrPad,
                    ..
                }
        ))
    ) && call.operands.len() == 5;
    let mb_pad_length = if is_mb_pad {
        let Some(encoding) = const_string_value(function, data, call.operands[4]) else {
            return false;
        };
        if !encoding.eq_ignore_ascii_case("UTF-8") {
            return false;
        }
        let Some((subject, _)) = const_local_string(function, data, operand) else {
            return false;
        };
        let Some((pad, _)) = const_local_string(function, data, call.operands[2]) else {
            return false;
        };
        let Some(width) = const_i64(function, call.operands[1]) else {
            return false;
        };
        if !subject.is_ascii() || !pad.is_ascii() || width < 0 {
            return false;
        }
        let length = if pad.is_empty() {
            subject.len()
        } else {
            subject.len().max(width as usize)
        };
        Some(length as i64)
    } else {
        None
    };
    let is_mb_slice = matches!(
        call.immediate,
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(RuntimeFnId::MbSubstr | RuntimeFnId::MbStrcut)
                | RuntimeCallTarget::ProfiledFunction {
                    target: RuntimeFnId::MbSubstr | RuntimeFnId::MbStrcut,
                    ..
                }
        ))
    ) && call.operands.len() == 4;
    let mb_slice_length = if is_mb_slice {
        let Some(encoding) = const_string_value(function, data, call.operands[3]) else {
            return false;
        };
        if !encoding.eq_ignore_ascii_case("UTF-8") {
            return false;
        }
        let Some((subject, _)) = const_local_string(function, data, operand) else {
            return false;
        };
        let Some(start) = const_i64(function, call.operands[1]) else {
            return false;
        };
        let Some(length) = const_i64(function, call.operands[2]) else {
            return false;
        };
        if !subject.is_ascii() || start < 0 || length < 0 {
            return false;
        }
        let start = start as usize;
        if start > subject.len() {
            return false;
        }
        let available = subject.len() - start;
        Some(available.min(length as usize) as i64)
    } else {
        None
    };
    let mb_length = mb_case_length
        .or(mb_trim_length)
        .or(mb_pad_length)
        .or(mb_slice_length);
    // Any mbstring producer that did not match its exact optional-argument shape must stay
    // on the runtime path; the remaining rewrite arm is reserved for base64 encoding.
    let is_mb_target = matches!(
        call.immediate,
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(
                RuntimeFnId::MbStrtoupper
                    | RuntimeFnId::MbStrtolower
                    | RuntimeFnId::MbUcfirst
                    | RuntimeFnId::MbLcfirst
                    | RuntimeFnId::MbConvertCase
                    | RuntimeFnId::MbTrim
                    | RuntimeFnId::MbLtrim
                    | RuntimeFnId::MbRtrim
                    | RuntimeFnId::MbStrPad
                    | RuntimeFnId::MbSubstr
                    | RuntimeFnId::MbStrcut
            ) | RuntimeCallTarget::ProfiledFunction {
                target:
                    RuntimeFnId::MbStrtoupper
                    | RuntimeFnId::MbStrtolower
                    | RuntimeFnId::MbUcfirst
                    | RuntimeFnId::MbLcfirst
                    | RuntimeFnId::MbConvertCase
                    | RuntimeFnId::MbTrim
                    | RuntimeFnId::MbLtrim
                    | RuntimeFnId::MbRtrim
                    | RuntimeFnId::MbStrPad
                    | RuntimeFnId::MbSubstr
                    | RuntimeFnId::MbStrcut,
                ..
            }
        ))
    );
    if is_mb_target && mb_length.is_none() {
        return false;
    }
    let Some((length, releases)) = analyze_uses(function, result) else {
        return false;
    };

    // Rewrite the length observer in place, then retire the unobserved producer.
    if let Some(length_value) = mb_length {
        let Some(length) = function.instruction_mut(length) else {
            return false;
        };
        length.op = Op::ConstI64;
        length.operands.clear();
        length.immediate = Some(Immediate::I64(length_value));
        length.result_type = IrType::I64;
        length.result_php_type = PhpType::Int;
        length.result_ownership = Ownership::NonHeap;
        length.effects = Op::ConstI64.default_effects();
    } else if is_md5 || is_repeat {
        let Some(length) = function.instruction_mut(length) else {
            return false;
        };
        length.op = Op::ConstI64;
        length.operands.clear();
        length.immediate = Some(Immediate::I64(if is_md5 {
            32
        } else {
            repeat_length.expect("repeat length proven")
        }));
        length.result_type = IrType::I64;
        length.result_php_type = PhpType::Int;
        length.result_ownership = Ownership::NonHeap;
        length.effects = Op::ConstI64.default_effects();
    } else if is_bin2hex {
        let source = detach_operand_pin(function, operand, length, candidate).unwrap_or(operand);
        let Some(length) = function.instruction_mut(length) else {
            return false;
        };
        length.op = Op::RuntimeCall;
        length.operands = vec![source];
        length.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::Bin2HexLen,
        )));
        length.result_type = IrType::I64;
        length.result_php_type = PhpType::Int;
        length.result_ownership = Ownership::NonHeap;
        length.effects = RuntimeFnId::Bin2HexLen.intrinsic_effects();
    } else if is_chunk_split {
        let Some(mut operands) = chunk_split_operands else {
            return false;
        };
        for index in [0usize, 2] {
            if let Some(source) = detach_operand_pin(function, operands[index], length, candidate) {
                operands[index] = source;
            }
        }
        let Some(length) = function.instruction_mut(length) else {
            return false;
        };
        length.op = Op::RuntimeCall;
        length.operands = operands;
        length.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::ChunkSplitLen,
        )));
        length.result_type = IrType::I64;
        length.result_php_type = PhpType::Int;
        length.result_ownership = Ownership::NonHeap;
        length.effects = RuntimeFnId::ChunkSplitLen.intrinsic_effects();
    } else if is_case_conversion || is_pairwise_strtr {
        let source = detach_operand_pin(function, operand, length, candidate).unwrap_or(operand);
        let Some(length) = function.instruction_mut(length) else {
            return false;
        };
        length.op = Op::StrLen;
        length.operands = vec![source];
        length.immediate = None;
        length.result_type = IrType::I64;
        length.result_php_type = PhpType::Int;
        length.result_ownership = Ownership::NonHeap;
        length.effects = Op::StrLen.default_effects();
    } else {
        let Some(length) = function.instruction_mut(length) else {
            return false;
        };
        length.op = Op::RuntimeCall;
        length.operands = vec![operand];
        length.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::Base64EncodedLen,
        )));
        length.result_type = IrType::I64;
        length.result_php_type = PhpType::Int;
        length.result_ownership = Ownership::NonHeap;
        length.effects = RuntimeFnId::Base64EncodedLen.intrinsic_effects();
    }
    if let Some(inst) = function.instruction_mut(candidate) {
        neutralize_to_nop(inst);
    }
    for release in releases {
        if let Some(inst) = function.instruction_mut(release) {
            neutralize_to_nop(inst);
        }
    }
    if mb_length.is_some() && count_value_uses(function).get(&operand).copied().unwrap_or(0) == 0 {
        if let Some((load_id, load)) = defining_instruction_with_id(function, operand) {
            if load.op == Op::LoadLocal {
                if let Some(load) = function.instruction_mut(load_id) {
                    neutralize_to_nop(load);
                }
            }
        }
    }
    true
}

/// Retires a local lifetime pin whose value is consumed only by the folded length.
///
/// `strtr()` and the case transforms pin an owned subject while their result may alias it.
/// Once the producer is replaced by a raw `StrLen`, the pin and its hidden slot are dead.
fn detach_operand_pin(
    function: &mut Function,
    operand: ValueId,
    keep: InstId,
    producer: InstId,
) -> Option<ValueId> {
    let crate::ir::ValueDef::Instruction { inst: acquire_id, .. } = function.value(operand)?.def else {
        return None;
    };
    let acquire = function.instruction(acquire_id)?;
    if acquire.op != Op::Acquire
        || acquire.immediate != Some(Immediate::Bool(true))
        || acquire.operands.len() != 1
    {
        return None;
    }
    let source = acquire.operands[0];
    let Some(slot) = store_slot(function, operand) else {
        return None;
    };
    let mut removable = vec![acquire_id];
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&operand) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id == keep || id == producer {
            continue;
        }
        match inst.op {
            Op::Release | Op::StoreLocal | Op::Borrow => removable.push(id),
            _ => return None,
        }
    }
    for (raw, inst) in function.instructions.iter().enumerate() {
        let Some(Immediate::LocalSlot(candidate)) = inst.immediate else {
            continue;
        };
        if candidate != slot {
            continue;
        }
        match inst.op {
            Op::ReleaseLocalSlot
            | Op::StoreLocal
            | Op::PushCallOperandOwner
            | Op::PopCallOperandOwner
            | Op::UnsetLocal => removable.push(InstId::from_raw(raw as u32)),
            _ => { eprintln!("string_len detach: bad slot op {:?}", inst.op); return None; }
        }
    }
    for id in removable {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    Some(source)
}

/// Returns the local slot written by the `StoreLocal` that stores `value`.
fn store_slot(function: &Function, value: ValueId) -> Option<crate::ir::LocalSlotId> {
    for inst in &function.instructions {
        if inst.op == Op::StoreLocal && inst.operands.as_slice() == [value] {
            if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
                return Some(slot);
            }
        }
    }
    None
}

/// Returns the literal data behind a `ConstStr` value, when available.
fn const_str_literal<'a>(function: &Function, data: &'a DataPool, value: ValueId) -> Option<&'a str> {
    let inst = defining_instruction(function, value)?;
    if inst.op != Op::ConstStr {
        return None;
    }
    let Some(Immediate::Data(id)) = inst.immediate else {
        return None;
    };
    data.strings.get(id.as_raw() as usize).map(String::as_str)
}

/// Returns the integer literal behind a value, when available.
fn const_i64(function: &Function, value: ValueId) -> Option<i64> {
    let inst = defining_instruction(function, value)?;
    if inst.op != Op::ConstI64 {
        return None;
    }
    match inst.immediate {
        Some(Immediate::I64(value)) => Some(value),
        _ => None,
    }
}

/// Returns the single `str_len` observer and removable releases of `result`.
fn analyze_uses(function: &Function, result: ValueId) -> Option<(InstId, Vec<InstId>)> {
    let mut length = None;
    let mut releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::StrLen if inst.operands.as_slice() == [result] && inst.result.is_some() => {
                if length.replace(id).is_some() {
                    return None;
                }
            }
            Op::Release if inst.operands.as_slice() == [result] => releases.push(id),
            _ => return None,
        }
    }
    for block in &function.blocks {
        if block
            .terminator
            .as_ref()
            .is_some_and(|term| term.used_values().contains(&result))
        {
            return None;
        }
    }
    Some((length?, releases))
}

/// Computes PHP's byte-set trim length for an ASCII subject and mask.
fn trimmed_ascii_len(subject: &[u8], mask: &[u8], left: bool, right: bool) -> i64 {
    let mut set = [false; 256];
    for &byte in mask {
        set[byte as usize] = true;
    }
    let mut begin = 0;
    let mut end = subject.len();
    if left {
        while begin < end && set[subject[begin] as usize] {
            begin += 1;
        }
    }
    if right {
        while end > begin && set[subject[end - 1] as usize] {
            end -= 1;
        }
    }
    (end - begin) as i64
}

/// Returns a `ConstStr` value, tracing one `Acquire` in front of it.
pub(super) fn const_string_value(function: &Function, data: &DataPool, value: ValueId) -> Option<String> {
    let value = match defining_instruction(function, value) {
        Some(inst) if inst.op == Op::Acquire && inst.operands.len() == 1 => inst.operands[0],
        _ => value,
    };
    let (_, inst) = defining_instruction_with_id(function, value)?;
    let (Op::ConstStr, Some(Immediate::Data(data_id))) = (inst.op, &inst.immediate) else {
        return None;
    };
    data.strings.get(data_id.as_raw() as usize).cloned()
}

/// Returns an immutable literal local's value and its load instruction.
pub(super) fn const_local_string(
    function: &Function,
    data: &DataPool,
    value: ValueId,
) -> Option<(String, InstId)> {
    let (_, load) = defining_instruction_with_id(function, value)?;
    // A bridge call stages an owned string argument through one Acquire; that identity
    // operation does not make the local's value dynamic.
    let value = if load.op == Op::Acquire && load.operands.len() == 1 {
        load.operands[0]
    } else {
        value
    };
    let (load_id, load) = defining_instruction_with_id(function, value)?;
    let (value, require_load) = if load.op == Op::LoadLocal {
        let Some(Immediate::LocalSlot(slot)) = load.immediate else {
            return None;
        };
        let entry = function.blocks.first()?;
        let mut stored = None;
        for (raw, inst) in function.instructions.iter().enumerate() {
            let id = InstId::from_raw(raw as u32);
            let touches_slot = matches!(inst.immediate, Some(Immediate::LocalSlot(found)) if found == slot);
            if !touches_slot {
                continue;
            }
            match inst.op {
                Op::StoreLocal if inst.operands.len() == 1 => {
                    if !entry.instructions.contains(&id) || stored.replace(inst.operands[0]).is_some() {
                        return None;
                    }
                }
                Op::LoadLocal | Op::ReleaseLocalSlot => {}
                _ => return None,
            }
        }
        (stored?, true)
    } else {
        (value, false)
    };
    let string = const_string_value(function, data, value)?;
    Some((string, if require_load { load_id } else { InstId::from_raw(u32::MAX) }))
}

/// Returns the defining instruction and identifier for one SSA value.
fn defining_instruction_with_id(function: &Function, value: ValueId) -> Option<(InstId, &crate::ir::Instruction)> {
    let definition = function.value(value)?;
    let crate::ir::ValueDef::Instruction { inst, .. } = definition.def else {
        return None;
    };
    Some((inst, function.instruction(inst)?))
}
