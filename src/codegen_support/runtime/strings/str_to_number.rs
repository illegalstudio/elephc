//! Purpose:
//! Emits string numeric-detection helpers used by PHP loose comparison, `is_numeric()`,
//! the `(float)` string cast, and int-parameter coercion.
//! Converts pointer/length PHP strings through libc `strtod` after clipping them to
//! PHP's own numeric-string grammar.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The helper returns both a numeric flag and the parsed double without losing PHP byte-string bounds.
//! - `__rt_php_num_scan` runs between `__rt_cstr` and `strtod`, so libc never sees the
//!   spellings PHP's grammar rejects: hexadecimal (`"0x1A"` is `0`, not `26`), `INF` /
//!   `INFINITY` / `NAN` (all `0.0`), and underscore separators. It also owns the
//!   leading/trailing PHP-whitespace rules, so `" 42 "` is numeric while `"1e"` is not.
//! - The numeric flag is PHP's `is_numeric_string(..., allow_errors = 0)`; the parsed
//!   double is always the value of the *leading* numeric run, which is what the
//!   `(float)` cast wants even for `"12abc"`.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_str_to_number`: converts a PHP string to a double and reports whether it is numeric.
///
/// Copies the PHP string into the C-string scratch buffer via `__rt_cstr`, clips it to PHP's
/// leading numeric run with `__rt_php_num_scan`, then parses that run with libc `strtod`.
/// The parsed double is returned in d0/xmm0 (`0.0` when the string has no numeric prefix);
/// the integer result register is 1 when the whole string was numeric (PHP whitespace on
/// either side is allowed), 0 otherwise.
/// Dispatches to the x86_64-specific implementation when targeting that architecture.
pub fn emit_str_to_number(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_to_number_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_to_number ---");
    emitter.label_global("__rt_str_to_number");

    emitter.instruction("sub sp, sp, #32");                                      // allocate a helper slot for the numeric flag
    emitter.instruction("stp x29, x30, [sp, #16]");                              // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                     // establish a stable helper frame pointer
    emitter.instruction("bl __rt_cstr");                                         // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("bl __rt_php_num_scan");                                 // clip the scratch to PHP's leading numeric run
    emitter.instruction("str x1, [sp, #0]");                                     // save the fully-numeric flag across strtod
    emitter.instruction("mov x1, #0");                                           // strtod endptr = NULL: the run is already clipped
    emitter.bl_c("strtod");                                                     // parse the clipped numeric run into d0
    emitter.instruction("ldr x0, [sp, #0]");                                     // reload the fully-numeric flag as the result

    emitter.instruction("ldp x29, x30, [sp, #16]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                      // release the helper stack frame
    emitter.instruction("ret");                                                  // return the numeric flag while preserving the parsed double in d0
}

/// Emits `__rt_str_to_number` for the Linux x86_64 target. Identical logic to the ARM64 path but using
/// x86_64 calling conventions: copies the PHP string via `__rt_cstr`, clips it with
/// `__rt_php_num_scan`, then parses the clipped run with libc `strtod`. Returns 1 in rax when the
/// whole string was numeric, 0 otherwise; the parsed double is preserved in xmm0.
fn emit_str_to_number_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_to_number ---");
    emitter.label_global("__rt_str_to_number");

    emitter.instruction("push rbp");                                             // save the caller frame pointer before nested libc calls
    emitter.instruction("mov rbp, rsp");                                         // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 32");                                          // allocate an aligned helper slot for the numeric flag
    emitter.instruction("call __rt_cstr");                                       // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("mov rdi, rax");                                         // pass the C-string pointer to the numeric-grammar scanner
    emitter.instruction("call __rt_php_num_scan");                               // clip the scratch to PHP's leading numeric run
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                         // save the fully-numeric flag across strtod
    emitter.instruction("mov rdi, rax");                                         // pass the clipped numeric run as strtod's first argument
    emitter.instruction("xor esi, esi");                                         // strtod endptr = NULL: the run is already clipped
    emitter.instruction("call strtod");                                          // parse the clipped numeric run into xmm0
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                         // reload the fully-numeric flag as the result

    emitter.instruction("add rsp, 32");                                          // release the helper stack frame
    emitter.instruction("pop rbp");                                              // restore the caller frame pointer
    emitter.instruction("ret");                                                  // return the numeric flag while preserving the parsed double in xmm0
}

/// Emits a strict bounded PHP numeric-string parser for string-to-int coercion.
///
/// The parser reads only the supplied pointer/length range and accepts decimal mantissas with an
/// optional exponent plus PHP whitespace. It normalizes a bounded significant prefix and sticky
/// digit into fixed stack storage before one `strtod` call, so embedded NUL bytes and libc-only
/// hexadecimal, infinity, and NaN spellings cannot escape the grammar or shared scratch storage.
pub fn emit_str_looks_like_int_for_coercion(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_looks_like_int_for_coercion_normalized_linux_x86_64(emitter);
        return;
    }

    emit_str_looks_like_int_for_coercion_normalized_aarch64(emitter);
}

/// Emits the AArch64 bounded numeric-key parser backed by one correctly-rounded `strtod` call.
///
/// The parser scans the complete pointer/length range but writes only a normalized scientific
/// spelling into a fixed stack buffer: 768 significant digits, plus one sticky digit. A
/// binary64 rounding midpoint has at most 768 significant decimal digits, so the sticky digit is
/// sufficient to retain its side of every possible tie without copying attacker-controlled input.
fn emit_str_looks_like_int_for_coercion_normalized_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_looks_like_int_for_coercion ---");
    emitter.label_global("__rt_str_looks_like_int_for_coercion");
    emitter.instruction("sub sp, sp, #896");                                     // reserve a fixed normalized decimal buffer and preserve call alignment
    emitter.instruction("add x17, sp, #880");                                    // materialize the frame-link address beyond pair-load/store immediate range
    emitter.instruction("stp x29, x30, [x17]");                                  // save the caller frame across the single libc conversion
    emitter.instruction("add x29, sp, #880");                                    // establish the bounded parser frame pointer
    emitter.instruction("mov x9, x1");                                           // move the PHP byte pointer into the bounded scan cursor
    emitter.instruction("mov x10, x2");                                          // retain the exact remaining PHP byte count
    emitter.instruction("mov x3, sp");                                           // begin the private normalized C spelling at the stack buffer base
    emitter.instruction("mov w4, #0");                                           // clear the decimal-point-seen flag
    emitter.instruction("mov w5, #0");                                           // clear the any-mantissa-digit flag
    emitter.instruction("mov w6, #0");                                           // clear the first-nonzero-significand flag
    emitter.instruction("mov w7, #0");                                           // clear the first-significant-digit-in-integer flag
    emitter.instruction("mov x8, #0");                                           // initialize digits-after-first-integer-significant count
    emitter.instruction("mov x11, #0");                                          // initialize leading fractional-zero count
    emitter.instruction("mov x12, #0");                                          // initialize the retained significant-digit count
    emitter.instruction("mov w13, #0");                                          // clear the discarded-suffix sticky flag
    emitter.instruction("adds x0, x2, #4095");                                   // allow explicit exponents to cancel every source digit plus the binary64 margin
    emitter.instruction("b.cc __rt_sliic_normal_threshold_ready");               // retain the exact source-relative saturation threshold when it fits
    emitter.instruction("mov x0, #-1");                                          // saturate a wrapped threshold to the unsigned machine maximum
    emitter.label("__rt_sliic_normal_threshold_ready");
    emitter.instruction("str x0, [sp, #864]");                                   // preserve the exponent threshold in fixed frame-local state
    emitter.instruction("cbz x10, __rt_sliic_normal_false");                     // reject an empty bounded PHP string before dereferencing it

    emitter.label("__rt_sliic_normal_leading_ws");
    emitter.instruction("ldrb w17, [x9]");                                       // load one byte while the explicit range remains nonempty
    emitter.instruction("cmp w17, #32");                                         // accept ASCII space as PHP leading whitespace
    emitter.instruction("b.eq __rt_sliic_normal_ws_next");                       // consume an ASCII leading space
    emitter.instruction("sub w0, w17, #9");                                      // normalize tab through carriage return for the whitespace range test
    emitter.instruction("cmp w0, #4");                                           // test the remaining PHP leading-whitespace bytes
    emitter.instruction("b.ls __rt_sliic_normal_ws_next");                       // consume a control-whitespace prefix byte
    emitter.instruction("b __rt_sliic_normal_sign");                             // inspect the first non-whitespace byte for an optional sign

    emitter.label("__rt_sliic_normal_ws_next");
    emitter.instruction("add x9, x9, #1");                                       // advance inside the source range after one whitespace byte
    emitter.instruction("sub x10, x10, #1");                                     // decrease the bounded source byte count
    emitter.instruction("cbnz x10, __rt_sliic_normal_leading_ws");               // continue until the first non-whitespace byte or range end
    emitter.instruction("b __rt_sliic_normal_false");                            // whitespace alone is not a numeric string

    emitter.label("__rt_sliic_normal_sign");
    emitter.instruction("cmp w17, #43");                                         // recognize an optional explicit plus sign
    emitter.instruction("b.eq __rt_sliic_normal_consume_sign");                  // consume the positive sign without materializing it
    emitter.instruction("cmp w17, #45");                                         // recognize an optional minus sign
    emitter.instruction("b.ne __rt_sliic_normal_mantissa");                      // start mantissa scanning when no sign is present
    emitter.instruction("strb w17, [x3], #1");                                   // retain the minus sign so `strtod` preserves negative zero

    emitter.label("__rt_sliic_normal_consume_sign");
    emitter.instruction("add x9, x9, #1");                                       // advance past the accepted sign byte
    emitter.instruction("sub x10, x10, #1");                                     // account for the consumed sign byte
    emitter.instruction("cbz x10, __rt_sliic_normal_false");                     // reject a sign with no mantissa following it

    emitter.label("__rt_sliic_normal_mantissa");
    emitter.instruction("cbz x10, __rt_sliic_normal_after_mantissa");            // finish the mantissa exactly at the supplied string boundary
    emitter.instruction("ldrb w17, [x9]");                                       // read the current bounded mantissa byte
    emitter.instruction("sub w0, w17, #48");                                     // normalize a candidate decimal digit
    emitter.instruction("cmp w0, #9");                                           // test the complete ASCII decimal-digit range
    emitter.instruction("b.hi __rt_sliic_normal_mantissa_non_digit");            // route punctuation and letters to point/exponent validation
    emitter.instruction("mov w5, #1");                                           // remember that the mantissa contains at least one digit
    emitter.instruction("cbnz w6, __rt_sliic_normal_after_first");               // route subsequent significant digits to the bounded output path
    emitter.instruction("cbnz w0, __rt_sliic_normal_first_nonzero");             // retain only the first nonzero digit as the scientific significand start
    emitter.instruction("cbz w4, __rt_sliic_normal_digit_next");                 // ignore leading integer zeros because they do not affect the exponent
    emitter.instruction("add x11, x11, #1");                                     // record one zero before the first fractional significant digit
    emitter.instruction("b __rt_sliic_normal_digit_next");                       // consume the leading fractional zero

    emitter.label("__rt_sliic_normal_first_nonzero");
    emitter.instruction("mov w6, #1");                                           // mark that the normalized significand now has a first digit
    emitter.instruction("cbnz w4, __rt_sliic_normal_first_fractional");          // a consumed point makes this the first fractional significant digit
    emitter.instruction("mov w7, #1");                                           // remember that the first significant digit was in the integer portion
    emitter.instruction("b __rt_sliic_normal_write_first");                      // share output construction after the location flag is established
    emitter.label("__rt_sliic_normal_first_fractional");
    emitter.instruction("mov w7, #0");                                           // remember that leading fractional zeros determine the scientific exponent
    emitter.label("__rt_sliic_normal_write_first");
    emitter.instruction("strb w17, [x3], #1");                                   // write the first significant decimal digit to the local spelling
    emitter.instruction("mov w0, #46");                                          // materialize the decimal point between the first and later digits
    emitter.instruction("strb w0, [x3], #1");                                    // write a stable scientific mantissa separator
    emitter.instruction("mov x12, #1");                                          // count the retained first significant digit
    emitter.instruction("b __rt_sliic_normal_digit_next");                       // consume the source digit after initializing the output spelling

    emitter.label("__rt_sliic_normal_after_first");
    emitter.instruction("cbnz w4, __rt_sliic_normal_store_digit");               // fractional digits do not alter the integer-side scientific exponent
    emitter.instruction("add x8, x8, #1");                                       // count one integer digit after the first significant digit
    emitter.label("__rt_sliic_normal_store_digit");
    emitter.instruction("cmp x12, #768");                                        // retain exactly enough digits to distinguish every binary64 midpoint
    emitter.instruction("b.hs __rt_sliic_normal_sticky_digit");                  // inspect discarded digits only for their nonzero sticky contribution
    emitter.instruction("strb w17, [x3], #1");                                   // append a retained significant decimal digit to the local buffer
    emitter.instruction("add x12, x12, #1");                                     // record the bounded retained-significand length
    emitter.instruction("b __rt_sliic_normal_digit_next");                       // consume the source digit after storing it
    emitter.label("__rt_sliic_normal_sticky_digit");
    emitter.instruction("cbz w0, __rt_sliic_normal_digit_next");                 // discarded zeros cannot influence a binary64 rounding decision
    emitter.instruction("mov w13, #1");                                          // retain one sticky bit for every discarded nonzero suffix

    emitter.label("__rt_sliic_normal_digit_next");
    emitter.instruction("add x9, x9, #1");                                       // advance to the next supplied PHP byte
    emitter.instruction("sub x10, x10, #1");                                     // account for the consumed mantissa byte
    emitter.instruction("b __rt_sliic_normal_mantissa");                         // continue the bounded grammar scan

    emitter.label("__rt_sliic_normal_mantissa_non_digit");
    emitter.instruction("cmp w17, #46");                                         // recognize the single optional decimal point
    emitter.instruction("b.ne __rt_sliic_normal_after_mantissa");                // any other byte terminates the mantissa
    emitter.instruction("cbnz w4, __rt_sliic_normal_after_mantissa");            // leave a second decimal point for trailing validation to reject
    emitter.instruction("mov w4, #1");                                           // remember that later digits are fractional
    emitter.instruction("add x9, x9, #1");                                       // consume the single accepted decimal point
    emitter.instruction("sub x10, x10, #1");                                     // account for the decimal-point byte
    emitter.instruction("b __rt_sliic_normal_mantissa");                         // continue with the optional fractional digit run

    emitter.label("__rt_sliic_normal_after_mantissa");
    emitter.instruction("cbz w5, __rt_sliic_normal_false");                      // reject signs and points that contain no mantissa digit
    emitter.instruction("mov x16, #0");                                          // initialize the explicit exponent magnitude to zero
    emitter.instruction("mov w15, #0");                                          // initialize the explicit exponent sign to positive
    emitter.instruction("cbz x10, __rt_sliic_normal_finish_scan");               // a complete mantissa may end exactly at the bounded input end
    emitter.instruction("ldrb w17, [x9]");                                       // inspect the byte immediately after the mantissa
    emitter.instruction("cmp w17, #101");                                        // recognize a lowercase scientific exponent marker
    emitter.instruction("b.eq __rt_sliic_normal_exponent_start");                // consume a lowercase exponent suffix
    emitter.instruction("cmp w17, #69");                                         // recognize an uppercase scientific exponent marker
    emitter.instruction("b.eq __rt_sliic_normal_exponent_start");                // consume an uppercase exponent suffix
    emitter.instruction("b __rt_sliic_normal_trailing_ws");                      // only PHP whitespace may follow a complete numeric string

    emitter.label("__rt_sliic_normal_exponent_start");
    emitter.instruction("add x9, x9, #1");                                       // consume the exponent marker inside the bounded range
    emitter.instruction("sub x10, x10, #1");                                     // account for the exponent marker byte
    emitter.instruction("cbz x10, __rt_sliic_normal_false");                     // reject an exponent marker without exponent digits
    emitter.instruction("mov w14, #0");                                          // clear the exponent-digit-seen flag before optional sign handling
    emitter.instruction("ldrb w17, [x9]");                                       // inspect the optional exponent sign byte
    emitter.instruction("cmp w17, #43");                                         // recognize an explicit positive exponent sign
    emitter.instruction("b.eq __rt_sliic_normal_exponent_consume_sign");         // consume the explicit positive exponent sign
    emitter.instruction("cmp w17, #45");                                         // recognize an explicit negative exponent sign
    emitter.instruction("b.ne __rt_sliic_normal_exponent_digits");               // start digit parsing when the first byte is already a digit
    emitter.instruction("mov w15, #1");                                          // remember that the explicit exponent subtracts from the scientific exponent
    emitter.label("__rt_sliic_normal_exponent_consume_sign");
    emitter.instruction("add x9, x9, #1");                                       // consume the optional exponent sign
    emitter.instruction("sub x10, x10, #1");                                     // account for the exponent sign byte
    emitter.instruction("cbz x10, __rt_sliic_normal_false");                     // reject a sign without exponent digits

    emitter.label("__rt_sliic_normal_exponent_digits");
    emitter.instruction("cbz x10, __rt_sliic_normal_exponent_done");             // finish exponent parsing at the exact input boundary
    emitter.instruction("ldrb w17, [x9]");                                       // read one bounded exponent candidate byte
    emitter.instruction("sub w0, w17, #48");                                     // normalize a candidate exponent decimal digit
    emitter.instruction("cmp w0, #9");                                           // test the exponent digit range
    emitter.instruction("b.hi __rt_sliic_normal_exponent_done");                 // leave trailing whitespace for the common validator
    emitter.instruction("mov w14, #1");                                          // record that the exponent contains at least one digit
    emitter.instruction("ldr x17, [sp, #864]");                                  // load the source-relative exponent saturation threshold
    emitter.instruction("cmp x16, x17");                                         // has the accumulated exponent already reached that threshold?
    emitter.instruction("b.hs __rt_sliic_normal_exponent_next");                 // preserve the saturated magnitude while scanning the remaining bytes
    emitter.instruction("mov x1, #10");                                          // materialize the decimal radix for checked exponent accumulation
    emitter.instruction("udiv x2, x17, x1");                                     // compute the largest pre-multiply value that cannot exceed the threshold
    emitter.instruction("cmp x16, x2");                                          // would multiplying the current magnitude necessarily exceed the threshold?
    emitter.instruction("b.hi __rt_sliic_normal_exponent_saturate");             // clamp before arithmetic can overflow
    emitter.instruction("madd x16, x16, x1, x0");                                // append the next exponent digit in constant work
    emitter.instruction("cmp x16, x17");                                         // did the appended digit cross the source-relative threshold?
    emitter.instruction("b.ls __rt_sliic_normal_exponent_next");                 // retain magnitudes that remain inside the cancellation-safe interval
    emitter.label("__rt_sliic_normal_exponent_saturate");
    emitter.instruction("mov x16, x17");                                         // saturate while preserving enough magnitude for any source-length cancellation
    emitter.label("__rt_sliic_normal_exponent_next");
    emitter.instruction("add x9, x9, #1");                                       // advance past one consumed exponent digit
    emitter.instruction("sub x10, x10, #1");                                     // account for that exponent digit
    emitter.instruction("b __rt_sliic_normal_exponent_digits");                  // scan all exponent bytes without exponent-sized scale loops

    emitter.label("__rt_sliic_normal_exponent_done");
    emitter.instruction("cbz w14, __rt_sliic_normal_false");                     // reject an exponent marker that has no decimal digit

    emitter.label("__rt_sliic_normal_trailing_ws");
    emitter.instruction("cbz x10, __rt_sliic_normal_finish_scan");               // all remaining bytes were valid when the bounded count reaches zero
    emitter.instruction("ldrb w17, [x9]");                                       // inspect one trailing byte without reading beyond the PHP string
    emitter.instruction("cmp w17, #32");                                         // accept ASCII space after a numeric payload
    emitter.instruction("b.eq __rt_sliic_normal_trailing_next");                 // consume a trailing ASCII space
    emitter.instruction("sub w0, w17, #9");                                      // normalize tab through carriage return for trailing whitespace
    emitter.instruction("cmp w0, #4");                                           // test the remaining PHP trailing-whitespace bytes
    emitter.instruction("b.hi __rt_sliic_normal_false");                         // reject every non-whitespace trailing byte
    emitter.label("__rt_sliic_normal_trailing_next");
    emitter.instruction("add x9, x9, #1");                                       // advance after one accepted trailing whitespace byte
    emitter.instruction("sub x10, x10, #1");                                     // account for the consumed trailing byte
    emitter.instruction("b __rt_sliic_normal_trailing_ws");                      // validate the entire remaining bounded suffix

    emitter.label("__rt_sliic_normal_finish_scan");
    emitter.instruction("cbnz w6, __rt_sliic_normal_nonzero_value");             // zero-only mantissas do not need a scientific exponent spelling
    emitter.instruction("mov w0, #48");                                          // materialize the canonical zero digit for an all-zero mantissa
    emitter.instruction("strb w0, [x3], #1");                                    // append the zero payload after a possible preserved minus sign
    emitter.instruction("b __rt_sliic_normal_call_strtod");                      // parse the signed or unsigned zero once through libc

    emitter.label("__rt_sliic_normal_nonzero_value");
    emitter.instruction("cbz w13, __rt_sliic_normal_append_exponent");           // omit the sticky digit when every discarded digit was zero
    emitter.instruction("mov w0, #49");                                          // materialize the nonzero sticky digit after the retained prefix
    emitter.instruction("strb w0, [x3], #1");                                    // encode the discarded suffix direction beyond all binary64 midpoints
    emitter.label("__rt_sliic_normal_append_exponent");
    emitter.instruction("mov w0, #101");                                         // materialize the scientific exponent separator
    emitter.instruction("strb w0, [x3], #1");                                    // append the exponent marker after the normalized significand
    emitter.instruction("cbz w7, __rt_sliic_normal_fractional_exponent");        // fractional first digits derive the exponent from leading fractional zeros
    emitter.instruction("b __rt_sliic_normal_combine_exponent");                 // combine the explicit exponent with the integer-derived exponent
    emitter.label("__rt_sliic_normal_fractional_exponent");
    emitter.instruction("neg x8, x11");                                          // negate leading fractional zeros to form the decimal scientific exponent
    emitter.instruction("sub x8, x8, #1");                                       // account for the first nonzero digit itself in the fractional exponent
    emitter.label("__rt_sliic_normal_combine_exponent");
    emitter.instruction("cbz w15, __rt_sliic_normal_add_exponent");              // positive explicit exponents add to the normalized decimal exponent
    emitter.instruction("sub x8, x8, x16");                                      // apply a negative explicit exponent in one constant-time operation
    emitter.instruction("b __rt_sliic_normal_clamp_exponent");                   // clamp the combined exponent before decimal rendering
    emitter.label("__rt_sliic_normal_add_exponent");
    emitter.instruction("add x8, x8, x16");                                      // apply a positive explicit exponent in one constant-time operation
    emitter.label("__rt_sliic_normal_clamp_exponent");
    emitter.instruction("cmp x8, #4095");                                        // test whether the combined exponent already guarantees overflow direction
    emitter.instruction("b.le __rt_sliic_normal_clamp_low");                     // preserve representable and underflow-side exponent values
    emitter.instruction("mov x8, #4095");                                        // saturate positive exponents for a short local spelling
    emitter.label("__rt_sliic_normal_clamp_low");
    emitter.instruction("mov x14, #-4095");                                      // materialize the bounded negative exponent floor in a register
    emitter.instruction("cmp x8, x14");                                          // test whether the combined exponent already guarantees underflow direction
    emitter.instruction("b.ge __rt_sliic_normal_render_exponent");               // retain exponents inside the symmetric local rendering interval
    emitter.instruction("mov x8, #-4095");                                       // saturate negative exponents for a short local spelling
    emitter.label("__rt_sliic_normal_render_exponent");
    emitter.instruction("cmp x8, #0");                                           // decide whether the rendered exponent needs a minus sign
    emitter.instruction("b.ge __rt_sliic_normal_render_digits");                 // render nonnegative exponents directly
    emitter.instruction("mov w0, #45");                                          // materialize the exponent minus sign
    emitter.instruction("strb w0, [x3], #1");                                    // append the negative exponent sign
    emitter.instruction("neg x8, x8");                                           // convert the bounded negative magnitude to an unsigned decimal value
    emitter.label("__rt_sliic_normal_render_digits");
    emitter.instruction("add x9, sp, #848");                                     // start reverse exponent rendering inside the fixed frame-local tail buffer
    emitter.instruction("mov x10, #10");                                         // materialize the decimal divisor for exponent rendering
    emitter.label("__rt_sliic_normal_reverse_digit");
    emitter.instruction("udiv x11, x8, x10");                                    // compute the next exponent decimal quotient
    emitter.instruction("msub x12, x11, x10, x8");                               // derive the unsigned decimal remainder without division-side effects
    emitter.instruction("sub x9, x9, #1");                                       // reserve one reverse-rendered exponent byte
    emitter.instruction("add x12, x12, #48");                                    // encode the decimal remainder as ASCII
    emitter.instruction("strb w12, [x9]");                                       // store one exponent digit in reverse order
    emitter.instruction("mov x8, x11");                                          // continue with the remaining exponent quotient
    emitter.instruction("cbnz x8, __rt_sliic_normal_reverse_digit");             // render at most four digits because the exponent is saturated to 9999
    emitter.instruction("add x14, sp, #848");                                    // retain the fixed reverse-rendering tail end for the forward copy loop
    emitter.label("__rt_sliic_normal_copy_exponent");
    emitter.instruction("ldrb w0, [x9], #1");                                    // load one forward exponent digit from the reverse-rendered local tail
    emitter.instruction("strb w0, [x3], #1");                                    // append that digit to the normalized scientific spelling
    emitter.instruction("cmp x9, x14");                                          // stop after copying the exact reverse-rendered exponent suffix
    emitter.instruction("b.ne __rt_sliic_normal_copy_exponent");                 // copy every rendered exponent digit into the normalized spelling

    emitter.label("__rt_sliic_normal_call_strtod");
    emitter.instruction("strb wzr, [x3]");                                       // terminate the private normalized spelling for the single libc call
    emitter.instruction("mov x0, sp");                                           // pass the local normalized spelling as `strtod`'s first argument
    emitter.instruction("mov x1, #0");                                           // request no end-pointer because the bounded parser consumed the full grammar
    emitter.bl_c("strtod");                                                     // convert once with libc's correctly-rounded binary64 decimal parser
    emitter.instruction("mov x0, #1");                                           // report a complete PHP numeric string while preserving d0
    emitter.instruction("add x17, sp, #880");                                    // rematerialize the frame-link address after the libc call clobbered scratch state
    emitter.instruction("ldp x29, x30, [x17]");                                  // restore the caller frame after the local conversion buffer is no longer needed
    emitter.instruction("add sp, sp, #896");                                     // release the fixed parser frame without heap allocation
    emitter.instruction("ret");                                                  // return the numeric flag and parsed double

    emitter.label("__rt_sliic_normal_false");
    emitter.instruction("mov x0, #0");                                           // report a nonnumeric bounded PHP string without invoking libc
    emitter.instruction("add x17, sp, #880");                                    // materialize the frame-link address on the parser rejection path
    emitter.instruction("ldp x29, x30, [x17]");                                  // restore the caller frame on the parser rejection path
    emitter.instruction("add sp, sp, #896");                                     // release the fixed parser frame after rejection
    emitter.instruction("ret");                                                  // return failure without reading outside the supplied pointer/length range
}

/// Emits the x86_64 bounded numeric-key parser backed by one correctly-rounded `strtod` call.
///
/// This is the SysV mirror of the AArch64 state machine. It scans the source exactly once,
/// retains 768 significant digits plus one sticky digit in a fixed local buffer, and preserves
/// every callee-saved register used for parser state across the libc conversion.
fn emit_str_looks_like_int_for_coercion_normalized_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_looks_like_int_for_coercion ---");
    emitter.label_global("__rt_str_looks_like_int_for_coercion");
    emitter.instruction("push rbp");                                             // preserve the caller frame and align the SysV stack before nested calls
    emitter.instruction("mov rbp, rsp");                                         // establish a stable frame above the fixed normalization storage
    emitter.instruction("sub rsp, 896");                                         // reserve the bounded decimal buffer and callee-saved register spills
    emitter.instruction("mov QWORD PTR [rsp + 856], rbx");                       // preserve rbx before using it for fractional-zero accounting
    emitter.instruction("mov QWORD PTR [rsp + 864], r12");                       // preserve r12 before using it as the any-digit flag
    emitter.instruction("mov QWORD PTR [rsp + 872], r13");                       // preserve r13 before using it as the nonzero-digit flag
    emitter.instruction("mov QWORD PTR [rsp + 888], r15");                       // preserve r15 before using it for the scientific exponent
    emitter.instruction("mov r8, rax");                                          // move the PHP byte pointer into the bounded scan cursor
    emitter.instruction("mov r9, rdx");                                          // retain the exact remaining PHP byte count
    emitter.instruction("mov r10, rsp");                                         // begin the private normalized C spelling at the stack buffer base
    emitter.instruction("xor r11d, r11d");                                       // clear the decimal-point-seen flag
    emitter.instruction("xor r12d, r12d");                                       // clear the any-mantissa-digit flag
    emitter.instruction("xor r13d, r13d");                                       // clear the first-nonzero-significand flag
    emitter.instruction("mov QWORD PTR [rsp + 880], 0");                         // clear the first-significant-digit-in-integer flag (its own slot: r14 is the reserved ctx register, and 840 belongs to the exponent threshold)
    emitter.instruction("xor r15d, r15d");                                       // initialize digits-after-first-integer-significant count
    emitter.instruction("xor ebx, ebx");                                         // initialize leading fractional-zero count
    emitter.instruction("xor ecx, ecx");                                         // initialize the retained significant-digit count
    emitter.instruction("mov QWORD PTR [rsp + 848], 0");                        // clear the discarded-suffix sticky flag in a private state slot
    emitter.instruction("mov rax, r9");                                          // seed the cancellation-safe exponent threshold from the source length
    emitter.instruction("add rax, 4095");                                        // include the complete binary64 overflow and underflow margin
    emitter.instruction("jnc __rt_sliic_normal_threshold_ready_x");              // retain the exact source-relative threshold when it fits
    emitter.instruction("mov rax, -1");                                          // saturate a wrapped threshold to the unsigned machine maximum
    emitter.label("__rt_sliic_normal_threshold_ready_x");
    emitter.instruction("mov QWORD PTR [rsp + 840], rax");                       // preserve the threshold in fixed frame-local state
    emitter.instruction("test r9, r9");                                          // reject an empty bounded PHP string before dereferencing it
    emitter.instruction("jz __rt_sliic_normal_false_x");                         // an empty range cannot contain a numeric payload

    emitter.label("__rt_sliic_normal_leading_ws_x");
    emitter.instruction("movzx eax, BYTE PTR [r8]");                             // load one byte while the explicit range remains nonempty
    emitter.instruction("cmp eax, 32");                                          // accept ASCII space as PHP leading whitespace
    emitter.instruction("je __rt_sliic_normal_ws_next_x");                       // consume an ASCII leading space
    emitter.instruction("mov edx, eax");                                         // preserve the source byte while normalizing control whitespace
    emitter.instruction("sub edx, 9");                                           // normalize tab through carriage return for the whitespace range test
    emitter.instruction("cmp edx, 4");                                           // test the remaining PHP leading-whitespace bytes
    emitter.instruction("jbe __rt_sliic_normal_ws_next_x");                      // consume a control-whitespace prefix byte
    emitter.instruction("jmp __rt_sliic_normal_sign_x");                         // inspect the first non-whitespace byte for an optional sign

    emitter.label("__rt_sliic_normal_ws_next_x");
    emitter.instruction("add r8, 1");                                            // advance inside the source range after one whitespace byte
    emitter.instruction("sub r9, 1");                                            // decrease the bounded source byte count
    emitter.instruction("jnz __rt_sliic_normal_leading_ws_x");                   // continue until the first non-whitespace byte or range end
    emitter.instruction("jmp __rt_sliic_normal_false_x");                        // whitespace alone is not a numeric string

    emitter.label("__rt_sliic_normal_sign_x");
    emitter.instruction("cmp eax, 43");                                          // recognize an optional explicit plus sign
    emitter.instruction("je __rt_sliic_normal_consume_sign_x");                  // consume the positive sign without materializing it
    emitter.instruction("cmp eax, 45");                                          // recognize an optional minus sign
    emitter.instruction("jne __rt_sliic_normal_mantissa_x");                     // start mantissa scanning when no sign is present
    emitter.instruction("mov BYTE PTR [r10], al");                               // retain the minus sign so `strtod` preserves negative zero
    emitter.instruction("add r10, 1");                                           // advance the normalized output after the retained sign

    emitter.label("__rt_sliic_normal_consume_sign_x");
    emitter.instruction("add r8, 1");                                            // advance past the accepted sign byte
    emitter.instruction("sub r9, 1");                                            // account for the consumed sign byte
    emitter.instruction("jz __rt_sliic_normal_false_x");                         // reject a sign with no mantissa following it

    emitter.label("__rt_sliic_normal_mantissa_x");
    emitter.instruction("test r9, r9");                                          // finish the mantissa exactly at the supplied string boundary
    emitter.instruction("jz __rt_sliic_normal_after_mantissa_x");                // no bounded byte remains in the mantissa
    emitter.instruction("movzx eax, BYTE PTR [r8]");                             // read the current bounded mantissa byte
    emitter.instruction("mov edx, eax");                                         // copy the byte before decimal-digit normalization
    emitter.instruction("sub edx, 48");                                          // normalize a candidate decimal digit
    emitter.instruction("cmp edx, 9");                                           // test the complete ASCII decimal-digit range
    emitter.instruction("ja __rt_sliic_normal_mantissa_non_digit_x");            // route punctuation and letters to point/exponent validation
    emitter.instruction("mov r12d, 1");                                          // remember that the mantissa contains at least one digit
    emitter.instruction("test r13d, r13d");                                      // has the first nonzero significand digit already been found?
    emitter.instruction("jnz __rt_sliic_normal_after_first_x");                  // route subsequent significant digits to the bounded output path
    emitter.instruction("test edx, edx");                                        // does this candidate begin the nonzero significand?
    emitter.instruction("jnz __rt_sliic_normal_first_nonzero_x");                // retain the first nonzero digit as the scientific significand start
    emitter.instruction("test r11d, r11d");                                      // are leading zeros still on the integer side of the point?
    emitter.instruction("jz __rt_sliic_normal_digit_next_x");                    // integer leading zeros do not affect the scientific exponent
    emitter.instruction("add rbx, 1");                                           // record one zero before the first fractional significant digit
    emitter.instruction("jmp __rt_sliic_normal_digit_next_x");                   // consume the leading fractional zero

    emitter.label("__rt_sliic_normal_first_nonzero_x");
    emitter.instruction("mov r13d, 1");                                          // mark that the normalized significand now has a first digit
    emitter.instruction("test r11d, r11d");                                      // was the decimal point consumed before this first digit?
    emitter.instruction("jnz __rt_sliic_normal_first_fractional_x");             // derive the exponent from fractional leading zeros when needed
    emitter.instruction("mov QWORD PTR [rsp + 880], 1");                         // remember that the first significant digit was in the integer portion
    emitter.instruction("jmp __rt_sliic_normal_write_first_x");                  // share output construction after setting the location flag
    emitter.label("__rt_sliic_normal_first_fractional_x");
    emitter.instruction("mov QWORD PTR [rsp + 880], 0");                         // mark a fractional first significant digit
    emitter.label("__rt_sliic_normal_write_first_x");
    emitter.instruction("mov BYTE PTR [r10], al");                               // write the first significant decimal digit to the local spelling
    emitter.instruction("add r10, 1");                                           // advance after the normalized first significand digit
    emitter.instruction("mov BYTE PTR [r10], 46");                               // write the stable scientific mantissa separator
    emitter.instruction("add r10, 1");                                           // advance past the normalized decimal point
    emitter.instruction("mov ecx, 1");                                           // count the retained first significant digit
    emitter.instruction("jmp __rt_sliic_normal_digit_next_x");                   // consume the source digit after initializing the output spelling

    emitter.label("__rt_sliic_normal_after_first_x");
    emitter.instruction("test r11d, r11d");                                      // do integer-side suffix digits still affect the scientific exponent?
    emitter.instruction("jnz __rt_sliic_normal_store_digit_x");                  // fractional digits leave the integer-derived exponent unchanged
    emitter.instruction("add r15, 1");                                           // count one integer digit after the first significant digit
    emitter.label("__rt_sliic_normal_store_digit_x");
    emitter.instruction("cmp rcx, 768");                                         // retain enough digits to distinguish every binary64 midpoint
    emitter.instruction("jae __rt_sliic_normal_sticky_digit_x");                 // inspect discarded digits only for their nonzero sticky contribution
    emitter.instruction("mov BYTE PTR [r10], al");                               // append a retained significant decimal digit to the local buffer
    emitter.instruction("add r10, 1");                                           // advance the private normalized output cursor
    emitter.instruction("add rcx, 1");                                           // record the bounded retained-significand length
    emitter.instruction("jmp __rt_sliic_normal_digit_next_x");                   // consume the source digit after storing it
    emitter.label("__rt_sliic_normal_sticky_digit_x");
    emitter.instruction("test edx, edx");                                        // can this discarded digit influence binary64 rounding?
    emitter.instruction("jz __rt_sliic_normal_digit_next_x");                    // discarded zeros do not alter the exact suffix direction
    emitter.instruction("mov QWORD PTR [rsp + 848], 1");                         // retain one sticky bit for every discarded nonzero suffix

    emitter.label("__rt_sliic_normal_digit_next_x");
    emitter.instruction("add r8, 1");                                            // advance to the next supplied PHP byte
    emitter.instruction("sub r9, 1");                                            // account for the consumed mantissa byte
    emitter.instruction("jmp __rt_sliic_normal_mantissa_x");                     // continue the bounded grammar scan

    emitter.label("__rt_sliic_normal_mantissa_non_digit_x");
    emitter.instruction("cmp eax, 46");                                          // recognize the single optional decimal point
    emitter.instruction("jne __rt_sliic_normal_after_mantissa_x");               // any other byte terminates the mantissa
    emitter.instruction("test r11d, r11d");                                      // has a decimal point already been consumed?
    emitter.instruction("jnz __rt_sliic_normal_after_mantissa_x");               // leave a second decimal point for trailing validation
    emitter.instruction("mov r11d, 1");                                          // remember that later digits are fractional
    emitter.instruction("add r8, 1");                                            // consume the single accepted decimal point
    emitter.instruction("sub r9, 1");                                            // account for the decimal-point byte
    emitter.instruction("jmp __rt_sliic_normal_mantissa_x");                     // continue with the optional fractional digit run

    emitter.label("__rt_sliic_normal_after_mantissa_x");
    emitter.instruction("test r12d, r12d");                                      // did the mantissa contain a digit on either side of the point?
    emitter.instruction("jz __rt_sliic_normal_false_x");                         // reject signs and points that contain no mantissa digit
    emitter.instruction("xor eax, eax");                                         // initialize the explicit exponent magnitude to zero
    emitter.instruction("xor edi, edi");                                         // initialize the explicit exponent sign to positive
    emitter.instruction("test r9, r9");                                          // can an exponent or trailing whitespace still follow?
    emitter.instruction("jz __rt_sliic_normal_finish_scan_x");                   // a complete mantissa may end exactly at the bounded input end
    emitter.instruction("movzx edx, BYTE PTR [r8]");                             // inspect the byte immediately after the mantissa
    emitter.instruction("cmp edx, 101");                                         // recognize a lowercase scientific exponent marker
    emitter.instruction("je __rt_sliic_normal_exponent_start_x");                // consume a lowercase exponent suffix
    emitter.instruction("cmp edx, 69");                                          // recognize an uppercase scientific exponent marker
    emitter.instruction("je __rt_sliic_normal_exponent_start_x");                // consume an uppercase exponent suffix
    emitter.instruction("jmp __rt_sliic_normal_trailing_ws_x");                  // only PHP whitespace may follow a complete numeric string

    emitter.label("__rt_sliic_normal_exponent_start_x");
    emitter.instruction("add r8, 1");                                            // consume the exponent marker inside the bounded range
    emitter.instruction("sub r9, 1");                                            // account for the exponent marker byte
    emitter.instruction("jz __rt_sliic_normal_false_x");                         // reject an exponent marker without exponent digits
    emitter.instruction("xor esi, esi");                                         // clear the exponent-digit-seen flag before optional sign handling
    emitter.instruction("movzx edx, BYTE PTR [r8]");                             // inspect the optional exponent sign byte
    emitter.instruction("cmp edx, 43");                                          // recognize an explicit positive exponent sign
    emitter.instruction("je __rt_sliic_normal_exponent_consume_sign_x");         // consume the explicit positive exponent sign
    emitter.instruction("cmp edx, 45");                                          // recognize an explicit negative exponent sign
    emitter.instruction("jne __rt_sliic_normal_exponent_digits_x");              // start digit parsing when the first byte is already a digit
    emitter.instruction("mov edi, 1");                                           // remember that the explicit exponent subtracts from the scientific exponent
    emitter.label("__rt_sliic_normal_exponent_consume_sign_x");
    emitter.instruction("add r8, 1");                                            // consume the optional exponent sign
    emitter.instruction("sub r9, 1");                                            // account for the exponent sign byte
    emitter.instruction("jz __rt_sliic_normal_false_x");                         // reject a sign without exponent digits

    emitter.label("__rt_sliic_normal_exponent_digits_x");
    emitter.instruction("test r9, r9");                                          // did exponent parsing reach the exact input boundary?
    emitter.instruction("jz __rt_sliic_normal_exponent_done_x");                 // finish after consuming every bounded exponent byte
    emitter.instruction("movzx edx, BYTE PTR [r8]");                             // read one bounded exponent candidate byte
    emitter.instruction("sub edx, 48");                                          // normalize a candidate exponent decimal digit
    emitter.instruction("cmp edx, 9");                                           // test the exponent digit range
    emitter.instruction("ja __rt_sliic_normal_exponent_done_x");                 // leave trailing whitespace for the common validator
    emitter.instruction("mov esi, 1");                                           // record that the exponent contains at least one digit
    emitter.instruction("cmp rax, QWORD PTR [rsp + 840]");                       // has the exponent reached its source-relative saturation threshold?
    emitter.instruction("jae __rt_sliic_normal_exponent_next_x");                // preserve the saturated magnitude while scanning remaining bytes
    emitter.instruction("imul rax, rax, 10");                                    // shift the bounded exponent magnitude by one decimal place
    emitter.instruction("jo __rt_sliic_normal_exponent_saturate_x");             // clamp instead of wrapping a hostile exponent magnitude
    emitter.instruction("add rax, rdx");                                         // append the next decimal exponent digit in constant work
    emitter.instruction("jc __rt_sliic_normal_exponent_saturate_x");             // clamp if the appended digit overflows the unsigned magnitude
    emitter.instruction("cmp rax, QWORD PTR [rsp + 840]");                       // did the appended digit cross the cancellation-safe threshold?
    emitter.instruction("jbe __rt_sliic_normal_exponent_next_x");                // retain magnitudes that remain inside the useful interval
    emitter.label("__rt_sliic_normal_exponent_saturate_x");
    emitter.instruction("mov rax, QWORD PTR [rsp + 840]");                       // saturate while retaining enough magnitude for source-length cancellation
    emitter.label("__rt_sliic_normal_exponent_next_x");
    emitter.instruction("add r8, 1");                                            // advance past one consumed exponent digit
    emitter.instruction("sub r9, 1");                                            // account for that exponent digit
    emitter.instruction("jmp __rt_sliic_normal_exponent_digits_x");              // scan all exponent bytes without exponent-sized scale loops

    emitter.label("__rt_sliic_normal_exponent_done_x");
    emitter.instruction("test esi, esi");                                        // did the exponent marker have at least one decimal digit?
    emitter.instruction("jz __rt_sliic_normal_false_x");                         // reject an empty exponent suffix

    emitter.label("__rt_sliic_normal_trailing_ws_x");
    emitter.instruction("test r9, r9");                                          // has the complete bounded suffix been validated?
    emitter.instruction("jz __rt_sliic_normal_finish_scan_x");                   // finish when no trailing byte remains
    emitter.instruction("movzx edx, BYTE PTR [r8]");                             // inspect one trailing byte without reading past the PHP string
    emitter.instruction("cmp edx, 32");                                          // accept ASCII space after a numeric payload
    emitter.instruction("je __rt_sliic_normal_trailing_next_x");                 // consume a trailing ASCII space
    emitter.instruction("mov ecx, edx");                                         // preserve the byte while normalizing trailing control whitespace
    emitter.instruction("sub ecx, 9");                                           // normalize tab through carriage return for trailing whitespace
    emitter.instruction("cmp ecx, 4");                                           // test the remaining PHP trailing-whitespace bytes
    emitter.instruction("ja __rt_sliic_normal_false_x");                         // reject every non-whitespace trailing byte
    emitter.label("__rt_sliic_normal_trailing_next_x");
    emitter.instruction("add r8, 1");                                            // advance after one accepted trailing whitespace byte
    emitter.instruction("sub r9, 1");                                            // account for the consumed trailing byte
    emitter.instruction("jmp __rt_sliic_normal_trailing_ws_x");                  // validate the entire remaining bounded suffix

    emitter.label("__rt_sliic_normal_finish_scan_x");
    emitter.instruction("test r13d, r13d");                                      // did the mantissa contain any nonzero digit?
    emitter.instruction("jnz __rt_sliic_normal_nonzero_value_x");                // nonzero mantissas need a normalized scientific exponent
    emitter.instruction("mov BYTE PTR [r10], 48");                               // append the canonical zero after a possible retained minus sign
    emitter.instruction("add r10, 1");                                           // advance the local spelling after the zero payload
    emitter.instruction("jmp __rt_sliic_normal_call_strtod_x");                  // parse signed or unsigned zero once through libc

    emitter.label("__rt_sliic_normal_nonzero_value_x");
    emitter.instruction("cmp QWORD PTR [rsp + 848], 0");                         // did a discarded nonzero suffix set the sticky bit?
    emitter.instruction("je __rt_sliic_normal_append_exponent_x");               // omit the sticky digit when every discarded digit was zero
    emitter.instruction("mov BYTE PTR [r10], 49");                               // encode the discarded suffix direction after the retained prefix
    emitter.instruction("add r10, 1");                                           // advance past the single sticky digit
    emitter.label("__rt_sliic_normal_append_exponent_x");
    emitter.instruction("mov BYTE PTR [r10], 101");                              // append the normalized scientific exponent separator
    emitter.instruction("add r10, 1");                                           // advance after the exponent marker
    emitter.instruction("cmp QWORD PTR [rsp + 880], 0");                         // was the first significant digit in the integer portion?
    emitter.instruction("jne __rt_sliic_normal_combine_exponent_x");             // integer suffix count already is the base scientific exponent
    emitter.instruction("mov r15, rbx");                                         // load the leading fractional-zero count
    emitter.instruction("neg r15");                                              // negate fractional leading zeros for the decimal exponent
    emitter.instruction("sub r15, 1");                                           // account for the first nonzero fractional digit itself
    emitter.label("__rt_sliic_normal_combine_exponent_x");
    emitter.instruction("test edi, edi");                                        // does the explicit exponent have a negative sign?
    emitter.instruction("jz __rt_sliic_normal_add_exponent_x");                  // positive explicit exponents add to the normalized exponent
    emitter.instruction("sub r15, rax");                                         // apply a negative explicit exponent in constant time
    emitter.instruction("jmp __rt_sliic_normal_clamp_exponent_x");               // clamp the combined exponent before decimal rendering
    emitter.label("__rt_sliic_normal_add_exponent_x");
    emitter.instruction("add r15, rax");                                         // apply a positive explicit exponent in constant time
    emitter.label("__rt_sliic_normal_clamp_exponent_x");
    emitter.instruction("cmp r15, 4095");                                        // test whether the combined exponent guarantees overflow direction
    emitter.instruction("jle __rt_sliic_normal_clamp_low_x");                    // preserve representable and underflow-side exponent values
    emitter.instruction("mov r15, 4095");                                        // saturate positive exponents for a short local spelling
    emitter.label("__rt_sliic_normal_clamp_low_x");
    emitter.instruction("cmp r15, -4095");                                       // test whether the combined exponent guarantees underflow direction
    emitter.instruction("jge __rt_sliic_normal_render_exponent_x");              // retain exponents inside the symmetric rendering interval
    emitter.instruction("mov r15, -4095");                                       // saturate negative exponents for a short local spelling
    emitter.label("__rt_sliic_normal_render_exponent_x");
    emitter.instruction("test r15, r15");                                        // decide whether the rendered exponent needs a minus sign
    emitter.instruction("jns __rt_sliic_normal_render_digits_x");                // render nonnegative exponents directly
    emitter.instruction("mov BYTE PTR [r10], 45");                               // append the negative exponent sign
    emitter.instruction("add r10, 1");                                           // advance after the rendered exponent sign
    emitter.instruction("neg r15");                                              // convert the bounded exponent magnitude to unsigned decimal
    emitter.label("__rt_sliic_normal_render_digits_x");
    emitter.instruction("lea r8, [rsp + 848]");                                  // start reverse exponent rendering inside the fixed local tail buffer
    emitter.instruction("mov rsi, 10");                                          // materialize the decimal divisor for exponent rendering
    emitter.label("__rt_sliic_normal_reverse_digit_x");
    emitter.instruction("mov rax, r15");                                         // load the remaining exponent magnitude as the unsigned dividend
    emitter.instruction("xor edx, edx");                                         // clear the high dividend word before unsigned division
    emitter.instruction("div rsi");                                              // compute the next exponent quotient and decimal remainder
    emitter.instruction("sub r8, 1");                                            // reserve one reverse-rendered exponent byte
    emitter.instruction("add edx, 48");                                          // encode the decimal remainder as ASCII
    emitter.instruction("mov BYTE PTR [r8], dl");                                // store one exponent digit in reverse order
    emitter.instruction("mov r15, rax");                                         // continue with the remaining exponent quotient
    emitter.instruction("test r15, r15");                                        // did the quotient retain another decimal digit?
    emitter.instruction("jnz __rt_sliic_normal_reverse_digit_x");                // render at most four digits after exponent saturation
    emitter.instruction("lea r9, [rsp + 848]");                                  // retain the reverse-rendering tail end for the forward copy loop
    emitter.label("__rt_sliic_normal_copy_exponent_x");
    emitter.instruction("mov al, BYTE PTR [r8]");                                // load one forward exponent digit from the local reverse buffer
    emitter.instruction("add r8, 1");                                            // advance the reverse-buffer cursor after the load
    emitter.instruction("mov BYTE PTR [r10], al");                               // append the exponent digit to the normalized scientific spelling
    emitter.instruction("add r10, 1");                                           // advance the normalized output cursor
    emitter.instruction("cmp r8, r9");                                           // stop after copying the exact reverse-rendered exponent suffix
    emitter.instruction("jne __rt_sliic_normal_copy_exponent_x");                // copy every rendered exponent digit into the local spelling

    emitter.label("__rt_sliic_normal_call_strtod_x");
    emitter.instruction("mov BYTE PTR [r10], 0");                                // terminate the private normalized spelling for the libc call
    emitter.instruction("mov rdi, rsp");                                         // pass the local normalized spelling as `strtod`'s first argument
    emitter.instruction("xor esi, esi");                                         // request no end-pointer after complete bounded grammar validation
    emitter.instruction("call strtod");                                          // convert once with libc's correctly-rounded binary64 decimal parser
    emitter.instruction("mov rax, 1");                                           // report a complete PHP numeric string while preserving xmm0
    emitter.instruction("jmp __rt_sliic_normal_return_x");                       // share callee-saved restoration with the rejection path

    emitter.label("__rt_sliic_normal_false_x");
    emitter.instruction("xor eax, eax");                                         // report a nonnumeric bounded PHP string without invoking libc
    emitter.label("__rt_sliic_normal_return_x");
    emitter.instruction("mov rbx, QWORD PTR [rsp + 856]");                       // restore the caller's callee-saved rbx value
    emitter.instruction("mov r12, QWORD PTR [rsp + 864]");                       // restore the caller's callee-saved r12 value
    emitter.instruction("mov r13, QWORD PTR [rsp + 872]");                       // restore the caller's callee-saved r13 value
    emitter.instruction("mov r15, QWORD PTR [rsp + 888]");                       // restore the caller's callee-saved r15 value
    emitter.instruction("add rsp, 896");                                         // release the fixed normalization frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                  // return the numeric flag and correctly rounded double
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};
    use std::collections::BTreeMap;

    /// THE INTEGER-SIGNIFICAND FLAG AND THE EXPONENT THRESHOLD DO NOT SHARE A SLOT.
    ///
    /// On `main` the flag lived in `r14`, which the ctx-register work reserved. Moving it to
    /// the stack is correct; moving it to `[rsp + 840]` was not, because the exponent
    /// saturation threshold already owned that slot. The two then overwrote each other on
    /// every value carrying an exponent: the flag write destroyed the threshold, and the
    /// threshold read returned 0 or 1.
    ///
    /// MEASURED. `1e-2` stopped parsing as a number, so `ksort(["0.1" => …, "1e-2" => …])`
    /// fell back to byte ordering and put `0.1` first —
    /// `codegen::arrays::assoc_helpers::test_assoc_key_sorts_compare_negative_exponent_keys_numerically`,
    /// red on the linux-x86_64 shard only, because the AArch64 arm still holds the flag in a
    /// register.
    ///
    /// A register rename is safe by construction: the assembler refuses a name that does not
    /// exist, and a register is either free or visibly in use nearby. A stack slot is neither
    /// — nothing rejects an occupied offset, and the other owner can be four hundred lines
    /// away. That asymmetry is why this test exists and why it checks slots, not registers.
    #[test]
    fn the_numeric_parsers_flag_and_threshold_use_different_stack_slots() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        super::emit_str_looks_like_int_for_coercion(&mut emitter);
        let asm = emitter.output();

        // Every slot written with a literal is a flag; every slot that takes a register or is
        // compared against one carries a computed value. A slot in both sets has two owners.
        let mut literal_writes: BTreeMap<String, usize> = BTreeMap::new();
        let mut value_uses: BTreeMap<String, usize> = BTreeMap::new();
        for raw in asm.lines() {
            let line = raw.trim();
            let Some(start) = line.find("[rsp + ") else { continue };
            let Some(end) = line[start..].find(']') else { continue };
            let slot = line[start..start + end + 1].to_string();
            let after = line[start + end + 1..].trim_start();
            if let Some(value) = after.strip_prefix(',') {
                let value = value.trim();
                if value.parse::<i64>().is_ok() {
                    *literal_writes.entry(slot).or_default() += 1;
                } else {
                    *value_uses.entry(slot).or_default() += 1;
                }
            } else if line.starts_with("cmp ") || line.starts_with("mov r") || line.starts_with("mov e") {
                // `cmp rax, [slot]` and `mov <reg>, [slot]` both read a computed value.
                *value_uses.entry(slot).or_default() += 1;
            }
        }

        let shared: Vec<&String> = literal_writes
            .keys()
            .filter(|slot| value_uses.contains_key(*slot))
            .collect();
        assert!(
            shared.is_empty(),
            "these frame slots of __rt_str_looks_like_int_for_coercion are written with a \
             literal AND used to carry a computed value — two owners, one slot, which is how \
             the exponent threshold and the integer-significand flag destroyed each other: \
             {shared:?}\nliteral writes: {literal_writes:?}\nvalue uses: {value_uses:?}"
        );
    }
}
