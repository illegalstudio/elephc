//! Purpose:
//! Emits the `__rt_base_convert` runtime helper assembly for PHP's `base_convert`: parses a
//! numeral string in one base and renders it in another, reproducing php-src's float path.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - php-src composes `_php_math_basetozval` and `_php_math_zvaltobase`, so this helper
//!   composes the same two runtime pieces: `__rt_base_to_number` for the parse and
//!   `__rt_dec_to_base` for the integer render. Both already match reference PHP exactly.
//! - A value past `PHP_INT_MAX` widens to `double` during the parse, and php-src then renders
//!   it with a LOSSY loop (`digit = (int) fmod(v, base); v /= base;` without re-flooring).
//!   That is why `base_convert("ffffffffffffffff", 16, 10)` is `"18446744073709552046"` and
//!   not the exact value; the loop below reproduces those exact float operations.
//! - `fmod` is computed inline by scaled subtraction (double the divisor until it passes the
//!   dividend, then halve it back down, subtracting whenever it fits). Every step is exact
//!   under IEEE-754, so the result is bit-identical to libc `fmod` without linking libm into
//!   the runtime object.
//! - php-src caps the rendered digits at 64 (`char buf[(sizeof(double) << 3) + 1]`) and
//!   returns an empty string for an infinite value; both bounds are reproduced here.
//! - Bases outside `2..=36` never reach this helper: the EIR lowering raises php-src's
//!   `ValueError` for both base arguments before the call.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Bit pattern of IEEE-754 positive infinity, used to reject an overflowed parse result.
const F64_INFINITY_BITS: i64 = 0x7ff0_0000_0000_0000;

/// Mask that clears the IEEE-754 sign bit, turning a bit pattern into its magnitude.
const F64_ABS_MASK: i64 = 0x7fff_ffff_ffff_ffff;

/// Bit pattern of `1.0`, compared as an integer magnitude to test `fabs(value) >= 1`.
const F64_ONE_BITS: i64 = 0x3ff0_0000_0000_0000;

/// Bit pattern of `0.5`, the exact halving factor of the inline `fmod` reduction.
const F64_HALF_BITS: i64 = 0x3fe0_0000_0000_0000;

/// Largest digit count php-src's `_php_math_zvaltobase` float buffer can hold.
const MAX_FLOAT_DIGITS: i64 = 64;

/// Emits the `__rt_base_convert` runtime helper.
///
/// ABI (AArch64):
///   Input:  `x1` = numeral pointer, `x2` = numeral length, `x3` = source base (2..36),
///           `x4` = target base (2..36).
///   Output: `x1` = result pointer, `x2` = result length.
///
/// ABI (x86_64 System V):
///   Input:  `rdi` = numeral pointer, `rsi` = numeral length, `rdx` = source base (2..36),
///           `rcx` = target base (2..36).
///   Output: `rax` = result pointer, `rdx` = result length.
///
/// Clobbers every caller-saved register. The result is published through
/// `__rt_concat_publish`, so it lives in the shared concat scratch while it fits and in an
/// owned heap block otherwise.
pub fn emit_base_convert(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_base_convert_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: base_convert ---");
    emitter.label_global("__rt_base_convert");

    emitter.instruction("sub sp, sp, #96");                                     // reserve the 64-byte digit buffer plus spill and frame slots
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save the frame pointer and return address across the nested calls
    emitter.instruction("add x29, sp, #80");                                    // establish the base_convert helper frame pointer
    emitter.instruction("str x4, [sp, #64]");                                   // save the target base across the parse call
    emitter.instruction("bl __rt_base_to_number");                              // parse the numeral exactly like php-src's _php_math_basetozval
    emitter.instruction("ldr x4, [sp, #64]");                                   // reload the target base after the parse
    if crate::codegen_support::compile_php_version() >= crate::php_version::PhpVersion::Php86 {
        // The parse reports a widened float in `x0`; PHP 8.6 warns about the precision loss
        // before rendering. The wrapper tail-jumps into the shared dispatcher, which
        // preserves `x0`/`x1`/`d0`, so the widened result survives unchanged.
        emitter.instruction("cbz x0, __rt_base_convert_parse_exact");           // exact integer parses are silent
        emitter.instruction("bl __rt_base_conversion_imprecise_notice");         // report PHP 8.6's precision loss for the widening parse
        emitter.label("__rt_base_convert_parse_exact");
    }
    emitter.instruction("cbnz x0, __rt_base_convert_float");                    // a widened parse result renders through php-src's lossy float loop

    // -- integer result: render it exactly, like dechex/decbin/decoct do --
    emitter.instruction("mov x0, x1");                                          // the parsed integer is the value to render
    emitter.instruction("mov x3, x4");                                          // render it in the requested target base
    emitter.instruction("bl __rt_dec_to_base");                                 // reuse the shared unsigned integer-to-base renderer
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the base_convert helper frame
    emitter.instruction("ret");                                                 // return the rendered digits as a PHP string pair

    // -- float result: reproduce php-src's floor + fmod digit loop bit for bit --
    emitter.label("__rt_base_convert_float");
    emitter.instruction("frintm d0, d0");                                       // php-src floors the widened value before rendering it
    emitter.instruction("fmov x9, d0");                                         // inspect the value's bit pattern to reject infinities
    abi::emit_load_int_immediate(emitter, "x10", F64_ABS_MASK);
    emitter.instruction("and x9, x9, x10");                                     // drop the sign bit to compare the magnitude
    abi::emit_load_int_immediate(emitter, "x10", F64_INFINITY_BITS);
    emitter.instruction("cmp x9, x10");                                         // did the parse overflow to infinity?
    emitter.instruction("b.eq __rt_base_convert_too_large");                    // php-src returns an empty string for a value it cannot render
    emitter.instruction("ucvtf d1, x4");                                        // keep the target base available as a double divisor
    emitter.instruction("add x11, sp, #64");                                    // point the digit cursor just past the end of the digit buffer
    emitter.instruction("mov x12, #0");                                         // start with no digits emitted

    emitter.label("__rt_base_convert_digit");
    // -- inline exact fmod(d0, d1) into d2 by scaled subtraction --
    emitter.instruction("fmov d2, d0");                                         // the running remainder starts as the whole value
    emitter.instruction("fcmp d2, d1");                                         // is the value already smaller than the base?
    emitter.instruction("b.lt __rt_base_convert_mod_done");                     // then it is its own remainder

    emitter.instruction("fmov d5, d1");                                         // scaled divisor starts at the base itself
    emitter.label("__rt_base_convert_scale");
    emitter.instruction("fadd d6, d5, d5");                                     // double the scaled divisor, exactly
    emitter.instruction("fcmp d6, d2");                                         // has the scaled divisor passed the remainder?
    emitter.instruction("b.gt __rt_base_convert_reduce");                       // start reducing once doubling would overshoot
    emitter.instruction("fmov d5, d6");                                         // keep the doubled divisor and try again
    emitter.instruction("b __rt_base_convert_scale");                           // continue scaling the divisor upwards

    emitter.label("__rt_base_convert_reduce");
    emitter.instruction("fcmp d2, d5");                                         // does the scaled divisor still fit in the remainder?
    emitter.instruction("b.lt __rt_base_convert_no_sub");                       // skip the subtraction when it does not fit
    emitter.instruction("fsub d2, d2, d5");                                     // subtract it; both operands are within a factor of two, so this is exact
    emitter.label("__rt_base_convert_no_sub");
    emitter.instruction("fcmp d5, d1");                                         // has the divisor been halved back down to the base?
    emitter.instruction("b.le __rt_base_convert_mod_done");                     // the remainder is now below the base
    emitter.instruction("fmov d7, #0.5");                                       // exact halving factor
    emitter.instruction("fmul d5, d5, d7");                                     // halve the scaled divisor, exactly
    emitter.instruction("b __rt_base_convert_reduce");                          // reduce against the next lower scale

    emitter.label("__rt_base_convert_mod_done");
    emitter.instruction("fcvtzs x13, d2");                                      // php-src truncates the remainder to a digit index
    emitter.instruction("cmp x13, #10");                                        // does this digit need a letter rather than a numeral?
    emitter.instruction("b.lo __rt_base_convert_numeral");                      // digits 0-9 use the ASCII numerals
    emitter.instruction("add w13, w13, #87");                                   // map digits 10-35 to lowercase 'a'-'z'
    emitter.instruction("b __rt_base_convert_store");                           // the digit character is ready to store
    emitter.label("__rt_base_convert_numeral");
    emitter.instruction("add w13, w13, #48");                                   // map digits 0-9 to ASCII '0'-'9'
    emitter.label("__rt_base_convert_store");
    emitter.instruction("strb w13, [x11, #-1]!");                               // store the digit least-significant-first, walking backwards
    emitter.instruction("add x12, x12, #1");                                    // count the digit just emitted
    emitter.instruction("fdiv d0, d0, d1");                                     // php-src divides WITHOUT re-flooring, which is what makes the render lossy
    emitter.instruction(&format!("cmp x12, #{MAX_FLOAT_DIGITS}"));              // php-src's digit buffer holds at most 64 characters
    emitter.instruction("b.hs __rt_base_convert_emit");                         // stop once the buffer is full
    emitter.instruction("fabs d3, d0");                                         // php-src continues while fabs(value) >= 1
    emitter.instruction("fmov d4, #1.0");                                       // materialize the loop's lower bound
    emitter.instruction("fcmp d3, d4");                                         // is there still a whole digit left to render?
    emitter.instruction("b.ge __rt_base_convert_digit");                        // render the next digit

    emitter.label("__rt_base_convert_emit");
    emitter.instruction("stp x11, x12, [sp, #64]");                             // save the first-digit pointer and the digit count across the reservation
    emitter.instruction("mov x0, x12");                                         // reserve exactly as many bytes as the rendered result needs
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the rendered digits
    emitter.instruction("ldp x11, x12, [sp, #64]");                             // reload the first-digit pointer and digit count after the reservation
    emitter.instruction("mov x1, x0");                                          // the reservation start is the published result pointer
    emitter.instruction("mov x2, x12");                                         // the digit count is the published result length
    emitter.instruction("mov x13, #0");                                         // start copying at the first rendered digit

    emitter.label("__rt_base_convert_copy");
    emitter.instruction("cmp x13, x12");                                        // have all rendered digits been copied out?
    emitter.instruction("b.hs __rt_base_convert_copied");                       // finish once the whole result has been copied
    emitter.instruction("ldrb w14, [x11, x13]");                                // load the next rendered digit from the frame buffer
    emitter.instruction("strb w14, [x0, x13]");                                 // store it into the reserved result storage
    emitter.instruction("add x13, x13, #1");                                    // advance the copy index
    emitter.instruction("b __rt_base_convert_copy");                            // copy the next rendered digit

    emitter.label("__rt_base_convert_copied");
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the base_convert helper frame
    emitter.instruction("ret");                                                 // return the rendered digits as a PHP string pair

    // -- infinite parse result: php-src warns and returns an empty string --
    emitter.label("__rt_base_convert_too_large");
    emitter.instruction("mov x0, #0");                                          // an empty result needs no storage
    emitter.instruction("bl __rt_concat_reserve");                              // still take a valid zero-length reservation
    emitter.instruction("mov x1, x0");                                          // the reservation start is the published result pointer
    emitter.instruction("mov x2, #0");                                          // the empty result has zero length
    emitter.instruction("bl __rt_concat_publish");                              // publish the empty result without moving the scratch offset
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the base_convert helper frame
    emitter.instruction("ret");                                                 // return the empty string as a PHP string pair
}

/// Emits `__rt_base_convert` for x86_64 Linux using the System V ABI.
///
/// The 64-byte digit buffer sits at `[rbp-128, rbp-64)` and the spill slots at
/// `[rbp-40]`/`[rbp-48]`, deliberately clear of the `[rbp-8]`..`[rbp-24]` window other
/// runtime emitters reserve for pushed callee-saved registers. `fabs(value) >= 1` is tested
/// on the integer bit pattern, which is order-preserving for finite magnitudes and avoids
/// needing a separate sign-mask XMM register.
fn emit_base_convert_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: base_convert ---");
    emitter.label_global("__rt_base_convert");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the nested calls
    emitter.instruction("mov rbp, rsp");                                        // establish the base_convert helper frame pointer
    emitter.instruction("sub rsp, 144");                                        // reserve the digit buffer and spill slots, keeping the stack 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the target base across the parse call
    emitter.instruction("call __rt_base_to_number");                            // parse the numeral exactly like php-src's _php_math_basetozval
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the target base after the parse
    if crate::codegen_support::compile_php_version() >= crate::php_version::PhpVersion::Php86 {
        // `rax` carries the widened marker; the dispatcher preserves it and the float value.
        emitter.instruction("test rax, rax");                                   // did the parse widen the value to a float?
        emitter.instruction("jz __rt_base_convert_parse_exact_linux_x86_64");  // exact integer parses are silent
        emitter.instruction("call __rt_base_conversion_imprecise_notice");      // report PHP 8.6's precision loss for the widening parse
        emitter.label("__rt_base_convert_parse_exact_linux_x86_64");
    }
    emitter.instruction("test rax, rax");                                       // did the parse widen the value to a float?
    emitter.instruction("jnz __rt_base_convert_float_linux_x86_64");            // a widened parse result renders through php-src's lossy float loop

    // -- integer result: render it exactly, like dechex/decbin/decoct do --
    emitter.instruction("mov rax, rdx");                                        // the parsed integer is the value to render
    emitter.instruction("mov rdi, rcx");                                        // render it in the requested target base
    emitter.instruction("call __rt_dec_to_base");                               // reuse the shared unsigned integer-to-base renderer
    emitter.instruction("add rsp, 144");                                        // release the base_convert helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the rendered digits as a PHP string pair

    // -- float result: reproduce php-src's floor + fmod digit loop bit for bit --
    emitter.label("__rt_base_convert_float_linux_x86_64");
    emitter.instruction("roundsd xmm0, xmm0, 1");                               // php-src floors the widened value before rendering it
    emitter.instruction("movq rax, xmm0");                                      // inspect the value's bit pattern to reject infinities
    emitter.instruction(&format!("mov r8, 0x{F64_ABS_MASK:x}"));                // materialize the IEEE-754 sign mask
    emitter.instruction("and rax, r8");                                         // drop the sign bit to compare the magnitude
    emitter.instruction(&format!("mov r8, 0x{F64_INFINITY_BITS:x}"));           // materialize the infinity bit pattern
    emitter.instruction("cmp rax, r8");                                         // did the parse overflow to infinity?
    emitter.instruction("je __rt_base_convert_too_large_linux_x86_64");         // php-src returns an empty string for a value it cannot render
    emitter.instruction("cvtsi2sd xmm1, rcx");                                  // keep the target base available as a double divisor
    emitter.instruction(&format!("mov r8, 0x{F64_HALF_BITS:x}"));               // materialize the exact halving factor
    emitter.instruction("movq xmm7, r8");                                       // keep 0.5 available for the inline fmod reduction
    emitter.instruction("lea r9, [rbp - 64]");                                  // point the digit cursor just past the end of the digit buffer
    emitter.instruction("xor r10d, r10d");                                      // start with no digits emitted

    emitter.label("__rt_base_convert_digit_linux_x86_64");
    // -- inline exact fmod(xmm0, xmm1) into xmm2 by scaled subtraction --
    emitter.instruction("movapd xmm2, xmm0");                                   // the running remainder starts as the whole value
    emitter.instruction("comisd xmm2, xmm1");                                   // is the value already smaller than the base?
    emitter.instruction("jb __rt_base_convert_mod_done_linux_x86_64");          // then it is its own remainder
    emitter.instruction("movapd xmm5, xmm1");                                   // scaled divisor starts at the base itself

    emitter.label("__rt_base_convert_scale_linux_x86_64");
    emitter.instruction("movapd xmm6, xmm5");                                   // copy the scaled divisor before doubling it
    emitter.instruction("addsd xmm6, xmm5");                                    // double the scaled divisor, exactly
    emitter.instruction("comisd xmm6, xmm2");                                   // has the scaled divisor passed the remainder?
    emitter.instruction("ja __rt_base_convert_reduce_linux_x86_64");            // start reducing once doubling would overshoot
    emitter.instruction("movapd xmm5, xmm6");                                   // keep the doubled divisor and try again
    emitter.instruction("jmp __rt_base_convert_scale_linux_x86_64");            // continue scaling the divisor upwards

    emitter.label("__rt_base_convert_reduce_linux_x86_64");
    emitter.instruction("comisd xmm2, xmm5");                                   // does the scaled divisor still fit in the remainder?
    emitter.instruction("jb __rt_base_convert_no_sub_linux_x86_64");            // skip the subtraction when it does not fit
    emitter.instruction("subsd xmm2, xmm5");                                    // subtract it; both operands are within a factor of two, so this is exact

    emitter.label("__rt_base_convert_no_sub_linux_x86_64");
    emitter.instruction("comisd xmm5, xmm1");                                   // has the divisor been halved back down to the base?
    emitter.instruction("jbe __rt_base_convert_mod_done_linux_x86_64");         // the remainder is now below the base
    emitter.instruction("mulsd xmm5, xmm7");                                    // halve the scaled divisor, exactly
    emitter.instruction("jmp __rt_base_convert_reduce_linux_x86_64");           // reduce against the next lower scale

    emitter.label("__rt_base_convert_mod_done_linux_x86_64");
    emitter.instruction("cvttsd2si rax, xmm2");                                 // php-src truncates the remainder to a digit index
    emitter.instruction("cmp rax, 10");                                         // does this digit need a letter rather than a numeral?
    emitter.instruction("jb __rt_base_convert_numeral_linux_x86_64");           // digits 0-9 use the ASCII numerals
    emitter.instruction("add rax, 87");                                         // map digits 10-35 to lowercase 'a'-'z'
    emitter.instruction("jmp __rt_base_convert_store_linux_x86_64");            // the digit character is ready to store
    emitter.label("__rt_base_convert_numeral_linux_x86_64");
    emitter.instruction("add rax, 48");                                         // map digits 0-9 to ASCII '0'-'9'
    emitter.label("__rt_base_convert_store_linux_x86_64");
    emitter.instruction("sub r9, 1");                                           // walk the digit cursor backwards by one character
    emitter.instruction("mov BYTE PTR [r9], al");                               // store the digit least-significant-first
    emitter.instruction("add r10, 1");                                          // count the digit just emitted
    emitter.instruction("divsd xmm0, xmm1");                                    // php-src divides WITHOUT re-flooring, which is what makes the render lossy
    emitter.instruction(&format!("cmp r10, {MAX_FLOAT_DIGITS}"));               // php-src's digit buffer holds at most 64 characters
    emitter.instruction("jae __rt_base_convert_emit_linux_x86_64");             // stop once the buffer is full
    emitter.instruction("movq rax, xmm0");                                      // php-src continues while fabs(value) >= 1
    emitter.instruction(&format!("mov r11, 0x{F64_ABS_MASK:x}"));               // materialize the IEEE-754 sign mask
    emitter.instruction("and rax, r11");                                        // compare magnitudes, which is order-preserving on the bit pattern
    emitter.instruction(&format!("mov r11, 0x{F64_ONE_BITS:x}"));               // materialize the bit pattern of 1.0
    emitter.instruction("cmp rax, r11");                                        // is there still a whole digit left to render?
    emitter.instruction("jae __rt_base_convert_digit_linux_x86_64");            // render the next digit

    emitter.label("__rt_base_convert_emit_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save the first-digit pointer across the reservation call
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the digit count across the reservation call
    emitter.instruction("mov rax, r10");                                        // reserve exactly as many bytes as the rendered result needs
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the rendered digits
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the first-digit pointer after the reservation
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the digit count after the reservation
    emitter.instruction("xor ecx, ecx");                                        // start copying at the first rendered digit

    emitter.label("__rt_base_convert_copy_linux_x86_64");
    emitter.instruction("cmp rcx, r10");                                        // have all rendered digits been copied out?
    emitter.instruction("jae __rt_base_convert_copied_linux_x86_64");           // finish once the whole result has been copied
    emitter.instruction("mov r8b, BYTE PTR [r9 + rcx]");                        // load the next rendered digit from the frame buffer
    emitter.instruction("mov BYTE PTR [rax + rcx], r8b");                       // store it into the reserved result storage
    emitter.instruction("add rcx, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_base_convert_copy_linux_x86_64");             // copy the next rendered digit

    emitter.label("__rt_base_convert_copied_linux_x86_64");
    emitter.instruction("mov rdx, r10");                                        // the digit count is the published result length
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("add rsp, 144");                                        // release the base_convert helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the rendered digits as a PHP string pair

    // -- infinite parse result: php-src warns and returns an empty string --
    emitter.label("__rt_base_convert_too_large_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // an empty result needs no storage
    emitter.instruction("call __rt_concat_reserve");                            // still take a valid zero-length reservation
    emitter.instruction("xor edx, edx");                                        // the empty result has zero length
    emitter.instruction("call __rt_concat_publish");                            // publish the empty result without moving the scratch offset
    emitter.instruction("add rsp, 144");                                        // release the base_convert helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the empty string as a PHP string pair
}
