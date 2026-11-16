//! Purpose:
//! Distinguishes actual PHP Closure objects from internal callable descriptors.
//!
//! Called from:
//! - Numeric loose-comparison emitters in generated code and the shared runtime.
//!
//! Key details:
//! - Descriptor target kinds describe invocation, not the original PHP value form.
//! - The object-handle lookup is read-only and preserves the unboxed payload registers.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Branches for descriptors without object identity, preserving the result and payload words.
pub(crate) fn emit_require_object_handle(emitter: &mut Emitter, payload: &str, non_object: &str) {
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    abi::emit_reg_move(emitter, result, payload);
    abi::emit_call_label(emitter, "__rt_object_handle_of");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x12, x0");                                 // retain the handle outside the lookup's scratch registers
            abi::emit_pop_reg(emitter, "x0");
            emitter.instruction(&format!("cbz x12, {non_object}"));             // implicit string and array callables are not PHP objects
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, rax");                                 // retain the handle outside the lookup's scratch registers
            abi::emit_pop_reg(emitter, "rax");
            emitter.instruction("test r9, r9");                                 // only actual Closure descriptors carry object identity
            emitter.instruction(&format!("jz {non_object}"));                   // implicit string and array callables remain quiet
        }
    }
}
