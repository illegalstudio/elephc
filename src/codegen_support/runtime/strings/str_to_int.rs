//! Purpose:
//! Emits the `__rt_str_to_int` runtime helper for PHP string-to-int casts.
//! Parses the bounded PHP string as both an integer (libc `strtoll`) and a double (libc `strtod`)
//! and returns the integer-form value unless the string is actually float-form.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - `__rt_php_num_scan` clips the scratch string to PHP's leading numeric run before either
//!   libc parser runs, so `"0x1A"` is `0` (not `26`), `"INF"`/`"NAN"` are `0`, and `"1_000"`
//!   is `1` — libc's `strtoll`/`strtod` extensions never reach the value.
//! - A run whose double is NaN or ±INF casts to `0`, whichever form the string took. That check
//!   runs first, which is what makes `(int)str_repeat("1", 310)` agree with PHP instead of
//!   returning the `PHP_INT_MAX` `strtoll` saturated to.
//! - Integer-form run: `strtoll` gives the exact 64-bit value and PHP's saturating overflow
//!   (LLONG_MAX/MIN == PHP_INT_MAX/MIN), so large integer strings are not rounded through `f64`.
//! - Float-form run (`strtod` consumed more bytes than `strtoll`, e.g. `"1e19"`): the double goes
//!   through `__rt_php_float_to_int_cap`, PHP's SATURATING numeric-string rule. It is not the
//!   modulo-2^64 `__rt_php_float_to_int` a float VALUE takes — that one turns `(int)"1e19"` into
//!   -8446744073709551616 instead of `PHP_INT_MAX`. Both rules live in `runtime::numeric`.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_str_to_int` for PHP string-to-int conversion.
///
/// Input follows the active string-result convention:
/// AArch64 uses `x1`/`x2`; x86_64 uses `rax`/`rdx`.
/// The helper copies the string into the C-string scratch buffer via `__rt_cstr`, clips it to PHP's
/// leading numeric run with `__rt_php_num_scan`, then parses that run with `strtoll` (exact +
/// saturating) and `strtod`. A non-finite double casts to `0`; an integer-form run returns the
/// exact `strtoll` value; a float-form run is capped by `__rt_php_float_to_int_cap`.
pub fn emit_str_to_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_to_int_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_to_int ---");
    emitter.label_global("__rt_str_to_int");

    // -- direct decimal fast path --
    // A plain decimal run of at most 18 digits (optional ASCII whitespace and sign) always fits
    // i64 and needs neither the C-string copy nor libc. Accumulate it in place; a 19th digit or
    // a `.`/`e`/`E` continuation bails to the general parser, which owns saturation and PHP's
    // float-form cap. Any other terminator returns the exact integer prefix.
    emitter.instruction("mov x3, x1");                                          // cursor = the PHP string payload
    emitter.instruction("mov x4, x2");                                          // remaining input bytes
    emitter.instruction("mov x5, #0");                                          // negative flag
    emitter.label("__rt_str_to_int_fast_space");
    emitter.instruction("cbz x4, __rt_str_to_int_fast_zero");                   // a blank-only string parses to zero
    emitter.instruction("ldrb w8, [x3]");                                       // load the next candidate byte
    emitter.instruction("cmp w8, #32");                                         // is the byte a plain space?
    emitter.instruction("b.eq __rt_str_to_int_fast_space_next");                // spaces are skipped
    emitter.instruction("sub w9, w8, #9");                                      // rebase onto the tab..carriage-return block
    emitter.instruction("cmp w9, #4");                                          // is the byte one of \t \n \v \f \r?
    emitter.instruction("b.hi __rt_str_to_int_fast_sign");                      // the first non-blank byte starts the number
    emitter.label("__rt_str_to_int_fast_space_next");
    emitter.instruction("add x3, x3, #1");                                      // consume the blank byte
    emitter.instruction("sub x4, x4, #1");                                      // account for the consumed byte
    emitter.instruction("b __rt_str_to_int_fast_space");                        // keep skipping blanks
    emitter.label("__rt_str_to_int_fast_sign");
    emitter.instruction("cmp w8, #45");                                         // is the byte a minus sign?
    emitter.instruction("b.ne __rt_str_to_int_fast_plus");                      // try the plus sign instead
    emitter.instruction("mov x5, #1");                                          // remember that the result is negative
    emitter.instruction("b __rt_str_to_int_fast_sign_taken");                   // consume the sign byte
    emitter.label("__rt_str_to_int_fast_plus");
    emitter.instruction("cmp w8, #43");                                         // is the byte a plus sign?
    emitter.instruction("b.ne __rt_str_to_int_fast_digits");                    // no sign means the digits start here
    emitter.label("__rt_str_to_int_fast_sign_taken");
    emitter.instruction("add x3, x3, #1");                                      // consume the sign byte
    emitter.instruction("sub x4, x4, #1");                                      // account for the consumed byte
    emitter.instruction("cbz x4, __rt_str_to_int_fast_zero");                   // a sign with no digits parses to zero
    emitter.label("__rt_str_to_int_fast_digits");
    emitter.instruction("mov x6, #0");                                          // unsigned accumulator
    emitter.instruction("mov x11, #0");                                         // significant digit count
    emitter.label("__rt_str_to_int_fast_loop");
    emitter.instruction("cbz x4, __rt_str_to_int_fast_finish");                 // stop at the end of the string
    emitter.instruction("ldrb w8, [x3]");                                       // load the next input byte
    emitter.instruction("sub w13, w8, #48");                                    // map ASCII numerals onto 0..9
    emitter.instruction("cmp w13, #9");                                         // is this still a decimal digit?
    emitter.instruction("b.hi __rt_str_to_int_fast_non_digit");                 // the digit run ended
    emitter.instruction("add x11, x11, #1");                                    // count the significant digit
    emitter.instruction("cmp x11, #18");                                        // can the accumulator still hold this digit?
    emitter.instruction("b.hi __rt_str_to_int_fast_slow");                      // longer integer runs need saturation from the general parser
    emitter.instruction("mov x14, #10");                                        // decimal radix
    emitter.instruction("madd x6, x6, x14, x13");                               // acc = acc * 10 + digit
    emitter.instruction("add x3, x3, #1");                                      // consume the digit
    emitter.instruction("sub x4, x4, #1");                                      // account for the consumed digit
    emitter.instruction("b __rt_str_to_int_fast_loop");                         // continue scanning digits
    emitter.label("__rt_str_to_int_fast_non_digit");
    emitter.instruction("cbz x11, __rt_str_to_int_fast_finish");                // no digit has been seen: the run is empty
    emitter.instruction("cmp w8, #46");                                         // is the next byte a decimal point?
    emitter.instruction("b.eq __rt_str_to_int_fast_slow");                      // fraction handling needs the general parser
    emitter.instruction("cmp w8, #101");                                        // is the next byte a lowercase exponent?
    emitter.instruction("b.eq __rt_str_to_int_fast_slow");                      // exponent handling needs the general parser
    emitter.instruction("cmp w8, #69");                                         // is the next byte an uppercase exponent?
    emitter.instruction("b.eq __rt_str_to_int_fast_slow");                      // exponent handling needs the general parser
    emitter.label("__rt_str_to_int_fast_finish");
    emitter.instruction("mov x0, x6");                                          // return the accumulated magnitude
    emitter.instruction("cbz x5, __rt_str_to_int_fast_ret");                    // a positive run is already final
    emitter.instruction("neg x0, x0");                                          // apply the consumed minus sign
    emitter.label("__rt_str_to_int_fast_ret");
    emitter.instruction("ret");                                                 // return the exact integer
    emitter.label("__rt_str_to_int_fast_zero");
    emitter.instruction("mov x0, #0");                                          // an empty or digit-free run parses to zero
    emitter.instruction("ret");                                                 // return the zero result
    emitter.label("__rt_str_to_int_fast_slow");

    // -- set up the helper frame (slots: end_i=[sp,#0], end_d=[sp,#8], ll_val=[sp,#16], cstr=[sp,#24]) --
    emitter.instruction("sub sp, sp, #48");                                     // allocate slots for both end pointers, the integer value, and the C-string pointer
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across the libc calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable helper frame pointer

    // -- copy the PHP string into the C-string scratch buffer and clip it to PHP's grammar --
    emitter.instruction("bl __rt_cstr");                                        // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("bl __rt_php_num_scan");                                // clip the scratch to PHP's leading numeric run
    emitter.instruction("str x0, [sp, #24]");                                   // save the clipped run pointer for the second parse

    // -- integer parse: strtoll(run, &end_i, 10) gives the exact, saturating 64-bit value --
    emitter.instruction("add x1, sp, #0");                                      // pass &end_i so strtoll reports where the integer prefix ended
    emitter.instruction("mov x2, #10");                                         // parse in base 10 like PHP string-to-int
    emitter.bl_c("strtoll");
    emitter.instruction("str x0, [sp, #16]");                                   // save the integer-form value (LLONG_MAX/MIN on overflow == PHP_INT_MAX/MIN)

    // -- float parse: strtod(cstr, &end_d) detects a '.'/'e' float continuation --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the clipped run pointer for strtod
    emitter.instruction("add x1, sp, #8");                                      // pass &end_d so strtod reports where the numeric value ended
    emitter.bl_c("strtod");

    // -- a value PHP cannot represent casts to 0, whichever form the string took --
    // `strtoll` saturates a 400-digit integer to PHP_INT_MAX, but PHP classifies a string whose
    // value overflows the double as IS_DOUBLE and cast INF to 0. Checking the parsed double
    // first is what makes `(int)str_repeat("1", 310)` agree with PHP.
    emitter.instruction("fmov x9, d0");                                         // raw IEEE-754 bit pattern of the parsed double
    emitter.instruction("lsl x9, x9, #1");                                      // drop the sign bit: NaN and both infinities compare alike
    abi::emit_load_int_immediate(emitter, "x10", 0xffe0_0000_0000_0000u64 as i64); // twice the exponent-all-ones pattern
    emitter.instruction("cmp x9, x10");                                         // is the magnitude infinite or NaN?
    emitter.instruction("b.hs __rt_str_to_int_zero");                           // yes: PHP casts it to 0, whichever form the string took

    // -- choose the integer value unless strtod consumed more bytes (a float part) --
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the end pointer returned by strtod
    emitter.instruction("ldr x10, [sp, #0]");                                   // load the end pointer returned by strtoll
    emitter.instruction("cmp x9, x10");                                         // did strtod consume more bytes than strtoll?
    emitter.instruction("b.hi __rt_str_to_int_float");                          // yes: the string is float-form, cap the double
    emitter.instruction("ldr x0, [sp, #16]");                                   // no: return the exact integer-form value
    emitter.instruction("b __rt_str_to_int_done");                              // skip the float path

    emitter.label("__rt_str_to_int_float");
    // PHP CAPS a numeric string's value; it does not wrap it the way a float VALUE is wrapped,
    // so the sibling `__rt_php_float_to_int` is deliberately NOT used here -- its modulo-2^64
    // reduction turned `(int)"1e19"` into -8446744073709551616. Both rules live in
    // `runtime::numeric` so neither is open-coded at a call site.
    emitter.instruction("bl __rt_php_float_to_int_cap");                        // apply PHP's numeric-string cap
    emitter.instruction("mov x0, x9");                                          // move the capped value into the result register
    emitter.instruction("b __rt_str_to_int_done");                              // share the epilogue

    emitter.label("__rt_str_to_int_zero");
    emitter.instruction("mov x0, #0");                                          // NaN and +-INF cast to 0

    emitter.label("__rt_str_to_int_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the integer cast result
}

/// Emits the Linux x86_64 `__rt_str_to_int` runtime helper.
///
/// The input string arrives in the elephc string-result registers (`rax`/`rdx`).
/// Clips the scratch to PHP's leading numeric run with `__rt_php_num_scan`, then parses with
/// `strtoll` (exact + saturating) and `strtod`, returning the integer-form value in `rax`. A
/// non-finite double casts to `0`, and a run with a `.`/`e` float part is capped by
/// `__rt_php_float_to_int_cap` rather than wrapped.
fn emit_str_to_int_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_to_int ---");
    emitter.label_global("__rt_str_to_int");

    // -- direct decimal fast path --
    // A plain decimal run of at most 18 digits (optional ASCII whitespace and sign) always fits
    // i64 and needs neither the C-string copy nor libc. Accumulate it in place; a 19th digit or
    // a `.`/`e`/`E` continuation bails to the general parser, which owns saturation and PHP's
    // float-form cap. Any other terminator returns the exact integer prefix.
    emitter.instruction("mov r8, rax");                                         // cursor = the PHP string payload
    emitter.instruction("mov r9, rdx");                                         // remaining input bytes
    emitter.instruction("xor r10d, r10d");                                      // bit 0 = negative, bit 1 = digit seen
    emitter.label("__rt_str_to_int_fast_space_x");
    emitter.instruction("test r9, r9");                                         // is the string exhausted?
    emitter.instruction("jz __rt_str_to_int_fast_zero_x");                      // a blank-only string parses to zero
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                            // load the next candidate byte
    emitter.instruction("cmp ecx, 32");                                         // is the byte a plain space?
    emitter.instruction("je __rt_str_to_int_fast_space_next_x");                // spaces are skipped
    emitter.instruction("lea edi, [rcx - 9]");                                  // rebase onto the tab..carriage-return block
    emitter.instruction("cmp edi, 4");                                          // is the byte one of \t \n \v \f \r?
    emitter.instruction("ja __rt_str_to_int_fast_sign_x");                      // the first non-blank byte starts the number
    emitter.label("__rt_str_to_int_fast_space_next_x");
    emitter.instruction("add r8, 1");                                           // consume the blank byte
    emitter.instruction("sub r9, 1");                                           // account for the consumed byte
    emitter.instruction("jmp __rt_str_to_int_fast_space_x");                    // keep skipping blanks
    emitter.label("__rt_str_to_int_fast_sign_x");
    emitter.instruction("cmp ecx, 45");                                         // is the byte a minus sign?
    emitter.instruction("jne __rt_str_to_int_fast_plus_x");                     // try the plus sign instead
    emitter.instruction("mov r10d, 1");                                         // remember that the result is negative
    emitter.instruction("jmp __rt_str_to_int_fast_sign_taken_x");               // consume the sign byte
    emitter.label("__rt_str_to_int_fast_plus_x");
    emitter.instruction("cmp ecx, 43");                                         // is the byte a plus sign?
    emitter.instruction("jne __rt_str_to_int_fast_digits_x");                   // no sign means the digits start here
    emitter.label("__rt_str_to_int_fast_sign_taken_x");
    emitter.instruction("add r8, 1");                                           // consume the sign byte
    emitter.instruction("sub r9, 1");                                           // account for the consumed byte
    emitter.instruction("jz __rt_str_to_int_fast_zero_x");                      // a sign with no digits parses to zero
    emitter.label("__rt_str_to_int_fast_digits_x");
    emitter.instruction("xor r11d, r11d");                                      // unsigned accumulator
    emitter.instruction("xor edi, edi");                                        // significant digit count
    emitter.label("__rt_str_to_int_fast_loop_x");
    emitter.instruction("test r9, r9");                                         // stop at the end of the string?
    emitter.instruction("jz __rt_str_to_int_fast_finish_x");                    // every input byte has been classified
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                            // load the next input byte
    emitter.instruction("cmp ecx, 48");                                         // is the byte below ASCII '0'?
    emitter.instruction("jb __rt_str_to_int_fast_non_digit_x");                 // the digit run ended
    emitter.instruction("cmp ecx, 57");                                         // is the byte above ASCII '9'?
    emitter.instruction("ja __rt_str_to_int_fast_non_digit_x");                 // the digit run ended
    emitter.instruction("add rdi, 1");                                          // count the significant digit
    emitter.instruction("cmp rdi, 18");                                         // can the accumulator still hold this digit?
    emitter.instruction("ja __rt_str_to_int_fast_slow_x");                      // longer integer runs need saturation from the general parser
    emitter.instruction("sub ecx, 48");                                         // map the numeral onto 0..9
    emitter.instruction("imul r11, r11, 10");                                   // shift the accumulator up one decimal position
    emitter.instruction("add r11, rcx");                                        // append the decoded digit
    emitter.instruction("or r10d, 2");                                          // remember that a digit was seen
    emitter.instruction("add r8, 1");                                           // consume the digit
    emitter.instruction("sub r9, 1");                                           // account for the consumed digit
    emitter.instruction("jmp __rt_str_to_int_fast_loop_x");                     // continue scanning digits
    emitter.label("__rt_str_to_int_fast_non_digit_x");
    emitter.instruction("test r10b, 2");                                        // has any digit been seen?
    emitter.instruction("jz __rt_str_to_int_fast_finish_x");                    // no digit: the run is empty
    emitter.instruction("cmp ecx, 46");                                         // is the next byte a decimal point?
    emitter.instruction("je __rt_str_to_int_fast_slow_x");                      // fraction handling needs the general parser
    emitter.instruction("cmp ecx, 101");                                        // is the next byte a lowercase exponent?
    emitter.instruction("je __rt_str_to_int_fast_slow_x");                      // exponent handling needs the general parser
    emitter.instruction("cmp ecx, 69");                                         // is the next byte an uppercase exponent?
    emitter.instruction("je __rt_str_to_int_fast_slow_x");                      // exponent handling needs the general parser
    emitter.label("__rt_str_to_int_fast_finish_x");
    emitter.instruction("mov rax, r11");                                        // return the accumulated magnitude
    emitter.instruction("test r10b, 1");                                        // is the result negative?
    emitter.instruction("jz __rt_str_to_int_fast_ret_x");                       // a positive run is already final
    emitter.instruction("neg rax");                                             // apply the consumed minus sign
    emitter.label("__rt_str_to_int_fast_ret_x");
    emitter.instruction("ret");                                                 // return the exact integer
    emitter.label("__rt_str_to_int_fast_zero_x");
    emitter.instruction("xor eax, eax");                                        // an empty or digit-free run parses to zero
    emitter.instruction("ret");                                                 // return the zero result
    emitter.label("__rt_str_to_int_fast_slow_x");

    // -- set up the helper frame (locals: cstr=[rbp-8], ll_val=[rbp-16], end_i=[rbp-24], end_d=[rbp-32]) --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before calling libc parsers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate aligned slots for the C-string pointer, integer value, and end pointers

    // -- copy the PHP string into the C-string scratch buffer and clip it to PHP's grammar --
    emitter.instruction("call __rt_cstr");                                      // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("mov rdi, rax");                                        // pass the C-string pointer to the numeric-grammar scanner
    emitter.instruction("call __rt_php_num_scan");                              // clip the scratch to PHP's leading numeric run
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the clipped run pointer for the second parse

    // -- integer parse: strtoll(run, &end_i, 10) gives the exact, saturating 64-bit value --
    emitter.instruction("mov rdi, rax");                                        // strtoll arg1: the clipped run pointer
    emitter.instruction("lea rsi, [rbp - 24]");                                 // strtoll arg2: &end_i
    emitter.instruction("mov edx, 10");                                         // strtoll arg3: parse in base 10 like PHP string-to-int
    emitter.instruction("call strtoll");                                        // rax = integer-form value (LLONG_MAX/MIN on overflow == PHP_INT_MAX/MIN)
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the integer-form value

    // -- float parse: strtod(cstr, &end_d) detects a '.'/'e' float continuation --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the clipped run pointer for strtod
    emitter.instruction("lea rsi, [rbp - 32]");                                 // strtod arg2: &end_d
    emitter.instruction("call strtod");                                         // xmm0 = parsed double value

    // -- a value PHP cannot represent casts to 0, whichever form the string took --
    // `strtoll` saturates a 400-digit integer to PHP_INT_MAX, but PHP classifies a string whose
    // value overflows the double as IS_DOUBLE and casts INF to 0.
    emitter.instruction("movq r9, xmm0");                                       // raw IEEE-754 bit pattern of the parsed double
    emitter.instruction("add r9, r9");                                          // drop the sign bit: NaN and both infinities compare alike
    emitter.instruction("mov r10, 0xffe0000000000000");                         // twice the exponent-all-ones pattern
    emitter.instruction("cmp r9, r10");                                         // is the magnitude infinite or NaN?
    emitter.instruction("jae __rt_str_to_int_zero_linux_x86_64");               // yes: PHP casts it to 0, whichever form the string took

    // -- choose the integer value unless strtod consumed more bytes (a float part) --
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // load the end pointer returned by strtod
    emitter.instruction("cmp r8, QWORD PTR [rbp - 24]");                        // did strtod consume more bytes than strtoll?
    emitter.instruction("ja __rt_str_to_int_float_linux_x86_64");               // yes: the string is float-form, cap the double
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // no: return the exact integer-form value
    emitter.instruction("jmp __rt_str_to_int_done_linux_x86_64");               // skip the float path

    emitter.label("__rt_str_to_int_float_linux_x86_64");
    // PHP CAPS a numeric string's value rather than wrapping it, so the sibling
    // `__rt_php_float_to_int` is deliberately NOT used here -- its modulo-2^64 reduction turned
    // `(int)"1e19"` into -8446744073709551616. Both rules live in `runtime::numeric` so neither
    // is open-coded at a call site.
    emitter.instruction("call __rt_php_float_to_int_cap");                      // apply PHP's numeric-string cap
    emitter.instruction("mov rax, r11");                                        // move the capped value into the result register
    emitter.instruction("jmp __rt_str_to_int_done_linux_x86_64");               // share the epilogue

    emitter.label("__rt_str_to_int_zero_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // NaN and +-INF cast to 0

    emitter.label("__rt_str_to_int_done_linux_x86_64");
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the integer cast result
}
