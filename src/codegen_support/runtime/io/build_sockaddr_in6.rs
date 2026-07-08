//! Purpose:
//! Emits the `__rt_build_sockaddr_in6` runtime helper, which converts a
//! textual `[ipv6_literal]:port` (optionally preceded by a `scheme://`
//! that the caller has already consumed) into a 28-byte `sockaddr_in6`
//! laid out in the caller's buffer.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_stream_socket_sendto`'s bracketed-address dispatch.
//!
//! Key details:
//! - Returns `28` on success (sizeof(sockaddr_in6)) so callers can pass
//!   the return value straight to `sendto` / `connect` / `bind` as the
//!   addrlen argument. A parse failure returns `-1`.
//! - Locates the `[`, then the matching `]`, parses the IPv6 literal
//!   between them through `__rt_inet6_pton`, then expects `:port` and a
//!   decimal port. The 28-byte sockaddr_in6 is built in-place at the
//!   caller's buffer (sin6_len + sin6_family + sin6_port + sin6_flowinfo +
//!   sin6_addr + sin6_scope_id).
//! - The caller's buffer must be 28-byte writable and zeroable; the
//!   helper writes every byte of the family/port/flowinfo + addr +
//!   scope_id range before returning.

use crate::codegen_support::{emit::Emitter, platform::Arch, platform::Platform};

/// build_sockaddr_in6: turn `[ipv6]:port` into a sockaddr_in6 in the
/// caller's buffer.
/// Input:  AArch64 x0 = address pointer, x1 = address length, x2 = out buffer
///         x86_64  rdi = address pointer, rsi = address length, rdx = out buffer
/// Output: 28 on success (addrlen), -1 on failure.
pub fn emit_build_sockaddr_in6(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_build_sockaddr_in6_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let af_inet6 = plat.af_inet6();
    emitter.blank();
    emitter.comment("--- runtime: build_sockaddr_in6 ---");
    emitter.label_global("__rt_build_sockaddr_in6");

    // Frame (96 bytes): [0..16) saved x29/x30, [16) addr ptr, [24) addr len,
    //   [32) out buffer ptr, [40..56) sin6_addr scratch (16), [56) parsed port,
    //   [64) literal ptr stash for the DNS fallback, [72) literal len stash.
    emitter.instruction("sub sp, sp, #96");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the address pointer
    emitter.instruction("str x1, [sp, #24]");                                   // save the address length
    emitter.instruction("str x2, [sp, #32]");                                   // save the caller's out buffer pointer

    // -- locate '[' --
    emitter.instruction("mov x3, #0");                                          // bracket scan index
    emitter.label("__rt_bsi6_find_open");
    emitter.instruction("cmp x3, x1");                                          // reached the end of the address?
    emitter.instruction("b.ge __rt_bsi6_fail");                                 // no '[' found → bad address
    emitter.instruction("ldrb w4, [x0, x3]");                                   // load the candidate byte
    emitter.instruction("cmp w4, #91");                                         // ASCII '['
    emitter.instruction("b.eq __rt_bsi6_found_open");                           // start of the literal
    emitter.instruction("add x3, x3, #1");                                      // keep scanning
    emitter.instruction("b __rt_bsi6_find_open");                               // continue
    emitter.label("__rt_bsi6_found_open");
    emitter.instruction("add x5, x3, #1");                                      // literal start = byte after '['

    // -- locate ']' --
    emitter.instruction("mov x6, x5");                                          // scan from the literal start
    emitter.label("__rt_bsi6_find_close");
    emitter.instruction("cmp x6, x1");                                          // reached the end?
    emitter.instruction("b.ge __rt_bsi6_fail");                                 // no ']' → malformed
    emitter.instruction("ldrb w4, [x0, x6]");                                   // load the candidate byte
    emitter.instruction("cmp w4, #93");                                         // ASCII ']'
    emitter.instruction("b.eq __rt_bsi6_found_close");                          // end of the literal
    emitter.instruction("add x6, x6, #1");                                      // keep scanning
    emitter.instruction("b __rt_bsi6_find_close");                              // continue
    emitter.label("__rt_bsi6_found_close");
    emitter.instruction("sub x7, x6, x5");                                      // literal length = close - start
    emitter.instruction("str x6, [sp, #56]");                                   // stash the close-bracket index across the libc call

    // -- parse the IPv6 literal into the 16-byte sin6_addr scratch --
    emitter.instruction("add x0, x0, x5");                                      // literal pointer = address + start
    emitter.instruction("mov x1, x7");                                          // literal length
    emitter.instruction("add x2, sp, #40");                                     // out buffer = sin6_addr scratch
    emitter.instruction("str x0, [sp, #64]");                                   // save literal ptr in case pton fails and we need DNS
    emitter.instruction("str x1, [sp, #72]");                                   // save literal len for the DNS fallback
    emitter.instruction("bl __rt_inet6_pton");                                  // x0 = 1 on success, 0 on failure
    emitter.instruction("cbnz x0, __rt_bsi6_addr_ok");                          // pton succeeded — skip the DNS fallback
    // -- pton failed: try resolving the bracketed token as a hostname (Phase 11 B1) --
    emitter.instruction("ldr x0, [sp, #64]");                                   // reload literal ptr
    emitter.instruction("ldr x1, [sp, #72]");                                   // reload literal len
    emitter.instruction("add x2, sp, #40");                                     // out buffer
    emitter.instruction("bl __rt_resolve_host_v6");                             // resolves a hostname to a 16-byte AAAA via getaddrinfo
    emitter.instruction("cbz x0, __rt_bsi6_fail");                              // both pton and DNS rejected → bail
    emitter.label("__rt_bsi6_addr_ok");

    // -- expect ':' then decimal port immediately after the ']' --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the address pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the address length
    emitter.instruction("ldr x6, [sp, #56]");                                   // reload the close-bracket index
    emitter.instruction("add x8, x6, #1");                                      // index of the ':' after ']'
    emitter.instruction("cmp x8, x1");                                          // is there room for ':port'?
    emitter.instruction("b.ge __rt_bsi6_fail");                                 // missing ':port'
    emitter.instruction("ldrb w4, [x0, x8]");                                   // load the byte after ']'
    emitter.instruction("cmp w4, #58");                                         // ':'?
    emitter.instruction("b.ne __rt_bsi6_fail");                                 // not the port separator
    emitter.instruction("add x9, x8, #1");                                      // first port digit index
    emitter.instruction("mov x10, #0");                                         // accumulated port value
    emitter.instruction("cmp x9, x1");                                          // any port digits at all?
    emitter.instruction("b.ge __rt_bsi6_fail");                                 // empty port
    emitter.label("__rt_bsi6_port");
    emitter.instruction("cmp x9, x1");                                          // consumed every byte?
    emitter.instruction("b.ge __rt_bsi6_port_done");                            // port parsed
    emitter.instruction("ldrb w4, [x0, x9]");                                   // load the digit
    emitter.instruction("cmp w4, #48");                                         // ASCII '0'
    emitter.instruction("b.lt __rt_bsi6_fail");                                 // non-digit → bail
    emitter.instruction("cmp w4, #57");                                         // ASCII '9'
    emitter.instruction("b.gt __rt_bsi6_fail");                                 // non-digit → bail
    emitter.instruction("sub w4, w4, #48");                                     // digit value
    emitter.instruction("mov x11, #10");                                        // decimal base
    emitter.instruction("mul x10, x10, x11");                                   // shift port one decimal place
    emitter.instruction("add x10, x10, x4");                                    // add the new digit
    emitter.instruction("add x9, x9, #1");                                      // advance the digit cursor
    emitter.instruction("b __rt_bsi6_port");                                    // continue
    emitter.label("__rt_bsi6_port_done");

    // -- build the 28-byte sockaddr_in6 in the caller's out buffer --
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the out buffer pointer
    emitter.instruction("str xzr, [x12, #0]");                                  // zero bytes 0..8 (family/port/flowinfo)
    emitter.instruction("str xzr, [x12, #8]");                                  // zero bytes 8..16 (sin6_addr low)
    emitter.instruction("str xzr, [x12, #16]");                                 // zero bytes 16..24 (sin6_addr high)
    emitter.instruction("str wzr, [x12, #24]");                                 // zero bytes 24..28 (scope_id)
    if matches!(plat, Platform::MacOS) {
        emitter.instruction("mov w11, #28");                                    // sin6_len = sizeof(sockaddr_in6)
        emitter.instruction("strb w11, [x12, #0]");                             // macOS keeps sin6_len ahead of sin6_family
        emitter.instruction(&format!("mov w11, #{}", af_inet6));                // AF_INET6 = 30 on macOS
        emitter.instruction("strb w11, [x12, #1]");                             // store sin6_family in the second byte
    } else {
        emitter.instruction(&format!("mov w11, #{}", af_inet6));                // AF_INET6 = 10 on Linux
        emitter.instruction("strb w11, [x12, #0]");                             // sin6_family low byte
        emitter.instruction("strb wzr, [x12, #1]");                             // sin6_family high byte
    }
    emitter.instruction("lsr x13, x10, #8");                                    // high byte of the port
    emitter.instruction("strb w13, [x12, #2]");                                 // sin6_port is network byte order
    emitter.instruction("strb w10, [x12, #3]");                                 // low byte of the port
    emitter.instruction("ldr x13, [sp, #40]");                                  // sin6_addr low word from inet6_pton scratch
    emitter.instruction("ldr x14, [sp, #48]");                                  // sin6_addr high word from inet6_pton scratch
    emitter.instruction("str x13, [x12, #8]");                                  // store sin6_addr low half
    emitter.instruction("str x14, [x12, #16]");                                 // store sin6_addr high half

    emitter.instruction("mov x0, #28");                                         // success: sockaddr_in6 addrlen
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the frame
    emitter.instruction("ret");                                                 // return the addrlen

    emitter.label("__rt_bsi6_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 reports a failed sockaddr_in6 build
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for build sockaddr in6.
fn emit_build_sockaddr_in6_linux_x86_64(emitter: &mut Emitter) {
    let af_inet6 = Platform::Linux.af_inet6();
    emitter.blank();
    emitter.comment("--- runtime: build_sockaddr_in6 ---");
    emitter.label_global("__rt_build_sockaddr_in6");

    // Frame (rbp-relative): [-8) addr ptr, [-16) addr len, [-24) out buffer,
    //   [-40..-24) sin6_addr scratch (16), [-48) close-bracket scratch,
    //   [-56) parsed port stash.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // helper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the address pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the address length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the out buffer pointer

    // -- locate '[' --
    emitter.instruction("xor rcx, rcx");                                        // bracket scan index
    emitter.label("__rt_bsi6_find_open_x86");
    emitter.instruction("cmp rcx, rsi");                                        // reached the end?
    emitter.instruction("jae __rt_bsi6_fail_x86");                              // no '[' found → bad address
    emitter.instruction("movzx eax, BYTE PTR [rdi + rcx]");                     // load the candidate byte
    emitter.instruction("cmp eax, 91");                                         // ASCII '['
    emitter.instruction("je __rt_bsi6_found_open_x86");                         // start of the literal
    emitter.instruction("inc rcx");                                             // keep scanning
    emitter.instruction("jmp __rt_bsi6_find_open_x86");                         // continue
    emitter.label("__rt_bsi6_found_open_x86");
    emitter.instruction("lea r8, [rcx + 1]");                                   // literal start = byte after '['

    // -- locate ']' --
    emitter.instruction("mov r9, r8");                                          // scan from the literal start
    emitter.label("__rt_bsi6_find_close_x86");
    emitter.instruction("cmp r9, rsi");                                         // reached the end?
    emitter.instruction("jae __rt_bsi6_fail_x86");                              // no ']' → malformed
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");                      // load the candidate byte
    emitter.instruction("cmp eax, 93");                                         // ASCII ']'
    emitter.instruction("je __rt_bsi6_found_close_x86");                        // end of the literal
    emitter.instruction("inc r9");                                              // keep scanning
    emitter.instruction("jmp __rt_bsi6_find_close_x86");                        // continue
    emitter.label("__rt_bsi6_found_close_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // stash close-bracket idx across the libc call

    // -- parse the IPv6 literal into the 16-byte sin6_addr scratch --
    emitter.instruction("mov r10, r9");                                         // literal end for length math
    emitter.instruction("sub r10, r8");                                         // literal length = close - start
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the address pointer
    emitter.instruction("add rdi, r8");                                         // literal pointer = address + start
    emitter.instruction("mov rsi, r10");                                        // literal length
    emitter.instruction("lea rdx, [rbp - 40]");                                 // out buffer = sin6_addr scratch
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                       // stash literal ptr for the DNS fallback (rbp-64 is unused scratch within the 64-byte frame)
    emitter.instruction("mov QWORD PTR [rbp - 56], rsi");                       // stash literal len (reuses the close-bracket+8 scratch, which we'll reload below)
    emitter.instruction("call __rt_inet6_pton");                                // rax = 1 on success, 0 on failure
    emitter.instruction("test rax, rax");                                       // did the literal parse?
    emitter.instruction("jnz __rt_bsi6_addr_ok_x86");                           // pton succeeded — skip the DNS fallback
    // -- pton failed: try resolving the bracketed token as a hostname (Phase 11 B1) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // reload literal ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload literal len
    emitter.instruction("lea rdx, [rbp - 40]");                                 // out buffer
    emitter.instruction("call __rt_resolve_host_v6");                           // getaddrinfo with AF_INET6 hint
    emitter.instruction("test rax, rax");                                       // success?
    emitter.instruction("jz __rt_bsi6_fail_x86");                               // both pton and DNS rejected → bail
    emitter.label("__rt_bsi6_addr_ok_x86");

    // -- expect ':' then decimal port immediately after the ']' --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the address pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the address length
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the close-bracket idx
    emitter.instruction("lea r11, [r9 + 1]");                                   // index of the ':' after ']'
    emitter.instruction("cmp r11, rsi");                                        // is there room for ':port'?
    emitter.instruction("jae __rt_bsi6_fail_x86");                              // missing ':port'
    emitter.instruction("movzx eax, BYTE PTR [rdi + r11]");                     // load the byte after ']'
    emitter.instruction("cmp eax, 58");                                         // ':'?
    emitter.instruction("jne __rt_bsi6_fail_x86");                              // not the port separator
    emitter.instruction("lea rcx, [r11 + 1]");                                  // first port digit index
    emitter.instruction("xor edx, edx");                                        // accumulated port value
    emitter.instruction("cmp rcx, rsi");                                        // any port digits at all?
    emitter.instruction("jae __rt_bsi6_fail_x86");                              // empty port
    emitter.label("__rt_bsi6_port_x86");
    emitter.instruction("cmp rcx, rsi");                                        // consumed every byte?
    emitter.instruction("jae __rt_bsi6_port_done_x86");                         // port parsed
    emitter.instruction("movzx eax, BYTE PTR [rdi + rcx]");                     // load the digit
    emitter.instruction("cmp eax, 48");                                         // ASCII '0'
    emitter.instruction("jl __rt_bsi6_fail_x86");                               // non-digit → bail
    emitter.instruction("cmp eax, 57");                                         // ASCII '9'
    emitter.instruction("jg __rt_bsi6_fail_x86");                               // non-digit → bail
    emitter.instruction("sub eax, 48");                                         // digit value
    emitter.instruction("imul rdx, rdx, 10");                                   // shift port one decimal place
    emitter.instruction("add rdx, rax");                                        // add the new digit
    emitter.instruction("inc rcx");                                             // advance the digit cursor
    emitter.instruction("jmp __rt_bsi6_port_x86");                              // continue
    emitter.label("__rt_bsi6_port_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // stash the parsed port

    // -- build the 28-byte sockaddr_in6 in the caller's out buffer --
    emitter.instruction("mov r12, QWORD PTR [rbp - 24]");                       // reload the out buffer pointer
    emitter.instruction("mov QWORD PTR [r12 + 0], 0");                          // zero family/port/flowinfo half
    emitter.instruction("mov QWORD PTR [r12 + 8], 0");                          // zero sin6_addr low half
    emitter.instruction("mov QWORD PTR [r12 + 16], 0");                         // zero sin6_addr high half
    emitter.instruction("mov DWORD PTR [r12 + 24], 0");                         // zero scope_id
    emitter.instruction(&format!("mov WORD PTR [r12 + 0], {}", af_inet6));      // Linux sin6_family = AF_INET6 (10)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // reload the parsed port
    emitter.instruction("mov rax, rdx");                                        // port for the byte-shuffle
    emitter.instruction("shr rax, 8");                                          // high byte of the port
    emitter.instruction("mov BYTE PTR [r12 + 2], al");                          // sin6_port high byte (network byte order)
    emitter.instruction("mov BYTE PTR [r12 + 3], dl");                          // sin6_port low byte
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // sin6_addr low half from the scratch
    emitter.instruction("mov QWORD PTR [r12 + 8], rax");                        // store sin6_addr low half
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // sin6_addr high half
    emitter.instruction("mov QWORD PTR [r12 + 16], rax");                       // store sin6_addr high half

    emitter.instruction("mov eax, 28");                                         // success: sockaddr_in6 addrlen
    emitter.instruction("add rsp, 64");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the addrlen

    emitter.label("__rt_bsi6_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 reports a failed sockaddr_in6 build
    emitter.instruction("add rsp, 64");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
