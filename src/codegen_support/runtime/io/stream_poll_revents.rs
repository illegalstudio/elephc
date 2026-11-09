//! Purpose:
//! Emits `__rt_stream_poll_revents`, a zero-timeout `poll(2)` probe returning the kernel
//! readiness/error/hang-up flags for one descriptor.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::platform` during the I/O runtime section.
//! - The internal `__elephc_poll_revents()` prelude helper used by `Io\Poll`.
//!
//! Key details:
//! - `events` requests `POLLIN` (plus `POLLRDHUP` on Linux), because macOS reports
//!   `POLLHUP` only when an event is requested and Linux needs `POLLRDHUP` for a TCP
//!   half-close. Ordinary readiness still comes from `stream_select()`.
//! - `poll()` never blocks here because the timeout is zero, so the helper is safe on the
//!   event-loop path.
//! - Both targets keep the 8-byte `struct pollfd { int fd; short events; short revents; }`
//!   on their own aligned frame.

use crate::codegen_support::{
    emit::Emitter,
    platform::{Arch, Platform},
};

/// Emits `__rt_stream_poll_revents`.
///
/// # Input
/// - `x0`/`rdi`: the native stream descriptor.
///
/// # Output
/// - `x0`/`rax`: the `poll(2)` `revents` bitmask (`POLLIN=1`, `POLLPRI=2`, `POLLOUT=4`,
///   `POLLERR=8`, `POLLHUP=16`, `POLLNVAL=32`).
pub(crate) fn emit_stream_poll_revents(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 probe.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_poll_revents ---");
    emitter.label_global("__rt_stream_poll_revents");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the pollfd slot and the saved frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame and continuation across poll()
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("str w0, [sp, #0]");                                    // pollfd.fd = the stream descriptor
    let events = if emitter.target.platform == Platform::Linux { 0x2001 } else { 0x1 };
    emitter.instruction(&format!("mov w9, #{}", events));                       // POLLIN (+ POLLRDHUP on Linux) also reports HUP
    emitter.instruction("strh w9, [sp, #4]");                                   // pollfd.events = POLLIN/POLLRDHUP
    emitter.instruction("strh wzr, [sp, #6]");                                  // pollfd.revents = 0
    emitter.instruction("mov x0, sp");                                          // pollfds pointer
    emitter.instruction("mov x1, #1");                                          // one descriptor
    emitter.instruction("mov x2, #0");                                          // zero timeout: never block
    emitter.bl_c("poll");                                                       // ask the kernel for this descriptor's flags
    emitter.instruction("ldrh w0, [sp, #6]");                                   // return the poll(2) revents mask
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and continuation
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the x86_64 probe.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_poll_revents ---");
    emitter.label_global("__rt_stream_poll_revents");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 16");                                         // reserve one aligned pollfd slot
    emitter.instruction("mov DWORD PTR [rbp - 8], edi");                        // pollfd.fd = the stream descriptor
    let events = if emitter.target.platform == Platform::Linux { 0x2001 } else { 0x1 };
    emitter.instruction(&format!("mov WORD PTR [rbp - 4], {}", events));        // POLLIN (+ POLLRDHUP on Linux) also reports HUP
    emitter.instruction("mov WORD PTR [rbp - 2], 0");                           // pollfd.revents = 0
    emitter.instruction("lea rdi, [rbp - 8]");                                  // pollfds pointer
    emitter.instruction("mov esi, 1");                                          // one descriptor
    emitter.instruction("xor edx, edx");                                        // zero timeout: never block
    emitter.instruction("call poll");                                           // ask the kernel for this descriptor's flags
    emitter.instruction("movzx eax, WORD PTR [rbp - 2]");                       // return the poll(2) revents mask
    emitter.instruction("add rsp, 16");                                         // release the pollfd slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
