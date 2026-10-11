//! Purpose:
//! Emits the `__rt_usort_str` runtime helper assembly used by `usort()` when the
//! receiver is an indexed string array. String arrays store 16-byte
//! `[ptr:8][len:8]` payload slots, so the 8-byte slot permuter `__rt_usort` cannot
//! reorder them without corrupting the descriptors.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The algorithm is a stable insertion sort over whole 16-byte slots, matching
//!   PHP 8's stable `usort()` ordering for elements the comparator reports equal.
//! - The comparator ABI mirrors a PHP function of two string parameters: on
//!   AArch64 `x0`/`x1` carry the left pointer/length, `x2`/`x3` the right
//!   pointer/length, and `x4` the optional capture environment; on x86_64 the
//!   same values land in `rdi`/`rsi`, `rdx`/`rcx`, and `r8`. The integer result is
//!   read from `x0`/`rax`.
//! - Every piece of loop state lives in the frame because the comparator callback
//!   is free to clobber all caller-saved registers; the helper itself touches no
//!   callee-saved register other than the frame pointer.
//! - Slots are permuted in place, so string payload ownership stays with the
//!   array and no refcount traffic is needed.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// usort_str: sorts an indexed string array in place through a user comparator.
///
/// Input: AArch64 `x0` = comparator address, `x1` = array pointer, `x2` = optional
/// capture environment pointer (0 when the comparator takes no environment);
/// x86_64 `rdi` / `rsi` / `rdx` respectively.
/// Output: none — the array payload is reordered in place and keys are implicitly
/// renumbered because indexed arrays carry no key storage.
/// Arrays shorter than two elements return immediately.
pub fn emit_usort_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_usort_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: usort_str ---");
    emitter.label_global("__rt_usort_str");

    // Frame (96 bytes): [0]=length [8]=base [16]=i [24]=keyptr [32]=keylen
    //                   [40]=j [48]=comparator [56]=env [80]=x29,x30
    emitter.instruction("sub sp, sp, #96");                                     // reserve the insertion-sort state that must survive comparator calls
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #48]");                                   // save the comparator address for every inner-loop call
    emitter.instruction("str x2, [sp, #56]");                                   // save the optional comparator capture environment pointer
    emitter.instruction("ldr x9, [x1]");                                        // x9 = array length from the header
    emitter.instruction("str x9, [sp, #0]");                                    // save the array length
    emitter.instruction("add x9, x1, #24");                                     // x9 = base of the data region (skip header)
    emitter.instruction("str x9, [sp, #8]");                                    // save the data base
    emitter.instruction("mov x9, #1");                                          // outer-loop index i = 1
    emitter.instruction("str x9, [sp, #16]");                                   // save i

    emitter.label("__rt_usort_str_outer");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload i
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the array length
    emitter.instruction("cmp x3, x1");                                          // compare i with the array length
    emitter.instruction("b.ge __rt_usort_str_done");                            // i >= length: sorting complete
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the data base
    emitter.instruction("add x9, x2, x3, lsl #4");                              // x9 = &data[i] (16-byte string slots)
    emitter.instruction("ldr x4, [x9]");                                        // keyptr = data[i] string pointer
    emitter.instruction("ldr x5, [x9, #8]");                                    // keylen = data[i] string length
    emitter.instruction("str x4, [sp, #24]");                                   // save keyptr across comparator calls
    emitter.instruction("str x5, [sp, #32]");                                   // save keylen across comparator calls
    emitter.instruction("sub x6, x3, #1");                                      // j = i - 1 (scan the sorted prefix)
    emitter.instruction("str x6, [sp, #40]");                                   // save j

    emitter.label("__rt_usort_str_inner");
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload j
    emitter.instruction("cmp x6, #0");                                          // is j below the start of the array?
    emitter.instruction("b.lt __rt_usort_str_insert");                          // insertion point reached
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the data base
    emitter.instruction("add x9, x2, x6, lsl #4");                              // x9 = &data[j]
    emitter.instruction("ldr x0, [x9]");                                        // comparator arg a: data[j] string pointer
    emitter.instruction("ldr x1, [x9, #8]");                                    // comparator arg a: data[j] string length
    emitter.instruction("ldr x2, [sp, #24]");                                   // comparator arg b: keyptr
    emitter.instruction("ldr x3, [sp, #32]");                                   // comparator arg b: keylen
    emitter.instruction("ldr x4, [sp, #56]");                                   // pass the capture environment after the compared string pair
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the comparator address
    emitter.instruction("blr x9");                                              // x0 = comparator(data[j], key)
    emitter.instruction("cmp x0, #0");                                          // is data[j] already ordered at or before the key?
    emitter.instruction("b.le __rt_usort_str_insert");                          // ordered: insert here, which keeps equal elements stable
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload j for the shift
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the data base
    emitter.instruction("add x9, x2, x6, lsl #4");                              // x9 = &data[j]
    emitter.instruction("ldr x10, [x9]");                                       // data[j] string pointer
    emitter.instruction("ldr x11, [x9, #8]");                                   // data[j] string length
    emitter.instruction("str x10, [x9, #16]");                                  // data[j+1] pointer = data[j] pointer
    emitter.instruction("str x11, [x9, #24]");                                  // data[j+1] length = data[j] length
    emitter.instruction("sub x6, x6, #1");                                      // j -= 1 (continue scanning left)
    emitter.instruction("str x6, [sp, #40]");                                   // save j
    emitter.instruction("b __rt_usort_str_inner");                              // continue the inner loop

    emitter.label("__rt_usort_str_insert");
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload j
    emitter.instruction("add x12, x6, #1");                                     // insertion index j + 1
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the data base
    emitter.instruction("add x9, x2, x12, lsl #4");                             // x9 = &data[j+1]
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload keyptr
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload keylen
    emitter.instruction("str x10, [x9]");                                       // data[j+1] pointer = keyptr
    emitter.instruction("str x11, [x9, #8]");                                   // data[j+1] length = keylen
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload i
    emitter.instruction("add x3, x3, #1");                                      // advance the outer-loop index
    emitter.instruction("str x3, [sp, #16]");                                   // save i
    emitter.instruction("b __rt_usort_str_outer");                              // continue the outer loop

    emitter.label("__rt_usort_str_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the insertion-sort state frame
    emitter.instruction("ret");                                                 // return (void, string array sorted in place)
}

/// x86_64 Linux implementation of the `__rt_usort_str` runtime helper.
///
/// Inputs (System V): `rdi` = comparator address, `rsi` = array pointer,
/// `rdx` = optional capture environment pointer.
/// Uses the same stable insertion sort as the AArch64 path; the comparator is
/// invoked with `rdi`/`rsi` = left pointer/length, `rdx`/`rcx` = right
/// pointer/length, `r8` = environment, and returns the ordering in `rax`.
/// Emits `__rt_usort_str` as a global label.
fn emit_usort_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: usort_str ---");
    emitter.label_global("__rt_usort_str");

    // Frame (rbp-relative): [-8]=length [-16]=base [-24]=i [-32]=keyptr
    //                       [-40]=keylen [-48]=j [-56]=comparator [-64]=env
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve the insertion-sort state slots and keep rsp 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // save the comparator address for every inner-loop call
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save the optional comparator capture environment pointer
    emitter.instruction("mov r8, QWORD PTR [rsi]");                             // r8 = array length from the header
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // save the array length
    emitter.instruction("lea r8, [rsi + 24]");                                  // r8 = base of the data region (skip header)
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // save the data base
    emitter.instruction("mov r8, 1");                                           // outer-loop index i = 1
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // save i

    emitter.label("__rt_usort_str_outer_linux_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload i
    emitter.instruction("cmp r10, QWORD PTR [rbp - 8]");                        // compare i with the array length
    emitter.instruction("jge __rt_usort_str_done_linux_x86_64");                // i >= length: sorting complete
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the data base
    emitter.instruction("shl r10, 4");                                          // i * 16 (16-byte string slots)
    emitter.instruction("add r9, r10");                                         // r9 = &data[i]
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // keyptr = data[i] string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save keyptr across comparator calls
    emitter.instruction("mov rax, QWORD PTR [r9 + 8]");                         // keylen = data[i] string length
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save keylen across comparator calls
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the unscaled outer-loop index
    emitter.instruction("sub r10, 1");                                          // j = i - 1 (scan the sorted prefix)
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save j

    emitter.label("__rt_usort_str_inner_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload j
    emitter.instruction("cmp r9, 0");                                           // is j below the start of the array?
    emitter.instruction("jl __rt_usort_str_insert_linux_x86_64");               // insertion point reached
    emitter.instruction("shl r9, 4");                                           // j * 16 (16-byte string slots)
    emitter.instruction("add r9, QWORD PTR [rbp - 16]");                        // r9 = &data[j]
    emitter.instruction("mov rdi, QWORD PTR [r9]");                             // comparator arg a: data[j] string pointer
    emitter.instruction("mov rsi, QWORD PTR [r9 + 8]");                         // comparator arg a: data[j] string length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // comparator arg b: keyptr
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // comparator arg b: keylen
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // pass the capture environment after the compared string pair
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the comparator address
    emitter.emit_platform_callback_call("r11", 5);                                // call generated PHP comparator with the target ABI
    emitter.instruction("cmp rax, 0");                                          // is data[j] already ordered at or before the key?
    emitter.instruction("jle __rt_usort_str_insert_linux_x86_64");              // ordered: insert here, which keeps equal elements stable
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload j for the shift
    emitter.instruction("shl r9, 4");                                           // j * 16 (16-byte string slots)
    emitter.instruction("add r9, QWORD PTR [rbp - 16]");                        // r9 = &data[j]
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // data[j] string pointer
    emitter.instruction("mov rax, QWORD PTR [r9 + 8]");                         // data[j] string length
    emitter.instruction("mov QWORD PTR [r9 + 16], r10");                        // data[j+1] pointer = data[j] pointer
    emitter.instruction("mov QWORD PTR [r9 + 24], rax");                        // data[j+1] length = data[j] length
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the unscaled inner-loop index
    emitter.instruction("sub r10, 1");                                          // j -= 1 (continue scanning left)
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save j
    emitter.instruction("jmp __rt_usort_str_inner_linux_x86_64");               // continue the inner loop

    emitter.label("__rt_usort_str_insert_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload j
    emitter.instruction("add r9, 1");                                           // insertion index j + 1
    emitter.instruction("shl r9, 4");                                           // (j + 1) * 16
    emitter.instruction("add r9, QWORD PTR [rbp - 16]");                        // r9 = &data[j+1]
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload keyptr
    emitter.instruction("mov QWORD PTR [r9], r10");                             // data[j+1] pointer = keyptr
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload keylen
    emitter.instruction("mov QWORD PTR [r9 + 8], r10");                         // data[j+1] length = keylen
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload i
    emitter.instruction("add r10, 1");                                          // advance the outer-loop index
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save i
    emitter.instruction("jmp __rt_usort_str_outer_linux_x86_64");               // continue the outer loop

    emitter.label("__rt_usort_str_done_linux_x86_64");
    emitter.instruction("add rsp, 64");                                         // release the insertion-sort state slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return (void, string array sorted in place)
}
