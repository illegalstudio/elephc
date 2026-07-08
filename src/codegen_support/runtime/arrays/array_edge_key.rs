//! Purpose:
//! Emits the `__rt_array_edge_key` runtime helper assembly for array_key_first / array_key_last.
//! Returns the first or last key of a PHP array boxed as a Mixed cell.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The key is boxed through `__rt_mixed_from_value` (tail call); empty containers yield a boxed null.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// array_edge_key: box the first or last key of a container as a Mixed cell.
/// Input:  x0 = container pointer (indexed array, hash, or boxed mixed cell)
///         x1 = which (0 = first key, 1 = last key)
/// Output: x0 = boxed Mixed key, or boxed null when the container is empty / not an array
///
/// Tail-calls `__rt_mixed_from_value` so the boxed result is returned directly to the
/// original caller. Integer keys box with tag 0, string keys with tag 1 (the string is
/// persisted by the box helper), and empty/non-array inputs box with the null tag 8.
pub fn emit_array_edge_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_edge_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_edge_key ---");
    emitter.label_global("__rt_array_edge_key");
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the uniform heap-kind header word
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low-byte heap kind
    emitter.instruction("cmp x9, #5");                                          // is the container a boxed mixed cell?
    emitter.instruction("b.eq __rt_array_edge_key_mixed");                      // mixed cells are unwrapped first
    emitter.instruction("cmp x9, #2");                                          // is the container an indexed array?
    emitter.instruction("b.eq __rt_array_edge_key_indexed");                    // indexed arrays use positional keys
    emitter.instruction("cmp x9, #3");                                          // is the container an associative hash?
    emitter.instruction("b.eq __rt_array_edge_key_hash");                       // hashes read the head/tail slot
    emitter.instruction("b __rt_array_edge_key_null");                          // any other kind has no key
    emitter.label("__rt_array_edge_key_indexed");
    emitter.instruction("ldr x10, [x0, #0]");                                   // x10 = element count from the array header
    emitter.instruction("cbz x10, __rt_array_edge_key_null");                   // empty arrays have no key
    emitter.instruction("cbz x1, __rt_array_edge_key_idx_first");               // which == 0 selects the first index
    emitter.instruction("sub x10, x10, #1");                                    // last index = element count - 1
    emitter.instruction("mov x1, x10");                                         // value_lo = last index
    emitter.instruction("mov x0, #0");                                          // value_tag = 0 (integer)
    emitter.instruction("mov x2, #0");                                          // value_hi unused for integers
    emitter.instruction("b __rt_mixed_from_value");                             // box the integer key and return it to the caller
    emitter.label("__rt_array_edge_key_idx_first");
    emitter.instruction("mov x0, #0");                                          // value_tag = 0 (integer)
    emitter.instruction("mov x1, #0");                                          // value_lo = first index 0
    emitter.instruction("mov x2, #0");                                          // value_hi unused for integers
    emitter.instruction("b __rt_mixed_from_value");                             // box the integer key and return it to the caller
    emitter.label("__rt_array_edge_key_hash");
    emitter.instruction("cbz x1, __rt_array_edge_key_hash_head");               // which == 0 selects the insertion-order head
    emitter.instruction("ldr x11, [x0, #32]");                                  // x11 = tail slot index
    emitter.instruction("b __rt_array_edge_key_hash_slot");                     // load the selected entry
    emitter.label("__rt_array_edge_key_hash_head");
    emitter.instruction("ldr x11, [x0, #24]");                                  // x11 = head slot index
    emitter.label("__rt_array_edge_key_hash_slot");
    emitter.instruction("cmn x11, #1");                                         // is the selected slot empty (index == -1)?
    emitter.instruction("b.eq __rt_array_edge_key_null");                       // empty hashes have no key
    emitter.instruction("mov x12, #64");                                        // hash entry stride in bytes
    emitter.instruction("mul x12, x11, x12");                                   // byte offset of the selected slot
    emitter.instruction("add x12, x0, x12");                                    // advance from the hash base to the slot
    emitter.instruction("add x12, x12, #40");                                   // skip the 40-byte hash header
    emitter.instruction("ldr x13, [x12, #16]");                                 // x13 = key_len (-1 marks an integer key)
    emitter.instruction("ldr x14, [x12, #8]");                                  // x14 = key payload (integer value or string pointer)
    emitter.instruction("cmn x13, #1");                                         // is the entry keyed by an integer?
    emitter.instruction("b.eq __rt_array_edge_key_int");                        // integer keys box with tag 0
    emitter.instruction("mov x0, #1");                                          // value_tag = 1 (string)
    emitter.instruction("mov x1, x14");                                         // value_lo = key string pointer
    emitter.instruction("mov x2, x13");                                         // value_hi = key string length
    emitter.instruction("b __rt_mixed_from_value");                             // box (and persist) the string key and return it
    emitter.label("__rt_array_edge_key_int");
    emitter.instruction("mov x0, #0");                                          // value_tag = 0 (integer)
    emitter.instruction("mov x1, x14");                                         // value_lo = integer key
    emitter.instruction("mov x2, #0");                                          // value_hi unused for integers
    emitter.instruction("b __rt_mixed_from_value");                             // box the integer key and return it to the caller
    emitter.label("__rt_array_edge_key_mixed");
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed mixed value tag
    emitter.instruction("cmp x9, #4");                                          // does the cell box an indexed array?
    emitter.instruction("b.eq __rt_array_edge_key_unwrap");                     // unwrap indexed array payloads
    emitter.instruction("cmp x9, #5");                                          // does the cell box an associative array?
    emitter.instruction("b.eq __rt_array_edge_key_unwrap");                     // unwrap associative array payloads
    emitter.instruction("b __rt_array_edge_key_null");                          // non-array mixed payloads have no key
    emitter.label("__rt_array_edge_key_unwrap");
    emitter.instruction("ldr x0, [x0, #8]");                                    // unbox the container pointer from mixed[8]
    emitter.instruction("b __rt_array_edge_key");                               // re-dispatch with the same which selector in x1
    emitter.label("__rt_array_edge_key_null");
    emitter.instruction("mov x0, #8");                                          // value_tag = 8 (null)
    emitter.instruction("movz x1, #0xFFFE");                                    // value_lo = null sentinel bits [15:0]
    emitter.instruction("movk x1, #0xFFFF, lsl #16");                           // value_lo = null sentinel bits [31:16]
    emitter.instruction("movk x1, #0xFFFF, lsl #32");                           // value_lo = null sentinel bits [47:32]
    emitter.instruction("movk x1, #0x7FFF, lsl #48");                           // value_lo = null sentinel bits [63:48] = 0x7FFFFFFFFFFFFFFE
    emitter.instruction("mov x2, #0");                                          // value_hi unused
    emitter.instruction("b __rt_mixed_from_value");                             // box the null sentinel and return it to the caller
}

/// x86_64 Linux implementation of `__rt_array_edge_key`.
/// Input:  rdi = container pointer, rsi = which (0 = first, 1 = last)
/// Output: rax = boxed Mixed key (tail-call result of `__rt_mixed_from_value`)
fn emit_array_edge_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_edge_key ---");
    emitter.label_global("__rt_array_edge_key");
    emitter.instruction("movzx eax, BYTE PTR [rdi - 8]");                       // load the low-byte heap kind from the uniform header
    emitter.instruction("cmp eax, 5");                                          // is the container a boxed mixed cell?
    emitter.instruction("je __rt_array_edge_key_mixed");                        // mixed cells are unwrapped first
    emitter.instruction("cmp eax, 2");                                          // is the container an indexed array?
    emitter.instruction("je __rt_array_edge_key_indexed");                      // indexed arrays use positional keys
    emitter.instruction("cmp eax, 3");                                          // is the container an associative hash?
    emitter.instruction("je __rt_array_edge_key_hash");                         // hashes read the head/tail slot
    emitter.instruction("jmp __rt_array_edge_key_null");                        // any other kind has no key
    emitter.label("__rt_array_edge_key_indexed");
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // r10 = element count from the array header
    emitter.instruction("test r10, r10");                                       // is the array empty?
    emitter.instruction("je __rt_array_edge_key_null");                         // empty arrays have no key
    emitter.instruction("test rsi, rsi");                                       // which == 0 selects the first index?
    emitter.instruction("je __rt_array_edge_key_idx_first");                    // box the first index
    emitter.instruction("sub r10, 1");                                          // last index = element count - 1
    emitter.instruction("mov rdi, r10");                                        // value_lo = last index
    emitter.instruction("xor esi, esi");                                        // value_hi unused for integers
    emitter.instruction("mov rax, 0");                                          // value_tag = 0 (integer)
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the integer key and return it to the caller
    emitter.label("__rt_array_edge_key_idx_first");
    emitter.instruction("xor edi, edi");                                        // value_lo = first index 0
    emitter.instruction("xor esi, esi");                                        // value_hi unused for integers
    emitter.instruction("mov rax, 0");                                          // value_tag = 0 (integer)
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the integer key and return it to the caller
    emitter.label("__rt_array_edge_key_hash");
    emitter.instruction("test rsi, rsi");                                       // which == 0 selects the insertion-order head?
    emitter.instruction("je __rt_array_edge_key_hash_head");                    // load the head slot
    emitter.instruction("mov r11, QWORD PTR [rdi + 32]");                       // r11 = tail slot index
    emitter.instruction("jmp __rt_array_edge_key_hash_slot");                   // load the selected entry
    emitter.label("__rt_array_edge_key_hash_head");
    emitter.instruction("mov r11, QWORD PTR [rdi + 24]");                       // r11 = head slot index
    emitter.label("__rt_array_edge_key_hash_slot");
    emitter.instruction("cmp r11, -1");                                         // is the selected slot empty (index == -1)?
    emitter.instruction("je __rt_array_edge_key_null");                         // empty hashes have no key
    emitter.instruction("mov rcx, r11");                                        // copy the slot index before scaling it
    emitter.instruction("shl rcx, 6");                                          // convert the slot index into a 64-byte entry offset
    emitter.instruction("add rcx, rdi");                                        // advance from the hash base to the slot
    emitter.instruction("add rcx, 40");                                         // skip the 40-byte hash header
    emitter.instruction("mov r8, QWORD PTR [rcx + 16]");                        // r8 = key_len (-1 marks an integer key)
    emitter.instruction("mov r9, QWORD PTR [rcx + 8]");                         // r9 = key payload (integer value or string pointer)
    emitter.instruction("cmp r8, -1");                                          // is the entry keyed by an integer?
    emitter.instruction("je __rt_array_edge_key_int");                          // integer keys box with tag 0
    emitter.instruction("mov rdi, r9");                                         // value_lo = key string pointer
    emitter.instruction("mov rsi, r8");                                         // value_hi = key string length
    emitter.instruction("mov rax, 1");                                          // value_tag = 1 (string)
    emitter.instruction("jmp __rt_mixed_from_value");                           // box (and persist) the string key and return it
    emitter.label("__rt_array_edge_key_int");
    emitter.instruction("mov rdi, r9");                                         // value_lo = integer key
    emitter.instruction("xor esi, esi");                                        // value_hi unused for integers
    emitter.instruction("mov rax, 0");                                          // value_tag = 0 (integer)
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the integer key and return it to the caller
    emitter.label("__rt_array_edge_key_mixed");
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the boxed mixed value tag
    emitter.instruction("cmp rax, 4");                                          // does the cell box an indexed array?
    emitter.instruction("je __rt_array_edge_key_unwrap");                       // unwrap indexed array payloads
    emitter.instruction("cmp rax, 5");                                          // does the cell box an associative array?
    emitter.instruction("je __rt_array_edge_key_unwrap");                       // unwrap associative array payloads
    emitter.instruction("jmp __rt_array_edge_key_null");                        // non-array mixed payloads have no key
    emitter.label("__rt_array_edge_key_unwrap");
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // unbox the container pointer from mixed[8]
    emitter.instruction("jmp __rt_array_edge_key");                             // re-dispatch with the same which selector in rsi
    emitter.label("__rt_array_edge_key_null");
    emitter.instruction("mov rdi, 0x7ffffffffffffffe");                         // value_lo = shared null sentinel
    emitter.instruction("xor esi, esi");                                        // value_hi unused
    emitter.instruction("mov rax, 8");                                          // value_tag = 8 (null)
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the null sentinel and return it to the caller
}

