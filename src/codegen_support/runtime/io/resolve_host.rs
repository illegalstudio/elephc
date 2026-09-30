//! Purpose:
//! Emits the `__rt_resolve_host` runtime helper, which resolves a host name to
//! a packed IPv4 address for the socket address parser.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_inet_addr_parse` when the address slice is not a numeric dotted quad.
//!
//! Key details:
//! - Resolution goes through libc `getaddrinfo` with `AF_INET` hints, replacing
//!   the deprecated `gethostbyname`. `freeaddrinfo` is called after the address
//!   is copied so the libc allocation is released.
//! - The result is byte-swapped to match `__rt_ip2long`'s host-readable packed
//!   integer. The packed IPv4 is saved across the `freeaddrinfo` call on the
//!   stack so it survives the callee-clobbered registers.

use super::socket_errno::{emit_publish_gai_code, emit_publish_gai_host};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// resolve_host: resolve a host-name slice to a packed IPv4 integer.
/// Input:  AArch64 x0 = host pointer, x1 = host length
///         x86_64  rdi = host pointer, rsi = host length
/// Output: packed IPv4 integer in `__rt_ip2long` form, or -1 when unresolved.
pub fn emit_resolve_host(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_resolve_host_linux_x86_64(emitter);
        return;
    }

    let af_inet: i64 = 2; // AF_INET is 2 on all supported targets (macOS + Linux)
    let addr_off = emitter.platform.addrinfo_addr_offset();

    emitter.blank();
    emitter.comment("--- runtime: resolve_host ---");
    emitter.label_global("__rt_resolve_host");

    // Frame (96 bytes):
    //   [sp, #0..48]   struct addrinfo hints (zeroed except ai_family at +4)
    //   [sp, #48..56]  out pointer (struct addrinfo *res from getaddrinfo)
    //   [sp, #56..64]  c_str / saved packed IPv4 result (reused after c_str is consumed)
    //   [sp, #72..80]  saved x29
    //   [sp, #80..88]  saved x30
    emitter.instruction("sub sp, sp, #96");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #72]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #72");                                    // establish the helper frame pointer

    // -- zero the 48-byte hints struct then set ai_family = AF_INET --
    emitter.instruction("stp xzr, xzr, [sp, #0]");                              // hints[0..16]
    emitter.instruction("stp xzr, xzr, [sp, #16]");                             // hints[16..32]
    emitter.instruction("stp xzr, xzr, [sp, #32]");                             // hints[32..48]
    emitter.instruction(&format!("mov w9, #{}", af_inet));                       // AF_INET (2 on all supported targets)
    emitter.instruction("str w9, [sp, #4]");                                    // ai_family at offset 4

    // The host is published before anything can clobber the argument pair: a failure names it,
    // and by the failure paths below `getaddrinfo` and `freeaddrinfo` have taken those registers.
    emit_publish_gai_host(emitter, "x0", "x1");

    // -- null-terminate the host slice for getaddrinfo --
    emitter.instruction("mov x2, x1");                                          // host length into __rt_cstr's length register
    emitter.instruction("mov x1, x0");                                          // host pointer into __rt_cstr's pointer register
    emitter.instruction("bl __rt_cstr");                                        // x0 = null-terminated host name
    emitter.instruction("str x0, [sp, #56]");                                   // save the c_str pointer

    // -- getaddrinfo(c_str, NULL, &hints, &res) --
    emitter.instruction("ldr x0, [sp, #56]");                                   // arg 1: c_str
    emitter.instruction("mov x1, #0");                                          // arg 2: service = NULL (name-only mode)
    emitter.instruction("mov x2, sp");                                          // arg 3: &hints
    emitter.instruction("add x3, sp, #48");                                     // arg 4: &res (output slot)
    emitter.bl_c("getaddrinfo");                                                // returns 0 on success, error code otherwise
    emit_publish_gai_code(emitter, "x0");                                       // success publishes 0, which is the no-failure state
    emitter.instruction("cbnz x0, __rt_resolve_host_fail");                      // non-zero means resolution failed

    // -- copy res->ai_addr->sin_addr (4 bytes) and byte-swap --
    emitter.instruction("ldr x9, [sp, #48]");                                   // x9 = first addrinfo in the result list
    emitter.instruction("cbz x9, __rt_resolve_host_fail");                      // empty list — bail
    emitter.instruction(&format!("ldr x10, [x9, #{}]", addr_off));              // x10 = ai_addr (struct sockaddr *)
    emitter.instruction("cbz x10, __rt_resolve_host_free_fail");                 // null sockaddr — free and bail
    emitter.instruction("ldr w0, [x10, #4]");                                   // sin_addr at offset 4 inside sockaddr_in
    emitter.instruction("rev w0, w0");                                          // byte-swap into __rt_ip2long packed form
    emitter.instruction("str w0, [sp, #56]");                                   // save the result across freeaddrinfo

    // -- freeaddrinfo(res) so libc's allocation is released --
    emitter.instruction("ldr x0, [sp, #48]");                                   // arg 1: res
    emitter.bl_c("freeaddrinfo");                                               // libc releases the returned list

    // -- return the packed IPv4 --
    emitter.instruction("ldr w0, [sp, #56]");                                   // reload the saved result
    emitter.instruction("ldp x29, x30, [sp, #72]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the frame
    emitter.instruction("ret");                                                 // return the packed IPv4 integer

    emitter.label("__rt_resolve_host_free_fail");
    emitter.instruction("ldr x0, [sp, #48]");                                   // res
    emitter.bl_c("freeaddrinfo");                                               // free even though we found no usable addr
    // fall through
    emitter.label("__rt_resolve_host_fail");
    emitter.instruction("bl __rt_gai_publish");                                 // compose the message PHP reports for this
    emitter.instruction("mov x0, #-1");                                         // -1 signals an unresolvable host name
    emitter.instruction("ldp x29, x30, [sp, #72]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for resolve host.
fn emit_resolve_host_linux_x86_64(emitter: &mut Emitter) {
    let af_inet: i64 = 2; // AF_INET is 2 on all supported targets
    let addr_off = emitter.platform.addrinfo_addr_offset();

    emitter.blank();
    emitter.comment("--- runtime: resolve_host ---");
    emitter.label_global("__rt_resolve_host");

    // rbp-relative layout:
    //   [rbp - 48]  struct addrinfo hints (48 bytes)
    //   [rbp - 56]  out pointer (struct addrinfo *res)
    //   [rbp - 64]  c_str pointer
    //   [rbp - 72]  saved packed IPv4 result
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 80");                                         // 48 hints + 32 scratch = 80 (16-aligned)

    // Zero the hints struct.
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");
    emitter.instruction("mov QWORD PTR [rbp - 8], 0");
    emitter.instruction(&format!("mov DWORD PTR [rbp - 44], {}", af_inet));     // ai_family at hints+4 (rbp-48+4 = rbp-44)

    // See the AArch64 counterpart: the host is published while the argument pair is still live.
    emit_publish_gai_host(emitter, "rdi", "rsi");

    // -- null-terminate the host slice for getaddrinfo --
    emitter.instruction("mov rax, rdi");                                        // host pointer into __rt_cstr's pointer register
    emitter.instruction("mov rdx, rsi");                                        // host length into __rt_cstr's length register
    emitter.instruction("call __rt_cstr");                                      // rax = null-terminated host name
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                        // save the c_str pointer

    // -- getaddrinfo(c_str, NULL, &hints, &res) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                        // arg 1: c_str
    emitter.instruction("xor esi, esi");                                        // arg 2: service = NULL
    emitter.instruction("lea rdx, [rbp - 48]");                                 // arg 3: &hints
    emitter.instruction("lea rcx, [rbp - 56]");                                 // arg 4: &res
    emitter.instruction("call getaddrinfo");                                    // returns 0 on success
    emitter.instruction("movsxd r11, eax");                                     // widen the resolver's code before publishing it
    emit_publish_gai_code(emitter, "r11");                                      // success publishes 0, which is the no-failure state
    emitter.instruction("test eax, eax");                                       // did resolution fail?
    emitter.instruction("jnz __rt_resolve_host_fail_x86");                       // non-zero means failure

    // -- copy res->ai_addr->sin_addr (4 bytes) and byte-swap --
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // r9 = first addrinfo
    emitter.instruction("test r9, r9");                                         // empty list?
    emitter.instruction("jz __rt_resolve_host_fail_x86");                        // bail
    emitter.instruction(&format!("mov r10, QWORD PTR [r9 + {}]", addr_off));     // r10 = ai_addr
    emitter.instruction("test r10, r10");                                       // null sockaddr?
    emitter.instruction("jz __rt_resolve_host_free_fail_x86");                   // free and bail
    emitter.instruction("mov eax, DWORD PTR [r10 + 4]");                        // sin_addr at offset 4
    emitter.instruction("bswap eax");                                           // byte-swap into packed form
    emitter.instruction("mov DWORD PTR [rbp - 72], eax");                        // save the result across freeaddrinfo

    // -- freeaddrinfo(res) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                        // arg 1: res
    emitter.instruction("call freeaddrinfo");                                   // libc releases the list

    // -- return the packed IPv4 --
    emitter.instruction("mov eax, DWORD PTR [rbp - 72]");                        // reload the saved result
    emitter.instruction("cdqe");                                                // sign-extend to 64-bit
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return the packed IPv4

    emitter.label("__rt_resolve_host_free_fail_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                        // res
    emitter.instruction("call freeaddrinfo");                                   // free even though we found no usable addr
    // fall through
    emitter.label("__rt_resolve_host_fail_x86");
    emitter.instruction("call __rt_gai_publish");                               // compose the message PHP reports for this
    emitter.instruction("mov rax, -1");                                         // -1 signals an unresolvable host name
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return the failure result
}