//! Purpose:
//! Emits the `__rt_stream_socket_server_v6` runtime helper, which opens a
//! listening TCP socket on a literal `[ipv6]:port` address. Mirrors
//! `__rt_stream_socket_client_v6` and shares the same bracketed-host
//! detection in the dispatcher.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_stream_socket_server`'s bracket-detection probe.
//!
//! Key details:
//! - Accepts a `[tcp://]?[ipv6_literal]:port` address. v1 only supports the
//!   bracketed-literal form (no DNS, no UDP).
//! - Sets `SO_REUSEADDR` so back-to-back test runs don't trip the
//!   `[::1]:port` already-bound check while a previous bind is in
//!   `TIME_WAIT`.
//! - Returns the listening descriptor, or -1 on any failure.

use crate::codegen_support::{emit::Emitter, platform::Arch, platform::Platform};

/// stream_socket_server_v6: open a bound IPv6 socket on
/// `[scheme://]?[ipv6_literal]:port`. The socket type is passed in by the
/// dispatcher so this one helper covers both `tcp://` (SOCK_STREAM, with
/// listen()) and `udp://` (SOCK_DGRAM, bind-only) IPv6 servers.
/// Input:  AArch64 x0 = address pointer, x1 = address length, x2 = sock_type
///         x86_64  rdi = address pointer, rsi = address length, rdx = sock_type
///         where sock_type is 1 (SOCK_STREAM) or 2 (SOCK_DGRAM).
/// Output: bound descriptor, or -1 on failure
pub fn emit_stream_socket_server_v6(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_socket_server_v6_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let af_inet6 = plat.af_inet6();
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_server_v6 ---");
    emitter.label_global("__rt_stream_socket_server_v6");

    // Frame (128 bytes): [0..16) saved x29/x30, [16) addr ptr, [24) addr len,
    //   [32) fd, [40..56) sin6_addr scratch (16 bytes), [56..84) sockaddr_in6
    //   (28 bytes), [84..88) SO_REUSEADDR scratch, [88..96) sock_type,
    //   [96..104) literal ptr stash (DNS fallback), [104..112) literal len,
    //   [112..120) close-bracket index stash (libc calls clobber x6).
    emitter.instruction("sub sp, sp, #128");                                    // frame for saved regs, parse state, sockaddr_in6
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the address pointer
    emitter.instruction("str x1, [sp, #24]");                                   // save the address length
    emitter.instruction("str x2, [sp, #88]");                                   // save the sock_type (1 stream, 2 dgram)

    // -- locate '[' --
    emitter.instruction("mov x3, #0");                                          // bracket scan index
    emitter.label("__rt_sssv6_find_open");
    emitter.instruction("cmp x3, x1");                                          // reached the end of the address?
    emitter.instruction("b.ge __rt_sssv6_fail");                                // no '[' found → bad address
    emitter.instruction("ldrb w4, [x0, x3]");                                   // load the candidate byte
    emitter.instruction("cmp w4, #91");                                         // ASCII '['
    emitter.instruction("b.eq __rt_sssv6_found_open");                          // start of the literal
    emitter.instruction("add x3, x3, #1");                                      // keep scanning
    emitter.instruction("b __rt_sssv6_find_open");                              // continue
    emitter.label("__rt_sssv6_found_open");
    emitter.instruction("add x5, x3, #1");                                      // literal start = byte after '['

    // -- locate ']' --
    emitter.instruction("mov x6, x5");                                          // scan from the literal start
    emitter.label("__rt_sssv6_find_close");
    emitter.instruction("cmp x6, x1");                                          // reached the end?
    emitter.instruction("b.ge __rt_sssv6_fail");                                // no ']' → malformed
    emitter.instruction("ldrb w4, [x0, x6]");                                   // load the candidate byte
    emitter.instruction("cmp w4, #93");                                         // ASCII ']'
    emitter.instruction("b.eq __rt_sssv6_found_close");                         // end of the literal
    emitter.instruction("add x6, x6, #1");                                      // keep scanning
    emitter.instruction("b __rt_sssv6_find_close");                             // continue
    emitter.label("__rt_sssv6_found_close");
    emitter.instruction("sub x7, x6, x5");                                      // literal length = close - start

    // -- parse the IPv6 literal into the 16-byte sin6_addr scratch --
    emitter.instruction("add x0, x0, x5");                                      // literal pointer = address + start
    emitter.instruction("mov x1, x7");                                          // literal length
    emitter.instruction("add x2, sp, #40");                                     // out buffer = sin6_addr scratch
    emitter.instruction("str x0, [sp, #96]");                                   // stash literal ptr for the DNS fallback
    emitter.instruction("str x1, [sp, #104]");                                  // stash literal len for the DNS fallback
    emitter.instruction("str x6, [sp, #112]");                                  // stash close-bracket index — libc calls below may clobber x6
    emitter.instruction("bl __rt_inet6_pton");                                  // x0 = 1 on success, 0 on failure
    emitter.instruction("cbnz x0, __rt_sssv6_addr_ok");                         // pton succeeded — skip the DNS fallback
    // -- DNS fallback (Phase 11 B1) --
    emitter.instruction("ldr x0, [sp, #96]");                                   // reload literal ptr
    emitter.instruction("ldr x1, [sp, #104]");                                  // reload literal len
    emitter.instruction("add x2, sp, #40");                                     // out buffer
    emitter.instruction("bl __rt_resolve_host_v6");                             // getaddrinfo with AF_INET6 hint
    emitter.instruction("cbz x0, __rt_sssv6_fail");                             // pton and DNS both rejected → bail
    emitter.label("__rt_sssv6_addr_ok");
    emitter.instruction("ldr x6, [sp, #112]");                                  // reload close-bracket index after the libc calls

    // -- expect ':' then decimal port immediately after the ']' --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the address pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the address length
    emitter.instruction("add x8, x6, #1");                                      // index of the ':' after ']'
    emitter.instruction("cmp x8, x1");                                          // is there room for ':port'?
    emitter.instruction("b.ge __rt_sssv6_fail");                                // missing ':port'
    emitter.instruction("ldrb w4, [x0, x8]");                                   // load the byte after ']'
    emitter.instruction("cmp w4, #58");                                         // ':'?
    emitter.instruction("b.ne __rt_sssv6_fail");                                // not the port separator
    emitter.instruction("add x9, x8, #1");                                      // first port digit index
    emitter.instruction("mov x10, #0");                                         // accumulated port value
    emitter.instruction("cmp x9, x1");                                          // any port digits at all?
    emitter.instruction("b.ge __rt_sssv6_fail");                                // empty port
    emitter.label("__rt_sssv6_port");
    emitter.instruction("cmp x9, x1");                                          // consumed every byte?
    emitter.instruction("b.ge __rt_sssv6_port_done");                           // port parsed
    emitter.instruction("ldrb w4, [x0, x9]");                                   // load the digit
    emitter.instruction("cmp w4, #48");                                         // ASCII '0'
    emitter.instruction("b.lt __rt_sssv6_fail");                                // non-digit → bail
    emitter.instruction("cmp w4, #57");                                         // ASCII '9'
    emitter.instruction("b.gt __rt_sssv6_fail");                                // non-digit → bail
    emitter.instruction("sub w4, w4, #48");                                     // digit value
    emitter.instruction("mov x11, #10");                                        // decimal base
    emitter.instruction("mul x10, x10, x11");                                   // shift port one decimal place
    emitter.instruction("add x10, x10, x4");                                    // add the new digit
    emitter.instruction("add x9, x9, #1");                                      // advance the digit cursor
    emitter.instruction("b __rt_sssv6_port");                                   // continue
    emitter.label("__rt_sssv6_port_done");

    // -- build the 28-byte sockaddr_in6 at [sp, #56] --
    emitter.instruction("str xzr, [sp, #56]");                                  // zero family/port/flowinfo
    emitter.instruction("str xzr, [sp, #64]");                                  // zero sin6_addr low half
    emitter.instruction("str xzr, [sp, #72]");                                  // zero sin6_addr high half
    emitter.instruction("str wzr, [sp, #80]");                                  // zero scope_id
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

    // -- copy the parsed sin6_addr (16 bytes) --
    emitter.instruction("ldr x12, [sp, #40]");                                  // sin6_addr low word
    emitter.instruction("ldr x13, [sp, #48]");                                  // sin6_addr high word
    emitter.instruction("str x12, [sp, #64]");                                  // store at sockaddr offset 8
    emitter.instruction("str x13, [sp, #72]");                                  // store at sockaddr offset 16

    // -- socket(AF_INET6, sock_type, 0) --
    emitter.instruction(&format!("mov x0, #{}", af_inet6));                     // family: AF_INET6
    emitter.instruction("ldr x1, [sp, #88]");                                   // sock_type from the dispatcher (1=STREAM, 2=DGRAM)
    emitter.instruction("mov x2, #0");                                          // default protocol
    emitter.syscall(97);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative descriptor means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_sssv6_sock_ok")); // continue when socket succeeded
    emitter.instruction("b __rt_sssv6_fail");                                   // socket() failed
    emitter.label("__rt_sssv6_sock_ok");
    emitter.instruction("str x0, [sp, #32]");                                   // save the socket descriptor

    // -- setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &1, 4) --
    // SOL_SOCKET = 0xffff (macOS) / 1 (Linux); SO_REUSEADDR = 4 / 2.
    {
        let (sol_socket, so_reuseaddr): (i64, i64) = match plat {
            Platform::MacOS => (0xffff, 4),
            Platform::Linux => (1, 2),
        };
        emitter.instruction("mov w11, #1");                                     // SO_REUSEADDR option value = 1
        emitter.instruction("str w11, [sp, #84]");                              // stash the option value in stack scratch
        emitter.instruction("ldr x0, [sp, #32]");                               // reload the socket descriptor
        emitter.instruction(&format!("mov x1, #{}", sol_socket));               // level: SOL_SOCKET
        emitter.instruction(&format!("mov x2, #{}", so_reuseaddr));             // name: SO_REUSEADDR
        emitter.instruction("add x3, sp, #84");                                 // pointer to the option value
        emitter.instruction("mov x4, #4");                                      // option length = sizeof(int)
        emitter.syscall(105);
    }

    // -- bind(fd, &sockaddr_in6, 28) --
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the socket descriptor
    emitter.instruction("add x1, sp, #56");                                     // pointer to the sockaddr_in6
    emitter.instruction("mov x2, #28");                                         // sockaddr_in6 length
    emitter.syscall(104);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_sssv6_bind_ok")); // continue when bind succeeded
    emitter.instruction("b __rt_sssv6_fail_close");                             // bind() failed

    emitter.label("__rt_sssv6_bind_ok");
    // -- listen(fd, 128) for SOCK_STREAM only; SOCK_DGRAM has no accept() --
    emitter.instruction("ldr x9, [sp, #88]");                                   // reload the sock_type
    emitter.instruction("cmp x9, #1");                                          // is this a SOCK_STREAM (tcp://) server?
    emitter.instruction("b.ne __rt_sssv6_listen_ok");                           // SOCK_DGRAM (udp://) skips listen(): bind alone suffices
    emitter.instruction("bl __rt_socket_backlog");                              // resolve the configured backlog (default 128)
    emitter.instruction("mov x1, x0");                                          // backlog → listen() arg 1
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the socket descriptor (after the call clobbers x0)
    emitter.syscall(106);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_sssv6_listen_ok")); // continue when listen succeeded
    emitter.instruction("b __rt_sssv6_fail_close");                             // listen() failed

    emitter.label("__rt_sssv6_listen_ok");
    emitter.instruction("ldr x0, [sp, #32]");                                   // return the listening descriptor
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the frame
    emitter.instruction("ret");                                                 // return the listening socket

    emitter.label("__rt_sssv6_fail_close");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the socket descriptor
    emitter.syscall(6);

    emitter.label("__rt_sssv6_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 reports a failed IPv6 server socket
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for stream socket server v6.
fn emit_stream_socket_server_v6_linux_x86_64(emitter: &mut Emitter) {
    let af_inet6 = Platform::Linux.af_inet6();
    let sol_socket = 1i64; // Linux SOL_SOCKET
    let so_reuseaddr = 2i64; // Linux SO_REUSEADDR
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_server_v6 ---");
    emitter.label_global("__rt_stream_socket_server_v6");

    // Frame (rbp-relative):
    //   [-8)   addr ptr
    //   [-16)  addr len
    //   [-24)  fd
    //   [-32)  parsed port
    //   [-40)  close-bracket scratch
    //   [-56..-40) sin6_addr scratch (16 bytes from __rt_inet6_pton)
    //   [-88..-56) sockaddr_in6 (28 bytes + padding)
    //   [-96..-88) SO_REUSEADDR option value scratch
    //   [-104..-96) sock_type carried across the syscalls
    //   [-112..-104) literal ptr stash for the DNS fallback (Phase 11 B1)
    //   [-120..-112) literal len stash for the DNS fallback
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 128");                                        // frame for saved state and sockaddr_in6
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the address pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the address length
    emitter.instruction("mov QWORD PTR [rbp - 104], rdx");                      // save the sock_type (1 stream, 2 dgram)

    // -- locate '[' --
    emitter.instruction("xor rcx, rcx");                                        // bracket scan index
    emitter.label("__rt_sssv6_find_open_x86");
    emitter.instruction("cmp rcx, rsi");                                        // reached the end?
    emitter.instruction("jae __rt_sssv6_fail_x86");                             // no '[' found → bad address
    emitter.instruction("movzx eax, BYTE PTR [rdi + rcx]");                     // load the candidate byte
    emitter.instruction("cmp eax, 91");                                         // ASCII '['
    emitter.instruction("je __rt_sssv6_found_open_x86");                        // start of the literal
    emitter.instruction("inc rcx");                                             // keep scanning
    emitter.instruction("jmp __rt_sssv6_find_open_x86");                        // continue
    emitter.label("__rt_sssv6_found_open_x86");
    emitter.instruction("lea r8, [rcx + 1]");                                   // literal start = byte after '['

    // -- locate ']' --
    emitter.instruction("mov r9, r8");                                          // scan from the literal start
    emitter.label("__rt_sssv6_find_close_x86");
    emitter.instruction("cmp r9, rsi");                                         // reached the end?
    emitter.instruction("jae __rt_sssv6_fail_x86");                             // no ']' → malformed
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");                      // load the candidate byte
    emitter.instruction("cmp eax, 93");                                         // ASCII ']'
    emitter.instruction("je __rt_sssv6_found_close_x86");                       // end of the literal
    emitter.instruction("inc r9");                                              // keep scanning
    emitter.instruction("jmp __rt_sssv6_find_close_x86");                       // continue
    emitter.label("__rt_sssv6_found_close_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // stash the close-bracket index across libc/syscalls

    // -- parse the IPv6 literal into the 16-byte sin6_addr scratch --
    emitter.instruction("mov r10, r9");                                         // literal end for length math
    emitter.instruction("sub r10, r8");                                         // literal length = close - start
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the address pointer
    emitter.instruction("add rdi, r8");                                         // literal pointer = address + start
    emitter.instruction("mov rsi, r10");                                        // literal length
    emitter.instruction("lea rdx, [rbp - 56]");                                 // out buffer = sin6_addr scratch (16 bytes)
    emitter.instruction("mov QWORD PTR [rbp - 112], rdi");                      // stash literal ptr for the DNS fallback
    emitter.instruction("mov QWORD PTR [rbp - 120], rsi");                      // stash literal len for the DNS fallback
    emitter.instruction("call __rt_inet6_pton");                                // rax = 1 on success, 0 on failure
    emitter.instruction("test rax, rax");                                       // did the literal parse?
    emitter.instruction("jnz __rt_sssv6_addr_ok_x86");                          // pton succeeded — skip the DNS fallback
    // -- DNS fallback (Phase 11 B1) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 112]");                      // reload literal ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 120]");                      // reload literal len
    emitter.instruction("lea rdx, [rbp - 56]");                                 // out buffer = sin6_addr scratch
    emitter.instruction("call __rt_resolve_host_v6");                           // getaddrinfo with AF_INET6 hint
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_sssv6_fail_x86");                              // pton and DNS both rejected → bail
    emitter.label("__rt_sssv6_addr_ok_x86");

    // -- expect ':' then decimal port immediately after the ']' --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the address pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the address length
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the close-bracket index
    emitter.instruction("lea r11, [r9 + 1]");                                   // index of the ':' after ']'
    emitter.instruction("cmp r11, rsi");                                        // is there room for ':port'?
    emitter.instruction("jae __rt_sssv6_fail_x86");                             // missing ':port'
    emitter.instruction("movzx eax, BYTE PTR [rdi + r11]");                     // load the byte after ']'
    emitter.instruction("cmp eax, 58");                                         // ':'?
    emitter.instruction("jne __rt_sssv6_fail_x86");                             // not the port separator
    emitter.instruction("lea rcx, [r11 + 1]");                                  // first port digit index
    emitter.instruction("xor edx, edx");                                        // accumulated port value
    emitter.instruction("cmp rcx, rsi");                                        // any port digits at all?
    emitter.instruction("jae __rt_sssv6_fail_x86");                             // empty port
    emitter.label("__rt_sssv6_port_x86");
    emitter.instruction("cmp rcx, rsi");                                        // consumed every byte?
    emitter.instruction("jae __rt_sssv6_port_done_x86");                        // port parsed
    emitter.instruction("movzx eax, BYTE PTR [rdi + rcx]");                     // load the digit
    emitter.instruction("cmp eax, 48");                                         // ASCII '0'
    emitter.instruction("jl __rt_sssv6_fail_x86");                              // non-digit → bail
    emitter.instruction("cmp eax, 57");                                         // ASCII '9'
    emitter.instruction("jg __rt_sssv6_fail_x86");                              // non-digit → bail
    emitter.instruction("sub eax, 48");                                         // digit value
    emitter.instruction("imul rdx, rdx, 10");                                   // shift port one decimal place
    emitter.instruction("add rdx, rax");                                        // add the new digit
    emitter.instruction("inc rcx");                                             // advance the digit cursor
    emitter.instruction("jmp __rt_sssv6_port_x86");                             // continue
    emitter.label("__rt_sssv6_port_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // stash the parsed port

    // -- build the 28-byte sockaddr_in6 at [rbp - 88] --
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // zero family/port/flowinfo half
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // zero sin6_addr low half
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // zero sin6_addr high half
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // zero scope_id + tail
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
    emitter.instruction("mov rsi, QWORD PTR [rbp - 104]");                      // sock_type from the dispatcher (1=STREAM, 2=DGRAM)
    emitter.instruction("xor edx, edx");                                        // default protocol
    emitter.instruction("mov eax, 41");                                         // Linux x86_64 syscall 41 = socket
    emitter.instruction("syscall");                                             // create the IPv6 socket
    emitter.instruction("test rax, rax");                                       // did socket() fail?
    emitter.instruction("js __rt_sssv6_fail_x86");                              // negative descriptor → failure
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the socket descriptor

    // -- setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &1, 4) --
    emitter.instruction("mov DWORD PTR [rbp - 96], 1");                         // SO_REUSEADDR option value = 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the socket descriptor
    emitter.instruction(&format!("mov esi, {}", sol_socket));                   // level: SOL_SOCKET (Linux = 1)
    emitter.instruction(&format!("mov edx, {}", so_reuseaddr));                 // name: SO_REUSEADDR (Linux = 2)
    emitter.instruction("lea r10, [rbp - 96]");                                 // pointer to the option value
    emitter.instruction("mov r8d, 4");                                          // option length = sizeof(int)
    emitter.instruction("mov eax, 54");                                         // Linux x86_64 syscall 54 = setsockopt
    emitter.instruction("syscall");                                             // apply SO_REUSEADDR (ignore return)

    // -- bind(fd, &sockaddr_in6, 28) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the socket descriptor
    emitter.instruction("lea rsi, [rbp - 88]");                                 // pointer to the sockaddr_in6
    emitter.instruction("mov edx, 28");                                         // sockaddr_in6 length
    emitter.instruction("mov eax, 49");                                         // Linux x86_64 syscall 49 = bind
    emitter.instruction("syscall");                                             // bind the IPv6 socket
    emitter.instruction("test rax, rax");                                       // did bind() fail?
    emitter.instruction("js __rt_sssv6_fail_close_x86");                        // bind() failed

    // -- listen(fd, 128) for SOCK_STREAM only; SOCK_DGRAM has no accept() --
    emitter.instruction("mov r10, QWORD PTR [rbp - 104]");                      // reload the sock_type
    emitter.instruction("cmp r10, 1");                                          // is this a SOCK_STREAM (tcp://) server?
    emitter.instruction("jne __rt_sssv6_done_x86");                             // SOCK_DGRAM (udp://) skips listen()
    emitter.instruction("call __rt_socket_backlog");                            // resolve the configured backlog (default 128)
    emitter.instruction("mov esi, eax");                                        // backlog → listen() arg 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the socket descriptor (after the call clobbers rax)
    emitter.instruction("mov eax, 50");                                         // Linux x86_64 syscall 50 = listen
    emitter.instruction("syscall");                                             // mark the socket as listening
    emitter.instruction("test rax, rax");                                       // did listen() fail?
    emitter.instruction("js __rt_sssv6_fail_close_x86");                        // listen() failed

    emitter.label("__rt_sssv6_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the bound descriptor
    emitter.instruction("add rsp, 128");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the bound socket

    emitter.label("__rt_sssv6_fail_close_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the socket descriptor
    emitter.instruction("mov eax, 3");                                          // Linux x86_64 syscall 3 = close
    emitter.instruction("syscall");                                             // close the failed socket

    emitter.label("__rt_sssv6_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 reports a failed IPv6 server socket
    emitter.instruction("add rsp, 128");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
