//! Purpose:
//! Emits `__rt_int_div_checked`, the runtime implementation of PHP's `int / int`
//! (`div_function_base`): an integer when the division is exact, a double otherwise.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//! - Generated code through `Op::ICheckedDiv`
//!   (`crate::codegen::lower_inst::arithmetic::lower_int_checked_div`).
//!
//! Key details:
//! - php-src answers `ZEND_LONG_MIN / -1` as a double (the quotient does not fit), an exact
//!   quotient as `int` (`3600 / 60` is `int(60)`), and every other quotient as
//!   `(double) a / (double) b`. elephc divided in floating point always, so `var_dump()`,
//!   `is_int()`, `json_encode()` and strict-typed parameters all saw a float php never makes.
//! - A zero divisor never reaches the helper: the call site raises php's `DivisionByZeroError`
//!   first, exactly as the float division did.
//! - Input/output follow the checked-binop helper contract shared with
//!   `__rt_int_{add,sub,mul,pow}_checked`: two raw I64 operands in, one boxed Mixed cell out.
//! - The entry point is fully self-contained (no cross-function local-label branches) to
//!   survive macOS `.subsections_via_symbols` dead-stripping.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the checked integer division helper for both AArch64 and x86_64.
///
/// Input (AArch64):  x0 = dividend I64, x1 = non-zero divisor I64
/// Input (x86_64):   rdi = dividend I64, rsi = non-zero divisor I64
/// Output: boxed Mixed pointer in the integer result register (x0 / rax), tagged `0`
/// (integer) for an exact quotient and `2` (double) otherwise.
pub fn emit_int_div_checked(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_int_div_checked_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: int_div_checked ---");
    emitter.label_global("__rt_int_div_checked");

    emitter.instruction("sub sp, sp, #16");                                     // reserve the saved FP/LR area
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across the boxing call
    emitter.instruction("mov x29, sp");                                         // establish a stable helper frame pointer

    emitter.instruction("cmn x1, #1");                                          // is the divisor -1?
    emitter.instruction("b.ne __rt_int_div_checked_exact");                     // only MIN / -1 cannot be represented as an integer
    emitter.instruction("movz x9, #0x8000, lsl #48");                           // x9 = ZEND_LONG_MIN
    emitter.instruction("cmp x0, x9");                                          // is the dividend ZEND_LONG_MIN?
    emitter.instruction("b.eq __rt_int_div_checked_float");                     // MIN / -1 overflows: php answers a double

    emitter.label("__rt_int_div_checked_exact");
    emitter.instruction("sdiv x9, x0, x1");                                     // x9 = truncated integer quotient
    emitter.instruction("msub x10, x9, x1, x0");                                // x10 = dividend - quotient * divisor (the remainder)
    emitter.instruction("cbnz x10, __rt_int_div_checked_float");                // an inexact quotient is a double in php
    emitter.instruction("mov x1, x9");                                          // the exact quotient is the integer payload
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("bl __rt_mixed_from_value");                            // box the integer quotient into a Mixed cell
    emitter.instruction("b __rt_int_div_checked_done");                         // restore the helper frame and return

    emitter.label("__rt_int_div_checked_float");
    emitter.instruction("scvtf d0, x0");                                        // (double) dividend
    emitter.instruction("scvtf d1, x1");                                        // (double) divisor
    emitter.instruction("fdiv d0, d0, d1");                                     // php's floating-point quotient
    emitter.instruction("fmov x1, d0");                                         // move the double bits into the Mixed helper payload register
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the double quotient into a Mixed cell

    emitter.label("__rt_int_div_checked_done");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return to generated code with the boxed Mixed result in x0
}

/// Emits the Linux x86_64 variant of `__rt_int_div_checked`: `rdi` = dividend, `rsi` = divisor,
/// boxed Mixed pointer returned in `rax`.
fn emit_int_div_checked_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: int_div_checked ---");
    emitter.label_global("__rt_int_div_checked");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before the boxing call
    emitter.instruction("mov rbp, rsp");                                        // establish a stable, 16-byte aligned helper frame

    emitter.instruction("cmp rsi, -1");                                         // is the divisor -1?
    emitter.instruction("jne __rt_int_div_checked_exact_x");                    // only MIN / -1 cannot be represented as an integer
    emitter.instruction("movabs rax, -9223372036854775808");                    // rax = ZEND_LONG_MIN
    emitter.instruction("cmp rdi, rax");                                        // is the dividend ZEND_LONG_MIN?
    emitter.instruction("je __rt_int_div_checked_float_x");                     // MIN / -1 overflows: php answers a double

    emitter.label("__rt_int_div_checked_exact_x");
    emitter.instruction("mov rax, rdi");                                        // the dividend goes in rax for idiv
    emitter.instruction("cqo");                                                 // sign-extend the dividend into rdx:rax
    emitter.instruction("idiv rsi");                                            // rax = quotient, rdx = remainder
    emitter.instruction("test rdx, rdx");                                       // is the quotient exact?
    emitter.instruction("jnz __rt_int_div_checked_float_x");                    // an inexact quotient is a double in php
    emitter.instruction("mov rdi, rax");                                        // the exact quotient is the integer payload
    emitter.instruction("xor rsi, rsi");                                        // integer payloads do not use a high word
    emitter.instruction("mov rax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the integer quotient into a Mixed cell
    emitter.instruction("jmp __rt_int_div_checked_done_x");                     // restore the helper frame and return

    emitter.label("__rt_int_div_checked_float_x");
    emitter.instruction("cvtsi2sd xmm0, rdi");                                  // (double) dividend
    emitter.instruction("cvtsi2sd xmm1, rsi");                                  // (double) divisor
    emitter.instruction("divsd xmm0, xmm1");                                    // php's floating-point quotient
    emitter.instruction("movq rdi, xmm0");                                      // move the double bits into the Mixed helper payload register
    emitter.instruction("xor rsi, rsi");                                        // double payloads do not use a high word
    emitter.instruction("mov rax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the double quotient into a Mixed cell

    emitter.label("__rt_int_div_checked_done_x");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code with the boxed Mixed result in rax
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies both targets emit the MIN / -1 guard, the exact-remainder test and both boxing
    /// tags of php's `div_function_base`.
    #[test]
    fn test_emit_int_div_checked_covers_php_semantics() {
        for arch in [Arch::AArch64, Arch::X86_64] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_int_div_checked(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_int_div_checked:\n"), "missing entry point for {:?}", arch);
            for fragment in ["__rt_int_div_checked_exact", "__rt_int_div_checked_float", "__rt_mixed_from_value"] {
                assert!(asm.contains(fragment), "missing {} for {:?}", fragment, arch);
            }
        }
    }
}
