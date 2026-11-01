//! Purpose:
//! Emits the `__rt_array_merge_str` runtime helper for merging two string-element
//! indexed arrays (16-byte `(ptr, len)` slots) into a freshly owned string array.
//!
//! Called from:
//! - `crate::codegen_support::runtime::arrays` via the top-level runtime emitter.
//! - The EIR `array_merge` lowering when both operands are `Array(Str)`.
//!
//! Key details:
//! - Reuses `__rt_array_push_str`, which persists each string into owned heap storage,
//!   handles capacity growth, and stamps the 16-byte string-slot layout. The result is
//!   independently owned (its strings are copies), so the source arrays stay untouched
//!   and a single release of the result frees it.

use crate::codegen_support::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_array_merge_str(array1, array2) -> array` for the active target.
///
/// Allocates a destination string array sized for both inputs, then appends every
/// element of the first and second source array through `__rt_array_push_str`. Each
/// appended string is persisted (copied) into owned heap storage, so the merged
/// array owns its values and both source arrays remain borrowed.
pub fn emit_array_merge_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_merge_str_x86_64(emitter);
        return;
    }
    emit_array_merge_str_aarch64(emitter);
}

/// ARM64 implementation of `__rt_array_merge_str` (inputs `x0`/`x1`, result `x0`).
fn emit_array_merge_str_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge_str ---");
    emitter.label_global("__rt_array_merge_str");

    // Frame: [sp,#0]=array1 [sp,#8]=array2 [sp,#16]=len1 [sp,#24]=len2
    //        [sp,#32]=dest [sp,#40]=index [sp,#48]=fp/lr
    emitter.instruction("sub sp, sp, #64");                                     // reserve the merge bookkeeping frame and saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the first source array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the second source array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load the first source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save the first source array length
    emitter.instruction("ldr x10, [x1]");                                       // load the second source array length
    emitter.instruction("str x10, [sp, #24]");                                  // save the second source array length

    // -- allocate a string destination sized for both inputs --
    emitter.instruction("add x0, x9, x10");                                     // combined element count = len1 + len2
    emitter.instruction("mov x1, #16");                                         // 16-byte slots for (ptr, len) string payloads
    emitter.instruction("bl __rt_array_new");                                   // allocate the destination string array
    emitter.instruction("str x0, [sp, #32]");                                   // save the destination array pointer

    // -- append every element of the first source array --
    emitter.instruction("mov x4, #0");                                          // initialize the first-source copy index
    emitter.instruction("str x4, [sp, #40]");                                   // persist the first-source copy index
    emitter.label("__rt_array_merge_str_copy1");
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the current first-source index
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the first source array length
    emitter.instruction("cmp x4, x9");                                          // has the first source array been fully copied?
    emitter.instruction("b.ge __rt_array_merge_str_copy2_setup");               // switch to the second source array when the first is done
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the first source array pointer
    emitter.instruction("add x2, x1, #24");                                     // skip the header to the first-source data region
    emitter.instruction("lsl x3, x4, #4");                                      // current index * 16 = byte offset of the string slot
    emitter.instruction("add x2, x2, x3");                                      // address of the current first-source string slot
    emitter.instruction("ldr x1, [x2]");                                        // load the source string pointer (push_str arg1)
    emitter.instruction("ldr x2, [x2, #8]");                                    // load the source string length (push_str arg2)
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the destination array pointer
    emitter.instruction("bl __rt_array_push_str");                              // persist and append the source string into the destination
    emitter.instruction("str x0, [sp, #32]");                                   // save the possibly-grown destination array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the first-source index
    emitter.instruction("add x4, x4, #1");                                      // advance to the next first-source element
    emitter.instruction("str x4, [sp, #40]");                                   // persist the advanced first-source index
    emitter.instruction("b __rt_array_merge_str_copy1");                        // continue copying the first source array

    // -- append every element of the second source array --
    emitter.label("__rt_array_merge_str_copy2_setup");
    emitter.instruction("mov x4, #0");                                          // initialize the second-source copy index
    emitter.instruction("str x4, [sp, #40]");                                   // persist the second-source copy index
    emitter.label("__rt_array_merge_str_copy2");
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the current second-source index
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the second source array length
    emitter.instruction("cmp x4, x10");                                         // has the second source array been fully copied?
    emitter.instruction("b.ge __rt_array_merge_str_done");                      // finish once both source arrays are copied
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the second source array pointer
    emitter.instruction("add x2, x1, #24");                                     // skip the header to the second-source data region
    emitter.instruction("lsl x3, x4, #4");                                      // current index * 16 = byte offset of the string slot
    emitter.instruction("add x2, x2, x3");                                      // address of the current second-source string slot
    emitter.instruction("ldr x1, [x2]");                                        // load the source string pointer (push_str arg1)
    emitter.instruction("ldr x2, [x2, #8]");                                    // load the source string length (push_str arg2)
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the destination array pointer
    emitter.instruction("bl __rt_array_push_str");                              // persist and append the source string into the destination
    emitter.instruction("str x0, [sp, #32]");                                   // save the possibly-grown destination array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the second-source index
    emitter.instruction("add x4, x4, #1");                                      // advance to the next second-source element
    emitter.instruction("str x4, [sp, #40]");                                   // persist the advanced second-source index
    emitter.instruction("b __rt_array_merge_str_copy2");                        // continue copying the second source array

    // -- return the merged string array --
    emitter.label("__rt_array_merge_str_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the merged destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the local frame
    emitter.instruction("ret");                                                 // return the merged string array in x0
}

/// x86_64 implementation of `__rt_array_merge_str` (inputs `rdi`/`rsi`, result `rax`).
fn emit_array_merge_str_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge_str ---");
    emitter.label_global("__rt_array_merge_str");

    // Frame: [rbp-8]=array1 [rbp-16]=array2 [rbp-24]=len1 [rbp-32]=len2
    //        [rbp-40]=dest [rbp-48]=index
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the merge bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the first source array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the second source array pointer
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the first source array length
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the first source array length
    emitter.instruction("mov r11, QWORD PTR [rsi]");                            // load the second source array length
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the second source array length

    // -- allocate a string destination sized for both inputs --
    emitter.instruction("mov rdi, r10");                                        // seed the combined element count with len1
    emitter.instruction("add rdi, r11");                                        // combined element count = len1 + len2
    emitter.instruction("mov rsi, 16");                                         // 16-byte slots for (ptr, len) string payloads
    emitter.instruction("call __rt_array_new");                                 // allocate the destination string array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the destination array pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the first-source copy index

    // -- append every element of the first source array --
    emitter.label("__rt_array_merge_str_copy1_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the current first-source index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // has the first source array been fully copied?
    emitter.instruction("jge __rt_array_merge_str_copy2_setup_x86");            // switch to the second source array when the first is done
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the first source array pointer
    emitter.instruction("lea r10, [r10 + 24]");                                 // skip the header to the first-source data region
    emitter.instruction("shl rcx, 4");                                          // current index * 16 = byte offset of the string slot
    emitter.instruction("lea r10, [r10 + rcx]");                                // address of the current first-source string slot
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the source string pointer (push_str arg2)
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the source string length (push_str arg3)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the destination array pointer
    emitter.instruction("call __rt_array_push_str");                            // persist and append the source string into the destination
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the possibly-grown destination array pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the first-source index
    emitter.instruction("add rcx, 1");                                          // advance to the next first-source element
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the advanced first-source index
    emitter.instruction("jmp __rt_array_merge_str_copy1_x86");                  // continue copying the first source array

    // -- append every element of the second source array --
    emitter.label("__rt_array_merge_str_copy2_setup_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // reset the copy index for the second source array

    emitter.label("__rt_array_merge_str_copy2_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the current second-source index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // has the second source array been fully copied?
    emitter.instruction("jge __rt_array_merge_str_done_x86");                   // finish once both source arrays are copied
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the second source array pointer
    emitter.instruction("lea r10, [r10 + 24]");                                 // skip the header to the second-source data region
    emitter.instruction("shl rcx, 4");                                          // current index * 16 = byte offset of the string slot
    emitter.instruction("lea r10, [r10 + rcx]");                                // address of the current second-source string slot
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the source string pointer (push_str arg2)
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the source string length (push_str arg3)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the destination array pointer
    emitter.instruction("call __rt_array_push_str");                            // persist and append the source string into the destination
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the possibly-grown destination array pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the second-source index
    emitter.instruction("add rcx, 1");                                          // advance to the next second-source element
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the advanced second-source index
    emitter.instruction("jmp __rt_array_merge_str_copy2_x86");                  // continue copying the second source array

    // -- return the merged string array --
    emitter.label("__rt_array_merge_str_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the merged destination array pointer
    emitter.instruction("mov rsp, rbp");                                        // restore the stack pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the merged string array in rax
}

