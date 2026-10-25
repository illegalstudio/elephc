//! Purpose:
//! Emits the `__rt_array_diff_str` runtime helper assembly for `array_diff()` over indexed string arrays.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - An indexed `array<string>` stores 16-byte `{pointer, length}` slots. `__rt_array_diff*`
//!   compare and copy 8 bytes per element, so they cannot carry a string pair (issue #675).
//! - PHP compares `array_diff()` entries as strings, so byte equality (`__rt_str_eq`) is exact.
//! - The kept strings are DUPLICATED through `__rt_array_push_str` into a result renumbered from
//!   zero, matching the integer helper. (PHP keeps the surviving keys; that gap is shared by every
//!   element type and tracked separately.)

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_array_diff_str` runtime helper.
///
/// Returns a new indexed string array holding every string of the first array that does not occur
/// in the second, in order. Neither input is modified.
///
/// ## ABI
/// - **ARM64**: `x0` = first array, `x1` = second array. Result returned in `x0`.
/// - **x86_64 Linux**: `rdi`, `rsi` with the same meaning. Result in `rax`.
pub fn emit_array_diff_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_diff_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_diff_str ---");
    emitter.label_global("__rt_array_diff_str");

    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save first array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save second array pointer
    emitter.instruction("ldr x0, [x0]");                                        // destination capacity = first length
    emitter.instruction("mov x1, #16");                                         // request 16-byte slots for {pointer, length} pairs
    emitter.instruction("bl __rt_array_new");                                   // allocate the destination array
    emitter.instruction("str x0, [sp, #16]");                                   // save destination array pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // start the first-array cursor at 0

    emitter.label("__rt_array_diff_str_outer");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload first array pointer
    emitter.instruction("ldr x11, [x10]");                                      // load the first length
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the first-array cursor
    emitter.instruction("cmp x9, x11");                                         // has every candidate been examined?
    emitter.instruction("b.ge __rt_array_diff_str_done");                       // yes: return the destination
    emitter.instruction("str xzr, [sp, #32]");                                  // start the second-array cursor at 0

    emitter.label("__rt_array_diff_str_inner");
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload second array pointer
    emitter.instruction("ldr x13, [x12]");                                      // load the second length
    emitter.instruction("ldr x14, [sp, #32]");                                  // reload the second-array cursor
    emitter.instruction("cmp x14, x13");                                        // was the candidate absent from the second array?
    emitter.instruction("b.ge __rt_array_diff_str_keep");                       // yes: keep it
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload first array pointer
    emitter.instruction("add x10, x10, #24");                                   // compute first data base past the 24-byte header
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the first-array cursor
    emitter.instruction("add x10, x10, x9, lsl #4");                            // address the candidate {pointer, length} pair
    emitter.instruction("ldp x1, x2, [x10]");                                   // load the candidate pointer and length
    emitter.instruction("add x12, x12, #24");                                   // compute second data base past the 24-byte header
    emitter.instruction("add x12, x12, x14, lsl #4");                           // address the excluded {pointer, length} pair
    emitter.instruction("ldp x3, x4, [x12]");                                   // load the excluded pointer and length
    emitter.instruction("bl __rt_str_eq");                                      // compare the two strings byte for byte
    emitter.instruction("cbnz x0, __rt_array_diff_str_next");                   // an equal string removes the candidate
    emitter.instruction("ldr x14, [sp, #32]");                                  // reload the second-array cursor
    emitter.instruction("add x14, x14, #1");                                    // advance to the next excluded string
    emitter.instruction("str x14, [sp, #32]");                                  // spill the updated cursor
    emitter.instruction("b __rt_array_diff_str_inner");                         // keep searching the second array

    emitter.label("__rt_array_diff_str_keep");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload first array pointer
    emitter.instruction("add x10, x10, #24");                                   // compute first data base past the 24-byte header
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the first-array cursor
    emitter.instruction("add x10, x10, x9, lsl #4");                            // address the candidate {pointer, length} pair
    emitter.instruction("ldp x1, x2, [x10]");                                   // load the borrowed candidate pointer and length
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_str");                              // append a PERSISTED copy of the candidate
    emitter.instruction("str x0, [sp, #16]");                                   // persist destination pointer after possible growth

    emitter.label("__rt_array_diff_str_next");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the first-array cursor
    emitter.instruction("add x9, x9, #1");                                      // advance to the next candidate
    emitter.instruction("str x9, [sp, #24]");                                   // spill the updated cursor
    emitter.instruction("b __rt_array_diff_str_outer");                         // examine the next candidate

    emitter.label("__rt_array_diff_str_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return the difference
}

/// Emits the x86_64 Linux variant of `__rt_array_diff_str`.
///
/// Same semantics as the ARM64 variant under the System V AMD64 ABI: `rdi`, `rsi` in, `rax` out.
fn emit_array_diff_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_diff_str ---");
    emitter.label_global("__rt_array_diff_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // reserve spill slots, keeping calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the first array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the second array pointer
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // destination capacity = first length
    emitter.instruction("mov rsi, 16");                                         // request 16-byte slots for {pointer, length} pairs
    emitter.instruction("call __rt_array_new");                                 // allocate the destination array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the destination array pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the first-array cursor at 0

    emitter.label("__rt_array_diff_str_outer_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the first array pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the first-array cursor
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // has every candidate been examined?
    emitter.instruction("jge __rt_array_diff_str_done_x86");                    // yes: return the destination
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // start the second-array cursor at 0

    emitter.label("__rt_array_diff_str_inner_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the second array pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the second-array cursor
    emitter.instruction("cmp r8, QWORD PTR [r11]");                             // was the candidate absent from the second array?
    emitter.instruction("jge __rt_array_diff_str_keep_x86");                    // yes: keep it
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the first array pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the first-array cursor
    emitter.instruction("shl rcx, 4");                                          // scale the cursor by the 16-byte string slot
    emitter.instruction("lea r10, [r10 + rcx + 24]");                           // address the candidate {pointer, length} pair
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // load the candidate pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // load the candidate length
    emitter.instruction("shl r8, 4");                                           // scale the second cursor by the 16-byte string slot
    emitter.instruction("lea r11, [r11 + r8 + 24]");                            // address the excluded {pointer, length} pair
    emitter.instruction("mov rdx, QWORD PTR [r11]");                            // load the excluded pointer
    emitter.instruction("mov rcx, QWORD PTR [r11 + 8]");                        // load the excluded length
    emitter.instruction("call __rt_str_eq");                                    // compare the two strings byte for byte
    emitter.instruction("test rax, rax");                                       // were they equal?
    emitter.instruction("jnz __rt_array_diff_str_next_x86");                    // an equal string removes the candidate
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // advance to the next excluded string
    emitter.instruction("jmp __rt_array_diff_str_inner_x86");                   // keep searching the second array

    emitter.label("__rt_array_diff_str_keep_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the first array pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the first-array cursor
    emitter.instruction("shl rcx, 4");                                          // scale the cursor by the 16-byte string slot
    emitter.instruction("lea r10, [r10 + rcx + 24]");                           // address the candidate {pointer, length} pair
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the borrowed candidate pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the borrowed candidate length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the destination array pointer
    emitter.instruction("call __rt_array_push_str");                            // append a PERSISTED copy of the candidate
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the possibly-grown destination

    emitter.label("__rt_array_diff_str_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next candidate
    emitter.instruction("jmp __rt_array_diff_str_outer_x86");                   // examine the next candidate

    emitter.label("__rt_array_diff_str_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the destination array pointer
    emitter.instruction("add rsp, 48");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the difference
}
