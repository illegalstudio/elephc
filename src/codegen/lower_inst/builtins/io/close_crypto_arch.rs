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
/// Leaves 1 in the int result register when the stream has a live TLS session.
///
/// `stream_socket_enable_crypto($s, false)` answers on this and nothing else. php-src
/// runs the openssl handler only for a stream that is actually SSL-active: that handler
/// shuts the session down and falls through to `return -1`, which `RETURN_FALSE`s. A
/// stream with no crypto support never reaches it — `php_stream_set_option` reports
/// NOTIMPL, which lands on the `default:` arm and `RETURN_TRUE`s. So a disable on a
/// plain `php://memory` handle is true and a disable on a live TLS socket is false.
///
/// `handle_slot` is the stack offset of the opaque stream handle. The probe has to run
/// BEFORE the teardown, which detaches the session it reads.
pub(super) fn emit_tls_session_present_flag(ctx: &mut FunctionContext<'_>, handle_slot: i64) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_slot)); // the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_tls_session");        // x0 = attached session, zero when plain
            ctx.emitter.instruction("cmp x0, #0");
            ctx.emitter.instruction("cset x0, ne");                              // 1 when a session is attached
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_slot)); // the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_tls_session");        // rax = attached session, zero when plain
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction("setne al");                                 // 1 when a session is attached
            ctx.emitter.instruction("movzx rax, al");
        }
    }
}

/// Tears down the TLS session attached to a stream, if one exists.
///
/// `handle_slot` is the stack offset of the opaque stream handle, measured from
/// `sp`/`rsp` on entry. The session is reached THROUGH THE HANDLE rather than through
/// the descriptor: it lives on the StreamState, which a reused descriptor number cannot
/// address — that aliasing is exactly what the raw-fd table used to allow.
///
/// The descriptor in the result register is preserved, because both callers go on to
/// use it for descriptor-keyed cleanup.
pub(super) fn emit_tls_session_teardown_for_handle(ctx: &mut FunctionContext<'_>, handle_slot: i64) {
    let skip = ctx.next_label("tls_teardown_skip");
    let clear = ctx.next_label("tls_teardown_clear");
    let slot = handle_slot + 16;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction(&format!("ldr x0, [sp, #{}]", slot));       // load the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_tls_session");       // x0 = attached session, zero when plain
            ctx.emitter.instruction(&format!("cbz x0, {}", skip));              // nothing attached: skip close_notify
            abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_tls_close_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the published TLS close function pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", clear));             // the TLS backend is not linked into this program
            ctx.emitter.instruction("blr x9");                                  // close the TLS session and send close_notify
            ctx.emitter.label(&clear);
            ctx.emitter.instruction(&format!("ldr x0, [sp, #{}]", slot));       // reload the stream handle
            ctx.emitter.instruction("mov x1, #0");                              // detach the session from the stream
            abi::emit_call_label(ctx.emitter, "__rt_stream_set_tls_session");
            ctx.emitter.label(&skip);
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", slot)); // load the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_tls_session");       // rax = attached session, zero when plain
            ctx.emitter.instruction("test rax, rax");                           // is a TLS session attached?
            ctx.emitter.instruction(&format!("jz {}", skip));                   // nothing attached: skip close_notify
            abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_elephc_tls_close_fn", 0); // load the published TLS close function pointer
            ctx.emitter.instruction("test r9, r9");                             // is the TLS backend linked into this program?
            ctx.emitter.instruction(&format!("jz {}", clear));                  // no TLS backend: just detach
            ctx.emitter.instruction("mov rdi, rax");                            // pass the session handle to the close helper
            ctx.emitter.instruction("call r9");                                 // close the TLS session and send close_notify
            ctx.emitter.label(&clear);
            ctx.emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", slot)); // reload the stream handle
            ctx.emitter.instruction("xor esi, esi");                            // detach the session from the stream
            abi::emit_call_label(ctx.emitter, "__rt_stream_set_tls_session");
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
            ctx.emitter.instruction("cmp x0, #256");                            // the transitional zlib side table has 256 descriptor slots
            ctx.emitter.instruction(&format!("b.hs {}", skip));                 // high descriptors cannot have a table-backed zlib filter
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
            ctx.emitter.instruction("cmp rax, 256");                            // the transitional zlib side table has 256 descriptor slots
            ctx.emitter.instruction(&format!("jae {}", skip));                  // high descriptors cannot have a table-backed zlib filter
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
            ctx.emitter.instruction("cmp x0, #256");                            // the transitional bzip2 side table has 256 descriptor slots
            ctx.emitter.instruction(&format!("b.hs {}", skip));                 // high descriptors cannot have a table-backed bzip2 filter
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
            ctx.emitter.instruction("cmp rax, 256");                            // the transitional bzip2 side table has 256 descriptor slots
            ctx.emitter.instruction(&format!("jae {}", skip));                  // high descriptors cannot have a table-backed bzip2 filter
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
            ctx.emitter.instruction("cmp x0, #256");                            // the transitional iconv side table has 256 descriptor slots
            ctx.emitter.instruction(&format!("b.hs {}", skip));                 // high descriptors cannot have a table-backed iconv filter
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
            ctx.emitter.instruction("cmp rax, 256");                            // the transitional iconv side table has 256 descriptor slots
            ctx.emitter.instruction(&format!("jae {}", skip));                  // high descriptors cannot have a table-backed iconv filter
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
pub(super) fn lower_stream_socket_enable_crypto_attach_aarch64(
    ctx: &mut FunctionContext<'_>,
    done_label: &str,
) {
    crate::codegen::tls::publish_tls_function_pointers(ctx.emitter);
    let fail_label = ctx.next_label("ssec_attach_fail");
    let peer_ok = ctx.next_label("ssec_peer_ok");
    let host_default = ctx.next_label("ssec_host_default");
    let plain_attach = ctx.next_label("ssec_plain_attach");
    let do_attach = ctx.next_label("ssec_do_attach");
    ctx.emitter.instruction("sub sp, sp, #64");                                 // reserve peer-name and client-cert/key spill storage
    // `ssl.peer_name` is read through the same generic string-option helper that
    // local_cert and local_pk use, rather than a dedicated one. The dedicated copy
    // accepted only a raw string tag, but `stream_context_set_option()` stores a boxed
    // Mixed cell — so it missed every time and the SNI went out as the connection host.
    abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
    ctx.emitter.instruction("mov x1, #3");                                      // strlen("ssl")
    abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_peer_name_key_str");
    ctx.emitter.instruction("mov x3, #9");                                      // strlen("peer_name")
    ctx.emitter.instruction("add x4, sp, #0");                                  // pass peer-name out_ptr address
    ctx.emitter.instruction("add x5, sp, #8");                                  // pass peer-name out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_string_context_option");
    ctx.emitter.instruction(&format!("cbnz x0, {}", peer_ok));                  // use ssl.peer_name when the context provides it
    ctx.emitter.instruction("ldr x0, [sp, #80]");                               // reload the opaque handle for connection-host lookup
    abi::emit_call_label(ctx.emitter, "__rt_stream_get_connect_host");
    ctx.emitter.instruction(&format!("cbz x2, {}", host_default));              // fall back to localhost when no connection host is known
    ctx.emitter.instruction("str x1, [sp, #0]");                                // use the connection host as peer_name pointer
    ctx.emitter.instruction("str x2, [sp, #8]");                                // use the connection host as peer_name length
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
    // ssl.verify_peer = false selects the non-verifying attach, mirroring how
    // https:// already picks its insecure connect variant. Without this the
    // context option was ignored and a self-signed peer was always rejected.
    let insecure_attach = ctx.next_label("ssec_insecure_attach");
    let verifying_attach = ctx.next_label("ssec_verifying_attach");
    // The attach arguments are already materialized in x0-x6 at this point, and
    // the option lookup clobbers x0-x5, so they are saved across the probe.
    ctx.emitter.instruction("stp x0, x1, [sp, #-16]!");
    ctx.emitter.instruction("stp x2, x3, [sp, #-16]!");
    ctx.emitter.instruction("stp x4, x5, [sp, #-16]!");
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    // verify_peer is normally set with a PHP bool, so the int/bool reader is the
    // right one; the string reader missed it entirely and left verification on.
    ctx.emitter.instruction("str xzr, [sp, #0]");                               // out value default
    abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
    ctx.emitter.instruction("mov x1, #3");                                      // strlen("ssl")
    abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_verify_peer_key_str");
    ctx.emitter.instruction("mov x3, #11");                                     // strlen("verify_peer")
    ctx.emitter.instruction("add x4, sp, #0");                                  // out value address
    abi::emit_call_label(ctx.emitter, "__rt_get_int_context_option");
    ctx.emitter.instruction(&format!("cbz x0, {}", verifying_attach));          // option absent: keep verifying
    ctx.emitter.instruction("ldr x9, [sp, #0]");                                // verify_peer value
    ctx.emitter.instruction(&format!("cbz x9, {}", insecure_attach));           // false/0 disables verification
    ctx.emitter.label(&verifying_attach);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction("ldp x4, x5, [sp], #16");                           // restore the attach arguments
    ctx.emitter.instruction("ldp x2, x3, [sp], #16");
    ctx.emitter.instruction("ldp x0, x1, [sp], #16");
    abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_tls_attach_fd_fn");
    ctx.emitter.instruction("ldr x9, [x9]");                                    // load the default TLS attach function pointer
    ctx.emitter.instruction(&format!("b {}", do_attach));
    ctx.emitter.label(&insecure_attach);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction("ldp x4, x5, [sp], #16");                           // restore the attach arguments
    ctx.emitter.instruction("ldp x2, x3, [sp], #16");
    ctx.emitter.instruction("ldp x0, x1, [sp], #16");
    abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_tls_attach_fd_insecure_fn");
    ctx.emitter.instruction("ldr x9, [x9]");                                    // load the non-verifying attach function pointer
    ctx.emitter.label(&do_attach);
    ctx.emitter.instruction("blr x9");                                          // attach TLS to the fd and return a session handle
    ctx.emitter.instruction("ldr x10, [sp, #80]");                              // reload the opaque stream handle before releasing the spill storage
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.instruction("cmp x0, #0");                                      // negative handles indicate TLS attach failure
    ctx.emitter.instruction(&format!("b.lt {}", fail_label));                   // return false when attach failed
    ctx.emitter.instruction("mov x1, x0");                                      // the session becomes the second argument
    ctx.emitter.instruction("mov x0, x10");                                     // the stream handle becomes the first argument
    abi::emit_call_label(ctx.emitter, "__rt_stream_set_tls_session");           // publish the session on the StreamState
    ctx.emitter.instruction("mov x0, #1");                                      // return true after successful TLS attach
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the failure result
    ctx.emitter.label(&fail_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false after TLS attach failure
}

/// Emits the x86_64 TLS attach path for `stream_socket_enable_crypto(true)`.
pub(super) fn lower_stream_socket_enable_crypto_attach_x86_64(
    ctx: &mut FunctionContext<'_>,
    done_label: &str,
) {
    crate::codegen::tls::publish_tls_function_pointers(ctx.emitter);
    let fail_label = ctx.next_label("ssec_attach_fail");
    let peer_ok = ctx.next_label("ssec_peer_ok");
    let host_default = ctx.next_label("ssec_host_default");
    let plain_attach = ctx.next_label("ssec_plain_attach_x");
    let after_attach = ctx.next_label("ssec_after_attach_x");
    ctx.emitter.instruction("sub rsp, 64");                                     // reserve peer-name and client-cert/key spill storage
    // See the AArch64 counterpart: the generic string-option helper is the one that
    // understands how `stream_context_set_option()` actually stores a value.
    abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
    ctx.emitter.instruction("mov rsi, 3");                                      // strlen("ssl")
    abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_peer_name_key_str");
    ctx.emitter.instruction("mov rcx, 9");                                      // strlen("peer_name")
    ctx.emitter.instruction("lea r8, [rsp + 0]");                               // pass peer-name out_ptr address
    ctx.emitter.instruction("lea r9, [rsp + 8]");                               // pass peer-name out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_string_context_option");
    ctx.emitter.instruction("test rax, rax");                                   // did the context provide ssl.peer_name?
    ctx.emitter.instruction(&format!("jnz {}", peer_ok));                       // use ssl.peer_name when present
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 80]");                   // reload the opaque handle for connection-host lookup
    abi::emit_call_label(ctx.emitter, "__rt_stream_get_connect_host");
    ctx.emitter.instruction("test rdx, rdx");                                   // is a connection host known for this handle?
    ctx.emitter.instruction(&format!("jz {}", host_default));                   // fall back to localhost when no host is known
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");                    // use the connection host as peer_name pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                    // use the connection host as peer_name length
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
    // See the AArch64 counterpart: `ssl.verify_peer = false` selects the non-verifying attach.
    // Without this probe the option was read on one architecture and ignored on the other, so a
    // self-signed peer that AArch64 accepted was rejected on x86_64 — every TLS fixture sets it.
    let insecure_attach = ctx.next_label("ssec_insecure_attach_x");
    let verifying_attach = ctx.next_label("ssec_verifying_attach_x");
    let do_attach = ctx.next_label("ssec_do_attach_x");
    // The option lookup takes rdi/rsi/rdx itself, so the attach arguments are saved across it.
    abi::emit_push_reg_pair(ctx.emitter, "rdi", "rsi");
    abi::emit_push_reg(ctx.emitter, "rdx");
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], 0");                      // out value default
    abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
    ctx.emitter.instruction("mov rsi, 3");                                      // strlen("ssl")
    abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_verify_peer_key_str");
    ctx.emitter.instruction("mov rcx, 11");                                     // strlen("verify_peer")
    ctx.emitter.instruction("lea r8, [rsp + 0]");                               // out value address
    abi::emit_call_label(ctx.emitter, "__rt_get_int_context_option");
    ctx.emitter.instruction("test rax, rax");                                   // did the context carry verify_peer?
    ctx.emitter.instruction(&format!("jz {}", verifying_attach));               // option absent: keep verifying
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 0]");                    // the verify_peer value
    ctx.emitter.instruction("test r10, r10");
    ctx.emitter.instruction(&format!("jz {}", insecure_attach));                // false/0 disables verification
    ctx.emitter.label(&verifying_attach);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_pop_reg(ctx.emitter, "rdx");                                      // restore the attach arguments
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_elephc_tls_attach_fd_fn", 0); // load the default TLS attach function pointer
    ctx.emitter.instruction(&format!("jmp {}", do_attach));
    ctx.emitter.label(&insecure_attach);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_pop_reg(ctx.emitter, "rdx");                                      // restore the attach arguments
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        "r9",
        "_elephc_tls_attach_fd_insecure_fn",
        0,
    );                                                                          // load the non-verifying attach function pointer
    ctx.emitter.label(&do_attach);
    ctx.emitter.instruction("call r9");                                         // attach TLS and return a session handle
    ctx.emitter.label(&after_attach);
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 80]");                   // reload the opaque stream handle before releasing the spill storage
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.instruction("cmp rax, 0");                                      // negative handles indicate TLS attach failure
    ctx.emitter.instruction(&format!("jl {}", fail_label));                     // return false when attach failed
    ctx.emitter.instruction("mov rsi, rax");                                    // the session becomes the second argument
    ctx.emitter.instruction("mov rdi, r10");                                    // the stream handle becomes the first argument
    abi::emit_call_label(ctx.emitter, "__rt_stream_set_tls_session");           // publish the session on the StreamState
    ctx.emitter.instruction("mov eax, 1");                                      // return true after successful TLS attach
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the failure result
    ctx.emitter.label(&fail_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false after TLS attach failure
}

