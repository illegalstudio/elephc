//! Purpose:
//! Emits the `__rt_array_pad_str` runtime helper assembly for padding an indexed string array.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - An indexed `array<string>` stores 16-byte `{pointer, length}` slots. `__rt_array_pad`
//!   copies 8 bytes per element, so it cannot carry a string pair (issue #675).
//! - Every slot owns its bytes: the source strings and each pad copy are DUPLICATED through
//!   `__rt_array_push_str`, so the pad value stays borrowed.
//! - `abs(size)` is clamped like `__rt_array_pad`: `INT64_MIN` has no representable magnitude,
//!   so it counts as zero. The lowering bounds `$length` first and raises PHP's `ValueError`.
//! - The fill loop and the copy loop are each emitted once; a stage counter runs them in the
//!   order the sign of `size` asks for (pad first when negative, copy first otherwise).

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_array_pad_str` runtime helper.
///
/// Builds a new indexed string array holding the source strings plus copies of the pad string
/// up to `abs(size)` elements: appended when `size` is positive, prepended when negative. A size
/// no larger than the source yields a copy of the source.
///
/// ## ABI
/// - **ARM64**: `x0` = source array, `x1` = size, `x2` = pad string pointer, `x3` = pad string
///   length. Result returned in `x0`.
/// - **x86_64 Linux**: `rdi`, `rsi`, `rdx`, `rcx` with the same meaning. Result in `rax`.
pub fn emit_array_pad_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_pad_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_pad_str ---");
    emitter.label_global("__rt_array_pad_str");

    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested size
    emitter.instruction("stp x2, x3, [sp, #16]");                               // save the borrowed pad string pointer and length
    emitter.instruction("ldr x9, [x0]");                                        // load the source length
    emitter.instruction("cmp x1, #0");                                          // is the requested size negative?
    emitter.instruction("cneg x4, x1, lt");                                     // x4 = abs(size)
    emitter.instruction("cmp x4, #0");                                          // INT64_MIN stays negative after the negation
    emitter.instruction("csel x4, x4, xzr, ge");                                // clamp that wrapped magnitude to zero
    emitter.instruction("subs x5, x4, x9");                                     // x5 = abs(size) - source length
    emitter.instruction("csel x5, x5, xzr, gt");                                // pad count, zero when no padding is needed
    emitter.instruction("str x5, [sp, #40]");                                   // save the pad count
    emitter.instruction("add x0, x9, x5");                                      // destination capacity = source length + pad count
    emitter.instruction("mov x1, #16");                                         // request 16-byte slots for {pointer, length} pairs
    emitter.instruction("bl __rt_array_new");                                   // allocate the destination array
    emitter.instruction("str x0, [sp, #32]");                                   // save destination array pointer
    emitter.instruction("str xzr, [sp, #56]");                                  // start at stage 0
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the requested size
    emitter.instruction("cmp x1, #0");                                          // a negative size pads on the left
    emitter.instruction("b.lt __rt_array_pad_str_fill");                        // left padding: fill first
    emitter.instruction("b __rt_array_pad_str_copy");                           // right padding: copy first

    emitter.label("__rt_array_pad_str_fill");
    emitter.instruction("str xzr, [sp, #48]");                                  // start the fill counter at 0
    emitter.label("__rt_array_pad_str_fill_loop");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the fill counter
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the pad count
    emitter.instruction("cmp x9, x10");                                         // have enough pad copies been appended?
    emitter.instruction("b.ge __rt_array_pad_str_fill_end");                    // yes: finish this stage
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload the borrowed pad string
    emitter.instruction("bl __rt_array_push_str");                              // append a PERSISTED copy of the pad string
    emitter.instruction("str x0, [sp, #32]");                                   // persist destination pointer after possible growth
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the fill counter across the append call
    emitter.instruction("add x9, x9, #1");                                      // count one more pad copy
    emitter.instruction("str x9, [sp, #48]");                                   // spill the updated fill counter
    emitter.instruction("b __rt_array_pad_str_fill_loop");                      // continue filling
    emitter.label("__rt_array_pad_str_fill_end");
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload the stage counter
    emitter.instruction("add x9, x9, #1");                                      // this stage is complete
    emitter.instruction("str x9, [sp, #56]");                                   // spill the updated stage counter
    emitter.instruction("cmp x9, #2");                                          // have both stages run?
    emitter.instruction("b.ge __rt_array_pad_str_done");                        // yes: return the destination
    emitter.instruction("b __rt_array_pad_str_copy");                           // otherwise copy the source next

    emitter.label("__rt_array_pad_str_copy");
    emitter.instruction("str xzr, [sp, #48]");                                  // start the source cursor at 0
    emitter.label("__rt_array_pad_str_copy_loop");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload source array pointer
    emitter.instruction("ldr x11, [x10]");                                      // load the source length
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the source cursor
    emitter.instruction("cmp x9, x11");                                         // has the source been copied completely?
    emitter.instruction("b.ge __rt_array_pad_str_copy_end");                    // yes: finish this stage
    emitter.instruction("add x10, x10, #24");                                   // compute source data base past the 24-byte header
    emitter.instruction("add x10, x10, x9, lsl #4");                            // address the source {pointer, length} pair
    emitter.instruction("ldp x1, x2, [x10]");                                   // load the borrowed source pointer and length
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_str");                              // append a PERSISTED copy of the pair
    emitter.instruction("str x0, [sp, #32]");                                   // persist destination pointer after possible growth
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the cursor across the append call
    emitter.instruction("add x9, x9, #1");                                      // advance the source cursor
    emitter.instruction("str x9, [sp, #48]");                                   // spill the updated cursor
    emitter.instruction("b __rt_array_pad_str_copy_loop");                      // continue copying
    emitter.label("__rt_array_pad_str_copy_end");
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload the stage counter
    emitter.instruction("add x9, x9, #1");                                      // this stage is complete
    emitter.instruction("str x9, [sp, #56]");                                   // spill the updated stage counter
    emitter.instruction("cmp x9, #2");                                          // have both stages run?
    emitter.instruction("b.ge __rt_array_pad_str_done");                        // yes: return the destination
    emitter.instruction("b __rt_array_pad_str_fill");                           // otherwise pad next

    emitter.label("__rt_array_pad_str_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // return the destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return the padded array
}

/// Emits the x86_64 Linux variant of `__rt_array_pad_str`.
///
/// Same semantics as the ARM64 variant under the System V AMD64 ABI: `rdi`, `rsi`, `rdx`, `rcx`
/// in, `rax` out.
fn emit_array_pad_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_pad_str ---");
    emitter.label_global("__rt_array_pad_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                         // reserve spill slots, keeping calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the requested size
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the borrowed pad string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // preserve the borrowed pad string length
    emitter.instruction("mov rax, rsi");                                        // start from the requested size
    emitter.instruction("test rax, rax");                                       // is the requested size negative?
    emitter.instruction("jge __rt_array_pad_str_abs_ready_x86");                // non-negative sizes are their own magnitude
    emitter.instruction("neg rax");                                             // rax = abs(size)
    emitter.instruction("test rax, rax");                                       // INT64_MIN stays negative after the negation
    emitter.instruction("jge __rt_array_pad_str_abs_ready_x86");                // a representable magnitude is ready
    emitter.instruction("xor eax, eax");                                        // clamp that wrapped magnitude to zero
    emitter.label("__rt_array_pad_str_abs_ready_x86");
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source length
    emitter.instruction("sub rax, r10");                                        // rax = abs(size) - source length
    emitter.instruction("jg __rt_array_pad_str_count_ready_x86");               // a positive difference is the pad count
    emitter.instruction("xor eax, eax");                                        // no padding when the source is already long enough
    emitter.label("__rt_array_pad_str_count_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the pad count
    emitter.instruction("lea rdi, [r10 + rax]");                                // destination capacity = source length + pad count
    emitter.instruction("mov rsi, 16");                                         // request 16-byte slots for {pointer, length} pairs
    emitter.instruction("call __rt_array_new");                                 // allocate the destination array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the destination array pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // start at stage 0
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // a negative size pads on the left
    emitter.instruction("jl __rt_array_pad_str_fill_x86");                      // left padding: fill first
    emitter.instruction("jmp __rt_array_pad_str_copy_x86");                     // right padding: copy first

    emitter.label("__rt_array_pad_str_fill_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // start the fill counter at 0
    emitter.label("__rt_array_pad_str_fill_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the fill counter
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 48]");                       // have enough pad copies been appended?
    emitter.instruction("jge __rt_array_pad_str_fill_end_x86");                 // yes: finish this stage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the destination array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the borrowed pad string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the borrowed pad string length
    emitter.instruction("call __rt_array_push_str");                            // append a PERSISTED copy of the pad string
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // persist the possibly-grown destination
    emitter.instruction("add QWORD PTR [rbp - 56], 1");                         // count one more pad copy
    emitter.instruction("jmp __rt_array_pad_str_fill_loop_x86");                // continue filling
    emitter.label("__rt_array_pad_str_fill_end_x86");
    emitter.instruction("add QWORD PTR [rbp - 64], 1");                         // this stage is complete
    emitter.instruction("cmp QWORD PTR [rbp - 64], 2");                         // have both stages run?
    emitter.instruction("jge __rt_array_pad_str_done_x86");                     // yes: return the destination
    emitter.instruction("jmp __rt_array_pad_str_copy_x86");                     // otherwise copy the source next

    emitter.label("__rt_array_pad_str_copy_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // start the source cursor at 0
    emitter.label("__rt_array_pad_str_copy_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source array pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the source cursor
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // has the source been copied completely?
    emitter.instruction("jge __rt_array_pad_str_copy_end_x86");                 // yes: finish this stage
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute source data base past the 24-byte header
    emitter.instruction("shl rcx, 4");                                          // scale the cursor by the 16-byte string slot
    emitter.instruction("add r10, rcx");                                        // address the source {pointer, length} pair
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the borrowed source pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the borrowed source length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the destination array pointer
    emitter.instruction("call __rt_array_push_str");                            // append a PERSISTED copy of the pair
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // persist the possibly-grown destination
    emitter.instruction("add QWORD PTR [rbp - 56], 1");                         // advance the source cursor
    emitter.instruction("jmp __rt_array_pad_str_copy_loop_x86");                // continue copying
    emitter.label("__rt_array_pad_str_copy_end_x86");
    emitter.instruction("add QWORD PTR [rbp - 64], 1");                         // this stage is complete
    emitter.instruction("cmp QWORD PTR [rbp - 64], 2");                         // have both stages run?
    emitter.instruction("jge __rt_array_pad_str_done_x86");                     // yes: return the destination
    emitter.instruction("jmp __rt_array_pad_str_fill_x86");                     // otherwise pad next

    emitter.label("__rt_array_pad_str_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the destination array pointer
    emitter.instruction("add rsp, 64");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the padded array
}
