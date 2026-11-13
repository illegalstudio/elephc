//! Purpose:
//! Folds mbstring calls with compile-time constant arguments into string literals.
//!
//! Called from:
//! - The fixed-point EIR pass driver, after cross-function inlining exposes literal
//!   subjects and arguments.
//!
//! Key details:
//! - Only the canonical full-decimal numeric-entity map and an explicit valid encoding
//!   are folded; any other map, encoding, mode or dynamic subject falls back to the
//!   runtime bridge so PHP diagnostics and behavior stay exact.
//! - The folded result is a persistent `ConstStr`; the existing acquire/store/release
//!   scaffolding around the call keeps working without per-iteration allocation.
//! - The fold is target-independent and never crosses a user function or mutable state.

use elephc_mbstring::encoding::{Encoding, Substitute};
use elephc_mbstring::text::{self, KanaMode};

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget,
    RuntimeFnId, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::pure_mbstring_calls::full_decimal_map;
use super::string_len_sink::{const_local_string, const_string_value};

/// Canonical full decimal numeric-entity map used by the allocation-free length kernels.
const FULL_DECIMAL_MAP: [i64; 4] = [128, 1_114_111, 0, 2_097_151];

/// Folds constant `mb_convert_kana`/`mb_encode_numericentity`/`mb_decode_numericentity` calls.
pub struct MbConstantFold;

impl IrPass for MbConstantFold {
    /// Returns the stable pass name used in fixed-point diagnostics.
    fn name(&self) -> &'static str {
        "mb_constant_fold"
    }

    /// Skips functions with no constant-foldable mbstring producer.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(
                direct_target(inst),
                Some(
                    RuntimeFnId::MbConvertKana
                        | RuntimeFnId::MbEncodeNumericentity
                        | RuntimeFnId::MbDecodeNumericentity
                )
            )
        })
    }

    /// Folds every eligible call, returning true when at least one call changed.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| {
                matches!(
                    direct_target(inst),
                    Some(
                        RuntimeFnId::MbConvertKana
                            | RuntimeFnId::MbEncodeNumericentity
                            | RuntimeFnId::MbDecodeNumericentity
                    )
                )
            })
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if fold(function, data, candidate) {
                changed = true;
            }
        }
        changed
    }
}

/// Returns the direct or profiled runtime target for one call.
fn direct_target(inst: &crate::ir::Instruction) -> Option<RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
            | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// Reads a constant string through one acquire wrapper or an entry-local literal.
fn const_text(function: &Function, data: &DataPool, value: ValueId) -> Option<String> {
    const_string_value(function, data, value)
        .or_else(|| const_local_string(function, data, value).map(|(text, _)| text))
}

/// Follows transparent ownership wrappers back to the array value.
fn underlying(function: &Function, mut value: ValueId) -> ValueId {
    for _ in 0..8 {
        let Some(definition) = function.value(value) else {
            break;
        };
        let ValueDef::Instruction { inst, .. } = definition.def else {
            break;
        };
        let Some(instruction) = function.instruction(inst) else {
            break;
        };
        if matches!(
            instruction.op,
            Op::Acquire | Op::Borrow | Op::EnsureOwned | Op::Move | Op::MixedClone
        ) && instruction.operands.len() == 1
        {
            value = instruction.operands[0];
        } else {
            break;
        }
    }
    value
}

/// Folds one mbstring producer when every argument is a compile-time constant.
fn fold(function: &mut Function, data: &mut DataPool, call_id: InstId) -> bool {
    let Some(call) = function.instruction(call_id) else {
        return false;
    };
    let Some(target) = direct_target(call) else {
        return false;
    };
    let expected_operands = match target {
        RuntimeFnId::MbConvertKana
        | RuntimeFnId::MbEncodeNumericentity
        | RuntimeFnId::MbDecodeNumericentity => 3,
        _ => return false,
    };
    if call.operands.len() != expected_operands {
        return false;
    }
    let Some(subject) = const_text(function, data, call.operands[0]) else {
        return false;
    };
    let Some(encoding_name) = const_text(function, data, call.operands[2]) else {
        return false;
    };
    let Some(encoding) = Encoding::lookup_c_string(encoding_name.as_bytes()) else {
        return false;
    };
    // A valid UTF-8 subject cannot trigger the process-global replacement character
    // selected by `mb_substitute_character()`. Other encodings may need substitution,
    // so they stay on the runtime bridge where that state is authoritative.
    if !encoding.name().eq_ignore_ascii_case("UTF-8") {
        return false;
    }
    let output = match target {
        RuntimeFnId::MbConvertKana => {
            let Some(mode) = const_text(function, data, call.operands[1]) else {
                return false;
            };
            let Ok(mode) = KanaMode::parse(mode.as_bytes()) else {
                return false;
            };
            text::convert_kana(subject.as_bytes(), mode, encoding, Substitute::default())
        }
        RuntimeFnId::MbEncodeNumericentity => {
            if !full_decimal_map(function, underlying(function, call.operands[1])) {
                return false;
            }
            let Ok(output) = text::encode_numericentity(
                subject.as_bytes(),
                &FULL_DECIMAL_MAP,
                encoding,
                Substitute::default(),
                false,
            ) else {
                return false;
            };
            output
        }
        RuntimeFnId::MbDecodeNumericentity => {
            if !full_decimal_map(function, underlying(function, call.operands[1])) {
                return false;
            }
            let Ok(output) = text::decode_numericentity(
                subject.as_bytes(),
                &FULL_DECIMAL_MAP,
                encoding,
                Substitute::default(),
            ) else {
                return false;
            };
            output
        }
        _ => return false,
    };
    let Ok(output) = String::from_utf8(output) else {
        return false;
    };
    let Some(result) = call.result else {
        return false;
    };
    let data_id = data.intern_string(&output);
    let Some(call) = function.instruction_mut(call_id) else {
        return false;
    };
    call.op = Op::ConstStr;
    call.operands.clear();
    call.immediate = Some(Immediate::Data(data_id));
    call.result_type = IrType::Str;
    call.result_php_type = PhpType::Str;
    call.result_ownership = Ownership::Persistent;
    call.effects = Op::ConstStr.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::Str;
        value.php_type = PhpType::Str;
        value.ownership = Ownership::Persistent;
    }
    true
}
