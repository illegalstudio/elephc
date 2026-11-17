//! Purpose:
//! Emits the `__rt_array_reduce_int_direct` runtime helper for typed int reducers.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_runtime()`.
//!
//! Key details:
//! - The source is a packed `array<int>` and the callback is a capture-free closure with two
//!   `int` parameters and an `int` return, called through its raw entry point.
//! - The accumulator stays a raw integer in a callee-saved register; no Mixed cell is allocated
//!   per element.
//! - The helper owns no heap values, so a throwing callback has nothing to leak; the lowering
//!   still admits only reducers without observable state mutation.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the direct int reduce helper for the active target.
///
/// AArch64: x0 = callback, x1 = source array, x2 = initial int, x3 = ignored environment;
/// x0 = final int. x86_64: rdi = callback, rsi = source, rdx = initial int, rcx = environment;
/// rax = final int.
pub fn emit_array_reduce_int_direct(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_reduce_int_direct_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_reduce_int_direct ---");
    emitter.label_global("__rt_array_reduce_int_direct");
    emitter.instruction("sub sp, sp, #64");                                     // reserve callback, source, cursor, length, and accumulator saves
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a stable frame base
    emitter.instruction("stp x19, x20, [sp, #0]");                              // save the callback and source array
    emitter.instruction("stp x21, x22, [sp, #16]");                             // save the cursor and source length
    emitter.instruction("str x23, [sp, #32]");                                  // save the raw accumulator
    emitter.instruction("mov x19, x0");                                         // keep the reducer entry across calls
    emitter.instruction("mov x20, x1");                                         // keep the packed source array
    emitter.instruction("mov x23, x2");                                         // seed the raw integer accumulator
    emitter.instruction("ldr x22, [x1]");                                       // read the source length
    emitter.instruction("mov x21, #0");                                         // start at the first source element

    emitter.label("__rt_array_reduce_int_direct_loop");
    emitter.instruction("cmp x21, x22");                                        // has every source element been reduced?
    emitter.instruction("b.ge __rt_array_reduce_int_direct_done");              // finish once the source is exhausted
    emitter.instruction("add x9, x20, #24");                                    // address the packed source payload
    emitter.instruction("ldr x1, [x9, x21, lsl #3]");                           // pass the current int element as the second parameter
    emitter.instruction("mov x0, x23");                                         // pass the raw accumulator as the first parameter
    emitter.instruction("blr x19");                                             // call the typed int reducer
    emitter.instruction("mov x23, x0");                                         // keep the returned int accumulator
    emitter.instruction("add x21, x21, #1");                                    // advance to the next source element
    emitter.instruction("b __rt_array_reduce_int_direct_loop");                 // continue reducing

    emitter.label("__rt_array_reduce_int_direct_done");
    emitter.instruction("mov x0, x23");                                         // return the final raw int accumulator
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore the callback and source array
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore the cursor and source length
    emitter.instruction("ldr x23, [sp, #32]");                                  // restore the raw accumulator slot
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the final int accumulator
}

/// Emits the direct int reduce helper for Linux x86_64.
fn emit_array_reduce_int_direct_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_reduce_int_direct ---");
    emitter.label_global("__rt_array_reduce_int_direct");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push rbx");                                            // save the raw accumulator
    emitter.instruction("push r12");                                            // save the reducer entry
    emitter.instruction("push r13");                                            // save the packed source array
    emitter.instruction("push r14");                                            // save the loop cursor
    emitter.instruction("push r15");                                            // save the source length
    emitter.instruction("sub rsp, 8");                                          // align the stack before callback calls
    emitter.instruction("mov r12, rdi");                                        // keep the reducer entry
    emitter.instruction("mov r13, rsi");                                        // keep the packed source array
    emitter.instruction("mov rbx, rdx");                                        // seed the raw integer accumulator
    emitter.instruction("mov r15, QWORD PTR [rsi]");                            // read the source length
    emitter.instruction("xor r14d, r14d");                                      // start at the first source element

    emitter.label("__rt_array_reduce_int_direct_loop_x86_64");
    emitter.instruction("cmp r14, r15");                                        // has every source element been reduced?
    emitter.instruction("jge __rt_array_reduce_int_direct_done_x86_64");        // finish once the source is exhausted
    emitter.instruction("mov rsi, QWORD PTR [r13 + r14 * 8 + 24]");             // pass the current int element as the second parameter
    emitter.instruction("mov rdi, rbx");                                        // pass the raw accumulator as the first parameter
    emitter.instruction("call r12");                                            // call the typed int reducer
    emitter.instruction("mov rbx, rax");                                        // keep the returned int accumulator
    emitter.instruction("inc r14");                                             // advance to the next source element
    emitter.instruction("jmp __rt_array_reduce_int_direct_loop_x86_64");        // continue reducing

    emitter.label("__rt_array_reduce_int_direct_done_x86_64");
    emitter.instruction("mov rax, rbx");                                        // return the final raw int accumulator
    emitter.instruction("add rsp, 8");                                          // release the alignment slot
    emitter.instruction("pop r15");                                             // restore the source length
    emitter.instruction("pop r14");                                             // restore the loop cursor
    emitter.instruction("pop r13");                                             // restore the packed source array
    emitter.instruction("pop r12");                                             // restore the reducer entry
    emitter.instruction("pop rbx");                                             // restore the raw accumulator
    emitter.instruction("leave");                                               // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the final int accumulator
}
