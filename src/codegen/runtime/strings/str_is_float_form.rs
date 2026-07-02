//! Purpose:
//! Emits the `__rt_str_is_float_form` runtime helper that reports whether a PHP
//! string is float-form (contains a `.` or exponent that `strtod` consumes beyond
//! what `strtoll` parses). Used by the mixed numeric dispatch to route string
//! operands to the correct arithmetic path.
//!
//! Called from:
//! - `crate::codegen::runtime::arrays::mixed_numeric_binops` during operand classification.
//!
//! Key details:
//! - Returns 1 in the integer result register when the string is float-form, 0 otherwise.
//! - The input string follows the active string-result convention (AArch64 x1/x2, x86_64 rax/rdx).

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_str_is_float_form` for both supported targets.
///
/// Input follows the active string-result convention:
/// AArch64 uses `x1`/`x2`; x86_64 uses `rax`/`rdx`.
/// Output: integer result register holds 1 (float-form) or 0 (int-form).
pub fn emit_str_is_float_form(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_is_float_form_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_is_float_form ---");
    emitter.label_global("__rt_str_is_float_form");

    // -- set up the helper frame (slots: end_i=[sp,#0], end_d=[sp,#8], cstr=[sp,#16]) --
    emitter.instruction("sub sp, sp, #32");                                     // allocate slots for both end pointers and the C-string pointer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across the libc calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable helper frame pointer

    // -- copy the PHP string into the C-string scratch buffer --
    emitter.instruction("bl __rt_cstr");                                        // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("str x0, [sp, #16]");                                   // save the C-string pointer for the second parse

    // -- integer parse: strtoll(cstr, &end_i, 10) reports where the integer prefix ends --
    emitter.instruction("add x1, sp, #0");                                      // pass &end_i so strtoll reports where the integer prefix ended
    emitter.instruction("mov x2, #10");                                         // parse in base 10 like PHP string-to-int
    emitter.bl_c("strtoll");

    // -- float parse: strtod(cstr, &end_d) reports where the numeric value ended --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the C-string pointer for strtod
    emitter.instruction("add x1, sp, #8");                                      // pass &end_d so strtod reports where the numeric value ended
    emitter.bl_c("strtod");

    // -- if strtod consumed more bytes than strtoll, the string is float-form --
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the end pointer returned by strtod
    emitter.instruction("ldr x10, [sp, #0]");                                   // load the end pointer returned by strtoll
    emitter.instruction("cmp x9, x10");                                         // did strtod consume more bytes than strtoll?
    emitter.instruction("b.hi __rt_str_is_float_form_true");                    // yes: the string is float-form
    emitter.instruction("mov x0, #0");                                          // no: the string is int-form
    emitter.instruction("b __rt_str_is_float_form_done");                       // skip the true path

    emitter.label("__rt_str_is_float_form_true");
    emitter.instruction("mov x0, #1");                                          // report that the string is float-form

    emitter.label("__rt_str_is_float_form_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore caller frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the float-form flag in x0
}

/// Emits the Linux x86_64 `__rt_str_is_float_form` runtime helper.
///
/// The input string arrives in the elephc string-result registers (`rax`/`rdx`).
/// Parses with `strtoll` and `strtod`, returning 1 in `rax` if `strtod` consumed
/// more bytes (float-form), 0 otherwise.
fn emit_str_is_float_form_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_is_float_form ---");
    emitter.label_global("__rt_str_is_float_form");

    // -- set up the helper frame (locals: cstr=[rbp-8], end_i=[rbp-16], end_d=[rbp-24]) --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before calling libc parsers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // allocate aligned slots for the C-string pointer and end pointers

    // -- copy the PHP string into the C-string scratch buffer --
    emitter.instruction("call __rt_cstr");                                      // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the C-string pointer for the second parse

    // -- integer parse: strtoll(cstr, &end_i, 10) reports where the integer prefix ends --
    emitter.instruction("mov rdi, rax");                                        // strtoll arg1: the C-string pointer
    emitter.instruction("lea rsi, [rbp - 16]");                                 // strtoll arg2: &end_i
    emitter.instruction("mov edx, 10");                                         // strtoll arg3: parse in base 10
    emitter.instruction("call strtoll");                                        // parse the integer prefix

    // -- float parse: strtod(cstr, &end_d) reports where the numeric value ended --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the C-string pointer for strtod
    emitter.instruction("lea rsi, [rbp - 24]");                                 // strtod arg2: &end_d
    emitter.instruction("call strtod");                                         // parse the full numeric value

    // -- if strtod consumed more bytes than strtoll, the string is float-form --
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // load the end pointer returned by strtod
    emitter.instruction("cmp r8, QWORD PTR [rbp - 16]");                        // did strtod consume more bytes than strtoll?
    emitter.instruction("ja __rt_str_is_float_form_true_linux_x86_64");         // yes: the string is float-form
    emitter.instruction("xor rax, rax");                                        // no: the string is int-form
    emitter.instruction("jmp __rt_str_is_float_form_done_linux_x86_64");        // skip the true path

    emitter.label("__rt_str_is_float_form_true_linux_x86_64");
    emitter.instruction("mov rax, 1");                                          // report that the string is float-form

    emitter.label("__rt_str_is_float_form_done_linux_x86_64");
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the float-form flag in rax
}