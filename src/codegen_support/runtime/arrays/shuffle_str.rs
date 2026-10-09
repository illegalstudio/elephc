//! Purpose:
//! Emits the `__rt_shuffle_str` runtime helper assembly for shuffling an indexed string array in place.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - An indexed `array<string>` stores 16-byte `{pointer, length}` slots. `__rt_shuffle` swaps
//!   8-byte slots, so it would tear string pairs apart (issue #675).
//! - A swap moves whole pairs between slots of the same array, so ownership is unchanged. The
//!   lowering has already split a shared receiver before this runs.
//! - The Fisher-Yates cursor lives in a stack slot: `__rt_random_uniform` is a full call.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_shuffle_str` runtime helper.
///
/// Shuffles an indexed string array in place with Fisher-Yates, drawing each partner index from
/// `__rt_random_uniform`, exactly like `__rt_shuffle`.
///
/// ## ABI
/// - **ARM64**: `x0` = array pointer. No return value.
/// - **x86_64 Linux**: `rdi` = array pointer. No return value.
pub fn emit_shuffle_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_shuffle_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: shuffle_str ---");
    emitter.label_global("__rt_shuffle_str");

    emitter.instruction("sub sp, sp, #32");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load the array length
    emitter.instruction("sub x9, x9, #1");                                      // start the cursor at the last slot
    emitter.instruction("str x9, [sp, #8]");                                    // spill the descending cursor

    emitter.label("__rt_shuffle_str_loop");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the descending cursor
    emitter.instruction("cmp x9, #1");                                          // is there still a slot above index 0 to place?
    emitter.instruction("b.lt __rt_shuffle_str_done");                          // no: the array is shuffled
    emitter.instruction("add x0, x9, #1");                                      // exclusive upper bound i + 1
    emitter.instruction("bl __rt_random_uniform");                              // draw a partner index j in [0, i]
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the array pointer
    emitter.instruction("add x1, x1, #24");                                     // compute the data base past the 24-byte header
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the cursor across the random call
    emitter.instruction("add x3, x1, x9, lsl #4");                              // address slot i
    emitter.instruction("add x4, x1, x0, lsl #4");                              // address slot j
    emitter.instruction("ldp x5, x6, [x3]");                                    // load the pair in slot i
    emitter.instruction("ldp x7, x8, [x4]");                                    // load the pair in slot j
    emitter.instruction("stp x7, x8, [x3]");                                    // move pair j into slot i
    emitter.instruction("stp x5, x6, [x4]");                                    // move pair i into slot j
    emitter.instruction("sub x9, x9, #1");                                      // step the cursor down
    emitter.instruction("str x9, [sp, #8]");                                    // spill the updated cursor
    emitter.instruction("b __rt_shuffle_str_loop");                             // continue shuffling

    emitter.label("__rt_shuffle_str_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return after shuffling in place
}

/// Emits the x86_64 Linux variant of `__rt_shuffle_str`.
///
/// Same semantics as the ARM64 variant under the System V AMD64 ABI: `rdi` in, no return value.
fn emit_shuffle_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: shuffle_str ---");
    emitter.label_global("__rt_shuffle_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 16");                                         // reserve spill slots, keeping calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the array pointer
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the array length
    emitter.instruction("sub rax, 1");                                          // start the cursor at the last slot
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // spill the descending cursor

    emitter.label("__rt_shuffle_str_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the descending cursor
    emitter.instruction("cmp r10, 1");                                          // is there still a slot above index 0 to place?
    emitter.instruction("jl __rt_shuffle_str_done_x86");                        // no: the array is shuffled
    emitter.instruction("lea rdi, [r10 + 1]");                                  // exclusive upper bound i + 1
    emitter.instruction("call __rt_random_uniform");                            // draw a partner index j in [0, i]
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the cursor across the random call
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the array pointer
    emitter.instruction("lea r8, [r8 + 24]");                                   // compute the data base past the 24-byte header
    emitter.instruction("mov r9, r10");                                         // copy the cursor before scaling it
    emitter.instruction("shl r9, 4");                                           // scale i by the 16-byte string slot
    emitter.instruction("add r9, r8");                                          // address slot i
    emitter.instruction("shl rax, 4");                                          // scale j by the 16-byte string slot
    emitter.instruction("add rax, r8");                                         // address slot j
    emitter.instruction("mov rcx, QWORD PTR [r9]");                             // load the pointer in slot i
    emitter.instruction("mov rdx, QWORD PTR [r9 + 8]");                         // load the length in slot i
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // load the pointer in slot j
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the length in slot j
    emitter.instruction("mov QWORD PTR [r9], rsi");                             // move pointer j into slot i
    emitter.instruction("mov QWORD PTR [r9 + 8], rdi");                         // move length j into slot i
    emitter.instruction("mov QWORD PTR [rax], rcx");                            // move pointer i into slot j
    emitter.instruction("mov QWORD PTR [rax + 8], rdx");                        // move length i into slot j
    emitter.instruction("sub r10, 1");                                          // step the cursor down
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // spill the updated cursor
    emitter.instruction("jmp __rt_shuffle_str_loop_x86");                       // continue shuffling

    emitter.label("__rt_shuffle_str_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return after shuffling in place
}
