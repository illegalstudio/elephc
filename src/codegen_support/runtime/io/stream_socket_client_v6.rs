//! Purpose:
//! Emits the `__rt_stream_socket_client_v6` runtime helper, which opens a
//! connected TCP socket to a literal `[ipv6]:port` destination. The IPv4
//! dispatcher in `__rt_stream_socket_client` tail-branches here when the
//! address contains a `[`, so existing IPv4 clients keep their fast path.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_stream_socket_client`'s bracket-detection probe.
//!
//! Key details:
//! - Accepts a `[tcp://]?[ipv6_literal]:port` address. v1 only supports the
//!   bracketed-literal form; no DNS, no UDP (a `udp://` IPv6 client falls
//!   through to the IPv4 parser, where the bracketed host fails to parse).
//! - Builds a 28-byte `sockaddr_in6`: family + port + flowinfo + sin6_addr +
//!   scope_id. macOS keeps a 1-byte `sin6_len` ahead of the family byte
//!   (BSD layout); Linux uses a 2-byte `sin6_family` directly. `AF_INET6`
//!   is 30 on macOS and 10 on Linux (see `Platform::af_inet6`).
//! - Returns the connected descriptor, or -1 on any failure (bad literal,
//!   socket / connect error).

use crate::codegen_support::{emit::Emitter, platform::Arch, platform::Platform};

/// stream_socket_client_v6: open a connected IPv6 socket to
/// `[scheme://]?[ipv6_literal]:port`. The socket type is passed in by the
/// dispatcher so this one helper covers both `tcp://` (SOCK_STREAM) and
/// `udp://` (SOCK_DGRAM) IPv6 clients.
/// Input:  AArch64 x0 = address pointer, x1 = address length, x2 = sock_type
///         x86_64  rdi = address pointer, rsi = address length, rdx = sock_type
///         where sock_type is 1 (SOCK_STREAM) or 2 (SOCK_DGRAM).
/// Output: connected descriptor, or -1 on failure
pub fn emit_stream_socket_client_v6(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_socket_client_v6_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let af_inet6 = plat.af_inet6();
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_client_v6 ---");
    emitter.label_global("__rt_stream_socket_client_v6");

    // Frame (128 bytes): [0..16) saved x29/x30, [16) addr ptr, [24) addr len,
    //   [32) fd, [40..56) sin6_addr scratch (16 bytes), [56..84) sockaddr_in6
    //   (28 bytes), [84..92) sock_type carried across syscalls, [92..96) padding,
    //   [96..104) literal ptr stash (for DNS fallback), [104..112) literal len,
    //   [112..120) close-bracket index stash (DNS path's bl clobbers x6).
    emitter.instruction("sub sp, sp, #128");                                    // frame for saved regs, parse state, sockaddr_in6
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the address pointer
    emitter.instruction("str x1, [sp, #24]");                                   // save the address length
    emitter.instruction("str x2, [sp, #84]");                                   // save the sock_type (1 stream, 2 dgram)

    // -- locate '[' anywhere in the address: marks the start of the literal --
    emitter.instruction("mov x3, #0");                                          // bracket scan index
    emitter.label("__rt_sscv6_find_open");
    emitter.instruction("cmp x3, x1");                                          // reached the end of the address?
    emitter.instruction("b.ge __rt_sscv6_fail");                                // no '[' found → bad address
    emitter.instruction("ldrb w4, [x0, x3]");                                   // load the candidate byte
    emitter.instruction("cmp w4, #91");                                         // ASCII '['
    emitter.instruction("b.eq __rt_sscv6_found_open");                          // start of the literal
    emitter.instruction("add x3, x3, #1");                                      // keep scanning
    emitter.instruction("b __rt_sscv6_find_open");                              // continue
    emitter.label("__rt_sscv6_found_open");
    emitter.instruction("add x5, x3, #1");                                      // literal start = byte after '['

    // -- locate ']': end of the literal --
    emitter.instruction("mov x6, x5");                                          // scan from the literal start
    emitter.label("__rt_sscv6_find_close");
    emitter.instruction("cmp x6, x1");                                          // reached the end?
    emitter.instruction("b.ge __rt_sscv6_fail");                                // no ']' → malformed
    emitter.instruction("ldrb w4, [x0, x6]");                                   // load the candidate byte
    emitter.instruction("cmp w4, #93");                                         // ASCII ']'
    emitter.instruction("b.eq __rt_sscv6_found_close");                         // end of the literal
    emitter.instruction("add x6, x6, #1");                                      // keep scanning
    emitter.instruction("b __rt_sscv6_find_close");                             // continue
    emitter.label("__rt_sscv6_found_close");
    emitter.instruction("sub x7, x6, x5");                                      // literal length = close - start

    // -- parse the IPv6 literal into the 16-byte sin6_addr slot --
    emitter.instruction("add x0, x0, x5");                                      // literal pointer = address + start
    emitter.instruction("mov x1, x7");                                          // literal length
    emitter.instruction("add x2, sp, #40");                                     // out buffer = sin6_addr scratch
    emitter.instruction("str x0, [sp, #96]");                                   // stash literal ptr for the DNS fallback
    emitter.instruction("str x1, [sp, #104]");                                  // stash literal len for the DNS fallback
    emitter.instruction("str x6, [sp, #112]");                                  // stash close-bracket index — libc calls below may clobber x6
    emitter.instruction("bl __rt_inet6_pton");                                  // x0 = 1 on success, 0 on failure
    emitter.instruction("cbnz x0, __rt_sscv6_addr_ok");                         // pton succeeded — skip the DNS fallback
    // -- DNS fallback: resolve the bracketed token as a hostname (Phase 11 B1) --
    emitter.instruction("ldr x0, [sp, #96]");                                   // reload literal ptr
    emitter.instruction("ldr x1, [sp, #104]");                                  // reload literal len
    emitter.instruction("add x2, sp, #40");                                     // out buffer = sin6_addr scratch
    emitter.instruction("bl __rt_resolve_host_v6");                             // getaddrinfo with AF_INET6 hint
    emitter.instruction("cbz x0, __rt_sscv6_fail");                             // pton and DNS both rejected → bail
    emitter.label("__rt_sscv6_addr_ok");
    emitter.instruction("ldr x6, [sp, #112]");                                  // reload close-bracket index after the libc calls

    // -- expect ':' then decimal port immediately after the ']' --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the address pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the address length
    emitter.instruction("add x8, x6, #1");                                      // index of the ':' after ']'
    emitter.instruction("cmp x8, x1");                                          // is there room for ':port'?
    emitter.instruction("b.ge __rt_sscv6_fail");                                // missing ':port'
    emitter.instruction("ldrb w4, [x0, x6]");                                   // re-check the closing bracket position
    emitter.instruction("cmp w4, #93");                                         // ']'?
    emitter.instruction("b.ne __rt_sscv6_fail");                                // not a bracket → unreachable but defensive
    emitter.instruction("ldrb w4, [x0, x8]");                                   // load the byte after ']'
    emitter.instruction("cmp w4, #58");                                         // ':'?
    emitter.instruction("b.ne __rt_sscv6_fail");                                // not the port separator
    emitter.instruction("add x9, x8, #1");                                      // first port digit index
    emitter.instruction("mov x10, #0");                                         // accumulated port value
    emitter.instruction("cmp x9, x1");                                          // any port digits at all?
    emitter.instruction("b.ge __rt_sscv6_fail");                                // empty port
    emitter.label("__rt_sscv6_port");
    emitter.instruction("cmp x9, x1");                                          // consumed every byte?
    emitter.instruction("b.ge __rt_sscv6_port_done");                           // port parsed
    emitter.instruction("ldrb w4, [x0, x9]");                                   // load the digit
    emitter.instruction("cmp w4, #48");                                         // ASCII '0'
    emitter.instruction("b.lt __rt_sscv6_fail");                                // non-digit → bail
    emitter.instruction("cmp w4, #57");                                         // ASCII '9'
    emitter.instruction("b.gt __rt_sscv6_fail");                                // non-digit → bail
    emitter.instruction("sub w4, w4, #48");                                     // digit value
    emitter.instruction("mov x11, #10");                                        // decimal base
    emitter.instruction("mul x10, x10, x11");                                   // shift port one decimal place
    emitter.instruction("add x10, x10, x4");                                    // add the new digit
    emitter.instruction("add x9, x9, #1");                                      // advance the digit cursor
    emitter.instruction("b __rt_sscv6_port");                                   // continue
    emitter.label("__rt_sscv6_port_done");

    // -- build the 28-byte sockaddr_in6 at [sp, #56] --
    emitter.instruction("str xzr, [sp, #56]");                                  // zero the first 8 bytes (family/port/flowinfo low)
    emitter.instruction("str xzr, [sp, #64]");                                  // zero bytes 8..16 of the sockaddr_in6
    emitter.instruction("str xzr, [sp, #72]");                                  // zero bytes 16..24
    emitter.instruction("str wzr, [sp, #80]");                                  // zero the trailing 4 bytes (scope_id)
    if matches!(plat, Platform::MacOS) {
        emitter.instruction("mov w11, #28");                                    // sin6_len = sizeof(sockaddr_in6)
        emitter.instruction("strb w11, [sp, #56]");                             // macOS keeps sin6_len ahead of sin6_family
        emitter.instruction(&format!("mov w11, #{}", af_inet6));                // AF_INET6 = 30 on macOS
        emitter.instruction("strb w11, [sp, #57]");                             // store sin6_family in the second byte
    } else {
        emitter.instruction(&format!("mov w11, #{}", af_inet6));                // AF_INET6 = 10 on Linux
        emitter.instruction("strb w11, [sp, #56]");                             // sin6_family low byte
        emitter.instruction("strb wzr, [sp, #57]");                             // sin6_family high byte
    }
    emitter.instruction("lsr x12, x10, #8");                                    // high byte of the port
    emitter.instruction("strb w12, [sp, #58]");                                 // sin6_port is network byte order
    emitter.instruction("strb w10, [sp, #59]");                                 // low byte of the port

    // -- copy the parsed sin6_addr (16 bytes) into the sockaddr_in6 --
    emitter.instruction("ldr x12, [sp, #40]");                                  // sin6_addr low word
    emitter.instruction("ldr x13, [sp, #48]");                                  // sin6_addr high word
    emitter.instruction("str x12, [sp, #64]");                                  // store the low half (sockaddr offset 8..16)
    emitter.instruction("str x13, [sp, #72]");                                  // store the high half (sockaddr offset 16..24)

    // -- socket(AF_INET6, sock_type, 0) --
    emitter.instruction(&format!("mov x0, #{}", af_inet6));                     // family: AF_INET6
    emitter.instruction("ldr x1, [sp, #84]");                                   // sock_type from the dispatcher (1=STREAM, 2=DGRAM)
    emitter.instruction("mov x2, #0");                                          // default protocol
    emitter.syscall(97);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative descriptor means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_sscv6_sock_ok")); // continue when socket succeeded
    emitter.instruction("b __rt_sscv6_fail");                                   // socket() failed
    emitter.label("__rt_sscv6_sock_ok");
    emitter.instruction("str x0, [sp, #32]");                                   // save the socket descriptor

    // -- connect(fd, &sockaddr_in6, 28) --
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the socket descriptor
    emitter.instruction("add x1, sp, #56");                                     // pointer to the sockaddr_in6
    emitter.instruction("mov x2, #28");                                         // sockaddr_in6 length
    emitter.syscall(98);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_sscv6_connect_ok")); // continue when connect succeeded
    emitter.instruction("b __rt_sscv6_fail_close");                             // connect() failed

    emitter.label("__rt_sscv6_connect_ok");
    emitter.instruction("ldr x0, [sp, #32]");                                   // return the connected descriptor
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the frame
    emitter.instruction("ret");                                                 // return the connected socket

    emitter.label("__rt_sscv6_fail_close");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the socket descriptor
    emitter.syscall(6);

    emitter.label("__rt_sscv6_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 reports a failed IPv6 connect
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for stream socket client v6.
fn emit_stream_socket_client_v6_linux_x86_64(emitter: &mut Emitter) {
    let af_inet6 = Platform::Linux.af_inet6();
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_client_v6 ---");
    emitter.label_global("__rt_stream_socket_client_v6");

    // Frame (rbp-relative):
    //   [-8)   addr ptr
    //   [-16)  addr len
    //   [-24)  fd
    //   [-32)  parsed port
    //   [-40)  close-bracket scratch (also literal-end index)
    //   [-56..-40) sin6_addr scratch (16 bytes from __rt_inet6_pton)
    //   [-88..-56) sockaddr_in6 (28 bytes + 4 padding)
    //   [-96)  sock_type carried across syscalls
    //   [-104) literal ptr stash for the DNS fallback (Phase 11 B1)
    //   [-112) literal len stash for the DNS fallback (Phase 11 B1)
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 112");                                        // frame for saved state and sockaddr_in6
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the address pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the address length
    emitter.instruction("mov QWORD PTR [rbp - 96], rdx");                       // save the sock_type (1 stream, 2 dgram)

    // -- locate '[' --
    emitter.instruction("xor rcx, rcx");                                        // bracket scan index
    emitter.label("__rt_sscv6_find_open_x86");
    emitter.instruction("cmp rcx, rsi");                                        // reached the end?
    emitter.instruction("jae __rt_sscv6_fail_x86");                             // no '[' found → bad address
    emitter.instruction("movzx eax, BYTE PTR [rdi + rcx]");                     // load the candidate byte
    emitter.instruction("cmp eax, 91");                                         // ASCII '['
    emitter.instruction("je __rt_sscv6_found_open_x86");                        // start of the literal
    emitter.instruction("inc rcx");                                             // keep scanning
    emitter.instruction("jmp __rt_sscv6_find_open_x86");                        // continue
    emitter.label("__rt_sscv6_found_open_x86");
    emitter.instruction("lea r8, [rcx + 1]");                                   // literal start = byte after '['

    // -- locate ']' --
    emitter.instruction("mov r9, r8");                                          // scan from the literal start
    emitter.label("__rt_sscv6_find_close_x86");
    emitter.instruction("cmp r9, rsi");                                         // reached the end?
    emitter.instruction("jae __rt_sscv6_fail_x86");                             // no ']' → malformed
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");                      // load the candidate byte
    emitter.instruction("cmp eax, 93");                                         // ASCII ']'
    emitter.instruction("je __rt_sscv6_found_close_x86");                       // end of the literal
    emitter.instruction("inc r9");                                              // keep scanning
    emitter.instruction("jmp __rt_sscv6_find_close_x86");                       // continue
    emitter.label("__rt_sscv6_found_close_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // stash the close-bracket index across libc/syscalls

    // -- parse the IPv6 literal into the 16-byte sin6_addr scratch slot --
    emitter.instruction("mov r10, r9");                                         // literal end for length math
    emitter.instruction("sub r10, r8");                                         // literal length = close - start
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the address pointer
    emitter.instruction("add rdi, r8");                                         // literal pointer = address + start
    emitter.instruction("mov rsi, r10");                                        // literal length
    emitter.instruction("lea rdx, [rbp - 56]");                                 // out buffer = sin6_addr scratch (16 bytes)
    emitter.instruction("mov QWORD PTR [rbp - 104], rdi");                      // stash literal ptr for the DNS fallback
    emitter.instruction("mov QWORD PTR [rbp - 112], rsi");                      // stash literal len for the DNS fallback
    emitter.instruction("call __rt_inet6_pton");                                // rax = 1 on success, 0 on failure
    emitter.instruction("test rax, rax");                                       // did the literal parse?
    emitter.instruction("jnz __rt_sscv6_addr_ok_x86");                          // pton succeeded — skip the DNS fallback
    // -- DNS fallback (Phase 11 B1) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 104]");                      // reload literal ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 112]");                      // reload literal len
    emitter.instruction("lea rdx, [rbp - 56]");                                 // out buffer = sin6_addr scratch
    emitter.instruction("call __rt_resolve_host_v6");                           // getaddrinfo with AF_INET6 hint
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_sscv6_fail_x86");                              // pton and DNS both rejected → bail
    emitter.label("__rt_sscv6_addr_ok_x86");

    // -- expect ':' then decimal port immediately after the ']' --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the address pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the address length
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the close-bracket index
    emitter.instruction("lea r11, [r9 + 1]");                                   // index of the ':' after ']'
    emitter.instruction("cmp r11, rsi");                                        // is there room for ':port'?
    emitter.instruction("jae __rt_sscv6_fail_x86");                             // missing ':port'
    emitter.instruction("movzx eax, BYTE PTR [rdi + r11]");                     // load the byte after ']'
    emitter.instruction("cmp eax, 58");                                         // ':'?
    emitter.instruction("jne __rt_sscv6_fail_x86");                             // not the port separator
    emitter.instruction("lea rcx, [r11 + 1]");                                  // first port digit index
    emitter.instruction("xor edx, edx");                                        // accumulated port value
    emitter.instruction("cmp rcx, rsi");                                        // any port digits at all?
    emitter.instruction("jae __rt_sscv6_fail_x86");                             // empty port
    emitter.label("__rt_sscv6_port_x86");
    emitter.instruction("cmp rcx, rsi");                                        // consumed every byte?
    emitter.instruction("jae __rt_sscv6_port_done_x86");                        // port parsed
    emitter.instruction("movzx eax, BYTE PTR [rdi + rcx]");                     // load the digit
    emitter.instruction("cmp eax, 48");                                         // ASCII '0'
    emitter.instruction("jl __rt_sscv6_fail_x86");                              // non-digit → bail
    emitter.instruction("cmp eax, 57");                                         // ASCII '9'
    emitter.instruction("jg __rt_sscv6_fail_x86");                              // non-digit → bail
    emitter.instruction("sub eax, 48");                                         // digit value
    emitter.instruction("imul rdx, rdx, 10");                                   // shift port one decimal place
    emitter.instruction("add rdx, rax");                                        // add the new digit
    emitter.instruction("inc rcx");                                             // advance the digit cursor
    emitter.instruction("jmp __rt_sscv6_port_x86");                             // continue
    emitter.label("__rt_sscv6_port_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // stash the parsed port across the syscalls

    // -- build the 28-byte sockaddr_in6 at [rbp - 88] --
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // zero family/port/flowinfo half (8 bytes)
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // zero the first half of sin6_addr
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // zero the second half of sin6_addr
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // zero scope_id + tail padding
    emitter.instruction(&format!("mov WORD PTR [rbp - 88], {}", af_inet6));     // Linux sin6_family = AF_INET6 (10)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the parsed port
    emitter.instruction("mov rax, rdx");                                        // port for the byte-shuffle
    emitter.instruction("shr rax, 8");                                          // high byte of the port
    emitter.instruction("mov BYTE PTR [rbp - 86], al");                         // sin6_port high byte (network byte order)
    emitter.instruction("mov BYTE PTR [rbp - 85], dl");                         // sin6_port low byte

    // -- copy the parsed sin6_addr (16 bytes) into the sockaddr_in6 --
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // sin6_addr low half
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // store at sockaddr offset 8
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // sin6_addr high half
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // store at sockaddr offset 16

    // -- socket(AF_INET6, sock_type, 0) --
    emitter.instruction(&format!("mov edi, {}", af_inet6));                     // family: AF_INET6
    emitter.instruction("mov rsi, QWORD PTR [rbp - 96]");                       // sock_type from the dispatcher (1=STREAM, 2=DGRAM)
    emitter.instruction("xor edx, edx");                                        // default protocol
    emitter.instruction("mov eax, 41");                                         // Linux x86_64 syscall 41 = socket
    emitter.instruction("syscall");                                             // create the IPv6 socket
    emitter.instruction("test rax, rax");                                       // did socket() fail?
    emitter.instruction("js __rt_sscv6_fail_x86");                              // negative descriptor → failure
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the socket descriptor

    // -- connect(fd, &sockaddr_in6, 28) --
    emitter.instruction("mov rdi, rax");                                        // socket descriptor
    emitter.instruction("lea rsi, [rbp - 88]");                                 // pointer to the sockaddr_in6
    emitter.instruction("mov edx, 28");                                         // sockaddr_in6 length
    emitter.instruction("mov eax, 42");                                         // Linux x86_64 syscall 42 = connect
    emitter.instruction("syscall");                                             // connect the IPv6 socket
    emitter.instruction("test rax, rax");                                       // did connect() fail?
    emitter.instruction("js __rt_sscv6_fail_close_x86");                        // connect() failed

    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the connected descriptor
    emitter.instruction("add rsp, 112");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the connected socket

    emitter.label("__rt_sscv6_fail_close_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the socket descriptor
    emitter.instruction("mov eax, 3");                                          // Linux x86_64 syscall 3 = close
    emitter.instruction("syscall");                                             // close the failed socket

    emitter.label("__rt_sscv6_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 reports a failed IPv6 connect
    emitter.instruction("add rsp, 112");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
