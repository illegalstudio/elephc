//! Purpose:
//! Publishes initialized Mixed prefixes in exclusively owned, preallocated arrays.
//!
//! Called from:
//! - Compiler adapters that already hold a fresh cell to transfer into the current slot.
//!
//! Key details:
//! - A prefix advances only after the cell pointer is stored.
//! - No runtime call, retain or capacity growth is emitted. The caller proves capacity.

use crate::{Arch, Emitter};

const HEADER_BYTES: usize = 24;
const SLOT_SHIFT: usize = 3;

/// Transfers the result cell using a frame-relative cursor following any prefilled slots.
///
/// Uses only caller-saved registers. The cursor excludes the prefilled prefix.
pub fn emit_append_from_frame(
    emitter: &mut impl Emitter, array_slot: usize, cursor_slot: usize, prefilled: usize,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [sp, #{cursor_slot}]"));      // reload the current cursor
            emitter.instruction(&format!("ldr x10, [sp, #{array_slot}]"));      // reload the exclusive array owner
            if prefilled == 0 {
                emitter.instruction(&format!("lsl x12, x9, #{SLOT_SHIFT}"));    // compute the cursor's byte offset
            } else {
                emitter.instruction(&format!("add x12, x9, #{prefilled}"));     // skip the slots initialized before this loop
                emitter.instruction(&format!("lsl x12, x12, #{SLOT_SHIFT}"));   // compute the complete slot's byte offset
            }
            emitter.instruction(&format!("add x12, x12, #{HEADER_BYTES}"));     // advance to the payload region
            emitter.instruction("str x0, [x10, x12]");                          // transfer the result cell into the array owner
            emitter.instruction("add x9, x9, #1");                              // advance the loop cursor
            emitter.instruction(&format!("str x9, [sp, #{cursor_slot}]"));      // save the cursor without the prefilled slots
            if prefilled == 0 {
                emitter.instruction("str x9, [x10, #0]");                       // publish the initialized length
            } else {
                emitter.instruction(&format!("add x11, x9, #{prefilled}"));     // include the prefilled slots in the initialized length
                emitter.instruction("str x11, [x10, #0]");                      // publish the initialized length
            }
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r9, QWORD PTR [rbp - {cursor_slot}]")); // reload the current cursor
            emitter.instruction(&format!("mov r11, QWORD PTR [rbp - {array_slot}]")); // reload the exclusive array owner
            emitter.instruction("mov rcx, r9");                                 // stage the slot index in caller-saved scratch
            if prefilled != 0 {
                emitter.instruction(&format!("add rcx, {prefilled}"));          // skip the slots initialized before this loop
            }
            emitter.instruction(&format!("shl rcx, {SLOT_SHIFT}"));             // compute the cursor's byte offset
            emitter.instruction(&format!("add rcx, {HEADER_BYTES}"));           // advance to the payload region
            emitter.instruction("mov QWORD PTR [r11 + rcx], rax");              // transfer the result cell into the array owner
            emitter.instruction("add r9, 1");                                   // advance the loop cursor
            emitter.instruction(&format!("mov QWORD PTR [rbp - {cursor_slot}], r9")); // save the cursor without the prefilled slots
            if prefilled != 0 {
                emitter.instruction(&format!("add r9, {prefilled}"));           // include the prefilled slots in the initialized length
            }
            emitter.instruction("mov QWORD PTR [r11], r9");                     // publish the initialized length
        }
    }
}

/// Transfers the result cell using stack-top slots with a cursor equal to initialized length.
pub fn emit_append_from_stack(emitter: &mut impl Emitter, array_slot: usize, cursor_slot: usize) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [sp, #{array_slot}]"));       // reload the exclusive array owner
            emitter.instruction(&format!("ldr x10, [sp, #{cursor_slot}]"));     // reload the next uninitialized index
            emitter.instruction(&format!("add x11, x9, #{HEADER_BYTES}"));      // address the payload region
            emitter.instruction(&format!("str x0, [x11, x10, lsl #{SLOT_SHIFT}]")); // transfer the result cell into the array owner
            emitter.instruction("add x10, x10, #1");                            // extend the initialized prefix by one slot
            emitter.instruction("str x10, [x9]");                               // publish the safe cleanup length
            emitter.instruction(&format!("str x10, [sp, #{cursor_slot}]"));     // save the next insertion index
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {array_slot}]")); // reload the exclusive array owner
            emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {cursor_slot}]")); // reload the next uninitialized index
            let stride = 1 << SLOT_SHIFT;
            emitter.instruction(&format!("mov QWORD PTR [r10 + r11 * {stride} + {HEADER_BYTES}], rax")); // transfer the result cell into the array owner
            emitter.instruction("inc r11");                                     // extend the initialized prefix by one slot
            emitter.instruction("mov QWORD PTR [r10], r11");                    // publish the safe cleanup length
            emitter.instruction(&format!("mov QWORD PTR [rsp + {cursor_slot}], r11")); // save the next insertion index
        }
    }
}
