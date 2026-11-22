//! Purpose:
//! Resizes the native owned Mixed slots backing a fixed array.
//!
//! Called from:
//! - The compiler's fixed-array runtime after validating the requested size.
//!
//! Key details:
//! - Growth publishes each replacement backing allocation on the receiver.
//! - Truncated cells are released before clearing their slots. The new length is published last.

use crate::{Arch, Emitter};

/// Resizes a fixed array at the supplied receiver offset using the active resize frame.
pub fn emit_resize(emitter: &mut impl Emitter, storage_offset: usize) {
    match emitter.arch() {
        Arch::AArch64 => resize_aarch64(emitter, storage_offset),
        Arch::X86_64 => resize_x86_64(emitter, storage_offset),
    }
}

/// Emits physical resize for the aarch64 storage and its existing spills.
fn resize_aarch64(emitter: &mut impl Emitter, storage_offset: usize) {
    emitter.instruction("str x1, [sp, #8]");                                    // save requested new size
    emitter.instruction(&format!("ldr x9, [x0, #{}]", storage_offset));         // load current storage
    emitter.instruction("str x9, [sp, #16]");                                   // save current storage pointer
    emitter.instruction("ldr x10, [x9]");                                       // load old size
    emitter.instruction("str x10, [sp, #24]");                                  // save old size
    emitter.label("__rt_spl_fixed_set_size_grow_check");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload storage pointer
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload requested size
    emitter.instruction("ldr x11, [x9, #8]");                                   // load storage capacity
    emitter.instruction("cmp x10, x11");                                        // does storage need more capacity?
    emitter.instruction("b.le __rt_spl_fixed_set_size_release_tail");           // skip growth when capacity is enough
    emitter.instruction("mov x0, x9");                                          // pass current storage to array_grow
    emitter.instruction("bl __rt_array_grow");                                  // grow fixed-array storage
    emitter.instruction("str x0, [sp, #16]");                                   // save grown storage pointer
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload receiver
    emitter.instruction(&format!("str x0, [x9, #{}]", storage_offset));         // publish grown storage on receiver
    emitter.instruction("b __rt_spl_fixed_set_size_grow_check");                // grow again if requested size still exceeds capacity
    emitter.label("__rt_spl_fixed_set_size_release_tail");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload storage pointer
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload requested new size
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload old size
    emitter.instruction("add x12, x9, #24");                                    // point at first storage slot
    emitter.instruction("mov x13, x10");                                        // tail-release cursor starts at new size
    emitter.label("__rt_spl_fixed_set_size_tail_loop");
    emitter.instruction("cmp x13, x11");                                        // have all truncated slots been released?
    emitter.instruction("b.ge __rt_spl_fixed_set_size_zero_new");               // stop tail release at old size
    emitter.instruction("ldr x0, [x12, x13, lsl #3]");                          // load truncated Mixed cell
    emitter.instruction("str x9, [sp, #16]");                                   // preserve storage across release
    emitter.instruction("str x10, [sp, #8]");                                   // preserve requested size across release
    emitter.instruction("str x11, [sp, #24]");                                  // preserve old size across release
    emitter.instruction("str x13, [sp, #32]");                                  // preserve tail-release cursor
    emitter.instruction("bl __rt_decref_mixed");                                // release truncated Mixed cell if present
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload storage after release
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload requested size after release
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload old size after release
    emitter.instruction("ldr x13, [sp, #32]");                                  // reload tail-release cursor after release
    emitter.instruction("add x12, x9, #24");                                    // restore storage slot base
    emitter.instruction("str xzr, [x12, x13, lsl #3]");                         // clear released slot
    emitter.instruction("add x13, x13, #1");                                    // advance tail-release cursor
    emitter.instruction("b __rt_spl_fixed_set_size_tail_loop");                 // continue releasing truncated slots
    emitter.label("__rt_spl_fixed_set_size_zero_new");
    emitter.instruction("ldr x13, [sp, #24]");                                  // zero-fill starts at old size
    emitter.label("__rt_spl_fixed_set_size_zero_loop");
    emitter.instruction("cmp x13, x10");                                        // have all newly exposed slots been zeroed?
    emitter.instruction("b.ge __rt_spl_fixed_set_size_done");                   // finish after zero-filling to requested size
    emitter.instruction("str xzr, [x12, x13, lsl #3]");                         // initialize grown slot as unset/null
    emitter.instruction("add x13, x13, #1");                                    // advance zero-fill cursor
    emitter.instruction("b __rt_spl_fixed_set_size_zero_loop");                 // continue zero-filling new slots
    emitter.label("__rt_spl_fixed_set_size_done");
    emitter.instruction("str x10, [x9]");                                       // store new logical fixed size
}

/// Emits physical resize for the x86_64 storage and its existing spills.
fn resize_x86_64(emitter: &mut impl Emitter, storage_offset: usize) {
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save requested new size
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", storage_offset)); // load current storage
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // save current storage
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load old size
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save old size
    emitter.label("__rt_spl_fixed_set_size_grow_check");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload storage
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload requested size
    emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                         // load capacity
    emitter.instruction("cmp r10, r11");                                        // does storage need more capacity?
    emitter.instruction("jle __rt_spl_fixed_set_size_release_tail");            // skip growth when capacity is enough
    emitter.instruction("mov rdi, r9");                                         // pass current storage to array_grow
    emitter.instruction("call __rt_array_grow");                                // grow fixed-array storage
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save grown storage
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload receiver
    emitter.instruction(&format!("mov QWORD PTR [r9 + {}], rax", storage_offset)); // publish grown storage on receiver
    emitter.instruction("jmp __rt_spl_fixed_set_size_grow_check");              // grow again if requested size still exceeds capacity
    emitter.label("__rt_spl_fixed_set_size_release_tail");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload storage
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload requested size
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload old size
    emitter.instruction("lea r12, [r9 + 24]");                                  // point at first storage slot
    emitter.instruction("mov r13, r10");                                        // tail-release cursor starts at new size
    emitter.label("__rt_spl_fixed_set_size_tail_loop");
    emitter.instruction("cmp r13, r11");                                        // have all truncated slots been released?
    emitter.instruction("jge __rt_spl_fixed_set_size_zero_new");                // stop tail release at old size
    emitter.instruction("mov rax, QWORD PTR [r12 + r13 * 8]");                  // load truncated Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // preserve tail-release cursor
    emitter.instruction("call __rt_decref_mixed");                              // release truncated Mixed cell if present
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload storage after release
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload requested size after release
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload old size after release
    emitter.instruction("lea r12, [r9 + 24]");                                  // restore storage slot base
    emitter.instruction("mov r13, QWORD PTR [rbp - 40]");                       // reload tail-release cursor
    emitter.instruction("mov QWORD PTR [r12 + r13 * 8], 0");                    // clear released slot
    emitter.instruction("add r13, 1");                                          // advance tail-release cursor
    emitter.instruction("jmp __rt_spl_fixed_set_size_tail_loop");               // continue releasing truncated slots
    emitter.label("__rt_spl_fixed_set_size_zero_new");
    emitter.instruction("mov r13, QWORD PTR [rbp - 32]");                       // zero-fill starts at old size
    emitter.label("__rt_spl_fixed_set_size_zero_loop");
    emitter.instruction("cmp r13, r10");                                        // have all newly exposed slots been zeroed?
    emitter.instruction("jge __rt_spl_fixed_set_size_done");                    // finish after zero-filling to requested size
    emitter.instruction("mov QWORD PTR [r12 + r13 * 8], 0");                    // initialize grown slot as unset/null
    emitter.instruction("add r13, 1");                                          // advance zero-fill cursor
    emitter.instruction("jmp __rt_spl_fixed_set_size_zero_loop");               // continue zero-filling new slots
    emitter.label("__rt_spl_fixed_set_size_done");
    emitter.instruction("mov QWORD PTR [r9], r10");                             // store new logical fixed size
}

/// Retires the old Mixed owner and publishes its staged replacement.
/// The caller has validated the index in x10 or r10 into storage in x9 or r9.
pub fn emit_replace_slot(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("add x12, x9, #24");                                    // point at first storage slot
            emitter.instruction("ldr x0, [x12, x10, lsl #3]");                          // load previous Mixed cell
            emitter.instruction("str x9, [sp, #40]");                                   // preserve storage across release
            emitter.instruction("str x10, [sp, #48]");                                  // preserve offset across release
            emitter.instruction("bl __rt_decref_mixed");                                // release previous slot value if present
            emitter.instruction("ldr x9, [sp, #40]");                                   // reload storage after release
            emitter.instruction("ldr x10, [sp, #48]");                                  // reload offset after release
            emitter.instruction("add x12, x9, #24");                                    // restore storage slot base
            emitter.instruction("ldr x13, [sp, #16]");                                  // reload owned Mixed replacement
            emitter.instruction("str x13, [x12, x10, lsl #3]");                         // store replacement Mixed cell
        }
        Arch::X86_64 => {
            emitter.instruction("lea r12, [r9 + 24]");                                  // point at first storage slot
            emitter.instruction("mov rax, QWORD PTR [r12 + r10 * 8]");                  // load previous Mixed cell
            emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // preserve storage across release
            emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // preserve offset across release
            emitter.instruction("call __rt_decref_mixed");                              // release previous slot value if present
            emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload storage after release
            emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload offset after release
            emitter.instruction("lea r12, [r9 + 24]");                                  // restore storage slot base
            emitter.instruction("mov r13, QWORD PTR [rbp - 24]");                       // reload owned Mixed replacement
            emitter.instruction("mov QWORD PTR [r12 + r10 * 8], r13");                  // store replacement Mixed cell
        }
    }
}

/// Retires the old Mixed owner and clears the slot.
/// The caller has validated the index in x10 or r10 into storage in x9 or r9.
pub fn emit_unset_slot(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("add x11, x9, #24");                                    // point at first storage slot
            emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // load existing Mixed cell
            emitter.instruction("str x11, [sp, #32]");                                  // preserve storage slot base across release
            emitter.instruction("str x10, [sp, #40]");                                  // preserve offset across release
            emitter.instruction("bl __rt_decref_mixed");                                // release existing Mixed cell if present
            emitter.instruction("ldr x11, [sp, #32]");                                  // reload storage slot base
            emitter.instruction("ldr x10, [sp, #40]");                                  // reload offset
            emitter.instruction("str xzr, [x11, x10, lsl #3]");                         // mark slot unset/null
        }
        Arch::X86_64 => {
            emitter.instruction("lea r11, [r9 + 24]");                                  // point at first storage slot
            emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8]");                  // load existing Mixed cell
            emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // preserve storage slot base across release
            emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // preserve offset across release
            emitter.instruction("call __rt_decref_mixed");                              // release existing Mixed cell if present
            emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload storage slot base
            emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload offset
            emitter.instruction("mov QWORD PTR [r11 + r10 * 8], 0");                    // mark slot unset/null
        }
    }
}
