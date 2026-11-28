//! Purpose:
//! Computes unary numeric results from one exact borrowed operand snapshot.
//!
//! Called from:
//! - Typed EIR numeric unary calls and the protected eval unary bridge.
//!
//! Key details:
//! - Warning callbacks run after the normalized payload and kind have been captured.
//! - PHP and checked negation coexist; neither mutates or retains the borrowed input.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_semantics::{numeric_unary::NumericUnaryOp, StrictFeature};

/// Selects an explicit kernel without inferring arithmetic behavior from runtime-global state.
pub(crate) fn symbol(op: NumericUnaryOp, checked: bool) -> &'static str {
    match (op, checked) {
        (NumericUnaryOp::Plus, _) => "__rt_numeric_unary_plus",
        (NumericUnaryOp::Negate, false) => "__rt_numeric_unary_negate",
        (NumericUnaryOp::Negate, true) => "__rt_strict_numeric_unary_negate",
    }
}

/// Emits ordinary PHP entries everywhere and checked negation only in a capable runtime.
pub(in crate::codegen_support::runtime) fn emit(emitter: &mut Emitter) {
    super::strict_numeric_operand::emit(emitter);
    entry(emitter, NumericUnaryOp::Plus, false);
    entry(emitter, NumericUnaryOp::Negate, false);
    if emitter.semantic_profile.contains(StrictFeature::Overflow) { entry(emitter, NumericUnaryOp::Negate, true); }
}

/// Snapshots normalization before diagnostics, then computes and boxes exactly one numeric result.
fn entry(emitter: &mut Emitter, op: NumericUnaryOp, checked: bool) {
    let prefix = symbol(op, checked);
    let floating = format!("{prefix}_floating");
    let box_value = format!("{prefix}_box");
    let result = abi::int_result_reg(emitter);
    let (payload, status) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rdi", "rdx") };
    emitter.label_global(prefix);
    abi::emit_frame_prologue(emitter, 48);
    abi::emit_call_label(emitter, "__rt_strict_numeric_operand");
    for (register, offset) in [(payload, 0), (result, 8), (status, 16)] { abi::emit_store_to_sp(emitter, register, offset); }
    super::strict_mixed_numeric::dispatch_operand_status(emitter, prefix, 16);
    abi::emit_load_temporary_stack_slot(emitter, result, 8);
    abi::emit_load_temporary_stack_slot(emitter, payload, 0);
    if op == NumericUnaryOp::Negate {
        abi::emit_branch_if_int_result_nonzero(emitter, &floating);
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("negs x1, x1"),                // negate the exact integer and retain signed-overflow evidence
            Arch::X86_64 => emitter.instruction("neg rdi"),                     // negate the exact integer and retain signed-overflow evidence
        }
        abi::emit_branch_if_overflow_wide(emitter, if checked { "__rt_strict_integer_overflow_fail" } else { &floating });
        abi::emit_jump(emitter, &box_value);
        emitter.label(&floating);
        super::strict_mixed_numeric::load_float(emitter, 0, 8, prefix);
        abi::emit_negate_float_result(emitter);
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("fmov x1, d0"),                // retain the negated floating payload without integer conversion
            Arch::X86_64 => emitter.instruction("movq rdi, xmm0"),              // retain the negated floating payload without integer conversion
        }
        abi::emit_load_int_immediate(emitter, result, 2);
    }
    emitter.label(&box_value);
    abi::emit_load_int_immediate(emitter, if emitter.target.arch == Arch::AArch64 { "x2" } else { "rsi" }, 0);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}
