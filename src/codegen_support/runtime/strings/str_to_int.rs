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
    emitter.emit_call_c("strtoll");
    emitter.instruction("str x0, [sp, #16]");                                   // save the integer-form value (LLONG_MAX/MIN on overflow == PHP_INT_MAX/MIN)

    // -- float parse: strtod(cstr, &end_d) detects a '.'/'e' float continuation --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the clipped run pointer for strtod
    emitter.instruction("add x1, sp, #8");                                      // pass &end_d so strtod reports where the numeric value ended
    emitter.emit_call_c("strtod");

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
    emitter.emit_call_c("strtoll");                                             // rax = integer-form value (LLONG_MAX/MIN on overflow == PHP_INT_MAX/MIN)
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the integer-form value

    // -- float parse: strtod(cstr, &end_d) detects a '.'/'e' float continuation --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the clipped run pointer for strtod
    emitter.instruction("lea rsi, [rbp - 32]");                                 // strtod arg2: &end_d
    emitter.emit_call_c("strtod");                                              // xmm0 = parsed double value

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
