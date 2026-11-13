//! Purpose:
//! Computes boxed checked arithmetic from exact PHP numeric operand classifications.
//!
//! Called from:
//! - Mixed numeric add/subtract/multiply/power emission when checked overflow is enabled.
//!
//! Key details:
//! - Inputs are borrowed; only the independently boxed numeric result is returned.
//! - Both numeric payloads are captured before any PHP warning callback runs.
//! - Integer-form strings, booleans and null retain the integer overflow contract.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::ir::MixedNumericOp;

pub(super) const FRAME: usize = 80;

/// Emits the strict boxed entry alongside the unchanged PHP power helper.
pub(super) fn emit(emitter: &mut Emitter) {
    if !emitter.semantic_profile.contains(elephc_semantics::StrictFeature::Overflow) { return; }
    super::strict_numeric_operand::emit(emitter);
    emit_entry(emitter, MixedNumericOp::Pow);
    super::strict_numeric_div::emit(emitter);
}

/// Emits independent checked kernels without altering the ordinary PHP-promoting entries.
pub(super) fn emit_basic(emitter: &mut Emitter) {
    if !emitter.semantic_profile.contains(elephc_semantics::StrictFeature::Overflow) { return; }
    for op in [MixedNumericOp::Add, MixedNumericOp::Sub, MixedNumericOp::Mul] { emit_entry(emitter, op); }
}

/// Shares input classification, callback snapshots, numeric kind selection and result ownership.
fn emit_entry(emitter: &mut Emitter, op: MixedNumericOp) {
    let result = abi::int_result_reg(emitter);
    let name = match op { MixedNumericOp::Add => "add", MixedNumericOp::Sub => "sub",
        MixedNumericOp::Mul => "mul", MixedNumericOp::Pow => "pow" };
    let prefix = format!("__rt_strict_mixed_numeric_{name}");
    let floating = format!("{prefix}_floating");
    let done = format!("{prefix}_done");
    emitter.label_global(&prefix);
    abi::emit_frame_prologue(emitter, FRAME);
    capture_operands(emitter, &prefix);
    for tag in [24, 40] {
        abi::emit_load_temporary_stack_slot(emitter, result, tag);
        abi::emit_branch_if_int_result_nonzero(emitter, &floating);
    }
    if op == MixedNumericOp::Pow {
        abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 0), 16);
        abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 1), 32);
        abi::emit_call_label(emitter, "__rt_int_pow_or_throw");
    } else {
        checked_integer(emitter, op);
    }
    abi::emit_jump(emitter, &done);
    emitter.label(&floating);
    load_float(emitter, 16, 24, &prefix);
    abi::emit_store_to_sp(emitter, abi::float_result_reg(emitter), 0);
    load_float(emitter, 32, 40, &prefix);
    ins(emitter, "fmov d1, d0", "movapd xmm1, xmm0");
    abi::emit_load_temporary_stack_slot(emitter, abi::float_result_reg(emitter), 0);
    match op {
        MixedNumericOp::Add => ins(emitter, "fadd d0, d0, d1", "addsd xmm0, xmm1"),
        MixedNumericOp::Sub => ins(emitter, "fsub d0, d0, d1", "subsd xmm0, xmm1"),
        MixedNumericOp::Mul => ins(emitter, "fmul d0, d0, d1", "mulsd xmm0, xmm1"),
        MixedNumericOp::Pow => emitter.bl_c("pow"),                              // preserve IEEE-754 power for explicitly floating operands
    }
    ins(emitter, "fmov x1, d0", "movq rdi, xmm0");
    box_number(emitter, 2);
    emitter.label(&done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Captures borrowed inputs and both exact numeric payloads before any PHP warning callback.
/// The caller reserves FRAME bytes; captured payload/tag pairs occupy 16/24 and 32/40.
pub(super) fn capture_operands(emitter: &mut Emitter, prefix: &str) {
    let result = abi::int_result_reg(emitter);
    let (low, high) = match emitter.target.arch { Arch::AArch64 => ("x1", "x2"), Arch::X86_64 => ("rdi", "rdx") };
    abi::emit_store_to_sp(emitter, result, 0);
    abi::emit_store_to_sp(emitter, low, 8);
    for (input, payload, tag, status) in [(0, 16, 24, 48), (8, 32, 40, 56)] {
        abi::emit_load_temporary_stack_slot(emitter, result, input);
        abi::emit_call_label(emitter, "__rt_strict_numeric_operand");
        abi::emit_store_to_sp(emitter, low, payload);
        abi::emit_store_to_sp(emitter, result, tag);
        abi::emit_store_to_sp(emitter, high, status);
    }
    for status in [48, 56] {
        let next = format!("{prefix}_warning_{status}_done");
        abi::emit_load_temporary_stack_slot(emitter, result, status);
        abi::emit_branch_if_int_result_zero(emitter, &next);
        let (pointer, length) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rdi", "rsi") };
        abi::emit_symbol_address(emitter, pointer, super::strict_numeric_operand::PREFIX_LABEL);
        abi::emit_load_int_immediate(emitter, length, super::strict_numeric_operand::PREFIX_WARNING.len() as i64);
        abi::emit_call_label(emitter, "__rt_diag_warning");
        emitter.label(&next);
    }
}

/// Computes exact integer payloads and throws before boxing when signed arithmetic overflows.
fn checked_integer(emitter: &mut Emitter, op: MixedNumericOp) {
    let (left, right) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rdi", "rdx") };
    abi::emit_load_temporary_stack_slot(emitter, left, 16);
    abi::emit_load_temporary_stack_slot(emitter, right, 32);
    match op {
        MixedNumericOp::Add => ins(emitter, "adds x1, x1, x2", "add rdi, rdx"),
        MixedNumericOp::Sub => ins(emitter, "subs x1, x1, x2", "sub rdi, rdx"),
        MixedNumericOp::Mul => {
            if emitter.target.arch == Arch::AArch64 {
                emitter.instruction("smulh x9, x1, x2");                        // retain the signed high half before overwriting the left operand
                emitter.instruction("mul x1, x1, x2");                          // compute the low signed product without converting either payload
                emitter.instruction("cmp x9, x1, asr #63");                     // a fitting product has only the low word's sign extension above it
                abi::emit_branch_if_not_equal_wide(emitter, "__rt_strict_integer_overflow_fail");
            } else {
                emitter.instruction("imul rdi, rdx");                           // multiply exact signed payloads and set the hardware overflow flag
                abi::emit_branch_if_overflow_wide(emitter, "__rt_strict_integer_overflow_fail");
            }
        }
        MixedNumericOp::Pow => unreachable!("power uses its reviewed integer kernel"),
    }
    if op != MixedNumericOp::Mul {
        abi::emit_branch_if_overflow_wide(emitter, "__rt_strict_integer_overflow_fail");
    }
    box_number(emitter, 0);
}

/// Transfers one exact numeric payload into a newly owned box without retaining either input.
pub(super) fn box_number(emitter: &mut Emitter, tag: i64) {
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), tag);
    abi::emit_load_int_immediate(emitter, if emitter.target.arch == Arch::AArch64 { "x2" } else { "rsi" }, 0);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
}

/// Converts an exact integer only after the operation has selected a float result.
pub(super) fn load_float(emitter: &mut Emitter, payload: usize, tag: usize, prefix: &str) {
    let result = abi::int_result_reg(emitter);
    let bits = format!("{prefix}_float_bits_{payload}");
    let done = format!("{prefix}_float_ready_{payload}");
    abi::emit_load_temporary_stack_slot(emitter, result, tag);
    abi::emit_branch_if_int_result_nonzero(emitter, &bits);
    abi::emit_load_temporary_stack_slot(emitter, result, payload);
    abi::emit_int_result_to_float_result(emitter);
    abi::emit_jump(emitter, &done);
    emitter.label(&bits);
    abi::emit_load_temporary_stack_slot(emitter, result, payload);
    ins(emitter, "fmov d0, x0", "movq xmm0, rax");
    emitter.label(&done);
}

/// Emits equivalent floating-register moves on both supported instruction sets.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // retain the same IEEE-754 payload on every target
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use elephc_semantics::{SemanticProfile, StrictFeature};

    /// Every target snapshots normalized values before callbacks and keeps exact integer power.
    #[test]
    fn strict_pow_runtime_normalizes_numeric_kinds_before_warning_dispatch_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for checked in [false, true] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.semantic_profile = profile;
                emit(&mut emitter);
                let asm = emitter.output();
                assert_eq!(asm.contains("__rt_strict_numeric_operand:"), checked, "{name}");
                assert_eq!(super::super::strict_numeric_operand::data(profile).is_empty(), !checked, "{name}");
                if !checked { continue; }
                let entry = asm.split("__rt_strict_mixed_numeric_pow:\n").nth(1).unwrap()
                    .split("__rt_strict_mixed_numeric_pow_done:\n").next().unwrap();
                let normalized = entry.match_indices("__rt_strict_numeric_operand").map(|(index, _)| index).collect::<Vec<_>>();
                assert_eq!(normalized.len(), 2, "{name}");
                assert!(normalized[1] < entry.find("__rt_diag_warning").unwrap(), "{name}");
                for helper in ["__rt_str_numeric_value", "__rt_int_pow_or_throw", "_spl_type_error_class_id"] {
                    assert!(asm.contains(helper), "{name} {helper}");
                }
                assert!(!asm.contains("__rt_mixed_cast_float"), "{name}");
            }
        }
    }

    /// All checked binary kernels capture both operands before callbacks on every target.
    #[test]
    fn strict_numeric_operands_runtime_shares_snapshots_and_integer_guards() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for checked in [false, true] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
                emit_basic(&mut emitter);
                emit(&mut emitter);
                let asm = emitter.output();
                for op in ["add", "sub", "mul", "pow"] {
                    let prefix = format!("__rt_strict_mixed_numeric_{op}");
                    assert_eq!(asm.contains(&format!("{prefix}:\n")), checked, "{name}, {op}");
                    if !checked { continue; }
                    let entry = asm.split(&format!("{prefix}:\n")).nth(1).unwrap()
                        .split(&format!("{prefix}_done:\n")).next().unwrap();
                    let inputs = entry.match_indices("__rt_strict_numeric_operand").map(|(index, _)| index).collect::<Vec<_>>();
                    assert_eq!(inputs.len(), 2, "{name}, {op}");
                    assert!(inputs[1] < entry.find("__rt_diag_warning").unwrap(), "{name}, {op}");
                    assert!(entry.contains("__rt_mixed_from_value"), "{name}, {op}");
                    if op != "pow" { assert!(entry.contains("__rt_strict_integer_overflow_fail"), "{name}, {op}"); }
                    assert!(!entry.contains("__rt_mixed_cast_"), "{name}, {op}");
                }
            }
        }
    }

}
