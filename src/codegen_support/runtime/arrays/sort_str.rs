//! Purpose:
//! Emits the `__rt_sort_str`, `__rt_rsort_str` runtime helper assembly for
//! sorting indexed string arrays in place.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - String arrays use 16-byte payload slots (`[ptr:8][len:8]`); the integer
//!   sort helper would misread them, so string sorts need their own routine.
//! - Comparison delegates to `__rt_strcmp`; the insertion-sort state is held on
//!   the stack so it survives the call.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// sort_str / rsort_str: insertion sort on an indexed string array (in-place).
/// Input: AArch64 x0 / x86_64 rdi = array pointer
pub fn emit_sort_str(emitter: &mut Emitter, reverse: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_sort_str_linux_x86_64(emitter, reverse);
        return;
    }

    let label = if reverse { "__rt_rsort_str" } else { "__rt_sort_str" };
    let cmp_branch = if reverse { "b.ge" } else { "b.le" };
    let outer = format!("{}_outer", label);
    let inner = format!("{}_inner", label);
    let insert = format!("{}_insert", label);
    let done = format!("{}_done", label);

    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", label));
    emitter.label_global(label);

    // Frame (64 bytes): [0]=length [8]=base [16]=i [24]=keyptr [32]=keylen
    //                   [40]=j [48]=x29 [56]=x30
    emitter.instruction("sub sp, sp, #64");                                     // frame for saved registers and sort state
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("ldr x1, [x0]");                                        // x1 = array length from the header
    emitter.instruction("str x1, [sp, #0]");                                    // save the array length
    emitter.instruction("add x1, x0, #24");                                     // x1 = base of the data region (skip header)
    emitter.instruction("str x1, [sp, #8]");                                    // save the data base
    emitter.instruction("mov x1, #1");                                          // outer-loop index i = 1
    emitter.instruction("str x1, [sp, #16]");                                   // save i

    emitter.label(&outer);
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload i
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the array length
    emitter.instruction("cmp x3, x1");                                          // compare i with the array length
    emitter.instruction(&format!("b.ge {}", done));                             // i >= length: sorting complete
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the data base
    emitter.instruction("add x9, x2, x3, lsl #4");                              // x9 = &data[i] (16-byte slots)
    emitter.instruction("ldr x4, [x9]");                                        // keyptr = data[i] string pointer
    emitter.instruction("ldr x5, [x9, #8]");                                    // keylen = data[i] string length
    emitter.instruction("str x4, [sp, #24]");                                   // save keyptr
    emitter.instruction("str x5, [sp, #32]");                                   // save keylen
    emitter.instruction("sub x6, x3, #1");                                      // j = i - 1 (scan the sorted prefix)
    emitter.instruction("str x6, [sp, #40]");                                   // save j

    emitter.label(&inner);
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload j
    emitter.instruction("cmp x6, #0");                                          // is j below the start of the array?
    emitter.instruction(&format!("b.lt {}", insert));                           // insertion point reached
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the data base
    emitter.instruction("add x9, x2, x6, lsl #4");                              // x9 = &data[j]
    emitter.instruction("ldr x1, [x9]");                                        // strcmp arg a: data[j] pointer
    emitter.instruction("ldr x2, [x9, #8]");                                    // strcmp arg a: data[j] length
    emitter.instruction("ldr x3, [sp, #24]");                                   // strcmp arg b: keyptr
    emitter.instruction("ldr x4, [sp, #32]");                                   // strcmp arg b: keylen
    emitter.instruction("bl __rt_strcmp");                                      // x0 = strcmp(data[j], key)
    emitter.instruction("cmp x0, #0");                                          // is data[j] already ordered against the key?
    emitter.instruction(&format!("{} {}", cmp_branch, insert));                 // ordered: insert the key here
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload j for the shift
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the data base
    emitter.instruction("add x9, x2, x6, lsl #4");                              // x9 = &data[j]
    emitter.instruction("ldr x10, [x9]");                                       // data[j] pointer
    emitter.instruction("ldr x11, [x9, #8]");                                   // data[j] length
    emitter.instruction("add x12, x6, #1");                                     // shift destination index j + 1
    emitter.instruction("add x9, x2, x12, lsl #4");                             // x9 = &data[j+1]
    emitter.instruction("str x10, [x9]");                                       // data[j+1] pointer = data[j] pointer
    emitter.instruction("str x11, [x9, #8]");                                   // data[j+1] length = data[j] length
    emitter.instruction("sub x6, x6, #1");                                      // j -= 1 (continue scanning left)
    emitter.instruction("str x6, [sp, #40]");                                   // save j
    emitter.instruction(&format!("b {}", inner));                               // continue the inner loop

    emitter.label(&insert);
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
    emitter.instruction(&format!("b {}", outer));                               // continue the outer loop

    emitter.label(&done);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 array runtime helper for sort str.
fn emit_sort_str_linux_x86_64(emitter: &mut Emitter, reverse: bool) {
    let label = if reverse { "__rt_rsort_str" } else { "__rt_sort_str" };
    let cmp_jump = if reverse { "jge" } else { "jle" };
    let outer = format!("{}_outer", label);
    let inner = format!("{}_inner", label);
    let insert = format!("{}_insert", label);
    let done = format!("{}_done", label);

    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", label));
    emitter.label_global(label);

    // Frame (rbp-relative): [-8]=length [-16]=base [-24]=i [-32]=keyptr
    //                       [-40]=keylen [-48]=j
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve the insertion-sort state slots
    emitter.instruction("mov r8, QWORD PTR [rdi]");                             // r8 = array length from the header
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // save the array length
    emitter.instruction("lea r8, [rdi + 24]");                                  // r8 = base of the data region (skip header)
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // save the data base
    emitter.instruction("mov r8, 1");                                           // outer-loop index i = 1
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // save i

    emitter.label(&outer);
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload i
    emitter.instruction("cmp r10, QWORD PTR [rbp - 8]");                        // compare i with the array length
    emitter.instruction(&format!("jae {}", done));                              // i >= length: sorting complete
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the data base
    emitter.instruction("mov r11, r10");                                        // copy i for the slot offset
    emitter.instruction("shl r11, 4");                                          // i * 16 (16-byte string slots)
    emitter.instruction("add r9, r11");                                         // r9 = &data[i]
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // keyptr = data[i] string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save keyptr
    emitter.instruction("mov rax, QWORD PTR [r9 + 8]");                         // keylen = data[i] string length
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save keylen
    emitter.instruction("sub r10, 1");                                          // j = i - 1 (scan the sorted prefix)
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save j

    emitter.label(&inner);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload j
    emitter.instruction("cmp rcx, 0");                                          // is j below the start of the array?
    emitter.instruction(&format!("jl {}", insert));                             // insertion point reached
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the data base
    emitter.instruction("mov r11, rcx");                                        // copy j for the slot offset
    emitter.instruction("shl r11, 4");                                          // j * 16
    emitter.instruction("add r9, r11");                                         // r9 = &data[j]
    emitter.instruction("mov rdi, QWORD PTR [r9]");                             // strcmp arg a: data[j] pointer
    emitter.instruction("mov rsi, QWORD PTR [r9 + 8]");                         // strcmp arg a: data[j] length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // strcmp arg b: keyptr
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // strcmp arg b: keylen
    emitter.instruction("call __rt_strcmp");                                    // rax = strcmp(data[j], key)
    emitter.instruction("cmp rax, 0");                                          // is data[j] already ordered against the key?
    emitter.instruction(&format!("{} {}", cmp_jump, insert));                   // ordered: insert the key here
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload j for the shift
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the data base
    emitter.instruction("mov r11, rcx");                                        // copy j for the slot offset
    emitter.instruction("shl r11, 4");                                          // j * 16
    emitter.instruction("add r9, r11");                                         // r9 = &data[j]
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // data[j] pointer
    emitter.instruction("mov rax, QWORD PTR [r9 + 8]");                         // data[j] length
    emitter.instruction("mov r8, rcx");                                         // copy j for the destination offset
    emitter.instruction("add r8, 1");                                           // shift destination index j + 1
    emitter.instruction("shl r8, 4");                                           // (j + 1) * 16
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the data base
    emitter.instruction("add rsi, r8");                                         // rsi = &data[j+1]
    emitter.instruction("mov QWORD PTR [rsi], r10");                            // data[j+1] pointer = data[j] pointer
    emitter.instruction("mov QWORD PTR [rsi + 8], rax");                        // data[j+1] length = data[j] length
    emitter.instruction("sub rcx, 1");                                          // j -= 1 (continue scanning left)
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save j
    emitter.instruction(&format!("jmp {}", inner));                             // continue the inner loop

    emitter.label(&insert);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload j
    emitter.instruction("add rcx, 1");                                          // insertion index j + 1
    emitter.instruction("shl rcx, 4");                                          // (j + 1) * 16
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the data base
    emitter.instruction("add r9, rcx");                                         // r9 = &data[j+1]
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload keyptr
    emitter.instruction("mov QWORD PTR [r9], r10");                             // data[j+1] pointer = keyptr
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload keylen
    emitter.instruction("mov QWORD PTR [r9 + 8], r10");                         // data[j+1] length = keylen
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload i
    emitter.instruction("add r10, 1");                                          // advance the outer-loop index
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save i
    emitter.instruction(&format!("jmp {}", outer));                             // continue the outer loop

    emitter.label(&done);
    emitter.instruction("add rsp, 48");                                         // release the insertion-sort state slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
