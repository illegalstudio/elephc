//! Purpose:
//! Bounds producer result conversion before recursive packing can exhaust the host stack.
//!
//! Called from:
//! - The two target implementations of `__rt_zval_pack_element`.
//!
//! Key details:
//! - Only the producer profile references this request-local counter.
//! - Conversion failures roll back the native image instead of recursively retiring partial results.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Interposes a bounded wrapper before the ordinary value packer implementation.
pub(super) fn emit_guard(emitter: &mut Emitter) {
    const DEPTH: &str = "_elephc_phpext_pack_depth";
    if !emitter.phpext_output { return; }
    let full = "L_phpext_pack_depth_exceeded";
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_load_symbol_to_reg(emitter, scratch, DEPTH, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {scratch}, #65"));                // allow 64 nested arrays and their terminal scalar value
            emitter.instruction(&format!("b.hs {full}"));                       // reject excessive recursion before packing another child
            emitter.instruction(&format!("add {scratch}, {scratch}, #1"));      // reserve one conversion level for this value
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {scratch}, 65"));                 // allow 64 nested arrays and their terminal scalar value
            emitter.instruction(&format!("jae {full}"));                        // reject excessive recursion before packing another child
            emitter.instruction(&format!("add {scratch}, 1"));                  // reserve one conversion level for this value
        }
    }
    abi::emit_store_reg_to_symbol(emitter, scratch, DEPTH, 0);
    abi::emit_call_label(emitter, "__rt_zval_pack_element_impl");
    abi::emit_load_symbol_to_reg(emitter, scratch, DEPTH, 0);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("sub {scratch}, {scratch}, #1")), // release this completed conversion level without changing the result pointer
        Arch::X86_64 => emitter.instruction(&format!("sub {scratch}, 1")),      // release this completed conversion level without changing the result pointer
    }
    abi::emit_store_reg_to_symbol(emitter, scratch, DEPTH, 0);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
    emitter.label(full);
    abi::emit_store_imm_to_symbol(emitter, crate::codegen_support::cdylib::BOUNDARY_STATUS, 0,
        crate::codegen_support::cdylib::STATUS_INVALID_ARGUMENT as i64);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label_global("__rt_zval_pack_element_impl");
}
