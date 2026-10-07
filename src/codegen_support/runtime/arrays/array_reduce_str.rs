//! Purpose:
//! Emits the `__rt_array_reduce_str` runtime helper assembly used by
//! `array_reduce()` when the source is an indexed string array. String arrays
//! store 16-byte `[ptr:8][len:8]` payload slots, so the 8-byte element loader in
//! `__rt_array_reduce` would misread them.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The accumulator stays a single integer-register value; only the element is
//!   widened to a pointer/length pair. The lowering side rejects string
//!   accumulators, so no intermediate string ever has to be persisted or freed
//!   here.
//! - Callback ABI: AArch64 `x0` = accumulator, `x1`/`x2` = element pointer/length,
//!   `x3` = optional capture environment; x86_64 `rdi`, `rsi`/`rdx`, `rcx`. The
//!   new accumulator is read back from `x0`/`rax`.
//! - All loop state lives in the frame because the callback may clobber every
//!   caller-saved register; the helper touches no callee-saved register other
//!   than the frame pointer.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// array_reduce_str: folds an indexed string array into one integer accumulator.
///
/// Input: AArch64 `x0` = callback address, `x1` = source array pointer,
/// `x2` = initial accumulator, `x3` = optional capture environment pointer;
/// x86_64 `rdi` / `rsi` / `rdx` / `rcx` respectively.
/// Output: `x0` / `rax` = the accumulator returned by the final callback call, or
/// the initial value when the source array is empty.
pub fn emit_array_reduce_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_reduce_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_reduce_str ---");
    emitter.label_global("__rt_array_reduce_str");

    // Frame (80 bytes): [0]=data base [8]=length [16]=i [24]=accumulator
    //                   [32]=callback [40]=env [64]=x29,x30
    emitter.instruction("sub sp, sp, #80");                                     // reserve the fold state that must survive callback calls
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #32]");                                   // save the callback address for every loop iteration
    emitter.instruction("str x2, [sp, #24]");                                   // seed the accumulator with the initial value
    emitter.instruction("str x3, [sp, #40]");                                   // save the optional callback capture environment pointer
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length from the header
    emitter.instruction("str x9, [sp, #8]");                                    // save the source array length
    emitter.instruction("add x9, x1, #24");                                     // x9 = base of the data region (skip header)
    emitter.instruction("str x9, [sp, #0]");                                    // save the data base
    emitter.instruction("mov x9, xzr");                                         // loop index i = 0
    emitter.instruction("str x9, [sp, #16]");                                   // save i

    emitter.label("__rt_array_reduce_str_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload i
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the source array length
    emitter.instruction("cmp x9, x10");                                         // compare i with the source array length
    emitter.instruction("b.ge __rt_array_reduce_str_done");                     // i >= length: the fold is complete
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the data base
    emitter.instruction("add x10, x10, x9, lsl #4");                            // x10 = &data[i] (16-byte string slots)
    emitter.instruction("ldr x0, [sp, #24]");                                   // callback arg 0: current accumulator
    emitter.instruction("ldr x1, [x10]");                                       // callback arg 1: element string pointer
    emitter.instruction("ldr x2, [x10, #8]");                                   // callback arg 2: element string length
    emitter.instruction("ldr x3, [sp, #40]");                                   // pass the capture environment after the element pair
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the callback address
    emitter.instruction("blr x9");                                              // x0 = callback(accumulator, element)
    emitter.instruction("str x0, [sp, #24]");                                   // accumulator = callback result
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload i after the callback clobbered caller-saved registers
    emitter.instruction("add x9, x9, #1");                                      // i += 1
    emitter.instruction("str x9, [sp, #16]");                                   // save i
    emitter.instruction("b __rt_array_reduce_str_loop");                        // continue folding the remaining elements

    emitter.label("__rt_array_reduce_str_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // return the final accumulator
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the fold state frame
    emitter.instruction("ret");                                                 // return with x0 = accumulated value
}

/// x86_64 Linux implementation of the `__rt_array_reduce_str` runtime helper.
///
/// Inputs (System V): `rdi` = callback address, `rsi` = source array pointer,
/// `rdx` = initial accumulator, `rcx` = optional capture environment pointer.
/// The callback is invoked with `rdi` = accumulator, `rsi`/`rdx` = element
/// pointer/length, `rcx` = environment, and returns the new accumulator in `rax`.
/// Emits `__rt_array_reduce_str` as a global label.
fn emit_array_reduce_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_reduce_str ---");
    emitter.label_global("__rt_array_reduce_str");

    // Frame (rbp-relative): [-8]=data base [-16]=length [-24]=i
    //                       [-32]=accumulator [-40]=callback [-48]=env
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve the fold state slots and keep rsp 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save the callback address for every loop iteration
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // seed the accumulator with the initial value
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the optional callback capture environment pointer
    emitter.instruction("mov r8, QWORD PTR [rsi]");                             // r8 = source array length from the header
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // save the source array length
    emitter.instruction("lea r8, [rsi + 24]");                                  // r8 = base of the data region (skip header)
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // save the data base
    emitter.instruction("xor r8d, r8d");                                        // loop index i = 0
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // save i

    emitter.label("__rt_array_reduce_str_loop_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload i
    emitter.instruction("cmp r9, QWORD PTR [rbp - 16]");                        // compare i with the source array length
    emitter.instruction("jge __rt_array_reduce_str_done_linux_x86_64");         // i >= length: the fold is complete
    emitter.instruction("shl r9, 4");                                           // i * 16 (16-byte string slots)
    emitter.instruction("add r9, QWORD PTR [rbp - 8]");                         // r9 = &data[i]
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // callback arg 0: current accumulator
    emitter.instruction("mov rsi, QWORD PTR [r9]");                             // callback arg 1: element string pointer
    emitter.instruction("mov rdx, QWORD PTR [r9 + 8]");                         // callback arg 2: element string length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // pass the capture environment after the element pair
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the callback address
    emitter.emit_platform_callback_call("r11", 4);                                // call generated PHP callback with the target ABI
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // accumulator = callback result
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload i after the callback clobbered caller-saved registers
    emitter.instruction("add r9, 1");                                           // i += 1
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // save i
    emitter.instruction("jmp __rt_array_reduce_str_loop_linux_x86_64");         // continue folding the remaining elements

    emitter.label("__rt_array_reduce_str_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the final accumulator
    emitter.instruction("add rsp, 48");                                         // release the fold state slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with rax = accumulated value
}
