//! Purpose:
//! Emits the `__rt_array_merge_str` runtime helper: merges two indexed STRING arrays
//! (16-byte ptr+len slots) into a new owned string array.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - String indexed arrays use 16-byte (pointer, length) element slots, unlike the
//!   scalar/refcounted merges which assume 8-byte payloads. Routing `Array(Str)` through
//!   the 8-byte `__rt_array_merge` corrupts every element past the first.
//! - Each element is appended with `__rt_array_push_str`, which persists the string to the
//!   heap, so the merged array owns its own copies and is safe to deep-free.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_array_merge_str`.
/// Input: x0/rdi = first string array, x1/rsi = second string array.
/// Output: x0/rax = pointer to the new merged string array.
pub fn emit_array_merge_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_merge_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_merge_str ---");
    emitter.label_global("__rt_array_merge_str");

    // -- set up stack frame, save both source string arrays and their lengths --
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save first array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save second array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load first array length
    emitter.instruction("str x9, [sp, #16]");                                   // save first array length
    emitter.instruction("ldr x10, [x1]");                                       // load second array length
    emitter.instruction("str x10, [sp, #24]");                                  // save second array length

    // -- allocate an empty destination sized for both arrays (16-byte string slots) --
    emitter.instruction("add x0, x9, x10");                                     // combined element capacity
    emitter.instruction("mov x1, #16");                                         // 16-byte ptr+len slots for string payloads
    emitter.instruction("bl __rt_array_new");                                   // allocate destination string array
    emitter.instruction("str x0, [sp, #32]");                                   // save destination array pointer

    // -- append every string from the first array (push_str persists each one) --
    emitter.instruction("mov x4, #0");                                          // first-array loop index
    emitter.label("__rt_array_merge_str_copy1");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload first array length
    emitter.instruction("cmp x4, x9");                                          // index reached the first array length?
    emitter.instruction("b.ge __rt_array_merge_str_copy2_setup");               // move on to the second array
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload first array pointer
    emitter.instruction("add x1, x1, #24");                                     // first array data base
    emitter.instruction("lsl x5, x4, #4");                                      // index × 16-byte string slot
    emitter.instruction("add x5, x1, x5");                                      // address of the current string slot
    emitter.instruction("ldr x1, [x5]");                                        // string pointer → push_str argument
    emitter.instruction("ldr x2, [x5, #8]");                                    // string length → push_str argument
    emitter.instruction("str x4, [sp, #40]");                                   // preserve the loop index across the push_str call
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_str");                              // persist + append the string → x0 = possibly-grown destination
    emitter.instruction("str x0, [sp, #32]");                                   // save the possibly-grown destination pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // restore the loop index
    emitter.instruction("add x4, x4, #1");                                      // advance the first-array loop index
    emitter.instruction("b __rt_array_merge_str_copy1");                        // continue with the first array

    // -- append every string from the second array after the first segment --
    emitter.label("__rt_array_merge_str_copy2_setup");
    emitter.instruction("mov x4, #0");                                          // second-array loop index
    emitter.label("__rt_array_merge_str_copy2");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload second array length
    emitter.instruction("cmp x4, x10");                                         // index reached the second array length?
    emitter.instruction("b.ge __rt_array_merge_str_done");                      // finished merging
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload second array pointer
    emitter.instruction("add x1, x1, #24");                                     // second array data base
    emitter.instruction("lsl x5, x4, #4");                                      // index × 16-byte string slot
    emitter.instruction("add x5, x1, x5");                                      // address of the current string slot
    emitter.instruction("ldr x1, [x5]");                                        // string pointer → push_str argument
    emitter.instruction("ldr x2, [x5, #8]");                                    // string length → push_str argument
    emitter.instruction("str x4, [sp, #40]");                                   // preserve the loop index across the push_str call
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_str");                              // persist + append the string → x0 = possibly-grown destination
    emitter.instruction("str x0, [sp, #32]");                                   // save the possibly-grown destination pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // restore the loop index
    emitter.instruction("add x4, x4, #1");                                      // advance the second-array loop index
    emitter.instruction("b __rt_array_merge_str_copy2");                        // continue with the second array

    // -- return the merged string array (length was set incrementally by push_str) --
    emitter.label("__rt_array_merge_str_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the merged destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return merged array in x0
}

/// x86_64 Linux variant of `__rt_array_merge_str`.
/// Takes first/second string-array pointers in rdi/rsi, returns the merged array in rax.
/// Mirrors the ARM64 algorithm using `__rt_array_push_str` to persist and append each string.
fn emit_array_merge_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge_str ---");
    emitter.label_global("__rt_array_merge_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving merge spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source arrays, lengths, destination, and index
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots while keeping helper calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the first source string-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the second source string-array pointer
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the first source string-array length
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the first source string-array length
    emitter.instruction("mov r11, QWORD PTR [rsi]");                            // load the second source string-array length
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the second source string-array length
    emitter.instruction("mov rdi, r10");                                        // seed the combined capacity from the first length
    emitter.instruction("add rdi, r11");                                        // add the second length for the combined capacity
    emitter.instruction("mov rsi, 16");                                         // request 16-byte ptr+len slots for string payloads
    emitter.instruction("call __rt_array_new");                                 // allocate the destination string array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the destination string-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the first-array loop index

    emitter.label("__rt_array_merge_str_copy_first_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the first-array loop index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // index reached the first array length?
    emitter.instruction("jge __rt_array_merge_str_copy_second_setup_x86");      // move on to the second array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the first source string-array pointer
    emitter.instruction("lea r10, [r10 + 24]");                                 // first array data base
    emitter.instruction("mov r11, rcx");                                        // copy the loop index for slot scaling
    emitter.instruction("shl r11, 4");                                          // index × 16-byte string slot
    emitter.instruction("mov rsi, QWORD PTR [r10 + r11]");                      // string pointer → push_str argument
    emitter.instruction("mov rdx, QWORD PTR [r10 + r11 + 8]");                  // string length → push_str argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the destination array pointer
    emitter.instruction("call __rt_array_push_str");                            // persist + append the string → rax = possibly-grown destination
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the possibly-grown destination pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the loop index after the call
    emitter.instruction("add rcx, 1");                                          // advance the first-array loop index
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the updated index
    emitter.instruction("jmp __rt_array_merge_str_copy_first_x86");             // continue with the first array

    emitter.label("__rt_array_merge_str_copy_second_setup_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // reset the loop index for the second array

    emitter.label("__rt_array_merge_str_copy_second_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the second-array loop index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // index reached the second array length?
    emitter.instruction("jge __rt_array_merge_str_done_x86");                   // finished merging
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the second source string-array pointer
    emitter.instruction("lea r10, [r10 + 24]");                                 // second array data base
    emitter.instruction("mov r11, rcx");                                        // copy the loop index for slot scaling
    emitter.instruction("shl r11, 4");                                          // index × 16-byte string slot
    emitter.instruction("mov rsi, QWORD PTR [r10 + r11]");                      // string pointer → push_str argument
    emitter.instruction("mov rdx, QWORD PTR [r10 + r11 + 8]");                  // string length → push_str argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the destination array pointer
    emitter.instruction("call __rt_array_push_str");                            // persist + append the string → rax = possibly-grown destination
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the possibly-grown destination pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the loop index after the call
    emitter.instruction("add rcx, 1");                                          // advance the second-array loop index
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the updated index
    emitter.instruction("jmp __rt_array_merge_str_copy_second_x86");            // continue with the second array

    emitter.label("__rt_array_merge_str_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the merged destination string-array pointer
    emitter.instruction("add rsp, 48");                                         // release the merge spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return merged array in rax
}
