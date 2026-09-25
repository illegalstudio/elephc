//! Purpose:
//! Emits `__rt_int_pow_checked`, the runtime implementation of PHP's `int ** int`
//! (`zend_pow_function_base`): an integer result whenever the exponent is non-negative
//! and the value fits `i64`, a double otherwise.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//! - Generated code through `Op::ICheckedPow`
//!   (`crate::codegen::lower_inst::arithmetic::lower_int_checked_binop`).
//!
//! Key details:
//! - PHP does NOT compute `int ** int` as `pow((double) base, (double) exp)`. It runs a
//!   square-and-multiply loop over `i64` and only bails out to `double` at the exact
//!   multiplication that overflows, combining the exact accumulator with `pow()` of the
//!   remaining factor. The two differ in the last ULP for most overflowing inputs, so the
//!   loop is reproduced verbatim — it is the same algorithm the compile-time folder in
//!   `crate::optimize::fold::ops::try_fold_int_pow` implements, and the two must agree.
//! - A negative exponent is plain `pow((double) base, (double) exp)`, always a double.
//! - `exp == 0` is `int(1)` (even for base `0`), and `base == 0` with a positive exponent
//!   is `int(0)`; both are answered before the loop, exactly like php-src.
//! - Input/output follow the checked-binop helper contract shared with
//!   `__rt_int_{add,sub,mul}_checked`: two raw I64 operands in, one boxed Mixed cell out.
//! - The entry point is fully self-contained (no cross-function local-label branches) to
//!   survive macOS `.subsections_via_symbols` dead-stripping.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the checked integer exponentiation helper for both AArch64 and x86_64.
///
/// Input (AArch64):  x0 = base I64, x1 = exponent I64
/// Input (x86_64):   rdi = base I64, rsi = exponent I64
/// Output: boxed Mixed pointer in the integer result register (x0 / rax), tagged `0`
/// (integer) when PHP keeps an int and `2` (double) when PHP promotes.
pub fn emit_int_pow_checked(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_int_pow_checked_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: int_pow_checked ---");
    emitter.label_global("__rt_int_pow_checked");

    // -- frame slots: l1=[sp,#0] accumulator, l2=[sp,#8] factor, i=[sp,#16] exponent, dval=[sp,#24] --
    emitter.instruction("sub sp, sp, #64");                                     // allocate the loop state and saved FP/LR area
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame pointer

    emitter.instruction("cmp x1, #0");                                          // is the exponent negative?
    emitter.instruction("b.lt __rt_int_pow_checked_neg");                       // a negative exponent is always a double result
    emitter.instruction("cbz x1, __rt_int_pow_checked_one");                    // anything to the power of zero is int(1)
    emitter.instruction("cbz x0, __rt_int_pow_checked_zero");                   // zero to a positive power is int(0)

    emitter.instruction("mov x2, #1");                                          // seed the exact accumulator
    emitter.instruction("str x2, [sp, #0]");                                    // l1 = 1
    emitter.instruction("str x0, [sp, #8]");                                    // l2 = base
    emitter.instruction("str x1, [sp, #16]");                                   // i = exponent

    emitter.label("__rt_int_pow_checked_loop");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the remaining exponent
    emitter.instruction("cmp x3, #1");                                          // php-src loops while the exponent is at least 1
    emitter.instruction("b.lt __rt_int_pow_checked_int");                       // exhausted: return the exact accumulator
    emitter.instruction("tst x3, #1");                                          // is the remaining exponent odd?
    emitter.instruction("b.eq __rt_int_pow_checked_even");                      // even exponents square the factor

    // -- odd step: i -= 1; l1 *= l2 with signed-overflow detection --
    emitter.instruction("sub x3, x3, #1");                                      // consume one factor from the exponent
    emitter.instruction("str x3, [sp, #16]");                                   // publish the decremented exponent
    emitter.instruction("ldr x4, [sp, #0]");                                    // reload the accumulator
    emitter.instruction("ldr x5, [sp, #8]");                                    // reload the current factor
    emitter.instruction("mul x6, x4, x5");                                      // low half of the signed product
    emitter.instruction("smulh x7, x4, x5");                                    // high half needed for overflow detection
    emitter.instruction("cmp x7, x6, asr #63");                                 // high half must equal the sign extension of the low half
    emitter.instruction("b.ne __rt_int_pow_checked_odd_of");                    // overflow: promote exactly the way php-src does
    emitter.instruction("str x6, [sp, #0]");                                    // l1 = l1 * l2
    emitter.instruction("b __rt_int_pow_checked_next");                         // continue the square-and-multiply loop

    // -- even step: i /= 2; l2 *= l2 with signed-overflow detection --
    emitter.label("__rt_int_pow_checked_even");
    emitter.instruction("lsr x3, x3, #1");                                      // halve the remaining exponent (it is non-negative)
    emitter.instruction("str x3, [sp, #16]");                                   // publish the halved exponent
    emitter.instruction("ldr x5, [sp, #8]");                                    // reload the current factor
    emitter.instruction("mul x6, x5, x5");                                      // low half of the squared factor
    emitter.instruction("smulh x7, x5, x5");                                    // high half needed for overflow detection
    emitter.instruction("cmp x7, x6, asr #63");                                 // high half must equal the sign extension of the low half
    emitter.instruction("b.ne __rt_int_pow_checked_even_of");                   // overflow: promote exactly the way php-src does
    emitter.instruction("str x6, [sp, #8]");                                    // l2 = l2 * l2
    emitter.instruction("b __rt_int_pow_checked_next");                         // continue the square-and-multiply loop

    emitter.label("__rt_int_pow_checked_next");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the remaining exponent
    emitter.instruction("cbz x3, __rt_int_pow_checked_int");                    // exponent consumed: the accumulator is exact
    emitter.instruction("b __rt_int_pow_checked_loop");                         // otherwise keep going

    // -- odd overflow: result = ((double) l1 * (double) l2) * pow((double) l2, (double) i) --
    emitter.label("__rt_int_pow_checked_odd_of");
    emitter.instruction("scvtf d0, x4");                                        // (double) accumulator before the overflowing multiply
    emitter.instruction("scvtf d1, x5");                                        // (double) factor
    emitter.instruction("fmul d0, d0, d1");                                     // php-src's ZEND_SIGNED_MULTIPLY_LONG dval
    emitter.instruction("str d0, [sp, #24]");                                   // save dval across the libc pow call
    emitter.instruction("scvtf d0, x5");                                        // pow base = (double) factor
    emitter.instruction("ldr x3, [sp, #16]");                                   // remaining exponent after the decrement
    emitter.instruction("scvtf d1, x3");                                        // pow exponent = (double) remaining
    emitter.emit_call_c("pow");                                                 // pow(l2, i) through the target C ABI shim
    emitter.instruction("ldr d1, [sp, #24]");                                   // reload dval
    emitter.instruction("fmul d0, d0, d1");                                     // dval * pow(l2, i)
    emitter.instruction("b __rt_int_pow_checked_box_double");                   // box the promoted double

    // -- even overflow: result = (double) l1 * pow((double) l2 * (double) l2, (double) i) --
    emitter.label("__rt_int_pow_checked_even_of");
    emitter.instruction("scvtf d0, x5");                                        // (double) factor before the overflowing square
    emitter.instruction("fmul d0, d0, d0");                                     // php-src's ZEND_SIGNED_MULTIPLY_LONG dval
    emitter.instruction("ldr x3, [sp, #16]");                                   // remaining exponent after the halving
    emitter.instruction("scvtf d1, x3");                                        // pow exponent = (double) remaining
    emitter.emit_call_c("pow");                                                 // pow(dval, i) through the target C ABI shim
    emitter.instruction("ldr x4, [sp, #0]");                                    // reload the exact accumulator
    emitter.instruction("scvtf d1, x4");                                        // (double) accumulator
    emitter.instruction("fmul d0, d0, d1");                                     // l1 * pow(dval, i)
    emitter.instruction("b __rt_int_pow_checked_box_double");                   // box the promoted double

    // -- negative exponent: pow((double) base, (double) exp) --
    emitter.label("__rt_int_pow_checked_neg");
    emitter.instruction("scvtf d0, x0");                                        // (double) base
    emitter.instruction("scvtf d1, x1");                                        // (double) exponent
    emitter.emit_call_c("pow");                                                 // pow(base, exp) through the target C ABI shim
    emitter.instruction("b __rt_int_pow_checked_box_double");                   // box the double result

    emitter.label("__rt_int_pow_checked_one");
    emitter.instruction("mov x1, #1");                                          // PHP answers int(1) for a zero exponent
    emitter.instruction("b __rt_int_pow_checked_box_int");                      // box the integer result

    emitter.label("__rt_int_pow_checked_zero");
    emitter.instruction("mov x1, #0");                                          // PHP answers int(0) for zero to a positive power
    emitter.instruction("b __rt_int_pow_checked_box_int");                      // box the integer result

    emitter.label("__rt_int_pow_checked_int");
    emitter.instruction("ldr x1, [sp, #0]");                                    // the exact accumulator is the integer result

    emitter.label("__rt_int_pow_checked_box_int");
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("bl __rt_mixed_from_value");                            // box the integer result into a Mixed cell
    emitter.instruction("b __rt_int_pow_checked_done");                         // restore the helper frame and return

    emitter.label("__rt_int_pow_checked_box_double");
    emitter.instruction("fmov x1, d0");                                         // move the double bits into the Mixed helper payload register
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the promoted double into a Mixed cell

    emitter.label("__rt_int_pow_checked_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return to generated code with the boxed Mixed result in x0
}

/// Emits the Linux x86_64 variant of `__rt_int_pow_checked`.
///
/// Mirrors the AArch64 square-and-multiply loop with SysV registers: `rdi` = base,
/// `rsi` = exponent, boxed Mixed pointer returned in `rax`. Overflow is detected with the
/// one-operand `imul` overflow flag, which is the x86 equivalent of the AArch64
/// `smulh`/`asr #63` comparison.
fn emit_int_pow_checked_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: int_pow_checked ---");
    emitter.label_global("__rt_int_pow_checked");

    // -- frame slots: l1=[rbp-8] accumulator, l2=[rbp-16] factor, i=[rbp-24] exponent, dval=[rbp-32] --
    emitter.instruction("push rbp");                                            // save the caller frame pointer before nested runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // allocate aligned slots for the loop state

    emitter.instruction("test rsi, rsi");                                       // inspect the exponent's sign
    emitter.instruction("js __rt_int_pow_checked_neg_x");                       // a negative exponent is always a double result
    emitter.instruction("jz __rt_int_pow_checked_one_x");                       // anything to the power of zero is int(1)
    emitter.instruction("test rdi, rdi");                                       // is the base zero?
    emitter.instruction("jz __rt_int_pow_checked_zero_x");                      // zero to a positive power is int(0)

    emitter.instruction("mov QWORD PTR [rbp - 8], 1");                          // l1 = 1
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // l2 = base
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // i = exponent

    emitter.label("__rt_int_pow_checked_loop_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the remaining exponent
    emitter.instruction("cmp rcx, 1");                                          // php-src loops while the exponent is at least 1
    emitter.instruction("jl __rt_int_pow_checked_int_x");                       // exhausted: return the exact accumulator
    emitter.instruction("test rcx, 1");                                         // is the remaining exponent odd?
    emitter.instruction("jz __rt_int_pow_checked_even_x");                      // even exponents square the factor

    emitter.instruction("dec rcx");                                             // consume one factor from the exponent
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // publish the decremented exponent
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the accumulator into the multiply operand
    emitter.instruction("imul QWORD PTR [rbp - 16]");                           // rdx:rax = l1 * l2 with the overflow flag set
    emitter.instruction("jo __rt_int_pow_checked_odd_of_x");                    // overflow: promote exactly the way php-src does
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // l1 = l1 * l2
    emitter.instruction("jmp __rt_int_pow_checked_next_x");                     // continue the square-and-multiply loop

    emitter.label("__rt_int_pow_checked_even_x");
    emitter.instruction("shr rcx, 1");                                          // halve the remaining exponent (it is non-negative)
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // publish the halved exponent
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the factor into the multiply operand
    emitter.instruction("imul QWORD PTR [rbp - 16]");                           // rdx:rax = l2 * l2 with the overflow flag set
    emitter.instruction("jo __rt_int_pow_checked_even_of_x");                   // overflow: promote exactly the way php-src does
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // l2 = l2 * l2
    emitter.instruction("jmp __rt_int_pow_checked_next_x");                     // continue the square-and-multiply loop

    emitter.label("__rt_int_pow_checked_next_x");
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // has the exponent been fully consumed?
    emitter.instruction("je __rt_int_pow_checked_int_x");                       // the accumulator is exact
    emitter.instruction("jmp __rt_int_pow_checked_loop_x");                     // otherwise keep going

    emitter.label("__rt_int_pow_checked_odd_of_x");
    emitter.instruction("cvtsi2sd xmm0, QWORD PTR [rbp - 8]");                  // (double) accumulator before the overflowing multiply
    emitter.instruction("cvtsi2sd xmm1, QWORD PTR [rbp - 16]");                 // (double) factor
    emitter.instruction("mulsd xmm0, xmm1");                                    // php-src's ZEND_SIGNED_MULTIPLY_LONG dval
    emitter.instruction("movsd QWORD PTR [rbp - 32], xmm0");                    // save dval across the libc pow call
    emitter.instruction("cvtsi2sd xmm0, QWORD PTR [rbp - 16]");                 // pow base = (double) factor
    emitter.instruction("cvtsi2sd xmm1, QWORD PTR [rbp - 24]");                 // pow exponent = (double) remaining
    emitter.emit_call_c("pow");                                                 // pow(l2, i) through the target C ABI shim
    emitter.instruction("mulsd xmm0, QWORD PTR [rbp - 32]");                    // dval * pow(l2, i)
    emitter.instruction("jmp __rt_int_pow_checked_box_double_x");               // box the promoted double

    emitter.label("__rt_int_pow_checked_even_of_x");
    emitter.instruction("cvtsi2sd xmm0, QWORD PTR [rbp - 16]");                 // (double) factor before the overflowing square
    emitter.instruction("mulsd xmm0, xmm0");                                    // php-src's ZEND_SIGNED_MULTIPLY_LONG dval
    emitter.instruction("cvtsi2sd xmm1, QWORD PTR [rbp - 24]");                 // pow exponent = (double) remaining
    emitter.emit_call_c("pow");                                                 // pow(dval, i) through the target C ABI shim
    emitter.instruction("cvtsi2sd xmm1, QWORD PTR [rbp - 8]");                  // (double) exact accumulator
    emitter.instruction("mulsd xmm0, xmm1");                                    // l1 * pow(dval, i)
    emitter.instruction("jmp __rt_int_pow_checked_box_double_x");               // box the promoted double

    emitter.label("__rt_int_pow_checked_neg_x");
    emitter.instruction("cvtsi2sd xmm0, rdi");                                  // (double) base
    emitter.instruction("cvtsi2sd xmm1, rsi");                                  // (double) exponent
    emitter.emit_call_c("pow");                                                 // pow(base, exp) through the target C ABI shim
    emitter.instruction("jmp __rt_int_pow_checked_box_double_x");               // box the double result

    emitter.label("__rt_int_pow_checked_one_x");
    emitter.instruction("mov rdi, 1");                                          // PHP answers int(1) for a zero exponent
    emitter.instruction("jmp __rt_int_pow_checked_box_int_x");                  // box the integer result

    emitter.label("__rt_int_pow_checked_zero_x");
    emitter.instruction("xor edi, edi");                                        // PHP answers int(0) for zero to a positive power
    emitter.instruction("jmp __rt_int_pow_checked_box_int_x");                  // box the integer result

    emitter.label("__rt_int_pow_checked_int_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the exact accumulator is the integer result

    emitter.label("__rt_int_pow_checked_box_int_x");
    emitter.instruction("xor rsi, rsi");                                        // integer payloads do not use a high word
    emitter.instruction("mov rax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the integer result into a Mixed cell
    emitter.instruction("jmp __rt_int_pow_checked_done_x");                     // restore the helper frame and return

    emitter.label("__rt_int_pow_checked_box_double_x");
    emitter.instruction("movq rdi, xmm0");                                      // move the double bits into the Mixed helper payload register
    emitter.instruction("xor rsi, rsi");                                        // double payloads do not use a high word
    emitter.instruction("mov rax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the promoted double into a Mixed cell

    emitter.label("__rt_int_pow_checked_done_x");
    emitter.instruction("add rsp, 64");                                         // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code with the boxed Mixed result in rax
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies both targets emit the php-src square-and-multiply structure: the odd and
    /// even overflow bail-outs, the negative-exponent `pow` path, and both boxing tags.
    #[test]
    fn test_emit_int_pow_checked_covers_php_algorithm() {
        for arch in [Arch::AArch64, Arch::X86_64] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_int_pow_checked(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_int_pow_checked:\n"), "missing entry point for {:?}", arch);
            for fragment in [
                "__rt_int_pow_checked_odd_of",
                "__rt_int_pow_checked_even_of",
                "__rt_int_pow_checked_neg",
                "__rt_int_pow_checked_box_int",
                "__rt_int_pow_checked_box_double",
            ] {
                assert!(asm.contains(fragment), "missing {} for {:?}", fragment, arch);
            }
            assert!(asm.contains("__rt_mixed_from_value"), "missing boxing call for {:?}", arch);
        }
    }
}
