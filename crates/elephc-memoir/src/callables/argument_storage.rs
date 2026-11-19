//! Purpose:
//! Retires both owners of a framed boxed argument array.
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
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x0, [sp, #{slot}]"));         // take an owner from the enclosing ARM frame
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {slot}]")); // take an owner from the enclosing x86 frame
            }
        }
        abi::emit_call_label(emitter, release);
    }
}
