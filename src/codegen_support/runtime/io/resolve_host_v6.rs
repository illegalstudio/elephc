//! Purpose:
//! Emits the `__rt_resolve_host_v6` runtime helper, which resolves a host
//! name to a 16-byte IPv6 address through libc `getaddrinfo` with an
//! `AF_INET6` hint. The wrapper is used by the IPv6 socket helpers as a
//! fallback when `inet_pton(AF_INET6, ...)` rejects the bracketed host —
//! i.e. when the source uses `tcp://[example.com]:80` instead of a
//! literal `[2001:db8::1]:80`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The `tcp://[...]` / `udp://[...]` IPv6 socket dispatch in
//!   `__rt_build_sockaddr_in6` / `__rt_stream_socket_client_v6` /
//!   `__rt_stream_socket_server_v6` after a failed `__rt_inet6_pton`.
//!
//! Key details:
//! - `addrinfo` field layout differs between BSD (macOS) and glibc
//!   (Linux): `ai_addr` is at offset 32 on macOS and 24 on Linux. The
//!   helper reads `Platform::addrinfo_addr_offset()` so the same logic
//!   compiles correctly on both targets.
//! - The 48-byte `hints` struct (`sizeof(struct addrinfo) = 48` on every
//!   LP64 target) is built on the stack, zeroed, and only sets
//!   `ai_family = AF_INET6` at offset 4. `service` is passed as null,
//!   making `getaddrinfo` work in name-only mode.
//! - `sin6_addr` lives at offset 8 inside the returned sockaddr_in6 on
//!   both targets.
//! - `freeaddrinfo` is called after the address bytes are copied so the
//!   libc allocation does not leak across resolver invocations.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// resolve_host_v6: resolve a host-name slice to a 16-byte IPv6 address.
/// Input:  AArch64 x0 = host pointer, x1 = host length, x2 = out buffer
///         x86_64  rdi = host pointer, rsi = host length, rdx = out buffer
/// Output: 1 on success, 0 when no AF_INET6 address could be resolved.
pub fn emit_resolve_host_v6(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_resolve_host_v6_linux_x86_64(emitter);
        return;
    }

    let af_inet6 = emitter.platform.af_inet6();
    let addr_off = emitter.platform.addrinfo_addr_offset();

    emitter.blank();
    emitter.comment("--- runtime: resolve_host_v6 ---");
    emitter.label_global("__rt_resolve_host_v6");

    // Frame (96 bytes):
    //   [sp, #0..48]   struct addrinfo hints (zeroed except ai_family at +4)
    //   [sp, #48..56]  out pointer (struct addrinfo *res from getaddrinfo)
    //   [sp, #56..64]  caller out buffer pointer
    //   [sp, #64..72]  c_str of the host name (returned by __rt_cstr)
    //   [sp, #72..80]  saved x29
    //   [sp, #80..88]  saved x30
    emitter.instruction("sub sp, sp, #96");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #72]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #72");                                    // establish the helper frame pointer
    emitter.instruction("str x2, [sp, #56]");                                   // save the caller's out buffer pointer

    // -- zero the 48-byte hints struct then set ai_family = AF_INET6 --
    emitter.instruction("stp xzr, xzr, [sp, #0]");                              // hints[0..16]
    emitter.instruction("stp xzr, xzr, [sp, #16]");                             // hints[16..32]
    emitter.instruction("stp xzr, xzr, [sp, #32]");                             // hints[32..48]
    emitter.instruction(&format!("mov w9, #{}", af_inet6));                     // AF_INET6 (10 Linux / 30 macOS)
    emitter.instruction("str w9, [sp, #4]");                                    // ai_family at offset 4

    // -- null-terminate the host slice for getaddrinfo --
    emitter.instruction("mov x2, x1");                                          // host length into __rt_cstr's length register
    emitter.instruction("mov x1, x0");                                          // host pointer into __rt_cstr's pointer register
    emitter.instruction("bl __rt_cstr");                                        // x0 = null-terminated host name
    emitter.instruction("str x0, [sp, #64]");                                   // save the c_str pointer (also passed as arg 1)

    // -- getaddrinfo(c_str, NULL, &hints, &res) --
    emitter.instruction("ldr x0, [sp, #64]");                                   // arg 1: c_str
    emitter.instruction("mov x1, #0");                                          // arg 2: service = NULL (name-only mode)
    emitter.instruction("mov x2, sp");                                          // arg 3: &hints
    emitter.instruction("add x3, sp, #48");                                     // arg 4: &res (output slot)
    emitter.bl_c("getaddrinfo");                                                // returns 0 on success, error code otherwise
    emitter.instruction("cbnz x0, __rt_rhv6_fail");                             // non-zero means resolution failed

    // -- copy res->ai_addr->sin6_addr (16 bytes) into the caller's out buffer --
    emitter.instruction("ldr x9, [sp, #48]");                                   // x9 = first addrinfo in the result list
    emitter.instruction("cbz x9, __rt_rhv6_fail");                              // empty list — bail
    emitter.instruction(&format!("ldr x10, [x9, #{}]", addr_off));              // x10 = ai_addr (struct sockaddr *)
    emitter.instruction("cbz x10, __rt_rhv6_free_fail");                        // null sockaddr — free and bail
    emitter.instruction("ldr x11, [sp, #56]");                                  // x11 = caller out buffer
    emitter.instruction("ldr x12, [x10, #8]");                                  // sin6_addr[0..8] (offset 8 inside sockaddr_in6)
    emitter.instruction("str x12, [x11]");                                      // copy low 8 bytes of sin6_addr
    emitter.instruction("ldr x12, [x10, #16]");                                 // sin6_addr[8..16]
    emitter.instruction("str x12, [x11, #8]");                                  // copy high 8 bytes of sin6_addr

    // -- freeaddrinfo(res) so libc's allocation is released --
    emitter.instruction("ldr x0, [sp, #48]");                                   // arg 1: res
    emitter.bl_c("freeaddrinfo");                                               // libc releases the returned list

    emitter.instruction("mov x0, #1");                                          // success
    emitter.instruction("ldp x29, x30, [sp, #72]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller

    emitter.label("__rt_rhv6_free_fail");
    emitter.instruction("ldr x0, [sp, #48]");                                   // res
    emitter.bl_c("freeaddrinfo");                                               // free even though we found no usable addr
    // fall through
    emitter.label("__rt_rhv6_fail");
    emitter.instruction("mov x0, #0");                                          // failure
    emitter.instruction("ldp x29, x30, [sp, #72]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for resolve host v6.
fn emit_resolve_host_v6_linux_x86_64(emitter: &mut Emitter) {
    let af_inet6 = emitter.platform.af_inet6();
    let addr_off = emitter.platform.addrinfo_addr_offset();

    emitter.blank();
    emitter.comment("--- runtime: resolve_host_v6 ---");
    emitter.label_global("__rt_resolve_host_v6");

    // rbp-relative layout:
    //   [rbp - 48]  struct addrinfo hints (48 bytes)
    //   [rbp - 56]  out pointer (struct addrinfo *res)
    //   [rbp - 64]  caller out buffer
    //   [rbp - 72]  c_str pointer
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 80");                                         // 48 hints + 24 scratch + 8 padding = 80 (16-aligned)
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save caller's out buffer pointer

    // Zero the hints struct.
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 8], 0");                          // store runtime value
    emitter.instruction(&format!("mov DWORD PTR [rbp - 44], {}", af_inet6));    // ai_family at hints+4 (rbp-48+4 = rbp-44)

    // -- null-terminate the host slice --
    emitter.instruction("mov rax, rdi");                                        // host pointer into __rt_cstr's pointer register
    emitter.instruction("mov rdx, rsi");                                        // host length into __rt_cstr's length register
    emitter.instruction("call __rt_cstr");                                      // rax = c_str
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save c_str

    // -- getaddrinfo(c_str, NULL, &hints, &res) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // arg 1: c_str
    emitter.instruction("xor esi, esi");                                        // arg 2: service = NULL
    emitter.instruction("lea rdx, [rbp - 48]");                                 // arg 3: &hints
    emitter.instruction("lea rcx, [rbp - 56]");                                 // arg 4: &res
    emitter.emit_call_c("getaddrinfo");                                         // call external helper
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jnz __rt_rhv6_fail_x86");                              // non-zero return = error

    // -- copy res->ai_addr->sin6_addr (16 bytes) into the caller's out buffer --
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // first addrinfo
    emitter.instruction("test r9, r9");                                         // check whether the runtime value is zero
    emitter.instruction("jz __rt_rhv6_fail_x86");                               // branch when the checked value is zero or equal
    emitter.instruction(&format!("mov r10, QWORD PTR [r9 + {}]", addr_off));    // ai_addr
    emitter.instruction("test r10, r10");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_rhv6_free_fail_x86");                          // branch when the checked value is zero or equal
    emitter.instruction("mov r11, QWORD PTR [rbp - 64]");                       // caller out buffer
    emitter.instruction("mov r12, QWORD PTR [r10 + 8]");                        // sin6_addr[0..8]
    emitter.instruction("mov QWORD PTR [r11], r12");                            // store runtime value
    emitter.instruction("mov r12, QWORD PTR [r10 + 16]");                       // sin6_addr[8..16]
    emitter.instruction("mov QWORD PTR [r11 + 8], r12");                        // store runtime value

    // -- freeaddrinfo(res) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    emitter.emit_call_c("freeaddrinfo");                                        // call external helper

    emitter.instruction("mov eax, 1");                                          // success
    emitter.instruction("add rsp, 80");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller

    emitter.label("__rt_rhv6_free_fail_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    emitter.emit_call_c("freeaddrinfo");                                        // call external helper
    // fall through
    emitter.label("__rt_rhv6_fail_x86");
    emitter.instruction("xor eax, eax");                                        // failure
    emitter.instruction("add rsp, 80");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
