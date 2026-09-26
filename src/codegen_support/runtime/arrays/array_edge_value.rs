//! Purpose:
//! Emits the `__rt_array_edge_value` runtime helper assembly for PHP 8.4's
//! `array_first` / `array_last`. Returns the first or last VALUE of a PHP array, in
//! insertion order, boxed as a Mixed cell.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - A LEAF routine: the shared internal-pointer normalization loop unwraps boxed Mixed
//!   cells inline, and every exit is a tail jump, so no stack frame is built.
//! - Empty containers and non-array Mixed payloads box `null` (tag 8), never `false`:
//!   that is the one difference from `current()`/`reset()`/`end()`.
//! - Boxing is delegated to the same audited ownership paths as `current()`: indexed
//!   storage tail calls `__rt_array_get_mixed_key` (which understands every indexed
//!   `value_type`), hash storage tail calls `__rt_mixed_from_value` (which retains
//!   containers and persists strings). The returned cell is therefore independently owned.
//! - Hashes read the header's insertion-order head (`[+24]`) or tail (`[+32]`) slot
//!   directly, so both selectors are `O(1)`; a PHP reference entry (tag 11) is
//!   dereferenced so the caller receives the value, not the reference cell.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

use super::array_internal_pointer::{emit_normalize_aarch64, emit_normalize_x86_64};

/// array_edge_value: box the first or last value of a container as a Mixed cell.
/// Input:  x0 = container pointer (indexed array, hash, or boxed mixed cell)
///         x1 = which (0 = first value, 1 = last value)
/// Output: x0 = boxed Mixed value, or boxed null when the container is empty / not an array
pub fn emit_array_edge_value(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_edge_value_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_edge_value ---");
    emitter.label_global("__rt_array_edge_value");
    emit_normalize_aarch64(emitter, "__rt_aedge_val", "__rt_aedge_val_null");
    emitter.instruction("cbz x11, __rt_aedge_val_null");                        // an empty container has no edge value
    emitter.instruction("cmp x12, #3");                                         // is the container an associative hash?
    emitter.instruction("b.eq __rt_aedge_val_hash");                            // hashes read the head/tail slot directly
    emitter.instruction("cbz x1, __rt_aedge_val_idx");                          // which == 0 keeps x1 = 0, the first index
    emitter.instruction("sub x1, x11, #1");                                     // last index = element count - 1
    emitter.label("__rt_aedge_val_idx");
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer indexed key
    emitter.instruction("mov x3, #0");                                          // never warn: the index was already bounds-checked
    emitter.instruction("b __rt_array_get_mixed_key");                          // reuse the ordinary indexed read path and return its box
    emitter.label("__rt_aedge_val_hash");
    emitter.instruction("cbz x1, __rt_aedge_val_head");                         // which == 0 selects the insertion-order head
    emitter.instruction("ldr x9, [x0, #32]");                                   // x9 = insertion-order tail slot index
    emitter.instruction("b __rt_aedge_val_slot");                               // load the selected entry
    emitter.label("__rt_aedge_val_head");
    emitter.instruction("ldr x9, [x0, #24]");                                   // x9 = insertion-order head slot index
    emitter.label("__rt_aedge_val_slot");
    emitter.instruction("cmn x9, #1");                                          // is the selected slot empty (index == -1)?
    emitter.instruction("b.eq __rt_aedge_val_null");                            // an inconsistent empty chain has no edge value
    emitter.instruction("mov x10, #64");                                        // x10 = hash entry stride in bytes
    emitter.instruction("mul x10, x9, x10");                                    // byte offset of the selected slot
    emitter.instruction("add x10, x0, x10");                                    // advance from the hash base to the slot
    emitter.instruction("add x10, x10, #40");                                   // skip the 40-byte hash header
    emitter.instruction("ldr x9, [x10, #24]");                                  // x9 = value_lo from the hash entry
    emitter.instruction("ldr x13, [x10, #32]");                                 // x13 = value_hi from the hash entry
    emitter.instruction("ldr x14, [x10, #40]");                                 // x14 = value_tag from the hash entry
    emitter.instruction("mov x0, x14");                                         // value_tag = the entry's runtime tag
    emitter.instruction("mov x1, x9");                                          // value_lo = the entry's low payload word
    emitter.instruction("mov x2, x13");                                         // value_hi = the entry's high payload word
    super::hash_entry_reference::emit_inline_entry_deref(emitter, "__rt_aedge_val_deref_done", "x0", "x1", "x2");
    emitter.instruction("b __rt_mixed_from_value");                             // retain/persist the payload and return the box
    emitter.label("__rt_aedge_val_null");
    emitter.instruction("mov x0, #8");                                          // value_tag = 8 (null)
    emitter.instruction("mov x1, #0");                                          // canonical null has no low payload word
    emitter.instruction("mov x2, #0");                                          // value_hi unused
    emitter.instruction("b __rt_mixed_from_value");                             // box canonical null and return it to the caller
}

/// x86_64 Linux implementation of `__rt_array_edge_value`.
/// Input:  rdi = container pointer, rsi = which (0 = first value, 1 = last value)
/// Output: rax = boxed Mixed value, or boxed null when the container is empty / not an array
fn emit_array_edge_value_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_edge_value ---");
    emitter.label_global("__rt_array_edge_value");
    emit_normalize_x86_64(emitter, "__rt_aedge_val", "__rt_aedge_val_null");
    emitter.instruction("test r11, r11");                                       // is the container empty?
    emitter.instruction("je __rt_aedge_val_null");                              // an empty container has no edge value
    emitter.instruction("movzx eax, BYTE PTR [rdi - 8]");                       // reload the low-byte heap kind after normalization
    emitter.instruction("cmp eax, 3");                                          // is the container an associative hash?
    emitter.instruction("je __rt_aedge_val_hash");                              // hashes read the head/tail slot directly
    emitter.instruction("test rsi, rsi");                                       // which == 0 keeps rsi = 0, the first index?
    emitter.instruction("je __rt_aedge_val_idx");                               // read index 0
    emitter.instruction("lea rsi, [r11 - 1]");                                  // last index = element count - 1
    emitter.label("__rt_aedge_val_idx");
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer indexed key
    emitter.instruction("xor ecx, ecx");                                        // never warn: the index was already bounds-checked
    emitter.instruction("jmp __rt_array_get_mixed_key");                        // reuse the ordinary indexed read path and return its box
    emitter.label("__rt_aedge_val_hash");
    emitter.instruction("test rsi, rsi");                                       // which == 0 selects the insertion-order head?
    emitter.instruction("je __rt_aedge_val_head");                              // load the head slot
    emitter.instruction("mov rax, QWORD PTR [rdi + 32]");                       // rax = insertion-order tail slot index
    emitter.instruction("jmp __rt_aedge_val_slot");                             // load the selected entry
    emitter.label("__rt_aedge_val_head");
    emitter.instruction("mov rax, QWORD PTR [rdi + 24]");                       // rax = insertion-order head slot index
    emitter.label("__rt_aedge_val_slot");
    emitter.instruction("cmp rax, -1");                                         // is the selected slot empty (index == -1)?
    emitter.instruction("je __rt_aedge_val_null");                              // an inconsistent empty chain has no edge value
    emitter.instruction("mov r10, rax");                                        // copy the slot index before scaling it
    emitter.instruction("shl r10, 6");                                          // convert the slot index into a 64-byte entry offset
    emitter.instruction("add r10, rdi");                                        // advance from the hash base to the slot
    emitter.instruction("add r10, 40");                                         // skip the 40-byte hash header
    emitter.instruction("mov r8, QWORD PTR [r10 + 24]");                        // r8 = value_lo from the hash entry
    emitter.instruction("mov r9, QWORD PTR [r10 + 32]");                        // r9 = value_hi from the hash entry
    emitter.instruction("mov rax, QWORD PTR [r10 + 40]");                       // rax = value_tag from the hash entry
    emitter.instruction("mov rdi, r8");                                         // value_lo = the entry's low payload word
    emitter.instruction("mov rsi, r9");                                         // value_hi = the entry's high payload word
    super::hash_entry_reference::emit_inline_entry_deref(emitter, "__rt_aedge_val_deref_done", "rax", "rdi", "rsi");
    emitter.instruction("jmp __rt_mixed_from_value");                           // retain/persist the payload and return the box
    emitter.label("__rt_aedge_val_null");
    emitter.instruction("xor edi, edi");                                        // canonical null has no low payload word
    emitter.instruction("xor esi, esi");                                        // value_hi unused
    emitter.instruction("mov rax, 8");                                          // value_tag = 8 (null)
    emitter.instruction("jmp __rt_mixed_from_value");                           // box canonical null and return it to the caller
}
