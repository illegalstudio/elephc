//! Purpose:
//! Emits the `__rt_array_slice_preserve` runtime helper assembly for `array_slice($a, $off, $len, true)`.
//! Builds an integer-keyed associative result that preserves the source indexed array's original keys.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Mirrors `__rt_array_slice`'s offset/length normalization, then builds a hash like
//!   `__rt_array_flip` (integer keys via the `key_hi == -1` sentinel) where each element keeps its
//!   original index `offset + i` as the key. Scalar (8-byte) values only; the source element tag is
//!   carried through so the boxed hash values read back as the correct type.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_array_slice_preserve` runtime helper.
///
/// Input:  x0 = source indexed array, x1 = offset, x2 = length (-1 = until end), x3 = value_type tag
/// Output: x0 = new hash table keyed by the preserved original integer indices
pub fn emit_array_slice_preserve(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_slice_preserve_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_slice_preserve ---");
    emitter.label_global("__rt_array_slice_preserve");

    // Stack layout:
    //   [sp, #0]  = source array pointer
    //   [sp, #8]  = value_type tag
    //   [sp, #16] = normalized offset
    //   [sp, #24] = clamped slice length
    //   [sp, #32] = result hash pointer
    //   [sp, #40] = loop index i
    //   [sp, #48] = saved x29
    //   [sp, #56] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate the slice-preserve stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("str x3, [sp, #8]");                                    // save the source element value_type tag
    emitter.instruction("ldr x9, [x0]");                                        // x9 = source array length

    // -- normalize a negative offset to a positive index (clamped to 0) --
    emitter.instruction("cmp x1, #0");                                          // is the requested offset negative?
    emitter.instruction("b.ge __rt_aslp_pos_off");                              // skip adjustment when the offset is already non-negative
    emitter.instruction("add x1, x9, x1");                                      // offset = length + offset for negative offsets
    emitter.instruction("cmp x1, #0");                                          // is the adjusted offset still before the start?
    emitter.instruction("csel x1, xzr, x1, lt");                                // clamp a still-negative offset to zero

    // -- clamp the slice length to the available tail --
    emitter.label("__rt_aslp_pos_off");
    emitter.instruction("cmp x1, x9");                                          // does the offset start at or beyond the array length?
    emitter.instruction("b.ge __rt_aslp_empty");                                // an out-of-range offset yields an empty result hash
    emitter.instruction("sub x3, x9, x1");                                      // x3 = remaining = length - offset
    emitter.instruction("cmn x2, #1");                                          // was the length the -1 until-end sentinel?
    emitter.instruction("csel x2, x3, x2, eq");                                 // use the remaining length when until-end was requested
    emitter.instruction("cmp x2, x3");                                          // does the requested length exceed the remaining tail?
    emitter.instruction("csel x2, x3, x2, gt");                                 // clamp the slice length to the remaining tail
    emitter.instruction("str x1, [sp, #16]");                                   // save the normalized offset
    emitter.instruction("str x2, [sp, #24]");                                   // save the clamped slice length

    // -- create the result hash with capacity = slice_len * 2 (min 16) --
    emitter.instruction("lsl x0, x2, #1");                                      // x0 = slice length * 2 for hash headroom
    emitter.instruction("mov x9, #16");                                         // x9 = minimum hash capacity
    emitter.instruction("cmp x0, x9");                                          // compare the derived capacity with the minimum
    emitter.instruction("csel x0, x9, x0, lt");                                 // clamp the capacity up to the minimum bucket count
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = result value_type tag (source element type)
    emitter.instruction("bl __rt_hash_new");                                    // allocate the result hash table
    emitter.instruction("str x0, [sp, #32]");                                   // save the result hash pointer
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize the loop index to zero

    emitter.label("__rt_aslp_loop");
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the clamped slice length
    emitter.instruction("cmp x4, x3");                                          // have all slice elements been copied?
    emitter.instruction("b.ge __rt_aslp_done");                                 // stop once the whole slice has been inserted

    // -- key = offset + i (preserved original index); value = source[offset + i] --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("add x5, x5, #24");                                     // advance to the source data region
    emitter.instruction("ldr x6, [sp, #16]");                                   // reload the normalized offset
    emitter.instruction("add x7, x6, x4");                                      // x7 = offset + i = preserved integer key
    emitter.instruction("ldr x8, [x5, x7, lsl #3]");                            // x8 = source[offset + i] scalar value

    // -- insert the preserved-key/value pair (key_hi = -1 marks an integer key) --
    emitter.instruction("ldr x0, [sp, #32]");                                   // x0 = result hash pointer
    emitter.instruction("mov x1, x7");                                          // x1 = key_lo = preserved original index
    emitter.instruction("mov x2, #-1");                                         // x2 = key_hi sentinel marks an integer key
    emitter.instruction("mov x3, x8");                                          // x3 = value_lo = source scalar value
    emitter.instruction("mov x4, #0");                                          // x4 = value_hi = 0 for scalar payloads
    emitter.instruction("ldr x5, [sp, #8]");                                    // x5 = value_tag = source element type
    emitter.instruction("bl __rt_hash_set");                                    // insert the preserved key/value pair
    emitter.instruction("str x0, [sp, #32]");                                   // update the hash pointer after possible growth

    // -- advance the loop --
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #1");                                      // increment the loop index
    emitter.instruction("str x4, [sp, #40]");                                   // save the updated loop index
    emitter.instruction("b __rt_aslp_loop");                                    // continue copying the slice

    emitter.label("__rt_aslp_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // x0 = result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the result hash

    // -- empty result: offset started beyond the array bounds --
    emitter.label("__rt_aslp_empty");
    emitter.instruction("mov x0, #16");                                         // minimum hash capacity for the empty result
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = result value_type tag
    emitter.instruction("bl __rt_hash_new");                                    // allocate an empty result hash
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the empty result hash
}

/// Emits `__rt_array_slice_preserve` for the x86_64-linux ABI target.
///
/// Input:  rdi = source array, rsi = offset, rdx = length (-1 = until end), rcx = value_type tag
/// Output: rax = new hash table keyed by the preserved original integer indices
fn emit_array_slice_preserve_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_slice_preserve ---");
    emitter.label_global("__rt_array_slice_preserve");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the slice-preserve bookkeeping
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots while keeping nested calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source array pointer across nested helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // save the source element value_type tag across nested helper calls
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // r9 = source array length

    // -- normalize a negative offset to a positive index (clamped to 0) --
    emitter.instruction("cmp rsi, 0");                                          // is the requested offset negative?
    emitter.instruction("jge __rt_aslp_off_ready_x86");                         // skip adjustment when the offset is already non-negative
    emitter.instruction("add rsi, r9");                                         // offset = length + offset for negative offsets
    emitter.instruction("cmp rsi, 0");                                          // is the adjusted offset still before the start?
    emitter.instruction("jge __rt_aslp_off_ready_x86");                         // keep a now-non-negative adjusted offset
    emitter.instruction("xor esi, esi");                                        // clamp a still-negative offset to zero
    emitter.label("__rt_aslp_off_ready_x86");
    emitter.instruction("cmp rsi, r9");                                         // does the offset start at or beyond the array length?
    emitter.instruction("jge __rt_aslp_empty_x86");                             // an out-of-range offset yields an empty result hash

    // -- clamp the slice length to the available tail --
    emitter.instruction("mov r8, r9");                                          // r8 = length
    emitter.instruction("sub r8, rsi");                                         // r8 = remaining = length - offset
    emitter.instruction("cmp rdx, -1");                                         // was the length the -1 until-end sentinel?
    emitter.instruction("jne __rt_aslp_have_len_x86");                          // keep an explicit requested length
    emitter.instruction("mov rdx, r8");                                         // use the remaining length when until-end was requested
    emitter.label("__rt_aslp_have_len_x86");
    emitter.instruction("cmp rdx, r8");                                         // does the requested length exceed the remaining tail?
    emitter.instruction("jle __rt_aslp_len_ok_x86");                            // keep a length that already fits the remaining tail
    emitter.instruction("mov rdx, r8");                                         // clamp the slice length to the remaining tail
    emitter.label("__rt_aslp_len_ok_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the normalized offset
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the clamped slice length

    // -- create the result hash with capacity = slice_len * 2 (min 16) --
    emitter.instruction("mov rdi, rdx");                                        // rdi = slice length
    emitter.instruction("shl rdi, 1");                                          // rdi = slice length * 2 for hash headroom
    emitter.instruction("cmp rdi, 16");                                         // compare the derived capacity with the minimum
    emitter.instruction("jge __rt_aslp_cap_x86");                               // keep a capacity that already meets the minimum
    emitter.instruction("mov rdi, 16");                                         // clamp the capacity up to the minimum bucket count
    emitter.label("__rt_aslp_cap_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // rsi = result value_type tag (source element type)
    emitter.instruction("call __rt_hash_new");                                  // allocate the result hash table
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the result hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the loop index to zero

    emitter.label("__rt_aslp_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the loop index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // have all slice elements been copied?
    emitter.instruction("jge __rt_aslp_done_x86");                              // stop once the whole slice has been inserted

    // -- key = offset + i (preserved original index); value = source[offset + i] --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source array pointer
    emitter.instruction("lea r10, [r10 + 24]");                                 // advance to the source data region
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the normalized offset
    emitter.instruction("add r11, rcx");                                        // r11 = offset + i = preserved integer key
    emitter.instruction("mov rax, QWORD PTR [r10 + r11 * 8]");                  // rax = source[offset + i] scalar value

    // -- insert the preserved-key/value pair (key_hi = -1 marks an integer key) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // rdi = result hash pointer
    emitter.instruction("mov rsi, r11");                                        // rsi = key_lo = preserved original index
    emitter.instruction("mov rdx, -1");                                         // rdx = key_hi sentinel marks an integer key
    emitter.instruction("mov rcx, rax");                                        // rcx = value_lo = source scalar value
    emitter.instruction("xor r8d, r8d");                                        // r8 = value_hi = 0 for scalar payloads
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // r9 = value_tag = source element type
    emitter.instruction("call __rt_hash_set");                                  // insert the preserved key/value pair
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // update the hash pointer after possible growth

    // -- advance the loop --
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the loop index
    emitter.instruction("add r10, 1");                                          // increment the loop index
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the updated loop index
    emitter.instruction("jmp __rt_aslp_loop_x86");                              // continue copying the slice

    emitter.label("__rt_aslp_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // rax = result hash pointer
    emitter.instruction("add rsp, 48");                                         // deallocate the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result hash

    // -- empty result: offset started beyond the array bounds --
    emitter.label("__rt_aslp_empty_x86");
    emitter.instruction("mov rdi, 16");                                         // minimum hash capacity for the empty result
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // rsi = result value_type tag
    emitter.instruction("call __rt_hash_new");                                  // allocate an empty result hash
    emitter.instruction("add rsp, 48");                                         // deallocate the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the empty result hash
}
