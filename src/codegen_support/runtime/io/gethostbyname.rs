//! Purpose:
//! Emits the `__rt_gethostbyname` runtime helper for the PHP `gethostbyname`
//! builtin.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Resolution reuses `__rt_resolve_host`; the packed address is rendered with
//!   `__rt_long2ip`. PHP returns the host name unchanged when it cannot be
//!   resolved, so the failure path returns the original argument.
//! - Both the resolved address and the unchanged host name are persisted with
//!   `__rt_str_persist` so the result owns stable heap storage.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// gethostbyname: resolve a host name to its IPv4 dotted-quad string.
/// Input:  AArch64 x1/x2 = host string / x86_64 rax/rdx = host string
/// Output: the resolved address string, or the host name when unresolved.
pub fn emit_gethostbyname(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_gethostbyname_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: gethostbyname ---");
    emitter.label_global("__rt_gethostbyname");

    // Frame (32 bytes): [0]=host ptr [8]=host len [16]=x29 [24]=x30.
    emitter.instruction("sub sp, sp, #32");                                     // frame for the saved host string
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the host string pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the host string length

    // -- resolve the host name to a packed IPv4 address --
    emitter.instruction("mov x0, x1");                                          // host pointer into the resolve_host pointer register
    emitter.instruction("mov x1, x2");                                          // host length into the resolve_host length register
    emitter.instruction("bl __rt_resolve_host");                                // x0 = packed IPv4 integer or -1
    emitter.instruction("cmn x0, #1");                                          // did the host fail to resolve?
    emitter.instruction("b.eq __rt_gethostbyname_fail");                        // return the host name unchanged on failure

    // -- success: render the address and persist it --
    emitter.instruction("bl __rt_long2ip");                                     // x1/x2 = dotted-quad address string
    emitter.instruction("bl __rt_str_persist");                                 // x1/x2 = owned heap copy of the address
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the resolved address string

    emitter.label("__rt_gethostbyname_fail");
    emitter.instruction("ldr x1, [sp, #0]");                                    // PHP returns the host name unchanged on failure
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the saved host length
    emitter.instruction("bl __rt_str_persist");                                 // x1/x2 = owned heap copy of the host name
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the unchanged host name
}

/// Emits the Linux x86_64 stream runtime helper for gethostbyname.
fn emit_gethostbyname_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gethostbyname ---");
    emitter.label_global("__rt_gethostbyname");

    // Frame (rbp-relative): [-8]=host ptr [-16]=host len.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // frame for the saved host string
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the host string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the host string length

    // -- resolve the host name to a packed IPv4 address --
    emitter.instruction("mov rdi, rax");                                        // host pointer into the resolve_host argument
    emitter.instruction("mov rsi, rdx");                                        // host length into the resolve_host argument
    emitter.instruction("call __rt_resolve_host");                              // rax = packed IPv4 integer or -1
    emitter.instruction("cmp rax, -1");                                         // did the host fail to resolve?
    emitter.instruction("je __rt_gethostbyname_fail_x86");                      // return the host name unchanged on failure

    // -- success: render the address and persist it --
    emitter.instruction("mov rdi, rax");                                        // packed IPv4 into the long2ip argument
    emitter.instruction("call __rt_long2ip");                                   // rax/rdx = dotted-quad address string
    emitter.instruction("call __rt_str_persist");                               // rax/rdx = owned heap copy of the address
    emitter.instruction("add rsp, 16");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the resolved address string

    emitter.label("__rt_gethostbyname_fail_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // PHP returns the host name unchanged on failure
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the saved host length
    emitter.instruction("call __rt_str_persist");                               // rax/rdx = owned heap copy of the host name
    emitter.instruction("add rsp, 16");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the unchanged host name
}
