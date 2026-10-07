//! Purpose:
//! Close-time filters, TLS teardown, and socket crypto attach.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Tears down the TLS session attached to the current fd result, if one exists.
pub(super) fn emit_tls_session_teardown_for_current_fd(ctx: &mut FunctionContext<'_>) {
    let skip = ctx.next_label("tls_teardown_skip");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_tls_session_clear");
            ctx.emitter.instruction(&format!("cbz x0, {}", skip));              // skip close_notify when no TLS session is attached
            abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_tls_close_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the published TLS close function pointer
            ctx.emitter.emit_published_bridge_call("x9");                       // close the TLS session and send close_notify
            ctx.emitter.label(&skip);
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the descriptor to the bounded TLS session registry
            abi::emit_call_label(ctx.emitter, "__rt_tls_session_clear");
            ctx.emitter.instruction("test rax, rax");                           // did this descriptor own a TLS session?
            ctx.emitter.instruction(&format!("je {}", skip));                   // skip close_notify when no TLS session is attached
            ctx.emitter.instruction("mov rdi, rax");                            // pass the removed TLS session to the close helper
            abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_elephc_tls_close_fn", 0); // load the published TLS close function pointer
            ctx.emitter.emit_published_bridge_call("r9");                       // close the TLS session and send close_notify
            ctx.emitter.label(&skip);
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
}

/// Flushes an attached zlib.deflate write filter before the fd is closed.
pub(super) fn emit_zlib_flush_on_close_for_current_fd(ctx: &mut FunctionContext<'_>) {
    let skip = ctx.next_label("fclose_zlib_skip");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_zstream_handles");
            ctx.emitter.instruction("ldr x10, [x9, x0, lsl #3]");               // load this descriptor's zlib stream handle
            ctx.emitter.instruction(&format!("cbz x10, {}", skip));             // skip flush when no zlib filter is attached
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_zlib_close_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the zlib close helper pointer
            ctx.emitter.instruction("blr x9");                                  // flush the deflate tail and end the zlib stream
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.label(&skip);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_zstream_handles");    // zlib stream handle table base
            ctx.emitter.instruction("mov r10, QWORD PTR [r9 + rax*8]");         // load this descriptor's zlib stream handle
            ctx.emitter.instruction("test r10, r10");                           // test whether a zlib filter is attached
            ctx.emitter.instruction(&format!("je {}", skip));                   // skip flush when no zlib filter is attached
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the fd to the zlib close helper
            abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_zlib_close_fn", 0); // load the zlib close helper pointer
            ctx.emitter.instruction("call r9");                                 // flush the deflate tail and end the zlib stream
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.label(&skip);
        }
    }
}

/// Flushes a `bzip2.compress` write filter before closing the current descriptor.
pub(super) fn emit_bz2_flush_on_close_for_current_fd(ctx: &mut FunctionContext<'_>) {
    let skip = ctx.next_label("fclose_bz2_skip");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_bzstream_handles");
            ctx.emitter.instruction("ldr x10, [x9, x0, lsl #3]");               // load this descriptor's bzip2 stream handle
            ctx.emitter.instruction(&format!("cbz x10, {}", skip));             // skip flush when no bzip2 filter is attached
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_bz2_close_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the bzip2 close helper pointer
            ctx.emitter.instruction("blr x9");                                  // flush the compressed tail and end the bzip2 stream
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.label(&skip);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_bzstream_handles");   // bzip2 stream handle table base
            ctx.emitter.instruction("mov r10, QWORD PTR [r9 + rax*8]");         // load this descriptor's bzip2 stream handle
            ctx.emitter.instruction("test r10, r10");                           // test whether a bzip2 filter is attached
            ctx.emitter.instruction(&format!("je {}", skip));                   // skip flush when no bzip2 filter is attached
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the fd to the bzip2 close helper
            abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_bz2_close_fn", 0); // load the bzip2 close helper pointer
            ctx.emitter.instruction("call r9");                                 // flush the compressed tail and end the bzip2 stream
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.label(&skip);
        }
    }
}

/// Closes a `convert.iconv` write filter before closing the current descriptor.
pub(super) fn emit_iconv_flush_on_close_for_current_fd(ctx: &mut FunctionContext<'_>) {
    let skip = ctx.next_label("fclose_iconv_skip");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_iconv_handles");
            ctx.emitter.instruction("ldr x10, [x9, x0, lsl #3]");               // load this descriptor's iconv transcoder handle
            ctx.emitter.instruction(&format!("cbz x10, {}", skip));             // skip close when no iconv write filter is attached
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_iconv_close_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the iconv close helper pointer
            ctx.emitter.instruction("blr x9");                                  // close the transcoder and clear the handle
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.label(&skip);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_iconv_handles");      // iconv transcoder handle table base
            ctx.emitter.instruction("mov r10, QWORD PTR [r9 + rax*8]");         // load this descriptor's iconv transcoder handle
            ctx.emitter.instruction("test r10, r10");                           // test whether an iconv write filter is attached
            ctx.emitter.instruction(&format!("je {}", skip));                   // skip close when no iconv write filter is attached
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the fd to the iconv close helper
            abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_iconv_close_fn", 0); // load the iconv close helper pointer
            ctx.emitter.instruction("call r9");                                 // close the transcoder and clear the handle
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.label(&skip);
        }
    }
}

/// Emits the AArch64 TLS attach path for `stream_socket_enable_crypto(true)`.
#[allow(dead_code)]
fn legacy_lower_stream_socket_enable_crypto_attach_aarch64(
    ctx: &mut FunctionContext<'_>,
    done_label: &str,
) {
    let fail_label = ctx.next_label("ssec_attach_fail");
    let peer_ok = ctx.next_label("ssec_peer_ok");
    let host_default = ctx.next_label("ssec_host_default");
    let plain_attach = ctx.next_label("ssec_plain_attach");
    let do_attach = ctx.next_label("ssec_do_attach");
    ctx.emitter.instruction("sub sp, sp, #64");                                 // reserve peer-name and client-cert/key spill storage
    ctx.emitter.instruction("add x0, sp, #0");                                  // pass peer-name out_ptr address
    ctx.emitter.instruction("add x1, sp, #8");                                  // pass peer-name out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_ssl_peer_name");
    ctx.emitter.instruction(&format!("cbnz x0, {}", peer_ok));                  // use ssl.peer_name when the context provides it
    ctx.emitter.instruction("ldr x10, [sp, #64]");                              // reload fd for the connect-host table lookup
    abi::emit_symbol_address(ctx.emitter, "x9", "_stream_connect_host");
    ctx.emitter.instruction("add x9, x9, x10, lsl #4");                         // address this fd's saved host pointer/length pair
    ctx.emitter.instruction("ldr x11, [x9, #8]");                               // load the saved connection-host byte length
    ctx.emitter.instruction(&format!("cbz x11, {}", host_default));             // fall back to localhost when no connection host is known
    ctx.emitter.instruction("ldr x12, [x9, #0]");                               // load the saved connection-host pointer
    ctx.emitter.instruction("str x12, [sp, #0]");                               // use the connection host as peer_name pointer
    ctx.emitter.instruction("str x11, [sp, #8]");                               // use the connection host as peer_name length
    ctx.emitter.instruction(&format!("b {}", peer_ok));                         // skip the localhost fallback
    ctx.emitter.label(&host_default);
    abi::emit_symbol_address(ctx.emitter, "x9", "_tls_peer_name_default");
    ctx.emitter.instruction("str x9, [sp, #0]");                                // use localhost as the fallback peer_name pointer
    ctx.emitter.instruction("mov x9, #9");                                      // strlen("localhost")
    ctx.emitter.instruction("str x9, [sp, #8]");                                // use localhost as the fallback peer_name length
    ctx.emitter.label(&peer_ok);

    ctx.emitter.instruction("str xzr, [sp, #24]");                              // default local_cert length to zero
    ctx.emitter.instruction("str xzr, [sp, #40]");                              // default local_pk length to zero
    abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
    ctx.emitter.instruction("mov x1, #3");                                      // strlen("ssl")
    abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_local_cert_key_str");
    ctx.emitter.instruction("mov x3, #10");                                     // strlen("local_cert")
    ctx.emitter.instruction("add x4, sp, #16");                                 // pass local_cert out_ptr address
    ctx.emitter.instruction("add x5, sp, #24");                                 // pass local_cert out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_string_context_option");
    abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
    ctx.emitter.instruction("mov x1, #3");                                      // strlen("ssl")
    abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_local_pk_key_str");
    ctx.emitter.instruction("mov x3, #8");                                      // strlen("local_pk")
    ctx.emitter.instruction("add x4, sp, #32");                                 // pass local_pk out_ptr address
    ctx.emitter.instruction("add x5, sp, #40");                                 // pass local_pk out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_string_context_option");

    ctx.emitter.instruction("ldr x0, [sp, #64]");                               // reload fd as the first TLS attach argument
    ctx.emitter.instruction("ldr x1, [sp, #0]");                                // pass peer_name pointer
    ctx.emitter.instruction("ldr x2, [sp, #8]");                                // pass peer_name byte length
    ctx.emitter.instruction("ldr x9, [sp, #24]");                               // load local_cert byte length
    ctx.emitter.instruction(&format!("cbz x9, {}", plain_attach));              // no client certificate selects plain TLS attach
    ctx.emitter.instruction("ldr x9, [sp, #40]");                               // load local_pk byte length
    ctx.emitter.instruction(&format!("cbz x9, {}", plain_attach));              // missing key selects plain TLS attach
    ctx.emitter.instruction("ldr x3, [sp, #16]");                               // pass local_cert path pointer
    ctx.emitter.instruction("ldr x4, [sp, #24]");                               // pass local_cert path length
    ctx.emitter.instruction("ldr x5, [sp, #32]");                               // pass local_pk path pointer
    ctx.emitter.instruction("ldr x6, [sp, #40]");                               // pass local_pk path length
    abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_tls_attach_fd_client_cert_fn");
    ctx.emitter.instruction("ldr x9, [x9]");                                    // load the mutual-TLS attach function pointer
    ctx.emitter.instruction(&format!("b {}", do_attach));                       // call the selected attach function
    ctx.emitter.label(&plain_attach);
    abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_tls_attach_fd_fn");
    ctx.emitter.instruction("ldr x9, [x9]");                                    // load the default TLS attach function pointer
    ctx.emitter.label(&do_attach);
    ctx.emitter.instruction("blr x9");                                          // attach TLS to the fd and return a session handle
    ctx.emitter.instruction("ldr x10, [sp, #64]");                              // reload fd before releasing the spill storage
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.instruction("cmp x0, #0");                                      // negative handles indicate TLS attach failure
    ctx.emitter.instruction(&format!("b.lt {}", fail_label));                   // return false when attach failed
    abi::emit_symbol_address(ctx.emitter, "x11", "_tls_sessions");
    ctx.emitter.instruction("str x0, [x11, x10, lsl #3]");                      // store the TLS session handle for this fd
    ctx.emitter.instruction("mov x0, #1");                                      // return true after successful TLS attach
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the failure result
    ctx.emitter.label(&fail_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false after TLS attach failure
}

/// Emits the x86_64 TLS attach path for `stream_socket_enable_crypto(true)`.
#[allow(dead_code)]
fn legacy_lower_stream_socket_enable_crypto_attach_x86_64(
    ctx: &mut FunctionContext<'_>,
    done_label: &str,
) {
    let fail_label = ctx.next_label("ssec_attach_fail");
    let peer_ok = ctx.next_label("ssec_peer_ok");
    let host_default = ctx.next_label("ssec_host_default");
    let plain_attach = ctx.next_label("ssec_plain_attach_x");
    let after_attach = ctx.next_label("ssec_after_attach_x");
    ctx.emitter.instruction("sub rsp, 64");                                     // reserve peer-name and client-cert/key spill storage
    ctx.emitter.instruction("lea rdi, [rsp + 0]");                              // pass peer-name out_ptr address
    ctx.emitter.instruction("lea rsi, [rsp + 8]");                              // pass peer-name out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_ssl_peer_name");
    ctx.emitter.instruction("test rax, rax");                                   // did the context provide ssl.peer_name?
    ctx.emitter.instruction(&format!("jnz {}", peer_ok));                       // use ssl.peer_name when present
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 64]");                   // reload fd for the connect-host table lookup
    abi::emit_symbol_address(ctx.emitter, "r9", "_stream_connect_host");
    ctx.emitter.instruction("shl r10, 4");                                      // fd * 16, the host table stride
    ctx.emitter.instruction("add r9, r10");                                     // address this fd's saved host pointer/length pair
    ctx.emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                     // load the saved connection-host byte length
    ctx.emitter.instruction("test r11, r11");                                   // is a connection host known for this fd?
    ctx.emitter.instruction(&format!("jz {}", host_default));                   // fall back to localhost when no host is known
    ctx.emitter.instruction("mov r10, QWORD PTR [r9 + 0]");                     // load the saved connection-host pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], r10");                    // use the connection host as peer_name pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r11");                    // use the connection host as peer_name length
    ctx.emitter.instruction(&format!("jmp {}", peer_ok));                       // skip the localhost fallback
    ctx.emitter.label(&host_default);
    abi::emit_symbol_address(ctx.emitter, "r9", "_tls_peer_name_default");
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], r9");                     // use localhost as the fallback peer_name pointer
    ctx.emitter.instruction("mov r9, 9");                                       // strlen("localhost")
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r9");                     // use localhost as the fallback peer_name length
    ctx.emitter.label(&peer_ok);

    ctx.emitter.instruction("mov QWORD PTR [rsp + 24], 0");                     // default local_cert length to zero
    ctx.emitter.instruction("mov QWORD PTR [rsp + 40], 0");                     // default local_pk length to zero
    abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
    ctx.emitter.instruction("mov rsi, 3");                                      // strlen("ssl")
    abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_local_cert_key_str");
    ctx.emitter.instruction("mov rcx, 10");                                     // strlen("local_cert")
    ctx.emitter.instruction("lea r8, [rsp + 16]");                              // pass local_cert out_ptr address
    ctx.emitter.instruction("lea r9, [rsp + 24]");                              // pass local_cert out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_string_context_option");
    abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
    ctx.emitter.instruction("mov rsi, 3");                                      // strlen("ssl")
    abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_local_pk_key_str");
    ctx.emitter.instruction("mov rcx, 8");                                      // strlen("local_pk")
    ctx.emitter.instruction("lea r8, [rsp + 32]");                              // pass local_pk out_ptr address
    ctx.emitter.instruction("lea r9, [rsp + 40]");                              // pass local_pk out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_string_context_option");

    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 64]");                   // reload fd as the first TLS attach argument
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");                    // pass peer_name pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                    // pass peer_name byte length
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 24]");                   // load local_cert byte length
    ctx.emitter.instruction("test rax, rax");                                   // is a client certificate path present?
    ctx.emitter.instruction(&format!("jz {}", plain_attach));                   // no client certificate selects plain TLS attach
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                   // load local_pk byte length
    ctx.emitter.instruction("test rax, rax");                                   // is a client private key path present?
    ctx.emitter.instruction(&format!("jz {}", plain_attach));                   // missing key selects plain TLS attach
    ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");                   // pass local_cert path pointer
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 24]");                    // pass local_cert path length
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                   // stage local_pk path length for the stack argument
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 32]");                    // pass local_pk path pointer
    ctx.emitter.instruction("sub rsp, 16");                                     // reserve the seventh stack argument plus padding
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");                    // pass local_pk path length as the seventh argument
    abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_elephc_tls_attach_fd_client_cert_fn", 0); // load the mutual-TLS attach function pointer
    ctx.emitter.instruction("call r10");                                        // attach TLS with a client certificate
    ctx.emitter.instruction("add rsp, 16");                                     // release the seventh stack argument
    ctx.emitter.instruction(&format!("jmp {}", after_attach));                  // skip the default attach variant
    ctx.emitter.label(&plain_attach);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 64]");                   // reload fd as the first TLS attach argument
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");                    // pass peer_name pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                    // pass peer_name byte length
    abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_elephc_tls_attach_fd_fn", 0); // load the default TLS attach function pointer
    ctx.emitter.instruction("call r9");                                         // attach TLS and return a session handle
    ctx.emitter.label(&after_attach);
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 64]");                   // reload fd before releasing the spill storage
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.instruction("cmp rax, 0");                                      // negative handles indicate TLS attach failure
    ctx.emitter.instruction(&format!("jl {}", fail_label));                     // return false when attach failed
    abi::emit_symbol_address(ctx.emitter, "r11", "_tls_sessions");
    ctx.emitter.instruction("mov QWORD PTR [r11 + r10 * 8], rax");              // store the TLS session handle for this fd
    ctx.emitter.instruction("mov eax, 1");                                      // return true after successful TLS attach
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the failure result
    ctx.emitter.label(&fail_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false after TLS attach failure
}

/// Reads one string-valued SSL context option into the current TLS options frame.
fn emit_tls_string_option(
    ctx: &mut FunctionContext<'_>,
    key_symbol: &str,
    key_len: usize,
    ptr_offset: usize,
    len_offset: usize,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
            ctx.emitter.instruction("mov x1, #3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "x2", key_symbol);
            ctx.emitter.instruction(&format!("mov x3, #{}", key_len));          // pass the SSL option-name length
            ctx.emitter.instruction(&format!("add x4, sp, #{}", ptr_offset));   // pass the option pointer-field address
            ctx.emitter.instruction(&format!("add x5, sp, #{}", len_offset));   // pass the option length-field address
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
            ctx.emitter.instruction("mov rsi, 3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "rdx", key_symbol);
            ctx.emitter.instruction(&format!("mov rcx, {}", key_len));          // pass the SSL option-name length
            ctx.emitter.instruction(&format!("lea r8, [rsp + {}]", ptr_offset)); // pass the option pointer-field address
            ctx.emitter.instruction(&format!("lea r9, [rsp + {}]", len_offset)); // pass the option length-field address
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_get_string_context_option");
}

/// Resolves the TLS options PHP exposes through the `ssl` stream-context group.
///
/// The frame begins with `ElephcTlsClientOptions` v6 (184 bytes); caller
/// metadata follows at offsets 216..240 after the attach function reserves its
/// scratch.
fn emit_stream_crypto_tls_options(ctx: &mut FunctionContext<'_>) {
    let peer_ok = ctx.next_label("ssec_peer_ok");
    let verify_peer_done = ctx.next_label("ssec_verify_peer_done");
    let verify_name_done = ctx.next_label("ssec_verify_name_done");
    let allow_self_signed_done = ctx.next_label("ssec_allow_self_signed_done");
    let sni_done = ctx.next_label("ssec_sni_done");
    let no_ticket_done = ctx.next_label("ssec_no_ticket_done");
    let method_done = ctx.next_label("ssec_crypto_method_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #6");                              // select TLS options ABI v6 with cipher/security/compression tail
            ctx.emitter.instruction("str w9, [sp, #0]");                        // initialize the options ABI version
            ctx.emitter.instruction("mov w9, #3");                              // verify peer and peer name by default
            ctx.emitter.instruction("str w9, [sp, #4]");                        // initialize PHP-compatible verification flags
            for offset in [8usize, 24, 40, 56, 72, 88, 104, 128, 144] {
                ctx.emitter.instruction(&format!("stp xzr, xzr, [sp, #{}]", offset)); // clear one optional pointer-length pair
            }
            ctx.emitter.instruction("str wzr, [sp, #120]");                     // clear the v6 option flags
            ctx.emitter.instruction("str xzr, [sp, #160]");                     // clear the optional security level
            ctx.emitter.instruction("str wzr, [sp, #168]");                     // clear security-level presence
            ctx.emitter.instruction("str wzr, [sp, #172]");                     // clear the optional compression flag
            ctx.emitter.instruction("str wzr, [sp, #176]");                     // clear compression-option presence
            ctx.emitter.instruction("add x0, sp, #8");                          // pass the peer-name pointer-field address
            ctx.emitter.instruction("add x1, sp, #16");                         // pass the peer-name length-field address
            abi::emit_call_label(ctx.emitter, "__rt_get_ssl_peer_name");
            ctx.emitter.instruction(&format!("cbnz x0, {}", peer_ok));          // retain an explicit ssl.peer_name when present
            ctx.emitter.instruction("ldr x0, [sp, #240]");                      // reload the descriptor for saved-host lookup
            abi::emit_call_label(ctx.emitter, "__rt_get_stashed_connect_host");
            ctx.emitter.instruction(&format!("cbz x0, {}", peer_ok));           // leave the peer name absent when URL parsing had no host
            ctx.emitter.instruction("str x1, [sp, #8]");                        // use the saved connect host as the TLS peer name
            ctx.emitter.instruction("str x2, [sp, #16]");                       // retain the saved connect-host byte length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov DWORD PTR [rsp + 0], 6");              // select TLS options ABI v6 with cipher/security/compression tail
            ctx.emitter.instruction("mov DWORD PTR [rsp + 4], 3");              // verify peer and peer name by default
            for offset in [8usize, 16, 24, 32, 40, 48, 56, 64, 72, 80, 88, 96, 104, 112, 128, 136, 144, 152] {
                ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", offset)); // clear one optional options word
            }
            ctx.emitter.instruction("mov DWORD PTR [rsp + 120], 0");            // clear v6 option flags
            ctx.emitter.instruction("mov QWORD PTR [rsp + 160], 0");            // clear the optional security level
            ctx.emitter.instruction("mov DWORD PTR [rsp + 168], 0");            // clear security-level presence
            ctx.emitter.instruction("mov DWORD PTR [rsp + 172], 0");            // clear the optional compression flag
            ctx.emitter.instruction("mov DWORD PTR [rsp + 176], 0");            // clear compression-option presence
            ctx.emitter.instruction("lea rdi, [rsp + 8]");                      // pass the peer-name pointer-field address
            ctx.emitter.instruction("lea rsi, [rsp + 16]");                     // pass the peer-name length-field address
            abi::emit_call_label(ctx.emitter, "__rt_get_ssl_peer_name");
            ctx.emitter.instruction("test rax, rax");                           // did the context provide ssl.peer_name?
            ctx.emitter.instruction(&format!("jnz {}", peer_ok));               // retain an explicit ssl.peer_name when present
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 240]");          // reload the descriptor for saved-host lookup
            abi::emit_call_label(ctx.emitter, "__rt_get_stashed_connect_host");
            ctx.emitter.instruction("test rax, rax");                           // did URL parsing retain a connection host?
            ctx.emitter.instruction(&format!("jz {}", peer_ok));                // leave the peer name absent when URL parsing had no host
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // use the saved connect host as the TLS peer name
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rdx");           // retain the saved connect-host byte length
        }
    }
    ctx.emitter.label(&peer_ok);
    emit_tls_string_option(ctx, "_ssl_cafile_key_str", 6, 24, 32);
    emit_tls_string_option(ctx, "_ssl_capath_key_str", 6, 40, 48);
    emit_tls_string_option(ctx, "_ssl_local_cert_key_str", 10, 56, 64);
    emit_tls_string_option(ctx, "_ssl_local_pk_key_str", 8, 72, 80);
    emit_tls_string_option(ctx, "_ssl_passphrase_key_str", 10, 128, 136);
    emit_tls_string_option(ctx, "_ssl_alpn_protocols_key_str", 14, 88, 96);
    emit_tls_string_option(ctx, "_ssl_ciphers_key_str", 7, 144, 152);
    emit_tls_peer_fingerprint_option(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, #1");                              // default verify_peer to true
            ctx.emitter.instruction("str x9, [sp, #184]");                      // seed boolean context-option scratch storage
            abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
            ctx.emitter.instruction("mov x1, #3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_verify_peer_key_str");
            ctx.emitter.instruction("mov x3, #11");                             // pass strlen("verify_peer")
            ctx.emitter.instruction("add x4, sp, #184");                        // pass reusable boolean scratch storage
            abi::emit_call_label(ctx.emitter, "__rt_get_bool_context_option");
            ctx.emitter.instruction("ldr x9, [sp, #184]");                      // load the resolved verify_peer value
            ctx.emitter.instruction(&format!("cbnz x9, {}", verify_peer_done)); // retain certificate verification when truthy
            ctx.emitter.instruction("ldr w10, [sp, #4]");                       // load the TLS verification policy bits
            ctx.emitter.instruction("bic w10, w10, #1");                        // disable only certificate-chain verification
            ctx.emitter.instruction("str w10, [sp, #4]");                       // retain the adjusted verification policy
            ctx.emitter.label(&verify_peer_done);
            ctx.emitter.instruction("mov x9, #1");                              // default verify_peer_name to true
            ctx.emitter.instruction("str x9, [sp, #184]");                      // seed boolean context-option scratch storage
            abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
            ctx.emitter.instruction("mov x1, #3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_verify_peer_name_key_str");
            ctx.emitter.instruction("mov x3, #16");                             // pass strlen("verify_peer_name")
            ctx.emitter.instruction("add x4, sp, #184");                        // pass reusable boolean scratch storage
            abi::emit_call_label(ctx.emitter, "__rt_get_bool_context_option");
            ctx.emitter.instruction("ldr x9, [sp, #184]");                      // load the resolved verify_peer_name value
            ctx.emitter.instruction(&format!("cbnz x9, {}", verify_name_done)); // retain peer-name verification when truthy
            ctx.emitter.instruction("ldr w10, [sp, #4]");                       // load the TLS verification policy bits
            ctx.emitter.instruction("bic w10, w10, #2");                        // disable only peer-name verification
            ctx.emitter.instruction("str w10, [sp, #4]");                       // retain the adjusted verification policy
            ctx.emitter.label(&verify_name_done);
            ctx.emitter.instruction("str xzr, [sp, #184]");                     // default allow_self_signed to false
            abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
            ctx.emitter.instruction("mov x1, #3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_allow_self_signed_key_str");
            ctx.emitter.instruction("mov x3, #17");                             // pass strlen("allow_self_signed")
            ctx.emitter.instruction("add x4, sp, #184");                        // pass reusable boolean scratch storage
            abi::emit_call_label(ctx.emitter, "__rt_get_bool_context_option");
            ctx.emitter.instruction("ldr x9, [sp, #184]");                      // load the resolved self-signed policy value
            ctx.emitter.instruction(&format!("cbz x9, {}", allow_self_signed_done)); // leave self-signed certificates rejected by default
            ctx.emitter.instruction("ldr w10, [sp, #4]");                       // load the TLS verification policy bits
            ctx.emitter.instruction("orr w10, w10, #4");                        // allow only a depth-zero self-signed leaf
            ctx.emitter.instruction("str w10, [sp, #4]");                       // retain the adjusted verification policy
            ctx.emitter.label(&allow_self_signed_done);
            ctx.emitter.instruction("mov x9, #1");                              // default SNI enabled
            ctx.emitter.instruction("str x9, [sp, #184]");                      // materialize TLS ABI state: str x9, [sp, #184]
            abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
            ctx.emitter.instruction("mov x1, #3");                              // materialize TLS ABI state: mov x1, #3
            abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_sni_enabled_key_str");
            ctx.emitter.instruction("mov x3, #11");                             // materialize TLS ABI state: mov x3, #11
            ctx.emitter.instruction("add x4, sp, #184");                        // materialize TLS ABI state: add x4, sp, #184
            abi::emit_call_label(ctx.emitter, "__rt_get_bool_context_option");
            ctx.emitter.instruction("ldr x9, [sp, #184]");                      // reload the resolved SNI value after the lookup call
            ctx.emitter.instruction(&format!("cbnz x9, {}", sni_done));         // keep SNI enabled when the option is truthy
            ctx.emitter.instruction("ldr w10, [sp, #120]");                     // materialize TLS ABI state: ldr w10, [sp, #120]
            ctx.emitter.instruction("orr w10, w10, #1");                        // materialize TLS ABI state: orr w10, w10, #1
            ctx.emitter.instruction("str w10, [sp, #120]");                     // materialize TLS ABI state: str w10, [sp, #120]
            ctx.emitter.label(&sni_done);
            ctx.emitter.instruction("str xzr, [sp, #184]");                     // default no_ticket=false
            abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
            ctx.emitter.instruction("mov x1, #3");                              // materialize TLS ABI state: mov x1, #3
            abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_no_ticket_key_str");
            ctx.emitter.instruction("mov x3, #9");                              // materialize TLS ABI state: mov x3, #9
            ctx.emitter.instruction("add x4, sp, #184");                        // materialize TLS ABI state: add x4, sp, #184
            abi::emit_call_label(ctx.emitter, "__rt_get_bool_context_option");
            ctx.emitter.instruction("ldr x9, [sp, #184]");                      // reload the resolved ticket value after the lookup call
            ctx.emitter.instruction(&format!("cbz x9, {}", no_ticket_done));    // keep tickets enabled when the option is false
            ctx.emitter.instruction("ldr w10, [sp, #120]");                     // materialize TLS ABI state: ldr w10, [sp, #120]
            ctx.emitter.instruction("orr w10, w10, #2");                        // materialize TLS ABI state: orr w10, w10, #2
            ctx.emitter.instruction("str w10, [sp, #120]");                     // materialize TLS ABI state: str w10, [sp, #120]
            ctx.emitter.label(&no_ticket_done);
            abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
            ctx.emitter.instruction("mov x1, #3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_security_level_key_str");
            ctx.emitter.instruction("mov x3, #14");                             // pass strlen("security_level")
            ctx.emitter.instruction("add x4, sp, #160");                        // write the optional security level
            abi::emit_call_label(ctx.emitter, "__rt_get_int_context_option");
            ctx.emitter.instruction("str w0, [sp, #168]");                      // retain whether security_level was supplied
            ctx.emitter.instruction("str wzr, [sp, #172]");                     // default disable_compression=false
            abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
            ctx.emitter.instruction("mov x1, #3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_disable_compression_key_str");
            ctx.emitter.instruction("mov x3, #19");                             // pass strlen("disable_compression")
            ctx.emitter.instruction("add x4, sp, #172");                        // write the optional compression flag
            abi::emit_call_label(ctx.emitter, "__rt_get_bool_context_option");
            ctx.emitter.instruction("str w0, [sp, #176]");                      // retain whether compression was supplied
            ctx.emitter.instruction("ldr x9, [sp, #224]");                      // did the caller explicitly supply a crypto method?
            ctx.emitter.instruction(&format!("cbnz x9, {}", method_done));      // explicit zero must override ssl.crypto_method
            abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
            ctx.emitter.instruction("mov x1, #3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_crypto_method_key_str");
            ctx.emitter.instruction("mov x3, #13");                             // pass strlen("crypto_method")
            ctx.emitter.instruction("add x4, sp, #216");                        // write the resolved context crypto method
            abi::emit_call_label(ctx.emitter, "__rt_get_int_context_option");
            ctx.emitter.instruction("str x0, [sp, #224]");                      // retain whether ssl.crypto_method was found
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 192], 1");            // default verify_peer to true
            abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
            ctx.emitter.instruction("mov rsi, 3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_verify_peer_key_str");
            ctx.emitter.instruction("mov rcx, 11");                             // pass strlen("verify_peer")
            ctx.emitter.instruction("lea r8, [rsp + 192]");                     // pass reusable boolean scratch storage
            abi::emit_call_label(ctx.emitter, "__rt_get_bool_context_option");
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 192], 0");            // is certificate verification enabled?
            ctx.emitter.instruction(&format!("jne {}", verify_peer_done));      // retain certificate verification when truthy
            ctx.emitter.instruction("and DWORD PTR [rsp + 4], -2");             // disable only certificate-chain verification
            ctx.emitter.label(&verify_peer_done);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 192], 1");            // default verify_peer_name to true
            abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
            ctx.emitter.instruction("mov rsi, 3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_verify_peer_name_key_str");
            ctx.emitter.instruction("mov rcx, 16");                             // pass strlen("verify_peer_name")
            ctx.emitter.instruction("lea r8, [rsp + 192]");                     // pass reusable boolean scratch storage
            abi::emit_call_label(ctx.emitter, "__rt_get_bool_context_option");
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 192], 0");            // is peer-name verification enabled?
            ctx.emitter.instruction(&format!("jne {}", verify_name_done));      // retain peer-name verification when truthy
            ctx.emitter.instruction("and DWORD PTR [rsp + 4], -3");             // disable only peer-name verification
            ctx.emitter.label(&verify_name_done);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 192], 0");            // default allow_self_signed to false
            abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
            ctx.emitter.instruction("mov rsi, 3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_allow_self_signed_key_str");
            ctx.emitter.instruction("mov rcx, 17");                             // pass strlen("allow_self_signed")
            ctx.emitter.instruction("lea r8, [rsp + 192]");                     // pass reusable boolean scratch storage
            abi::emit_call_label(ctx.emitter, "__rt_get_bool_context_option");
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 192], 0");            // permit a self-signed leaf only when requested
            ctx.emitter.instruction(&format!("je {}", allow_self_signed_done)); // leave self-signed certificates rejected by default
            ctx.emitter.instruction("or DWORD PTR [rsp + 4], 4");               // allow only a depth-zero self-signed leaf
            ctx.emitter.label(&allow_self_signed_done);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 192], 1");            // default SNI enabled
            abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
            ctx.emitter.instruction("mov rsi, 3");                              // materialize TLS ABI state: mov rsi, 3
            abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_sni_enabled_key_str");
            ctx.emitter.instruction("mov rcx, 11");                             // materialize TLS ABI state: mov rcx, 11
            ctx.emitter.instruction("lea r8, [rsp + 192]");                     // materialize TLS ABI state: lea r8, [rsp + 192]
            abi::emit_call_label(ctx.emitter, "__rt_get_bool_context_option");
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 192], 0");            // materialize TLS ABI state: cmp QWORD PTR [rsp + 192], 0
            ctx.emitter.instruction(&format!("jne {}", sni_done));              // materialize TLS ABI state: runtime ABI operation
            ctx.emitter.instruction("or DWORD PTR [rsp + 120], 1");             // materialize TLS ABI state: or DWORD PTR [rsp + 120], 1
            ctx.emitter.label(&sni_done);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 192], 0");            // default no_ticket=false
            abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
            ctx.emitter.instruction("mov rsi, 3");                              // materialize TLS ABI state: mov rsi, 3
            abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_no_ticket_key_str");
            ctx.emitter.instruction("mov rcx, 9");                              // materialize TLS ABI state: mov rcx, 9
            ctx.emitter.instruction("lea r8, [rsp + 192]");                     // materialize TLS ABI state: lea r8, [rsp + 192]
            abi::emit_call_label(ctx.emitter, "__rt_get_bool_context_option");
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 192], 0");            // materialize TLS ABI state: cmp QWORD PTR [rsp + 192], 0
            ctx.emitter.instruction(&format!("je {}", no_ticket_done));         // materialize TLS ABI state: runtime ABI operation
            ctx.emitter.instruction("or DWORD PTR [rsp + 120], 2");             // materialize TLS ABI state: or DWORD PTR [rsp + 120], 2
            ctx.emitter.label(&no_ticket_done);
            abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
            ctx.emitter.instruction("mov rsi, 3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_security_level_key_str");
            ctx.emitter.instruction("mov rcx, 14");                             // pass strlen("security_level")
            ctx.emitter.instruction("lea r8, [rsp + 160]");                     // write the optional security level
            abi::emit_call_label(ctx.emitter, "__rt_get_int_context_option");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 168], rax");          // retain whether security_level was supplied
            ctx.emitter.instruction("mov DWORD PTR [rsp + 172], 0");            // default disable_compression=false
            abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
            ctx.emitter.instruction("mov rsi, 3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_disable_compression_key_str");
            ctx.emitter.instruction("mov rcx, 19");                             // pass strlen("disable_compression")
            ctx.emitter.instruction("lea r8, [rsp + 172]");                     // write the optional compression flag
            abi::emit_call_label(ctx.emitter, "__rt_get_bool_context_option");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 176], rax");          // retain whether compression was supplied
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 224], 0");            // did the caller explicitly supply a crypto method?
            ctx.emitter.instruction(&format!("jne {}", method_done));           // explicit zero must override ssl.crypto_method
            abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
            ctx.emitter.instruction("mov rsi, 3");                              // pass strlen("ssl") to the context lookup
            abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_crypto_method_key_str");
            ctx.emitter.instruction("mov rcx, 13");                             // pass strlen("crypto_method")
            ctx.emitter.instruction("lea r8, [rsp + 216]");                     // write the resolved context crypto method
            abi::emit_call_label(ctx.emitter, "__rt_get_int_context_option");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 224], rax");          // retain whether ssl.crypto_method was found
        }
    }
    ctx.emitter.label(&method_done);
}

/// Loads scalar or array `ssl.peer_fingerprint` into the v6 ABI pair.
///
/// The runtime helper serializes associative arrays as a bounded
/// `algorithm=fingerprint;...` list. The TLS bridge parses every entry and
/// fails closed for unsupported keys or malformed values.
fn emit_tls_peer_fingerprint_option(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add x0, sp, #104");                        // materialize TLS ABI state: add x0, sp, #104
            ctx.emitter.instruction("add x1, sp, #112");                        // materialize TLS ABI state: add x1, sp, #112
            abi::emit_call_label(ctx.emitter, "__rt_get_ssl_peer_fingerprint");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("lea rdi, [rsp + 104]");                    // materialize TLS ABI state: lea rdi, [rsp + 104]
            ctx.emitter.instruction("lea rsi, [rsp + 112]");                    // materialize TLS ABI state: lea rsi, [rsp + 112]
            abi::emit_call_label(ctx.emitter, "__rt_get_ssl_peer_fingerprint");
        }
    }
}

/// Attaches or resumes TLS on AArch64 and returns the bridge's PHP tri-state status.
pub(super) fn lower_stream_socket_enable_crypto_attach_aarch64(
    ctx: &mut FunctionContext<'_>,
    done_label: &str,
) {
    let fail_label = ctx.next_label("ssec_attach_fail");
    let existing_session = ctx.next_label("ssec_existing_session");
    let handshake = ctx.next_label("ssec_handshake");
    let handshake_failed = ctx.next_label("ssec_handshake_failed");
    let release_failure = ctx.next_label("ssec_release_failure");
    ctx.emitter.instruction("sub sp, sp, #240");                                // reserve v6 TLS options, session, status, and boolean scratch storage
    ctx.emitter.instruction("ldr x0, [sp, #240]");                              // look up any TLS session already associated with this descriptor
    abi::emit_call_label(ctx.emitter, "__rt_tls_session_get");
    ctx.emitter.instruction(&format!("cbnz x0, {}", existing_session));         // resume an in-progress handshake instead of attaching twice
    emit_stream_crypto_tls_options(ctx);
    ctx.emitter.instruction("ldr x0, [sp, #240]");                              // pass the descriptor to the options-aware TLS attach bridge
    ctx.emitter.instruction("mov x1, sp");                                      // pass the complete TLS client options structure
    ctx.emitter.instruction("ldr x2, [sp, #216]");                              // pass the requested PHP crypto method
    ctx.emitter.instruction("ldr x3, [sp, #224]");                              // distinguish explicit zero from an absent method
    ctx.emitter.instruction("ldr x4, [sp, #232]");                              // pass an optional source TLS session for resumption
    abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_tls_attach_fd_with_options_fn");
    ctx.emitter.instruction("ldr x9, [x9]");                                    // load the options-aware TLS attach function pointer
    ctx.emitter.emit_published_bridge_call("x9");                              // attach TLS with independent PHP verification policy
    ctx.emitter.instruction("cmp x0, #0");                                      // negative handles indicate TLS attach failure
    ctx.emitter.instruction(&format!("b.lt {}", fail_label));                   // report attach failure after releasing storage
    ctx.emitter.instruction("str x0, [sp, #144]");                              // preserve the new session across table insertion
    ctx.emitter.instruction("mov x1, x0");                                      // pass the new session as the bounded-map value
    ctx.emitter.instruction("ldr x0, [sp, #240]");                              // pass the full-width descriptor as the bounded-map key
    abi::emit_call_label(ctx.emitter, "__rt_tls_session_set");
    ctx.emitter.instruction(&format!("cbz x0, {}", fail_label));                // table exhaustion closes the session and reports false
    ctx.emitter.instruction(&format!("b {}", handshake));                       // progress the newly attached TLS session
    ctx.emitter.label(&existing_session);
    ctx.emitter.instruction("str x0, [sp, #144]");                              // preserve the existing TLS session across handshake
    ctx.emitter.label(&handshake);
    ctx.emitter.instruction("ldr x0, [sp, #144]");                              // pass the persisted TLS session handle to rustls
    abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_tls_handshake_fn");
    ctx.emitter.instruction("ldr x9, [x9]");                                    // load the published TLS handshake entry pointer
    ctx.emitter.emit_published_bridge_call("x9");                              // complete or advance the TLS handshake through rustls
    ctx.emitter.instruction("sxtw x0, w0");                                     // sign-extend the bridge's i32 handshake status
    ctx.emitter.instruction("str x0, [sp, #152]");                              // retain the PHP tri-state result across frame cleanup
    ctx.emitter.instruction("cmp x0, #0");                                      // terminal handshake failure is negative
    ctx.emitter.instruction(&format!("b.lt {}", handshake_failed));             // clear and close the failed TLS session
    ctx.emitter.instruction("ldr x0, [sp, #152]");                              // reload the retained handshake status
    abi::emit_release_temporary_stack(ctx.emitter, 240);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction(&format!("b {}", done_label));                      // return success or nonblocking progress
    ctx.emitter.label(&handshake_failed);
    ctx.emitter.instruction("ldr x0, [sp, #240]");                              // clear the descriptor-to-session association
    abi::emit_call_label(ctx.emitter, "__rt_tls_session_clear");
    ctx.emitter.instruction("ldr x0, [sp, #144]");                              // close the failed rustls session
    abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_tls_close_fn");
    ctx.emitter.instruction("ldr x9, [x9]");                                    // load the published TLS close entry pointer
    ctx.emitter.emit_published_bridge_call("x9");                              // release the failed TLS session and duplicated socket
    ctx.emitter.instruction("mov x0, #-1");                                     // normalize terminal TLS errors to PHP false
    ctx.emitter.instruction(&format!("b {}", release_failure));                 // release temporary storage before returning
    ctx.emitter.label(&fail_label);
    ctx.emitter.instruction("mov x0, #-1");                                     // return false after TLS attach or table failure
    ctx.emitter.label(&release_failure);
    abi::emit_release_temporary_stack(ctx.emitter, 240);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction(&format!("b {}", done_label));                      // normalize failures through PHP return boxing
}

/// Attaches or resumes TLS on x86_64 and returns the bridge's PHP tri-state status.
pub(super) fn lower_stream_socket_enable_crypto_attach_x86_64(
    ctx: &mut FunctionContext<'_>,
    done_label: &str,
) {
    let fail_label = ctx.next_label("ssec_attach_fail");
    let existing_session = ctx.next_label("ssec_existing_session_x");
    let handshake = ctx.next_label("ssec_handshake_x");
    let handshake_failed = ctx.next_label("ssec_handshake_failed_x");
    let release_failure = ctx.next_label("ssec_release_failure_x");
    ctx.emitter.instruction("sub rsp, 240");                                    // reserve v6 TLS options, session, status, and boolean scratch storage
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 240]");                  // look up any TLS session already associated with this descriptor
    abi::emit_call_label(ctx.emitter, "__rt_tls_session_get");
    ctx.emitter.instruction("test rax, rax");                                   // does this descriptor already own a rustls session?
    ctx.emitter.instruction(&format!("jnz {}", existing_session));              // resume an in-progress handshake instead of attaching twice
    emit_stream_crypto_tls_options(ctx);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 240]");                  // pass the descriptor to the options-aware TLS attach bridge
    ctx.emitter.instruction("mov rsi, rsp");                                    // pass the complete TLS client options structure
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 216]");                  // pass the requested PHP crypto method
    ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 224]");                  // distinguish explicit zero from an absent method
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 232]");                   // pass an optional source TLS session for resumption
    abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_elephc_tls_attach_fd_with_options_fn", 0); // load the options-aware TLS attach pointer
    ctx.emitter.emit_published_bridge_call("r9");                              // attach TLS with independent PHP verification policy
    ctx.emitter.instruction("cmp rax, 0");                                      // negative handles indicate TLS attach failure
    ctx.emitter.instruction(&format!("jl {}", fail_label));                     // report attach failure after releasing storage
    ctx.emitter.instruction("mov QWORD PTR [rsp + 144], rax");                  // preserve the new session across table insertion
    ctx.emitter.instruction("mov rsi, rax");                                    // pass the new session as the bounded-map value
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 240]");                  // pass the full-width descriptor as the bounded-map key
    abi::emit_call_label(ctx.emitter, "__rt_tls_session_set");
    ctx.emitter.instruction("test rax, rax");                                   // did the bounded table retain the new session?
    ctx.emitter.instruction(&format!("jz {}", fail_label));                     // table exhaustion closes the session and reports false
    ctx.emitter.instruction(&format!("jmp {}", handshake));                     // progress the newly attached TLS session
    ctx.emitter.label(&existing_session);
    ctx.emitter.instruction("mov QWORD PTR [rsp + 144], rax");                  // preserve the existing TLS session across handshake
    ctx.emitter.label(&handshake);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 144]");                  // pass the persisted TLS session handle to rustls
    abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_elephc_tls_handshake_fn", 0);
    ctx.emitter.emit_published_bridge_call("r9");                              // complete or advance the TLS handshake through rustls
    ctx.emitter.instruction("movsxd rax, eax");                                 // sign-extend the bridge's i32 handshake status
    ctx.emitter.instruction("mov QWORD PTR [rsp + 152], rax");                  // retain the PHP tri-state result across cleanup
    ctx.emitter.instruction("cmp rax, 0");                                      // terminal handshake failure is negative
    ctx.emitter.instruction(&format!("jl {}", handshake_failed));               // clear and close the failed TLS session
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 152]");                  // reload the retained handshake status
    abi::emit_release_temporary_stack(ctx.emitter, 240);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // return success or nonblocking progress
    ctx.emitter.label(&handshake_failed);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 240]");                  // clear the descriptor-to-session association
    abi::emit_call_label(ctx.emitter, "__rt_tls_session_clear");
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 144]");                  // close the failed rustls session
    abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_elephc_tls_close_fn", 0);
    ctx.emitter.emit_published_bridge_call("r9");                              // release the failed TLS session and duplicated socket
    ctx.emitter.instruction("mov rax, -1");                                     // normalize terminal TLS errors to PHP false
    ctx.emitter.instruction(&format!("jmp {}", release_failure));               // release temporary storage before returning
    ctx.emitter.label(&fail_label);
    ctx.emitter.instruction("mov rax, -1");                                     // return false after TLS attach or table failure
    ctx.emitter.label(&release_failure);
    abi::emit_release_temporary_stack(ctx.emitter, 240);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // normalize failures through PHP return boxing
}
