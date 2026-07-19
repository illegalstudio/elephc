//! Purpose:
//! Emits the `__rt_inet6_pton` runtime helper, which converts a textual
//! IPv6 literal (e.g. `::1`, `2001:db8::1`) into a 16-byte network-order
//! address through libc `inet_pton(AF_INET6, ...)`. Wraps the libc call so
//! the assembly callers don't have to thread NUL termination, AF_INET6's
//! macOS/Linux value divergence, or the libc result conversion.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `tcp://[ipv6]:port` / `udp://[ipv6]:port` dispatch in the stream
//!   socket helpers; on failure they fall back to the IPv4 / hostname path.
//!
//! Key details:
//! - Returns `1` when libc reports the literal parsed cleanly and `0`
//!   otherwise (libc returns `0` for malformed input and `-1` for the
//!   unsupported-family case; both collapse to "fail" for the caller).
//! - The 16-byte `sin6_addr` is written to the buffer pointed to by the
//!   caller's third argument. The caller is responsible for the buffer's
//!   alignment and lifetime.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// inet6_pton: parse an IPv6 literal into a 16-byte address buffer.
/// Input:  AArch64 x0 = host pointer, x1 = host length, x2 = out buffer (16 bytes)
///         x86_64  rdi = host pointer, rsi = host length, rdx = out buffer (16 bytes)
/// Output: 1 on success, 0 on failure
pub fn emit_inet6_pton(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_inet6_pton_linux_x86_64(emitter);
        return;
    }

    let af_inet6 = emitter.platform.af_inet6();
    emitter.blank();
    emitter.comment("--- runtime: inet6_pton ---");
    emitter.label_global("__rt_inet6_pton");

    // Frame (32 bytes): [0..16) saved x29 / x30, [16) saved out buffer pointer.
    emitter.instruction("sub sp, sp, #32");                                     // frame for saved regs and the out-buffer pointer
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x2, [sp, #16]");                                   // stash the caller's out-buffer pointer across libc calls

    // -- null-terminate the host slice for the libc parser --
    emitter.instruction("mov x2, x1");                                          // host length into __rt_cstr's length register
    emitter.instruction("mov x1, x0");                                          // host pointer into __rt_cstr's pointer register
    emitter.instruction("bl __rt_cstr");                                        // x0 = null-terminated host literal

    // -- inet_pton(AF_INET6, c_str, out_buf) --
    emitter.instruction("mov x1, x0");                                          // c_str into argument 1 (src)
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the out buffer pointer into argument 2 (dst)
    emitter.instruction(&format!("mov x0, #{}", af_inet6));                     // family: AF_INET6 (30 on macOS, 10 on Linux)
    emitter.bl_c("inet_pton");                                                  // x0 = 1 success, 0 fail, -1 EAFNOSUPPORT

    // -- collapse libc result to 0/1 (any non-positive return means fail) --
    emitter.instruction("cmp x0, #1");                                          // did libc report exactly one successful conversion?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 on success, 0 otherwise
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the success flag
}

/// Emits the Linux x86_64 stream runtime helper for inet6 pton.
fn emit_inet6_pton_linux_x86_64(emitter: &mut Emitter) {
    let af_inet6 = emitter.platform.af_inet6();
    emitter.blank();
    emitter.comment("--- runtime: inet6_pton ---");
    emitter.label_global("__rt_inet6_pton");

    // Frame (16 bytes, rbp-relative): [-8) saved out buffer pointer.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve the scratch slot
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // stash the caller's out-buffer pointer across libc calls

    // -- null-terminate the host slice for the libc parser --
    emitter.instruction("mov rax, rdi");                                        // host pointer into __rt_cstr's pointer register
    emitter.instruction("mov rdx, rsi");                                        // host length into __rt_cstr's length register
    emitter.instruction("call __rt_cstr");                                      // rax = null-terminated host literal

    // -- inet_pton(AF_INET6, c_str, out_buf) --
    emitter.instruction("mov rsi, rax");                                        // c_str into argument 1 (src)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the out buffer pointer into argument 2 (dst)
    emitter.instruction(&format!("mov edi, {}", af_inet6));                     // family: AF_INET6 (30 on macOS, 10 on Linux)
    emitter.instruction("call inet_pton");                                      // rax = 1 success, 0 fail, -1 EAFNOSUPPORT

    // -- collapse libc result to 0/1 (any non-positive return means fail) --
    emitter.instruction("cmp eax, 1");                                          // did libc report exactly one successful conversion?
    emitter.instruction("sete al");                                             // al = 1 on success, 0 otherwise
    emitter.instruction("movzx eax, al");                                       // widen the success flag to a full word
    emitter.instruction("add rsp, 16");                                         // release the scratch slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the success flag
}
