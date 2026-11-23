//! Purpose:
//! Emits `__rt_apply_socket_client_opts` and `__rt_apply_socket_server_opts`,
//! which apply socket-wrapper context options (`tcp_nodelay`, `so_reuseport`,
//! `ipv6_v6only`) to a freshly-created socket descriptor.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
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

use crate::codegen_support::{abi, emit::Emitter, platform::{Arch, Platform}};

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
/// Output: x0 = 0 when the options applied, -1 when an invalid buffer size failed creation.
pub fn emit_apply_socket_client_opts(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_apply_socket_client_opts_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let php86 = crate::codegen_support::compile_php_version()
        >= crate::php_version::PhpVersion::Php86;
    emitter.blank();
    emitter.comment("--- runtime: apply_socket_client_opts ---");
    emitter.label_global("__rt_apply_socket_client_opts");

    // Frame (48 bytes):
    //   [sp,  0] saved fd
    //   [sp,  8] int-option value / out slot
    //   [sp, 16..24) struct linger
    //   [sp, 24] status (0 = ok, -1 = invalid buffer size)
    //   [sp, 32] saved x29
    //   [sp, 40] saved x30
    emitter.instruction("sub sp, sp, #48");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // advance runtime pointer or counter
    emitter.instruction("str x0, [sp, #0]");                                    // save fd
    emitter.instruction("str xzr, [sp, #24]");                                  // assume a successful option pass

    // -- nodelay: apply when present and truthy --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_tcp_nodelay_key_str@PAGE");           // load runtime data address
    emitter.instruction("add x2, x2, _socket_tcp_nodelay_key_str@PAGEOFF");     // complete the option key address
    emitter.instruction("mov x3, #11");                                         // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asco_nodelay_skip");                      // missing option → leave the default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured value
    emitter.instruction("cbz x9, __rt_asco_nodelay_skip");                      // falsy value → skip the setsockopt
    emitter.instruction("mov w9, #1");                                          // option value = 1
    emitter.instruction("str w9, [sp, #8]");                                    // publish the boolean option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.tcp_nodelay()));                         // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.label("__rt_asco_nodelay_skip");

    // -- bcast: apply when present and truthy --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_so_broadcast_key_str@PAGE");          // load runtime data address
    emitter.instruction("add x2, x2, _socket_so_broadcast_key_str@PAGEOFF");    // complete the option key address
    emitter.instruction("mov x3, #12");                                         // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asco_bcast_skip");                        // missing option → leave the default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured value
    emitter.instruction("cbz x9, __rt_asco_bcast_skip");                        // falsy value → skip the setsockopt
    emitter.instruction("mov w9, #1");                                          // option value = 1
    emitter.instruction("str w9, [sp, #8]");                                    // publish the boolean option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.so_broadcast()));                        // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort, ignore failures
    emitter.label("__rt_asco_bcast_skip");
    if php86 {
    // -- keepalive: apply when present and truthy --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_so_keepalive_key_str@PAGE");          // load runtime data address
    emitter.instruction("add x2, x2, _socket_so_keepalive_key_str@PAGEOFF");    // complete the option key address
    emitter.instruction("mov x3, #12");                                         // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asco_keepalive_skip");                    // missing option → leave the default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured value
    emitter.instruction("cbz x9, __rt_asco_keepalive_skip");                    // falsy value → skip the setsockopt
    emitter.instruction("mov w9, #1");                                          // option value = 1
    emitter.instruction("str w9, [sp, #8]");                                    // publish the boolean option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.so_keepalive()));                        // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.label("__rt_asco_keepalive_skip");

    // -- so_linger: positive enables, zero/negative disables, 65535 clamp --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_so_linger_key_str@PAGE");             // load runtime data address
    emitter.instruction("add x2, x2, _socket_so_linger_key_str@PAGEOFF");       // complete the option key address
    emitter.instruction("mov x3, #9");                                          // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asco_linger_skip");                       // missing option → keep the kernel default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured linger time
    emitter.instruction("mov x10, #0");                                         // start with lingering disabled
    emitter.instruction("cmp x9, #0");                                          // is the configured value positive?
    emitter.instruction("b.le __rt_asco_linger_off");                           // zero or negative disables lingering
    emitter.instruction("mov x10, #1");                                         // a positive value enables lingering
    emitter.instruction("mov x11, #65535");                                     // the platform linger cap
    emitter.instruction("cmp x9, x11");                                         // is the linger time above the cap?
    emitter.instruction("b.le __rt_asco_linger_store");                         // a value within the cap is kept as-is
    emitter.instruction("mov x9, x11");                                         // clamp the linger time to the unsigned short cap
    emitter.label("__rt_asco_linger_store");
    emitter.instruction("str w10, [sp, #16]");                                  // publish struct linger's l_onoff
    emitter.instruction("str w9, [sp, #20]");                                   // publish struct linger's l_linger
    emitter.instruction("b __rt_asco_linger_apply");                            // apply the assembled linger value
    emitter.label("__rt_asco_linger_off");
    emitter.instruction("str wzr, [sp, #16]");                                  // disable lingering
    emitter.instruction("str wzr, [sp, #20]");                                  // a disabled linger keeps a zero timeout
    emitter.label("__rt_asco_linger_apply");
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.so_linger()));                           // setsockopt option name
    emitter.instruction("add x3, sp, #16");                                     // option value pointer
    emitter.instruction("mov x4, #8");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.label("__rt_asco_linger_skip");

    // -- keepidle: apply the configured integer when present --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_tcp_keepidle_key_str@PAGE");          // load runtime data address
    emitter.instruction("add x2, x2, _socket_tcp_keepidle_key_str@PAGEOFF");    // complete the option key address
    emitter.instruction("mov x3, #12");                                         // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asco_keepidle_skip");                     // missing option → keep the kernel default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured value
    emitter.instruction("str w9, [sp, #8]");                                    // publish the 32-bit option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.tcp_keepidle()));                        // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.label("__rt_asco_keepidle_skip");

    // -- keepintvl: apply the configured integer when present --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_tcp_keepintvl_key_str@PAGE");         // load runtime data address
    emitter.instruction("add x2, x2, _socket_tcp_keepintvl_key_str@PAGEOFF");   // complete the option key address
    emitter.instruction("mov x3, #13");                                         // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asco_keepintvl_skip");                    // missing option → keep the kernel default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured value
    emitter.instruction("str w9, [sp, #8]");                                    // publish the 32-bit option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.tcp_keepintvl()));                       // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.label("__rt_asco_keepintvl_skip");

    // -- keepcnt: apply the configured integer when present --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_tcp_keepcnt_key_str@PAGE");           // load runtime data address
    emitter.instruction("add x2, x2, _socket_tcp_keepcnt_key_str@PAGEOFF");     // complete the option key address
    emitter.instruction("mov x3, #11");                                         // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asco_keepcnt_skip");                      // missing option → keep the kernel default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured value
    emitter.instruction("str w9, [sp, #8]");                                    // publish the 32-bit option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.tcp_keepcnt()));                         // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.label("__rt_asco_keepcnt_skip");

    // -- rcvbuf: validate 1..=INT_MAX, failing stream creation otherwise --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_so_rcvbuf_key_str@PAGE");             // load runtime data address
    emitter.instruction("add x2, x2, _socket_so_rcvbuf_key_str@PAGEOFF");       // complete the option key address
    emitter.instruction("mov x3, #9");                                          // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asco_rcvbuf_skip");                       // missing option → keep the kernel default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the requested buffer size
    emitter.instruction("cmp x9, #1");                                          // the wrapper requires a positive size
    emitter.instruction("b.lt __rt_asco_rcvbuf_invalid");                       // below one byte → reject the stream
    emitter.instruction("mov x10, #2147483647");                                // the largest accepted buffer size
    emitter.instruction("cmp x9, x10");                                         // is the request still an int?
    emitter.instruction("b.gt __rt_asco_rcvbuf_invalid");                       // above INT_MAX → reject the stream
    emitter.instruction("str w9, [sp, #8]");                                    // publish the 32-bit option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.so_rcvbuf()));                           // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.instruction("b __rt_asco_rcvbuf_skip");                             // the valid buffer size has been applied
    emitter.label("__rt_asco_rcvbuf_invalid");
    emitter.instruction("adrp x1, _diag_so_rcvbuf_range@PAGE");                 // load the range warning text
    emitter.instruction("add x1, x1, _diag_so_rcvbuf_range@PAGEOFF");           // complete the warning text address
    emitter.instruction("mov x2, #67");                                         // pass the complete range-warning length
    emitter.instruction("bl __rt_diag_warning");                                // report the invalid buffer size before failing
    emitter.instruction("mov x0, #-1");                                         // mark the helper as failed
    emitter.instruction("str x0, [sp, #24]");                                   // publish the failure to the epilogue
    emitter.label("__rt_asco_rcvbuf_skip");

    // -- sndbuf: validate 1..=INT_MAX, failing stream creation otherwise --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_so_sndbuf_key_str@PAGE");             // load runtime data address
    emitter.instruction("add x2, x2, _socket_so_sndbuf_key_str@PAGEOFF");       // complete the option key address
    emitter.instruction("mov x3, #9");                                          // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asco_sndbuf_skip");                       // missing option → keep the kernel default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the requested buffer size
    emitter.instruction("cmp x9, #1");                                          // the wrapper requires a positive size
    emitter.instruction("b.lt __rt_asco_sndbuf_invalid");                       // below one byte → reject the stream
    emitter.instruction("mov x10, #2147483647");                                // the largest accepted buffer size
    emitter.instruction("cmp x9, x10");                                         // is the request still an int?
    emitter.instruction("b.gt __rt_asco_sndbuf_invalid");                       // above INT_MAX → reject the stream
    emitter.instruction("str w9, [sp, #8]");                                    // publish the 32-bit option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.so_sndbuf()));                           // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.instruction("b __rt_asco_sndbuf_skip");                             // the valid buffer size has been applied
    emitter.label("__rt_asco_sndbuf_invalid");
    emitter.instruction("adrp x1, _diag_so_sndbuf_range@PAGE");                 // load the range warning text
    emitter.instruction("add x1, x1, _diag_so_sndbuf_range@PAGEOFF");           // complete the warning text address
    emitter.instruction("mov x2, #67");                                         // pass the complete range-warning length
    emitter.instruction("bl __rt_diag_warning");                                // report the invalid buffer size before failing
    emitter.instruction("mov x0, #-1");                                         // mark the helper as failed
    emitter.instruction("str x0, [sp, #24]");                                   // publish the failure to the epilogue
    emitter.label("__rt_asco_sndbuf_skip");
    }

    // -- return the pass/fail status --
    emitter.instruction("ldr x0, [sp, #24]");                                   // report 0 or -1 to the stream constructor
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the socket constructor
}

/// `__rt_apply_socket_server_opts(fd)` — apply the pre-bind socket context
/// options, including PHP 8.6's `so_reuseaddr` default-on behaviour and the
/// keepalive/linger/buffer-size family.
/// Input:  AArch64 x0 = fd
/// Output: x0 = 0 when the options applied, -1 when an invalid buffer size failed creation.
pub fn emit_apply_socket_server_opts(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_apply_socket_server_opts_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let php86 = crate::codegen_support::compile_php_version()
        >= crate::php_version::PhpVersion::Php86;
    emitter.blank();
    emitter.comment("--- runtime: apply_socket_server_opts ---");
    emitter.label_global("__rt_apply_socket_server_opts");

    // Frame (48 bytes):
    //   [sp,  0] saved fd
    //   [sp,  8] int-option value / out slot
    //   [sp, 16..24) struct linger
    //   [sp, 24] status (0 = ok, -1 = invalid buffer size)
    //   [sp, 32] saved x29
    //   [sp, 40] saved x30
    emitter.instruction("sub sp, sp, #48");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // advance runtime pointer or counter
    emitter.instruction("str x0, [sp, #0]");                                    // save fd
    emitter.instruction("str xzr, [sp, #24]");                                  // assume a successful option pass

    if php86 {
    // -- reuseaddr: enabled by default unless the option is present and falsy --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_so_reuseaddr_key_str@PAGE");          // load runtime data address
    emitter.instruction("add x2, x2, _socket_so_reuseaddr_key_str@PAGEOFF");    // complete the option key address
    emitter.instruction("mov x3, #12");                                         // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbnz x0, __rt_asso_reuseaddr_present");                // present option → inspect its value
    emitter.instruction("b __rt_asso_reuseaddr_apply");                         // missing option → keep the default-on behaviour
    emitter.label("__rt_asso_reuseaddr_present");
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured value
    emitter.instruction("cbz x9, __rt_asso_reuseaddr_skip");                    // explicitly falsy → disable the option
    emitter.label("__rt_asso_reuseaddr_apply");
    emitter.instruction("mov w9, #1");                                          // option value = 1
    emitter.instruction("str w9, [sp, #8]");                                    // publish the boolean option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.so_reuseaddr()));                        // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.label("__rt_asso_reuseaddr_skip");
    } else {
    // -- reuseaddr: PHP enables address reuse by default --
    emitter.instruction("mov w9, #1");                                          // option value = 1
    emitter.instruction("str w9, [sp, #8]");                                    // publish the boolean option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.so_reuseaddr()));                        // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    }

    // -- reuseport: apply when present and truthy --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_so_reuseport_key_str@PAGE");          // load runtime data address
    emitter.instruction("add x2, x2, _socket_so_reuseport_key_str@PAGEOFF");    // complete the option key address
    emitter.instruction("mov x3, #12");                                         // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asso_reuseport_skip");                    // missing option → leave the default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured value
    emitter.instruction("cbz x9, __rt_asso_reuseport_skip");                    // falsy value → skip the setsockopt
    emitter.instruction("mov w9, #1");                                          // option value = 1
    emitter.instruction("str w9, [sp, #8]");                                    // publish the boolean option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.so_reuseport()));                        // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort, ignore failures
    emitter.label("__rt_asso_reuseport_skip");

    // -- v6only: apply when present and truthy --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_ipv6_v6only_key_str@PAGE");           // load runtime data address
    emitter.instruction("add x2, x2, _socket_ipv6_v6only_key_str@PAGEOFF");     // complete the option key address
    emitter.instruction("mov x3, #11");                                         // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asso_v6only_skip");                       // missing option → leave the default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured value
    emitter.instruction("cbz x9, __rt_asso_v6only_skip");                       // falsy value → skip the setsockopt
    emitter.instruction("mov w9, #1");                                          // option value = 1
    emitter.instruction("str w9, [sp, #8]");                                    // publish the boolean option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.ipproto_ipv6()));                        // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.ipv6_v6only()));                         // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.label("__rt_asso_v6only_skip");
    if php86 {
    // -- keepalive: apply when present and truthy --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_so_keepalive_key_str@PAGE");          // load runtime data address
    emitter.instruction("add x2, x2, _socket_so_keepalive_key_str@PAGEOFF");    // complete the option key address
    emitter.instruction("mov x3, #12");                                         // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asso_keepalive_skip");                    // missing option → leave the default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured value
    emitter.instruction("cbz x9, __rt_asso_keepalive_skip");                    // falsy value → skip the setsockopt
    emitter.instruction("mov w9, #1");                                          // option value = 1
    emitter.instruction("str w9, [sp, #8]");                                    // publish the boolean option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.so_keepalive()));                        // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.label("__rt_asso_keepalive_skip");

    // -- so_linger: positive enables, zero/negative disables, 65535 clamp --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_so_linger_key_str@PAGE");             // load runtime data address
    emitter.instruction("add x2, x2, _socket_so_linger_key_str@PAGEOFF");       // complete the option key address
    emitter.instruction("mov x3, #9");                                          // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asso_linger_skip");                       // missing option → keep the kernel default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured linger time
    emitter.instruction("mov x10, #0");                                         // start with lingering disabled
    emitter.instruction("cmp x9, #0");                                          // is the configured value positive?
    emitter.instruction("b.le __rt_asso_linger_off");                           // zero or negative disables lingering
    emitter.instruction("mov x10, #1");                                         // a positive value enables lingering
    emitter.instruction("mov x11, #65535");                                     // the platform linger cap
    emitter.instruction("cmp x9, x11");                                         // is the linger time above the cap?
    emitter.instruction("b.le __rt_asso_linger_store");                         // a value within the cap is kept as-is
    emitter.instruction("mov x9, x11");                                         // clamp the linger time to the unsigned short cap
    emitter.label("__rt_asso_linger_store");
    emitter.instruction("str w10, [sp, #16]");                                  // publish struct linger's l_onoff
    emitter.instruction("str w9, [sp, #20]");                                   // publish struct linger's l_linger
    emitter.instruction("b __rt_asso_linger_apply");                            // apply the assembled linger value
    emitter.label("__rt_asso_linger_off");
    emitter.instruction("str wzr, [sp, #16]");                                  // disable lingering
    emitter.instruction("str wzr, [sp, #20]");                                  // a disabled linger keeps a zero timeout
    emitter.label("__rt_asso_linger_apply");
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.so_linger()));                           // setsockopt option name
    emitter.instruction("add x3, sp, #16");                                     // option value pointer
    emitter.instruction("mov x4, #8");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.label("__rt_asso_linger_skip");

    // -- keepidle: apply the configured integer when present --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_tcp_keepidle_key_str@PAGE");          // load runtime data address
    emitter.instruction("add x2, x2, _socket_tcp_keepidle_key_str@PAGEOFF");    // complete the option key address
    emitter.instruction("mov x3, #12");                                         // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asso_keepidle_skip");                     // missing option → keep the kernel default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured value
    emitter.instruction("str w9, [sp, #8]");                                    // publish the 32-bit option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.tcp_keepidle()));                        // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.label("__rt_asso_keepidle_skip");

    // -- keepintvl: apply the configured integer when present --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_tcp_keepintvl_key_str@PAGE");         // load runtime data address
    emitter.instruction("add x2, x2, _socket_tcp_keepintvl_key_str@PAGEOFF");   // complete the option key address
    emitter.instruction("mov x3, #13");                                         // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asso_keepintvl_skip");                    // missing option → keep the kernel default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured value
    emitter.instruction("str w9, [sp, #8]");                                    // publish the 32-bit option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.tcp_keepintvl()));                       // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.label("__rt_asso_keepintvl_skip");

    // -- keepcnt: apply the configured integer when present --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_tcp_keepcnt_key_str@PAGE");           // load runtime data address
    emitter.instruction("add x2, x2, _socket_tcp_keepcnt_key_str@PAGEOFF");     // complete the option key address
    emitter.instruction("mov x3, #11");                                         // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asso_keepcnt_skip");                      // missing option → keep the kernel default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the configured value
    emitter.instruction("str w9, [sp, #8]");                                    // publish the 32-bit option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.tcp_keepcnt()));                         // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.label("__rt_asso_keepcnt_skip");

    // -- rcvbuf: validate 1..=INT_MAX, failing stream creation otherwise --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_so_rcvbuf_key_str@PAGE");             // load runtime data address
    emitter.instruction("add x2, x2, _socket_so_rcvbuf_key_str@PAGEOFF");       // complete the option key address
    emitter.instruction("mov x3, #9");                                          // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asso_rcvbuf_skip");                       // missing option → keep the kernel default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the requested buffer size
    emitter.instruction("cmp x9, #1");                                          // the wrapper requires a positive size
    emitter.instruction("b.lt __rt_asso_rcvbuf_invalid");                       // below one byte → reject the stream
    emitter.instruction("mov x10, #2147483647");                                // the largest accepted buffer size
    emitter.instruction("cmp x9, x10");                                         // is the request still an int?
    emitter.instruction("b.gt __rt_asso_rcvbuf_invalid");                       // above INT_MAX → reject the stream
    emitter.instruction("str w9, [sp, #8]");                                    // publish the 32-bit option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.so_rcvbuf()));                           // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.instruction("b __rt_asso_rcvbuf_skip");                             // the valid buffer size has been applied
    emitter.label("__rt_asso_rcvbuf_invalid");
    emitter.instruction("adrp x1, _diag_so_rcvbuf_range@PAGE");                 // load the range warning text
    emitter.instruction("add x1, x1, _diag_so_rcvbuf_range@PAGEOFF");           // complete the warning text address
    emitter.instruction("mov x2, #67");                                         // pass the complete range-warning length
    emitter.instruction("bl __rt_diag_warning");                                // report the invalid buffer size before failing
    emitter.instruction("mov x0, #-1");                                         // mark the helper as failed
    emitter.instruction("str x0, [sp, #24]");                                   // publish the failure to the epilogue
    emitter.label("__rt_asso_rcvbuf_skip");

    // -- sndbuf: validate 1..=INT_MAX, failing stream creation otherwise --
    emitter.instruction("str xzr, [sp, #8]");                                   // out_int default = 0
    emitter.instruction("adrp x0, _socket_key_str@PAGE");                       // load runtime data address
    emitter.instruction("add x0, x0, _socket_key_str@PAGEOFF");                 // complete the socket wrapper key address
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    emitter.instruction("adrp x2, _socket_so_sndbuf_key_str@PAGE");             // load runtime data address
    emitter.instruction("add x2, x2, _socket_so_sndbuf_key_str@PAGEOFF");       // complete the option key address
    emitter.instruction("mov x3, #9");                                          // option key length
    emitter.instruction("add x4, sp, #8");                                      // out_int_addr
    emitter.instruction("bl __rt_get_int_context_option");                      // look up the socket context option
    emitter.instruction("cbz x0, __rt_asso_sndbuf_skip");                       // missing option → keep the kernel default
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the requested buffer size
    emitter.instruction("cmp x9, #1");                                          // the wrapper requires a positive size
    emitter.instruction("b.lt __rt_asso_sndbuf_invalid");                       // below one byte → reject the stream
    emitter.instruction("mov x10, #2147483647");                                // the largest accepted buffer size
    emitter.instruction("cmp x9, x10");                                         // is the request still an int?
    emitter.instruction("b.gt __rt_asso_sndbuf_invalid");                       // above INT_MAX → reject the stream
    emitter.instruction("str w9, [sp, #8]");                                    // publish the 32-bit option value
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov x2, #{}", plat.so_sndbuf()));                           // setsockopt option name
    emitter.instruction("add x3, sp, #8");                                      // option value pointer
    emitter.instruction("mov x4, #4");                                          // option value size
    emitter.instruction("svc #105");                                            // setsockopt — best-effort
    emitter.instruction("b __rt_asso_sndbuf_skip");                             // the valid buffer size has been applied
    emitter.label("__rt_asso_sndbuf_invalid");
    emitter.instruction("adrp x1, _diag_so_sndbuf_range@PAGE");                 // load the range warning text
    emitter.instruction("add x1, x1, _diag_so_sndbuf_range@PAGEOFF");           // complete the warning text address
    emitter.instruction("mov x2, #67");                                         // pass the complete range-warning length
    emitter.instruction("bl __rt_diag_warning");                                // report the invalid buffer size before failing
    emitter.instruction("mov x0, #-1");                                         // mark the helper as failed
    emitter.instruction("str x0, [sp, #24]");                                   // publish the failure to the epilogue
    emitter.label("__rt_asso_sndbuf_skip");
    }

    // -- return the pass/fail status --
    emitter.instruction("ldr x0, [sp, #24]");                                   // report 0 or -1 to the stream constructor
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the socket constructor
}

/// Emits the Linux x86_64 stream runtime helper for apply socket client opts.
fn emit_apply_socket_client_opts_linux_x86_64(emitter: &mut Emitter) {
    let plat = emitter.platform;
    let php86 = crate::codegen_support::compile_php_version()
        >= crate::php_version::PhpVersion::Php86;
    emitter.blank();
    emitter.comment("--- runtime: apply_socket_client_opts ---");
    emitter.label_global("__rt_apply_socket_client_opts");

    // rbp-relative frame:
    //   [rbp -  8] saved fd
    //   [rbp - 16] int-option value / out slot
    //   [rbp - 24] struct linger l_onoff
    //   [rbp - 28] struct linger l_linger
    //   [rbp - 32] status (0 = ok, -1 = invalid buffer size)
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate the option frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save fd
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // assume a successful option pass

    // -- nodelay: apply when present and truthy --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_tcp_nodelay_key_str");
    emitter.instruction("mov rcx, 11");                                         // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asco_nodelay_skip_x");                         // missing option → leave the default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured value
    emitter.instruction("test rax, rax");                                       // is the configured value truthy?
    emitter.instruction("jz __rt_asco_nodelay_skip_x");                         // falsy value → skip the setsockopt
    emitter.instruction("mov DWORD PTR [rbp - 16], 1");                         // option value = 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.tcp_nodelay()));                         // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asco_nodelay_skip_x");

    // -- bcast: apply when present and truthy --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_so_broadcast_key_str");
    emitter.instruction("mov rcx, 12");                                         // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asco_bcast_skip_x");                           // missing option → leave the default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured value
    emitter.instruction("test rax, rax");                                       // is the configured value truthy?
    emitter.instruction("jz __rt_asco_bcast_skip_x");                           // falsy value → skip the setsockopt
    emitter.instruction("mov DWORD PTR [rbp - 16], 1");                         // option value = 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.so_broadcast()));                        // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asco_bcast_skip_x");
    if php86 {
    // -- keepalive: apply when present and truthy --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_so_keepalive_key_str");
    emitter.instruction("mov rcx, 12");                                         // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asco_keepalive_skip_x");                       // missing option → leave the default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured value
    emitter.instruction("test rax, rax");                                       // is the configured value truthy?
    emitter.instruction("jz __rt_asco_keepalive_skip_x");                       // falsy value → skip the setsockopt
    emitter.instruction("mov DWORD PTR [rbp - 16], 1");                         // option value = 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.so_keepalive()));                        // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asco_keepalive_skip_x");

    // -- so_linger: positive enables, zero/negative disables, 65535 clamp --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_so_linger_key_str");
    emitter.instruction("mov rcx, 9");                                          // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asco_linger_skip_x");                          // missing option → keep the kernel default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured linger time
    emitter.instruction("xor ecx, ecx");                                        // start with lingering disabled
    emitter.instruction("cmp rax, 0");                                          // is the configured value positive?
    emitter.instruction("jle __rt_asco_linger_off_x");                          // zero or negative disables lingering
    emitter.instruction("mov ecx, 1");                                          // a positive value enables lingering
    emitter.instruction("mov rdx, 65535");                                      // the platform linger cap
    emitter.instruction("cmp rax, rdx");                                        // is the linger time above the cap?
    emitter.instruction("jle __rt_asco_linger_store_x");                        // a value within the cap is kept as-is
    emitter.instruction("mov rax, rdx");                                        // clamp the linger time to the unsigned short cap
    emitter.label("__rt_asco_linger_store_x");
    emitter.instruction("mov DWORD PTR [rbp - 24], ecx");                       // publish struct linger's l_onoff
    emitter.instruction("mov DWORD PTR [rbp - 28], eax");                       // publish struct linger's l_linger
    emitter.instruction("jmp __rt_asco_linger_apply_x");                        // apply the assembled linger value
    emitter.label("__rt_asco_linger_off_x");
    emitter.instruction("mov DWORD PTR [rbp - 24], 0");                         // disable lingering
    emitter.instruction("mov DWORD PTR [rbp - 28], 0");                         // a disabled linger keeps a zero timeout
    emitter.label("__rt_asco_linger_apply_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.so_linger()));                           // setsockopt option name
    emitter.instruction("lea r10, [rbp - 24]");                                 // option value pointer
    emitter.instruction("mov r8d, 8");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asco_linger_skip_x");

    // -- keepidle: apply the configured integer when present --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_tcp_keepidle_key_str");
    emitter.instruction("mov rcx, 12");                                         // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asco_keepidle_skip_x");                        // missing option → keep the kernel default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured value
    emitter.instruction("mov DWORD PTR [rbp - 16], eax");                       // publish the 32-bit option value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.tcp_keepidle()));                        // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asco_keepidle_skip_x");

    // -- keepintvl: apply the configured integer when present --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_tcp_keepintvl_key_str");
    emitter.instruction("mov rcx, 13");                                         // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asco_keepintvl_skip_x");                       // missing option → keep the kernel default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured value
    emitter.instruction("mov DWORD PTR [rbp - 16], eax");                       // publish the 32-bit option value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.tcp_keepintvl()));                       // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asco_keepintvl_skip_x");

    // -- keepcnt: apply the configured integer when present --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_tcp_keepcnt_key_str");
    emitter.instruction("mov rcx, 11");                                         // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asco_keepcnt_skip_x");                         // missing option → keep the kernel default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured value
    emitter.instruction("mov DWORD PTR [rbp - 16], eax");                       // publish the 32-bit option value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.tcp_keepcnt()));                         // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asco_keepcnt_skip_x");

    // -- rcvbuf: validate 1..=INT_MAX, failing stream creation otherwise --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_so_rcvbuf_key_str");
    emitter.instruction("mov rcx, 9");                                          // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asco_rcvbuf_skip_x");                          // missing option → keep the kernel default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the requested buffer size
    emitter.instruction("cmp rax, 1");                                          // the wrapper requires a positive size
    emitter.instruction("jl __rt_asco_rcvbuf_invalid_x");                       // below one byte → reject the stream
    emitter.instruction("mov rcx, 2147483647");                                 // the largest accepted buffer size
    emitter.instruction("cmp rax, rcx");                                        // is the request still an int?
    emitter.instruction("jg __rt_asco_rcvbuf_invalid_x");                       // above INT_MAX → reject the stream
    emitter.instruction("mov DWORD PTR [rbp - 16], eax");                       // publish the 32-bit option value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.so_rcvbuf()));                           // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.instruction("jmp __rt_asco_rcvbuf_skip_x");                         // the valid buffer size has been applied
    emitter.label("__rt_asco_rcvbuf_invalid_x");
    abi::emit_symbol_address(emitter, "rdi", "_diag_so_rcvbuf_range");
    emitter.instruction("mov esi, 67");                                         // pass the complete range-warning length
    emitter.instruction("call __rt_diag_warning");                              // report the invalid buffer size before failing
    emitter.instruction("mov QWORD PTR [rbp - 32], -1");                        // mark the helper as failed
    emitter.label("__rt_asco_rcvbuf_skip_x");

    // -- sndbuf: validate 1..=INT_MAX, failing stream creation otherwise --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_so_sndbuf_key_str");
    emitter.instruction("mov rcx, 9");                                          // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asco_sndbuf_skip_x");                          // missing option → keep the kernel default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the requested buffer size
    emitter.instruction("cmp rax, 1");                                          // the wrapper requires a positive size
    emitter.instruction("jl __rt_asco_sndbuf_invalid_x");                       // below one byte → reject the stream
    emitter.instruction("mov rcx, 2147483647");                                 // the largest accepted buffer size
    emitter.instruction("cmp rax, rcx");                                        // is the request still an int?
    emitter.instruction("jg __rt_asco_sndbuf_invalid_x");                       // above INT_MAX → reject the stream
    emitter.instruction("mov DWORD PTR [rbp - 16], eax");                       // publish the 32-bit option value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.so_sndbuf()));                           // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.instruction("jmp __rt_asco_sndbuf_skip_x");                         // the valid buffer size has been applied
    emitter.label("__rt_asco_sndbuf_invalid_x");
    abi::emit_symbol_address(emitter, "rdi", "_diag_so_sndbuf_range");
    emitter.instruction("mov esi, 67");                                         // pass the complete range-warning length
    emitter.instruction("call __rt_diag_warning");                              // report the invalid buffer size before failing
    emitter.instruction("mov QWORD PTR [rbp - 32], -1");                        // mark the helper as failed
    emitter.label("__rt_asco_sndbuf_skip_x");
    }

    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // report 0 or -1 to the stream constructor
    emitter.instruction("add rsp, 48");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the socket constructor
}

/// Emits the Linux x86_64 stream runtime helper for apply socket server opts.
fn emit_apply_socket_server_opts_linux_x86_64(emitter: &mut Emitter) {
    let plat = emitter.platform;
    let php86 = crate::codegen_support::compile_php_version()
        >= crate::php_version::PhpVersion::Php86;
    emitter.blank();
    emitter.comment("--- runtime: apply_socket_server_opts ---");
    emitter.label_global("__rt_apply_socket_server_opts");

    // rbp-relative frame:
    //   [rbp -  8] saved fd
    //   [rbp - 16] int-option value / out slot
    //   [rbp - 24] struct linger l_onoff
    //   [rbp - 28] struct linger l_linger
    //   [rbp - 32] status (0 = ok, -1 = invalid buffer size)
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate the option frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save fd
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // assume a successful option pass

    if php86 {
    // -- reuseaddr: enabled by default unless the option is present and falsy --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_so_reuseaddr_key_str");
    emitter.instruction("mov rcx, 12");                                         // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jnz __rt_asso_reuseaddr_present_x");                   // present option → inspect its value
    emitter.instruction("jmp __rt_asso_reuseaddr_apply_x");                     // missing option → keep the default-on behaviour
    emitter.label("__rt_asso_reuseaddr_present_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured value
    emitter.instruction("test rax, rax");                                       // is the configured value truthy?
    emitter.instruction("jz __rt_asso_reuseaddr_skip_x");                       // explicitly falsy → disable the option
    emitter.label("__rt_asso_reuseaddr_apply_x");
    emitter.instruction("mov DWORD PTR [rbp - 16], 1");                         // option value = 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.so_reuseaddr()));                        // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asso_reuseaddr_skip_x");
    } else {
    // -- reuseaddr: PHP enables address reuse by default --
    emitter.instruction("mov DWORD PTR [rbp - 16], 1");                         // option value = 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.so_reuseaddr()));                        // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    }

    // -- reuseport: apply when present and truthy --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_so_reuseport_key_str");
    emitter.instruction("mov rcx, 12");                                         // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asso_reuseport_skip_x");                       // missing option → leave the default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured value
    emitter.instruction("test rax, rax");                                       // is the configured value truthy?
    emitter.instruction("jz __rt_asso_reuseport_skip_x");                       // falsy value → skip the setsockopt
    emitter.instruction("mov DWORD PTR [rbp - 16], 1");                         // option value = 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.so_reuseport()));                        // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asso_reuseport_skip_x");

    // -- v6only: apply when present and truthy --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_ipv6_v6only_key_str");
    emitter.instruction("mov rcx, 11");                                         // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asso_v6only_skip_x");                          // missing option → leave the default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured value
    emitter.instruction("test rax, rax");                                       // is the configured value truthy?
    emitter.instruction("jz __rt_asso_v6only_skip_x");                          // falsy value → skip the setsockopt
    emitter.instruction("mov DWORD PTR [rbp - 16], 1");                         // option value = 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.ipproto_ipv6()));                        // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.ipv6_v6only()));                         // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asso_v6only_skip_x");
    if php86 {
    // -- keepalive: apply when present and truthy --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_so_keepalive_key_str");
    emitter.instruction("mov rcx, 12");                                         // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asso_keepalive_skip_x");                       // missing option → leave the default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured value
    emitter.instruction("test rax, rax");                                       // is the configured value truthy?
    emitter.instruction("jz __rt_asso_keepalive_skip_x");                       // falsy value → skip the setsockopt
    emitter.instruction("mov DWORD PTR [rbp - 16], 1");                         // option value = 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.so_keepalive()));                        // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asso_keepalive_skip_x");

    // -- so_linger: positive enables, zero/negative disables, 65535 clamp --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_so_linger_key_str");
    emitter.instruction("mov rcx, 9");                                          // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asso_linger_skip_x");                          // missing option → keep the kernel default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured linger time
    emitter.instruction("xor ecx, ecx");                                        // start with lingering disabled
    emitter.instruction("cmp rax, 0");                                          // is the configured value positive?
    emitter.instruction("jle __rt_asso_linger_off_x");                          // zero or negative disables lingering
    emitter.instruction("mov ecx, 1");                                          // a positive value enables lingering
    emitter.instruction("mov rdx, 65535");                                      // the platform linger cap
    emitter.instruction("cmp rax, rdx");                                        // is the linger time above the cap?
    emitter.instruction("jle __rt_asso_linger_store_x");                        // a value within the cap is kept as-is
    emitter.instruction("mov rax, rdx");                                        // clamp the linger time to the unsigned short cap
    emitter.label("__rt_asso_linger_store_x");
    emitter.instruction("mov DWORD PTR [rbp - 24], ecx");                       // publish struct linger's l_onoff
    emitter.instruction("mov DWORD PTR [rbp - 28], eax");                       // publish struct linger's l_linger
    emitter.instruction("jmp __rt_asso_linger_apply_x");                        // apply the assembled linger value
    emitter.label("__rt_asso_linger_off_x");
    emitter.instruction("mov DWORD PTR [rbp - 24], 0");                         // disable lingering
    emitter.instruction("mov DWORD PTR [rbp - 28], 0");                         // a disabled linger keeps a zero timeout
    emitter.label("__rt_asso_linger_apply_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.so_linger()));                           // setsockopt option name
    emitter.instruction("lea r10, [rbp - 24]");                                 // option value pointer
    emitter.instruction("mov r8d, 8");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asso_linger_skip_x");

    // -- keepidle: apply the configured integer when present --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_tcp_keepidle_key_str");
    emitter.instruction("mov rcx, 12");                                         // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asso_keepidle_skip_x");                        // missing option → keep the kernel default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured value
    emitter.instruction("mov DWORD PTR [rbp - 16], eax");                       // publish the 32-bit option value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.tcp_keepidle()));                        // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asso_keepidle_skip_x");

    // -- keepintvl: apply the configured integer when present --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_tcp_keepintvl_key_str");
    emitter.instruction("mov rcx, 13");                                         // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asso_keepintvl_skip_x");                       // missing option → keep the kernel default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured value
    emitter.instruction("mov DWORD PTR [rbp - 16], eax");                       // publish the 32-bit option value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.tcp_keepintvl()));                       // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asso_keepintvl_skip_x");

    // -- keepcnt: apply the configured integer when present --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_tcp_keepcnt_key_str");
    emitter.instruction("mov rcx, 11");                                         // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asso_keepcnt_skip_x");                         // missing option → keep the kernel default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the configured value
    emitter.instruction("mov DWORD PTR [rbp - 16], eax");                       // publish the 32-bit option value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.ipproto_tcp()));                         // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.tcp_keepcnt()));                         // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.label("__rt_asso_keepcnt_skip_x");

    // -- rcvbuf: validate 1..=INT_MAX, failing stream creation otherwise --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_so_rcvbuf_key_str");
    emitter.instruction("mov rcx, 9");                                          // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asso_rcvbuf_skip_x");                          // missing option → keep the kernel default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the requested buffer size
    emitter.instruction("cmp rax, 1");                                          // the wrapper requires a positive size
    emitter.instruction("jl __rt_asso_rcvbuf_invalid_x");                       // below one byte → reject the stream
    emitter.instruction("mov rcx, 2147483647");                                 // the largest accepted buffer size
    emitter.instruction("cmp rax, rcx");                                        // is the request still an int?
    emitter.instruction("jg __rt_asso_rcvbuf_invalid_x");                       // above INT_MAX → reject the stream
    emitter.instruction("mov DWORD PTR [rbp - 16], eax");                       // publish the 32-bit option value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.so_rcvbuf()));                           // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.instruction("jmp __rt_asso_rcvbuf_skip_x");                         // the valid buffer size has been applied
    emitter.label("__rt_asso_rcvbuf_invalid_x");
    abi::emit_symbol_address(emitter, "rdi", "_diag_so_rcvbuf_range");
    emitter.instruction("mov esi, 67");                                         // pass the complete range-warning length
    emitter.instruction("call __rt_diag_warning");                              // report the invalid buffer size before failing
    emitter.instruction("mov QWORD PTR [rbp - 32], -1");                        // mark the helper as failed
    emitter.label("__rt_asso_rcvbuf_skip_x");

    // -- sndbuf: validate 1..=INT_MAX, failing stream creation otherwise --
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // out_int default = 0
    abi::emit_symbol_address(emitter, "rdi", "_socket_key_str");                // load runtime data address
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    abi::emit_symbol_address(emitter, "rdx", "_socket_so_sndbuf_key_str");
    emitter.instruction("mov rcx, 9");                                          // option key length
    emitter.instruction("lea r8, [rbp - 16]");                                  // out_int_addr
    emitter.instruction("call __rt_get_int_context_option");                    // look up the socket context option
    emitter.instruction("test rax, rax");                                       // did the context option exist?
    emitter.instruction("jz __rt_asso_sndbuf_skip_x");                          // missing option → keep the kernel default
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the requested buffer size
    emitter.instruction("cmp rax, 1");                                          // the wrapper requires a positive size
    emitter.instruction("jl __rt_asso_sndbuf_invalid_x");                       // below one byte → reject the stream
    emitter.instruction("mov rcx, 2147483647");                                 // the largest accepted buffer size
    emitter.instruction("cmp rax, rcx");                                        // is the request still an int?
    emitter.instruction("jg __rt_asso_sndbuf_invalid_x");                       // above INT_MAX → reject the stream
    emitter.instruction("mov DWORD PTR [rbp - 16], eax");                       // publish the 32-bit option value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("mov esi, {}", plat.sol_socket()));                          // setsockopt level
    emitter.instruction(&format!("mov edx, {}", plat.so_sndbuf()));                           // setsockopt option name
    emitter.instruction("lea r10, [rbp - 16]");                                 // option value pointer
    emitter.instruction("mov r8d, 4");                                          // option value size
    emitter.instruction("mov eax, 54");                                         // setsockopt syscall
    emitter.instruction("syscall");                                             // setsockopt — best-effort
    emitter.instruction("jmp __rt_asso_sndbuf_skip_x");                         // the valid buffer size has been applied
    emitter.label("__rt_asso_sndbuf_invalid_x");
    abi::emit_symbol_address(emitter, "rdi", "_diag_so_sndbuf_range");
    emitter.instruction("mov esi, 67");                                         // pass the complete range-warning length
    emitter.instruction("call __rt_diag_warning");                              // report the invalid buffer size before failing
    emitter.instruction("mov QWORD PTR [rbp - 32], -1");                        // mark the helper as failed
    emitter.label("__rt_asso_sndbuf_skip_x");
    }

    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // report 0 or -1 to the stream constructor
    emitter.instruction("add rsp, 48");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the socket constructor
}
