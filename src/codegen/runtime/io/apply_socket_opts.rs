//! Purpose:
//! Emits `__rt_apply_socket_client_opts` and `__rt_apply_socket_server_opts`,
//! which apply socket-wrapper context options (`tcp_nodelay`, `so_reuseport`,
//! `ipv6_v6only`) to a freshly-created socket descriptor.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::io`.
//! - `__rt_stream_socket_client` (after connect succeeds) and
//!   `__rt_stream_socket_server` (after bind/listen succeeds), plus the
//!   IPv6 and Unix-domain variants.
//!
//! Key details:
//! - All option applications are best-effort: a setsockopt failure is
//!   swallowed silently so the caller still returns the descriptor.
//! - The lookup uses `__rt_get_int_context_option` so a missing option key
//!   is a no-op (the helper returns 0 without touching the out slot).
//! - The shared frame layout: a single 8-byte scratch slot at [sp,#0] holds
//!   the int-option result and (re-used) the 4-byte option value passed to
//!   setsockopt.

use crate::codegen::{abi, emit::Emitter, platform::{Arch, Platform}};

/// `__rt_apply_socket_bindto(fd)` — best-effort pre-connect bind to the
/// address specified by `_stream_context_options['socket']['bindto']`. The
/// option value is parsed through `__rt_inet_addr_parse`, which accepts
/// `host:port`, `host` (port 0), or `[scheme://]host:port`. Failures are
/// swallowed silently so the connect path still runs.
/// Input:  AArch64 x0 = fd
///         x86_64  rdi = fd
pub fn emit_apply_socket_bindto(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_apply_socket_bindto_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: apply_socket_bindto ---");
    emitter.label_global("__rt_apply_socket_bindto");

    // Frame (80 bytes):
    //   [sp,  0] saved fd
    //   [sp,  8] bindto string pointer (from context lookup)
    //   [sp, 16] bindto string length
    //   [sp, 24..40) sockaddr_in scratch (16 bytes)
    //   [sp, 40..64) padding
    //   [sp, 64] saved x29
    //   [sp, 72] saved x30
    emitter.instruction("sub sp, sp, #80");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // advance runtime pointer or counter
    emitter.instruction("str x0, [sp, #0]");                                    // save fd

    // bindto lookup. Zero ptr/len so a miss leaves them empty.
    emitter.instruction("str xzr, [sp, #8]");                                   // store runtime value
    emitter.instruction("str xzr, [sp, #16]");                                  // store runtime value
    abi::emit_symbol_address(emitter, "x0", "_socket_key_str");
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "x2", "_socket_bindto_key_str");
    emitter.instruction("mov x3, #6");                                          // strlen("bindto")
    emitter.instruction("add x4, sp, #8");                                      // out_ptr_addr
    emitter.instruction("add x5, sp, #16");                                     // out_len_addr
    emitter.instruction("bl __rt_get_string_context_option");                   // call runtime helper
    emitter.instruction("cbz x0, __rt_asbt_done");                              // miss → nothing to bind

    // Parse the bindto value into (packed_addr, port).
    emitter.instruction("ldr x0, [sp, #8]");                                    // bindto ptr
    emitter.instruction("ldr x1, [sp, #16]");                                   // bindto len
    emitter.instruction("bl __rt_inet_addr_parse");                             // x0 = packed addr or -1, x1 = port
    emitter.instruction("cmp x0, #0");                                          // compare runtime values for the next branch
    emitter.instruction("b.lt __rt_asbt_done");                                 // parse failed → silently skip

    // Build the 16-byte sockaddr_in at [sp, #24].
    if matches!(plat, Platform::MacOS) {
        emitter.instruction("mov w9, #16");                                     // macOS sin_len
        emitter.instruction("strb w9, [sp, #24]");                              // store runtime value
        emitter.instruction("mov w9, #2");                                      // AF_INET
        emitter.instruction("strb w9, [sp, #25]");                              // store runtime value
    } else {
        emitter.instruction("mov w9, #2");                                      // Linux sin_family low byte
        emitter.instruction("strb w9, [sp, #24]");                              // store runtime value
        emitter.instruction("strb wzr, [sp, #25]");                             // sin_family high byte
    }
    emitter.instruction("lsr x10, x1, #8");                                     // port high byte
    emitter.instruction("strb w10, [sp, #26]");                                 // sin_port network-order
    emitter.instruction("strb w1, [sp, #27]");                                  // port low byte
    emitter.instruction("lsr x10, x0, #24");                                    // shift runtime value
    emitter.instruction("strb w10, [sp, #28]");                                 // octet 0
    emitter.instruction("lsr x10, x0, #16");                                    // shift runtime value
    emitter.instruction("strb w10, [sp, #29]");                                 // store runtime value
    emitter.instruction("lsr x10, x0, #8");                                     // shift runtime value
    emitter.instruction("strb w10, [sp, #30]");                                 // store runtime value
    emitter.instruction("strb w0, [sp, #31]");                                  // octet 3
    emitter.instruction("str xzr, [sp, #32]");                                  // zero the sockaddr_in tail

    // bind(fd, &sockaddr, 16). Best-effort: failures swallowed.
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction("add x1, sp, #24");                                     // &sockaddr_in
    emitter.instruction("mov x2, #16");                                         // prepare AArch64 call argument
    emitter.syscall(104);                                                       // bind — ignore failures

    emitter.label("__rt_asbt_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for apply socket bindto.
fn emit_apply_socket_bindto_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: apply_socket_bindto ---");
    emitter.label_global("__rt_apply_socket_bindto");

    // rbp-relative frame:
    //   [rbp -  8] saved fd
    //   [rbp - 16] bindto ptr
    //   [rbp - 24] bindto len
    //   [rbp - 40..-24] sockaddr_in scratch (16 bytes, [rbp-40..-24])
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save fd
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // store runtime value

    // bindto lookup.
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rdx", "_socket_bindto_key_str");         // load runtime data address
    emitter.instruction("mov rcx, 6");                                          // prepare SysV call argument
    emitter.instruction("lea r8, [rbp - 16]");                                  // load runtime data address
    emitter.instruction("lea r9, [rbp - 24]");                                  // load runtime data address
    emitter.instruction("call __rt_get_string_context_option");                 // call runtime helper
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_asbt_done_x");                                 // branch when the checked value is zero or equal

    // Parse bindto into packed addr + port.
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // bindto ptr (per inet_addr_parse SysV ABI)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // bindto len
    emitter.instruction("call __rt_inet_addr_parse");                           // rax = packed, rdx = port (elephc-internal pair)
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("js __rt_asbt_done_x");                                 // negative packed = parse failed

    // Build sockaddr_in at [rbp - 40].
    emitter.instruction("mov WORD PTR [rbp - 40], 2");                          // Linux sin_family = AF_INET
    emitter.instruction("mov rcx, rdx");                                        // copy port for byte split
    emitter.instruction("shr rcx, 8");                                          // shift runtime value
    emitter.instruction("mov BYTE PTR [rbp - 38], cl");                         // sin_port hi
    emitter.instruction("mov BYTE PTR [rbp - 37], dl");                         // sin_port lo
    emitter.instruction("mov rcx, rax");                                        // prepare SysV call argument
    emitter.instruction("shr rcx, 24");                                         // shift runtime value
    emitter.instruction("mov BYTE PTR [rbp - 36], cl");                         // octet 0
    emitter.instruction("mov rcx, rax");                                        // prepare SysV call argument
    emitter.instruction("shr rcx, 16");                                         // shift runtime value
    emitter.instruction("mov BYTE PTR [rbp - 35], cl");                         // octet 1
    emitter.instruction("mov rcx, rax");                                        // prepare SysV call argument
    emitter.instruction("shr rcx, 8");                                          // shift runtime value
    emitter.instruction("mov BYTE PTR [rbp - 34], cl");                         // octet 2
    emitter.instruction("mov BYTE PTR [rbp - 33], al");                         // octet 3
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // zero sockaddr_in tail

    // bind(fd, &sockaddr, 16).
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction("lea rsi, [rbp - 40]");                                 // &sockaddr_in
    emitter.instruction("mov edx, 16");                                         // prepare SysV call argument
    emitter.instruction("mov eax, 49");                                         // Linux x86_64 syscall 49 = bind
    emitter.instruction("syscall");                                             // best-effort

    emitter.label("__rt_asbt_done_x");
    emitter.instruction("add rsp, 48");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_apply_socket_client_opts(fd)` — best-effort apply of post-connect
/// socket options. Currently honors `socket.tcp_nodelay`.
/// Input:  AArch64 x0 = fd
///         x86_64  rdi = fd
/// Output: none (fd is preserved through caller-saved spill).
pub fn emit_apply_socket_client_opts(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_apply_socket_client_opts_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: apply_socket_client_opts ---");
    emitter.label_global("__rt_apply_socket_client_opts");

    // Frame:
    //   [sp,  0] saved fd
    //   [sp,  8] int-option scratch (out + setsockopt arg)
    //   [sp, 16] saved x29
    //   [sp, 24] saved x30
    emitter.instruction("sub sp, sp, #32");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // advance runtime pointer or counter
    emitter.instruction("str x0, [sp, #0]");                                    // save fd

    // tcp_nodelay lookup. Zero scratch first so a miss keeps falsy default.
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    abi::emit_symbol_address(emitter, "x0", "_socket_key_str");
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "x2", "_socket_tcp_nodelay_key_str");
    emitter.instruction("mov x3, #11");                                         // strlen("tcp_nodelay")
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // call runtime helper
    emitter.instruction("ldr x9, [sp, #8]");                                    // load runtime value
    emitter.instruction("cbz x9, __rt_asco_bcast");                             // not truthy → still check so_broadcast

    // setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &1, 4)
    emitter.instruction("mov w9, #1");                                          // option value = 1
    emitter.instruction("str w9, [sp, #8]");                                    // reuse scratch as option-value buffer
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.ipproto_tcp()));           // IPPROTO_TCP level
    emitter.instruction(&format!("mov x2, #{}", plat.tcp_nodelay()));           // TCP_NODELAY option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // sizeof(int)
    emitter.syscall(105);                                                       // setsockopt — best-effort, ignore failures

    // so_broadcast lookup. Enables sendto() to broadcast addresses on UDP.
    emitter.label("__rt_asco_bcast");
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    abi::emit_symbol_address(emitter, "x0", "_socket_key_str");
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "x2", "_socket_so_broadcast_key_str");
    emitter.instruction("mov x3, #12");                                         // strlen("so_broadcast")
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // call runtime helper
    emitter.instruction("ldr x9, [sp, #8]");                                    // load runtime value
    emitter.instruction("cbz x9, __rt_asco_done");                              // not truthy → skip setsockopt

    // setsockopt(fd, SOL_SOCKET, SO_BROADCAST, &1, 4)
    emitter.instruction("mov w9, #1");                                          // option value = 1
    emitter.instruction("str w9, [sp, #8]");                                    // reuse scratch as option-value buffer
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));            // SOL_SOCKET level
    emitter.instruction(&format!("mov x2, #{}", plat.so_broadcast()));          // SO_BROADCAST option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // sizeof(int)
    emitter.syscall(105);                                                       // setsockopt — best-effort, ignore failures

    emitter.label("__rt_asco_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release frame
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_apply_socket_server_opts(fd)` — best-effort apply of pre-bind socket
/// options. Currently honors `socket.so_reuseport`.
/// Input:  AArch64 x0 = fd
///         x86_64  rdi = fd
pub fn emit_apply_socket_server_opts(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_apply_socket_server_opts_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: apply_socket_server_opts ---");
    emitter.label_global("__rt_apply_socket_server_opts");

    emitter.instruction("sub sp, sp, #32");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // advance runtime pointer or counter
    emitter.instruction("str x0, [sp, #0]");                                    // save fd

    // so_reuseport lookup.
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    abi::emit_symbol_address(emitter, "x0", "_socket_key_str");
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "x2", "_socket_so_reuseport_key_str");
    emitter.instruction("mov x3, #12");                                         // strlen("so_reuseport")
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // call runtime helper
    emitter.instruction("ldr x9, [sp, #8]");                                    // load runtime value
    emitter.instruction("cbz x9, __rt_asso_done");                              // not truthy → skip setsockopt

    // setsockopt(fd, SOL_SOCKET, SO_REUSEPORT, &1, 4)
    emitter.instruction("mov w9, #1");                                          // option value = 1
    emitter.instruction("str w9, [sp, #8]");                                    // reuse scratch as option-value buffer
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));            // SOL_SOCKET level
    emitter.instruction(&format!("mov x2, #{}", plat.so_reuseport()));          // SO_REUSEPORT option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // sizeof(int)
    emitter.syscall(105);                                                       // setsockopt — best-effort, ignore failures

    emitter.label("__rt_asso_done");
    // ipv6_v6only lookup. setsockopt on a v4 socket fails silently here; the
    // emitter is reused for both v4 and v6 server paths so we just probe and
    // let an unsupported family bail out via setsockopt's normal error
    // semantics.
    emitter.instruction("str xzr, [sp, #8]");                                   // reset out_int slot
    abi::emit_symbol_address(emitter, "x0", "_socket_key_str");
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "x2", "_socket_ipv6_v6only_key_str");
    emitter.instruction("mov x3, #11");                                         // strlen("ipv6_v6only")
    emitter.instruction("add x4, sp, #8");                                      // advance runtime pointer or counter
    emitter.instruction("bl __rt_get_int_context_option");                      // call runtime helper
    emitter.instruction("ldr x9, [sp, #8]");                                    // load runtime value
    emitter.instruction("cbz x9, __rt_asso_v6_done");                           // branch when the checked value is zero or equal
    emitter.instruction("mov w9, #1");                                          // option value = 1
    emitter.instruction("str w9, [sp, #8]");                                    // store runtime value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.ipproto_ipv6()));          // IPPROTO_IPV6 level
    emitter.instruction(&format!("mov x2, #{}", plat.ipv6_v6only()));           // IPV6_V6ONLY option name
    emitter.instruction("add x3, sp, #8");                                      // advance runtime pointer or counter
    emitter.instruction("mov x4, #4");                                          // prepare AArch64 call argument
    emitter.syscall(105);                                                       // best-effort
    emitter.label("__rt_asso_v6_done");

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for apply socket client opts.
fn emit_apply_socket_client_opts_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: apply_socket_client_opts ---");
    emitter.label_global("__rt_apply_socket_client_opts");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 16");                                         // [rbp-8]=fd, [rbp-16]=scratch
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save fd
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default

    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_tcp_nodelay_key_str");    // load runtime data address
    emitter.instruction("mov rcx, 11");                                         // strlen("tcp_nodelay")
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // call runtime helper
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // prepare runtime result value
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_asco_bcast_x");                                // not truthy → still check so_broadcast

    emitter.instruction("mov DWORD PTR [rbp - 16], 1");                         // option value = 1 (lower 4 bytes)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction("mov esi, 6");                                          // IPPROTO_TCP
    emitter.instruction("mov edx, 1");                                          // TCP_NODELAY
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // sizeof(int)
    emitter.instruction("mov eax, 54");                                         // Linux x86_64 syscall 54 = setsockopt
    emitter.instruction("syscall");                                             // best-effort, ignore failures

    // so_broadcast lookup. Enables sendto() to broadcast addresses on UDP.
    emitter.label("__rt_asco_bcast_x");
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_so_broadcast_key_str");   // load runtime data address
    emitter.instruction("mov rcx, 12");                                         // strlen("so_broadcast")
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // call runtime helper
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // prepare runtime result value
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_asco_done_x");                                 // not truthy → skip setsockopt

    emitter.instruction("mov DWORD PTR [rbp - 16], 1");                         // option value = 1 (lower 4 bytes)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction("mov esi, 1");                                          // SOL_SOCKET
    emitter.instruction("mov edx, 6");                                          // SO_BROADCAST (Linux)
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // sizeof(int)
    emitter.instruction("mov eax, 54");                                         // Linux x86_64 syscall 54 = setsockopt
    emitter.instruction("syscall");                                             // best-effort, ignore failures

    emitter.label("__rt_asco_done_x");
    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for apply socket server opts.
fn emit_apply_socket_server_opts_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: apply_socket_server_opts ---");
    emitter.label_global("__rt_apply_socket_server_opts");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save fd
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default

    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_so_reuseport_key_str");   // load runtime data address
    emitter.instruction("mov rcx, 12");                                         // strlen("so_reuseport")
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // call runtime helper
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // prepare runtime result value
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_asso_done_x");                                 // not truthy → skip setsockopt

    emitter.instruction("mov DWORD PTR [rbp - 16], 1");                         // option value = 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction("mov esi, 1");                                          // SOL_SOCKET
    emitter.instruction("mov edx, 15");                                         // SO_REUSEPORT (Linux)
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // sizeof(int)
    emitter.instruction("mov eax, 54");                                         // Linux x86_64 syscall 54 = setsockopt
    emitter.instruction("syscall");                                             // best-effort, ignore failures

    emitter.label("__rt_asso_done_x");
    // ipv6_v6only lookup. setsockopt on a v4 socket fails silently; we just
    // probe regardless and let the kernel reject if the family is wrong.
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // reset out_int slot
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_ipv6_v6only_key_str");    // load runtime data address
    emitter.instruction("mov rcx, 11");                                         // strlen("ipv6_v6only")
    emitter.instruction("lea r8, [rbp - 16]");                                  // load runtime data address
    emitter.instruction("call __rt_get_int_context_option");                    // call runtime helper
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // prepare runtime result value
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_asso_v6_done_x");                              // branch when the checked value is zero or equal
    emitter.instruction("mov DWORD PTR [rbp - 16], 1");                         // option value = 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction("mov esi, 41");                                         // IPPROTO_IPV6
    emitter.instruction("mov edx, 26");                                         // IPV6_V6ONLY (Linux)
    emitter.instruction("lea r10, [rbp - 16]");                                 // load runtime data address
    emitter.instruction("mov r8d, 4");                                          // prepare SysV call argument
    emitter.instruction("mov eax, 54");                                         // setsockopt
    emitter.instruction("syscall");                                             // best-effort
    emitter.label("__rt_asso_v6_done_x");
    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
