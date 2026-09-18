//! Purpose:
//! Contains native property-unset exceptions before control returns to Rust eval.
//!
//! Called from:
//! - The per-program typed-property unset helper emitter.
//!
//! Key details:
//! - The versioned C entry takes five borrowed arguments and a non-null Throwable output slot.
//! - Native jumps stay below Rust frames, and enclosing exception and diagnostic state is restored.

use super::*;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};

const FRAME: usize = TRY_HANDLER_SLOT_SIZE + 96;
const HANDLER: usize = FRAME - 16;
const OUTPUT: usize = 48;
const RESULT: usize = 56;
const PREVIOUS: usize = 64;
const THROWN: usize = 72;

/// Emits a C-compatible unset boundary that transfers any escaping Throwable as an owned box.
pub(super) fn emit(module: &Module, emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    label_c_global(module, emitter, "__elephc_eval_value_typed_property_unset_v2");
    // -- preserve inputs before installing the native jump target below the Rust caller --
    abi::emit_frame_prologue(emitter, FRAME);
    for index in 0..6 {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), (index + 1) * 8);
    }
    abi::load_at_offset(emitter, scratch, OUTPUT);
    abi::emit_store_zero_to_address(emitter, scratch, 0);
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::store_at_offset(emitter, result, PREVIOUS);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    for (symbol, offset) in [
        ("_exc_handler_top", HANDLER),
        ("_exc_call_frame_top", HANDLER - 8),
        ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET),
    ] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
    }
    abi::emit_frame_slot_address(emitter, result, HANDLER);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_handler_top", 0);
    abi::emit_frame_slot_address(emitter, arg0, HANDLER - TRY_HANDLER_JMP_BUF_OFFSET);
    emitter.bl_c("setjmp");                                                     // keep destructor exceptions below the Rust interpreter's frames
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_eval_unset_caught");
    for index in 0..5 {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), (index + 1) * 8);
    }
    abi::emit_call_label(emitter, "__rt_eval_typed_property_unset");
    abi::store_at_offset(emitter, result, RESULT);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::store_at_offset(emitter, result, THROWN);
    abi::emit_jump(emitter, "__rt_eval_unset_finish");

    // -- capture the escaping owner before restoring the enclosing native catch state --
    emitter.label("__rt_eval_unset_caught");
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::store_at_offset(emitter, result, THROWN);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::store_at_offset(emitter, result, RESULT);
    emitter.label("__rt_eval_unset_finish");
    for (symbol, offset) in [
        ("_exc_handler_top", HANDLER),
        ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET),
        ("_exc_value", PREVIOUS),
    ] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    }
    abi::load_at_offset(emitter, result, THROWN);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_eval_unset_return");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // use the escaping object as the Mixed payload
            emitter.instruction("mov x0, #6");                                  // tag six describes an object value
            emitter.instruction("mov x2, #0");                                  // object values have no second payload word
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // use the escaping object as the Mixed payload
            emitter.instruction("mov eax, 6");                                  // tag six describes an object value
            emitter.instruction("xor esi, esi");                                // object values have no second payload word
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::load_at_offset(emitter, scratch, OUTPUT);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::load_at_offset(emitter, result, THROWN);
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label("__rt_eval_unset_return");
    abi::load_at_offset(emitter, result, RESULT);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All targets expose only the versioned ABI and contain throws before transferring boxed ownership.
    #[test]
    fn native_unset_boundary_contains_throws_and_versions_its_output_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = Module::new(target);
            let mut emitter = Emitter::new(target);
            emit(&module, &mut emitter);
            let asm = emitter.output();
            assert!(asm.contains(&format!("{}:", target.extern_symbol("__elephc_eval_value_typed_property_unset_v2"))), "{name}");
            assert!(!asm.contains(&format!("{}:", target.extern_symbol("__elephc_eval_value_typed_property_unset"))), "{name}");
            let enter = asm.find(&target.extern_symbol("setjmp")).unwrap();
            let unset = asm.find("__rt_eval_typed_property_unset").unwrap();
            let caught = asm.find("__rt_eval_unset_caught:").unwrap();
            let boxed = asm.find("__rt_mixed_from_value").unwrap();
            let released = asm.find("__rt_decref_any").unwrap();
            assert!(enter < unset && unset < caught && caught < boxed && boxed < released, "{name}");
            assert!(asm.matches("_exc_value").count() >= 4, "{name}");
        }
    }
}
