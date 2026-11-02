//! Purpose:
//! Emits `__rt_mixed_numeric_div`, php's `/` over two boxed operands: an integer for an exact
//! quotient of integer operands, a double otherwise.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//! - Generated code through `Op::MixedNumericDiv`
//!   (`crate::codegen::lower_inst::arithmetic::lower_mixed_numeric_div`).
//!
//! Key details:
//! - Operand classification is the one `__rt_mixed_numeric_common` applies to `+`/`-`/`*`: a
//!   double payload or a float-form numeric string ("1.5") divides in floating point, every
//!   other operand (int, bool, null, an integer-form string) is cast to an integer and handed to
//!   `__rt_int_div_checked`, so `3600 / 60 / 60` is `int(1)` like php's, not `float(1)`.
//! - A zero divisor is not thrown here: the helper returns a null pointer and the call site
//!   raises php's `DivisionByZeroError`, exactly as the typed `int / int` path does.
//! - Both operands are borrowed; the result is a fresh boxed Mixed cell.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the boxed division helper for both AArch64 and x86_64.
///
/// Input (AArch64):  x0 = boxed dividend, x1 = boxed divisor
/// Input (x86_64):   rdi = boxed dividend, rsi = boxed divisor
/// Output: a boxed Mixed pointer in x0 / rax, or null when the divisor is zero.
pub fn emit_mixed_numeric_div(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_numeric_div_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_numeric_div ---");
    emitter.label_global("__rt_mixed_numeric_div");

    // -- frame slots: [sp,#0] left box, [sp,#8] right box, [sp,#16] left int, [sp,#24] left double --
    emitter.instruction("sub sp, sp, #48");                                     // allocate the operand slots and the saved FP/LR area
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed dividend
    emitter.instruction("str x1, [sp, #8]");                                    // save the boxed divisor

    // -- a zero divisor (0, 0.0, "0", false, null) is php's DivisionByZeroError --
    emitter.instruction("mov x0, x1");                                          // cast the divisor first: its zero test decides everything
    emitter.instruction("bl __rt_mixed_cast_float");                            // d0 = (double) divisor
    emitter.instruction("fcmp d0, #0.0");                                       // is the divisor zero?
    emitter.instruction("b.eq __rt_mixed_numeric_div_zero");                    // let the call site raise DivisionByZeroError

    // -- a double payload or a float-form string on either side divides in floating point --
    emitter.instruction("ldr x0, [sp, #0]");                                    // inspect the dividend
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0 = runtime tag of the dividend
    emitter.instruction("cmp x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("b.eq __rt_mixed_numeric_div_float");                   // a double dividend makes the quotient a double
    emitter.instruction("cmp x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("b.ne __rt_mixed_numeric_div_right");                   // non-strings take the integer classification
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed string dividend
    emitter.instruction("bl __rt_mixed_str_operand_is_float");                  // is it a float-form numeric string?
    emitter.instruction("cbnz x0, __rt_mixed_numeric_div_float");               // "1.5" divides in floating point
    emitter.label("__rt_mixed_numeric_div_right");
    emitter.instruction("ldr x0, [sp, #8]");                                    // inspect the divisor
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0 = runtime tag of the divisor
    emitter.instruction("cmp x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("b.eq __rt_mixed_numeric_div_float");                   // a double divisor makes the quotient a double
    emitter.instruction("cmp x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("b.ne __rt_mixed_numeric_div_int");                     // non-strings take the integer path
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed string divisor
    emitter.instruction("bl __rt_mixed_str_operand_is_float");                  // is it a float-form numeric string?
    emitter.instruction("cbnz x0, __rt_mixed_numeric_div_float");               // "1.5" divides in floating point

    // -- integer path: exact quotients stay integers --
    emitter.label("__rt_mixed_numeric_div_int");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed dividend
    emitter.instruction("bl __rt_mixed_cast_int");                              // x0 = (int) dividend
    emitter.instruction("str x0, [sp, #16]");                                   // keep it across the divisor cast
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed divisor
    emitter.instruction("bl __rt_mixed_cast_int");                              // x0 = (int) divisor, non-zero by the test above
    emitter.instruction("mov x1, x0");                                          // the divisor is the helper's second argument
    emitter.instruction("ldr x0, [sp, #16]");                                   // the dividend is its first
    emitter.instruction("bl __rt_int_div_checked");                             // php int / int, already boxed
    emitter.instruction("b __rt_mixed_numeric_div_done");                       // restore the helper frame and return

    // -- double path --
    emitter.label("__rt_mixed_numeric_div_float");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed dividend
    emitter.instruction("bl __rt_mixed_cast_float");                            // d0 = (double) dividend
    emitter.instruction("str d0, [sp, #24]");                                   // keep it across the divisor cast
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed divisor
    emitter.instruction("bl __rt_mixed_cast_float");                            // d0 = (double) divisor
    emitter.instruction("fmov d1, d0");                                         // the divisor goes in d1
    emitter.instruction("ldr d0, [sp, #24]");                                   // the dividend goes in d0
    emitter.instruction("fdiv d0, d0, d1");                                     // php's floating-point quotient
    emitter.instruction("fmov x1, d0");                                         // move the double bits into the Mixed helper payload register
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the double quotient into a Mixed cell
    emitter.instruction("b __rt_mixed_numeric_div_done");                       // restore the helper frame and return

    emitter.label("__rt_mixed_numeric_div_zero");
    emitter.instruction("mov x0, #0");                                          // null result: the call site raises DivisionByZeroError

    emitter.label("__rt_mixed_numeric_div_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the boxed quotient (or null) in x0
}

/// Emits the Linux x86_64 variant of `__rt_mixed_numeric_div`: `rdi` = boxed dividend, `rsi` =
/// boxed divisor; the internal Mixed helpers take their box in `rax`.
fn emit_mixed_numeric_div_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_numeric_div ---");
    emitter.label_global("__rt_mixed_numeric_div");

    // -- frame slots: [rbp-8] left box, [rbp-16] right box, [rbp-24] left int, [rbp-32] left double --
    emitter.instruction("push rbp");                                            // save the caller frame pointer before nested runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // allocate aligned operand slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the boxed dividend
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the boxed divisor

    emitter.instruction("mov rax, rsi");                                        // cast the divisor first: its zero test decides everything
    emitter.instruction("call __rt_mixed_cast_float");                          // xmm0 = (double) divisor
    emitter.instruction("xorpd xmm1, xmm1");                                    // xmm1 = 0.0
    emitter.instruction("ucomisd xmm0, xmm1");                                  // is the divisor zero?
    emitter.instruction("jp __rt_mixed_numeric_div_classify_x");                // NaN is not zero
    emitter.instruction("je __rt_mixed_numeric_div_zero_x");                    // let the call site raise DivisionByZeroError

    emitter.label("__rt_mixed_numeric_div_classify_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // inspect the dividend
    emitter.instruction("call __rt_mixed_unbox");                               // rax = runtime tag of the dividend
    emitter.instruction("cmp rax, 2");                                          // runtime tag 2 = double
    emitter.instruction("je __rt_mixed_numeric_div_float_x");                   // a double dividend makes the quotient a double
    emitter.instruction("cmp rax, 1");                                          // runtime tag 1 = string
    emitter.instruction("jne __rt_mixed_numeric_div_right_x");                  // non-strings take the integer classification
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the boxed string dividend
    emitter.instruction("call __rt_mixed_str_operand_is_float");                // is it a float-form numeric string?
    emitter.instruction("test rax, rax");                                       // test the classification result
    emitter.instruction("jnz __rt_mixed_numeric_div_float_x");                  // "1.5" divides in floating point
    emitter.label("__rt_mixed_numeric_div_right_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // inspect the divisor
    emitter.instruction("call __rt_mixed_unbox");                               // rax = runtime tag of the divisor
    emitter.instruction("cmp rax, 2");                                          // runtime tag 2 = double
    emitter.instruction("je __rt_mixed_numeric_div_float_x");                   // a double divisor makes the quotient a double
    emitter.instruction("cmp rax, 1");                                          // runtime tag 1 = string
    emitter.instruction("jne __rt_mixed_numeric_div_int_x");                    // non-strings take the integer path
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed string divisor
    emitter.instruction("call __rt_mixed_str_operand_is_float");                // is it a float-form numeric string?
    emitter.instruction("test rax, rax");                                       // test the classification result
    emitter.instruction("jnz __rt_mixed_numeric_div_float_x");                  // "1.5" divides in floating point

    emitter.label("__rt_mixed_numeric_div_int_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the boxed dividend
    emitter.instruction("call __rt_mixed_cast_int");                            // rax = (int) dividend
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // keep it across the divisor cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed divisor
    emitter.instruction("call __rt_mixed_cast_int");                            // rax = (int) divisor, non-zero by the test above
    emitter.instruction("mov rsi, rax");                                        // the divisor is the helper's second argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // the dividend is its first
    emitter.instruction("call __rt_int_div_checked");                           // php int / int, already boxed
    emitter.instruction("jmp __rt_mixed_numeric_div_done_x");                   // restore the helper frame and return

    emitter.label("__rt_mixed_numeric_div_float_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the boxed dividend
    emitter.instruction("call __rt_mixed_cast_float");                          // xmm0 = (double) dividend
    emitter.instruction("movsd QWORD PTR [rbp - 32], xmm0");                    // keep it across the divisor cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed divisor
    emitter.instruction("call __rt_mixed_cast_float");                          // xmm0 = (double) divisor
    emitter.instruction("movsd xmm1, xmm0");                                    // the divisor goes in xmm1
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 32]");                    // the dividend goes in xmm0
    emitter.instruction("divsd xmm0, xmm1");                                    // php's floating-point quotient
    emitter.instruction("movq rdi, xmm0");                                      // move the double bits into the Mixed helper payload register
    emitter.instruction("xor rsi, rsi");                                        // double payloads do not use a high word
    emitter.instruction("mov rax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the double quotient into a Mixed cell
    emitter.instruction("jmp __rt_mixed_numeric_div_done_x");                   // restore the helper frame and return

    emitter.label("__rt_mixed_numeric_div_zero_x");
    emitter.instruction("xor eax, eax");                                        // null result: the call site raises DivisionByZeroError

    emitter.label("__rt_mixed_numeric_div_done_x");
    emitter.instruction("add rsp, 32");                                         // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed quotient (or null) in rax
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies both targets classify the operands, route integers through
    /// `__rt_int_div_checked` and signal a zero divisor with a null result.
    #[test]
    fn test_emit_mixed_numeric_div_covers_php_semantics() {
        for arch in [Arch::AArch64, Arch::X86_64] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_mixed_numeric_div(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_mixed_numeric_div:\n"), "missing entry point for {:?}", arch);
            for fragment in [
                "__rt_mixed_str_operand_is_float",
                "__rt_int_div_checked",
                "__rt_mixed_numeric_div_zero",
                "__rt_mixed_from_value",
            ] {
                assert!(asm.contains(fragment), "missing {} for {:?}", fragment, arch);
            }
        }
    }
}
