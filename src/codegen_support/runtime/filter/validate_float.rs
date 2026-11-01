//! Purpose:
//! Emits the `__rt_filter_validate_float` runtime helper: a DEDICATED grammar
//! scanner backing `filter_var($v, FILTER_VALIDATE_FLOAT)` on string input.
//! The scanner alone decides accept/reject (never `strtod`'s own, more lenient
//! grammar — e.g. `strtod` accepts a `0x1A`-style hex-float prefix that PHP's
//! filter rejects); `strtod` is only used AFTER acceptance, purely to compute
//! the already-validated numeric value, mirroring `__rt_str_to_int`'s existing
//! "validate ourselves, delegate only the arithmetic" pattern.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::filter`.
//! - `crate::codegen::lower_inst::builtins::filter` (the `filter_var()` EIR lowering).
//!
//! Key details:
//! - Grammar (php-verified with `php -n -r 'var_dump(filter_var(..., FILTER_VALIDATE_FLOAT));'`,
//!   PHP 8.5.6 local): PHP-filter whitespace (`__rt_filter_trim_ws`), optional
//!   sign, a mantissa (`digits[.digits]` or `.digits`, requiring at least one
//!   digit somewhere), and an optional `[eE][+-]?digits` exponent (which, if the
//!   `e`/`E` is present, REQUIRES at least one exponent digit — `"1e"`/`"1e+"`
//!   fail). `"0x1A"`, `"1,5"`, `"."`, `"e3"`, `"INF"`, `"NAN"` all fail the grammar
//!   scan outright (no letters besides `e`/`E` are ever accepted).
//! - Overflow (`"1e400"`) is detected AFTER `strtod` by comparing `|result|`
//!   against the exact `+infinity` IEEE-754 bit pattern — `strtod` saturates to
//!   `+-HUGE_VAL` (`+-infinity`) on overflow per C99, so an exact-infinity result
//!   is unambiguously an overflow here (the grammar scan already rejected the
//!   literal tokens `"INF"`/`"NAN"`, so a genuine infinite/NaN result can only
//!   arise from overflow).

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_filter_validate_float` for the host target.
///
/// AArch64: input x1=ptr, x2=len. Output: x0=success (0/1), d0=parsed `f64`
/// (defined only when x0=1).
/// x86_64: input rax=ptr, rdx=len. Output: rax=success (0/1), xmm0=parsed `f64`
/// (defined only when rax=1).
pub fn emit_filter_validate_float(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_filter_validate_float_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: filter_validate_float ---");
    emitter.label_global("__rt_filter_validate_float");

    // -- set up the helper frame (protects x30 across the trim/cstr/strtod calls) --
    emitter.instruction("sub sp, sp, #16");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("add x29, sp, #0");                                     // establish a stable helper frame pointer
    abi::emit_call_label(emitter, "__rt_filter_trim_ws");                       // trim PHP-filter whitespace: x1=ptr, x2=len (trimmed)

    emitter.instruction("mov x3, #0");                                          // scan index
    emitter.instruction("cbz x2, __rt_filter_validate_float_fail");             // an empty (post-trim) string is never a valid float

    // -- optional sign --
    emitter.instruction("ldrb w4, [x1]");                                       // load the first byte to check for a sign
    emitter.instruction("cmp w4, #0x2D");                                       // is it '-'?
    emitter.instruction("b.eq __rt_filter_validate_float_lead_sign");           // sign found
    emitter.instruction("cmp w4, #0x2B");                                       // is it '+'?
    emitter.instruction("b.eq __rt_filter_validate_float_lead_sign");           // sign found
    emitter.instruction("b __rt_filter_validate_float_int_part");               // no sign present
    emitter.label("__rt_filter_validate_float_lead_sign");
    emitter.instruction("add x3, x3, #1");                                      // consume the sign byte

    // -- integer part digits --
    emitter.label("__rt_filter_validate_float_int_part");
    emitter.instruction("mov x5, #0");                                          // mantissa digit count
    emitter.label("__rt_filter_validate_float_int_loop");
    emitter.instruction("cmp x3, x2");                                          // reached the end of the string?
    emitter.instruction("b.ge __rt_filter_validate_float_check_mantissa");      // finish the integer part scan
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load the current byte
    emitter.instruction("sub w6, w4, #0x30");                                   // normalize to a candidate decimal digit
    emitter.instruction("cmp w6, #9");                                          // is it a decimal digit?
    emitter.instruction("b.hi __rt_filter_validate_float_check_mantissa");      // non-digit: stop the integer part scan
    emitter.instruction("add x3, x3, #1");                                      // advance past this digit
    emitter.instruction("add x5, x5, #1");                                      // count it in the mantissa total
    emitter.instruction("b __rt_filter_validate_float_int_loop");               // continue scanning integer-part digits

    // -- optional '.' fractional part --
    emitter.label("__rt_filter_validate_float_check_mantissa");
    emitter.instruction("cmp x3, x2");                                          // reached the end already?
    emitter.instruction("b.ge __rt_filter_validate_float_mantissa_done");       // no room for a '.' fractional part
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load the current byte
    emitter.instruction("cmp w4, #0x2E");                                       // is it '.'?
    emitter.instruction("b.ne __rt_filter_validate_float_mantissa_done");       // no fractional part present
    emitter.instruction("add x3, x3, #1");                                      // consume the '.'
    emitter.label("__rt_filter_validate_float_frac_loop");
    emitter.instruction("cmp x3, x2");                                          // reached the end of the string?
    emitter.instruction("b.ge __rt_filter_validate_float_mantissa_done");       // finish the fractional part scan
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load the current byte
    emitter.instruction("sub w6, w4, #0x30");                                   // normalize to a candidate decimal digit
    emitter.instruction("cmp w6, #9");                                          // is it a decimal digit?
    emitter.instruction("b.hi __rt_filter_validate_float_mantissa_done");       // non-digit: stop the fractional part scan
    emitter.instruction("add x3, x3, #1");                                      // advance past this digit
    emitter.instruction("add x5, x5, #1");                                      // count it in the mantissa total
    emitter.instruction("b __rt_filter_validate_float_frac_loop");              // continue scanning fractional-part digits

    emitter.label("__rt_filter_validate_float_mantissa_done");
    emitter.instruction("cbz x5, __rt_filter_validate_float_fail");             // the mantissa must contain at least one digit

    // -- optional exponent --
    emitter.instruction("cmp x3, x2");                                          // reached the end already?
    emitter.instruction("b.ge __rt_filter_validate_float_scan_done");           // no room for an exponent
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load the current byte
    emitter.instruction("cmp w4, #0x65");                                       // is it 'e'?
    emitter.instruction("b.eq __rt_filter_validate_float_has_exp");             // exponent marker found
    emitter.instruction("cmp w4, #0x45");                                       // is it 'E'?
    emitter.instruction("b.eq __rt_filter_validate_float_has_exp");             // exponent marker found
    emitter.instruction("b __rt_filter_validate_float_scan_done");              // no exponent present
    emitter.label("__rt_filter_validate_float_has_exp");
    emitter.instruction("add x3, x3, #1");                                      // consume the 'e'/'E'
    emitter.instruction("cmp x3, x2");                                          // did the exponent marker consume the whole string?
    emitter.instruction("b.ge __rt_filter_validate_float_fail");                // a bare trailing 'e'/'E' is invalid
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load the byte after 'e'/'E'
    emitter.instruction("cmp w4, #0x2D");                                       // is it '-'?
    emitter.instruction("b.eq __rt_filter_validate_float_exp_sign");            // exponent sign found
    emitter.instruction("cmp w4, #0x2B");                                       // is it '+'?
    emitter.instruction("b.eq __rt_filter_validate_float_exp_sign");            // exponent sign found
    emitter.instruction("b __rt_filter_validate_float_exp_digits");             // no exponent sign present
    emitter.label("__rt_filter_validate_float_exp_sign");
    emitter.instruction("add x3, x3, #1");                                      // consume the exponent sign byte
    emitter.label("__rt_filter_validate_float_exp_digits");
    emitter.instruction("mov x7, #0");                                          // exponent digit count
    emitter.label("__rt_filter_validate_float_exp_loop");
    emitter.instruction("cmp x3, x2");                                          // reached the end of the string?
    emitter.instruction("b.ge __rt_filter_validate_float_exp_done");            // finish the exponent scan
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load the current byte
    emitter.instruction("sub w6, w4, #0x30");                                   // normalize to a candidate decimal digit
    emitter.instruction("cmp w6, #9");                                          // is it a decimal digit?
    emitter.instruction("b.hi __rt_filter_validate_float_exp_done");            // non-digit: stop the exponent scan
    emitter.instruction("add x3, x3, #1");                                      // advance past this digit
    emitter.instruction("add x7, x7, #1");                                      // count this exponent digit
    emitter.instruction("b __rt_filter_validate_float_exp_loop");               // continue scanning exponent digits
    emitter.label("__rt_filter_validate_float_exp_done");
    emitter.instruction("cbz x7, __rt_filter_validate_float_fail");             // 'e'/'E' with no exponent digits is invalid

    emitter.label("__rt_filter_validate_float_scan_done");
    emitter.instruction("cmp x3, x2");                                          // did the scan consume the ENTIRE trimmed string?
    emitter.instruction("b.ne __rt_filter_validate_float_fail");                // leftover bytes make the string invalid

    // -- grammar accepted: delegate only the arithmetic to strtod --
    abi::emit_call_label(emitter, "__rt_cstr");                                // x0 = NUL-terminated copy of the (still-trimmed) x1/x2 string
    emitter.instruction("mov x1, #0");                                          // strtod endptr argument: NULL (grammar already validated)
    emitter.bl_c("strtod");                                                   // d0 = parsed double

    // -- overflow check: strtod saturates to +-infinity on overflow (C99) --
    emitter.instruction("fmov d1, d0");                                         // copy the parsed value for the magnitude check
    emitter.instruction("fabs d1, d1");                                         // d1 = |value|
    abi::emit_load_int_immediate(emitter, "x5", 0x7FF0_0000_0000_0000_i64);   // x5 = +infinity bit pattern
    emitter.instruction("fmov d2, x5");                                         // d2 = +infinity as a double
    emitter.instruction("fcmp d1, d2");                                         // does |value| equal +infinity exactly?
    emitter.instruction("b.eq __rt_filter_validate_float_fail");                // an exact-infinity result means strtod overflowed

    emitter.instruction("mov x0, #1");                                          // report success
    emitter.instruction("b __rt_filter_validate_float_done");                   // done

    emitter.label("__rt_filter_validate_float_fail");
    emitter.instruction("mov x0, #0");                                          // report failure
    emitter.instruction("fmov d0, xzr");                                        // the value is undefined on failure

    emitter.label("__rt_filter_validate_float_done");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return (x0=success, d0=value)
}

/// Emits the x86_64 System V variant of `__rt_filter_validate_float`.
fn emit_filter_validate_float_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: filter_validate_float ---");
    emitter.label_global("__rt_filter_validate_float");

    // -- set up the helper frame (holds the trimmed ptr/len across strcasecmp-free but strtod/cstr calls) --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate two 8-byte slots: [rbp-8]=ptr, [rbp-16]=len
    abi::emit_call_label(emitter, "__rt_filter_trim_ws");                       // trim PHP-filter whitespace: rax=ptr, rdx=len (trimmed)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the trimmed pointer across later calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the trimmed length across later calls

    emitter.instruction("xor r8, r8");                                          // scan index
    emitter.instruction("test rdx, rdx");                                       // an empty (post-trim) string is never a valid float
    emitter.instruction("je __rt_filter_validate_float_fail_x86_64");           // reject empty input

    // -- optional sign --
    emitter.instruction("movzx r9d, BYTE PTR [rax]");                           // load the first byte to check for a sign
    emitter.instruction("cmp r9d, 0x2D");                                       // is it '-'?
    emitter.instruction("je __rt_filter_validate_float_lead_sign_x86_64");      // sign found
    emitter.instruction("cmp r9d, 0x2B");                                       // is it '+'?
    emitter.instruction("je __rt_filter_validate_float_lead_sign_x86_64");      // sign found
    emitter.instruction("jmp __rt_filter_validate_float_int_part_x86_64");      // no sign present
    emitter.label("__rt_filter_validate_float_lead_sign_x86_64");
    emitter.instruction("inc r8");                                              // consume the sign byte

    // -- integer part digits --
    emitter.label("__rt_filter_validate_float_int_part_x86_64");
    emitter.instruction("xor r10, r10");                                        // mantissa digit count
    emitter.label("__rt_filter_validate_float_int_loop_x86_64");
    emitter.instruction("cmp r8, rdx");                                         // reached the end of the string?
    emitter.instruction("jge __rt_filter_validate_float_check_mantissa_x86_64"); // finish the integer part scan
    emitter.instruction("movzx r9d, BYTE PTR [rax + r8]");                      // load the current byte
    emitter.instruction("sub r9d, 0x30");                                       // normalize to a candidate decimal digit
    emitter.instruction("cmp r9d, 9");                                          // is it a decimal digit?
    emitter.instruction("ja __rt_filter_validate_float_check_mantissa_x86_64"); // non-digit: stop the integer part scan
    emitter.instruction("inc r8");                                              // advance past this digit
    emitter.instruction("inc r10");                                             // count it in the mantissa total
    emitter.instruction("jmp __rt_filter_validate_float_int_loop_x86_64");      // continue scanning integer-part digits

    // -- optional '.' fractional part --
    emitter.label("__rt_filter_validate_float_check_mantissa_x86_64");
    emitter.instruction("cmp r8, rdx");                                         // reached the end already?
    emitter.instruction("jge __rt_filter_validate_float_mantissa_done_x86_64"); // no room for a '.' fractional part
    emitter.instruction("movzx r9d, BYTE PTR [rax + r8]");                      // load the current byte
    emitter.instruction("cmp r9d, 0x2E");                                       // is it '.'?
    emitter.instruction("jne __rt_filter_validate_float_mantissa_done_x86_64"); // no fractional part present
    emitter.instruction("inc r8");                                              // consume the '.'
    emitter.label("__rt_filter_validate_float_frac_loop_x86_64");
    emitter.instruction("cmp r8, rdx");                                         // reached the end of the string?
    emitter.instruction("jge __rt_filter_validate_float_mantissa_done_x86_64"); // finish the fractional part scan
    emitter.instruction("movzx r9d, BYTE PTR [rax + r8]");                      // load the current byte
    emitter.instruction("sub r9d, 0x30");                                       // normalize to a candidate decimal digit
    emitter.instruction("cmp r9d, 9");                                          // is it a decimal digit?
    emitter.instruction("ja __rt_filter_validate_float_mantissa_done_x86_64");  // non-digit: stop the fractional part scan
    emitter.instruction("inc r8");                                              // advance past this digit
    emitter.instruction("inc r10");                                             // count it in the mantissa total
    emitter.instruction("jmp __rt_filter_validate_float_frac_loop_x86_64");     // continue scanning fractional-part digits

    emitter.label("__rt_filter_validate_float_mantissa_done_x86_64");
    emitter.instruction("test r10, r10");                                       // the mantissa must contain at least one digit
    emitter.instruction("jz __rt_filter_validate_float_fail_x86_64");           // reject a missing mantissa

    // -- optional exponent --
    emitter.instruction("cmp r8, rdx");                                         // reached the end already?
    emitter.instruction("jge __rt_filter_validate_float_scan_done_x86_64");     // no room for an exponent
    emitter.instruction("movzx r9d, BYTE PTR [rax + r8]");                      // load the current byte
    emitter.instruction("cmp r9d, 0x65");                                       // is it 'e'?
    emitter.instruction("je __rt_filter_validate_float_has_exp_x86_64");        // exponent marker found
    emitter.instruction("cmp r9d, 0x45");                                       // is it 'E'?
    emitter.instruction("je __rt_filter_validate_float_has_exp_x86_64");        // exponent marker found
    emitter.instruction("jmp __rt_filter_validate_float_scan_done_x86_64");     // no exponent present
    emitter.label("__rt_filter_validate_float_has_exp_x86_64");
    emitter.instruction("inc r8");                                              // consume the 'e'/'E'
    emitter.instruction("cmp r8, rdx");                                         // did the exponent marker consume the whole string?
    emitter.instruction("jge __rt_filter_validate_float_fail_x86_64");          // a bare trailing 'e'/'E' is invalid
    emitter.instruction("movzx r9d, BYTE PTR [rax + r8]");                      // load the byte after 'e'/'E'
    emitter.instruction("cmp r9d, 0x2D");                                       // is it '-'?
    emitter.instruction("je __rt_filter_validate_float_exp_sign_x86_64");       // exponent sign found
    emitter.instruction("cmp r9d, 0x2B");                                       // is it '+'?
    emitter.instruction("je __rt_filter_validate_float_exp_sign_x86_64");       // exponent sign found
    emitter.instruction("jmp __rt_filter_validate_float_exp_digits_x86_64");    // no exponent sign present
    emitter.label("__rt_filter_validate_float_exp_sign_x86_64");
    emitter.instruction("inc r8");                                              // consume the exponent sign byte
    emitter.label("__rt_filter_validate_float_exp_digits_x86_64");
    emitter.instruction("xor r11, r11");                                        // exponent digit count
    emitter.label("__rt_filter_validate_float_exp_loop_x86_64");
    emitter.instruction("cmp r8, rdx");                                         // reached the end of the string?
    emitter.instruction("jge __rt_filter_validate_float_exp_done_x86_64");      // finish the exponent scan
    emitter.instruction("movzx r9d, BYTE PTR [rax + r8]");                      // load the current byte
    emitter.instruction("sub r9d, 0x30");                                       // normalize to a candidate decimal digit
    emitter.instruction("cmp r9d, 9");                                          // is it a decimal digit?
    emitter.instruction("ja __rt_filter_validate_float_exp_done_x86_64");       // non-digit: stop the exponent scan
    emitter.instruction("inc r8");                                              // advance past this digit
    emitter.instruction("inc r11");                                             // count this exponent digit
    emitter.instruction("jmp __rt_filter_validate_float_exp_loop_x86_64");      // continue scanning exponent digits
    emitter.label("__rt_filter_validate_float_exp_done_x86_64");
    emitter.instruction("test r11, r11");                                       // 'e'/'E' with no exponent digits is invalid
    emitter.instruction("jz __rt_filter_validate_float_fail_x86_64");           // reject a bare exponent marker

    emitter.label("__rt_filter_validate_float_scan_done_x86_64");
    emitter.instruction("cmp r8, rdx");                                         // did the scan consume the ENTIRE trimmed string?
    emitter.instruction("jne __rt_filter_validate_float_fail_x86_64");          // leftover bytes make the string invalid

    // -- grammar accepted: delegate only the arithmetic to strtod --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the trimmed pointer for __rt_cstr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the trimmed length for __rt_cstr
    abi::emit_call_label(emitter, "__rt_cstr");                               // rax = NUL-terminated copy of the trimmed string
    emitter.instruction("mov rdi, rax");                                        // strtod arg1: the C-string pointer
    emitter.instruction("xor esi, esi");                                        // strtod arg2 (endptr): NULL (grammar already validated)
    emitter.bl_c("strtod");                                                   // xmm0 = parsed double

    // -- overflow check: strtod saturates to +-infinity on overflow (C99) --
    emitter.instruction("movq xmm1, xmm0");                                     // copy the parsed value for the magnitude check
    abi::emit_load_int_immediate(emitter, "rax", 0x7FFF_FFFF_FFFF_FFFF_i64);  // rax = sign-bit mask (all bits except the sign bit)
    emitter.instruction("movq xmm2, rax");                                      // xmm2 = the sign-bit mask as packed bits
    emitter.instruction("andpd xmm1, xmm2");                                    // xmm1 = |value| (clear the sign bit)
    abi::emit_load_int_immediate(emitter, "rax", 0x7FF0_0000_0000_0000_i64);  // rax = +infinity bit pattern
    emitter.instruction("movq xmm2, rax");                                      // xmm2 = +infinity as packed bits
    emitter.instruction("ucomisd xmm1, xmm2");                                  // does |value| equal +infinity exactly?
    emitter.instruction("je __rt_filter_validate_float_fail_x86_64");           // an exact-infinity result means strtod overflowed

    emitter.instruction("mov eax, 1");                                          // report success
    emitter.instruction("jmp __rt_filter_validate_float_done_x86_64");          // done

    emitter.label("__rt_filter_validate_float_fail_x86_64");
    emitter.instruction("xor eax, eax");                                        // report failure
    emitter.instruction("xorpd xmm0, xmm0");                                    // the value is undefined on failure

    emitter.label("__rt_filter_validate_float_done_x86_64");
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return (rax=success, xmm0=value)
}
