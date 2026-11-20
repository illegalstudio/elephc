//! Purpose:
//! Grows packed array storage and shifts existing slots to open an insertion gap.
//!
//! Called from:
//! - The compiler's packed array splice runtime emitters.
//!
//! Key details:
//! - The caller stages the destination, replacement and index in the splice frame and populates the gap.

use crate::{Arch, Emitter};

/// Opens a gap in single-word storage and branches to the caller's splice copy label.
pub fn emit_open_word_gap(emitter: &mut impl Emitter, prefix: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
        // -- grow the destination until the inserted elements fit its payload --
        emitter.instruction("ldr x10, [x0]");                                   // load the destination logical length
        emitter.instruction("ldr x11, [x0, #8]");                               // load the destination slot capacity
        emitter.instruction("add x12, x10, x9");                                // compute the element count the insertion needs room for
        emitter.label(&format!("{prefix}_grow_check"));
        emitter.instruction("cmp x12, x11");                                    // does the destination already have room for the insertion?
        emitter.instruction(&format!("b.le {prefix}_shift"));                   // yes, start sliding the tail right
        emitter.instruction("ldr x0, [sp, #0]");                                // reload the destination pointer before reallocating it
        emitter.instruction("bl __rt_array_grow");                              // at least double the destination payload capacity
        emitter.instruction("str x0, [sp, #0]");                                // persist the possibly-relocated destination pointer
        emitter.instruction("ldr x2, [sp, #8]");                                // reload the replacement pointer after the growth helper
        emitter.instruction("ldr x9, [x2]");                                    // reload the replacement element count after the growth helper
        emitter.instruction("ldr x10, [x0]");                                   // reload the destination logical length after the growth helper
        emitter.instruction("ldr x11, [x0, #8]");                               // reload the destination capacity after the growth helper
        emitter.instruction("add x12, x10, x9");                                // recompute the element count the insertion needs room for
        emitter.instruction(&format!("b {prefix}_grow_check"));                 // keep growing until the insertion fits

        // -- slide the elements at and after the insertion index to the right --
        emitter.label(&format!("{prefix}_shift"));
        emitter.instruction("ldr x0, [sp, #0]");                                // reload the destination indexed-array pointer
        emitter.instruction("ldr x2, [sp, #8]");                                // reload the replacement indexed-array pointer
        emitter.instruction("ldr x9, [x2]");                                    // x9 = replacement element count
        emitter.instruction("ldr x10, [x0]");                                   // x10 = destination logical length
        emitter.instruction("ldr x1, [sp, #16]");                               // x1 = requested insertion index
        emitter.instruction("cmp x1, #0");                                      // did the caller ask to insert before the first slot?
        emitter.instruction("csel x1, x1, xzr, ge");                            // clamp a negative insertion index to the front
        emitter.instruction("cmp x1, x10");                                     // does the insertion index lie past the last slot?
        emitter.instruction("csel x1, x1, x10, lt");                            // clamp an over-large insertion index to an append
        emitter.instruction("str x1, [sp, #16]");                               // persist the clamped insertion index for the copy loop
        emitter.instruction("add x3, x0, #24");                                 // x3 = destination payload base address
        emitter.instruction("sub x4, x10, #1");                                 // x4 = index of the last live destination element
        emitter.label(&format!("{prefix}_shift_loop"));
        emitter.instruction("cmp x4, x1");                                      // have all elements at or after the insertion index moved?
        emitter.instruction(&format!("b.lt {prefix}_copy"));                    // yes, write the replacement into the opened gap
        emitter.instruction("ldr x5, [x3, x4, lsl #3]");                        // load the element that has to slide right
        emitter.instruction("add x6, x4, x9");                                  // compute its slot after the opened gap
        emitter.instruction("str x5, [x3, x6, lsl #3]");                        // store the element past the opened gap
        emitter.instruction("sub x4, x4, #1");                                  // walk backwards so overlapping slots stay intact
        emitter.instruction(&format!("b {prefix}_shift_loop"));                 // continue sliding the tail right
        }
        Arch::X86_64 => {
        // -- grow the destination until the inserted elements fit its payload --
        emitter.instruction("mov r9, QWORD PTR [rdi]");                         // load the destination logical length
        emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                    // load the destination slot capacity
        emitter.instruction("mov r11, r9");                                     // seed the required element count from the destination length
        emitter.instruction("add r11, r8");                                     // compute the element count the insertion needs room for
        emitter.label(&format!("{prefix}_grow_check_x86"));
        emitter.instruction("cmp r11, r10");                                    // does the destination already have room for the insertion?
        emitter.instruction(&format!("jle {prefix}_shift_x86"));                // yes, start sliding the tail right
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // reload the destination pointer before reallocating it
        emitter.instruction("call __rt_array_grow");                            // at least double the destination payload capacity
        emitter.instruction("mov QWORD PTR [rbp - 8], rax");                    // persist the possibly-relocated destination pointer
        emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                   // reload the replacement pointer after the growth helper
        emitter.instruction("mov r8, QWORD PTR [rdx]");                         // reload the replacement element count after the growth helper
        emitter.instruction("mov r9, QWORD PTR [rax]");                         // reload the destination logical length after the growth helper
        emitter.instruction("mov r10, QWORD PTR [rax + 8]");                    // reload the destination capacity after the growth helper
        emitter.instruction("lea r11, [r9 + r8]");                              // recompute the element count the insertion needs room for
        emitter.instruction(&format!("jmp {prefix}_grow_check_x86"));           // keep growing until the insertion fits

        // -- slide the elements at and after the insertion index to the right --
        emitter.label(&format!("{prefix}_shift_x86"));
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // reload the destination indexed-array pointer
        emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                   // reload the replacement indexed-array pointer
        emitter.instruction("mov r8, QWORD PTR [rdx]");                         // r8 = replacement element count
        emitter.instruction("mov r9, QWORD PTR [rdi]");                         // r9 = destination logical length
        emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                   // rsi = requested insertion index
        emitter.instruction("xor eax, eax");                                    // materialize zero as the insertion-index floor
        emitter.instruction("test rsi, rsi");                                   // did the caller ask to insert before the first slot?
        emitter.instruction("cmovs rsi, rax");                                  // clamp a negative insertion index to the front
        emitter.instruction("cmp rsi, r9");                                     // does the insertion index lie past the last slot?
        emitter.instruction("cmovg rsi, r9");                                   // clamp an over-large insertion index to an append
        emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                   // persist the clamped insertion index for the copy loop
        emitter.instruction("lea r10, [rdi + 24]");                             // r10 = destination payload base address
        emitter.instruction("mov r11, r9");                                     // seed the slide cursor from the destination length
        emitter.instruction("sub r11, 1");                                      // r11 = index of the last live destination element
        emitter.label(&format!("{prefix}_shift_loop_x86"));
        emitter.instruction("cmp r11, rsi");                                    // have all elements at or after the insertion index moved?
        emitter.instruction(&format!("jl {prefix}_copy_x86"));                  // yes, write the replacement into the opened gap
        emitter.instruction("mov rax, QWORD PTR [r10 + r11 * 8]");              // load the element that has to slide right
        emitter.instruction("lea rcx, [r11 + r8]");                             // compute its slot after the opened gap
        emitter.instruction("mov QWORD PTR [r10 + rcx * 8], rax");              // store the element past the opened gap
        emitter.instruction("sub r11, 1");                                      // walk backwards so overlapping slots stay intact
        emitter.instruction(&format!("jmp {prefix}_shift_loop_x86"));           // continue sliding the tail right
        }
    }
}

/// Opens a gap in two-word string storage and branches to the string splice copy label.
pub fn emit_open_string_gap(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
        // -- grow the destination until the inserted string slots fit its payload --
        emitter.label("__rt_asis_grow_check");
        emitter.instruction("ldr x0, [sp, #0]");                                // reload the destination indexed-array pointer
        emitter.instruction("ldr x2, [sp, #8]");                                // reload the replacement indexed-array pointer
        emitter.instruction("ldr x9, [x2]");                                    // reload the replacement element count
        emitter.instruction("ldr x10, [x0]");                                   // reload the destination logical length
        emitter.instruction("ldr x11, [x0, #8]");                               // reload the destination slot capacity
        emitter.instruction("add x10, x10, x9");                                // compute the element count the insertion needs room for
        emitter.instruction("cmp x10, x11");                                    // does the destination already have room for the insertion?
        emitter.instruction("b.le __rt_asis_shift");                            // yes, start sliding the tail right
        emitter.instruction("bl __rt_array_grow");                              // at least double the destination payload capacity
        emitter.instruction("str x0, [sp, #0]");                                // persist the possibly-relocated destination pointer
        emitter.instruction("b __rt_asis_grow_check");                          // keep growing until the insertion fits

        // -- slide the string slots at and after the insertion index to the right --
        emitter.label("__rt_asis_shift");
        emitter.instruction("ldr x1, [sp, #16]");                               // x1 = requested insertion index
        emitter.instruction("ldr x10, [x0]");                                   // x10 = destination logical length
        emitter.instruction("cmp x1, #0");                                      // did the caller ask to insert before the first slot?
        emitter.instruction("csel x1, x1, xzr, ge");                            // clamp a negative insertion index to the front
        emitter.instruction("cmp x1, x10");                                     // does the insertion index lie past the last slot?
        emitter.instruction("csel x1, x1, x10, lt");                            // clamp an over-large insertion index to an append
        emitter.instruction("str x1, [sp, #16]");                               // persist the clamped insertion index for the copy loop
        emitter.instruction("add x3, x0, #24");                                 // x3 = destination string payload base address
        emitter.instruction("sub x4, x10, #1");                                 // x4 = index of the last live destination string slot
        emitter.label("__rt_asis_shift_loop");
        emitter.instruction("cmp x4, x1");                                      // have all slots at or after the insertion index moved?
        emitter.instruction("b.lt __rt_asis_copy");                             // yes, write the replacement into the opened gap
        emitter.instruction("lsl x5, x4, #4");                                  // scale the source slot index by the 16-byte string slot size
        emitter.instruction("add x5, x3, x5");                                  // compute the source string slot address
        emitter.instruction("ldp x6, x7, [x5]");                                // load the string pointer/length pair that has to slide right
        emitter.instruction("add x8, x4, x9");                                  // compute its slot after the opened gap
        emitter.instruction("lsl x8, x8, #4");                                  // scale that slot index by the string slot size
        emitter.instruction("add x8, x3, x8");                                  // compute the destination string slot address
        emitter.instruction("stp x6, x7, [x8]");                                // store the pair past the opened gap
        emitter.instruction("sub x4, x4, #1");                                  // walk backwards so overlapping slots stay intact
        emitter.instruction("b __rt_asis_shift_loop");                          // continue sliding the tail right
        }
        Arch::X86_64 => {
        // -- grow the destination until the inserted string slots fit its payload --
        emitter.label("__rt_asis_grow_check_x86");
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // reload the destination indexed-array pointer
        emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                   // reload the replacement indexed-array pointer
        emitter.instruction("mov r9, QWORD PTR [rdx]");                         // reload the replacement element count
        emitter.instruction("mov r10, QWORD PTR [rdi]");                        // reload the destination logical length
        emitter.instruction("mov r11, QWORD PTR [rdi + 8]");                    // reload the destination slot capacity
        emitter.instruction("add r10, r9");                                     // compute the element count the insertion needs room for
        emitter.instruction("cmp r10, r11");                                    // does the destination already have room for the insertion?
        emitter.instruction("jle __rt_asis_shift_x86");                         // yes, start sliding the tail right
        emitter.instruction("call __rt_array_grow");                            // at least double the destination payload capacity
        emitter.instruction("mov QWORD PTR [rbp - 8], rax");                    // persist the possibly-relocated destination pointer
        emitter.instruction("jmp __rt_asis_grow_check_x86");                    // keep growing until the insertion fits

        // -- slide the string slots at and after the insertion index to the right --
        emitter.label("__rt_asis_shift_x86");
        emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                   // rsi = requested insertion index
        emitter.instruction("mov r10, QWORD PTR [rdi]");                        // r10 = destination logical length
        emitter.instruction("test rsi, rsi");                                   // did the caller ask to insert before the first slot?
        emitter.instruction("jns __rt_asis_index_low_x86");                     // a non-negative index only needs the upper clamp
        emitter.instruction("xor esi, esi");                                    // clamp a negative insertion index to the front
        emitter.label("__rt_asis_index_low_x86");
        emitter.instruction("cmp rsi, r10");                                    // does the insertion index lie past the last slot?
        emitter.instruction("jle __rt_asis_index_ready_x86");                   // keep an index that still points inside the payload
        emitter.instruction("mov rsi, r10");                                    // clamp an over-large insertion index to an append
        emitter.label("__rt_asis_index_ready_x86");
        emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                   // persist the clamped insertion index for the copy loop
        emitter.instruction("lea r11, [rdi + 24]");                             // r11 = destination string payload base address
        emitter.instruction("mov rcx, r10");                                    // copy the destination length before turning it into a cursor
        emitter.instruction("sub rcx, 1");                                      // rcx = index of the last live destination string slot
        emitter.label("__rt_asis_shift_loop_x86");
        emitter.instruction("cmp rcx, rsi");                                    // have all slots at or after the insertion index moved?
        emitter.instruction("jl __rt_asis_copy_x86");                           // yes, write the replacement into the opened gap
        emitter.instruction("mov rax, rcx");                                    // copy the source slot index before scaling it
        emitter.instruction("shl rax, 4");                                      // scale it by the 16-byte string slot size
        emitter.instruction("mov r8, QWORD PTR [r11 + rax]");                   // load the string pointer that has to slide right
        emitter.instruction("mov rdi, QWORD PTR [r11 + rax + 8]");              // load the matching string length
        emitter.instruction("mov rax, rcx");                                    // recopy the source slot index for the destination offset
        emitter.instruction("add rax, r9");                                     // compute its slot after the opened gap
        emitter.instruction("shl rax, 4");                                      // scale that slot index by the string slot size
        emitter.instruction("mov QWORD PTR [r11 + rax], r8");                   // store the string pointer past the opened gap
        emitter.instruction("mov QWORD PTR [r11 + rax + 8], rdi");              // store the matching string length past the opened gap
        emitter.instruction("sub rcx, 1");                                      // walk backwards so overlapping slots stay intact
        emitter.instruction("jmp __rt_asis_shift_loop_x86");                    // continue sliding the tail right
        }
    }
}

mod payload;
pub use payload::{
    emit_array_splice_insert, emit_array_splice_insert_boxed, emit_array_splice_insert_refcounted,
    emit_array_splice_insert_str, emit_array_splice_insert_unboxed,
};
