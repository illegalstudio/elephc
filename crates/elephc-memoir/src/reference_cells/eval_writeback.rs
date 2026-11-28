//! Purpose:
//! Transfers an owned replacement payload into existing Mixed storage.
//!
//! Called from:
//! - Native writeback through eval references and AOT assignment into a live Mixed cell.
//!
//! Key details:
//! - The caller selects whether this path must retire the previous child owner.

use crate::{Arch, Emitter};

/// Finds the payload and original-cell words of one 32-byte eval staging slot.
pub const fn argument_slot_offsets(reverse_index: usize) -> (usize, usize) {
    let payload = reverse_index * 32;
    (payload, payload + 16)
}

/// Compares a staged heap pointer to the original Mixed cell's current payload.
pub fn emit_branch_on_argument_change(
    emitter: &mut impl Emitter, raw: usize, original: usize, changed: &str, unchanged: &str,
) {
    let (cell, candidate, previous, compare, difference, jump) = match emitter.arch() {
        Arch::AArch64 => ("x9", "x10", "x11", "cmp x10, x11", "b.ne", "b"),
        Arch::X86_64 => ("r10", "r11", "r9", "cmp r11, r9", "jne", "jmp"),
    };
    emitter.emit_load_temporary_stack_slot(cell, original);
    emitter.emit_load_temporary_stack_slot(candidate, raw);
    emitter.emit_load_from_address(previous, cell, 8);
    emitter.instruction(compare);                                               // compare native storage to the cell's current payload
    emitter.instruction(&format!("{difference} {changed}"));                    // the raw slot owns a different pointer
    emitter.instruction(&format!("{jump} {unchanged}"));                        // an unchanged slot still has the original pointer
}

/// Retires a raw argument after failure, releasing an unchanged value only if independently owned.
pub fn emit_retire_argument_value<E: Emitter>(
    emitter: &mut E, raw: usize, original: usize, owned: bool, release: &str, done: &str,
    emit_release: impl FnOnce(&mut E),
) {
    if !owned {
        emit_branch_on_argument_change(emitter, raw, original, release, done);
    }
    emitter.label(release);
    emitter.emit_load_temporary_stack_slot(emitter.int_result_reg(), raw);
    emit_release(emitter);
    emitter.label(done);
}

/// Moves the new payload and retires the previous owner of a known destination.
pub fn emit_replace(
    emitter: &mut impl Emitter,
    label_prefix: &str,
    index: usize,
    target: &str,
    replacement: &str,
) {
    let suffix = match emitter.arch() {
        Arch::AArch64 => "",
        Arch::X86_64 => "_x",
    };
    let release_string = format!("{label_prefix}_ref_{index}_release_string{suffix}");
    let copy_new = format!("{label_prefix}_ref_{index}_copy_new{suffix}");
    let done = format!("{label_prefix}_ref_{index}_assign_done{suffix}");

    emitter.emit_push_reg_pair(target, replacement);
    emit_replace_staged(emitter, &release_string, &copy_new, None);
    emitter.label(&done);
    emitter.emit_release_temporary_stack(16);
}

/// Moves the new payload when the previous child has already been retired.
pub fn emit_replace_retired(emitter: &mut impl Emitter, target: &str, replacement: &str) {
    emitter.emit_push_reg_pair(target, replacement);
    match emitter.arch() {
        Arch::AArch64 => emit_aarch64_move_replacement(emitter),
        Arch::X86_64 => emit_x86_64_move_replacement(emitter),
    }
    emitter.emit_release_temporary_stack(16);
}

/// Consumes the staged replacement even when the destination pointer is null.
/// The caller owns the stack record. Destination and replacement occupy its first two words.
pub fn emit_replace_staged_if_present(
    emitter: &mut impl Emitter,
    release_string: &str,
    copy_new: &str,
    drop_new: &str,
    done: &str,
) {
    emit_replace_staged(emitter, release_string, copy_new, Some(drop_new));
    emitter.emit_jump(done);
    emitter.label(drop_new);
    let result = emitter.int_result_reg();
    emitter.emit_load_temporary_stack_slot(result, 8);
    emitter.emit_call_label("__rt_decref_mixed");
    emitter.label(done);
}

/// Dispatches a staged replacement through the target's Mixed storage convention.
fn emit_replace_staged(
    emitter: &mut impl Emitter,
    release_string: &str,
    copy_new: &str,
    drop_new: Option<&str>,
) {
    match emitter.arch() {
        Arch::AArch64 => emit_aarch64_replace_staged(emitter, release_string, copy_new, drop_new),
        Arch::X86_64 => emit_x86_64_replace_staged(emitter, release_string, copy_new, drop_new),
    }
}

/// Copies one replacement ARM64 Mixed cell payload into an existing target cell.
fn emit_aarch64_replace_staged(
    emitter: &mut impl Emitter,
    release_string: &str,
    copy_new: &str,
    drop_new: Option<&str>,
) {
    emitter.instruction("ldr x9, [sp]");                                        // reload the original Mixed cell pointer for old-payload inspection
    if let Some(drop_new) = drop_new {
        emitter.instruction(&format!("cbz x9, {drop_new}"));                    // discard the replacement if the destination is absent
    }
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
    emitter.label(release_string);
    emitter.instruction("ldr x9, [sp]");                                        // reload the original Mixed cell before reading its string payload
    emitter.instruction("ldr x0, [x9, #8]");                                    // pass the old string payload pointer to the safe free helper
    emitter.emit_call_label("__rt_heap_free_safe");
    emitter.label(copy_new);
    emit_aarch64_move_replacement(emitter);
}

/// Copies one replacement x86_64 Mixed cell payload into an existing target cell.
fn emit_x86_64_replace_staged(
    emitter: &mut impl Emitter,
    release_string: &str,
    copy_new: &str,
    drop_new: Option<&str>,
) {
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // reload the original Mixed cell pointer for old-payload inspection
    if let Some(drop_new) = drop_new {
        emitter.instruction("test r10, r10");                                   // inspect the optional destination pointer
        emitter.instruction(&format!("jz {drop_new}"));                         // discard the replacement if the destination is absent
    }
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
    emitter.label(release_string);
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // reload the original Mixed cell before reading its string payload
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // pass the old string payload pointer to the safe free helper
    emitter.emit_call_label("__rt_heap_free_safe");
    emitter.label(copy_new);
    emit_x86_64_move_replacement(emitter);
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
