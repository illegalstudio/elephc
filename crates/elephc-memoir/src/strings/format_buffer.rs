//! Purpose:
//! Opens and publishes the bounded scratch window for formatted output.
//!
//! Called from:
//! - Both native sprintf emitters.
//!
//! Key details:
//! - The partial write cursor can be published before a reentrant conversion.

use crate::{abi, Emitter};

/// Initializes the output pointer, cursor and exclusive scratch bound.
pub fn emit_open_aarch64(emitter: &mut impl Emitter) {
    super::concat_cursor::emit_cursor_address(emitter, "x25");
    emitter.instruction("ldr x8, [x25]");                                       // current concat-buffer write offset
    super::concat_cursor::emit_base_address(emitter, "x7");
    emitter.instruction("add x23, x7, x8");                                     // write cursor = buffer base + offset
    emitter.instruction("mov x24, x23");                                        // remember where this result starts
    abi::emit_load_int_immediate(emitter, "x9", emitter.concat_capacity() as i64); // total concat-buffer capacity in bytes
    emitter.instruction("add x9, x7, x9");                                      // one-past-the-end address of the concat buffer
    emitter.instruction("str x9, [sp, #144]");                                  // publish the hard write limit for every copy below
}

/// Commits the exact end of the formatted output.
pub fn emit_finish_aarch64(emitter: &mut impl Emitter) {
    super::concat_cursor::emit_base_address(emitter, "x8");
    emitter.instruction("sub x8, x23, x8");                                     // derive the absolute cursor after nested concat-producing conversions
    emitter.instruction("str x8, [x25]");                                       // publish the exact new write offset without double-counting
}

/// Publishes the partial prefix before calling a nested producer.
pub fn emit_partial_aarch64(emitter: &mut impl Emitter) {
    super::concat_cursor::emit_base_address(emitter, "x9");
    emitter.instruction("sub x10, x23, x9");                                    // publish bytes already written before a nested __toString call
    emitter.instruction("str x10, [x25]");                                      // make nested concat users start after the partial sprintf result
}

/// Initializes the output pointer, cursor and exclusive scratch bound.
pub fn emit_open_x86_64(emitter: &mut impl Emitter) {
    super::concat_cursor::emit_cursor_address(emitter, "r10");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // current concat-buffer write offset
    super::concat_cursor::emit_base_address(emitter, "rcx");
    emitter.instruction("lea rbx, [rcx + r11]");                                // write cursor = buffer base + offset
    emitter.instruction("mov QWORD PTR [rbp - 48], rbx");                       // remember where this result starts
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // remember the concat-offset symbol address
    abi::emit_load_int_immediate(emitter, "r11", emitter.concat_capacity() as i64); // load the shared scratch capacity
    emitter.instruction("add rcx, r11");                                        // one-past-the-end address of the concat buffer
    emitter.instruction("mov QWORD PTR [rbp - 120], rcx");                      // publish the hard write limit for every copy below
}

/// Commits the exact end of the formatted output.
pub fn emit_finish_x86_64(emitter: &mut impl Emitter) {
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // concat-offset symbol address
    super::concat_cursor::emit_base_address(emitter, "r11");
    emitter.instruction("sub rbx, r11");                                        // derive the absolute cursor after nested concat-producing conversions
    emitter.instruction("mov QWORD PTR [r10], rbx");                            // publish the exact new write offset without double-counting
}

/// Publishes the partial prefix before calling a nested producer.
pub fn emit_partial_x86_64(emitter: &mut impl Emitter) {
    super::concat_cursor::emit_base_address(emitter, "r9");
    emitter.instruction("mov rax, rbx");                                        // copy the partial-result write cursor
    emitter.instruction("sub rax, r9");                                         // compute bytes already written before nested __toString
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // reload the address of the global concat offset
    emitter.instruction("mov QWORD PTR [r9], rax");                             // make nested concat users start after the partial result
}

/// Branches to the overflow handler when the next byte is outside the scratch window.
pub fn emit_require_literal_room(emitter: &mut impl Emitter, overflow: &str) {
    if emitter.arch() == crate::Arch::AArch64 {
        emitter.instruction("ldr x9, [sp, #144]");                              // reload the concat-buffer write limit
        emitter.instruction("cmp x23, x9");                                     // would this literal byte land outside the arena?
        emitter.instruction(&format!("b.hs {overflow}"));                       // yes → controlled fatal instead of an overrun
    } else {
        emitter.instruction("mov r9, QWORD PTR [rbp - 120]");                   // reload the concat-buffer write limit
        emitter.instruction("cmp rbx, r9");                                     // would this literal byte land outside the arena?
        emitter.instruction(&format!("jae {overflow}"));                        // yes → controlled fatal instead of an overrun
    }
}

/// Computes a formatted field end and checks it against the exclusive scratch bound.
pub fn emit_require_field_room(emitter: &mut impl Emitter, overflow: &str) {
    if emitter.arch() == crate::Arch::AArch64 {
        emitter.instruction("add x13, x13, x23");                               // address just past the emitted bytes
        emitter.instruction("ldr x15, [sp, #144]");                             // concat-buffer write limit
        emitter.instruction("cmp x13, x15");                                    // would the conversion leave the arena?
        emitter.instruction(&format!("b.hi {overflow}"));                       // yes → controlled fatal instead of an overrun
    } else {
        emitter.instruction("add rdx, rbx");                                    // address just past the emitted bytes
        emitter.instruction("cmp rdx, QWORD PTR [rbp - 120]");                  // would the conversion leave the arena?
        emitter.instruction(&format!("ja {overflow}"));                         // yes → controlled fatal instead of an overrun
    }
}
