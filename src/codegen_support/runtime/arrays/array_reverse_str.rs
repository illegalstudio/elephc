//! Purpose:
//! Emits the `__rt_array_reverse_str` runtime helper assembly for reversing an indexed string array.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - An indexed `array<string>` stores 16-byte `{pointer, length}` slots. `__rt_array_reverse`
//!   copies 8 bytes per element, so it cannot carry a string pair (issue #675).
//! - Ownership follows `__rt_array_slice_str`: every copied pair is DUPLICATED through
//!   `__rt_array_push_str`, so the source and the result own independent bytes.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_array_reverse_str` runtime helper.
///
/// Builds a new indexed string array holding the source strings in reverse order. The source is
/// never modified.
///
/// ## ABI
/// - **ARM64**: `x0` = source array pointer. Result returned in `x0`.
/// - **x86_64 Linux**: `rdi` = source array pointer. Result returned in `rax`.
pub fn emit_array_reverse_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_reverse_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_reverse_str ---");
    emitter.label_global("__rt_array_reverse_str");

    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load the source length
    emitter.instruction("str x9, [sp, #8]");                                    // save the source length
    emitter.instruction("mov x0, x9");                                          // destination capacity = source length
    emitter.instruction("mov x1, #16");                                         // request 16-byte slots for {pointer, length} pairs
    emitter.instruction("bl __rt_array_new");                                   // allocate the destination array
    emitter.instruction("str x0, [sp, #16]");                                   // save destination array pointer
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the source length
    emitter.instruction("sub x9, x9, #1");                                      // start the source cursor at the last element
    emitter.instruction("str x9, [sp, #24]");                                   // spill the descending source cursor

    // The cursor is SPILLED: `__rt_array_push_str` persists the payload and may grow the
    // destination, so every caller-saved register is fair game across it.
    emitter.label("__rt_array_reverse_str_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the descending source cursor
    emitter.instruction("cmp x9, #0");                                          // has the cursor passed the first element?
    emitter.instruction("b.lt __rt_array_reverse_str_done");                    // every element has been copied
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x1, x1, #24");                                     // compute source data base past the 24-byte header
    emitter.instruction("add x1, x1, x9, lsl #4");                              // address the source {pointer, length} pair
    emitter.instruction("ldp x1, x2, [x1]");                                    // load the borrowed source pointer and length
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_str");                              // append a PERSISTED copy of the pair
    emitter.instruction("str x0, [sp, #16]");                                   // persist destination pointer after possible growth
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the cursor across the append call
    emitter.instruction("sub x9, x9, #1");                                      // step the cursor one element toward the start
    emitter.instruction("str x9, [sp, #24]");                                   // spill the updated cursor
    emitter.instruction("b __rt_array_reverse_str_loop");                       // continue copying

    emitter.label("__rt_array_reverse_str_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return the reversed array
}

/// Emits the x86_64 Linux variant of `__rt_array_reverse_str`.
///
/// Same semantics as the ARM64 variant under the System V AMD64 ABI: `rdi` in, `rax` out.
fn emit_array_reverse_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_reverse_str ---");
    emitter.label_global("__rt_array_reverse_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve spill slots, keeping calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source array pointer
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the source length
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the source length
    emitter.instruction("mov rdi, rax");                                        // destination capacity = source length
    emitter.instruction("mov rsi, 16");                                         // request 16-byte slots for {pointer, length} pairs
    emitter.instruction("call __rt_array_new");                                 // allocate the destination array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the destination array pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the source length
    emitter.instruction("sub rax, 1");                                          // start the source cursor at the last element
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // spill the descending source cursor

    emitter.label("__rt_array_reverse_str_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the descending source cursor
    emitter.instruction("cmp rcx, 0");                                          // has the cursor passed the first element?
    emitter.instruction("jl __rt_array_reverse_str_done_x86");                  // every element has been copied
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source array pointer
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute source data base past the 24-byte header
    emitter.instruction("shl rcx, 4");                                          // scale the cursor by the 16-byte string slot
    emitter.instruction("add r10, rcx");                                        // address the source {pointer, length} pair
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the borrowed source pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the borrowed source length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the destination array pointer
    emitter.instruction("call __rt_array_push_str");                            // append a PERSISTED copy of the pair
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the possibly-grown destination
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the cursor across the append call
    emitter.instruction("sub rcx, 1");                                          // step the cursor one element toward the start
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // spill the updated cursor
    emitter.instruction("jmp __rt_array_reverse_str_loop_x86");                 // continue copying

    emitter.label("__rt_array_reverse_str_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the destination array pointer
    emitter.instruction("add rsp, 32");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the reversed array
}
