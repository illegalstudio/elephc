//! Purpose:
//! Transfers an owned replacement payload into existing Mixed storage.
//!
//! Called from:
//! - Native writeback of values returned through eval references.
//!
//! Key details:
//! - The caller selects whether this path must retire the previous child owner.

use crate::{Arch, Emitter};

/// Moves the new payload and retires the destination's previous owner.
pub fn emit_replace(
    emitter: &mut impl Emitter,
    label_prefix: &str,
    index: usize,
    target: &str,
    replacement: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => emit_aarch64_replace_mixed_cell(emitter, label_prefix, index, target, replacement),
        Arch::X86_64 => emit_x86_64_replace_mixed_cell(emitter, label_prefix, index, target, replacement),
    }
}

/// Moves the new payload when the previous child has already been retired.
pub fn emit_replace_retired(emitter: &mut impl Emitter, target: &str, replacement: &str) {
    match emitter.arch() {
        Arch::AArch64 => emit_aarch64_replace_mixed_cell_without_releasing_old(emitter, target, replacement),
        Arch::X86_64 => emit_x86_64_replace_mixed_cell_without_releasing_old(emitter, target, replacement),
    }
}

/// Copies a replacement ARM64 Mixed cell into an existing target cell without releasing old payload.
fn emit_aarch64_replace_mixed_cell_without_releasing_old(
    emitter: &mut impl Emitter,
    target_reg: &str,
    replacement_reg: &str,
) {
    emitter.emit_push_reg_pair(target_reg, replacement_reg);
    emit_aarch64_move_replacement(emitter);
    emitter.emit_release_temporary_stack(16);
}

/// Copies a replacement x86_64 Mixed cell into an existing target cell without releasing old payload.
fn emit_x86_64_replace_mixed_cell_without_releasing_old(
    emitter: &mut impl Emitter,
    target_reg: &str,
    replacement_reg: &str,
) {
    emitter.emit_push_reg_pair(target_reg, replacement_reg);
    emit_x86_64_move_replacement(emitter);
    emitter.emit_release_temporary_stack(16);
}

/// Copies one replacement ARM64 Mixed cell payload into an existing target cell.
fn emit_aarch64_replace_mixed_cell(
    emitter: &mut impl Emitter,
    label_prefix: &str,
    param_index: usize,
    target_reg: &str,
    replacement_reg: &str,
) {
    let release_string = format!("{}_ref_{}_release_string", label_prefix, param_index);
    let copy_new = format!("{}_ref_{}_copy_new", label_prefix, param_index);
    let done = format!("{}_ref_{}_assign_done", label_prefix, param_index);

    emitter.emit_push_reg_pair(target_reg, replacement_reg);
    emitter.instruction("ldr x9, [sp]");                                        // reload the original Mixed cell pointer for old-payload inspection
    emitter.instruction("ldr x11, [x9]");                                       // inspect the old payload tag before overwriting the cell
    emitter.instruction("cmp x11, #1");                                         // strings own a persisted heap payload that needs safe free
    emitter.instruction(&format!("b.eq {}", release_string));                   // release string payloads through the string-safe free path
    emitter.instruction("cmp x11, #4");                                         // tags below array/hash/object/mixed are scalar payloads
    emitter.instruction(&format!("b.lo {}", copy_new));                         // scalar payloads can be overwritten directly
    emitter.instruction("cmp x11, #7");                                         // tags above the refcounted payload range are not released here
    emitter.instruction(&format!("b.hi {}", copy_new));                         // unknown/null payload tags can be overwritten directly
    emitter.instruction("ldr x0, [x9, #8]");                                    // pass the old refcounted child payload to the generic release helper
    emitter.emit_call_label("__rt_decref_any");
    emitter.instruction(&format!("b {}", copy_new));                            // continue with replacement after releasing the old child
    emitter.label(&release_string);
    emitter.instruction("ldr x9, [sp]");                                        // reload the original Mixed cell before reading its string payload
    emitter.instruction("ldr x0, [x9, #8]");                                    // pass the old string payload pointer to the safe free helper
    emitter.emit_call_label("__rt_heap_free_safe");
    emitter.label(&copy_new);
    emit_aarch64_move_replacement(emitter);
    emitter.label(&done);
    emitter.emit_release_temporary_stack(16);
}

/// Copies one replacement x86_64 Mixed cell payload into an existing target cell.
fn emit_x86_64_replace_mixed_cell(
    emitter: &mut impl Emitter,
    label_prefix: &str,
    param_index: usize,
    target_reg: &str,
    replacement_reg: &str,
) {
    let release_string = format!("{}_ref_{}_release_string_x", label_prefix, param_index);
    let copy_new = format!("{}_ref_{}_copy_new_x", label_prefix, param_index);
    let done = format!("{}_ref_{}_assign_done_x", label_prefix, param_index);

    emitter.emit_push_reg_pair(target_reg, replacement_reg);
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // reload the original Mixed cell pointer for old-payload inspection
    emitter.instruction("mov r9, QWORD PTR [r10]");                             // inspect the old payload tag before overwriting the cell
    emitter.instruction("cmp r9, 1");                                           // strings own a persisted heap payload that needs safe free
    emitter.instruction(&format!("je {}", release_string));                     // release string payloads through the string-safe free path
    emitter.instruction("cmp r9, 4");                                           // tags below array/hash/object/mixed are scalar payloads
    emitter.instruction(&format!("jl {}", copy_new));                           // scalar payloads can be overwritten directly
    emitter.instruction("cmp r9, 7");                                           // tags above the refcounted payload range are not released here
    emitter.instruction(&format!("jg {}", copy_new));                           // unknown/null payload tags can be overwritten directly
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // pass the old refcounted child payload to the generic release helper
    emitter.emit_call_label("__rt_decref_any");
    emitter.instruction(&format!("jmp {}", copy_new));                          // continue with replacement after releasing the old child
    emitter.label(&release_string);
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // reload the original Mixed cell before reading its string payload
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // pass the old string payload pointer to the safe free helper
    emitter.emit_call_label("__rt_heap_free_safe");
    emitter.label(&copy_new);
    emit_x86_64_move_replacement(emitter);
    emitter.label(&done);
    emitter.emit_release_temporary_stack(16);
}

/// Transfers the staged payload and frees only the replacement box on aarch64.
fn emit_aarch64_move_replacement(emitter: &mut impl Emitter) {
    emitter.instruction("ldr x9, [sp]");                                        // reload the original Mixed cell pointer for direct replacement
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the replacement Mixed cell pointer
    emitter.instruction("ldr x11, [x10]");                                      // copy the replacement runtime tag
    emitter.instruction("str x11, [x9]");                                       // overwrite the target cell tag
    emitter.instruction("ldr x11, [x10, #8]");                                  // copy the replacement low payload word
    emitter.instruction("str x11, [x9, #8]");                                   // overwrite the target cell low payload word
    emitter.instruction("ldr x11, [x10, #16]");                                 // copy the replacement high payload word
    emitter.instruction("str x11, [x9, #16]");                                  // overwrite the target cell high payload word
    emitter.instruction("mov x0, x10");                                         // pass the now-empty replacement cell storage to heap_free
    emitter.emit_call_label("__rt_heap_free");
}

/// Transfers the staged payload and frees only the replacement box on x86_64.
fn emit_x86_64_move_replacement(emitter: &mut impl Emitter) {
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // reload the original Mixed cell pointer for direct replacement
    emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                        // reload the replacement Mixed cell pointer
    emitter.instruction("mov r9, QWORD PTR [r11]");                             // copy the replacement runtime tag
    emitter.instruction("mov QWORD PTR [r10], r9");                             // overwrite the target cell tag
    emitter.instruction("mov r9, QWORD PTR [r11 + 8]");                         // copy the replacement low payload word
    emitter.instruction("mov QWORD PTR [r10 + 8], r9");                         // overwrite the target cell low payload word
    emitter.instruction("mov r9, QWORD PTR [r11 + 16]");                        // copy the replacement high payload word
    emitter.instruction("mov QWORD PTR [r10 + 16], r9");                        // overwrite the target cell high payload word
    emitter.instruction("mov rax, r11");                                        // pass the now-empty replacement cell storage to heap_free
    emitter.emit_call_label("__rt_heap_free");
}
