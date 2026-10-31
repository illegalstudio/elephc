//! Purpose:
//! Recognizes mbstring calls that are pure and non-throwing for their literal
//! explicit-UTF-8 arguments, so loop-invariant code motion may evaluate them once.
//!
//! Called from:
//! - `crate::ir_passes::immutable_local_loads` to admit immutable string loads.
//! - `crate::ir_passes::licm` before moving a runtime call into a loop preheader.
//!
//! Key details:
//! - Only calls whose result is a raw integer or boolean are admitted; heap string
//!   and array results need ownership-aware hoisting that this pass deliberately
//!   leaves to a later change.
//! - The encoding argument must be the literal `UTF-8`; default-encoding calls read
//!   mutable mbstring state and stay in the loop.
//! - Position calls additionally require a literal zero offset, which is always in
//!   range for any string and therefore cannot raise PHP's out-of-range ValueError.
//! - `mb_ord()` is admitted only for a non-empty literal subject because an empty
//!   subject throws. Every other admitted call is total for string arguments.
//! - The loop may contain no other global writer except `concat_reset`, whose
//!   process-local scratch reset cannot affect an explicit-encoding mbstring call.
//! - A numeric-entity length call is admitted only when its map is the literal full
//!   decimal packed array; the proof follows the load/store/array-push chain and
//!   rejects any other map or later array mutation.

use crate::ir::{
    DataPool, Effects, Function, Immediate, InstId, Instruction, IrHeapKind, IrType, Op,
    Ownership, RuntimeCallTarget, RuntimeFnId, ValueId,
};

use super::loops::NaturalLoop;
use super::rewrite::defining_instruction;
use super::string_len_sink::const_string_value;

/// Returns the runtime target a call instruction invokes, when it is a direct call.
pub(super) fn instruction_target(inst: &Instruction) -> Option<RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
            | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// Returns whether `target` is an mbstring builtin this pass may hoist.
pub(super) fn is_whitelisted_target(target: RuntimeFnId) -> bool {
    matches!(
        target,
        RuntimeFnId::MbStrlen
            | RuntimeFnId::MbStrwidth
            | RuntimeFnId::MbCheckEncoding
            | RuntimeFnId::MbSubstrCount
            | RuntimeFnId::MbOrdInt
            | RuntimeFnId::MbStrposInt
            | RuntimeFnId::MbStriposInt
            | RuntimeFnId::MbStrrposInt
            | RuntimeFnId::MbStrriposInt
            | RuntimeFnId::MbStrstrLen
            | RuntimeFnId::MbStristrLen
            | RuntimeFnId::MbStrrchrLen
            | RuntimeFnId::MbStrrichrLen
            | RuntimeFnId::MbStrimwidthLen
            | RuntimeFnId::MbChrLen
            | RuntimeFnId::MbConvertEncodingLen
            | RuntimeFnId::MbSplitCount
            | RuntimeFnId::MbEregReplaceLen
            | RuntimeFnId::MbDetectEncodingLen
            | RuntimeFnId::MbSubstr
            | RuntimeFnId::MbStrcut
            | RuntimeFnId::MbScrub
            | RuntimeFnId::MbConvertKana
            | RuntimeFnId::MbEncodeNumericentityLen
            | RuntimeFnId::MbDecodeNumericentityLen
    )
}

/// Returns whether `inst` is a pure, non-throwing mbstring call that may move out of `loop_ref`.
pub(super) fn is_hoistable_mbstring_call(
    function: &Function,
    data: &DataPool,
    inst_id: InstId,
    inst: &Instruction,
    loop_ref: &NaturalLoop,
) -> bool {
    let Some(target) = instruction_target(inst) else {
        return false;
    };
    if !is_whitelisted_target(target)
        || inst.result_type != IrType::I64
        || !matches!(inst.result_ownership, Ownership::NonHeap | Ownership::Persistent)
        || !matches!(
            inst.result_php_type.codegen_repr(),
            crate::types::PhpType::Int | crate::types::PhpType::Bool | crate::types::PhpType::False
        )
    {
        return false;
    }
    if !loop_writes_no_mbstring_state(function, inst_id, loop_ref) {
        return false;
    }
    let string_arg = |index: usize| string_arg(function, inst, index);
    let encoding_arg = |index: usize| utf8_encoding_arg(function, data, inst, index);
    let zero_offset = || {
        inst.operands.get(2).is_some_and(|&value| {
            defining_instruction(function, value).is_some_and(|offset| {
                offset.op == Op::ConstI64 && offset.immediate == Some(Immediate::I64(0))
            })
        })
    };
    let nonempty_literal = |index: usize| {
        inst.operands.get(index).is_some_and(|&value| {
            const_string_value(function, data, value).is_some_and(|text| !text.is_empty())
        })
    };
    let literal_is = |index: usize, expected: &str| {
        inst.operands.get(index).is_some_and(|&value| {
            const_string_value(function, data, value)
                .is_some_and(|text| text == expected)
        })
    };
    let latin1_arg = |index: usize| {
        inst.operands.get(index).is_some_and(|&value| {
            const_string_value(function, data, value).is_some_and(|encoding| {
                matches!(
                    encoding.to_ascii_uppercase().as_str(),
                    "ISO-8859-1" | "ISO8859-1" | "LATIN1" | "LATIN-1"
                )
            })
        })
    };
    let int_arg = |index: usize| {
        inst.operands
            .get(index)
            .and_then(|&value| const_i64(function, value))
    };
    let literal_separator = |index: usize| {
        inst.operands.get(index).is_some_and(|&value| {
            const_string_value(function, data, value).is_some_and(|text| {
                text.len() == 1
                    && matches!(text.as_bytes()[0], b',' | b';' | b':' | b'_' | b'=' | b'@' | b'!' | b'%' | b'&' | b'/')
            })
        })
    };
    match target {
        RuntimeFnId::MbStrlen
        | RuntimeFnId::MbStrwidth
        | RuntimeFnId::MbCheckEncoding => {
            inst.operands.len() == 2 && string_arg(0) && encoding_arg(1)
        }
        RuntimeFnId::MbSubstrCount => {
            inst.operands.len() == 3
                && string_arg(0)
                && string_arg(1)
                && nonempty_literal(1)
                && encoding_arg(2)
        }
        RuntimeFnId::MbStrposInt
        | RuntimeFnId::MbStriposInt
        | RuntimeFnId::MbStrrposInt
        | RuntimeFnId::MbStrriposInt => {
            inst.operands.len() == 4
                && string_arg(0)
                && string_arg(1)
                && zero_offset()
                && encoding_arg(3)
        }
        RuntimeFnId::MbOrdInt => {
            inst.operands.len() == 2
                && string_arg(0)
                && nonempty_literal(0)
                && encoding_arg(1)
        }
        RuntimeFnId::MbStrstrLen
        | RuntimeFnId::MbStristrLen
        | RuntimeFnId::MbStrrchrLen
        | RuntimeFnId::MbStrrichrLen => {
            inst.operands.len() == 4
                && string_arg(0)
                && string_arg(1)
                && encoding_arg(3)
        }
        RuntimeFnId::MbStrimwidthLen => {
            inst.operands.len() == 5
                && string_arg(0)
                && int_arg(1) == Some(0)
                && int_arg(2).is_some_and(|width| width >= 0)
                && inst.operands.get(3).is_some_and(|&value| {
                    const_string_value(function, data, value).is_some()
                })
                && encoding_arg(4)
        }
        RuntimeFnId::MbChrLen => {
            inst.operands.len() == 2
                && inst.operands.first().is_some_and(|&value| {
                    function.value(value).is_some_and(|value| {
                        value.php_type.codegen_repr() == crate::types::PhpType::Int
                    })
                })
                && encoding_arg(1)
        }
        RuntimeFnId::MbConvertEncodingLen => {
            inst.operands.len() == 3
                && string_arg(0)
                && latin1_arg(1)
                && encoding_arg(2)
        }
        RuntimeFnId::MbSplitCount => {
            inst.operands.len() == 2 && literal_separator(0) && string_arg(1)
        }
        RuntimeFnId::MbEregReplaceLen => {
            inst.operands.len() == 3
                && literal_is(0, "[0-9]+")
                && literal_is(1, "#")
                && string_arg(2)
        }
        RuntimeFnId::MbDetectEncodingLen => {
            inst.operands.len() == 2 && string_arg(0) && literal_is(1, "UTF-8")
        }
        RuntimeFnId::MbEncodeNumericentityLen | RuntimeFnId::MbDecodeNumericentityLen => {
            inst.operands.len() == 3
                && string_arg(0)
                && int_array_arg(function, inst, 1)
                && full_decimal_map(function, inst.operands[1])
                && encoding_arg(2)
        }
        _ => false,
    }
}

/// Returns the integer value of one constant-producing operand, if known.
fn const_i64(function: &Function, value: ValueId) -> Option<i64> {
    let inst = defining_instruction(function, value)?;
    match (inst.op, inst.immediate.as_ref()) {
        (Op::ConstI64, Some(Immediate::I64(number))) => Some(*number),
        _ => None,
    }
}

/// Returns whether `target` is a string slice whose length can replace its value.
pub(super) fn is_string_producer_target(target: RuntimeFnId) -> bool {
    matches!(
        target,
        RuntimeFnId::MbSubstr
            | RuntimeFnId::MbStrcut
            | RuntimeFnId::MbScrub
            | RuntimeFnId::MbConvertKana
    )
}

/// Returns whether `inst` is a total explicit-UTF-8 string slice whose owned
/// result may move once its length observer and release move with it.
pub(super) fn is_hoistable_string_producer_call(
    function: &Function,
    data: &DataPool,
    inst_id: InstId,
    inst: &Instruction,
    loop_ref: &NaturalLoop,
) -> bool {
    let Some(target) = instruction_target(inst) else {
        return false;
    };
    if !is_string_producer_target(target)
        || inst.result_type != IrType::Str
        || inst.result_ownership != Ownership::Owned
        || !string_arg(function, inst, 0)
    {
        return false;
    }
    let arguments_are_total = match target {
        RuntimeFnId::MbSubstr | RuntimeFnId::MbStrcut => {
            inst.operands.len() == 4
                && matches!(
                    inst.operands.get(1).and_then(|&value| function.value(value)).map(|value| value.php_type.codegen_repr()),
                    Some(crate::types::PhpType::Int)
                )
                && matches!(
                    inst.operands.get(2).and_then(|&value| function.value(value)).map(|value| value.php_type.codegen_repr()),
                    Some(crate::types::PhpType::Int)
                )
                && utf8_encoding_arg(function, data, inst, 3)
        }
        RuntimeFnId::MbScrub => {
            inst.operands.len() == 2 && utf8_encoding_arg(function, data, inst, 1)
        }
        RuntimeFnId::MbConvertKana => {
            inst.operands.len() == 3
                && inst.operands.get(1).is_some_and(|&value| {
                    const_string_value(function, data, value).is_some_and(|mode| {
                        mode.bytes().all(|byte| b"rRnNaAsSkKhHcCV".contains(&byte))
                    })
                })
                && utf8_encoding_arg(function, data, inst, 2)
        }
        _ => false,
    };
    if !arguments_are_total {
        return false;
    }
    let Some(result) = inst.result else {
        return false;
    };
    string_producer_observers_are_movable(function, result, loop_ref)
        && loop_writes_no_mbstring_state(function, inst_id, loop_ref)
}

/// Requires exactly one length observer and one release, both inside the loop.
/// Any other observer would keep using the string after the release moved, and a
/// missing release would turn a per-iteration temporary into one leaked owner.
fn string_producer_observers_are_movable(
    function: &Function,
    result: ValueId,
    loop_ref: &NaturalLoop,
) -> bool {
    let mut length = None;
    let mut release = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::StrLen if length.is_none() => length = Some(id),
            Op::Release if inst.operands.len() == 1 && release.is_none() => release = Some(id),
            _ => return false,
        }
    }
    let (Some(length), Some(release)) = (length, release) else {
        return false;
    };
    let in_loop = |id: InstId| {
        function.blocks.iter().any(|block| {
            loop_ref.blocks.contains(&block.id) && block.instructions.contains(&id)
        })
    };
    in_loop(length) && in_loop(release)
}

/// Returns whether one operand is a concrete PHP string value.
fn string_arg(function: &Function, inst: &Instruction, index: usize) -> bool {
    inst.operands
        .get(index)
        .and_then(|&value| function.value(value))
        .is_some_and(|value| value.php_type.codegen_repr() == crate::types::PhpType::Str)
}

/// Returns whether one operand is the literal UTF-8 encoding name.
fn utf8_encoding_arg(function: &Function, data: &DataPool, inst: &Instruction, index: usize) -> bool {
    inst.operands.get(index).is_some_and(|&value| {
        const_string_value(function, data, value)
            .is_some_and(|encoding| encoding.eq_ignore_ascii_case("UTF-8"))
    })
}

/// Returns whether one operand is a packed `array<int>`.
fn int_array_arg(function: &Function, inst: &Instruction, index: usize) -> bool {
    inst.operands
        .get(index)
        .and_then(|&value| function.value(value))
        .is_some_and(|value| {
            matches!(&value.php_type, crate::types::PhpType::Array(inner)
                if inner.codegen_repr() == crate::types::PhpType::Int)
        })
}

/// Proves that `map` is the literal `[0x80, 0x10FFFF, 0, 0x1FFFFF]` packed
/// integer array used by the allocation-free numeric-entity length helpers.
///
/// The proof follows the load/store wrapper chain, requires exactly four packed
/// pushes with those constants, and rejects any other use of the array value so
/// a later mutation cannot invalidate the map.
fn full_decimal_map(function: &Function, map: ValueId) -> bool {
    let mut value = map;
    let array = loop {
        let Some(inst) = defining_instruction(function, value) else {
            return false;
        };
        match inst.op {
            Op::LoadLocal => {
                let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
                    return false;
                };
                let mut stores = function.instructions.iter().filter(|other| {
                    other.op == Op::StoreLocal
                        && other.immediate == Some(Immediate::LocalSlot(slot))
                });
                let Some(store) = stores.next() else {
                    return false;
                };
                if stores.next().is_some() {
                    return false;
                }
                let Some(&stored) = store.operands.first() else {
                    return false;
                };
                value = stored;
            }
            Op::Acquire | Op::Move | Op::Borrow => {
                let Some(&inner) = inst.operands.first() else {
                    return false;
                };
                value = inner;
            }
            Op::ArrayNew => break value,
            _ => return false,
        }
    };
    let Some(array_inst) = defining_instruction(function, array) else {
        return false;
    };
    if array_inst.op != Op::ArrayNew
        || !matches!(array_inst.result_type, IrType::Heap(IrHeapKind::Array))
        || !matches!(&array_inst.result_php_type, crate::types::PhpType::Array(inner)
            if inner.codegen_repr() == crate::types::PhpType::Int)
    {
        return false;
    }
    let mut elements = Vec::new();
    for inst in &function.instructions {
        if !inst.operands.contains(&array) {
            continue;
        }
        match inst.op {
            Op::ArrayPush => {
                if inst.operands.len() != 2 || inst.operands[0] != array {
                    return false;
                }
                let Some(element) = const_i64(function, inst.operands[1]) else {
                    return false;
                };
                elements.push(element);
            }
            Op::Acquire | Op::Move | Op::Borrow | Op::Release => {
                if inst.operands.len() != 1 {
                    return false;
                }
            }
            _ => return false,
        }
    }
    elements == [0x80, 0x10FFFF, 0, 0x1FFFFF]
}

/// Returns whether the loop has no global writer other than the call and the
/// process-local concatenation scratch reset.
fn loop_writes_no_mbstring_state(
    function: &Function,
    current: InstId,
    loop_ref: &NaturalLoop,
) -> bool {
    for &block_id in &loop_ref.blocks {
        let Some(block) = function.block(block_id) else {
            continue;
        };
        for &inst_id in &block.instructions {
            if inst_id == current {
                continue;
            }
            let Some(inst) = function.instruction(inst_id) else {
                continue;
            };
            if inst.effects.contains(Effects::WRITES_GLOBAL) && inst.op != Op::ConcatReset {
                return false;
            }
        }
    }
    true
}

