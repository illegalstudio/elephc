//! Purpose:
//! Emits the `__rt_stream_socket_sendto` runtime helper, which sends a
//! datagram or stream message through the `sendto` system call.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - An empty address sends on the connected peer (NULL `sockaddr`); a
//!   `[scheme://]A.B.C.D:port` address is parsed and a 16-byte
//!   `sockaddr_in` is built for an explicit IPv4 destination.
//! - `unix://path` and `udg://path` addresses build a `sockaddr_un` whose
//!   `sun_path` starts at offset 2 on both macOS and Linux. The same
//!   sendto path is taken — the kernel routes the datagram based on the
//!   socket's address family.
//! - Returns the number of bytes sent, or -1 on failure.

use crate::codegen::{emit::Emitter, platform::Arch, platform::Platform};

/// stream_socket_sendto: send a message on a socket descriptor.
/// Input:  AArch64 x0=fd, x1=data ptr, x2=data len, x3=flags, x4=addr ptr, x5=addr len
///         x86_64  rdi=fd, rsi=data ptr, rdx=data len, rcx=flags, r8=addr ptr, r9=addr len
/// Output: bytes sent, or -1 on failure
pub fn emit_stream_socket_sendto(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_socket_sendto_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_sendto ---");
    emitter.label_global("__rt_stream_socket_sendto");

    // Frame (192 bytes): [0..16) saved regs, [16) fd, [24) data ptr,
    //   [32) data len, [40) flags, [48..176) sockaddr buffer (fits
    //   sockaddr_un / sockaddr_in), [176) addrlen for the syscall.
    emitter.instruction("sub sp, sp, #192");                                    // frame for saved regs, args, sockaddr buffer
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the socket descriptor
    emitter.instruction("str x1, [sp, #24]");                                   // save the data pointer
    emitter.instruction("str x2, [sp, #32]");                                   // save the data length
    emitter.instruction("str x3, [sp, #40]");                                   // save the send flags
    emitter.instruction("cbz x5, __rt_sst_connected");                          // empty address: send on the connected peer

    // -- bracketed-host detection: route `[ipv6]:port` targets through the
    //    IPv6 sockaddr_in6 builder before the unix:// / udg:// / IPv4
    //    dispatch. RFC 3986 forbids '[' in plain hostnames, so the scan
    //    safely separates IPv6 from everything else.
    emitter.instruction("mov x9, #0");                                          // bracket scan index
    emitter.label("__rt_sst_try_v6");
    emitter.instruction("cmp x9, x5");                                          // reached the end of the address?
    emitter.instruction("b.ge __rt_sst_try_unix");                              // no '[' found → continue with unix:// / IPv4
    emitter.instruction("ldrb w10, [x4, x9]");                                  // load the candidate byte
    emitter.instruction("cmp w10, #91");                                        // ASCII '['
    emitter.instruction("b.eq __rt_sst_build_inet6");                           // bracketed host: IPv6 path
    emitter.instruction("add x9, x9, #1");                                      // keep scanning
    emitter.instruction("b __rt_sst_try_v6");                                   // continue

    emitter.label("__rt_sst_build_inet6");
    // -- delegate to __rt_build_sockaddr_in6 with our sockaddr buffer --
    emitter.instruction("mov x0, x4");                                          // address pointer
    emitter.instruction("mov x1, x5");                                          // address length
    emitter.instruction("add x2, sp, #48");                                     // out buffer = sendto's sockaddr slot
    emitter.instruction("bl __rt_build_sockaddr_in6");                          // x0 = 28 on success, -1 on failure
    emitter.instruction("cmp x0, #0");                                          // did the build fail?
    emitter.instruction("b.lt __rt_sst_fail");                                  // propagate the failure
    emitter.instruction("str x0, [sp, #176]");                                  // save the sockaddr_in6 addrlen (28)
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the socket descriptor
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the data pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the data length
    emitter.instruction("ldr x3, [sp, #40]");                                   // reload the send flags
    emitter.instruction("add x4, sp, #48");                                     // pointer to the destination sockaddr_in6
    emitter.instruction("ldr x5, [sp, #176]");                                  // sockaddr_in6 addrlen
    emitter.instruction("b __rt_sst_send");                                     // issue the sendto syscall

    emitter.label("__rt_sst_try_unix");
    // -- detect unix:// / udg:// schemes before falling through to IPv4 --
    emitter.instruction("cmp x5, #7");                                          // "unix://" needs at least seven bytes
    emitter.instruction("b.lt __rt_sst_try_udg");                               // too short for unix://: try udg://
    emitter.instruction("ldrb w9, [x4, #0]");                                   // load scheme byte 0
    emitter.instruction("cmp w9, #117");                                        // is it 'u'?
    emitter.instruction("b.ne __rt_sst_try_udg");                               // not the unix scheme: try udg://
    emitter.instruction("ldrb w9, [x4, #1]");                                   // load scheme byte 1
    emitter.instruction("cmp w9, #110");                                        // is it 'n'?
    emitter.instruction("b.ne __rt_sst_try_udg");                               // not the unix scheme: try udg://
    emitter.instruction("ldrb w9, [x4, #2]");                                   // load scheme byte 2
    emitter.instruction("cmp w9, #105");                                        // is it 'i'?
    emitter.instruction("b.ne __rt_sst_try_udg");                               // not the unix scheme: try udg://
    emitter.instruction("ldrb w9, [x4, #3]");                                   // load scheme byte 3
    emitter.instruction("cmp w9, #120");                                        // is it 'x'?
    emitter.instruction("b.ne __rt_sst_try_udg");                               // not the unix scheme: try udg://
    emitter.instruction("ldrb w9, [x4, #4]");                                   // load scheme byte 4
    emitter.instruction("cmp w9, #58");                                         // is it ':'?
    emitter.instruction("b.ne __rt_sst_try_udg");                               // not the unix scheme: try udg://
    emitter.instruction("ldrb w9, [x4, #5]");                                   // load scheme byte 5
    emitter.instruction("cmp w9, #47");                                         // is it '/'?
    emitter.instruction("b.ne __rt_sst_try_udg");                               // not the unix scheme: try udg://
    emitter.instruction("ldrb w9, [x4, #6]");                                   // load scheme byte 6
    emitter.instruction("cmp w9, #47");                                         // is it '/'?
    emitter.instruction("b.ne __rt_sst_try_udg");                               // not the unix scheme: try udg://
    emitter.instruction("add x4, x4, #7");                                      // skip the "unix://" prefix to the path
    emitter.instruction("sub x5, x5, #7");                                      // adjust the length past the scheme
    emitter.instruction("b __rt_sst_build_unix");                               // common Unix-domain sockaddr build

    emitter.label("__rt_sst_try_udg");
    emitter.instruction("cmp x5, #6");                                          // "udg://" needs at least six bytes
    emitter.instruction("b.lt __rt_sst_inet");                                  // too short for any Unix scheme
    emitter.instruction("ldrb w9, [x4, #0]");                                   // load scheme byte 0
    emitter.instruction("cmp w9, #117");                                        // is it 'u'?
    emitter.instruction("b.ne __rt_sst_inet");                                  // not the udg scheme
    emitter.instruction("ldrb w9, [x4, #1]");                                   // load scheme byte 1
    emitter.instruction("cmp w9, #100");                                        // is it 'd'?
    emitter.instruction("b.ne __rt_sst_inet");                                  // not the udg scheme
    emitter.instruction("ldrb w9, [x4, #2]");                                   // load scheme byte 2
    emitter.instruction("cmp w9, #103");                                        // is it 'g'?
    emitter.instruction("b.ne __rt_sst_inet");                                  // not the udg scheme
    emitter.instruction("ldrb w9, [x4, #3]");                                   // load scheme byte 3
    emitter.instruction("cmp w9, #58");                                         // is it ':'?
    emitter.instruction("b.ne __rt_sst_inet");                                  // not the udg scheme
    emitter.instruction("ldrb w9, [x4, #4]");                                   // load scheme byte 4
    emitter.instruction("cmp w9, #47");                                         // is it '/'?
    emitter.instruction("b.ne __rt_sst_inet");                                  // not the udg scheme
    emitter.instruction("ldrb w9, [x4, #5]");                                   // load scheme byte 5
    emitter.instruction("cmp w9, #47");                                         // is it '/'?
    emitter.instruction("b.ne __rt_sst_inet");                                  // not the udg scheme
    emitter.instruction("add x4, x4, #6");                                      // skip the "udg://" prefix to the path
    emitter.instruction("sub x5, x5, #6");                                      // adjust the length past the scheme

    // -- build a sockaddr_un at [sp, #48] from the path at (x4, x5) --
    emitter.label("__rt_sst_build_unix");
    if matches!(plat, Platform::MacOS) {
        emitter.instruction("add w9, w5, #3");                                  // sun_len = 2 family + path + NUL
        emitter.instruction("strb w9, [sp, #48]");                              // macOS sockaddr_un begins with sun_len
        emitter.instruction("mov w9, #1");                                      // AF_UNIX
        emitter.instruction("strb w9, [sp, #49]");                              // store sin_family
    } else {
        emitter.instruction("mov w9, #1");                                      // Linux sun_family is a 2-byte field
        emitter.instruction("strb w9, [sp, #48]");                              // store the family low byte
        emitter.instruction("strb wzr, [sp, #49]");                             // store the family high byte
    }
    emitter.instruction("add x11, sp, #50");                                    // sun_path destination cursor
    emitter.instruction("mov x12, #0");                                         // copy index
    emitter.label("__rt_sst_unix_copy");
    emitter.instruction("cmp x12, x5");                                         // copied every path byte?
    emitter.instruction("b.hs __rt_sst_unix_copy_done");                        // copy complete
    emitter.instruction("ldrb w13, [x4, x12]");                                 // load a path byte
    emitter.instruction("strb w13, [x11, x12]");                                // store it into sun_path
    emitter.instruction("add x12, x12, #1");                                    // advance the copy index
    emitter.instruction("b __rt_sst_unix_copy");                                // keep copying
    emitter.label("__rt_sst_unix_copy_done");
    emitter.instruction("strb wzr, [x11, x12]");                                // NUL-terminate sun_path
    emitter.instruction("add x9, x5, #3");                                      // addrlen = 2 family + path + NUL
    emitter.instruction("str x9, [sp, #176]");                                  // save the sockaddr_un addrlen

    // -- sendto(fd, data, len, flags, &sockaddr_un, addrlen) --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the socket descriptor
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the data pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the data length
    emitter.instruction("ldr x3, [sp, #40]");                                   // reload the send flags
    emitter.instruction("add x4, sp, #48");                                     // pointer to the destination sockaddr_un
    emitter.instruction("ldr x5, [sp, #176]");                                  // sockaddr_un addrlen
    emitter.instruction("b __rt_sst_send");                                     // issue the sendto syscall

    // -- IPv4 fallback: parse "[tcp|udp://]A.B.C.D:port" --
    emitter.label("__rt_sst_inet");
    emitter.instruction("mov x0, x4");                                          // address string pointer
    emitter.instruction("mov x1, x5");                                          // address string length
    emitter.instruction("bl __rt_inet_addr_parse");                             // x0 = packed IPv4 or -1, x1 = port
    emitter.instruction("cmp x0, #0");                                          // did the address fail to parse?
    emitter.instruction("b.lt __rt_sst_fail");                                  // bail out on a bad address

    // -- build the 16-byte sockaddr_in at [sp, #48] --
    if matches!(plat, Platform::MacOS) {
        emitter.instruction("mov w9, #16");                                     // macOS sockaddr_in begins with sin_len
        emitter.instruction("strb w9, [sp, #48]");                              // store sin_len
        emitter.instruction("mov w9, #2");                                      // AF_INET
        emitter.instruction("strb w9, [sp, #49]");                              // store sin_family
    } else {
        emitter.instruction("mov w9, #2");                                      // Linux sin_family is a 2-byte field
        emitter.instruction("strb w9, [sp, #48]");                              // store the family low byte
        emitter.instruction("strb wzr, [sp, #49]");                             // store the family high byte
    }
    emitter.instruction("lsr x10, x1, #8");                                     // high byte of the port
    emitter.instruction("strb w10, [sp, #50]");                                 // sin_port is network byte order
    emitter.instruction("strb w1, [sp, #51]");                                  // low byte of the port
    emitter.instruction("lsr x10, x0, #24");                                    // address octet 0
    emitter.instruction("strb w10, [sp, #52]");                                 // store octet 0
    emitter.instruction("lsr x10, x0, #16");                                    // address octet 1
    emitter.instruction("strb w10, [sp, #53]");                                 // store octet 1
    emitter.instruction("lsr x10, x0, #8");                                     // address octet 2
    emitter.instruction("strb w10, [sp, #54]");                                 // store octet 2
    emitter.instruction("strb w0, [sp, #55]");                                  // store octet 3
    emitter.instruction("str xzr, [sp, #56]");                                  // zero the sockaddr_in tail

    // -- sendto(fd, data, len, flags, &sockaddr_in, 16) --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the socket descriptor
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the data pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the data length
    emitter.instruction("ldr x3, [sp, #40]");                                   // reload the send flags
    emitter.instruction("add x4, sp, #48");                                     // pointer to the destination sockaddr
    emitter.instruction("mov x5, #16");                                         // sockaddr_in length
    emitter.instruction("b __rt_sst_send");                                     // issue the sendto syscall

    // -- empty address: send on the connected peer --
    emitter.label("__rt_sst_connected");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the socket descriptor
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the data pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the data length
    emitter.instruction("ldr x3, [sp, #40]");                                   // reload the send flags
    emitter.instruction("mov x4, #0");                                          // NULL destination address
    emitter.instruction("mov x5, #0");                                          // zero destination address length

    emitter.label("__rt_sst_send");
    emitter.syscall(133);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_sst_ok"));        // continue when sendto succeeded
    emitter.instruction("mov x0, #-1");                                         // sendto failed: report -1
    emitter.label("__rt_sst_ok");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #192");                                    // release the frame
    emitter.instruction("ret");                                                 // return the byte count or -1

    emitter.label("__rt_sst_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 reports a failed send
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #192");                                    // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

fn emit_stream_socket_sendto_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_sendto ---");
    emitter.label_global("__rt_stream_socket_sendto");

    // Frame (rbp-relative): [-8) fd, [-16) data ptr, [-24) data len,
    //   [-32) flags, [-160..-32) sockaddr buffer (fits sockaddr_un / sockaddr_in),
    //   [-168) addrlen, [-176) saved addr ptr, [-184) saved addr len.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 192");                                        // frame for args and the sockaddr buffer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the socket descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the data pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the data length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the send flags
    emitter.instruction("mov QWORD PTR [rbp - 176], r8");                       // save the address pointer
    emitter.instruction("mov QWORD PTR [rbp - 184], r9");                       // save the address length
    emitter.instruction("test r9, r9");                                         // is the address length zero?
    emitter.instruction("jz __rt_sst_connected_x86");                           // empty address: send on the connected peer

    // -- bracketed-host detection: route `[ipv6]:port` targets through the
    //    IPv6 sockaddr_in6 builder before the unix:// / udg:// / IPv4
    //    dispatch.
    emitter.instruction("xor rcx, rcx");                                        // bracket scan index
    emitter.label("__rt_sst_try_v6_x86");
    emitter.instruction("cmp rcx, r9");                                         // reached the end of the address?
    emitter.instruction("jae __rt_sst_try_unix_x86");                           // no '[' found → continue with unix:// / IPv4
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load the candidate byte
    emitter.instruction("cmp eax, 91");                                         // ASCII '['
    emitter.instruction("je __rt_sst_build_inet6_x86");                         // bracketed host: IPv6 path
    emitter.instruction("inc rcx");                                             // keep scanning
    emitter.instruction("jmp __rt_sst_try_v6_x86");                             // continue

    emitter.label("__rt_sst_build_inet6_x86");
    // -- delegate to __rt_build_sockaddr_in6 with our sockaddr buffer --
    emitter.instruction("mov rdi, r8");                                         // address pointer
    emitter.instruction("mov rsi, r9");                                         // address length
    emitter.instruction("lea rdx, [rbp - 160]");                                // out buffer = sendto's sockaddr slot
    emitter.instruction("call __rt_build_sockaddr_in6");                        // rax = 28 on success, -1 on failure
    emitter.instruction("cmp rax, 0");                                          // did the build fail?
    emitter.instruction("jl __rt_sst_fail_x86");                                // propagate the failure
    emitter.instruction("mov QWORD PTR [rbp - 168], rax");                      // save the sockaddr_in6 addrlen (28)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the socket descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the data pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the data length
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the send flags
    emitter.instruction("lea r8, [rbp - 160]");                                 // pointer to the destination sockaddr_in6
    emitter.instruction("mov r9, QWORD PTR [rbp - 168]");                       // sockaddr_in6 addrlen
    emitter.instruction("jmp __rt_sst_send_x86");                               // issue the sendto syscall

    emitter.label("__rt_sst_try_unix_x86");
    // -- detect unix:// / udg:// schemes before falling through to IPv4 --
    emitter.instruction("cmp r9, 7");                                           // "unix://" needs at least seven bytes
    emitter.instruction("jl __rt_sst_try_udg_x86");                             // too short for unix://: try udg://
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load scheme byte 0
    emitter.instruction("cmp eax, 117");                                        // is it 'u'?
    emitter.instruction("jne __rt_sst_try_udg_x86");                            // not the unix scheme: try udg://
    emitter.instruction("movzx eax, BYTE PTR [r8 + 1]");                        // load scheme byte 1
    emitter.instruction("cmp eax, 110");                                        // is it 'n'?
    emitter.instruction("jne __rt_sst_try_udg_x86");                            // not the unix scheme: try udg://
    emitter.instruction("movzx eax, BYTE PTR [r8 + 2]");                        // load scheme byte 2
    emitter.instruction("cmp eax, 105");                                        // is it 'i'?
    emitter.instruction("jne __rt_sst_try_udg_x86");                            // not the unix scheme: try udg://
    emitter.instruction("movzx eax, BYTE PTR [r8 + 3]");                        // load scheme byte 3
    emitter.instruction("cmp eax, 120");                                        // is it 'x'?
    emitter.instruction("jne __rt_sst_try_udg_x86");                            // not the unix scheme: try udg://
    emitter.instruction("movzx eax, BYTE PTR [r8 + 4]");                        // load scheme byte 4
    emitter.instruction("cmp eax, 58");                                         // is it ':'?
    emitter.instruction("jne __rt_sst_try_udg_x86");                            // not the unix scheme: try udg://
    emitter.instruction("movzx eax, BYTE PTR [r8 + 5]");                        // load scheme byte 5
    emitter.instruction("cmp eax, 47");                                         // is it '/'?
    emitter.instruction("jne __rt_sst_try_udg_x86");                            // not the unix scheme: try udg://
    emitter.instruction("movzx eax, BYTE PTR [r8 + 6]");                        // load scheme byte 6
    emitter.instruction("cmp eax, 47");                                         // is it '/'?
    emitter.instruction("jne __rt_sst_try_udg_x86");                            // not the unix scheme: try udg://
    emitter.instruction("add r8, 7");                                           // skip the "unix://" prefix to the path
    emitter.instruction("sub r9, 7");                                           // adjust the length past the scheme
    emitter.instruction("jmp __rt_sst_build_unix_x86");                         // common Unix-domain sockaddr build

    emitter.label("__rt_sst_try_udg_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 176]");                       // reload the address pointer (clobbered above)
    emitter.instruction("mov r9, QWORD PTR [rbp - 184]");                       // reload the address length
    emitter.instruction("cmp r9, 6");                                           // "udg://" needs at least six bytes
    emitter.instruction("jl __rt_sst_inet_x86");                                // too short for any Unix scheme
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load scheme byte 0
    emitter.instruction("cmp eax, 117");                                        // is it 'u'?
    emitter.instruction("jne __rt_sst_inet_x86");                               // not the udg scheme
    emitter.instruction("movzx eax, BYTE PTR [r8 + 1]");                        // load scheme byte 1
    emitter.instruction("cmp eax, 100");                                        // is it 'd'?
    emitter.instruction("jne __rt_sst_inet_x86");                               // not the udg scheme
    emitter.instruction("movzx eax, BYTE PTR [r8 + 2]");                        // load scheme byte 2
    emitter.instruction("cmp eax, 103");                                        // is it 'g'?
    emitter.instruction("jne __rt_sst_inet_x86");                               // not the udg scheme
    emitter.instruction("movzx eax, BYTE PTR [r8 + 3]");                        // load scheme byte 3
    emitter.instruction("cmp eax, 58");                                         // is it ':'?
    emitter.instruction("jne __rt_sst_inet_x86");                               // not the udg scheme
    emitter.instruction("movzx eax, BYTE PTR [r8 + 4]");                        // load scheme byte 4
    emitter.instruction("cmp eax, 47");                                         // is it '/'?
    emitter.instruction("jne __rt_sst_inet_x86");                               // not the udg scheme
    emitter.instruction("movzx eax, BYTE PTR [r8 + 5]");                        // load scheme byte 5
    emitter.instruction("cmp eax, 47");                                         // is it '/'?
    emitter.instruction("jne __rt_sst_inet_x86");                               // not the udg scheme
    emitter.instruction("add r8, 6");                                           // skip the "udg://" prefix to the path
    emitter.instruction("sub r9, 6");                                           // adjust the length past the scheme

    // -- build a sockaddr_un at [rbp - 160] from the path at (r8, r9) --
    emitter.label("__rt_sst_build_unix_x86");
    emitter.instruction("mov WORD PTR [rbp - 160], 1");                         // Linux sun_family = AF_UNIX
    emitter.instruction("lea r10, [rbp - 158]");                                // sun_path destination cursor
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_sst_unix_copy_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied every path byte?
    emitter.instruction("jae __rt_sst_unix_copy_done_x86");                     // copy complete
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load a path byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], al");                        // store it into sun_path
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_sst_unix_copy_x86");                          // keep copying
    emitter.label("__rt_sst_unix_copy_done_x86");
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0");                         // NUL-terminate sun_path
    emitter.instruction("mov rax, r9");                                         // path length
    emitter.instruction("add rax, 3");                                          // addrlen = 2 family + path + NUL
    emitter.instruction("mov QWORD PTR [rbp - 168], rax");                      // save the sockaddr_un addrlen

    // -- sendto(fd, data, len, flags, &sockaddr_un, addrlen) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the socket descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the data pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the data length
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the send flags
    emitter.instruction("lea r8, [rbp - 160]");                                 // pointer to the destination sockaddr_un
    emitter.instruction("mov r9, QWORD PTR [rbp - 168]");                       // sockaddr_un addrlen
    emitter.instruction("jmp __rt_sst_send_x86");                               // issue the sendto syscall

    // -- IPv4 fallback: parse "[tcp|udp://]A.B.C.D:port" --
    emitter.label("__rt_sst_inet_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 176]");                      // reload the address pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 184]");                      // reload the address length
    emitter.instruction("call __rt_inet_addr_parse");                           // rax = packed IPv4 or -1, rdx = port
    emitter.instruction("test rax, rax");                                       // did the address fail to parse?
    emitter.instruction("js __rt_sst_fail_x86");                                // bail out on a bad address

    // -- build the 16-byte sockaddr_in at [rbp - 160] --
    emitter.instruction("mov WORD PTR [rbp - 160], 2");                         // Linux sin_family = AF_INET
    emitter.instruction("mov rcx, rdx");                                        // copy the port for the high byte
    emitter.instruction("shr rcx, 8");                                          // high byte of the port
    emitter.instruction("mov BYTE PTR [rbp - 158], cl");                        // sin_port is network byte order
    emitter.instruction("mov BYTE PTR [rbp - 157], dl");                        // low byte of the port
    emitter.instruction("mov rcx, rax");                                        // copy for shifting
    emitter.instruction("shr rcx, 24");                                         // address octet 0
    emitter.instruction("mov BYTE PTR [rbp - 156], cl");                        // store octet 0
    emitter.instruction("mov rcx, rax");                                        // copy for shifting
    emitter.instruction("shr rcx, 16");                                         // address octet 1
    emitter.instruction("mov BYTE PTR [rbp - 155], cl");                        // store octet 1
    emitter.instruction("mov rcx, rax");                                        // copy for shifting
    emitter.instruction("shr rcx, 8");                                          // address octet 2
    emitter.instruction("mov BYTE PTR [rbp - 154], cl");                        // store octet 2
    emitter.instruction("mov BYTE PTR [rbp - 153], al");                        // store octet 3
    emitter.instruction("mov QWORD PTR [rbp - 152], 0");                        // zero the sockaddr_in tail

    // -- sendto(fd, data, len, flags, &sockaddr_in, 16) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the socket descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the data pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the data length
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the send flags
    emitter.instruction("lea r8, [rbp - 160]");                                 // pointer to the destination sockaddr
    emitter.instruction("mov r9d, 16");                                         // sockaddr_in length
    emitter.instruction("jmp __rt_sst_send_x86");                               // issue the sendto syscall

    // -- empty address: send on the connected peer --
    emitter.label("__rt_sst_connected_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the socket descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the data pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the data length
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the send flags
    emitter.instruction("xor r8d, r8d");                                        // NULL destination address
    emitter.instruction("xor r9d, r9d");                                        // zero destination address length

    emitter.label("__rt_sst_send_x86");
    emitter.instruction("mov eax, 44");                                         // Linux x86_64 syscall 44 = sendto
    emitter.instruction("syscall");                                             // send the message
    emitter.instruction("cmp rax, 0");                                          // did sendto fail?
    emitter.instruction("jl __rt_sst_fail_x86");                                // a negative result means failure
    emitter.instruction("add rsp, 192");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the byte count

    emitter.label("__rt_sst_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 reports a failed send
    emitter.instruction("add rsp, 192");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
