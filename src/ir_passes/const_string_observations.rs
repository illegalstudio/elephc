//! Purpose:
//! Folds observations of compile-time constant strings: `strlen`, constant string offsets and
//! `ord()`.
//!
//! Called from:
//! - The EIR fixed-point pass driver after constant mbstring producers are folded.
//!
//! Key details:
//! - Only `ConstStr` values (including one reached through an immutable local) are folded; a
//!   load from a mutable slot keeps the dynamic instruction and its warning behavior.
//! - `strlen` counts PHP bytes (`literal_bytes`), not Rust UTF-8 length.
//! - A constant offset is folded only when it is provably in range; an out-of-range offset must
//!   keep the warning path. Negative offsets count from the end, matching PHP.
//! - `ord("")` is `0` in PHP; a non-empty constant folds to its first byte. The folded string
//!   uses the pool's byte-marker convention so codegen emits the exact PHP byte.
//! - The pass is target-independent and performs no allocation at run time.

use elephc_builtin_contract::string_literal::{literal_bytes, push_escaped_byte};

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget, RuntimeFnId,
    ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::string_len_sink::{const_local_string, const_string_value};

/// Folds `strlen`, constant offsets and `ord` over compile-time constant strings.
pub struct ConstStringObservations;

impl IrPass for ConstStringObservations {
    /// Returns the stable pass name used in fixed-point diagnostics.
    fn name(&self) -> &'static str {
        "const_string_observations"
    }

    /// Skips functions without an observation of a constant string producer.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(inst.op, Op::StrLen | Op::StrCharAt)
                || matches!(
                    inst.immediate,
                    Some(Immediate::RuntimeCall(
                        RuntimeCallTarget::Function(RuntimeFnId::Ord)
                            | RuntimeCallTarget::ProfiledFunction {
                                target: RuntimeFnId::Ord,
                                ..
                            }
                    ))
                )
        })
    }

    /// Rewrites one observation at a time; the driver re-runs the pass to a fixed point.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(raw, inst)| {
                let relevant = matches!(inst.op, Op::StrLen | Op::StrCharAt)
                    || matches!(
                        inst.immediate,
                        Some(Immediate::RuntimeCall(
                            RuntimeCallTarget::Function(RuntimeFnId::Ord)
                                | RuntimeCallTarget::ProfiledFunction {
                                    target: RuntimeFnId::Ord,
                                    ..
                                }
                        ))
                    );
                relevant.then_some(InstId::from_raw(raw as u32))
            })
            .collect();
        let mut changed = false;
        for id in candidates {
            let Some(inst) = function.instruction(id) else {
                continue;
            };
            let Some(result) = inst.result else {
                continue;
            };
            let Some(text) = resolve_constant_string(function, data, inst.operands.first().copied()) else {
                continue;
            };
            match inst.op {
                Op::StrLen => {
                    let length = literal_bytes(&text).len() as i64;
                    rewrite_int(function, id, result, length);
                    changed = true;
                }
                Op::StrCharAt => {
                    let Some(index) = const_i64_operand(function, inst.operands.get(1).copied()) else {
                        continue;
                    };
                    let Some(byte) = constant_offset(&text, index) else {
                        continue;
                    };
                    let mut literal = String::new();
                    push_escaped_byte(byte, &mut literal);
                    let data_id = data.intern_string(&literal);
                    rewrite_str(function, id, result, data_id);
                    changed = true;
                }
                _ => {
                    let byte = literal_bytes(&text).first().copied().unwrap_or(0) as i64;
                    rewrite_int(function, id, result, byte);
                    changed = true;
                }
            }
        }
        changed
    }
}

/// Resolves one operand to a constant string, following `acquire` and immutable local loads.
fn resolve_constant_string(function: &Function, data: &DataPool, value: Option<ValueId>) -> Option<String> {
    let value = value?;
    const_string_value(function, data, value)
        .or_else(|| const_local_string(function, data, value).map(|(text, _)| text))
}

/// Returns a constant integer operand, or `None` when it is dynamic.
fn const_i64_operand(function: &Function, value: Option<ValueId>) -> Option<i64> {
    let value = value?;
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    let inst = function.instruction(inst)?;
    if inst.op != Op::ConstI64 {
        return None;
    }
    match inst.immediate {
        Some(Immediate::I64(value)) => Some(value),
        _ => None,
    }
}

/// Returns the byte at a PHP string offset, or `None` when the offset is out of range.
fn constant_offset(text: &str, index: i64) -> Option<u8> {
    let bytes = literal_bytes(text);
    let length = i64::try_from(bytes.len()).ok()?;
    let resolved = if index < 0 { length.checked_add(index)? } else { index };
    if resolved < 0 || resolved >= length {
        return None;
    }
    bytes.get(usize::try_from(resolved).ok()?).copied()
}

/// Replaces one instruction with an integer constant, preserving its result value.
fn rewrite_int(function: &mut Function, id: InstId, result: ValueId, value: i64) {
    let Some(inst) = function.instruction_mut(id) else {
        return;
    };
    inst.op = Op::ConstI64;
    inst.operands.clear();
    inst.immediate = Some(Immediate::I64(value));
    inst.result_type = IrType::I64;
    inst.result_php_type = PhpType::Int;
    inst.result_ownership = Ownership::NonHeap;
    inst.effects = Op::ConstI64.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
}

/// Replaces one instruction with a persistent string constant, preserving its result value.
fn rewrite_str(function: &mut Function, id: InstId, result: ValueId, data_id: crate::ir::DataId) {
    let Some(inst) = function.instruction_mut(id) else {
        return;
    };
    inst.op = Op::ConstStr;
    inst.operands.clear();
    inst.immediate = Some(Immediate::Data(data_id));
    inst.result_type = IrType::Str;
    inst.result_php_type = PhpType::Str;
    inst.result_ownership = Ownership::Persistent;
    inst.effects = Op::ConstStr.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::Str;
        value.php_type = PhpType::Str;
        value.ownership = Ownership::Persistent;
    }
}
