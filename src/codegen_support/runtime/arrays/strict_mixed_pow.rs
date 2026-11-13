//! Purpose:
//! Computes boxed checked powers from exact PHP numeric operand classifications.
//!
//! Called from:
//! - Mixed numeric power runtime emission when checked overflow is enabled.
//!
//! Key details:
//! - Inputs are borrowed; only the independently boxed numeric result is returned.
//! - Both numeric payloads are captured before any PHP warning callback runs.
//! - Integer-form strings, booleans and null retain the integer overflow contract.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const FRAME: usize = 80;

/// Emits the strict boxed entry alongside the unchanged PHP power helper.
pub(super) fn emit(emitter: &mut Emitter) {
    if !emitter.semantic_profile.contains(elephc_semantics::StrictFeature::Overflow) { return; }
    super::strict_numeric_operand::emit(emitter);
    let result = abi::int_result_reg(emitter);
    let (low, high) = match emitter.target.arch { Arch::AArch64 => ("x1", "x2"), Arch::X86_64 => ("rdi", "rdx") };
    let prefix = "__rt_strict_mixed_numeric_pow";
    let floating = format!("{prefix}_floating");
    let done = format!("{prefix}_done");
    emitter.label_global(prefix);
    abi::emit_frame_prologue(emitter, FRAME);
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
    for tag in [24, 40] {
        abi::emit_load_temporary_stack_slot(emitter, result, tag);
        abi::emit_branch_if_int_result_nonzero(emitter, &floating);
    }
    abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 0), 16);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 1), 32);
    abi::emit_call_label(emitter, "__rt_int_pow_or_throw");
    abi::emit_jump(emitter, &done);
    emitter.label(&floating);
    load_float(emitter, 16, 24, prefix);
    abi::emit_store_to_sp(emitter, abi::float_result_reg(emitter), 0);
    load_float(emitter, 32, 40, prefix);
    ins(emitter, "fmov d1, d0", "movapd xmm1, xmm0");
    abi::emit_load_temporary_stack_slot(emitter, abi::float_result_reg(emitter), 0);
    emitter.bl_c("pow");                                                        // preserve IEEE-754 power for explicitly floating operands
    ins(emitter, "fmov x1, d0", "movq rdi, xmm0");
    abi::emit_load_int_immediate(emitter, result, 2);
    abi::emit_load_int_immediate(emitter, if emitter.target.arch == Arch::AArch64 { "x2" } else { "rsi" }, 0);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.label(&done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Converts an exact integer only after the operation has selected a float result.
fn load_float(emitter: &mut Emitter, payload: usize, tag: usize, prefix: &str) {
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
                let entry = asm.split("__rt_strict_mixed_numeric_pow:\n").nth(1).unwrap();
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
}
