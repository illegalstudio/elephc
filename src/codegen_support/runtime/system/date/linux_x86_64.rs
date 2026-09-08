//! Purpose:
//! Emits the Linux x86_64 `__rt_date` and `__rt_gmdate` wrappers over the
//! vendored php-src timelib bridge.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system::date::emit_date()`.
//!
//! Key details:
//! - Every signed timestamp uses timelib so historical zones and expanded years
//!   cannot diverge through a platform-libc fast path.
//! - The seventh SysV bridge argument is materialized in the outgoing stack slot.

use crate::codegen_support::emit::Emitter;

/// Emits the target ABI wrappers for procedural `date()` and `gmdate()`.
///
/// The caller supplies the timestamp in `rax`, format pointer/length in
/// `rdi`/`rsi`, and a timestamp-present flag in `rcx`. The helper returns the
/// bridge string pointer in `rax` and its byte length in `rdx`.
pub(super) fn emit_date_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: date / gmdate via php-src timelib ---");

    emitter.label_global("__rt_gmdate");
    emitter.instruction("mov r10d, 1");                                        // select UTC formatting
    emitter.instruction("jmp __rt_date_entry_linux_x86_64");                   // share the timelib wrapper
    emitter.label_global("__rt_date");
    emitter.instruction("xor r10d, r10d");                                    // select the active PHP timezone
    emitter.label("__rt_date_entry_linux_x86_64");

    emitter.instruction("push rbp");                                           // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                       // establish stable local slots
    emitter.instruction("sub rsp, 64");                                        // reserve aligned locals plus the outgoing seventh argument
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                       // save the requested timestamp
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                      // save the format pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                      // save the format byte length
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                      // save the date-vs-gmdate selector
    emitter.instruction("test rcx, rcx");                                      // was a concrete timestamp supplied?
    emitter.instruction("jne __rt_date_have_time_linux_x86_64");               // preserve explicit timestamps, including -1
    emitter.instruction("xor edi, edi");                                       // time(NULL)
    emitter.bl_c("time");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                       // retain the current timestamp

    emitter.label("__rt_date_have_time_linux_x86_64");
    emitter.instruction("call __rt_tz_init_utc");                              // initialize the default PHP timezone once
    emitter.instruction("call __rt_date_default_timezone_get");                // rax/rdx = timezone pointer/length
    emitter.instruction("mov r8, rax");                                        // bridge arg 5 = timezone pointer
    emitter.instruction("mov r9, rdx");                                        // bridge arg 6 = timezone byte length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                       // bridge arg 1 = signed timestamp
    emitter.instruction("xor esi, esi");                                       // bridge arg 2 = procedural microseconds
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                      // bridge arg 3 = format pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                      // bridge arg 4 = format byte length
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                      // reload the UTC selector
    emitter.instruction("xor rax, 1");                                         // bridge arg 7: date=local, gmdate=UTC
    emitter.instruction("mov QWORD PTR [rsp], rax");                           // place arg 7 in the SysV outgoing stack slot
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                        // initialize the explicit output-length slot
    emitter.instruction("lea rax, [rbp - 40]");                                // bridge arg 8 = output-length address
    emitter.instruction("mov QWORD PTR [rsp + 8], rax");                       // place arg 8 beside the seventh outgoing slot
    emitter.bl_c("elephc_tz_format");                                         // rax = raw date bytes, including embedded NUL
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                      // return the bridge-provided byte length
    emitter.instruction("leave");                                              // restore the caller frame and stack
    emitter.instruction("ret");                                                // return the formatted string pair
}
