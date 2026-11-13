//! Purpose:
//! Contains unary numeric normalization and failures beneath the interpreter's Rust frames.
//!
//! Called from:
//! - Shared eval value bridge emission on every supported target.
//!
//! Key details:
//! - Version one takes a selector, numeric profile bits, a borrowed cell and an output slot.
//! - Unsupported selectors or policy bits return one before dereferencing either pointer.
//! - Exceptions return two and leave the output untouched; success transfers one owner.

use super::{abi, Arch, Emitter};
use crate::codegen_support::{runtime::exceptions::emit_protected_status, try_handlers::TRY_HANDLER_SLOT_SIZE};
use elephc_semantics::{arithmetic_policy::ArithmeticPolicy, numeric_unary::NumericUnaryOp, SemanticProfile, StrictFeature};

/// Emits an always-present callback with capabilities bounded by the immutable linked profile.
pub(super) fn emit(emitter: &mut Emitter) {
    let symbol = emitter.target.extern_symbol("__elephc_eval_numeric_unary_v1");
    emit_protected_status(emitter, &symbol, body);
}

/// Validates the requested contract, invokes its exact kernel and publishes only successful results.
fn body(emitter: &mut Emitter) {
    let prefix = "__rt_eval_numeric_unary_v1";
    let result = abi::int_result_reg(emitter);
    let scratch = abi::symbol_scratch_reg(emitter);
    let unsupported = format!("{prefix}_unsupported");
    let kind_ready = format!("{prefix}_kind_ready");
    let plus = format!("{prefix}_plus");
    let checked = format!("{prefix}_checked");
    let publish = format!("{prefix}_publish");
    let done = format!("{prefix}_done");
    abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE);
    ins(emitter, "cmp x0, #1", "cmp rax, 1");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b.hi {unsupported}")),   // reject invalid selectors before reading a borrowed value
        Arch::X86_64 => emitter.instruction(&format!("ja {unsupported}")),      // reject invalid selectors before reading a borrowed value
    }
    let allowed = ArithmeticPolicy::from_profile(emitter.semantic_profile).requirements().bits();
    abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE + 8);
    abi::emit_load_int_immediate(emitter, scratch, !allowed as i64);
    and_result(emitter, scratch);
    abi::emit_branch_if_int_result_nonzero(emitter, &unsupported);
    let kind_bits = SemanticProfile::PERMISSIVE.with(StrictFeature::Arithmetic, true).with(StrictFeature::Coercions, true).bits();
    abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE + 8);
    abi::emit_load_int_immediate(emitter, scratch, kind_bits as i64);
    and_result(emitter, scratch);
    abi::emit_branch_if_int_result_zero(emitter, &kind_ready);
    abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE + 16);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    for tag in [0, 2] {
        ins(emitter, &format!("cmp x0, #{tag}"), &format!("cmp rax, {tag}"));
        abi::emit_branch_if_equal_wide(emitter, &kind_ready);
    }
    abi::emit_jump(emitter, crate::codegen_support::runtime::arrays::strict_numeric_operand::TYPE_FAIL);
    emitter.label(&kind_ready);
    abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE);
    abi::emit_branch_if_int_result_zero(emitter, &plus);
    if emitter.semantic_profile.contains(StrictFeature::Overflow) {
        abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE + 8);
        abi::emit_load_int_immediate(emitter, scratch, SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true).bits() as i64);
        and_result(emitter, scratch);
        abi::emit_branch_if_int_result_nonzero(emitter, &checked);
    }
    invoke(emitter, NumericUnaryOp::Negate, false);
    abi::emit_jump(emitter, &publish);
    emitter.label(&plus);
    invoke(emitter, NumericUnaryOp::Plus, false);
    abi::emit_jump(emitter, &publish);
    if emitter.semantic_profile.contains(StrictFeature::Overflow) {
        emitter.label(&checked);
        invoke(emitter, NumericUnaryOp::Negate, true);
    }
    emitter.label(&publish);
    abi::emit_load_temporary_stack_slot(emitter, scratch, TRY_HANDLER_SLOT_SIZE + 24);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_jump(emitter, &done);
    emitter.label(&unsupported);
    abi::emit_load_int_immediate(emitter, result, 1);
    emitter.label(&done);
}

/// Loads the canonical borrowed operand and selects a reviewed runtime entry.
fn invoke(emitter: &mut Emitter, op: NumericUnaryOp, checked: bool) {
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), TRY_HANDLER_SLOT_SIZE + 16);
    abi::emit_call_label(emitter, crate::codegen_support::runtime::arrays::numeric_unary::symbol(op, checked));
}

/// Masks the selected C policy without relying on target-specific immediate encodings.
fn and_result(emitter: &mut Emitter, mask: &str) {
    ins(emitter, &format!("and x0, x0, {mask}"), &format!("and rax, {mask}"));
}

/// Keeps the boundary graph identical while spelling target instructions explicitly.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 });// preserve the versioned unary callback contract on every target
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target validates policy before input reads and contains all native failure paths.
    #[test]
    fn strict_unary_ieee_protected_boundary_checks_capabilities_before_values() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for checked in [false, true] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
                emit(&mut emitter);
                let asm = emitter.output();
                let validate = asm.find("__rt_eval_numeric_unary_v1_unsupported").unwrap();
                let classify = asm.find("__rt_mixed_unbox").unwrap();
                assert!(validate < classify, "{name}");
                assert!(asm.contains("_exc_handler_top") && asm.contains("_exc_call_frame_top"), "{name}");
                assert!(asm.contains("__rt_numeric_unary_plus") && asm.contains("__rt_numeric_unary_negate"), "{name}");
                assert_eq!(asm.contains("__rt_strict_numeric_unary_negate"), checked, "{name}");
                assert!(!asm.contains("__rt_throw_current"), "{name}");
            }
        }
    }
}
