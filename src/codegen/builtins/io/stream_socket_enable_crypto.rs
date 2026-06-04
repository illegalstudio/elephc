//! Purpose:
//! Emits PHP `stream_socket_enable_crypto` calls.
//!
//! When `$enable` is true, the helper invokes `elephc_tls_attach_fd` via
//! the runtime function-pointer slot, stores the returned handle in
//! `_tls_sessions[fd]`, and reports success. Subsequent fread/fwrite
//! consult that table and route through `elephc_tls_read_fn` /
//! `elephc_tls_write_fn` instead of the raw read/write syscalls.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - SNI / cert-name is taken from the active stream context's
//!   `['ssl']['peer_name']` (via `__rt_get_ssl_peer_name`). When no context
//!   peer-name is set, the SNI defaults to the transport host that
//!   `stream_socket_client` recorded for this fd in `_stream_connect_host[fd]`
//!   (matching PHP, which defaults the peer name to the connection host); the
//!   `localhost` constant (`_tls_peer_name_default`) is used only when neither a
//!   context peer-name nor a recorded connection host is available. With a
//!   peer-name set, real TLS to named hosts works end to end (verified against a
//!   public HTTPS host — see `test_stream_socket_enable_crypto_real_tls_handshake`).
//! - The 3rd ($crypto_method) and 4th ($session_stream) PHP args are
//!   evaluated for side effects but otherwise ignored — elephc relies on
//!   rustls's default TLS protocol negotiation.
//! - $enable=false (mid-stream TLS shutdown) reloads the fd and calls the
//!   shared `fclose::emit_tls_session_teardown`, which sends `close_notify`
//!   via `_elephc_tls_close_fn` and clears `_tls_sessions[fd]` (a no-op when
//!   no session is attached), leaving the fd as a plain TCP socket; it then
//!   reports `true`.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::https_stream::publish_tls_function_pointers;
use super::stream_arg::emit_stream_fd_arg;

/// Emits codegen for PHP `stream_socket_enable_crypto()` stream and I/O builtin calls.
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_socket_enable_crypto()");
    // -- evaluate the stream arg → fd in x0/rax, save on the stack --
    emit_stream_fd_arg(name, &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // [sp+0] = fd, preserved across the remaining arg evaluations and the attach call
    // -- evaluate $enable; branch on its value --
    let enable_label = ctx.next_label("ssec_enable");
    let done_label = ctx.next_label("ssec_done");
    emit_expr(&args[1], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve $enable while ignored optional args are evaluated
    for arg in &args[2..] {
        emit_expr(arg, emitter, ctx, data);                                     // side effects only
    }
    match emitter.target.arch {
        Arch::AArch64 => abi::emit_pop_reg(emitter, "x0"),
        Arch::X86_64 => abi::emit_pop_reg(emitter, "rax"),
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbnz x0, {}", enable_label));         // enable=true enters the TLS attach path
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // did the caller request TLS enablement?
            emitter.instruction(&format!("jnz {}", enable_label));              // enable=true enters the TLS attach path
        }
    }
    // -- disable path: unwind any live TLS session on the fd, then report
    //    success. The fd is reloaded from the stashed slot; the teardown sends
    //    close_notify and clears _tls_sessions[fd], and is a no-op when no TLS
    //    session is attached. After this the fd is a plain TCP socket again. --
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("ldr x0, [sp]"),                   // reload the stashed fd for the teardown
        Arch::X86_64 => emitter.instruction("mov rax, QWORD PTR [rsp]"),        // reload the stashed fd for the teardown
    }
    super::fclose::emit_tls_session_teardown(emitter, ctx);
    abi::emit_release_temporary_stack(emitter, 16);                             // drop the stashed fd
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x0, #1"),                     // mid-stream crypto disable succeeded
        Arch::X86_64 => emitter.instruction("mov eax, 1"),                      // mid-stream crypto disable succeeded
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {}", done_label)),     // return without attaching TLS
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", done_label)),    // return without attaching TLS
    }

    // -- enable path: publish tls fn pointers, attach the fd, record session --
    emitter.label(&enable_label);
    publish_tls_function_pointers(emitter);
    let fail_label = ctx.next_label("ssec_attach_fail");
    // Uniquified per call so a program may invoke stream_socket_enable_crypto
    // more than once (e.g. enable then disable) without a duplicate label.
    let peer_ok = ctx.next_label("ssec_peer_ok");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- look up the SSL peer-name from the stream context. Stack
            //    slots [sp+16] / [sp+24] receive (ptr, len) on success;
            //    the fall-back hardcoded "localhost" pair is written if
            //    the lookup misses. The fd stays in [sp+0] across all of
            //    this. --
            // 64 B spill: [0]/[8] = peer-name ptr/len, [16]/[24] = ssl.local_cert
            // ptr/len, [32]/[40] = ssl.local_pk ptr/len, [48]/[56] = padding. The
            // saved fd sits at [sp+64] (shifted by this push).
            emitter.instruction("sub sp, sp, #64");                             // peer-name + client-cert/key spill (extends the frame above the fd slot)
            emitter.instruction("add x0, sp, #0");                              // out_ptr address
            emitter.instruction("add x1, sp, #8");                              // out_len address
            emitter.instruction("bl __rt_get_ssl_peer_name");                   // x0 = 1 hit / 0 miss
            emitter.instruction(&format!("cbnz x0, {}", peer_ok));              // hit: use the loaded (ptr, len)
            // -- miss: default the SNI to the connection host recorded by
            //    stream_socket_client (_stream_connect_host[fd]) before falling
            //    back to the hardcoded "localhost". The fd sits at [sp+64]
            //    (saved-fd slot, shifted by the peer-name push above). --
            let host_default = ctx.next_label("ssec_host_default");
            emitter.instruction("ldr x10, [sp, #64]");                          // reload fd for the connect-host table index
            abi::emit_symbol_address(emitter, "x9", "_stream_connect_host");
            emitter.instruction("add x9, x9, x10, lsl #4");                     // &_stream_connect_host[fd] (16-byte ptr/len slots)
            emitter.instruction("ldr x11, [x9, #8]");                           // stashed host length (0 = unset)
            emitter.instruction(&format!("cbz x11, {}", host_default));         // no stashed host → use the "localhost" default
            emitter.instruction("ldr x12, [x9, #0]");                           // stashed host pointer
            emitter.instruction("str x12, [sp, #0]");                           // peer_name ptr = connection host
            emitter.instruction("str x11, [sp, #8]");                           // peer_name len = connection host length
            emitter.instruction(&format!("b {}", peer_ok));                     // host defaulted from the connection — skip localhost
            emitter.label(&host_default);
            abi::emit_symbol_address(emitter, "x9", "_tls_peer_name_default");
            emitter.instruction("str x9, [sp, #0]");                            // fall back to "localhost" ptr
            emitter.instruction("mov x9, #9");                                  // strlen("localhost")
            emitter.instruction("str x9, [sp, #8]");                            // fall back to "localhost" len
            emitter.label(&peer_ok);
            // -- look up ssl.local_cert / ssl.local_pk for mutual-TLS client
            //    auth. The getter leaves the out slots untouched on a miss, so
            //    pre-zero the length slots: a zero length selects the plain
            //    (no client cert) attach variant. --
            let plain_attach = ctx.next_label("ssec_plain_attach");
            let do_attach = ctx.next_label("ssec_do_attach");
            emitter.instruction("str xzr, [sp, #24]");                          // ssl.local_cert length = 0 (no client cert by default)
            emitter.instruction("str xzr, [sp, #40]");                          // ssl.local_pk length = 0
            abi::emit_symbol_address(emitter, "x0", "_ssl_key_str");
            emitter.instruction("mov x1, #3");                                  // strlen("ssl")
            abi::emit_symbol_address(emitter, "x2", "_ssl_local_cert_key_str");
            emitter.instruction("mov x3, #10");                                 // strlen("local_cert")
            emitter.instruction("add x4, sp, #16");                             // local_cert out_ptr address
            emitter.instruction("add x5, sp, #24");                             // local_cert out_len address
            emitter.instruction("bl __rt_get_string_context_option");           // fill [sp+16]/[sp+24] on hit
            abi::emit_symbol_address(emitter, "x0", "_ssl_key_str");
            emitter.instruction("mov x1, #3");                                  // strlen("ssl")
            abi::emit_symbol_address(emitter, "x2", "_ssl_local_pk_key_str");
            emitter.instruction("mov x3, #8");                                  // strlen("local_pk")
            emitter.instruction("add x4, sp, #32");                             // local_pk out_ptr address
            emitter.instruction("add x5, sp, #40");                             // local_pk out_len address
            emitter.instruction("bl __rt_get_string_context_option");           // fill [sp+32]/[sp+40] on hit
            // -- common attach args + variant selection --
            emitter.instruction("ldr x0, [sp, #64]");                           // reload fd → 1st arg
            emitter.instruction("ldr x1, [sp, #0]");                            // peer_name ptr → 2nd arg
            emitter.instruction("ldr x2, [sp, #8]");                            // peer_name len → 3rd arg
            emitter.instruction("ldr x9, [sp, #24]");                           // local_cert length
            emitter.instruction(&format!("cbz x9, {}", plain_attach));          // no client cert → plain attach
            emitter.instruction("ldr x9, [sp, #40]");                           // local_pk length
            emitter.instruction(&format!("cbz x9, {}", plain_attach));          // missing key → plain attach
            emitter.instruction("ldr x3, [sp, #16]");                           // local_cert path ptr → 4th arg
            emitter.instruction("ldr x4, [sp, #24]");                           // local_cert path len → 5th arg
            emitter.instruction("ldr x5, [sp, #32]");                           // local_pk path ptr → 6th arg
            emitter.instruction("ldr x6, [sp, #40]");                           // local_pk path len → 7th arg
            abi::emit_symbol_address(emitter, "x9", "_elephc_tls_attach_fd_client_cert_fn");
            emitter.instruction("ldr x9, [x9]");                                // mutual-TLS attach variant
            emitter.instruction(&format!("b {}", do_attach));                   // call the selected mutual-TLS attach function
            emitter.label(&plain_attach);
            abi::emit_symbol_address(emitter, "x9", "_elephc_tls_attach_fd_fn");
            emitter.instruction("ldr x9, [x9]");                                // server-auth-only attach variant
            emitter.label(&do_attach);
            emitter.instruction("blr x9");                                      // x0 = handle (>=1) or -1
            emitter.instruction("ldr x10, [sp, #64]");                          // reload fd
            abi::emit_release_temporary_stack(emitter, 64);                     // pop the peer-name + cert/key spill area
            abi::emit_release_temporary_stack(emitter, 16);                     // pop the saved fd
            emitter.instruction("cmp x0, #0");                                  // did TLS attach return a failure handle?
            emitter.instruction(&format!("b.lt {}", fail_label));               // report false when attach failed
            abi::emit_symbol_address(emitter, "x11", "_tls_sessions");
            emitter.instruction("str x0, [x11, x10, lsl #3]");                  // _tls_sessions[fd] = handle
            emitter.instruction("mov x0, #1");                                  // report successful TLS enablement
            emitter.instruction(&format!("b {}", done_label));                  // skip the failure result
            emitter.label(&fail_label);
            emitter.instruction("mov x0, #0");                                  // report failed TLS enablement
        }
        Arch::X86_64 => {
            // Same peer-name lookup as the AArch64 branch. `emit_push_reg`
            // on x86_64 reserves 16 bytes (sub rsp,16 + mov), so rsp at
            // this point is 0-mod-16; the spill area below must therefore
            // also be 0-mod-16 in size so the two SysV `call`s land on
            // an aligned rsp. 32 bytes covers (ptr, len) + the required
            // alignment padding.
            // 64 B spill: [0]/[8] = peer-name ptr/len, [16]/[24] = ssl.local_cert
            // ptr/len, [32]/[40] = ssl.local_pk ptr/len, [48]/[56] = padding. The
            // saved fd sits at [rsp+64]. 64 is 0-mod-16, so the SysV calls below
            // land on an aligned rsp.
            emitter.instruction("sub rsp, 64");                                 // peer-name + client-cert/key spill (0-mod-16)
            emitter.instruction("lea rdi, [rsp + 0]");                          // out_ptr address
            emitter.instruction("lea rsi, [rsp + 8]");                          // out_len address
            emitter.instruction("call __rt_get_ssl_peer_name");                 // rax = 1 hit / 0 miss
            emitter.instruction("test rax, rax");                               // did the context provide ssl.peer_name?
            emitter.instruction(&format!("jnz {}", peer_ok));                   // use the loaded peer-name when present
            // -- miss: default the SNI to the connection host recorded by
            //    stream_socket_client (_stream_connect_host[fd]) before falling
            //    back to the hardcoded "localhost". The fd sits at [rsp+64]. --
            let host_default = ctx.next_label("ssec_host_default");
            emitter.instruction("mov r10, QWORD PTR [rsp + 64]");               // reload fd for the connect-host table index
            emitter.instruction("lea r9, [rip + _stream_connect_host]");        // base of the per-fd connect-host table
            emitter.instruction("shl r10, 4");                                  // fd * 16 (ptr/len slot stride)
            emitter.instruction("add r9, r10");                                 // &_stream_connect_host[fd]
            emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                 // stashed host length (0 = unset)
            emitter.instruction("test r11, r11");                               // is a connection host recorded?
            emitter.instruction(&format!("jz {}", host_default));               // no stashed host → use the "localhost" default
            emitter.instruction("mov r10, QWORD PTR [r9 + 0]");                 // stashed host pointer
            emitter.instruction("mov QWORD PTR [rsp + 0], r10");                // peer_name ptr = connection host
            emitter.instruction("mov QWORD PTR [rsp + 8], r11");                // peer_name len = connection host length
            emitter.instruction(&format!("jmp {}", peer_ok));                   // host defaulted from the connection — skip localhost
            emitter.label(&host_default);
            emitter.instruction("lea r9, [rip + _tls_peer_name_default]");      // fallback peer-name literal
            emitter.instruction("mov QWORD PTR [rsp + 0], r9");                 // peer_name ptr = "localhost"
            emitter.instruction("mov r9, 9");                                   // route the immediate through a register so the assembler always emits a 64-bit store
            emitter.instruction("mov QWORD PTR [rsp + 8], r9");                 // peer_name len = strlen("localhost")
            emitter.label(&peer_ok);
            // -- look up ssl.local_cert / ssl.local_pk for mutual-TLS client
            //    auth; pre-zero the length slots so a miss selects plain attach. --
            let plain_attach = ctx.next_label("ssec_plain_attach_x");
            let after_attach = ctx.next_label("ssec_after_attach_x");
            emitter.instruction("mov QWORD PTR [rsp + 24], 0");                 // ssl.local_cert length = 0 (no client cert by default)
            emitter.instruction("mov QWORD PTR [rsp + 40], 0");                 // ssl.local_pk length = 0
            emitter.instruction("lea rdi, [rip + _ssl_key_str]");               // wrapper key "ssl"
            emitter.instruction("mov rsi, 3");                                  // strlen("ssl")
            emitter.instruction("lea rdx, [rip + _ssl_local_cert_key_str]");    // option key "local_cert"
            emitter.instruction("mov rcx, 10");                                 // strlen("local_cert")
            emitter.instruction("lea r8, [rsp + 16]");                          // local_cert out_ptr address
            emitter.instruction("lea r9, [rsp + 24]");                          // local_cert out_len address
            emitter.instruction("call __rt_get_string_context_option");         // fill [rsp+16]/[rsp+24] on hit
            emitter.instruction("lea rdi, [rip + _ssl_key_str]");               // wrapper key "ssl"
            emitter.instruction("mov rsi, 3");                                  // strlen("ssl")
            emitter.instruction("lea rdx, [rip + _ssl_local_pk_key_str]");      // option key "local_pk"
            emitter.instruction("mov rcx, 8");                                  // strlen("local_pk")
            emitter.instruction("lea r8, [rsp + 32]");                          // local_pk out_ptr address
            emitter.instruction("lea r9, [rsp + 40]");                          // local_pk out_len address
            emitter.instruction("call __rt_get_string_context_option");         // fill [rsp+32]/[rsp+40] on hit
            // -- common attach args + variant selection --
            emitter.instruction("mov rdi, QWORD PTR [rsp + 64]");               // reload fd → 1st arg
            emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");                // peer_name ptr → 2nd arg
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // peer_name len → 3rd arg
            emitter.instruction("mov rax, QWORD PTR [rsp + 24]");               // local_cert length
            emitter.instruction("test rax, rax");                               // is a client certificate path present?
            emitter.instruction(&format!("jz {}", plain_attach));               // no client cert → plain attach
            emitter.instruction("mov rax, QWORD PTR [rsp + 40]");               // local_pk length
            emitter.instruction("test rax, rax");                               // is a client key path present?
            emitter.instruction(&format!("jz {}", plain_attach));               // missing key → plain attach
            // mutual-TLS attach: args 4-6 in rcx/r8/r9, arg 7 (key_len) on stack
            emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");               // local_cert path ptr → 4th arg
            emitter.instruction("mov r8, QWORD PTR [rsp + 24]");                // local_cert path len → 5th arg
            emitter.instruction("mov rax, QWORD PTR [rsp + 40]");               // local_pk path len (for the stack arg)
            emitter.instruction("mov r9, QWORD PTR [rsp + 32]");                // local_pk path ptr → 6th arg
            emitter.instruction("sub rsp, 16");                                 // reserve the 7th stack arg + padding (stays 0-mod-16)
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // 7th arg = local_pk path len
            emitter.instruction("mov r10, QWORD PTR [rip + _elephc_tls_attach_fd_client_cert_fn]"); // mutual-TLS attach function pointer
            emitter.instruction("call r10");                                    // rax = handle or -1
            emitter.instruction("add rsp, 16");                                 // pop the 7th stack arg
            emitter.instruction(&format!("jmp {}", after_attach));              // skip the plain attach variant
            emitter.label(&plain_attach);
            emitter.instruction("mov rdi, QWORD PTR [rsp + 64]");               // reload fd → 1st arg
            emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");                // peer_name ptr → 2nd arg
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // peer_name len → 3rd arg
            emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_attach_fd_fn]"); // server-auth-only attach function pointer
            emitter.instruction("call r9");                                     // rax = handle or -1
            emitter.label(&after_attach);
            emitter.instruction("mov r10, QWORD PTR [rsp + 64]");               // reload fd
            abi::emit_release_temporary_stack(emitter, 64);                     // pop peer-name + cert/key spill
            abi::emit_release_temporary_stack(emitter, 16);                     // pop saved fd
            emitter.instruction("cmp rax, 0");                                  // did TLS attach return a failure handle?
            emitter.instruction(&format!("jl {}", fail_label));                 // report false when attach failed
            emitter.instruction("lea r11, [rip + _tls_sessions]");              // TLS session handle table
            emitter.instruction("mov QWORD PTR [r11 + r10 * 8], rax");          // _tls_sessions[fd] = handle
            emitter.instruction("mov eax, 1");                                  // report successful TLS enablement
            emitter.instruction(&format!("jmp {}", done_label));                // skip the failure result
            emitter.label(&fail_label);
            emitter.instruction("xor eax, eax");                                // report failed TLS enablement
        }
    }

    emitter.label(&done_label);
    Some(PhpType::Bool)
}
