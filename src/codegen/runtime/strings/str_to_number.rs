//! Purpose:
//! Emits string numeric-detection helpers used by PHP loose comparison.
//! Converts pointer/length PHP strings through libc `strtod` while rejecting trailing junk.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - The helper returns both a numeric flag and the parsed double without losing PHP byte-string bounds.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// str_to_number: parse a PHP string as a PHP-8-style numeric string.
/// Input:  AArch64 x1=ptr, x2=len; x86_64 rax=ptr, rdx=len
/// Output: integer result register = 1 when numeric, 0 otherwise; d0/xmm0 = parsed number
pub fn emit_str_to_number(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_to_number_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_to_number ---");
    emitter.label_global("__rt_str_to_number");

    emitter.instruction("sub sp, sp, #32");                                     // allocate helper slots for the C string start and strtod end pointer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a stable helper frame pointer
    emitter.instruction("bl __rt_cstr");                                        // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("str x0, [sp, #0]");                                    // save the C-string start pointer for the no-consumption check
    emitter.instruction("add x1, sp, #8");                                      // pass the address of the local end-pointer slot to strtod
    emitter.bl_c("strtod");
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the end pointer returned by strtod
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the C-string start pointer
    emitter.instruction("cmp x9, x10");                                         // reject strings where strtod consumed no numeric bytes
    emitter.instruction("b.eq __rt_str_to_number_false");                       // no consumed bytes means this is not a numeric string

    emitter.label("__rt_str_to_number_trailing_loop");
    emitter.instruction("ldrb w11, [x9], #1");                                  // load the next trailing byte and advance the scan cursor
    emitter.instruction("cbz w11, __rt_str_to_number_true");                    // end of C string means all trailing bytes were acceptable
    emitter.instruction("cmp w11, #32");                                        // ASCII space is allowed after the numeric payload
    emitter.instruction("b.eq __rt_str_to_number_trailing_loop");               // keep scanning after an allowed space
    emitter.instruction("sub w12, w11, #9");                                    // normalize ASCII tab/newline/form-feed/carriage-return range
    emitter.instruction("cmp w12, #4");                                         // values 9 through 13 are accepted trailing whitespace
    emitter.instruction("b.ls __rt_str_to_number_trailing_loop");               // keep scanning after accepted control whitespace

    emitter.label("__rt_str_to_number_false");
    emitter.instruction("mov x0, #0");                                          // report that the string is not numeric
    emitter.instruction("b __rt_str_to_number_done");                           // restore the helper frame and return

    emitter.label("__rt_str_to_number_true");
    emitter.instruction("mov x0, #1");                                          // report that the string parsed as a complete numeric string

    emitter.label("__rt_str_to_number_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the numeric flag while preserving the parsed double in d0
}

fn emit_str_to_number_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_to_number ---");
    emitter.label_global("__rt_str_to_number");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before nested libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // allocate aligned helper slots for start and end pointers
    emitter.instruction("call __rt_cstr");                                      // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the C-string start pointer for the no-consumption check
    emitter.instruction("lea rsi, [rbp - 16]");                                 // pass the address of the local end-pointer slot to strtod
    emitter.instruction("mov rdi, rax");                                        // pass the C-string start pointer as strtod's first argument
    emitter.instruction("call strtod");                                         // parse the C string as a double through libc
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // load the end pointer returned by strtod
    emitter.instruction("cmp r8, QWORD PTR [rbp - 8]");                         // reject strings where strtod consumed no numeric bytes
    emitter.instruction("je __rt_str_to_number_false_linux_x86_64");            // no consumed bytes means this is not a numeric string

    emitter.label("__rt_str_to_number_trailing_loop_linux_x86_64");
    emitter.instruction("movzx r9d, BYTE PTR [r8]");                            // load the next trailing byte without sign extension
    emitter.instruction("add r8, 1");                                           // advance the trailing-byte scan cursor
    emitter.instruction("test r9d, r9d");                                       // check whether the scan reached the C-string terminator
    emitter.instruction("je __rt_str_to_number_true_linux_x86_64");             // end of C string means all trailing bytes were acceptable
    emitter.instruction("cmp r9d, 32");                                         // ASCII space is allowed after the numeric payload
    emitter.instruction("je __rt_str_to_number_trailing_loop_linux_x86_64");    // keep scanning after an allowed space
    emitter.instruction("sub r9d, 9");                                          // normalize ASCII tab/newline/form-feed/carriage-return range
    emitter.instruction("cmp r9d, 4");                                          // values 9 through 13 are accepted trailing whitespace
    emitter.instruction("jbe __rt_str_to_number_trailing_loop_linux_x86_64");   // keep scanning after accepted control whitespace

    emitter.label("__rt_str_to_number_false_linux_x86_64");
    emitter.instruction("xor rax, rax");                                        // report that the string is not numeric
    emitter.instruction("jmp __rt_str_to_number_done_linux_x86_64");            // restore the helper frame and return

    emitter.label("__rt_str_to_number_true_linux_x86_64");
    emitter.instruction("mov rax, 1");                                          // report that the string parsed as a complete numeric string

    emitter.label("__rt_str_to_number_done_linux_x86_64");
    emitter.instruction("add rsp, 32");                                         // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the numeric flag while preserving the parsed double in xmm0
}
