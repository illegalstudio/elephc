//! Purpose:
//! Emits `__rt_str_smart_cmp`, the runtime implementation of PHP's `zendi_smart_strcmp` —
//! the single rule that orders two PHP strings — together with the two helpers it needs:
//! `__rt_num_run_class` (PHP's `IS_LONG` / `IS_DOUBLE` + `oflow` classification of a
//! numeric run) and `__rt_str_numeric_ex` (the full `is_numeric_string_ex` result).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//! - `__rt_php_compare`'s string-versus-string arm
//!   (`crate::codegen_support::runtime::compare::php_compare`).
//! - `__rt_str_loose_eq` (`crate::codegen_support::runtime::strings::str_loose_eq`),
//!   because PHP's `zendi_smart_streq` is exactly `zendi_smart_strcmp(..) == 0`.
//!
//! Key details:
//! - Two numeric strings do NOT compare as doubles. PHP classifies each side first, and
//!   two `IS_LONG` sides compare as `zend_long`, which is the only way
//!   `"9007199254740993" > "9007199254740992"` can be true: both round to the same
//!   double, so any `f64` route loses the answer.
//! - Integer text that does not fit `zend_long` becomes `IS_DOUBLE` and records `oflow`
//!   (`+1` / `-1`). PHP then leans on `oflow` twice: two same-side overflows that round to
//!   the same double fall back to a BYTE comparison, and an `IS_LONG` against an
//!   overflowed side is decided by the overflow's sign alone without consulting either
//!   value.
//! - `dval1 - dval2 == 0.` is written as a subtraction on purpose and is not the same test
//!   as `dval1 == dval2`: two integer strings long enough to round to infinity subtract to
//!   NaN, which is *not* equal to zero, so PHP does not take the byte fallback for them.
//! - A NUL byte anywhere in a string makes it non-numeric, which the bounded pre-scan in
//!   `__rt_str_numeric_ex` enforces. The shared `__rt_php_num_scan` is C-string based and
//!   cannot see past one; `is_numeric()` still goes through that path and still reads
//!   `"2\0"` as numeric, which is a separate defect in the scanner, not in this ordering.
//! - This is the runtime twin of `crate::optimize::fold::compare::compare_strings`, which
//!   already folds literal pairs correctly; the two must agree or a literal and a variable
//!   give different answers for the same comparison.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_num_run_class`: PHP's `IS_LONG` / `IS_DOUBLE` decision for a numeric run.
///
/// Input is the pointer `__rt_php_num_scan` returns: a NUL-terminated run with leading
/// whitespace already stripped and trailing garbage already clipped off.
///
/// Register contract (ARM64):
/// - Input: x0 = run pointer
/// - Output: x0 = 1 for `IS_LONG`, 0 for `IS_DOUBLE`; x1 = `oflow` (0, 1 or -1);
///   x2 = the exact `zend_long` value when x0 is 1
///
/// Register contract (x86_64 System V):
/// - Input: rdi = run pointer
/// - Output: rax = kind, rdx = `oflow`, rcx = the exact value
///
/// The classification follows `_is_numeric_string_ex`: leading zeros are not significant,
/// a `.` or an `e`/`E` anywhere in the run makes it `IS_DOUBLE` with `oflow` 0, and an
/// integer run is `IS_LONG` only while its value fits `zend_long` — 19 significant digits
/// are compared against 2^63 so that `-9223372036854775808` stays `IS_LONG` while
/// `9223372036854775808` does not. The helper is a leaf: it makes no calls and needs no
/// stack frame.
pub fn emit_num_run_class(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_num_run_class_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: num_run_class (PHP IS_LONG / IS_DOUBLE classification) ---");
    emitter.label_global("__rt_num_run_class");

    emitter.instruction("mov x9, x0");                                          // x9 = scan cursor over the clipped numeric run
    emitter.instruction("mov x12, #0");                                         // clear the negative-sign flag
    emitter.instruction("ldrb w10, [x9]");                                      // load the run's first byte to test for a sign
    emitter.instruction("cmp w10, #43");                                        // '+' is accepted and carries no sign information
    emitter.instruction("b.eq __rt_nrc_sign");                                  // consume the leading plus
    emitter.instruction("cmp w10, #45");                                        // '-' marks the run as negative
    emitter.instruction("b.ne __rt_nrc_zeros");                                 // no sign at all: start at the first digit
    emitter.instruction("mov x12, #1");                                         // record that the value is negative
    emitter.label("__rt_nrc_sign");
    emitter.instruction("add x9, x9, #1");                                      // step past the consumed sign byte

    // -- leading zeros are not significant digits: PHP skips them before counting --
    emitter.label("__rt_nrc_zeros");
    emitter.instruction("ldrb w10, [x9]");                                      // load the next candidate leading zero
    emitter.instruction("cmp w10, #48");                                        // '0'
    emitter.instruction("b.ne __rt_nrc_digits");                                // the first significant digit starts here
    emitter.instruction("add x9, x9, #1");                                      // skip an insignificant leading zero
    emitter.instruction("b __rt_nrc_zeros");                                    // keep skipping

    // -- accumulate the significant digits, which can never exceed 19 before overflowing --
    emitter.label("__rt_nrc_digits");
    emitter.instruction("mov x11, #0");                                         // significant digit count
    emitter.instruction("mov x13, #0");                                         // accumulated magnitude
    emitter.instruction("mov x15, #10");                                        // decimal radix for the multiply-accumulate
    emitter.label("__rt_nrc_digit_loop");
    emitter.instruction("ldrb w10, [x9]");                                      // load the next run byte
    emitter.instruction("sub w14, w10, #48");                                   // normalize it into a digit value
    emitter.instruction("cmp w14, #9");                                         // an unsigned compare rejects every non-digit byte at once
    emitter.instruction("b.hi __rt_nrc_end");                                   // the mantissa ends here
    emitter.instruction("add x11, x11, #1");                                    // count the significant digit
    emitter.instruction("cmp x11, #19");                                        // 19 digits still fit a u64 accumulator
    emitter.instruction("b.hi __rt_nrc_skip_acc");                              // past 19 the run overflows whatever follows
    emitter.instruction("madd x13, x13, x15, x14");                             // accumulate value = value * 10 + digit
    emitter.label("__rt_nrc_skip_acc");
    emitter.instruction("add x9, x9, #1");                                      // advance to the next run byte
    emitter.instruction("b __rt_nrc_digit_loop");                               // keep scanning the mantissa

    emitter.label("__rt_nrc_end");
    emitter.instruction("cbnz w10, __rt_nrc_double_spelling");                  // a surviving '.', 'e' or 'E' means the run spells a float
    emitter.instruction("cmp x11, #19");                                        // an integer spelling: does its magnitude fit?
    emitter.instruction("b.hi __rt_nrc_overflow");                              // 20 or more significant digits never fit
    emitter.instruction("b.lo __rt_nrc_long");                                  // 18 or fewer always fit
    emitter.instruction("movz x15, #0x8000, lsl #48");                          // 2^63: LONG_MAX + 1, and |LONG_MIN|
    emitter.instruction("cmp x13, x15");                                        // compare the exact 19-digit magnitude against it
    emitter.instruction("b.hi __rt_nrc_overflow");                              // above 2^63 overflows for either sign
    emitter.instruction("b.lo __rt_nrc_long");                                  // below 2^63 fits for either sign
    emitter.instruction("cbz x12, __rt_nrc_overflow");                          // exactly 2^63 only fits as the negative LONG_MIN

    emitter.label("__rt_nrc_long");
    emitter.instruction("mov x0, #1");                                          // report PHP's IS_LONG
    emitter.instruction("mov x1, #0");                                          // an in-range integer records no overflow
    emitter.instruction("mov x2, x13");                                         // hand back the exact magnitude
    emitter.instruction("cbz x12, __rt_nrc_ret");                               // a positive run needs no sign correction
    emitter.instruction("neg x2, x2");                                          // apply the leading minus
    emitter.label("__rt_nrc_ret");
    emitter.instruction("ret");                                                 // return the classification

    emitter.label("__rt_nrc_overflow");
    emitter.instruction("mov x0, #0");                                          // integer text beyond zend_long is IS_DOUBLE
    emitter.instruction("mov x1, #1");                                          // record the overflow as positive
    emitter.instruction("cbz x12, __rt_nrc_overflow_ret");                      // a positive run keeps oflow = 1
    emitter.instruction("mov x1, #-1");                                         // a negative run records oflow = -1
    emitter.label("__rt_nrc_overflow_ret");
    emitter.instruction("mov x2, #0");                                          // there is no exact integer value to report
    emitter.instruction("ret");                                                 // return the overflow classification

    emitter.label("__rt_nrc_double_spelling");
    emitter.instruction("mov x0, #0");                                          // a '.' or exponent makes the run IS_DOUBLE
    emitter.instruction("mov x1, #0");                                          // a genuine float spelling never sets oflow
    emitter.instruction("mov x2, #0");                                          // there is no exact integer value to report
    emitter.instruction("ret");                                                 // return the float classification
}

/// Emits the x86_64 Linux implementation of `__rt_num_run_class`.
///
/// Same classification as the ARM64 path: rdi carries the clipped run, and the helper
/// returns the kind in rax, `oflow` in rdx and the exact `zend_long` in rcx.
fn emit_num_run_class_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: num_run_class (PHP IS_LONG / IS_DOUBLE classification) ---");
    emitter.label_global("__rt_num_run_class");

    emitter.instruction("mov r8, rdi");                                         // r8 = scan cursor over the clipped numeric run
    emitter.instruction("xor r9d, r9d");                                        // clear the negative-sign flag
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the run's first byte to test for a sign
    emitter.instruction("cmp al, 43");                                          // '+' is accepted and carries no sign information
    emitter.instruction("je __rt_nrc_sign_linux_x86_64");                       // consume the leading plus
    emitter.instruction("cmp al, 45");                                          // '-' marks the run as negative
    emitter.instruction("jne __rt_nrc_zeros_linux_x86_64");                     // no sign at all: start at the first digit
    emitter.instruction("mov r9d, 1");                                          // record that the value is negative
    emitter.label("__rt_nrc_sign_linux_x86_64");
    emitter.instruction("inc r8");                                              // step past the consumed sign byte

    emitter.label("__rt_nrc_zeros_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the next candidate leading zero
    emitter.instruction("cmp al, 48");                                          // '0'
    emitter.instruction("jne __rt_nrc_digits_linux_x86_64");                    // the first significant digit starts here
    emitter.instruction("inc r8");                                              // skip an insignificant leading zero
    emitter.instruction("jmp __rt_nrc_zeros_linux_x86_64");                     // keep skipping

    emitter.label("__rt_nrc_digits_linux_x86_64");
    emitter.instruction("xor r10d, r10d");                                      // significant digit count
    emitter.instruction("xor r11d, r11d");                                      // accumulated magnitude
    emitter.label("__rt_nrc_digit_loop_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the next run byte
    emitter.instruction("mov esi, eax");                                        // copy it into the digit scratch register
    emitter.instruction("sub esi, 48");                                         // normalize it into a digit value
    emitter.instruction("cmp esi, 9");                                          // an unsigned compare rejects every non-digit byte at once
    emitter.instruction("ja __rt_nrc_end_linux_x86_64");                        // the mantissa ends here
    emitter.instruction("inc r10");                                             // count the significant digit
    emitter.instruction("cmp r10, 19");                                         // 19 digits still fit a u64 accumulator
    emitter.instruction("ja __rt_nrc_skip_acc_linux_x86_64");                   // past 19 the run overflows whatever follows
    emitter.instruction("imul r11, r11, 10");                                   // shift the accumulator one decimal place
    emitter.instruction("add r11, rsi");                                        // accumulate the digit
    emitter.label("__rt_nrc_skip_acc_linux_x86_64");
    emitter.instruction("inc r8");                                              // advance to the next run byte
    emitter.instruction("jmp __rt_nrc_digit_loop_linux_x86_64");                // keep scanning the mantissa

    emitter.label("__rt_nrc_end_linux_x86_64");
    emitter.instruction("test al, al");                                         // did the run end, or is a '.'/'e'/'E' left?
    emitter.instruction("jne __rt_nrc_double_spelling_linux_x86_64");           // a surviving byte means the run spells a float
    emitter.instruction("cmp r10, 19");                                         // an integer spelling: does its magnitude fit?
    emitter.instruction("ja __rt_nrc_overflow_linux_x86_64");                   // 20 or more significant digits never fit
    emitter.instruction("jb __rt_nrc_long_linux_x86_64");                       // 18 or fewer always fit
    emitter.instruction("mov rsi, 1");                                          // build 2^63 without a 64-bit immediate
    emitter.instruction("shl rsi, 63");                                         // 2^63: LONG_MAX + 1, and |LONG_MIN|
    emitter.instruction("cmp r11, rsi");                                        // compare the exact 19-digit magnitude against it
    emitter.instruction("ja __rt_nrc_overflow_linux_x86_64");                   // above 2^63 overflows for either sign
    emitter.instruction("jb __rt_nrc_long_linux_x86_64");                       // below 2^63 fits for either sign
    emitter.instruction("test r9d, r9d");                                       // exactly 2^63: only the negative LONG_MIN fits
    emitter.instruction("je __rt_nrc_overflow_linux_x86_64");                   // a positive 2^63 overflows

    emitter.label("__rt_nrc_long_linux_x86_64");
    emitter.instruction("mov rax, 1");                                          // report PHP's IS_LONG
    emitter.instruction("xor edx, edx");                                        // an in-range integer records no overflow
    emitter.instruction("mov rcx, r11");                                        // hand back the exact magnitude
    emitter.instruction("test r9d, r9d");                                       // was the run negative?
    emitter.instruction("je __rt_nrc_ret_linux_x86_64");                        // a positive run needs no sign correction
    emitter.instruction("neg rcx");                                             // apply the leading minus
    emitter.label("__rt_nrc_ret_linux_x86_64");
    emitter.instruction("ret");                                                 // return the classification

    emitter.label("__rt_nrc_overflow_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // integer text beyond zend_long is IS_DOUBLE
    emitter.instruction("mov rdx, 1");                                          // record the overflow as positive
    emitter.instruction("test r9d, r9d");                                       // was the run negative?
    emitter.instruction("je __rt_nrc_overflow_ret_linux_x86_64");               // a positive run keeps oflow = 1
    emitter.instruction("mov rdx, -1");                                         // a negative run records oflow = -1
    emitter.label("__rt_nrc_overflow_ret_linux_x86_64");
    emitter.instruction("xor ecx, ecx");                                        // there is no exact integer value to report
    emitter.instruction("ret");                                                 // return the overflow classification

    emitter.label("__rt_nrc_double_spelling_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // a '.' or exponent makes the run IS_DOUBLE
    emitter.instruction("xor edx, edx");                                        // a genuine float spelling never sets oflow
    emitter.instruction("xor ecx, ecx");                                        // there is no exact integer value to report
    emitter.instruction("ret");                                                 // return the float classification
}

/// Emits `__rt_str_numeric_ex`: the full `is_numeric_string_ex` result for a PHP string.
///
/// This is `__rt_str_to_number` plus the two facts a comparison needs and a bare double
/// cannot carry — whether the text spelled an integer, and the exact integer it spelled.
/// `strtod` runs only on the `IS_DOUBLE` path, matching PHP, which leaves `dval` at 0 for
/// an `IS_LONG` result.
///
/// Register contract (ARM64):
/// - Input: x1 = string pointer, x2 = string length
/// - Output: x0 = fully-numeric flag, x1 = kind, x2 = `oflow`, x3 = exact value, d0 = double
///
/// Register contract (x86_64 System V):
/// - Input: rax = string pointer, rdx = string length
/// - Output: rax = fully-numeric flag, rdx = kind, rcx = `oflow`, r8 = exact value,
///   xmm0 = double
///
/// A string containing a NUL byte is reported non-numeric before the scan even starts.
/// PHP measures its numeric run against the string LENGTH and NUL is not PHP whitespace, so
/// no fully numeric string can hold one — but `__rt_php_num_scan` reads a C string and would
/// stop at it, reading `"2\0"` as the numeric `2`.
///
/// Calls `__rt_cstr`, so the shared C-string scratch buffer holds only the most recent
/// operand: a caller comparing two strings must consume this result before parsing the next.
pub fn emit_str_numeric_ex(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_numeric_ex_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_numeric_ex (is_numeric_string_ex) ---");
    emitter.label_global("__rt_str_numeric_ex");

    emitter.instruction("sub sp, sp, #64");                                     // allocate slots for the classification across the strtod call
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame pointer

    // -- reject an embedded NUL before the C-string scanner can stop short of one --
    // PHP checks its numeric run against the string LENGTH, and NUL is not PHP whitespace,
    // so a fully numeric string can never contain one. `__rt_php_num_scan` works on a
    // C string and would read `"2\0"` as the numeric `2`; the bounded check here is what
    // keeps a comparison from treating it as a number.
    emitter.instruction("mov x9, x1");                                          // walk the PHP bytes, not the C-string copy
    emitter.instruction("mov x10, x2");                                         // count down the exact PHP length
    emitter.label("__rt_snx_nul_loop");
    emitter.instruction("cbz x10, __rt_snx_nul_clear");                         // the whole string is NUL-free
    emitter.instruction("ldrb w11, [x9], #1");                                  // load the next byte and advance
    emitter.instruction("cbz w11, __rt_snx_not_numeric");                       // an embedded NUL makes the string non-numeric
    emitter.instruction("sub x10, x10, #1");                                    // one fewer byte to inspect
    emitter.instruction("b __rt_snx_nul_loop");                                 // keep scanning

    emitter.label("__rt_snx_nul_clear");
    emitter.instruction("bl __rt_cstr");                                        // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("bl __rt_php_num_scan");                                // clip the scratch to PHP's leading numeric run
    emitter.instruction("str x1, [sp, #0]");                                    // save the fully-numeric flag
    emitter.instruction("str x0, [sp, #8]");                                    // save the clipped run pointer for the float parse
    emitter.instruction("bl __rt_num_run_class");                               // classify the run as IS_LONG or IS_DOUBLE
    emitter.instruction("stp x0, x1, [sp, #16]");                               // save the kind and the overflow marker
    emitter.instruction("str x2, [sp, #32]");                                   // save the exact integer value
    emitter.instruction("fmov d0, xzr");                                        // PHP leaves dval at 0 for an IS_LONG result
    emitter.instruction("cbnz x0, __rt_snx_done");                              // an integer run needs no strtod
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the clipped run for the float parse
    emitter.instruction("mov x1, #0");                                          // strtod endptr = NULL: the run is already clipped
    emitter.bl_c("strtod");                                                     // parse the clipped numeric run into d0

    emitter.instruction("b __rt_snx_done");                                     // fall through to the shared epilogue

    emitter.label("__rt_snx_not_numeric");
    emitter.instruction("str xzr, [sp, #0]");                                   // report the string as not fully numeric
    emitter.instruction("stp xzr, xzr, [sp, #16]");                             // clear the kind and the overflow marker
    emitter.instruction("str xzr, [sp, #32]");                                  // clear the exact integer value
    emitter.instruction("fmov d0, xzr");                                        // clear the double value

    emitter.label("__rt_snx_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the fully-numeric flag
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // return the kind and the overflow marker
    emitter.instruction("ldr x3, [sp, #32]");                                   // return the exact integer value
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the full numeric-string classification
}

/// Emits the x86_64 Linux implementation of `__rt_str_numeric_ex`.
///
/// Identical logic to the ARM64 path; the operand arrives in rax/rdx to match
/// `__rt_str_to_number`'s existing contract rather than the System V argument registers.
fn emit_str_numeric_ex_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_numeric_ex (is_numeric_string_ex) ---");
    emitter.label_global("__rt_str_numeric_ex");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before nested calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate aligned slots for the classification

    // -- reject an embedded NUL before the C-string scanner can stop short of one --
    emitter.instruction("mov r9, rax");                                         // walk the PHP bytes, not the C-string copy
    emitter.instruction("mov r10, rdx");                                        // count down the exact PHP length
    emitter.label("__rt_snx_nul_loop_linux_x86_64");
    emitter.instruction("test r10, r10");                                       // any bytes left to inspect?
    emitter.instruction("je __rt_snx_nul_clear_linux_x86_64");                  // the whole string is NUL-free
    emitter.instruction("cmp BYTE PTR [r9], 0");                                // is this byte an embedded NUL?
    emitter.instruction("je __rt_snx_not_numeric_linux_x86_64");                // an embedded NUL makes the string non-numeric
    emitter.instruction("inc r9");                                              // advance to the next byte
    emitter.instruction("dec r10");                                             // one fewer byte to inspect
    emitter.instruction("jmp __rt_snx_nul_loop_linux_x86_64");                  // keep scanning

    emitter.label("__rt_snx_nul_clear_linux_x86_64");
    emitter.instruction("call __rt_cstr");                                      // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("mov rdi, rax");                                        // pass the C-string pointer to the numeric-grammar scanner
    emitter.instruction("call __rt_php_num_scan");                              // clip the scratch to PHP's leading numeric run
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // save the fully-numeric flag
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the clipped run pointer for the float parse
    emitter.instruction("mov rdi, rax");                                        // pass the clipped run to the classifier
    emitter.instruction("call __rt_num_run_class");                             // classify the run as IS_LONG or IS_DOUBLE
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the kind
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the overflow marker
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the exact integer value
    emitter.instruction("pxor xmm0, xmm0");                                     // PHP leaves dval at 0 for an IS_LONG result
    emitter.instruction("test rax, rax");                                       // was the run classified as an integer?
    emitter.instruction("jne __rt_snx_done_linux_x86_64");                      // an integer run needs no strtod
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the clipped run for the float parse
    emitter.instruction("xor esi, esi");                                        // strtod endptr = NULL: the run is already clipped
    emitter.instruction("call strtod");                                         // parse the clipped numeric run into xmm0

    emitter.instruction("jmp __rt_snx_done_linux_x86_64");                      // fall through to the shared epilogue

    emitter.label("__rt_snx_not_numeric_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 8], 0");                          // report the string as not fully numeric
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // clear the kind
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // clear the overflow marker
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // clear the exact integer value
    emitter.instruction("pxor xmm0, xmm0");                                     // clear the double value

    emitter.label("__rt_snx_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the fully-numeric flag
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // return the kind
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // return the overflow marker
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // return the exact integer value
    emitter.instruction("add rsp, 48");                                         // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the full numeric-string classification
}

/// Emits `__rt_str_smart_cmp`: PHP's `zendi_smart_strcmp` for two runtime strings.
///
/// Register contract (ARM64):
/// - Input: x1/x2 = left (ptr, len), x3/x4 = right (ptr, len)
/// - Output: x0 = -1, 0 or 1
///
/// Register contract (x86_64 System V):
/// - Input: rdi/rsi = left (ptr, len), rdx/rcx = right (ptr, len)
/// - Output: rax = -1, 0 or 1
///
/// The order of the tests is PHP's and is load-bearing:
/// 1. either side non-numeric → byte comparison;
/// 2. both overflowed the same way and round to the same double → byte comparison, because
///    the doubles have thrown the difference away;
/// 3. both `IS_LONG` → exact `zend_long` comparison;
/// 4. one `IS_LONG` against an overflowed side → the overflow's sign decides, no values read;
/// 5. two infinities → byte comparison;
/// 6. otherwise → `ZEND_THREEWAY_COMPARE` on the doubles.
///
/// Calls: `__rt_str_numeric_ex` (twice) and `__rt_strcmp` (on every byte-comparison path).
pub fn emit_str_smart_cmp(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_smart_cmp_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_smart_cmp (zendi_smart_strcmp) ---");
    emitter.label_global("__rt_str_smart_cmp");

    emitter.instruction("sub sp, sp, #112");                                    // allocate slots for both operands and both classifications
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish a stable helper frame pointer
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save the left string pointer and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save the right string pointer and length

    emitter.instruction("bl __rt_str_numeric_ex");                              // classify the left string, which is already in x1/x2
    emitter.instruction("cbz x0, __rt_ssc_bytes");                              // a non-numeric operand forces the byte comparison
    emitter.instruction("stp x1, x2, [sp, #32]");                               // save the left kind and overflow marker
    emitter.instruction("str x3, [sp, #48]");                                   // save the left exact integer value
    emitter.instruction("str d0, [sp, #56]");                                   // save the left double value

    emitter.instruction("ldp x1, x2, [sp, #16]");                               // load the right string into the classifier input registers
    emitter.instruction("bl __rt_str_numeric_ex");                              // classify the right string
    emitter.instruction("cbz x0, __rt_ssc_bytes");                              // a non-numeric operand forces the byte comparison
    emitter.instruction("stp x1, x2, [sp, #64]");                               // save the right kind and overflow marker
    emitter.instruction("str x3, [sp, #80]");                                   // save the right exact integer value
    emitter.instruction("str d0, [sp, #88]");                                   // save the right double value

    // -- both integers overflowed the same way and round alike: the doubles cannot decide --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the left overflow marker
    emitter.instruction("cbz x9, __rt_ssc_kinds");                              // no overflow: the doubles are trustworthy
    emitter.instruction("ldr x10, [sp, #72]");                                  // reload the right overflow marker
    emitter.instruction("cmp x9, x10");                                         // did both overflow to the same side?
    emitter.instruction("b.ne __rt_ssc_kinds");                                 // opposite sides are ordered by sign below
    emitter.instruction("ldr d1, [sp, #56]");                                   // reload the left double value
    emitter.instruction("ldr d2, [sp, #88]");                                   // reload the right double value
    emitter.instruction("fsub d3, d1, d2");                                     // PHP tests dval1 - dval2, so two infinities give NaN
    emitter.instruction("fcmp d3, #0.0");                                       // NaN is not equal to zero, which is the point of the subtraction
    emitter.instruction("b.eq __rt_ssc_bytes");                                 // the doubles lost the difference: compare the spellings

    emitter.label("__rt_ssc_kinds");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the left kind
    emitter.instruction("ldr x10, [sp, #64]");                                  // reload the right kind
    emitter.instruction("cmp x9, #1");                                          // is the left operand IS_LONG?
    emitter.instruction("b.ne __rt_ssc_left_double");                           // the left operand is a double
    emitter.instruction("cmp x10, #1");                                         // is the right operand IS_LONG too?
    emitter.instruction("b.ne __rt_ssc_long_vs_double");                        // only the left one is an integer
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the left exact integer value
    emitter.instruction("ldr x12, [sp, #80]");                                  // reload the right exact integer value
    emitter.instruction("cmp x11, x12");                                        // two integers compare exactly, never through a double
    emitter.instruction("b.lt __rt_ssc_neg");                                   // the left integer is smaller
    emitter.instruction("b.gt __rt_ssc_pos");                                   // the left integer is larger
    emitter.instruction("b __rt_ssc_zero");                                     // the integers are equal

    emitter.label("__rt_ssc_long_vs_double");
    emitter.instruction("ldr x10, [sp, #72]");                                  // reload the right overflow marker
    emitter.instruction("cbz x10, __rt_ssc_widen_left");                        // an ordinary float: widen the integer and compare
    emitter.instruction("cmp x10, #0");                                         // the right operand is an integer beyond zend_long
    emitter.instruction("b.gt __rt_ssc_neg");                                   // it overflowed upwards, so it is larger
    emitter.instruction("b __rt_ssc_pos");                                      // it overflowed downwards, so it is smaller
    emitter.label("__rt_ssc_widen_left");
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the left exact integer value
    emitter.instruction("scvtf d1, x11");                                       // PHP widens the IS_LONG side into dval
    emitter.instruction("ldr d2, [sp, #88]");                                   // reload the right double value
    emitter.instruction("b __rt_ssc_doubles");                                  // compare as doubles

    emitter.label("__rt_ssc_left_double");
    emitter.instruction("cmp x10, #1");                                         // is the right operand IS_LONG?
    emitter.instruction("b.ne __rt_ssc_both_double");                           // both operands are doubles
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the left overflow marker
    emitter.instruction("cbz x9, __rt_ssc_widen_right");                        // an ordinary float: widen the integer and compare
    emitter.instruction("cmp x9, #0");                                          // the left operand is an integer beyond zend_long
    emitter.instruction("b.gt __rt_ssc_pos");                                   // it overflowed upwards, so it is larger
    emitter.instruction("b __rt_ssc_neg");                                      // it overflowed downwards, so it is smaller
    emitter.label("__rt_ssc_widen_right");
    emitter.instruction("ldr d1, [sp, #56]");                                   // reload the left double value
    emitter.instruction("ldr x12, [sp, #80]");                                  // reload the right exact integer value
    emitter.instruction("scvtf d2, x12");                                       // PHP widens the IS_LONG side into dval
    emitter.instruction("b __rt_ssc_doubles");                                  // compare as doubles

    emitter.label("__rt_ssc_both_double");
    emitter.instruction("ldr d1, [sp, #56]");                                   // reload the left double value
    emitter.instruction("ldr d2, [sp, #88]");                                   // reload the right double value
    emitter.instruction("fcmp d1, d2");                                         // are the two doubles equal?
    emitter.instruction("b.ne __rt_ssc_doubles");                               // different values order normally
    emitter.instruction("fmov x9, d1");                                         // reinterpret the shared value as bits
    emitter.instruction("and x9, x9, #0x7fffffffffffffff");                     // drop the sign to test the magnitude
    emitter.instruction("movz x10, #0x7ff0, lsl #48");                          // the exponent pattern of infinity
    emitter.instruction("cmp x9, x10");                                         // equal and non-finite means two infinities
    emitter.instruction("b.hs __rt_ssc_bytes");                                 // PHP compares the spellings rather than call them equal

    emitter.label("__rt_ssc_doubles");
    emitter.instruction("fcmp d1, d2");                                         // ZEND_THREEWAY_COMPARE on the two doubles
    emitter.instruction("b.mi __rt_ssc_neg");                                   // the left value is smaller
    emitter.instruction("b.eq __rt_ssc_zero");                                  // the values are equal
    emitter.instruction("b __rt_ssc_pos");                                      // the left value is larger, or a NaN is involved

    emitter.label("__rt_ssc_bytes");
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload the left string pointer and length
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload the right string pointer and length
    emitter.instruction("bl __rt_strcmp");                                      // compare both strings byte-wise, then by length
    emitter.instruction("cmp x0, #0");                                          // normalize the byte difference into a three-way result
    emitter.instruction("b.lt __rt_ssc_neg");                                   // the left string sorts first
    emitter.instruction("b.gt __rt_ssc_pos");                                   // the right string sorts first

    emitter.label("__rt_ssc_zero");
    emitter.instruction("mov x0, #0");                                          // the operands compare equal
    emitter.instruction("b __rt_ssc_done");                                     // fall through to the epilogue
    emitter.label("__rt_ssc_neg");
    emitter.instruction("mov x0, #-1");                                         // the left operand sorts first
    emitter.instruction("b __rt_ssc_done");                                     // fall through to the epilogue
    emitter.label("__rt_ssc_pos");
    emitter.instruction("mov x0, #1");                                          // the right operand sorts first

    emitter.label("__rt_ssc_done");
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the helper stack frame
    emitter.instruction("ret");                                                 // return the three-way comparison result
}

/// Emits the x86_64 Linux implementation of `__rt_str_smart_cmp`.
///
/// Identical decision tree to the ARM64 path, using the System V argument registers for the
/// two operands and `ucomisd` for the double comparisons.
fn emit_str_smart_cmp_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_smart_cmp (zendi_smart_strcmp) ---");
    emitter.label_global("__rt_str_smart_cmp");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before nested calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 112");                                        // allocate aligned slots for both operands and classifications
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the left string length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the right string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the right string length

    emitter.instruction("mov rax, rdi");                                        // move the left string pointer into the classifier input register
    emitter.instruction("mov rdx, rsi");                                        // move the left string length into the classifier input register
    abi::emit_call_label(emitter, "__rt_str_numeric_ex");                       // classify the left string
    emitter.instruction("test rax, rax");                                       // did the left string parse as fully numeric?
    emitter.instruction("je __rt_ssc_bytes_linux_x86_64");                      // a non-numeric operand forces the byte comparison
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the left kind
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the left overflow marker
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // save the left exact integer value
    emitter.instruction("movsd QWORD PTR [rbp - 64], xmm0");                    // save the left double value

    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // load the right string pointer into the classifier input register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // load the right string length into the classifier input register
    abi::emit_call_label(emitter, "__rt_str_numeric_ex");                       // classify the right string
    emitter.instruction("test rax, rax");                                       // did the right string parse as fully numeric?
    emitter.instruction("je __rt_ssc_bytes_linux_x86_64");                      // a non-numeric operand forces the byte comparison
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // save the right kind
    emitter.instruction("mov QWORD PTR [rbp - 80], rcx");                       // save the right overflow marker
    emitter.instruction("mov QWORD PTR [rbp - 88], r8");                        // save the right exact integer value
    emitter.instruction("movsd QWORD PTR [rbp - 96], xmm0");                    // save the right double value

    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the left overflow marker
    emitter.instruction("test r9, r9");                                         // did the left integer overflow zend_long?
    emitter.instruction("je __rt_ssc_kinds_linux_x86_64");                      // no overflow: the doubles are trustworthy
    emitter.instruction("cmp r9, QWORD PTR [rbp - 80]");                        // did both overflow to the same side?
    emitter.instruction("jne __rt_ssc_kinds_linux_x86_64");                     // opposite sides are ordered by sign below
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 64]");                    // reload the left double value
    emitter.instruction("subsd xmm1, QWORD PTR [rbp - 96]");                    // PHP tests dval1 - dval2, so two infinities give NaN
    emitter.instruction("pxor xmm2, xmm2");                                     // compare the difference against zero
    emitter.instruction("ucomisd xmm1, xmm2");                                  // is the difference exactly zero?
    emitter.instruction("jp __rt_ssc_kinds_linux_x86_64");                      // NaN is not equal to zero, which is the point of the subtraction
    emitter.instruction("je __rt_ssc_bytes_linux_x86_64");                      // the doubles lost the difference: compare the spellings

    emitter.label("__rt_ssc_kinds_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the left kind
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the right kind
    emitter.instruction("cmp r9, 1");                                           // is the left operand IS_LONG?
    emitter.instruction("jne __rt_ssc_left_double_linux_x86_64");               // the left operand is a double
    emitter.instruction("cmp r10, 1");                                          // is the right operand IS_LONG too?
    emitter.instruction("jne __rt_ssc_long_vs_double_linux_x86_64");            // only the left one is an integer
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the left exact integer value
    emitter.instruction("cmp r11, QWORD PTR [rbp - 88]");                       // two integers compare exactly, never through a double
    emitter.instruction("jl __rt_ssc_neg_linux_x86_64");                        // the left integer is smaller
    emitter.instruction("jg __rt_ssc_pos_linux_x86_64");                        // the left integer is larger
    emitter.instruction("jmp __rt_ssc_zero_linux_x86_64");                      // the integers are equal

    emitter.label("__rt_ssc_long_vs_double_linux_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the right overflow marker
    emitter.instruction("test r10, r10");                                       // is the right operand an overflowed integer?
    emitter.instruction("je __rt_ssc_widen_left_linux_x86_64");                 // an ordinary float: widen the integer and compare
    emitter.instruction("cmp r10, 0");                                          // the right operand is an integer beyond zend_long
    emitter.instruction("jg __rt_ssc_neg_linux_x86_64");                        // it overflowed upwards, so it is larger
    emitter.instruction("jmp __rt_ssc_pos_linux_x86_64");                       // it overflowed downwards, so it is smaller
    emitter.label("__rt_ssc_widen_left_linux_x86_64");
    emitter.instruction("cvtsi2sd xmm1, QWORD PTR [rbp - 56]");                 // PHP widens the IS_LONG side into dval
    emitter.instruction("movsd xmm2, QWORD PTR [rbp - 96]");                    // reload the right double value
    emitter.instruction("jmp __rt_ssc_doubles_linux_x86_64");                   // compare as doubles

    emitter.label("__rt_ssc_left_double_linux_x86_64");
    emitter.instruction("cmp r10, 1");                                          // is the right operand IS_LONG?
    emitter.instruction("jne __rt_ssc_both_double_linux_x86_64");               // both operands are doubles
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the left overflow marker
    emitter.instruction("test r9, r9");                                         // is the left operand an overflowed integer?
    emitter.instruction("je __rt_ssc_widen_right_linux_x86_64");                // an ordinary float: widen the integer and compare
    emitter.instruction("cmp r9, 0");                                           // the left operand is an integer beyond zend_long
    emitter.instruction("jg __rt_ssc_pos_linux_x86_64");                        // it overflowed upwards, so it is larger
    emitter.instruction("jmp __rt_ssc_neg_linux_x86_64");                       // it overflowed downwards, so it is smaller
    emitter.label("__rt_ssc_widen_right_linux_x86_64");
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 64]");                    // reload the left double value
    emitter.instruction("cvtsi2sd xmm2, QWORD PTR [rbp - 88]");                 // PHP widens the IS_LONG side into dval
    emitter.instruction("jmp __rt_ssc_doubles_linux_x86_64");                   // compare as doubles

    emitter.label("__rt_ssc_both_double_linux_x86_64");
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 64]");                    // reload the left double value
    emitter.instruction("movsd xmm2, QWORD PTR [rbp - 96]");                    // reload the right double value
    emitter.instruction("ucomisd xmm1, xmm2");                                  // are the two doubles equal?
    emitter.instruction("jp __rt_ssc_doubles_linux_x86_64");                    // a NaN operand orders normally
    emitter.instruction("jne __rt_ssc_doubles_linux_x86_64");                   // different values order normally
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reinterpret the shared value as bits
    emitter.instruction("mov r10, 1");                                          // build the sign mask without a 64-bit immediate
    emitter.instruction("shl r10, 63");                                         // 2^63 is the sign bit
    emitter.instruction("dec r10");                                             // 0x7fffffffffffffff drops the sign
    emitter.instruction("and r9, r10");                                         // keep only the magnitude bits
    emitter.instruction("mov r10, 2047");                                       // the all-ones exponent field
    emitter.instruction("shl r10, 52");                                         // the exponent pattern of infinity
    emitter.instruction("cmp r9, r10");                                         // equal and non-finite means two infinities
    emitter.instruction("jae __rt_ssc_bytes_linux_x86_64");                     // PHP compares the spellings rather than call them equal

    emitter.label("__rt_ssc_doubles_linux_x86_64");
    emitter.instruction("ucomisd xmm1, xmm2");                                  // ZEND_THREEWAY_COMPARE on the two doubles
    emitter.instruction("jp __rt_ssc_pos_linux_x86_64");                        // a NaN pair compares greater
    emitter.instruction("jb __rt_ssc_neg_linux_x86_64");                        // the left value is smaller
    emitter.instruction("je __rt_ssc_zero_linux_x86_64");                       // the values are equal
    emitter.instruction("jmp __rt_ssc_pos_linux_x86_64");                       // the left value is larger

    emitter.label("__rt_ssc_bytes_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the left string length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the right string pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the right string length
    abi::emit_call_label(emitter, "__rt_strcmp");                               // compare both strings byte-wise, then by length
    emitter.instruction("test rax, rax");                                       // normalize the byte difference into a three-way result
    emitter.instruction("jl __rt_ssc_neg_linux_x86_64");                        // the left string sorts first
    emitter.instruction("jg __rt_ssc_pos_linux_x86_64");                        // the right string sorts first

    emitter.label("__rt_ssc_zero_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // the operands compare equal
    emitter.instruction("jmp __rt_ssc_done_linux_x86_64");                      // fall through to the epilogue
    emitter.label("__rt_ssc_neg_linux_x86_64");
    emitter.instruction("mov rax, -1");                                         // the left operand sorts first
    emitter.instruction("jmp __rt_ssc_done_linux_x86_64");                      // fall through to the epilogue
    emitter.label("__rt_ssc_pos_linux_x86_64");
    emitter.instruction("mov rax, 1");                                          // the right operand sorts first

    emitter.label("__rt_ssc_done_linux_x86_64");
    emitter.instruction("add rsp, 112");                                        // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the three-way comparison result
}
