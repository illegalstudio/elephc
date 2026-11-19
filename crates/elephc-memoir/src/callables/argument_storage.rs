//! Purpose:
//! Builds and retires framed packed argument vectors.
//!
//! Called from:
//! - Native callback adapters with a packed argument vector.
//!
//! Key details:
//! - Boxing holds its own reference, so both owners must be consumed.

use crate::{abi, Arch, Emitter};

/// Releases the box before the independent array owner at the caller's target-specific offsets.
pub fn emit_release_boxed_array(emitter: &mut impl Emitter, boxed: usize, array: usize) {
    for (slot, release) in [(boxed, "__rt_decref_mixed"), (array, "__rt_decref_any")] {
        emit_load_owner(emitter, slot);
        abi::emit_call_label(emitter, release);
    }
}

/// Appends a cell supplied in the native argument registers, then consumes its staged owner.
pub fn emit_append_owned_cell(emitter: &mut impl Emitter, array: usize, cell: usize) {
    abi::emit_call_label(emitter, "__rt_array_push_refcounted");
    emit_save_owner(emitter, array);
    emit_load_owner(emitter, cell);
    abi::emit_call_label(emitter, "__rt_decref_any");
}

/// Boxes a completed argument array and retires its separate raw owner.
pub fn emit_box_arguments(emitter: &mut impl Emitter, array: usize, boxed: usize) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x1, [sp, #{array}]"));            // load the array owner to box
            emitter.instruction("mov x2, #0");                                  // arrays have no high payload word
            emitter.instruction("mov x0, #4");                                  // use the native indexed-array tag
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {array}]")); // load the array owner to box
            emitter.instruction("xor esi, esi");                                // arrays have no high payload word
            emitter.instruction("mov eax, 4");                                  // use the native indexed-array tag
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emit_save_owner(emitter, boxed);
    emit_load_owner(emitter, array);
    abi::emit_call_label(emitter, "__rt_decref_any");
}

/// Loads an owned value in the native result register.
fn emit_load_owner(emitter: &mut impl Emitter, slot: usize) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("ldr x0, [sp, #{slot}]")), // reload an ARM frame owner
        Arch::X86_64 => emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {slot}]")), // reload an x86 frame owner
    }
}

/// Stores the current owner before another native operation can observe the frame.
fn emit_save_owner(emitter: &mut impl Emitter, slot: usize) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("str x0, [sp, #{slot}]")), // publish the ARM frame owner
        Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [rbp - {slot}], rax")), // publish the x86 frame owner
    }
}
