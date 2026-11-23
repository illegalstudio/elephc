//! Purpose:
//! Emits the `__rt_base_to_number` runtime helper assembly shared by PHP's `hexdec`,
//! `bindec`, and `octdec` builtins: parses digits of one base into an `int`, widening to a
//! `float` exactly where reference PHP does.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Mirrors php-src's `_php_math_basetozval`: characters that are not digits of the requested
//!   base are IGNORED rather than terminating the scan (`hexdec("a0z") === 160`), and the
//!   integer accumulator switches to `double` as soon as the next digit would push it past
//!   `PHP_INT_MAX`. That threshold is `ZEND_LONG_MAX`, not `ZEND_ULONG_MAX`, which is why
//!   `hexdec("ffffffffffffffff")` is a float in reference PHP.
//! - The `0x`/`0b`/`0o` prefixes php-src strips before scanning are deliberately NOT special
//!   cased: for these three builtins the stripped characters are always either a leading `0`
//!   or a letter that is not a digit of that base, so ignoring them yields the identical
//!   value. php-src's `E_DEPRECATED` for ignored characters has no counterpart in elephc,
//!   which emits no deprecation diagnostics at all.
//! - The helper allocates nothing and calls nothing, so it needs no frame.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// The `.data` label carrying PHP 8.6's imprecise base-conversion notice.
///
/// The message begins with `Notice: ` so `__rt_diag_warning` classifies it as `E_NOTICE`
/// after stripping the category prefix, exactly like php-src's `zend_error(E_NOTICE, ...)`.
const IMPRECISE_CONVERSION_MESSAGE: &str = "_diag_base_conversion_imprecise";

/// Emits `__rt_base_conversion_imprecise_notice`, the tail-call wrapper that reports PHP
/// 8.6's precision-loss notice.
///
/// The callers (`bindec`/`hexdec`/`octdec` lowering and `__rt_base_convert`) invoke it only
/// when the parse widened to `double`. It is a tail jump: `__rt_diag_warning` preserves every
/// live register and the float result, so the caller keeps its parse result untouched.
pub fn emit_base_conversion_imprecise_notice(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: base conversion imprecise notice ---");
    emitter.label_global("__rt_base_conversion_imprecise_notice");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", IMPRECISE_CONVERSION_MESSAGE);
            emitter.instruction("mov x2, #87");                                 // pass the complete imprecision notice length
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rdi", IMPRECISE_CONVERSION_MESSAGE);
            emitter.instruction("mov esi, 87");                                 // pass the complete imprecision notice length
        }
    }
    abi::emit_jump(emitter, "__rt_diag_warning");
}

/// Emits the `__rt_base_to_number` runtime helper.
///
/// ABI (AArch64):
///   Input:  `x1` = string pointer, `x2` = string length, `x3` = base (2..36).
///   Output: `x0` = 0 when the result is an integer and 1 when it widened to a float,
///           `x1` = the integer result, `d0` = the float result.
///
/// ABI (x86_64 System V):
///   Input:  `rdi` = string pointer, `rsi` = string length, `rdx` = base (2..36).
///   Output: `rax` = 0 when the result is an integer and 1 when it widened to a float,
///           `rdx` = the integer result, `xmm0` = the float result.
///
/// An empty string, or one made entirely of characters that are not digits of the base,
/// yields the integer `0` — the same as reference PHP.
pub fn emit_base_to_number(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_base_to_number_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: base_to_number ---");
    emitter.label_global("__rt_base_to_number");

    // -- derive the php-src overflow thresholds from PHP_INT_MAX and the requested base --
    abi::emit_load_int_immediate(emitter, "x11", i64::MAX);
    emitter.instruction("udiv x4, x11, x3");                                    // x4 = PHP_INT_MAX / base, the last accumulator that can still take a digit
    emitter.instruction("msub x5, x4, x3, x11");                                // x5 = PHP_INT_MAX % base, the largest digit that threshold still accepts
    emitter.instruction("mov x6, #0");                                          // start the integer accumulator at zero
    emitter.instruction("mov x7, #0");                                          // start in integer mode; 1 means the result widened to a float
    emitter.instruction("fmov d0, xzr");                                        // start the float accumulator at 0.0
    emitter.instruction("ucvtf d2, x3");                                        // keep the base available as a double for the float accumulation

    emitter.label("__rt_base_to_number_loop");
    emitter.instruction("cbz x2, __rt_base_to_number_done");                    // stop once every input byte has been inspected
    emitter.instruction("ldrb w9, [x1], #1");                                   // load the next input byte and advance the cursor
    emitter.instruction("sub x2, x2, #1");                                      // record that one input byte has been consumed

    // -- decode one digit, ignoring every character that is not one --
    emitter.instruction("sub w10, w9, #48");                                    // try the ASCII numerals first
    emitter.instruction("cmp w10, #9");                                         // is the byte in '0'..'9' (unsigned, so lower bytes wrap high)?
    emitter.instruction("b.ls __rt_base_to_number_digit");                      // an ASCII numeral decodes directly
    emitter.instruction("cmp w9, #65");                                         // is the byte below 'A'?
    emitter.instruction("b.lo __rt_base_to_number_loop");                       // punctuation between the numerals and 'A' is ignored
    emitter.instruction("cmp w9, #90");                                         // is the byte at or below 'Z'?
    emitter.instruction("b.hi __rt_base_to_number_lower");                      // try the lowercase letters instead
    emitter.instruction("sub w10, w9, #55");                                    // map 'A'..'Z' to digit values 10..35
    emitter.instruction("b __rt_base_to_number_digit");                         // the uppercase letter decoded to a digit
    emitter.label("__rt_base_to_number_lower");
    emitter.instruction("cmp w9, #97");                                         // is the byte below 'a'?
    emitter.instruction("b.lo __rt_base_to_number_loop");                       // punctuation between 'Z' and 'a' is ignored
    emitter.instruction("cmp w9, #122");                                        // is the byte above 'z'?
    emitter.instruction("b.hi __rt_base_to_number_loop");                       // bytes past 'z' are ignored
    emitter.instruction("sub w10, w9, #87");                                    // map 'a'..'z' to digit values 10..35

    emitter.label("__rt_base_to_number_digit");
    emitter.instruction("cmp x10, x3");                                         // is the decoded digit valid in the requested base?
    emitter.instruction("b.hs __rt_base_to_number_loop");                       // digits outside the base are ignored, exactly like php-src
    emitter.instruction("cbnz x7, __rt_base_to_number_float");                  // once widened, every further digit accumulates as a float
    emitter.instruction("cmp x6, x4");                                          // would this digit push the accumulator past PHP_INT_MAX?
    emitter.instruction("b.lo __rt_base_to_number_int");                        // below the threshold the integer accumulator always fits
    emitter.instruction("b.hi __rt_base_to_number_widen");                      // above the threshold the accumulator must widen
    emitter.instruction("cmp x10, x5");                                         // at the threshold only digits up to the remainder still fit
    emitter.instruction("b.hi __rt_base_to_number_widen");                      // a larger digit forces the widening to float

    emitter.label("__rt_base_to_number_int");
    emitter.instruction("madd x6, x6, x3, x10");                                // accumulate the digit into the integer result
    emitter.instruction("b __rt_base_to_number_loop");                          // continue scanning the remaining input bytes

    emitter.label("__rt_base_to_number_widen");
    emitter.instruction("ucvtf d0, x6");                                        // seed the float accumulator from the integer digits parsed so far
    emitter.instruction("mov x7, #1");                                          // remember that the result is now a float

    emitter.label("__rt_base_to_number_float");
    emitter.instruction("fmul d0, d0, d2");                                     // shift the float accumulator up by one digit position
    emitter.instruction("ucvtf d3, x10");                                       // convert the current digit for the float accumulation
    emitter.instruction("fadd d0, d0, d3");                                     // accumulate the digit into the float result
    emitter.instruction("b __rt_base_to_number_loop");                          // continue scanning the remaining input bytes

    emitter.label("__rt_base_to_number_done");
    emitter.instruction("cbnz x7, __rt_base_to_number_float_result");           // report a widened result through the float register
    emitter.instruction("mov x0, #0");                                          // report that the result is a PHP integer
    emitter.instruction("mov x1, x6");                                          // return the accumulated integer value
    emitter.instruction("ret");                                                 // hand the integer result back to the caller

    emitter.label("__rt_base_to_number_float_result");
    emitter.instruction("mov x0, #1");                                          // report that the result widened to a PHP float
    emitter.instruction("ret");                                                 // the float result is already in d0
}

/// Emits `__rt_base_to_number` for x86_64 Linux using the System V ABI.
///
/// The base is moved into `rcx` up front so `rdx` is free for the unsigned division that
/// derives the overflow thresholds and, afterwards, for the decoded digit.
fn emit_base_to_number_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: base_to_number ---");
    emitter.label_global("__rt_base_to_number");

    // -- derive the php-src overflow thresholds from PHP_INT_MAX and the requested base --
    emitter.instruction("mov rcx, rdx");                                        // keep the requested base in a stable register
    abi::emit_load_int_immediate(emitter, "rax", i64::MAX);
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before the unsigned division
    emitter.instruction("div rcx");                                             // rax = PHP_INT_MAX / base, rdx = PHP_INT_MAX % base
    emitter.instruction("mov r8, rax");                                         // r8 = the last accumulator that can still take a digit
    emitter.instruction("mov r9, rdx");                                         // r9 = the largest digit that threshold still accepts
    emitter.instruction("xor r10d, r10d");                                      // start the integer accumulator at zero
    emitter.instruction("xor r11d, r11d");                                      // start in integer mode; 1 means the result widened to a float
    emitter.instruction("pxor xmm0, xmm0");                                     // start the float accumulator at 0.0
    emitter.instruction("cvtsi2sd xmm1, rcx");                                  // keep the base available as a double for the float accumulation

    emitter.label("__rt_base_to_number_loop_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // stop once every input byte has been inspected
    emitter.instruction("jz __rt_base_to_number_done_linux_x86_64");            // finish when the input string is exhausted
    emitter.instruction("movzx rax, BYTE PTR [rdi]");                           // load the next input byte
    emitter.instruction("add rdi, 1");                                          // advance the input cursor
    emitter.instruction("sub rsi, 1");                                          // record that one input byte has been consumed

    // -- decode one digit, ignoring every character that is not one --
    emitter.instruction("mov rdx, rax");                                        // copy the byte before deriving its numeral value
    emitter.instruction("sub rdx, 48");                                         // try the ASCII numerals first
    emitter.instruction("cmp rdx, 9");                                          // is the byte in '0'..'9' (unsigned, so lower bytes wrap high)?
    emitter.instruction("jbe __rt_base_to_number_digit_linux_x86_64");          // an ASCII numeral decodes directly
    emitter.instruction("cmp rax, 65");                                         // is the byte below 'A'?
    emitter.instruction("jb __rt_base_to_number_loop_linux_x86_64");            // punctuation between the numerals and 'A' is ignored
    emitter.instruction("cmp rax, 90");                                         // is the byte above 'Z'?
    emitter.instruction("ja __rt_base_to_number_lower_linux_x86_64");           // try the lowercase letters instead
    emitter.instruction("mov rdx, rax");                                        // copy the byte before deriving its letter value
    emitter.instruction("sub rdx, 55");                                         // map 'A'..'Z' to digit values 10..35
    emitter.instruction("jmp __rt_base_to_number_digit_linux_x86_64");          // the uppercase letter decoded to a digit
    emitter.label("__rt_base_to_number_lower_linux_x86_64");
    emitter.instruction("cmp rax, 97");                                         // is the byte below 'a'?
    emitter.instruction("jb __rt_base_to_number_loop_linux_x86_64");            // punctuation between 'Z' and 'a' is ignored
    emitter.instruction("cmp rax, 122");                                        // is the byte above 'z'?
    emitter.instruction("ja __rt_base_to_number_loop_linux_x86_64");            // bytes past 'z' are ignored
    emitter.instruction("mov rdx, rax");                                        // copy the byte before deriving its letter value
    emitter.instruction("sub rdx, 87");                                         // map 'a'..'z' to digit values 10..35

    emitter.label("__rt_base_to_number_digit_linux_x86_64");
    emitter.instruction("cmp rdx, rcx");                                        // is the decoded digit valid in the requested base?
    emitter.instruction("jae __rt_base_to_number_loop_linux_x86_64");           // digits outside the base are ignored, exactly like php-src
    emitter.instruction("test r11, r11");                                       // has the accumulator already widened to a float?
    emitter.instruction("jnz __rt_base_to_number_float_linux_x86_64");          // once widened, every further digit accumulates as a float
    emitter.instruction("cmp r10, r8");                                         // would this digit push the accumulator past PHP_INT_MAX?
    emitter.instruction("jb __rt_base_to_number_int_linux_x86_64");             // below the threshold the integer accumulator always fits
    emitter.instruction("ja __rt_base_to_number_widen_linux_x86_64");           // above the threshold the accumulator must widen
    emitter.instruction("cmp rdx, r9");                                         // at the threshold only digits up to the remainder still fit
    emitter.instruction("ja __rt_base_to_number_widen_linux_x86_64");           // a larger digit forces the widening to float

    emitter.label("__rt_base_to_number_int_linux_x86_64");
    emitter.instruction("mov rax, r10");                                        // stage the accumulator for the digit shift
    emitter.instruction("imul rax, rcx");                                       // shift the accumulator up by one digit position
    emitter.instruction("add rax, rdx");                                        // accumulate the digit into the integer result
    emitter.instruction("mov r10, rax");                                        // keep the updated integer accumulator
    emitter.instruction("jmp __rt_base_to_number_loop_linux_x86_64");           // continue scanning the remaining input bytes

    emitter.label("__rt_base_to_number_widen_linux_x86_64");
    emitter.instruction("cvtsi2sd xmm0, r10");                                  // seed the float accumulator from the integer digits parsed so far
    emitter.instruction("mov r11, 1");                                          // remember that the result is now a float

    emitter.label("__rt_base_to_number_float_linux_x86_64");
    emitter.instruction("mulsd xmm0, xmm1");                                    // shift the float accumulator up by one digit position
    emitter.instruction("cvtsi2sd xmm2, rdx");                                  // convert the current digit for the float accumulation
    emitter.instruction("addsd xmm0, xmm2");                                    // accumulate the digit into the float result
    emitter.instruction("jmp __rt_base_to_number_loop_linux_x86_64");           // continue scanning the remaining input bytes

    emitter.label("__rt_base_to_number_done_linux_x86_64");
    emitter.instruction("test r11, r11");                                       // did the accumulator widen to a float?
    emitter.instruction("jnz __rt_base_to_number_float_result_linux_x86_64");   // report a widened result through the float register
    emitter.instruction("xor eax, eax");                                        // report that the result is a PHP integer
    emitter.instruction("mov rdx, r10");                                        // return the accumulated integer value
    emitter.instruction("ret");                                                 // hand the integer result back to the caller

    emitter.label("__rt_base_to_number_float_result_linux_x86_64");
    emitter.instruction("mov rax, 1");                                          // report that the result widened to a PHP float
    emitter.instruction("ret");                                                 // the float result is already in xmm0
}
