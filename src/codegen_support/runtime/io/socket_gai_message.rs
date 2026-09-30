//! Purpose:
//! Emits `__rt_gai_publish`, which composes the message PHP reports when a socket address names a
//! host that does not resolve.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The IPv4 and IPv6 host resolvers, on the branch where `getaddrinfo` failed.
//!
//! Key details:
//! - php-src does not report an `errno` for this failure — `&$error_code` stays `0` — and instead
//!   composes `php_network_getaddresses: getaddrinfo for <host> failed: <reason>`, where the
//!   reason is the platform's own `gai_strerror` text. elephc left `&$error_message` empty, so the
//!   only PHP-visible sign of an unresolvable host was `false`.
//! - The message is composed into a fixed buffer rather than allocated: the caller's
//!   `&$error_message` receives the pointer by raw store, exactly as it receives the static
//!   `strerror` pointer for every other failure, so an allocation there would have no owner. A
//!   second failed resolution overwrites the buffer.
//! - Composition happens at the failure, not when the message is read, because the host pointer is
//!   borrowed from the caller's address string and only the resolver knows it is still live.

use super::socket_errno::SOCKET_ERRNO_SYMBOL;
use crate::codegen_support::runtime::data::{
    GAI_MSG_MIDDLE, GAI_MSG_PREFIX, SOCKET_GAI_HOST_CLAMP, SOCKET_GAI_REASON_CLAMP,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_gai_publish()`, which composes the unresolvable-host message from the globals the
/// resolver published, or does nothing when the resolver succeeded.
pub fn emit_gai_publish(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_gai_publish_aarch64(emitter),
        Arch::X86_64 => emit_gai_publish_x86_64(emitter),
    }
    let _ = SOCKET_ERRNO_SYMBOL;
}

/// Emits the AArch64 composer.
fn emit_gai_publish_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: compose the unresolvable-host message ---");
    emitter.label_global("__rt_gai_publish");
    emitter.instruction("sub sp, sp, #48");                                     // frame for the reason pair and the saved linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    abi::emit_symbol_address(emitter, "x9", "_socket_gai_err");
    emitter.instruction("ldr x9, [x9]");                                        // what the resolver answered
    emitter.instruction("cbz x9, __rt_gai_pub_done");                           // it succeeded: there is no message to compose

    emitter.instruction("mov x0, x9");                                          // describe the resolver's own error code
    emitter.bl_c("gai_strerror");                                               // x0 = static NUL-terminated reason
    emitter.instruction("cbz x0, __rt_gai_pub_no_reason");                      // an unknown code has no text
    emitter.instruction("mov x9, #0");                                          // measured reason length
    emitter.label("__rt_gai_pub_scan");
    emitter.instruction(&format!("cmp x9, #{SOCKET_GAI_REASON_CLAMP}"));        // never copy more than the buffer allows
    emitter.instruction("b.ge __rt_gai_pub_scanned");
    emitter.instruction("ldrb w10, [x0, x9]");                                  // load the next reason byte
    emitter.instruction("cbz w10, __rt_gai_pub_scanned");                       // reached the terminator
    emitter.instruction("add x9, x9, #1");                                      // keep measuring
    emitter.instruction("b __rt_gai_pub_scan");
    emitter.label("__rt_gai_pub_scanned");
    emitter.instruction("str x0, [sp, #0]");                                    // the reason text
    emitter.instruction("str x9, [sp, #8]");                                    // and its length
    emitter.instruction("b __rt_gai_pub_have_reason");
    emitter.label("__rt_gai_pub_no_reason");
    abi::emit_symbol_address(emitter, "x0", "_empty_str");
    emitter.instruction("str x0, [sp, #0]");                                    // no reason text to report
    emitter.instruction("str xzr, [sp, #8]");
    emitter.label("__rt_gai_pub_have_reason");

    abi::emit_symbol_address(emitter, "x12", "_socket_gai_msg");
    emitter.instruction("mov x11, x12");                                        // remember the base to measure the result
    abi::emit_symbol_address(emitter, "x13", "_gai_msg_prefix");
    emitter.instruction(&format!("mov x14, #{}", GAI_MSG_PREFIX.len()));
    emit_append_aarch64(emitter, "prefix");
    abi::emit_symbol_address(emitter, "x9", "_socket_gai_host_ptr");
    emitter.instruction("ldr x13, [x9]");                                       // the host the resolver was given
    abi::emit_symbol_address(emitter, "x9", "_socket_gai_host_len");
    emitter.instruction("ldr x14, [x9]");                                       // and its length
    emitter.instruction(&format!("cmp x14, #{SOCKET_GAI_HOST_CLAMP}"));         // clamp to what a DNS name can hold
    emitter.instruction("b.le __rt_gai_pub_host_ok");
    emitter.instruction(&format!("mov x14, #{SOCKET_GAI_HOST_CLAMP}"));
    emitter.label("__rt_gai_pub_host_ok");
    emitter.instruction("cbnz x13, __rt_gai_pub_host_copy");                    // a recorded host is copied
    emitter.instruction("mov x14, #0");                                         // no host recorded: copy nothing
    emitter.label("__rt_gai_pub_host_copy");
    emit_append_aarch64(emitter, "host");
    abi::emit_symbol_address(emitter, "x13", "_gai_msg_middle");
    emitter.instruction(&format!("mov x14, #{}", GAI_MSG_MIDDLE.len()));
    emit_append_aarch64(emitter, "middle");
    emitter.instruction("ldr x13, [sp, #0]");                                   // the reason text
    emitter.instruction("ldr x14, [sp, #8]");                                   // and its length
    emit_append_aarch64(emitter, "reason");
    emitter.instruction("sub x9, x12, x11");                                    // how many bytes the message occupies
    abi::emit_symbol_address(emitter, "x10", "_socket_gai_msg_len");
    emitter.instruction("str x9, [x10]");                                       // publish it for the error outputs and the warning

    emitter.label("__rt_gai_pub_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");
}

/// Emits one AArch64 append of `x14` bytes from `x13` to the cursor in `x12`.
fn emit_append_aarch64(emitter: &mut Emitter, tag: &str) {
    emitter.label(&format!("__rt_gai_pub_copy_{tag}"));
    emitter.instruction(&format!("cbz x14, __rt_gai_pub_copied_{tag}"));        // nothing left to append
    emitter.instruction("ldrb w15, [x13], #1");                                 // take one source byte
    emitter.instruction("strb w15, [x12], #1");                                 // append it at the cursor
    emitter.instruction("sub x14, x14, #1");                                    // one fewer byte to append
    emitter.instruction(&format!("b __rt_gai_pub_copy_{tag}"));
    emitter.label(&format!("__rt_gai_pub_copied_{tag}"));
}

/// Emits the x86_64 composer.
fn emit_gai_publish_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: compose the unresolvable-host message ---");
    emitter.label_global("__rt_gai_publish");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 48");                                         // reserve the reason and base slots
    abi::emit_symbol_address(emitter, "r10", "_socket_gai_err");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // what the resolver answered
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_gai_pub_done_x86");                            // it succeeded: there is no message to compose

    emitter.instruction("mov rdi, r10");                                        // describe the resolver's own error code
    emitter.bl_c("gai_strerror");                                               // rax = static NUL-terminated reason
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_gai_pub_no_reason_x86");                       // an unknown code has no text
    emitter.instruction("xor r9, r9");                                          // measured reason length
    emitter.label("__rt_gai_pub_scan_x86");
    emitter.instruction(&format!("cmp r9, {SOCKET_GAI_REASON_CLAMP}"));         // never copy more than the buffer allows
    emitter.instruction("jge __rt_gai_pub_scanned_x86");
    emitter.instruction("movzx r11d, BYTE PTR [rax + r9]");                     // load the next reason byte
    emitter.instruction("test r11b, r11b");
    emitter.instruction("jz __rt_gai_pub_scanned_x86");                         // reached the terminator
    emitter.instruction("inc r9");                                              // keep measuring
    emitter.instruction("jmp __rt_gai_pub_scan_x86");
    emitter.label("__rt_gai_pub_scanned_x86");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // the reason text
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // and its length
    emitter.instruction("jmp __rt_gai_pub_have_reason_x86");
    emitter.label("__rt_gai_pub_no_reason_x86");
    abi::emit_symbol_address(emitter, "rax", "_empty_str");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // no reason text to report
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");
    emitter.label("__rt_gai_pub_have_reason_x86");

    abi::emit_symbol_address(emitter, "rdi", "_socket_gai_msg");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // remember the base to measure the result
    abi::emit_symbol_address(emitter, "rsi", "_gai_msg_prefix");
    emitter.instruction(&format!("mov rcx, {}", GAI_MSG_PREFIX.len()));
    emitter.instruction("rep movsb");                                           // append the prefix
    abi::emit_symbol_address(emitter, "r10", "_socket_gai_host_ptr");
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // the host the resolver was given
    abi::emit_symbol_address(emitter, "r10", "_socket_gai_host_len");
    emitter.instruction("mov rcx, QWORD PTR [r10]");                            // and its length
    emitter.instruction(&format!("cmp rcx, {SOCKET_GAI_HOST_CLAMP}"));          // clamp to what a DNS name can hold
    emitter.instruction("jle __rt_gai_pub_host_ok_x86");
    emitter.instruction(&format!("mov rcx, {SOCKET_GAI_HOST_CLAMP}"));
    emitter.label("__rt_gai_pub_host_ok_x86");
    emitter.instruction("test rsi, rsi");
    emitter.instruction("jnz __rt_gai_pub_host_copy_x86");                      // a recorded host is copied
    emitter.instruction("xor ecx, ecx");                                        // no host recorded: copy nothing
    emitter.label("__rt_gai_pub_host_copy_x86");
    emitter.instruction("rep movsb");                                           // append the host
    abi::emit_symbol_address(emitter, "rsi", "_gai_msg_middle");
    emitter.instruction(&format!("mov rcx, {}", GAI_MSG_MIDDLE.len()));
    emitter.instruction("rep movsb");                                           // append the middle
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // the reason text
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // and its length
    emitter.instruction("rep movsb");                                           // append the reason
    emitter.instruction("mov rax, rdi");                                        // the cursor now sits past the message
    emitter.instruction("sub rax, QWORD PTR [rbp - 24]");                       // how many bytes it occupies
    abi::emit_symbol_address(emitter, "r10", "_socket_gai_msg_len");
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish it for the error outputs and the warning

    emitter.label("__rt_gai_pub_done_x86");
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
