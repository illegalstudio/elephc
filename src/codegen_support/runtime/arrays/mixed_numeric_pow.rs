//! Purpose:
//! Emits `__rt_mixed_numeric_pow`, PHP's `**` over two boxed Mixed operands.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//! - Generated code through `Op::MixedNumericBinop` with the `MixedNumericOp::Pow`
//!   immediate (`crate::codegen::lower_inst::arithmetic`).
//!
//! Key details:
//! - `**` is the only mixed numeric operator that is NOT add/sub/mul-shaped: the integer
//!   path is a square-and-multiply loop with a mid-loop promotion, so it lives here
//!   instead of inside `__rt_mixed_numeric_common`. The integer case simply reuses
//!   `__rt_int_pow_checked`, which already returns a boxed Mixed cell.
//! - The integer path is taken only when BOTH payload tags are exactly `0` (integer).
//!   Every other combination — a double payload, a numeric string, a bool, null — falls
//!   through to `pow((double) l, (double) r)`, which is what this operator did for all
//!   Mixed operands before the integer path existed. Narrowing the fast path this way is
//!   what keeps `"2.5" ** 2` a float while making `$i ** $j` an int.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_mixed_numeric_pow` for both AArch64 and x86_64.
///
/// Input:  AArch64 x0 = left Mixed*, x1 = right Mixed*
///         x86_64  rax = left Mixed*, rdi = right Mixed*
/// Output: boxed Mixed pointer in the integer result register (x0 / rax), holding an
/// integer when both operands were integers and PHP keeps an int, a double otherwise.
pub fn emit_mixed_numeric_pow(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_numeric_pow_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_numeric_pow ---");
    emitter.label_global("__rt_mixed_numeric_pow");

    emitter.instruction("sub sp, sp, #64");                                     // allocate slots for both boxed operands and the saved left value
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed left operand pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the boxed right operand pointer

    // -- integer exponentiation only when both payloads are genuine integers --
    emitter.instruction("bl __rt_mixed_unbox");                                 // inspect the left boxed payload tag
    emitter.instruction("cmp x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("b.ne __rt_mixed_numeric_pow_float");                   // any other payload uses the double path
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the boxed right operand pointer
    emitter.instruction("bl __rt_mixed_unbox");                                 // inspect the right boxed payload tag
    emitter.instruction("cmp x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("b.ne __rt_mixed_numeric_pow_float");                   // any other payload uses the double path

    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed left operand before casting to integer
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce the base to an integer
    emitter.instruction("str x0, [sp, #16]");                                   // save the integer base across the exponent cast
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed right operand before casting to integer
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce the exponent to an integer
    emitter.instruction("mov x1, x0");                                          // place the exponent in the second helper argument
    emitter.instruction("ldr x0, [sp, #16]");                                   // place the base in the first helper argument
    emitter.instruction("bl __rt_int_pow_checked");                             // php-src int ** int, already boxed as a Mixed cell
    emitter.instruction("b __rt_mixed_numeric_pow_done");                       // restore the helper frame and return

    // -- double path: pow((double) left, (double) right) --
    emitter.label("__rt_mixed_numeric_pow_float");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed left operand before casting to double
    emitter.instruction("bl __rt_mixed_cast_float");                            // coerce the base to a double
    emitter.instruction("str d0, [sp, #24]");                                   // save the base across the exponent cast
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed right operand before casting to double
    emitter.instruction("bl __rt_mixed_cast_float");                            // coerce the exponent to a double
    emitter.instruction("fmov d1, d0");                                         // place the exponent in the second libc pow argument
    emitter.instruction("ldr d0, [sp, #24]");                                   // place the base in the first libc pow argument
    emitter.emit_call_c("pow");                                                 // pow(base, exponent) through the target C ABI shim
    emitter.instruction("fmov x1, d0");                                         // move the double bits into the Mixed helper payload register
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the double result into a Mixed cell

    emitter.label("__rt_mixed_numeric_pow_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return to generated code with boxed Mixed result in x0
}

/// Emits the Linux x86_64 variant of `__rt_mixed_numeric_pow`.
///
/// Mirrors the AArch64 helper with the mixed-helper x86_64 convention: `rax` = left
/// Mixed*, `rdi` = right Mixed*, boxed Mixed pointer returned in `rax`.
fn emit_mixed_numeric_pow_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_numeric_pow ---");
    emitter.label_global("__rt_mixed_numeric_pow");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before nested runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // allocate aligned slots for both boxed operands and the saved base
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the boxed left operand pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the boxed right operand pointer

    emitter.instruction("call __rt_mixed_unbox");                               // inspect the left boxed payload tag
    emitter.instruction("cmp rax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("jne __rt_mixed_numeric_pow_float_x");                  // any other payload uses the double path
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the boxed right operand pointer
    emitter.instruction("call __rt_mixed_unbox");                               // inspect the right boxed payload tag
    emitter.instruction("cmp rax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("jne __rt_mixed_numeric_pow_float_x");                  // any other payload uses the double path

    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the boxed left operand before casting to integer
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce the base to an integer
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the integer base across the exponent cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed right operand before casting to integer
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce the exponent to an integer
    emitter.instruction("mov rsi, rax");                                        // place the exponent in the second helper argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // place the base in the first helper argument
    emitter.instruction("call __rt_int_pow_checked");                           // php-src int ** int, already boxed as a Mixed cell
    emitter.instruction("jmp __rt_mixed_numeric_pow_done_x");                   // restore the helper frame and return

    emitter.label("__rt_mixed_numeric_pow_float_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the boxed left operand before casting to double
    emitter.instruction("call __rt_mixed_cast_float");                          // coerce the base to a double
    emitter.instruction("movsd QWORD PTR [rbp - 32], xmm0");                    // save the base across the exponent cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed right operand before casting to double
    emitter.instruction("call __rt_mixed_cast_float");                          // coerce the exponent to a double
    emitter.instruction("movapd xmm1, xmm0");                                   // place the exponent in the second libc pow argument
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 32]");                    // place the base in the first libc pow argument
    emitter.emit_call_c("pow");                                                 // pow(base, exponent) through the target C ABI shim
    emitter.instruction("movq rdi, xmm0");                                      // move the double bits into the Mixed helper payload register
    emitter.instruction("xor rsi, rsi");                                        // double payloads do not use a high word
    emitter.instruction("mov rax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the double result into a Mixed cell

    emitter.label("__rt_mixed_numeric_pow_done_x");
    emitter.instruction("add rsp, 64");                                         // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code with boxed Mixed result in rax
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies both targets emit the integer fast path (delegating to
    /// `__rt_int_pow_checked`) and the `pow()` fallback for every non-integer payload.
    #[test]
    fn test_emit_mixed_numeric_pow_has_int_and_float_paths() {
        for arch in [Arch::AArch64, Arch::X86_64] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_mixed_numeric_pow(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_mixed_numeric_pow:\n"), "missing entry point for {:?}", arch);
            assert!(asm.contains("__rt_int_pow_checked"), "missing integer path for {:?}", arch);
            assert!(asm.contains("__rt_mixed_cast_float"), "missing double path for {:?}", arch);
            assert!(asm.contains("__rt_mixed_from_value"), "missing boxing call for {:?}", arch);
        }
    }
}
