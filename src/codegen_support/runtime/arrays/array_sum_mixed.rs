//! Purpose:
//! Emits `__rt_array_sum_mixed` for indexed arrays whose slots hold boxed Mixed cells.
//! Coerces each element through the shared PHP integer-cast helper before accumulation.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - The helper supports ARM64 and Linux x86_64 ABIs and never consumes the element boxes.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the target-specific boxed-Mixed indexed-array sum helper.
///
/// The array pointer arrives in `x0`/`rdi`; each 8-byte slot is a borrowed
/// Mixed-cell pointer and is coerced without changing its ownership. The
/// integer result is returned in `x0`/`rax`.
pub fn emit_array_sum_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_sum_mixed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_sum_mixed ---");
    emitter.label_global("__rt_array_sum_mixed");

    // -- preserve loop state across mixed coercion calls --
    emitter.instruction("sub sp, sp, #64");                                     // reserve aligned frame storage for array pointer, loop state, and saved linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer above the local loop state
    emitter.instruction("str x0, [sp]");                                        // save the source indexed-array pointer for each loop iteration
    emitter.instruction("cbz x0, __rt_array_sum_mixed_zero");                   // treat a null-container pointer as an empty array
    emitter.instruction("ldr x9, [x0]");                                        // load the source indexed-array logical length
    emitter.instruction("str x9, [sp, #8]");                                    // preserve the logical length across mixed coercion calls
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize the element cursor at index zero
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize the integer accumulator at zero

    // -- coerce and accumulate every boxed element --
    emitter.label("__rt_array_sum_mixed_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the current element cursor
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the source logical length
    emitter.instruction("cmp x9, x10");                                         // compare the current element cursor with the logical length
    emitter.instruction("b.ge __rt_array_sum_mixed_done");                      // finish after every boxed element has contributed
    emitter.instruction("ldr x10, [sp]");                                       // reload the source indexed-array pointer
    emitter.instruction("add x10, x10, #24");                                   // advance from the array header to the boxed slot region
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // load the borrowed Mixed-cell pointer for the current element
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce the boxed element through PHP integer conversion rules
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the running integer accumulator
    emitter.instruction("add x9, x9, x0");                                      // add the coerced element to the running sum
    emitter.instruction("str x9, [sp, #24]");                                   // preserve the updated accumulator across the next coercion call
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the element cursor after the coercion call
    emitter.instruction("add x9, x9, #1");                                      // advance to the next boxed element
    emitter.instruction("str x9, [sp, #16]");                                   // preserve the advanced cursor for the next loop iteration
    emitter.instruction("b __rt_array_sum_mixed_loop");                         // continue until the source logical length is exhausted

    // -- return the integer sum --
    emitter.label("__rt_array_sum_mixed_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // return the accumulated integer sum
    emitter.instruction("b __rt_array_sum_mixed_epilogue");                     // share frame restoration with the null-container path

    emitter.label("__rt_array_sum_mixed_zero");
    emitter.instruction("mov x0, #0");                                          // return the additive identity for an empty or null-container array

    emitter.label("__rt_array_sum_mixed_epilogue");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the aligned helper frame
    emitter.instruction("ret");                                                 // return the integer sum to the generated caller
}

/// Emits the Linux x86_64 System V implementation of boxed-Mixed array sum.
fn emit_array_sum_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_sum_mixed ---");
    emitter.label_global("__rt_array_sum_mixed");

    // -- preserve loop state across mixed coercion calls --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and align the stack for nested calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the loop locals
    emitter.instruction("sub rsp, 32");                                         // reserve array pointer, length, cursor, and accumulator slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source indexed-array pointer for each loop iteration
    emitter.instruction("test rdi, rdi");                                       // check whether the source is the null-container sentinel
    emitter.instruction("je __rt_array_sum_mixed_zero_x86");                    // treat a null-container pointer as an empty array
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the source indexed-array logical length
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the logical length across mixed coercion calls
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the element cursor at index zero
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the integer accumulator at zero

    // -- coerce and accumulate every boxed element --
    emitter.label("__rt_array_sum_mixed_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the current element cursor
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare the cursor with the source logical length
    emitter.instruction("jge __rt_array_sum_mixed_done_x86");                   // finish after every boxed element has contributed
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer
    emitter.instruction("mov rax, QWORD PTR [rax + rcx * 8 + 24]");             // load the borrowed Mixed-cell pointer in the x86 Mixed-helper input register
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce the boxed element through PHP integer conversion rules
    emitter.instruction("add QWORD PTR [rbp - 32], rax");                       // add the coerced element to the running sum
    emitter.instruction("add QWORD PTR [rbp - 24], 1");                         // advance the element cursor after consuming the current slot
    emitter.instruction("jmp __rt_array_sum_mixed_loop_x86");                   // continue until the source logical length is exhausted

    // -- return the integer sum --
    emitter.label("__rt_array_sum_mixed_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the accumulated integer sum
    emitter.instruction("jmp __rt_array_sum_mixed_epilogue_x86");               // share frame restoration with the null-container path

    emitter.label("__rt_array_sum_mixed_zero_x86");
    emitter.instruction("xor eax, eax");                                        // return the additive identity for an empty or null-container array

    emitter.label("__rt_array_sum_mixed_epilogue_x86");
    emitter.instruction("mov rsp, rbp");                                        // release all helper-local loop storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the integer sum to the generated caller
}
