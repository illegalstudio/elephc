//! Purpose:
//! Records why the last socket connect/bind failed, and turns that error number into the
//! message `stream_socket_client()`, `stream_socket_server()`, `fsockopen()`, and
//! `pfsockopen()` hand back through their `&$error_code` / `&$error_message` parameters.
//!
//! Called from:
//! - The socket helpers, which stash the failing syscall's error number.
//! - `crate::codegen::lower_inst::builtins::io`, which reads the stash after a failed call.
//!
//! Key details:
//! - A raw syscall reports its error differently per platform: Linux returns `-errno` in the
//!   result register, macOS returns a positive `errno` and sets the carry flag. The stash always
//!   holds the positive number, so every consumer reads one convention.
//! - The message comes from libc `strerror`, which is where php-src gets it too — so the text
//!   matches PHP's rather than approximating it.

use crate::codegen_support::{emit::Emitter, platform::Arch, platform::Platform};

/// Name of the process-global holding the last socket failure's error number.
pub(crate) const SOCKET_ERRNO_SYMBOL: &str = "_socket_errno";

/// Stashes the failing syscall's error number, normalized to a positive `errno`.
///
/// `result_reg` must still hold the failing syscall's raw result. Call this BEFORE any cleanup
/// syscall — a `close()` on the half-open descriptor overwrites the result register, and the
/// reason for the failure is gone with it.
pub(crate) fn emit_capture_socket_errno(emitter: &mut Emitter, result_reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if emitter.platform == Platform::Linux {
                emitter.instruction(&format!("neg x9, {}", result_reg));            // Linux returns -errno; store the positive number
            } else {
                emitter.instruction(&format!("mov x9, {}", result_reg));            // macOS already returns a positive errno
            }
            crate::codegen_support::abi::emit_symbol_address(emitter, "x10", SOCKET_ERRNO_SYMBOL);
            emitter.instruction("str x9, [x10]");                                   // publish the failure reason for the caller
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r9, {}", result_reg));                // copy the raw syscall result
            if emitter.platform == Platform::Linux {
                emitter.instruction("neg r9");                                      // Linux returns -errno; store the positive number
            }
            crate::codegen_support::abi::emit_symbol_address(emitter, "r10", SOCKET_ERRNO_SYMBOL);
            emitter.instruction("mov QWORD PTR [r10], r9");                         // publish the failure reason for the caller
        }
    }
}

/// Clears the stashed error number.
///
/// Used where the failure is not a syscall's: PHP reports `0` for an address it could not parse,
/// and a stale number from an earlier call would otherwise be read as this call's reason.
pub(crate) fn emit_clear_socket_errno(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            crate::codegen_support::abi::emit_symbol_address(emitter, "x10", SOCKET_ERRNO_SYMBOL);
            emitter.instruction("str xzr, [x10]");                                  // no syscall failed: PHP reports error code 0
        }
        Arch::X86_64 => {
            crate::codegen_support::abi::emit_symbol_address(emitter, "r10", SOCKET_ERRNO_SYMBOL);
            emitter.instruction("mov QWORD PTR [r10], 0");                          // no syscall failed: PHP reports error code 0
        }
    }
}

/// Clears everything the previous socket call left behind, including a composed resolver message.
///
/// Emitted at each socket helper's ENTRY, and only there: the clear that follows a failed address
/// parse must keep the message, because an unresolvable host IS a failed parse — the resolver has
/// already composed it by then, and clearing it there left `&$error_message` empty.
pub(crate) fn emit_reset_socket_error_state(emitter: &mut Emitter) {
    emit_clear_socket_errno(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_socket_gai_msg_len");
            emitter.instruction("str xzr, [x10]");                                  // and no resolution failed either
        }
        Arch::X86_64 => {
            crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_socket_gai_msg_len");
            emitter.instruction("mov QWORD PTR [r10], 0");                          // and no resolution failed either
        }
    }
}

/// Publishes the host and length a resolver was given, so a later failure can name it.
///
/// Emitted at the resolver's entry, where the pair is still in the argument registers, because
/// the failure paths are reached after `getaddrinfo` and `freeaddrinfo` have clobbered them.
pub(crate) fn emit_publish_gai_host(emitter: &mut Emitter, ptr_reg: &str, len_reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_socket_gai_host_ptr");
            emitter.instruction(&format!("str {}, [x9]", ptr_reg));                 // the host the resolver was asked about
            crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_socket_gai_host_len");
            emitter.instruction(&format!("str {}, [x9]", len_reg));                 // and its byte length
        }
        Arch::X86_64 => {
            crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_socket_gai_host_ptr");
            emitter.instruction(&format!("mov QWORD PTR [r10], {}", ptr_reg));      // the host the resolver was asked about
            crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_socket_gai_host_len");
            emitter.instruction(&format!("mov QWORD PTR [r10], {}", len_reg));      // and its byte length
        }
    }
}

/// Publishes the code `getaddrinfo` answered, straight from its result register.
///
/// A successful call publishes `0`, which is exactly the "nothing failed" state the composer
/// checks for, so this is emitted unconditionally rather than only on the failing branch.
pub(crate) fn emit_publish_gai_code(emitter: &mut Emitter, code_reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_socket_gai_err");
            emitter.instruction(&format!("str {}, [x9]", code_reg));                // what the resolver answered
        }
        Arch::X86_64 => {
            crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_socket_gai_err");
            emitter.instruction(&format!("mov QWORD PTR [r10], {}", code_reg));     // what the resolver answered
        }
    }
}

/// Emits `__rt_socket_strerror`: the message for a socket error number.
///
/// Input:  x0 / rdi = error number (0 means "no syscall failed")
/// Output: x0 / rax = message pointer, x1 / rdx = message byte length
///
/// A zero error number answers the empty string, matching PHP, which leaves `$errstr` empty when
/// it has no `errno` to describe. Everything else goes through libc `strerror`, whose result is a
/// static NUL-terminated string measured here because the callers need a pointer/length pair.
pub fn emit_socket_strerror(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_socket_strerror_aarch64(emitter),
        Arch::X86_64 => emit_socket_strerror_x86_64(emitter),
    }
}

/// Emits the aarch64 form of `__rt_socket_strerror`.
fn emit_socket_strerror_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: socket_strerror ---");
    emitter.label_global("__rt_socket_strerror");
    emitter.instruction("sub sp, sp, #16");                                         // frame for the saved link register
    emitter.instruction("stp x29, x30, [sp, #0]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                             // establish the helper frame pointer
    // A failed resolution has no errno at all, so its composed message is checked first.
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_socket_gai_msg_len");
    emitter.instruction("ldr x9, [x9]");                                            // did a resolution fail?
    emitter.instruction("cbnz x9, __rt_socket_strerror_gai");                       // report what php-src composes for it
    emitter.instruction("cmp x0, #0");                                              // no error number to describe?
    emitter.instruction("b.eq __rt_socket_strerror_empty");                         // answer the empty string
    emitter.bl_c("strerror");                                                       // x0 = static NUL-terminated message
    emitter.instruction("cmp x0, #0");                                              // strerror can answer NULL for an unknown code
    emitter.instruction("b.eq __rt_socket_strerror_empty");                         // treat that as no message
    emitter.instruction("mov x9, #0");                                              // measured length
    emitter.label("__rt_socket_strerror_scan");
    emitter.instruction("ldrb w10, [x0, x9]");                                      // load the next message byte
    emitter.instruction("cmp w10, #0");                                             // reached the terminator?
    emitter.instruction("b.eq __rt_socket_strerror_done");                          // the length is complete
    emitter.instruction("add x9, x9, #1");                                          // keep measuring
    emitter.instruction("b __rt_socket_strerror_scan");                             // continue
    emitter.label("__rt_socket_strerror_done");
    emitter.instruction("mov x1, x9");                                              // return the measured byte length
    emitter.instruction("ldp x29, x30, [sp, #0]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                         // release the frame
    emitter.instruction("ret");                                                     // return pointer and length
    emitter.label("__rt_socket_strerror_gai");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x0", "_socket_gai_msg");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_socket_gai_msg_len");
    emitter.instruction("ldr x1, [x1]");                                            // the composed message and its length
    emitter.instruction("ldp x29, x30, [sp, #0]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                         // release the frame
    emitter.instruction("ret");                                                     // return the composed message
    emitter.label("__rt_socket_strerror_empty");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x0", "_empty_str");
    emitter.instruction("mov x1, #0");                                              // an empty message has zero length
    emitter.instruction("ldp x29, x30, [sp, #0]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                         // release the frame
    emitter.instruction("ret");                                                     // return the empty message
}

/// Emits the Linux x86_64 form of `__rt_socket_strerror`.
fn emit_socket_strerror_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: socket_strerror ---");
    emitter.label_global("__rt_socket_strerror");
    emitter.instruction("push rbp");                                                // save the caller's frame pointer
    emitter.instruction("mov rbp, rsp");                                            // establish the helper frame pointer
    // See the AArch64 counterpart: a failed resolution has no errno, so it is checked first.
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_socket_gai_msg_len");
    emitter.instruction("mov r10, QWORD PTR [r10]");                                // did a resolution fail?
    emitter.instruction("test r10, r10");
    emitter.instruction("jnz __rt_socket_strerror_gai_x86");                        // report what php-src composes for it
    emitter.instruction("cmp rdi, 0");                                              // no error number to describe?
    emitter.instruction("je __rt_socket_strerror_empty");                           // answer the empty string
    emitter.bl_c("strerror");                                                       // rax = static NUL-terminated message
    emitter.instruction("cmp rax, 0");                                              // strerror can answer NULL for an unknown code
    emitter.instruction("je __rt_socket_strerror_empty");                           // treat that as no message
    emitter.instruction("xor r9, r9");                                              // measured length
    emitter.label("__rt_socket_strerror_scan");
    emitter.instruction("movzx r10d, BYTE PTR [rax + r9]");                         // load the next message byte
    emitter.instruction("cmp r10b, 0");                                             // reached the terminator?
    emitter.instruction("je __rt_socket_strerror_done");                            // the length is complete
    emitter.instruction("add r9, 1");                                               // keep measuring
    emitter.instruction("jmp __rt_socket_strerror_scan");                           // continue
    emitter.label("__rt_socket_strerror_done");
    emitter.instruction("mov rdx, r9");                                             // return the measured byte length
    emitter.instruction("mov rsp, rbp");                                            // release the frame from rbp
    emitter.instruction("pop rbp");                                                 // restore the caller's frame pointer
    emitter.instruction("ret");                                                     // return pointer and length
    emitter.label("__rt_socket_strerror_gai_x86");
    crate::codegen_support::abi::emit_symbol_address(emitter, "rax", "_socket_gai_msg");
    crate::codegen_support::abi::emit_symbol_address(emitter, "rdx", "_socket_gai_msg_len");
    emitter.instruction("mov rdx, QWORD PTR [rdx]");                                // the composed message and its length
    emitter.instruction("mov rsp, rbp");                                            // release the frame from rbp
    emitter.instruction("pop rbp");                                                 // restore the caller's frame pointer
    emitter.instruction("ret");                                                     // return the composed message
    emitter.label("__rt_socket_strerror_empty");
    crate::codegen_support::abi::emit_symbol_address(emitter, "rax", "_empty_str");
    emitter.instruction("xor edx, edx");                                            // an empty message has zero length
    emitter.instruction("mov rsp, rbp");                                            // release the frame from rbp
    emitter.instruction("pop rbp");                                                 // restore the caller's frame pointer
    emitter.instruction("ret");                                                     // return the empty message
}
