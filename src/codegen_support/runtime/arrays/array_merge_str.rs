//! Purpose:
//! Emits the `__rt_array_merge_str` runtime helper assembly for merging two indexed string arrays.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - An indexed `array<string>` stores 16-byte `{pointer, length}` slots. `__rt_array_merge`
//!   copies 8 bytes per element, so it cannot carry a string pair (issue #675).
//! - The result is renumbered from zero, as PHP's `array_merge()` does for integer keys. Every
//!   pair is DUPLICATED through `__rt_array_push_str`, so both inputs keep their own bytes.
//! - The two inputs are copied by one loop run twice (a pass counter selects the source), so the
//!   helper needs no local subroutine call.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_array_merge_str` runtime helper.
///
/// Appends every string of the first array, then every string of the second, into a new indexed
/// string array. Neither input is modified.
///
/// ## ABI
/// - **ARM64**: `x0` = first array, `x1` = second array. Result returned in `x0`.
/// - **x86_64 Linux**: `rdi` = first array, `rsi` = second array. Result returned in `rax`.
pub fn emit_array_merge_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_merge_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_merge_str ---");
    emitter.label_global("__rt_array_merge_str");

    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save first array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save second array pointer
    emitter.instruction("ldr x2, [x0]");                                        // load the first length
    emitter.instruction("ldr x3, [x1]");                                        // load the second length
    emitter.instruction("add x0, x2, x3");                                      // destination capacity = both lengths
    emitter.instruction("mov x1, #16");                                         // request 16-byte slots for {pointer, length} pairs
    emitter.instruction("bl __rt_array_new");                                   // allocate the destination array
    emitter.instruction("str x0, [sp, #16]");                                   // save destination array pointer
    emitter.instruction("str xzr, [sp, #40]");                                  // start with pass 0 (the first array)

    emitter.label("__rt_array_merge_str_pass");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the pass counter
    emitter.instruction("cmp x9, #2");                                          // have both inputs been copied?
    emitter.instruction("b.ge __rt_array_merge_str_done");                      // yes: return the destination
    emitter.instruction("ldr x10, [sp, #0]");                                   // pass 0 copies the first array
    emitter.instruction("cbz x9, __rt_array_merge_str_source_ready");           // keep it for pass 0
    emitter.instruction("ldr x10, [sp, #8]");                                   // pass 1 copies the second array
    emitter.label("__rt_array_merge_str_source_ready");
    emitter.instruction("str x10, [sp, #32]");                                  // save the current source array pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // start the source cursor at 0

    emitter.label("__rt_array_merge_str_copy");
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the current source array pointer
    emitter.instruction("ldr x11, [x10]");                                      // load its length
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the source cursor
    emitter.instruction("cmp x9, x11");                                         // has this source been copied completely?
    emitter.instruction("b.ge __rt_array_merge_str_next_pass");                 // move on to the next input
    emitter.instruction("add x10, x10, #24");                                   // compute source data base past the 24-byte header
    emitter.instruction("add x10, x10, x9, lsl #4");                            // address the source {pointer, length} pair
    emitter.instruction("ldp x1, x2, [x10]");                                   // load the borrowed source pointer and length
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_str");                              // append a PERSISTED copy of the pair
    emitter.instruction("str x0, [sp, #16]");                                   // persist destination pointer after possible growth
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the cursor across the append call
    emitter.instruction("add x9, x9, #1");                                      // advance the source cursor
    emitter.instruction("str x9, [sp, #24]");                                   // spill the updated cursor
    emitter.instruction("b __rt_array_merge_str_copy");                         // continue copying this source

    emitter.label("__rt_array_merge_str_next_pass");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the pass counter
    emitter.instruction("add x9, x9, #1");                                      // advance to the next input
    emitter.instruction("str x9, [sp, #40]");                                   // spill the updated pass counter
    emitter.instruction("b __rt_array_merge_str_pass");                         // start the next pass

    emitter.label("__rt_array_merge_str_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return the merged array
}

/// Emits the x86_64 Linux variant of `__rt_array_merge_str`.
///
/// Same semantics as the ARM64 variant under the System V AMD64 ABI: `rdi`, `rsi` in, `rax` out.
fn emit_array_merge_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge_str ---");
    emitter.label_global("__rt_array_merge_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // reserve spill slots, keeping calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the first array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the second array pointer
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the first length
    emitter.instruction("add rax, QWORD PTR [rsi]");                            // add the second length
    emitter.instruction("mov rdi, rax");                                        // destination capacity = both lengths
    emitter.instruction("mov rsi, 16");                                         // request 16-byte slots for {pointer, length} pairs
    emitter.instruction("call __rt_array_new");                                 // allocate the destination array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the destination array pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // start with pass 0 (the first array)

    emitter.label("__rt_array_merge_str_pass_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the pass counter
    emitter.instruction("cmp rcx, 2");                                          // have both inputs been copied?
    emitter.instruction("jge __rt_array_merge_str_done_x86");                   // yes: return the destination
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // pass 0 copies the first array
    emitter.instruction("test rcx, rcx");                                       // is this pass 0?
    emitter.instruction("jz __rt_array_merge_str_source_ready_x86");            // keep the first array
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // pass 1 copies the second array
    emitter.label("__rt_array_merge_str_source_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the current source array pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the source cursor at 0

    emitter.label("__rt_array_merge_str_copy_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current source array pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the source cursor
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // has this source been copied completely?
    emitter.instruction("jge __rt_array_merge_str_next_pass_x86");              // move on to the next input
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute source data base past the 24-byte header
    emitter.instruction("shl rcx, 4");                                          // scale the cursor by the 16-byte string slot
    emitter.instruction("add r10, rcx");                                        // address the source {pointer, length} pair
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the borrowed source pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the borrowed source length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the destination array pointer
    emitter.instruction("call __rt_array_push_str");                            // append a PERSISTED copy of the pair
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the possibly-grown destination
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the cursor across the append call
    emitter.instruction("add rcx, 1");                                          // advance the source cursor
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // spill the updated cursor
    emitter.instruction("jmp __rt_array_merge_str_copy_x86");                   // continue copying this source

    emitter.label("__rt_array_merge_str_next_pass_x86");
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // advance to the next input
    emitter.instruction("jmp __rt_array_merge_str_pass_x86");                   // start the next pass

    emitter.label("__rt_array_merge_str_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the destination array pointer
    emitter.instruction("add rsp, 48");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the merged array
}
