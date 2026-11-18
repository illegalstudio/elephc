//! Purpose:
//! Retires temporary owner slots and transfers validated boxed object ownership.
//!
//! Called from:
//! - Compiler exception cleanup and native/eval ownership bridges.
//!
//! Key details:
//! - Acquisition precedes source retirement; PHP validation stays in the compiler.

use crate::{abi, Arch, Emitter};

/// Emits the non-escaping unary callback for a temporary owner slot on every native ABI.
pub fn emit_cleanup_call_operand_owner(emitter: &mut impl Emitter) {
    emit_owner_cleanup_entry(emitter, "__rt_cleanup_call_operand_owner", "__rt_decref_any");
    emit_owner_cleanup_entry(emitter, "__rt_cleanup_call_operand_descriptor", "__rt_callable_descriptor_release");
    emit_owner_cleanup_entry(emitter, "__rt_cleanup_call_reference_argument", "__rt_decref_any");
}

/// Emits one bounded cleanup entry with the release discipline of its owning slot.
fn emit_owner_cleanup_entry(emitter: &mut impl Emitter, label: &str, release: &str) {
    emitter.blank();
    emitter.label_global(label);
    abi::emit_frame_prologue(emitter, 16);
    let address = abi::secondary_scratch_reg(emitter);
    let result = abi::int_result_reg(emitter);
    abi::emit_reg_move(emitter, address, abi::int_arg_reg_name(emitter, 0));
    abi::emit_load_from_address(emitter, result, address, 0);
    abi::emit_store_zero_to_address(emitter, address, 0);
    crate::cleanup_preserve::emit_unary_cleanup_preserving_exception(emitter, release, result);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

/// Emits consuming conversions between validated Throwable boxes and native object owners.
pub fn emit_throwable_boxed_owners(emitter: &mut impl Emitter) {
    // -- detach a raw exception owner from an owned eval box --
    emitter.label_global("__rt_throwable_take_boxed");
    abi::emit_frame_prologue(emitter, 32);
    let result = abi::int_result_reg(emitter);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction("mov x0, x1"),                     // retain the raw object payload before consuming its box
        Arch::X86_64 => emitter.instruction("mov rax, rdi"),                    // retain the raw object payload before consuming its box
    }
    abi::emit_call_label(emitter, "__rt_incref");
    abi::store_at_offset(emitter, result, 16);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::load_at_offset(emitter, result, 16);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);

    // -- box a transferred native exception without retaining a second raw owner --
    emitter.label_global("__rt_throwable_box_owned");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, result, 8);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // move the owned raw exception into the object payload
            emitter.instruction("mov x0, #6");                                  // box the validated object with the object tag
            emitter.instruction("mov x2, #0");                                  // objects have no second payload word
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // move the owned raw exception into the object payload
            emitter.instruction("mov eax, 6");                                  // box the validated object with the object tag
            emitter.instruction("xor esi, esi");                                // objects have no second payload word
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, result, 16);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::load_at_offset(emitter, result, 16);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}
