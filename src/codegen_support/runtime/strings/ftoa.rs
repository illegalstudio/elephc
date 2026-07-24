//! Purpose:
//! Emits the `__rt_ftoa` runtime helper assembly for float-to-string conversion at
//! PHP's `precision` setting (default 14 significant digits) — the formatter behind
//! `echo`, `(string)`/`strval`, string interpolation, `.` concat, `print_r`, and
//! `settype($x,'string')`. Keeps PHP byte-string pointer/length behavior and
//! target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.
//! - Reproduces php-src's `zend_gcvt(value, ndigit=14, dec_point='.', exponent='E', buf)`
//!   layout without linking `zend_dtoa`: a fixed 14-significant-digit `snprintf("%.*e", 13, x)`
//!   supplies the rounded mantissa, a trailing-zero trim on that fixed digit string emulates
//!   `zend_dtoa`'s mode-2 digit suppression, and the exponential-vs-fixed threshold
//!   (`decpt > 14 || decpt < -3`) and layout mirror `Zend/zend_strtod.c::zend_gcvt` exactly
//!   (verified against the php-src source, not guessed).
//! - `var_dump`/`var_export`/`json_encode`/`serialize` use PHP's OTHER float precision
//!   (`serialize_precision = -1`, shortest round-trip) and must NOT call this helper — they
//!   route to `__rt_json_ftoa` (`crate::codegen_support::runtime::system::json_ftoa`) instead.
//! - Non-finite inputs bypass the `snprintf`/`zend_gcvt` machinery entirely: the exponent-field
//!   bit test (`0x7FF`) and sign/mantissa follow-up mirror the proven `__rt_serialize` float
//!   special-casing (`crate::codegen_support::runtime::system::serialize`), emitting PHP's
//!   `"INF"`/`"-INF"`/`"NAN"` spellings directly into `_concat_buf`.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Converts a double-precision float to a PHP-compatible byte string at PHP's
/// `precision` setting (14 significant digits): `zend_gcvt(x, 14, '.', 'E', buf)`.
///
/// # Input
/// - ARM64: `d0` holds the float value
/// - x86_64: `xmm0` holds the float value (SysV variadic ABI)
///
/// # Output
/// - ARM64: `x1` = pointer to string, `x2` = length
/// - x86_64: `rax` = pointer to string, `rdx` = length
///
/// # Behavior
/// Non-finite inputs (`INF`/`-INF`/`NAN`) are detected via the IEEE-754 exponent
/// field and written verbatim. Finite inputs are rounded to 14 significant digits
/// with `snprintf("%.*e", 13, x)`, trailing zeros are trimmed to emulate
/// `zend_dtoa`'s digit suppression, and the trimmed digit string is laid out per
/// `zend_gcvt`: fixed-decimal when `-3 <= decpt <= 14`, exponential otherwise
/// (`decpt` = the position of the decimal point relative to the first significant
/// digit). Both layouts write into the global `_concat_buf` at the current
/// `_concat_off` cursor and advance the cursor by the bytes written.
///
/// # ABI Notes
/// - Apple ARM64: variadic floats are passed on the stack, not in SIMD registers
/// - Linux x86_64: delegates to `emit_ftoa_linux_x86_64`; uses SysV variadic ABI with `eax=1` to indicate one SIMD register argument
/// - windows-x86_64: `emit_ftoa_linux_x86_64` routes every libc call through `Emitter::emit_call_c` so `snprintf`/`strtod`/`strtol` reach the registered `__rt_sys_*` msvcrt shims instead of a raw SysV-staged import call
pub fn emit_ftoa(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ftoa_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ftoa (precision=14 zend_gcvt) ---");
    emitter.label_global("__rt_ftoa");

    // -- set up stack frame (128 bytes): variadic slots, %.*e scratch, saved --
    // -- input double, and the callee-saved p_eff/neg/E registers            --
    emitter.instruction("sub sp, sp, #128");                                    // allocate the ftoa scratch frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish new frame pointer
    emitter.instruction("stp x19, x20, [sp, #96]");                             // save callee-saved registers x19 (p_eff) / x20 (neg)
    emitter.instruction("str x21, [sp, #88]");                                  // save callee-saved register x21 (exponent E)
    emitter.instruction("str d0, [sp, #80]");                                   // save the input double for re-formatting and compare

    // -- non-finite check: exponent field == 0x7FF means Inf or NaN --
    emitter.instruction("fmov x9, d0");                                         // grab the raw bit pattern of the input double
    emitter.instruction("lsr x10, x9, #52");                                    // shift the exponent field into the low bits
    emitter.instruction("and x10, x10, #0x7ff");                                // isolate the 11-bit exponent
    emitter.instruction("cmp x10, #0x7ff");                                     // is the exponent all ones (Inf/NaN)?
    emitter.instruction("b.eq __rt_ftoa_nonfinite");                            // dispatch to the INF/-INF/NAN literal writer

    // -- finite path: snprintf("%.*e", 13, x) -> 14 significant digits --
    emitter.instruction("mov x19, #13");                                        // fixed precision: 14 significant digits (1 + 13 fractional)
    emitter.instruction("add x0, sp, #16");                                     // snprintf buffer = scratch area
    emitter.instruction("mov x1, #64");                                         // scratch buffer size
    abi::emit_symbol_address(emitter, "x2", "_fmt_star_e");
    emitter.instruction("str x19, [sp, #0]");                                   // Apple variadic arg 0: int precision (on stack)
    emitter.instruction("str d0, [sp, #8]");                                    // Apple variadic arg 1: the double value (on stack)
    emitter.instruction("ldr x3, [sp, #0]");                                    // AAPCS64 variadic arg 0: int precision (in x3)
    emitter.bl_c("snprintf");                                                   // format x at 14 significant digits (double stays in d0 for AAPCS64)

    // -- parse the sign from scratch[0] --
    emitter.instruction("ldrb w9, [sp, #16]");                                  // scratch[0]
    emitter.instruction("cmp w9, #45");                                         // is the first byte a '-' sign?
    emitter.instruction("cset x20, eq");                                        // neg = 1 when the value is negative

    // -- parse the decimal exponent E: the 'e' marker sits at a fixed offset --
    // -- (neg + 1 digit + '.' + 13 frac digits) since precision is always 13 --
    emitter.instruction("add x0, sp, #16");                                     // base of scratch string
    emitter.instruction("add x0, x0, x20");                                     // + neg
    emitter.instruction("add x0, x0, #16");                                     // address of the exponent text after 'e' (sign + digits)
    emitter.instruction("mov x1, #0");                                          // strtol endptr = NULL
    emitter.instruction("mov x2, #10");                                         // base 10
    emitter.bl_c("strtol");                                                     // E = parsed decimal exponent
    emitter.instruction("mov x21, x0");                                         // keep E in a callee-saved register

    // -- trim trailing zero fractional digits to emulate zend_dtoa's mode-2 --
    // -- digit suppression; p_eff = number of significant fractional digits --
    emitter.instruction("add x14, sp, #16");                                    // base of scratch string
    emitter.instruction("add x14, x14, x20");                                   // + neg
    emitter.instruction("add x14, x14, #2");                                    // frac digit base = scratch + neg + 2
    emitter.instruction("mov x19, #13");                                        // p_eff starts at the full 13 fractional digits
    emitter.instruction("mov x13, #12");                                        // j = index of the last fractional digit (0-based)
    emitter.label("__rt_ftoa_trim");
    emitter.instruction("cmp x19, #0");                                         // nothing left to trim?
    emitter.instruction("b.eq __rt_ftoa_trim_done");                            // stop once every digit is trimmed
    emitter.instruction("ldrb w15, [x14, x13]");                                // load the fractional digit at index j
    emitter.instruction("cmp w15, #48");                                        // is it '0'?
    emitter.instruction("b.ne __rt_ftoa_trim_done");                            // stop at the first non-zero trailing digit
    emitter.instruction("sub x19, x19, #1");                                    // trim one more trailing zero
    emitter.instruction("sub x13, x13, #1");                                    // move to the previous fractional digit
    emitter.instruction("b __rt_ftoa_trim");                                    // continue trimming
    emitter.label("__rt_ftoa_trim_done");

    // -- choose decimal vs exponential layout by decimal-point position --
    // -- (zend_gcvt: exponential when decpt > ndigit(14) or decpt < -3) --
    emitter.instruction("add x9, x21, #1");                                     // decpt = E + 1
    emitter.instruction("cmn x9, #3");                                          // compare decpt against -3
    emitter.instruction("b.lt __rt_ftoa_exp");                                  // decpt < -3 -> exponential form
    emitter.instruction("cmp x9, #14");                                         // compare decpt against 14
    emitter.instruction("b.gt __rt_ftoa_exp");                                  // decpt > 14 -> exponential form

    // -- decimal form: snprintf("%.*f", max(0, p_eff - E), x) into concat_buf --
    emitter.instruction("sub x9, x19, x21");                                    // fracdigits = p_eff - E
    emitter.instruction("cmp x9, #0");                                          // is the fractional digit count negative?
    emitter.instruction("csel x9, x9, xzr, ge");                                // clamp negatives to zero (integer-valued)
    emitter.instruction("str x9, [sp, #0]");                                    // Apple variadic arg 0: fractional digit count (stack)
    abi::emit_symbol_address(emitter, "x10", "_concat_off");
    emitter.instruction("ldr x11, [x10]");                                      // current concat offset
    abi::emit_symbol_address(emitter, "x12", "_concat_buf");
    emitter.instruction("add x0, x12, x11");                                    // destination = concat_buf + offset
    emitter.instruction("mov x1, #48");                                         // destination size cap
    abi::emit_symbol_address(emitter, "x2", "_fmt_star_f");
    emitter.instruction("ldr d0, [sp, #80]");                                   // reload the input double
    emitter.instruction("str d0, [sp, #8]");                                    // Apple variadic arg 1: the double value (stack)
    emitter.instruction("ldr x3, [sp, #0]");                                    // AAPCS64 variadic arg 0: fractional digit count (x3)
    emitter.bl_c("snprintf");                                                   // format the decimal digits (double stays in d0 for AAPCS64)
    emitter.instruction("mov x2, x0");                                          // result length = bytes written
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // original offset (unchanged by snprintf)
    abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x1, x11, x10");                                    // result pointer = concat_buf + offset
    emitter.instruction("add x10, x10, x2");                                    // advance the cursor past the digits
    emitter.instruction("str x10, [x9]");                                       // publish the new concat offset
    emitter.instruction("b __rt_ftoa_done");                                    // finished decimal layout

    // -- exponential form: d.dddd...E[+-]exp with the leftover p_eff digits --
    emitter.label("__rt_ftoa_exp");
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // current concat offset
    abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // cursor = concat_buf + offset
    emitter.instruction("mov x1, x12");                                         // remember the result start pointer
    emitter.instruction("cbz x20, __rt_ftoa_exp_first");                        // skip sign when the value is non-negative
    emitter.instruction("mov w13, #45");                                        // ASCII '-'
    emitter.instruction("strb w13, [x12], #1");                                 // emit the sign and advance the cursor
    emitter.label("__rt_ftoa_exp_first");
    emitter.instruction("add x13, sp, #16");                                    // base of scratch string
    emitter.instruction("ldrb w14, [x13, x20]");                                // first significant digit (after optional sign)
    emitter.instruction("strb w14, [x12], #1");                                 // emit the leading digit
    emitter.instruction("mov w13, #46");                                        // ASCII '.'
    emitter.instruction("strb w13, [x12], #1");                                 // emit the decimal point
    emitter.instruction("cbnz x19, __rt_ftoa_exp_frac");                        // p_eff>0 copies the fractional digits
    emitter.instruction("mov w13, #48");                                        // ASCII '0' (mantissa needs one fraction digit)
    emitter.instruction("strb w13, [x12], #1");                                 // emit "0" so the mantissa reads "d.0"
    emitter.instruction("b __rt_ftoa_exp_e");                                   // continue with the exponent marker
    emitter.label("__rt_ftoa_exp_frac");
    emitter.instruction("add x13, sp, #16");                                    // base of scratch string
    emitter.instruction("add x13, x13, x20");                                   // skip optional sign
    emitter.instruction("add x13, x13, #2");                                    // skip the leading digit and '.'
    emitter.instruction("mov x14, #0");                                         // fractional copy index
    emitter.label("__rt_ftoa_exp_frac_loop");
    emitter.instruction("cmp x14, x19");                                        // copied all p_eff fractional digits?
    emitter.instruction("b.ge __rt_ftoa_exp_e");                                // mantissa fraction complete
    emitter.instruction("ldrb w15, [x13, x14]");                                // load fractional digit
    emitter.instruction("strb w15, [x12], #1");                                 // emit fractional digit
    emitter.instruction("add x14, x14, #1");                                    // advance the fractional index
    emitter.instruction("b __rt_ftoa_exp_frac_loop");                           // copy the next fractional digit
    emitter.label("__rt_ftoa_exp_e");
    emitter.instruction("mov w13, #69");                                        // ASCII 'E' (precision-14 layout is always uppercase)
    emitter.instruction("strb w13, [x12], #1");                                 // emit the exponent marker
    emitter.instruction("cmp x21, #0");                                         // is the exponent negative?
    emitter.instruction("b.ge __rt_ftoa_exp_pos");                              // positive exponent uses '+'
    emitter.instruction("mov w13, #45");                                        // ASCII '-'
    emitter.instruction("strb w13, [x12], #1");                                 // emit the exponent sign
    emitter.instruction("neg x21, x21");                                        // make the exponent magnitude positive
    emitter.instruction("b __rt_ftoa_exp_mag");                                 // emit the magnitude digits
    emitter.label("__rt_ftoa_exp_pos");
    emitter.instruction("mov w13, #43");                                        // ASCII '+'
    emitter.instruction("strb w13, [x12], #1");                                 // emit the exponent sign
    emitter.label("__rt_ftoa_exp_mag");
    emitter.instruction("mov x13, #100");                                       // divisor for the hundreds digit
    emitter.instruction("udiv x14, x21, x13");                                  // hundreds = E / 100
    emitter.instruction("msub x15, x14, x13, x21");                             // remainder = E - hundreds*100
    emitter.instruction("mov x13, #10");                                        // divisor for the tens digit
    emitter.instruction("udiv x16, x15, x13");                                  // tens = remainder / 10
    emitter.instruction("msub x17, x16, x13, x15");                             // ones = remainder - tens*10
    emitter.instruction("cbz x14, __rt_ftoa_exp_tens");                         // skip hundreds when it is zero
    emitter.instruction("add w13, w14, #48");                                   // hundreds digit to ASCII
    emitter.instruction("strb w13, [x12], #1");                                 // emit hundreds digit
    emitter.instruction("add w13, w16, #48");                                   // tens digit to ASCII
    emitter.instruction("strb w13, [x12], #1");                                 // emit tens digit
    emitter.instruction("b __rt_ftoa_exp_ones");                                // emit the ones digit
    emitter.label("__rt_ftoa_exp_tens");
    emitter.instruction("cbz x16, __rt_ftoa_exp_ones");                         // skip tens when it (and hundreds) are zero
    emitter.instruction("add w13, w16, #48");                                   // tens digit to ASCII
    emitter.instruction("strb w13, [x12], #1");                                 // emit tens digit
    emitter.label("__rt_ftoa_exp_ones");
    emitter.instruction("add w13, w17, #48");                                   // ones digit to ASCII
    emitter.instruction("strb w13, [x12], #1");                                 // emit ones digit
    emitter.instruction("sub x2, x12, x1");                                     // result length = cursor - start
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // original concat offset
    emitter.instruction("add x10, x10, x2");                                    // advance past the emitted bytes
    emitter.instruction("str x10, [x9]");                                       // publish the new concat offset
    emitter.instruction("b __rt_ftoa_done");                                    // finished exponential layout

    // -- non-finite: INF / -INF / NAN, matching __rt_serialize's classification --
    emitter.label("__rt_ftoa_nonfinite");
    emitter.instruction("lsl x10, x9, #12");                                    // drop the sign+exponent to test the mantissa
    emitter.instruction("cbnz x10, __rt_ftoa_nan");                             // non-zero mantissa means NaN
    emitter.instruction("tbnz x9, #63, __rt_ftoa_neginf");                      // negative sign means -INF
    emit_ftoa_literal_aarch64(emitter, b"INF");                                 // +INF: "INF"
    emitter.instruction("b __rt_ftoa_done");                                    // return after emitting positive infinity
    emitter.label("__rt_ftoa_neginf");
    emit_ftoa_literal_aarch64(emitter, b"-INF");                                // -INF: "-INF"
    emitter.instruction("b __rt_ftoa_done");                                    // return after emitting negative infinity
    emitter.label("__rt_ftoa_nan");
    emit_ftoa_literal_aarch64(emitter, b"NAN");                                 // NaN never carries a sign in PHP output
    emitter.instruction("b __rt_ftoa_done");                                    // return after emitting the canonical NaN spelling

    // -- restore frame and return --
    emitter.label("__rt_ftoa_done");
    emitter.instruction("ldr x21, [sp, #88]");                                  // restore callee-saved register x21
    emitter.instruction("ldp x19, x20, [sp, #96]");                             // restore callee-saved registers x19/x20
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Writes a short non-finite literal (`"INF"`, `"-INF"`, or `"NAN"`) into
/// `_concat_buf` at the current `_concat_off`, publishes the advanced offset, and
/// leaves the `__rt_ftoa` AArch64 result (`x1` = pointer, `x2` = length) ready for
/// the caller to branch into the shared `__rt_ftoa_done` epilogue.
fn emit_ftoa_literal_aarch64(emitter: &mut Emitter, bytes: &[u8]) {
    abi::emit_symbol_address(emitter, "x9", "_concat_off");                     // resolve the concat-buffer cursor symbol
    emitter.instruction("ldr x10, [x9]");                                       // load the current write offset
    abi::emit_symbol_address(emitter, "x11", "_concat_buf");                    // resolve the concat-buffer base symbol
    emitter.instruction("add x1, x11, x10");                                    // result pointer = concat_buf + offset
    for (i, byte) in bytes.iter().enumerate() {
        emitter.instruction(&format!("mov w12, #{}", byte));                    // load one literal byte
        emitter.instruction(&format!("strb w12, [x1, #{}]", i));                // store the literal byte
    }
    emitter.instruction(&format!("mov x2, #{}", bytes.len()));                  // result length = literal byte count
    emitter.instruction(&format!("add x10, x10, #{}", bytes.len()));            // advance the offset past the literal
    emitter.instruction("str x10, [x9]");                                       // publish the advanced offset
}

/// Emits the `__rt_ftoa` routine for x86_64 (Linux, macOS, and windows-x86_64).
///
/// # Input
/// - `xmm0` holds the float value (SysV variadic ABI)
///
/// # Output
/// - `rax` = pointer to formatted string, `rdx` = length
///
/// # Behavior
/// Mirrors [`emit_ftoa`]'s AArch64 body exactly: an exponent-field bit test routes
/// non-finite inputs to a literal `"INF"`/`"-INF"`/`"NAN"` writer; finite inputs are
/// rounded to 14 significant digits with `snprintf("%.*e", 13, x)`, trimmed of
/// trailing zeros to emulate `zend_dtoa`'s mode-2 suppression, and laid out per
/// `zend_gcvt` (fixed-decimal for `-3 <= decpt <= 14`, exponential otherwise).
/// Every libc call is routed through [`Emitter::emit_call_c`] rather than `bl_c` so
/// windows-x86_64 reaches the registered `__rt_sys_snprintf`/`__rt_sys_strtol`
/// msvcrt shims instead of raw SysV-staged imports.
fn emit_ftoa_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ftoa (precision=14 zend_gcvt) ---");
    emitter.label_global("__rt_ftoa");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before using stack locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the formatting helper
    emitter.instruction("push rbx");                                            // save callee-saved rbx (p_eff)
    emitter.instruction("push r12");                                            // save callee-saved r12 (sign flag)
    emitter.instruction("push r13");                                            // save callee-saved r13 (exponent E)
    emitter.instruction("push r14");                                            // save callee-saved r14 (exponential result start pointer)
    emitter.instruction("sub rsp, 80");                                         // reserve scratch buffer + saved-double slot
    emitter.instruction("movsd QWORD PTR [rsp + 64], xmm0");                    // save the input double for re-formatting and compare

    // -- non-finite check: exponent field == 0x7FF means Inf or NaN --
    emitter.instruction("movq r9, xmm0");                                       // grab the raw bit pattern of the input double
    emitter.instruction("mov r10, r9");                                         // copy the bits for exponent extraction
    emitter.instruction("shr r10, 52");                                         // shift the exponent field into the low bits
    emitter.instruction("and r10, 0x7ff");                                      // isolate the 11-bit exponent
    emitter.instruction("cmp r10, 0x7ff");                                      // is the exponent all ones (Inf/NaN)?
    emitter.instruction("je __rt_ftoa_nonfinite_x");                            // dispatch to the INF/-INF/NAN literal writer

    // -- finite path: snprintf("%.*e", 13, x) -> 14 significant digits --
    emitter.instruction("lea rdi, [rsp]");                                      // snprintf buffer = scratch area
    emitter.instruction("mov esi, 64");                                         // scratch buffer size
    abi::emit_symbol_address(emitter, "rdx", "_fmt_star_e");
    emitter.instruction("mov ecx, 13");                                         // fixed precision: 14 significant digits (1 + 13 fractional)
    emitter.instruction("movsd xmm0, QWORD PTR [rsp + 64]");                    // reload the input double for the variadic call
    emitter.instruction("mov eax, 1");                                          // one vector register used by the variadic call
    emitter.emit_call_c("snprintf");                                            // format x at 14 significant digits into scratch

    // -- parse the sign from scratch[0] --
    emitter.instruction("movzx eax, BYTE PTR [rsp]");                           // scratch[0]
    emitter.instruction("cmp al, 45");                                          // is the first byte a '-' sign?
    emitter.instruction("sete r12b");                                           // neg = 1 when the value is negative
    emitter.instruction("movzx r12d, r12b");                                    // zero-extend the sign flag

    // -- parse the decimal exponent E: the 'e' marker sits at a fixed offset --
    // -- (neg + 1 digit + '.' + 13 frac digits) since precision is always 13 --
    emitter.instruction("lea rdi, [rsp + r12 + 16]");                           // address of the exponent text after 'e' (sign + digits)
    emitter.instruction("xor esi, esi");                                        // strtol endptr = NULL
    emitter.instruction("mov edx, 10");                                         // base 10
    emitter.emit_call_c("strtol");                                              // E = parsed decimal exponent
    emitter.instruction("mov r13, rax");                                        // keep E in a callee-saved register

    // -- trim trailing zero fractional digits to emulate zend_dtoa's mode-2 --
    // -- digit suppression; p_eff = number of significant fractional digits --
    emitter.instruction("mov ebx, 13");                                         // p_eff starts at the full 13 fractional digits
    emitter.instruction("mov ecx, 12");                                         // j = index of the last fractional digit (0-based)
    emitter.instruction("lea rsi, [rsp + r12 + 2]");                            // frac digit base = scratch + neg + 2
    emitter.label("__rt_ftoa_trim_x");
    emitter.instruction("test rbx, rbx");                                       // nothing left to trim?
    emitter.instruction("jz __rt_ftoa_trim_done_x");                            // stop once every digit is trimmed
    emitter.instruction("movzx eax, BYTE PTR [rsi + rcx]");                     // load the fractional digit at index j
    emitter.instruction("cmp al, 48");                                          // is it '0'?
    emitter.instruction("jne __rt_ftoa_trim_done_x");                           // stop at the first non-zero trailing digit
    emitter.instruction("dec rbx");                                             // trim one more trailing zero
    emitter.instruction("dec rcx");                                             // move to the previous fractional digit
    emitter.instruction("jmp __rt_ftoa_trim_x");                                // continue trimming
    emitter.label("__rt_ftoa_trim_done_x");

    // -- choose decimal vs exponential layout by decimal-point position --
    // -- (zend_gcvt: exponential when decpt > ndigit(14) or decpt < -3) --
    emitter.instruction("lea rax, [r13 + 1]");                                  // decpt = E + 1
    emitter.instruction("cmp rax, -3");                                         // compare decpt against -3
    emitter.instruction("jl __rt_ftoa_exp_x");                                  // decpt < -3 -> exponential form
    emitter.instruction("cmp rax, 14");                                         // compare decpt against 14
    emitter.instruction("jg __rt_ftoa_exp_x");                                  // decpt > 14 -> exponential form

    // -- decimal form: snprintf("%.*f", max(0, p_eff - E), x) into concat_buf --
    emitter.instruction("mov rcx, rbx");                                        // fracdigits = p_eff ...
    emitter.instruction("sub rcx, r13");                                        // ... minus E
    emitter.instruction("test rcx, rcx");                                       // is the fractional digit count negative?
    emitter.instruction("jns __rt_ftoa_frac_ok_x");                             // non-negative count is fine
    emitter.instruction("xor ecx, ecx");                                        // clamp to zero (integer-valued)
    emitter.label("__rt_ftoa_frac_ok_x");
    abi::emit_load_symbol_to_reg(emitter, "r8", "_concat_off", 0);              // current concat offset
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction("lea rdi, [r9 + r8]");                                  // destination = concat_buf + offset
    emitter.instruction("mov esi, 48");                                         // destination size cap
    abi::emit_symbol_address(emitter, "rdx", "_fmt_star_f");
    emitter.instruction("movsd xmm0, QWORD PTR [rsp + 64]");                    // reload the input double for the variadic call
    emitter.instruction("mov eax, 1");                                          // one vector register used by the variadic call
    emitter.emit_call_c("snprintf");                                            // format the decimal digits into concat_buf
    emitter.instruction("mov rdx, rax");                                        // result length = bytes written
    abi::emit_load_symbol_to_reg(emitter, "r8", "_concat_off", 0);              // original offset (unchanged by snprintf)
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction("lea rax, [r9 + r8]");                                  // result pointer = concat_buf + offset
    emitter.instruction("add r8, rdx");                                         // advance the cursor past the digits
    abi::emit_store_reg_to_symbol(emitter, "r8", "_concat_off", 0);             // publish the new concat offset
    emitter.instruction("jmp __rt_ftoa_done_x");                                // finished decimal layout

    // -- exponential form: d.dddd...E[+-]exp with the leftover p_eff digits --
    emitter.label("__rt_ftoa_exp_x");
    abi::emit_load_symbol_to_reg(emitter, "r8", "_concat_off", 0);              // current concat offset
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction("lea r10, [r9 + r8]");                                  // cursor = concat_buf + offset
    emitter.instruction("mov r14, r10");                                        // remember the result start pointer
    emitter.instruction("test r12, r12");                                       // is the value negative?
    emitter.instruction("jz __rt_ftoa_exp_first_x");                            // skip sign when non-negative
    emitter.instruction("mov BYTE PTR [r10], 45");                              // emit '-'
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.label("__rt_ftoa_exp_first_x");
    emitter.instruction("movzx ecx, BYTE PTR [rsp + r12]");                     // first significant digit (after optional sign)
    emitter.instruction("mov BYTE PTR [r10], cl");                              // emit the leading digit
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.instruction("mov BYTE PTR [r10], 46");                              // emit '.'
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.instruction("test rbx, rbx");                                       // does the mantissa have fractional digits?
    emitter.instruction("jnz __rt_ftoa_exp_frac_x");                            // p_eff>0 copies the fractional digits
    emitter.instruction("mov BYTE PTR [r10], 48");                              // emit '0' (mantissa "d.0")
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.instruction("jmp __rt_ftoa_exp_e_x");                               // continue with the exponent marker
    emitter.label("__rt_ftoa_exp_frac_x");
    emitter.instruction("lea rsi, [rsp + r12 + 2]");                            // &scratch[neg+2] = first fractional digit
    emitter.instruction("xor edi, edi");                                        // fractional copy index
    emitter.label("__rt_ftoa_exp_frac_loop_x");
    emitter.instruction("cmp rdi, rbx");                                        // copied all p_eff fractional digits?
    emitter.instruction("jge __rt_ftoa_exp_e_x");                               // mantissa fraction complete
    emitter.instruction("movzx ecx, BYTE PTR [rsi + rdi]");                     // load fractional digit
    emitter.instruction("mov BYTE PTR [r10], cl");                              // emit fractional digit
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.instruction("inc rdi");                                             // advance the fractional index
    emitter.instruction("jmp __rt_ftoa_exp_frac_loop_x");                       // copy the next fractional digit
    emitter.label("__rt_ftoa_exp_e_x");
    emitter.instruction("mov BYTE PTR [r10], 69");                              // ASCII 'E' (precision-14 layout is always uppercase)
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.instruction("test r13, r13");                                       // is the exponent negative?
    emitter.instruction("jns __rt_ftoa_exp_pos_x");                             // non-negative exponent uses '+'
    emitter.instruction("mov BYTE PTR [r10], 45");                              // emit '-'
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.instruction("neg r13");                                             // make the exponent magnitude positive
    emitter.instruction("jmp __rt_ftoa_exp_mag_x");                             // emit the magnitude digits
    emitter.label("__rt_ftoa_exp_pos_x");
    emitter.instruction("mov BYTE PTR [r10], 43");                              // emit '+'
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.label("__rt_ftoa_exp_mag_x");
    emitter.instruction("mov rax, r13");                                        // exponent magnitude into the dividend
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half
    emitter.instruction("mov ecx, 100");                                        // divisor for the hundreds digit
    emitter.instruction("div rcx");                                             // rax = E/100 (hundreds), rdx = E%100
    emitter.instruction("mov r8, rax");                                         // save the hundreds digit
    emitter.instruction("mov rax, rdx");                                        // remainder (E % 100) into the dividend
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half
    emitter.instruction("mov ecx, 10");                                         // divisor for the tens digit
    emitter.instruction("div rcx");                                             // rax = tens, rdx = ones
    emitter.instruction("mov r9, rax");                                         // save the tens digit
    emitter.instruction("mov r11, rdx");                                        // save the ones digit
    emitter.instruction("test r8, r8");                                         // is the hundreds digit zero?
    emitter.instruction("jz __rt_ftoa_exp_tens_x");                             // skip leading-zero hundreds
    emitter.instruction("lea ecx, [r8 + 48]");                                  // hundreds digit to ASCII
    emitter.instruction("mov BYTE PTR [r10], cl");                              // emit hundreds digit
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.instruction("lea ecx, [r9 + 48]");                                  // tens digit to ASCII
    emitter.instruction("mov BYTE PTR [r10], cl");                              // emit tens digit
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.instruction("jmp __rt_ftoa_exp_ones_x");                            // emit the ones digit
    emitter.label("__rt_ftoa_exp_tens_x");
    emitter.instruction("test r9, r9");                                         // is the tens digit zero?
    emitter.instruction("jz __rt_ftoa_exp_ones_x");                             // skip leading-zero tens
    emitter.instruction("lea ecx, [r9 + 48]");                                  // tens digit to ASCII
    emitter.instruction("mov BYTE PTR [r10], cl");                              // emit tens digit
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.label("__rt_ftoa_exp_ones_x");
    emitter.instruction("lea ecx, [r11 + 48]");                                 // ones digit to ASCII
    emitter.instruction("mov BYTE PTR [r10], cl");                              // emit ones digit
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.instruction("mov rax, r14");                                        // result pointer = start
    emitter.instruction("mov rdx, r10");                                        // cursor (one past the last byte)
    emitter.instruction("sub rdx, rax");                                        // result length = cursor - start
    abi::emit_load_symbol_to_reg(emitter, "r8", "_concat_off", 0);              // original concat offset
    emitter.instruction("add r8, rdx");                                         // advance past the emitted bytes
    abi::emit_store_reg_to_symbol(emitter, "r8", "_concat_off", 0);             // publish the new concat offset
    emitter.instruction("jmp __rt_ftoa_done_x");                                // finished exponential layout

    // -- non-finite: INF / -INF / NAN, matching __rt_serialize's classification --
    emitter.label("__rt_ftoa_nonfinite_x");
    emitter.instruction("mov r10, r9");                                         // copy the bits for mantissa testing
    emitter.instruction("shl r10, 12");                                         // drop the sign+exponent to test the mantissa
    emitter.instruction("jnz __rt_ftoa_nan_x");                                 // non-zero mantissa means NaN
    emitter.instruction("bt r9, 63");                                           // test the float sign bit
    emitter.instruction("jc __rt_ftoa_neginf_x");                               // negative sign means -INF
    emit_ftoa_literal_x86_64(emitter, b"INF");                                  // +INF: "INF"
    emitter.instruction("jmp __rt_ftoa_done_x");                                // return after emitting positive infinity
    emitter.label("__rt_ftoa_neginf_x");
    emit_ftoa_literal_x86_64(emitter, b"-INF");                                 // -INF: "-INF"
    emitter.instruction("jmp __rt_ftoa_done_x");                                // return after emitting negative infinity
    emitter.label("__rt_ftoa_nan_x");
    emit_ftoa_literal_x86_64(emitter, b"NAN");                                  // NaN never carries a sign in PHP output
    emitter.instruction("jmp __rt_ftoa_done_x");                                // return after emitting the canonical NaN spelling

    emitter.label("__rt_ftoa_done_x");
    emitter.instruction("add rsp, 80");                                         // release the local scratch area before returning
    emitter.instruction("pop r14");                                             // restore callee-saved r14
    emitter.instruction("pop r13");                                             // restore callee-saved r13
    emitter.instruction("pop r12");                                             // restore callee-saved r12
    emitter.instruction("pop rbx");                                             // restore callee-saved rbx
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return pointer+length in rax/rdx
}

/// Writes a short non-finite literal (`"INF"`, `"-INF"`, or `"NAN"`) into
/// `_concat_buf` at the current `_concat_off`, publishes the advanced offset, and
/// leaves the `__rt_ftoa` x86_64 result (`rax` = pointer, `rdx` = length) ready
/// for the caller to branch into the shared `__rt_ftoa_done_x` epilogue.
fn emit_ftoa_literal_x86_64(emitter: &mut Emitter, bytes: &[u8]) {
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // load the current write offset
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");                     // resolve the concat-buffer base symbol
    emitter.instruction("lea rax, [r9 + r10]");                                 // result pointer = concat_buf + offset
    for (i, byte) in bytes.iter().enumerate() {
        emitter.instruction(&format!("mov BYTE PTR [rax + {}], {}", i, byte));  // store one literal byte
    }
    emitter.instruction(&format!("mov rdx, {}", bytes.len()));                  // result length = literal byte count
    emitter.instruction(&format!("add r10, {}", bytes.len()));                  // advance the offset past the literal
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);            // publish the advanced offset
}

/// Converts a double-precision float to a PHP-compatible byte string at PHP's
/// `serialize_precision = -1` (shortest round-trip): the precision `var_dump`
/// uses, shared with `json_encode`/`serialize`/`var_export` — NOT the
/// `precision = 14` setting [`emit_ftoa`] implements for `echo`/`(string)`.
///
/// # Input
/// - ARM64: `d0` holds the float value
/// - x86_64: `xmm0` holds the float value (SysV variadic ABI)
///
/// # Output
/// - ARM64: `x1` = pointer to string, `x2` = length
/// - x86_64: `rax` = pointer to string, `rdx` = length
///
/// # Behavior
/// `__rt_json_ftoa` assumes its caller has already excluded non-finite values
/// (true of its other two callers, `__rt_json_encode_float` and
/// `__rt_serialize`'s float case, which both branch around it for Inf/NaN); a
/// bare `var_dump($inf)` has no such caller, so this wrapper repeats the
/// exponent-field bit test from [`emit_ftoa`]/`__rt_serialize` to emit PHP's
/// `"INF"`/`"-INF"`/`"NAN"` spellings directly, and tail-calls into
/// `__rt_json_ftoa` (uppercase `'E'` marker, matching `serialize`'s layout)
/// for every finite input — no stack frame of its own is needed since it never
/// has cleanup to run after the delegated call returns.
pub fn emit_var_dump_ftoa(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_ftoa_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_ftoa (serialize_precision=-1, Inf/NaN-safe) ---");
    emitter.label_global("__rt_var_dump_ftoa");

    // -- non-finite check: exponent field == 0x7FF means Inf or NaN --
    emitter.instruction("fmov x9, d0");                                         // grab the raw bit pattern of the input double
    emitter.instruction("lsr x10, x9, #52");                                    // shift the exponent field into the low bits
    emitter.instruction("and x10, x10, #0x7ff");                                // isolate the 11-bit exponent
    emitter.instruction("cmp x10, #0x7ff");                                     // is the exponent all ones (Inf/NaN)?
    emitter.instruction("b.ne __rt_var_dump_ftoa_finite");                      // finite values delegate to __rt_json_ftoa
    emitter.instruction("lsl x10, x9, #12");                                    // drop the sign+exponent to test the mantissa
    emitter.instruction("cbnz x10, __rt_var_dump_ftoa_nan");                    // non-zero mantissa means NaN
    emitter.instruction("tbnz x9, #63, __rt_var_dump_ftoa_neginf");             // negative sign means -INF
    emit_ftoa_literal_aarch64(emitter, b"INF");                                 // +INF: "INF"
    emitter.instruction("ret");                                                 // return after emitting positive infinity
    emitter.label("__rt_var_dump_ftoa_neginf");
    emit_ftoa_literal_aarch64(emitter, b"-INF");                                // -INF: "-INF"
    emitter.instruction("ret");                                                 // return after emitting negative infinity
    emitter.label("__rt_var_dump_ftoa_nan");
    emit_ftoa_literal_aarch64(emitter, b"NAN");                                 // NaN never carries a sign in PHP output
    emitter.instruction("ret");                                                 // return after emitting the canonical NaN spelling

    // -- finite: tail-call __rt_json_ftoa with the 'E' (serialize) marker --
    emitter.label("__rt_var_dump_ftoa_finite");
    emitter.instruction("mov w0, #69");                                         // exponent marker 'E' (serialize_precision=-1 layout)
    emitter.instruction("b __rt_json_ftoa");                                    // tail call: its ret returns straight to our caller
}

/// Emits the x86_64 variant of `__rt_var_dump_ftoa` (Linux, macOS, and windows-x86_64).
/// Mirrors [`emit_var_dump_ftoa`]'s AArch64 body exactly; see that function's doc
/// comment for the Inf/NaN rationale.
fn emit_var_dump_ftoa_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_ftoa (serialize_precision=-1, Inf/NaN-safe) ---");
    emitter.label_global("__rt_var_dump_ftoa");

    // -- non-finite check: exponent field == 0x7FF means Inf or NaN --
    emitter.instruction("movq r9, xmm0");                                       // grab the raw bit pattern of the input double
    emitter.instruction("mov r10, r9");                                         // copy the bits for exponent extraction
    emitter.instruction("shr r10, 52");                                         // shift the exponent field into the low bits
    emitter.instruction("and r10, 0x7ff");                                      // isolate the 11-bit exponent
    emitter.instruction("cmp r10, 0x7ff");                                      // is the exponent all ones (Inf/NaN)?
    emitter.instruction("jne __rt_var_dump_ftoa_finite_x");                     // finite values delegate to __rt_json_ftoa
    emitter.instruction("mov r10, r9");                                         // copy the bits for mantissa testing
    emitter.instruction("shl r10, 12");                                         // drop the sign+exponent to test the mantissa
    emitter.instruction("jnz __rt_var_dump_ftoa_nan_x");                        // non-zero mantissa means NaN
    emitter.instruction("bt r9, 63");                                           // test the float sign bit
    emitter.instruction("jc __rt_var_dump_ftoa_neginf_x");                      // negative sign means -INF
    emit_ftoa_literal_x86_64(emitter, b"INF");                                  // +INF: "INF"
    emitter.instruction("ret");                                                 // return after emitting positive infinity
    emitter.label("__rt_var_dump_ftoa_neginf_x");
    emit_ftoa_literal_x86_64(emitter, b"-INF");                                 // -INF: "-INF"
    emitter.instruction("ret");                                                 // return after emitting negative infinity
    emitter.label("__rt_var_dump_ftoa_nan_x");
    emit_ftoa_literal_x86_64(emitter, b"NAN");                                  // NaN never carries a sign in PHP output
    emitter.instruction("ret");                                                 // return after emitting the canonical NaN spelling

    // -- finite: tail-call __rt_json_ftoa with the 'E' (serialize) marker --
    emitter.label("__rt_var_dump_ftoa_finite_x");
    emitter.instruction("mov edi, 69");                                         // exponent marker 'E' (serialize_precision=-1 layout)
    emitter.instruction("jmp __rt_json_ftoa");                                  // tail call: its ret returns straight to our caller
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies that `emit_ftoa` on Linux x86_64 uses the SysV variadic calling convention
    /// by checking that `eax` is set to 1 (one SIMD register argument) before calling `snprintf`.
    #[test]
    fn test_emit_ftoa_linux_x86_64_uses_sysv_variadic_call() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_ftoa(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_ftoa:\n"));
        assert!(asm.contains("mov eax, 1\n"));
        assert!(asm.contains("call snprintf\n"));
        assert!(asm.contains("mov rdx, rax\n"));
    }

    /// Verifies the php_gcvt exponential-vs-fixed threshold constant (`decpt > 14`)
    /// and the fixed 14-significant-digit precision are present in the AArch64 body,
    /// pinning the values verified against php-src's `zend_gcvt` (not `json_ftoa`'s
    /// `> 17` shortest-round-trip threshold).
    #[test]
    fn test_emit_ftoa_aarch64_uses_precision_14_threshold() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_ftoa(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("mov x19, #13\n"));
        assert!(asm.contains("cmp x9, #14\n"));
        assert!(asm.contains("cmn x9, #3\n"));
        assert!(asm.contains("mov w13, #69\n"));
    }

    /// Verifies non-finite inputs are classified via the exponent/mantissa/sign bit
    /// tests (mirroring `__rt_serialize`'s proven float special-casing) rather than
    /// falling through to `snprintf("%.14G", ...)`, on both architectures.
    #[test]
    fn test_emit_ftoa_detects_non_finite_on_both_arches() {
        let mut aarch64 = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_ftoa(&mut aarch64);
        let aarch64_asm = aarch64.output();
        assert!(aarch64_asm.contains("__rt_ftoa_nonfinite:\n"));
        assert!(aarch64_asm.contains("tbnz x9, #63, __rt_ftoa_neginf\n"));

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_ftoa(&mut x86);
        let x86_asm = x86.output();
        assert!(x86_asm.contains("__rt_ftoa_nonfinite_x:\n"));
        assert!(x86_asm.contains("bt r9, 63\n"));
    }
}
