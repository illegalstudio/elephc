//! Purpose:
//! Emits the `__rt_abs_mixed` runtime helper assembly that computes PHP `abs()` on a boxed
//! Mixed value while preserving int-vs-float result typing.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Mirrors the `mixed_numeric_binops` pattern: unbox the payload (`__rt_mixed_unbox`),
//!   branch on the runtime tag, apply the integer or floating-point absolute value, then
//!   rebox via `__rt_mixed_from_value`. PHP keeps `abs(int)` integer and `abs(float)` float;
//!   non-numeric tags (bool/string/null) are coerced through `__rt_mixed_cast_int` first.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_abs_mixed` runtime helper.
///
/// Input: the boxed Mixed pointer in the integer result register (`x0` / `rax`).
/// Output: a freshly boxed Mixed pointer (int- or float-tagged) in the same register.
/// Dispatches to the x86_64 variant on Linux/x86_64; otherwise emits the ARM64 variant.
pub fn emit_mixed_abs(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_abs_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: abs_mixed ---");
    emitter.label_global("__rt_abs_mixed");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a frame for the saved pointer and link register
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across nested helper calls
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #-16]!");                                 // save the original boxed pointer for the non-numeric coercion fallback
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0 = runtime tag, x1 = value_lo, x2 = value_hi
    emitter.instruction("cmp x0, #2");                                          // does the payload hold a float?
    emitter.instruction("b.eq __rt_abs_mixed_float");                           // floats clear the IEEE-754 sign bit and stay float-typed
    emitter.instruction("cmp x0, #0");                                          // does the payload already hold an integer?
    emitter.instruction("b.eq __rt_abs_mixed_int");                             // ints use their unboxed payload directly
    emitter.instruction("ldr x0, [sp]");                                        // reload the original boxed pointer for the non-numeric coercion
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce bool/string/null payloads through PHP integer cast rules
    emitter.instruction("mov x1, x0");                                          // move the coerced integer into the absolute-value input register

    emitter.label("__rt_abs_mixed_int");
    emitter.instruction("cmp x1, #0");                                          // compare the integer payload against zero
    emitter.instruction("cneg x1, x1, lt");                                     // negate the integer only when it was negative
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("mov x2, #0");                                          // integer payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the integer absolute value into a Mixed cell
    emitter.instruction("b __rt_abs_mixed_done");                               // return the boxed integer result

    emitter.label("__rt_abs_mixed_float");
    emitter.instruction("fmov d0, x1");                                         // move the unboxed float bits into the FP register file
    emitter.instruction("fabs d0, d0");                                         // take the floating-point absolute value
    emitter.instruction("fmov x1, d0");                                         // move the absolute-value bits back for boxing
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = float
    emitter.instruction("mov x2, #0");                                          // float payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the float absolute value into a Mixed cell

    emitter.label("__rt_abs_mixed_done");
    emitter.instruction("add sp, sp, #16");                                     // discard the saved original boxed pointer slot
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed Mixed absolute value
}

/// x86_64 Linux variant of `emit_mixed_abs` using System V ABI register conventions.
///
/// `__rt_mixed_unbox` returns the tag in `rax` and the payload low word in `rdi`;
/// `__rt_mixed_from_value` takes the tag in `rax`, the value in `rdi`, and the high word
/// in `rsi`. The original boxed pointer is spilled to the frame for the non-numeric path.
fn emit_mixed_abs_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: abs_mixed ---");
    emitter.label_global("__rt_abs_mixed");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 16");                                         // reserve an aligned slot for the saved boxed pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the original boxed pointer for the non-numeric coercion fallback
    emitter.instruction("call __rt_mixed_unbox");                               // rax = runtime tag, rdi = value_lo, rdx = value_hi
    emitter.instruction("cmp rax, 2");                                          // does the payload hold a float?
    emitter.instruction("je __rt_abs_mixed_float_x86");                         // floats clear the IEEE-754 sign bit and stay float-typed
    emitter.instruction("cmp rax, 0");                                          // does the payload already hold an integer?
    emitter.instruction("je __rt_abs_mixed_int_x86");                           // ints use their unboxed payload directly
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the original boxed pointer for the non-numeric coercion
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce bool/string/null payloads through PHP integer cast rules
    emitter.instruction("mov rdi, rax");                                        // move the coerced integer into the absolute-value input register

    emitter.label("__rt_abs_mixed_int_x86");
    emitter.instruction("mov r10, rdi");                                        // copy the integer payload before branchless sign handling
    emitter.instruction("sar r10, 63");                                         // expand the sign bit into an all-zero or all-one mask
    emitter.instruction("xor rdi, r10");                                        // flip the payload bits when the integer was negative
    emitter.instruction("sub rdi, r10");                                        // subtract the sign mask to finish the two's-complement absolute value
    emitter.instruction("xor rsi, rsi");                                        // integer payloads do not use a high word
    emitter.instruction("mov rax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the integer absolute value into a Mixed cell
    emitter.instruction("jmp __rt_abs_mixed_done_x86");                         // return the boxed integer result

    emitter.label("__rt_abs_mixed_float_x86");
    emitter.instruction("mov r11, 0x7fffffffffffffff");                         // materialize a mask that clears the IEEE-754 sign bit
    emitter.instruction("and rdi, r11");                                        // clear the sign bit so the float payload becomes its absolute value
    emitter.instruction("xor rsi, rsi");                                        // float payloads do not use a high word
    emitter.instruction("mov rax, 2");                                          // runtime tag 2 = float
    emitter.instruction("call __rt_mixed_from_value");                          // box the float absolute value into a Mixed cell

    emitter.label("__rt_abs_mixed_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the saved-pointer slot
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed absolute value
}
