//! Purpose:
//! Acquires and retires the two independently owned string pairs in a registration row.
//!
//! Called from:
//! - The compiler's stream wrapper and filter registration and retirement emitters.
//!
//! Key details:
//! - Scratch storage holds the two detached pointers across release calls.

use crate::{Arch, Emitter, TargetAbi};

/// Persists and publishes two independent string pairs into the selected row.
/// ARM64 takes both pairs in registers. The x86-64 wrapper has staged the second pair.
pub fn emit_register_row(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("str x6, [sp, #16]");                           // save the free slot base across the persist calls
            emitter.instruction("str x2, [sp, #24]");                           // save the second string pointer across the name persist
            emitter.instruction("str x3, [sp, #32]");                           // save the second string length across the name persist
            emitter.instruction("mov x2, x1");                                  // name length → persist length input
            emitter.instruction("mov x1, x0");                                  // name pointer → persist pointer input
            emitter.instruction("bl __rt_str_persist");                         // x1 = owned name bytes, x2 = length
            emitter.instruction("ldr x6, [sp, #16]");                           // reload the free slot base
            emitter.instruction("str x1, [x6]");                                // owned name pointer
            emitter.instruction("str x2, [x6, #8]");                            // name length
            emitter.instruction("ldr x1, [sp, #24]");                           // second string pointer → persist pointer input
            emitter.instruction("ldr x2, [sp, #32]");                           // second string length → persist length input
            emitter.instruction("bl __rt_str_persist");                         // x1 = owned class name, x2 = length
            emitter.instruction("ldr x6, [sp, #16]");                           // reload the free slot base
            emitter.instruction("str x1, [x6, #16]");                           // owned second string pointer
            emitter.instruction("str x2, [x6, #24]");                           // second string length
        }
        Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [rbp - 8], r10");                // save the free slot base across the persist calls
            emit_store_staged_pairs_x86_64(emitter);
        }
    }
}

/// Publishes the filter row with the second x86-64 input pair still in registers.
pub fn emit_register_row_with_live_second_pair(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::AArch64 {
        emit_register_row(emitter);
        return;
    }
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save the free slot base across the persist calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the second string pointer across the filter-name persist
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the second string length across the filter-name persist
    emit_store_staged_pairs_x86_64(emitter);
}

/// Copies both input pairs into the row, using the existing x86-64 staging slots.
fn emit_store_staged_pairs_x86_64(emitter: &mut impl Emitter) {
    emitter.instruction("mov rax, rdi");                                        // name pointer → persist pointer input
    emitter.instruction("mov rdx, rsi");                                        // name length → persist length input
    emitter.instruction("call __rt_str_persist");                               // rax = owned name bytes, rdx = length
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the free slot base
    emitter.instruction("mov QWORD PTR [r10], rax");                            // owned name pointer
    emitter.instruction("mov QWORD PTR [r10 + 8], rdx");                        // name length
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // second string pointer → persist pointer input
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // second string length → persist length input
    emitter.instruction("call __rt_str_persist");                               // rax = owned class name, rdx = length
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the free slot base
    emitter.instruction("mov QWORD PTR [r10 + 16], rax");                       // owned second string pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], rdx");                       // second string length
}

/// Clears a two-string row and releases both detached pointers.
pub fn emit_retire_row(emitter: &mut impl TargetAbi, row: &str, frame: &str, scratch: &str) {
    for (source, destination) in [(0, 0), (16, 8)] {
        emitter.emit_load_from_address(scratch, row, source);
        emitter.emit_store_to_address(scratch, frame, destination);
    }
    for offset in [0, 8, 16, 24] {
        emitter.emit_store_zero_to_address(row, offset);
    }
    // -- release each transferred owner after the row no longer publishes it --
    for offset in [0, 8] {
        emitter.emit_load_from_address(row, frame, offset);
        emitter.emit_call_label("__rt_heap_free_safe");
    }
}
