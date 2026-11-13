//! Purpose:
//! Routes genuine boxed integer powers through checked arithmetic without changing PHP fallbacks.
//!
//! Called from:
//! - Mixed numeric power runtime emission when checked overflow is enabled.
//!
//! Key details:
//! - Mixed unboxing preserves reference-cell semantics.
//! - Inputs are borrowed; only the independently boxed numeric result is returned.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the strict boxed entry alongside the unchanged PHP power helper.
pub(super) fn emit(emitter: &mut Emitter) {
    if !emitter.semantic_profile.contains(elephc_semantics::StrictFeature::Overflow) { return; }
    let (left, right, payload) = match emitter.target.arch {
        Arch::AArch64 => ("x0", "x1", "x1"),
        Arch::X86_64 => ("rax", "rdi", "rdi"),
    };
    let fallback = "__rt_strict_mixed_numeric_pow_php";
    emitter.label_global("__rt_strict_mixed_numeric_pow");
    abi::emit_frame_prologue(emitter, 64);
    abi::emit_store_to_sp(emitter, left, 0);
    abi::emit_store_to_sp(emitter, right, 8);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    require_integer_or_fallback(emitter, fallback);
    abi::emit_store_to_sp(emitter, payload, 16);
    abi::emit_load_temporary_stack_slot(emitter, left, 8);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    require_integer_or_fallback(emitter, fallback);
    let base = abi::int_arg_reg_name(emitter.target, 0);
    let exponent = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_reg_move(emitter, exponent, payload);
    abi::emit_load_temporary_stack_slot(emitter, base, 16);
    abi::emit_call_label(emitter, "__rt_int_pow_or_throw");
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
    emitter.label(fallback);
    abi::emit_load_temporary_stack_slot(emitter, left, 0);
    abi::emit_load_temporary_stack_slot(emitter, right, 8);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_jump(emitter, "__rt_mixed_numeric_pow");
}

/// Inspects the unboxed tag without interpreting another PHP kind as an integer.
fn require_integer_or_fallback(emitter: &mut Emitter, fallback: &str) {
    let tag = abi::int_result_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cmp {tag}, #0")),        // genuine integer cells carry runtime tag zero
        Arch::X86_64 => emitter.instruction(&format!("cmp {tag}, 0")),          // genuine integer cells carry runtime tag zero
    }
    abi::emit_branch_if_not_equal_wide(emitter, fallback);
}
