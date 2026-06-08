//! Purpose:
//! Emits the `__rt_str_to_int` runtime helper for PHP string-to-int casts.
//! Parses the bounded PHP string as both an integer (libc `strtoll`) and a double (libc `strtod`)
//! and returns the integer-form value unless the string is actually float-form.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - `strtoll` gives the exact 64-bit value and PHP's saturating overflow (LLONG_MAX/MIN == PHP_INT_MAX/MIN),
//!   so large integer strings are not rounded through `f64`.
//! - When `strtod` consumes more bytes than `strtoll`, the string has a `.`/`e` float part (e.g. `"1e3"`),
//!   so the truncated double is returned, matching PHP's leading-numeric-string rules.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_str_to_int` for PHP string-to-int conversion.
///
/// Input follows the active string-result convention:
/// AArch64 uses `x1`/`x2`; x86_64 uses `rax`/`rdx`.
/// The helper copies the string into the C-string scratch buffer via `__rt_cstr`, then parses it
/// with `strtoll` (exact + saturating) and `strtod`, returning the integer-form value unless the
/// string is float-form, in which case the truncated double is returned.
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

    // -- copy the PHP string into the C-string scratch buffer --
    emitter.instruction("bl __rt_cstr");                                        // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("str x0, [sp, #24]");                                   // save the C-string pointer for the second parse

    // -- integer parse: strtoll(cstr, &end_i, 10) gives the exact, saturating 64-bit value --
    emitter.instruction("add x1, sp, #0");                                      // pass &end_i so strtoll reports where the integer prefix ended
    emitter.instruction("mov x2, #10");                                         // parse in base 10 like PHP string-to-int
    emitter.bl_c("strtoll");
    emitter.instruction("str x0, [sp, #16]");                                   // save the integer-form value (LLONG_MAX/MIN on overflow == PHP_INT_MAX/MIN)

    // -- float parse: strtod(cstr, &end_d) detects a '.'/'e' float continuation --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the C-string pointer for strtod
    emitter.instruction("add x1, sp, #8");                                      // pass &end_d so strtod reports where the numeric value ended
    emitter.bl_c("strtod");

    // -- choose the integer value unless strtod consumed more bytes (a float part) --
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the end pointer returned by strtod
    emitter.instruction("ldr x10, [sp, #0]");                                   // load the end pointer returned by strtoll
    emitter.instruction("cmp x9, x10");                                         // did strtod consume more bytes than strtoll?
    emitter.instruction("b.hi __rt_str_to_int_float");                          // yes: the string is float-form, return the truncated double
    emitter.instruction("ldr x0, [sp, #16]");                                   // no: return the exact integer-form value
    emitter.instruction("b __rt_str_to_int_done");                              // skip the float-truncation path

    emitter.label("__rt_str_to_int_float");
    abi::emit_float_result_to_int_result(emitter);                              // truncate the parsed double in d0 toward zero for PHP float-string casts

    emitter.label("__rt_str_to_int_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the integer cast result
}

/// Emits the Linux x86_64 `__rt_str_to_int` runtime helper.
///
/// The input string arrives in the elephc string-result registers (`rax`/`rdx`).
/// Parses with `strtoll` (exact + saturating) and `strtod`, returning the integer-form value in
/// `rax` unless `strtod` consumed a `.`/`e` float part, in which case the truncated double is used.
fn emit_str_to_int_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_to_int ---");
    emitter.label_global("__rt_str_to_int");

    // -- set up the helper frame (locals: cstr=[rbp-8], ll_val=[rbp-16], end_i=[rbp-24], end_d=[rbp-32]) --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before calling libc parsers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate aligned slots for the C-string pointer, integer value, and end pointers

    // -- copy the PHP string into the C-string scratch buffer --
    emitter.instruction("call __rt_cstr");                                      // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the C-string pointer for the second parse

    // -- integer parse: strtoll(cstr, &end_i, 10) gives the exact, saturating 64-bit value --
    emitter.instruction("mov rdi, rax");                                        // strtoll arg1: the C-string pointer
    emitter.instruction("lea rsi, [rbp - 24]");                                 // strtoll arg2: &end_i
    emitter.instruction("mov edx, 10");                                         // strtoll arg3: parse in base 10 like PHP string-to-int
    emitter.instruction("call strtoll");                                        // rax = integer-form value (LLONG_MAX/MIN on overflow == PHP_INT_MAX/MIN)
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the integer-form value

    // -- float parse: strtod(cstr, &end_d) detects a '.'/'e' float continuation --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the C-string pointer for strtod
    emitter.instruction("lea rsi, [rbp - 32]");                                 // strtod arg2: &end_d
    emitter.instruction("call strtod");                                         // xmm0 = parsed double value

    // -- choose the integer value unless strtod consumed more bytes (a float part) --
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // load the end pointer returned by strtod
    emitter.instruction("cmp r8, QWORD PTR [rbp - 24]");                        // did strtod consume more bytes than strtoll?
    emitter.instruction("ja __rt_str_to_int_float_linux_x86_64");               // yes: the string is float-form, return the truncated double
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // no: return the exact integer-form value
    emitter.instruction("jmp __rt_str_to_int_done_linux_x86_64");               // skip the float-truncation path

    emitter.label("__rt_str_to_int_float_linux_x86_64");
    abi::emit_float_result_to_int_result(emitter);                              // truncate the parsed double in xmm0 toward zero for PHP float-string casts

    emitter.label("__rt_str_to_int_done_linux_x86_64");
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the integer cast result
}
