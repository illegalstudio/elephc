//! Purpose:
//! Emits the `__rt_stream_socket_server` runtime helper assembly for the
//! stream_socket_server builtin.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - Creates a TCP listening socket: `socket` + `bind` + `listen` on the
//!   parsed `[tcp://]A.B.C.D:port` address; returns the descriptor or -1.

use crate::codegen::{emit::Emitter, platform::Arch, platform::Platform};

/// stream_socket_server: open a listening TCP socket on an IPv4 address.
/// Input:  x0 = address string pointer, x1 = address string length
/// Output: x0 = listening descriptor, or -1 on failure
pub fn emit_stream_socket_server(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_socket_server_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_server ---");
    emitter.label_global("__rt_stream_socket_server");

    // -- bracketed-host detection: any '[' in the address routes us to the
    //    IPv6 helper. Mirrors __rt_stream_socket_client's probe so both
    //    sides of an IPv6 connection share the same dispatch policy.
    emitter.instruction("mov x9, #0");                                          // bracket scan index
    emitter.label("__rt_stream_socket_server_try_v6");
    emitter.instruction("cmp x9, x1");                                          // reached the end of the address?
    emitter.instruction("b.ge __rt_stream_socket_server_after_v6_probe");       // no '[' found → continue with unix:// / IPv4
    emitter.instruction("ldrb w10, [x0, x9]");                                  // load the candidate byte
    emitter.instruction("cmp w10, #91");                                        // ASCII '['
    emitter.instruction("b.eq __rt_stream_socket_server_v6_scheme");            // bracketed host: pick sock_type then tail-call
    emitter.instruction("add x9, x9, #1");                                      // keep scanning
    emitter.instruction("b __rt_stream_socket_server_try_v6");                  // continue

    emitter.label("__rt_stream_socket_server_v6_scheme");
    // -- pick sock_type for the v6 helper: "udp://" prefix → SOCK_DGRAM,
    //    everything else (tcp://, bare bracketed host) → SOCK_STREAM.
    emitter.instruction("mov x2, #1");                                          // default: SOCK_STREAM
    emitter.instruction("cmp x1, #6");                                          // "udp://" needs at least six bytes
    emitter.instruction("b.lt __rt_stream_socket_server_v6_dispatch");          // too short for udp:// → keep STREAM
    emitter.instruction("ldrb w10, [x0, #0]");                                  // load scheme byte 0
    emitter.instruction("cmp w10, #117");                                       // 'u'?
    emitter.instruction("b.ne __rt_stream_socket_server_v6_dispatch");          // not udp scheme
    emitter.instruction("ldrb w10, [x0, #1]");                                  // load scheme byte 1
    emitter.instruction("cmp w10, #100");                                       // 'd'?
    emitter.instruction("b.ne __rt_stream_socket_server_v6_dispatch");          // not udp scheme
    emitter.instruction("ldrb w10, [x0, #2]");                                  // load scheme byte 2
    emitter.instruction("cmp w10, #112");                                       // 'p'?
    emitter.instruction("b.ne __rt_stream_socket_server_v6_dispatch");          // not udp scheme
    emitter.instruction("ldrb w10, [x0, #3]");                                  // load scheme byte 3
    emitter.instruction("cmp w10, #58");                                        // ':'?
    emitter.instruction("b.ne __rt_stream_socket_server_v6_dispatch");          // not udp scheme
    emitter.instruction("ldrb w10, [x0, #4]");                                  // load scheme byte 4
    emitter.instruction("cmp w10, #47");                                        // '/'?
    emitter.instruction("b.ne __rt_stream_socket_server_v6_dispatch");          // not udp scheme
    emitter.instruction("ldrb w10, [x0, #5]");                                  // load scheme byte 5
    emitter.instruction("cmp w10, #47");                                        // '/'?
    emitter.instruction("b.ne __rt_stream_socket_server_v6_dispatch");          // not udp scheme
    emitter.instruction("mov x2, #2");                                          // SOCK_DGRAM for udp://
    emitter.label("__rt_stream_socket_server_v6_dispatch");
    emitter.instruction("b __rt_stream_socket_server_v6");                      // tail-call into the IPv6 helper
    emitter.label("__rt_stream_socket_server_after_v6_probe");

    // -- redirect unix:// (SOCK_STREAM) addresses to the Unix-domain helper --
    emitter.instruction("cmp x1, #7");                                          // "unix://" needs at least seven bytes
    emitter.instruction("b.lt __rt_stream_socket_server_try_udg");              // too short for unix://: try udg://
    emitter.instruction("ldrb w9, [x0, #0]");                                   // load scheme byte 0
    emitter.instruction("cmp w9, #117");                                        // is it 'u'?
    emitter.instruction("b.ne __rt_stream_socket_server_try_udg");              // not 'u': try udg://
    emitter.instruction("ldrb w9, [x0, #1]");                                   // load scheme byte 1
    emitter.instruction("cmp w9, #110");                                        // is it 'n'?
    emitter.instruction("b.ne __rt_stream_socket_server_try_udg");              // not unix scheme: try udg://
    emitter.instruction("ldrb w9, [x0, #2]");                                   // load scheme byte 2
    emitter.instruction("cmp w9, #105");                                        // is it 'i'?
    emitter.instruction("b.ne __rt_stream_socket_server_try_udg");              // not unix scheme: try udg://
    emitter.instruction("ldrb w9, [x0, #3]");                                   // load scheme byte 3
    emitter.instruction("cmp w9, #120");                                        // is it 'x'?
    emitter.instruction("b.ne __rt_stream_socket_server_try_udg");              // not unix scheme: try udg://
    emitter.instruction("ldrb w9, [x0, #4]");                                   // load scheme byte 4
    emitter.instruction("cmp w9, #58");                                         // is it ':'?
    emitter.instruction("b.ne __rt_stream_socket_server_try_udg");              // not unix scheme: try udg://
    emitter.instruction("ldrb w9, [x0, #5]");                                   // load scheme byte 5
    emitter.instruction("cmp w9, #47");                                         // is it '/'?
    emitter.instruction("b.ne __rt_stream_socket_server_try_udg");              // not unix scheme: try udg://
    emitter.instruction("ldrb w9, [x0, #6]");                                   // load scheme byte 6
    emitter.instruction("cmp w9, #47");                                         // is it '/'?
    emitter.instruction("b.ne __rt_stream_socket_server_try_udg");              // not unix scheme: try udg://
    emitter.instruction("add x0, x0, #7");                                      // skip the "unix://" prefix to the path
    emitter.instruction("sub x1, x1, #7");                                      // adjust the length past the scheme
    emitter.instruction("mov x2, #1");                                          // SOCK_STREAM for the unix:// helper
    emitter.instruction("b __rt_unix_socket_server");                           // unix:// address: use the Unix-domain helper

    // -- redirect udg:// (SOCK_DGRAM) addresses to the Unix-domain helper --
    emitter.label("__rt_stream_socket_server_try_udg");
    emitter.instruction("cmp x1, #6");                                          // "udg://" needs at least six bytes
    emitter.instruction("b.lt __rt_stream_socket_server_inet");                 // too short for any Unix scheme
    emitter.instruction("ldrb w9, [x0, #0]");                                   // load scheme byte 0
    emitter.instruction("cmp w9, #117");                                        // is it 'u'?
    emitter.instruction("b.ne __rt_stream_socket_server_inet");                 // not the udg scheme
    emitter.instruction("ldrb w9, [x0, #1]");                                   // load scheme byte 1
    emitter.instruction("cmp w9, #100");                                        // is it 'd'?
    emitter.instruction("b.ne __rt_stream_socket_server_inet");                 // not the udg scheme
    emitter.instruction("ldrb w9, [x0, #2]");                                   // load scheme byte 2
    emitter.instruction("cmp w9, #103");                                        // is it 'g'?
    emitter.instruction("b.ne __rt_stream_socket_server_inet");                 // not the udg scheme
    emitter.instruction("ldrb w9, [x0, #3]");                                   // load scheme byte 3
    emitter.instruction("cmp w9, #58");                                         // is it ':'?
    emitter.instruction("b.ne __rt_stream_socket_server_inet");                 // not the udg scheme
    emitter.instruction("ldrb w9, [x0, #4]");                                   // load scheme byte 4
    emitter.instruction("cmp w9, #47");                                         // is it '/'?
    emitter.instruction("b.ne __rt_stream_socket_server_inet");                 // not the udg scheme
    emitter.instruction("ldrb w9, [x0, #5]");                                   // load scheme byte 5
    emitter.instruction("cmp w9, #47");                                         // is it '/'?
    emitter.instruction("b.ne __rt_stream_socket_server_inet");                 // not the udg scheme
    emitter.instruction("add x0, x0, #6");                                      // skip the "udg://" prefix to the path
    emitter.instruction("sub x1, x1, #6");                                      // adjust the length past the scheme
    emitter.instruction("mov x2, #2");                                          // SOCK_DGRAM for the udg:// helper
    emitter.instruction("b __rt_unix_socket_server");                           // udg:// address: use the Unix-domain helper
    emitter.label("__rt_stream_socket_server_inet");

    // Frame: [0..16) saved regs, [16) ip, [24) port, [32) fd, [40..56) sockaddr_in,
    //        [56) addr ptr, [64) addr len, [72) udp transport flag.
    emitter.instruction("sub sp, sp, #80");                                     // frame for saved regs, parse state, sockaddr, scheme
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- detect the udp:// transport before parsing the address --
    emitter.instruction("str x0, [sp, #56]");                                   // save the address pointer across the scheme probe
    emitter.instruction("str x1, [sp, #64]");                                   // save the address length across the scheme probe
    emitter.instruction("bl __rt_addr_is_udp");                                 // x0 = 1 for a udp:// address
    emitter.instruction("str x0, [sp, #72]");                                   // save the udp transport flag
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload the address pointer
    emitter.instruction("ldr x1, [sp, #64]");                                   // reload the address length

    emitter.instruction("bl __rt_inet_addr_parse");                             // x0 = packed IPv4 or -1, x1 = port
    emitter.instruction("cmp x0, #0");                                          // did the address fail to parse?
    emitter.instruction("b.lt __rt_stream_socket_server_fail");                 // bail out on a bad address
    emitter.instruction("str x0, [sp, #16]");                                   // save the packed address
    emitter.instruction("str x1, [sp, #24]");                                   // save the port

    // -- socket(AF_INET, SOCK_STREAM or SOCK_DGRAM, 0) --
    emitter.instruction("mov x0, #2");                                          // AF_INET
    emitter.instruction("ldr x1, [sp, #72]");                                   // load the udp transport flag
    emitter.instruction("add x1, x1, #1");                                      // SOCK_STREAM (1) for tcp, SOCK_DGRAM (2) for udp
    emitter.instruction("mov x2, #0");                                          // default protocol
    emitter.syscall(97);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative descriptor means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_stream_socket_server_sock_ok")); // continue when socket succeeded
    emitter.instruction("b __rt_stream_socket_server_fail");                    // socket() failed
    emitter.label("__rt_stream_socket_server_sock_ok");
    emitter.instruction("str x0, [sp, #32]");                                   // save the socket descriptor
    emitter.instruction("bl __rt_apply_socket_server_opts");                    // apply so_reuseport before bind (best-effort)
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the descriptor (helper clobbers x0)

    // -- build the 16-byte sockaddr_in at [sp, #40] --
    if matches!(plat, Platform::MacOS) {
        emitter.instruction("mov w9, #16");                                     // macOS sockaddr_in begins with sin_len
        emitter.instruction("strb w9, [sp, #40]");                              // store sin_len
        emitter.instruction("mov w9, #2");                                      // AF_INET
        emitter.instruction("strb w9, [sp, #41]");                              // store sin_family
    } else {
        emitter.instruction("mov w9, #2");                                      // Linux sin_family is a 2-byte field
        emitter.instruction("strb w9, [sp, #40]");                              // store the family low byte
        emitter.instruction("strb wzr, [sp, #41]");                             // store the family high byte
    }
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the port
    emitter.instruction("lsr x10, x9, #8");                                     // high byte of the port
    emitter.instruction("strb w10, [sp, #42]");                                 // sin_port is network byte order
    emitter.instruction("strb w9, [sp, #43]");                                  // low byte of the port
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the packed address
    emitter.instruction("lsr x10, x9, #24");                                    // address octet 0
    emitter.instruction("strb w10, [sp, #44]");                                 // store octet 0
    emitter.instruction("lsr x10, x9, #16");                                    // address octet 1
    emitter.instruction("strb w10, [sp, #45]");                                 // store octet 1
    emitter.instruction("lsr x10, x9, #8");                                     // address octet 2
    emitter.instruction("strb w10, [sp, #46]");                                 // store octet 2
    emitter.instruction("strb w9, [sp, #47]");                                  // store octet 3
    emitter.instruction("str xzr, [sp, #48]");                                  // zero the sockaddr_in tail

    // -- bind(fd, &sockaddr, 16) --
    emitter.instruction("ldr x0, [sp, #32]");                                   // socket descriptor
    emitter.instruction("add x1, sp, #40");                                     // pointer to the sockaddr_in
    emitter.instruction("mov x2, #16");                                         // sockaddr_in length
    emitter.syscall(104);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_stream_socket_server_bind_ok")); // continue when bind succeeded
    emitter.instruction("b __rt_stream_socket_server_fail_close");              // bind() failed
    emitter.label("__rt_stream_socket_server_bind_ok");

    // -- a udp socket is ready after bind; only tcp needs listen --
    emitter.instruction("ldr x9, [sp, #72]");                                   // load the udp transport flag
    emitter.instruction("cbnz x9, __rt_stream_socket_server_ok");               // udp sockets skip listen()

    // -- listen(fd, socket.backlog) --
    emitter.instruction("bl __rt_socket_backlog");                              // resolve the configured backlog (default 128)
    emitter.instruction("mov x1, x0");                                          // backlog → listen() arg 1
    emitter.instruction("ldr x0, [sp, #32]");                                   // socket descriptor (reload after the call clobbers x0)
    emitter.syscall(106);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_stream_socket_server_ok")); // continue when listen succeeded
    emitter.instruction("b __rt_stream_socket_server_fail_close");              // listen() failed

    emitter.label("__rt_stream_socket_server_ok");
    emitter.instruction("ldr x0, [sp, #32]");                                   // return the listening descriptor
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the frame
    emitter.instruction("ret");                                                 // return the listening socket

    emitter.label("__rt_stream_socket_server_fail_close");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the socket descriptor
    emitter.syscall(6);

    emitter.label("__rt_stream_socket_server_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 reports a failed server socket
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

fn emit_stream_socket_server_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_server ---");
    emitter.label_global("__rt_stream_socket_server");

    // -- bracketed-host detection: any '[' in the address routes us to the
    //    IPv6 helper. Mirrors __rt_stream_socket_client's probe so both
    //    sides of an IPv6 connection share the same dispatch policy.
    emitter.instruction("xor rcx, rcx");                                        // bracket scan index
    emitter.label("__rt_stream_socket_server_try_v6_x86");
    emitter.instruction("cmp rcx, rsi");                                        // reached the end of the address?
    emitter.instruction("jae __rt_stream_socket_server_after_v6_probe_x86");    // no '[' found → continue with unix:// / IPv4
    emitter.instruction("movzx eax, BYTE PTR [rdi + rcx]");                     // load the candidate byte
    emitter.instruction("cmp eax, 91");                                         // ASCII '['
    emitter.instruction("je __rt_stream_socket_server_v6_scheme_x86");          // bracketed host: pick sock_type then tail-call
    emitter.instruction("inc rcx");                                             // keep scanning
    emitter.instruction("jmp __rt_stream_socket_server_try_v6_x86");            // continue

    emitter.label("__rt_stream_socket_server_v6_scheme_x86");
    // -- pick sock_type for the v6 helper: "udp://" prefix → SOCK_DGRAM,
    //    everything else (tcp://, bare bracketed host) → SOCK_STREAM.
    emitter.instruction("mov edx, 1");                                          // default: SOCK_STREAM
    emitter.instruction("cmp rsi, 6");                                          // "udp://" needs at least six bytes
    emitter.instruction("jl __rt_stream_socket_server_v6_dispatch_x86");        // too short for udp:// → keep STREAM
    emitter.instruction("movzx eax, BYTE PTR [rdi + 0]");                       // load scheme byte 0
    emitter.instruction("cmp eax, 117");                                        // 'u'?
    emitter.instruction("jne __rt_stream_socket_server_v6_dispatch_x86");       // not udp scheme
    emitter.instruction("movzx eax, BYTE PTR [rdi + 1]");                       // load scheme byte 1
    emitter.instruction("cmp eax, 100");                                        // 'd'?
    emitter.instruction("jne __rt_stream_socket_server_v6_dispatch_x86");       // not udp scheme
    emitter.instruction("movzx eax, BYTE PTR [rdi + 2]");                       // load scheme byte 2
    emitter.instruction("cmp eax, 112");                                        // 'p'?
    emitter.instruction("jne __rt_stream_socket_server_v6_dispatch_x86");       // not udp scheme
    emitter.instruction("movzx eax, BYTE PTR [rdi + 3]");                       // load scheme byte 3
    emitter.instruction("cmp eax, 58");                                         // ':'?
    emitter.instruction("jne __rt_stream_socket_server_v6_dispatch_x86");       // not udp scheme
    emitter.instruction("movzx eax, BYTE PTR [rdi + 4]");                       // load scheme byte 4
    emitter.instruction("cmp eax, 47");                                         // '/'?
    emitter.instruction("jne __rt_stream_socket_server_v6_dispatch_x86");       // not udp scheme
    emitter.instruction("movzx eax, BYTE PTR [rdi + 5]");                       // load scheme byte 5
    emitter.instruction("cmp eax, 47");                                         // '/'?
    emitter.instruction("jne __rt_stream_socket_server_v6_dispatch_x86");       // not udp scheme
    emitter.instruction("mov edx, 2");                                          // SOCK_DGRAM for udp://
    emitter.label("__rt_stream_socket_server_v6_dispatch_x86");
    emitter.instruction("jmp __rt_stream_socket_server_v6");                    // tail-call into the IPv6 helper
    emitter.label("__rt_stream_socket_server_after_v6_probe_x86");

    // -- redirect unix:// (SOCK_STREAM) addresses to the Unix-domain helper --
    emitter.instruction("cmp rsi, 7");                                          // "unix://" needs at least seven bytes
    emitter.instruction("jl __rt_stream_socket_server_try_udg_x86");            // too short for unix://: try udg://
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load scheme byte 0
    emitter.instruction("cmp eax, 117");                                        // is it 'u'?
    emitter.instruction("jne __rt_stream_socket_server_try_udg_x86");           // not 'u': try udg://
    emitter.instruction("movzx eax, BYTE PTR [rdi + 1]");                       // load scheme byte 1
    emitter.instruction("cmp eax, 110");                                        // is it 'n'?
    emitter.instruction("jne __rt_stream_socket_server_try_udg_x86");           // not unix scheme: try udg://
    emitter.instruction("movzx eax, BYTE PTR [rdi + 2]");                       // load scheme byte 2
    emitter.instruction("cmp eax, 105");                                        // is it 'i'?
    emitter.instruction("jne __rt_stream_socket_server_try_udg_x86");           // not unix scheme: try udg://
    emitter.instruction("movzx eax, BYTE PTR [rdi + 3]");                       // load scheme byte 3
    emitter.instruction("cmp eax, 120");                                        // is it 'x'?
    emitter.instruction("jne __rt_stream_socket_server_try_udg_x86");           // not unix scheme: try udg://
    emitter.instruction("movzx eax, BYTE PTR [rdi + 4]");                       // load scheme byte 4
    emitter.instruction("cmp eax, 58");                                         // is it ':'?
    emitter.instruction("jne __rt_stream_socket_server_try_udg_x86");           // not unix scheme: try udg://
    emitter.instruction("movzx eax, BYTE PTR [rdi + 5]");                       // load scheme byte 5
    emitter.instruction("cmp eax, 47");                                         // is it '/'?
    emitter.instruction("jne __rt_stream_socket_server_try_udg_x86");           // not unix scheme: try udg://
    emitter.instruction("movzx eax, BYTE PTR [rdi + 6]");                       // load scheme byte 6
    emitter.instruction("cmp eax, 47");                                         // is it '/'?
    emitter.instruction("jne __rt_stream_socket_server_try_udg_x86");           // not unix scheme: try udg://
    emitter.instruction("add rdi, 7");                                          // skip the "unix://" prefix to the path
    emitter.instruction("sub rsi, 7");                                          // adjust the length past the scheme
    emitter.instruction("mov edx, 1");                                          // SOCK_STREAM for the unix:// helper
    emitter.instruction("jmp __rt_unix_socket_server");                         // unix:// address: use the Unix-domain helper

    // -- redirect udg:// (SOCK_DGRAM) addresses to the Unix-domain helper --
    emitter.label("__rt_stream_socket_server_try_udg_x86");
    emitter.instruction("cmp rsi, 6");                                          // "udg://" needs at least six bytes
    emitter.instruction("jl __rt_stream_socket_server_inet_x86");               // too short for any Unix scheme
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load scheme byte 0
    emitter.instruction("cmp eax, 117");                                        // is it 'u'?
    emitter.instruction("jne __rt_stream_socket_server_inet_x86");              // not the udg scheme
    emitter.instruction("movzx eax, BYTE PTR [rdi + 1]");                       // load scheme byte 1
    emitter.instruction("cmp eax, 100");                                        // is it 'd'?
    emitter.instruction("jne __rt_stream_socket_server_inet_x86");              // not the udg scheme
    emitter.instruction("movzx eax, BYTE PTR [rdi + 2]");                       // load scheme byte 2
    emitter.instruction("cmp eax, 103");                                        // is it 'g'?
    emitter.instruction("jne __rt_stream_socket_server_inet_x86");              // not the udg scheme
    emitter.instruction("movzx eax, BYTE PTR [rdi + 3]");                       // load scheme byte 3
    emitter.instruction("cmp eax, 58");                                         // is it ':'?
    emitter.instruction("jne __rt_stream_socket_server_inet_x86");              // not the udg scheme
    emitter.instruction("movzx eax, BYTE PTR [rdi + 4]");                       // load scheme byte 4
    emitter.instruction("cmp eax, 47");                                         // is it '/'?
    emitter.instruction("jne __rt_stream_socket_server_inet_x86");              // not the udg scheme
    emitter.instruction("movzx eax, BYTE PTR [rdi + 5]");                       // load scheme byte 5
    emitter.instruction("cmp eax, 47");                                         // is it '/'?
    emitter.instruction("jne __rt_stream_socket_server_inet_x86");              // not the udg scheme
    emitter.instruction("add rdi, 6");                                          // skip the "udg://" prefix to the path
    emitter.instruction("sub rsi, 6");                                          // adjust the length past the scheme
    emitter.instruction("mov edx, 2");                                          // SOCK_DGRAM for the udg:// helper
    emitter.instruction("jmp __rt_unix_socket_server");                         // udg:// address: use the Unix-domain helper
    emitter.label("__rt_stream_socket_server_inet_x86");

    // Frame: [rbp-8) ip, [rbp-16) port, [rbp-24) fd, [rbp-40..rbp-24) sockaddr_in,
    //        [rbp-48) addr ptr, [rbp-56) addr len, [rbp-64) udp transport flag.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // frame for parse state, the sockaddr, and scheme

    // -- detect the udp:// transport before parsing the address --
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save the address pointer across the scheme probe
    emitter.instruction("mov QWORD PTR [rbp - 56], rsi");                       // save the address length across the scheme probe
    emitter.instruction("call __rt_addr_is_udp");                               // rax = 1 for a udp:// address
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the udp transport flag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the address pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the address length

    emitter.instruction("call __rt_inet_addr_parse");                           // rax = packed IPv4 or -1, rdx = port
    emitter.instruction("test rax, rax");                                       // did the address fail to parse?
    emitter.instruction("js __rt_stream_socket_server_fail_x86");               // bail out on a bad address
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the packed address
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the port

    emitter.instruction("mov edi, 2");                                          // AF_INET
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // load the udp transport flag
    emitter.instruction("add rsi, 1");                                          // SOCK_STREAM (1) for tcp, SOCK_DGRAM (2) for udp
    emitter.instruction("xor edx, edx");                                        // default protocol
    emitter.instruction("mov eax, 41");                                         // Linux x86_64 syscall 41 = socket
    emitter.instruction("syscall");                                             // create the socket
    emitter.instruction("test rax, rax");                                       // did socket() fail?
    emitter.instruction("js __rt_stream_socket_server_fail_x86");               // socket() failed
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the socket descriptor
    emitter.instruction("mov rdi, rax");                                        // pass the fd to the options helper
    emitter.instruction("call __rt_apply_socket_server_opts");                  // apply so_reuseport before bind (best-effort)

    emitter.instruction("mov WORD PTR [rbp - 40], 2");                          // Linux sin_family = AF_INET
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the port
    emitter.instruction("mov rax, rcx");                                        // copy for the high byte
    emitter.instruction("shr rax, 8");                                          // high byte of the port
    emitter.instruction("mov BYTE PTR [rbp - 38], al");                         // sin_port is network byte order
    emitter.instruction("mov BYTE PTR [rbp - 37], cl");                         // low byte of the port
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the packed address
    emitter.instruction("mov rax, rcx");                                        // copy for shifting
    emitter.instruction("shr rax, 24");                                         // address octet 0
    emitter.instruction("mov BYTE PTR [rbp - 36], al");                         // store octet 0
    emitter.instruction("mov rax, rcx");                                        // copy for shifting
    emitter.instruction("shr rax, 16");                                         // address octet 1
    emitter.instruction("mov BYTE PTR [rbp - 35], al");                         // store octet 1
    emitter.instruction("mov rax, rcx");                                        // copy for shifting
    emitter.instruction("shr rax, 8");                                          // address octet 2
    emitter.instruction("mov BYTE PTR [rbp - 34], al");                         // store octet 2
    emitter.instruction("mov BYTE PTR [rbp - 33], cl");                         // store octet 3
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // zero the sockaddr_in tail

    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // socket descriptor
    emitter.instruction("lea rsi, [rbp - 40]");                                 // pointer to the sockaddr_in
    emitter.instruction("mov edx, 16");                                         // sockaddr_in length
    emitter.instruction("mov eax, 49");                                         // Linux x86_64 syscall 49 = bind
    emitter.instruction("syscall");                                             // bind the socket
    emitter.instruction("test rax, rax");                                       // did bind() fail?
    emitter.instruction("js __rt_stream_socket_server_fail_close_x86");         // bind() failed

    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the udp transport flag
    emitter.instruction("test rax, rax");                                       // is this a udp socket?
    emitter.instruction("jnz __rt_stream_socket_server_ok_x86");                // udp sockets skip listen()

    emitter.instruction("call __rt_socket_backlog");                            // resolve the configured backlog (default 128)
    emitter.instruction("mov esi, eax");                                        // backlog → listen() arg 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // socket descriptor (reload after the call clobbers rax)
    emitter.instruction("mov eax, 50");                                         // Linux x86_64 syscall 50 = listen
    emitter.instruction("syscall");                                             // mark the socket as listening
    emitter.instruction("test rax, rax");                                       // did listen() fail?
    emitter.instruction("js __rt_stream_socket_server_fail_close_x86");         // listen() failed

    emitter.label("__rt_stream_socket_server_ok_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the listening descriptor
    emitter.instruction("add rsp, 64");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the listening socket

    emitter.label("__rt_stream_socket_server_fail_close_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the socket descriptor
    emitter.instruction("mov eax, 3");                                          // Linux x86_64 syscall 3 = close
    emitter.instruction("syscall");                                             // close the failed socket

    emitter.label("__rt_stream_socket_server_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 reports a failed server socket
    emitter.instruction("add rsp, 64");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
