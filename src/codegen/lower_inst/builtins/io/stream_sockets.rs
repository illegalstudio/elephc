//! Purpose:
//! Stream socket creation, connection, crypto, and datagram calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `stream_socket_server(address)` and boxes `resource|false`.
pub(crate) fn lower_stream_socket_server(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "stream_socket_server", 1, 5)?;
    let address = expect_operand(inst, 0)?;
    load_string_to_result(ctx, address, "stream_socket_server address")?;
    let flags = inst.operands.get(3).copied();
    let context = inst.operands.get(4).copied();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve the temporary frame used by the stream operation
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve stream-socket arguments across TLS setup
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve stream-socket arguments across TLS setup
            if let Some(flags) = flags {
                ctx.load_value_to_result(flags)?;
                ctx.emitter.instruction("str x0, [sp, #16]");                   // preserve stream-socket arguments across TLS setup
            } else {
                ctx.emitter.instruction("mov x9, #12");                         // prepare stream-socket or TLS attach arguments
                ctx.emitter.instruction("str x9, [sp, #16]");                   // preserve stream-socket arguments across TLS setup
            }
            if let Some(context) = context {
                ctx.load_value_to_result(context)?;
                ctx.emitter.instruction("str x0, [sp, #24]");                   // preserve stream-socket arguments across TLS setup
            } else {
                ctx.emitter.instruction("str xzr, [sp, #24]");                  // preserve stream-socket arguments across TLS setup
            }
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // restore stream-socket arguments after TLS setup
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // restore stream-socket arguments after TLS setup
            ctx.emitter.instruction("ldr x2, [sp, #16]");                       // restore stream-socket arguments after TLS setup
            ctx.emitter.instruction("ldr x3, [sp, #24]");                       // restore stream-socket arguments after TLS setup
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                             // reserve the temporary frame used by the stream operation
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // prepare stream-socket or TLS attach arguments
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // prepare stream-socket or TLS attach arguments
            if let Some(flags) = flags {
                ctx.load_value_to_result(flags)?;
                ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");       // prepare stream-socket or TLS attach arguments
            } else {
                ctx.emitter.instruction("mov QWORD PTR [rsp + 16], 12");        // prepare stream-socket or TLS attach arguments
            }
            if let Some(context) = context {
                ctx.load_value_to_result(context)?;
                ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rax");       // prepare stream-socket or TLS attach arguments
            } else {
                ctx.emitter.instruction("mov QWORD PTR [rsp + 24], 0");         // prepare stream-socket or TLS attach arguments
            }
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // restore stream-socket arguments after TLS setup
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // restore stream-socket arguments after TLS setup
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");           // prepare stream-socket or TLS attach arguments
            ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 24]");           // prepare stream-socket or TLS attach arguments
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_server");
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("add sp, sp, #32"),            // release the temporary frame before returning
        Arch::X86_64 => ctx.emitter.instruction("add rsp, 32"),                 // release the temporary frame before returning
    }
    store_stream_socket_server_error_outputs(ctx, inst)?;
    box_stream_fd_or_false_result(ctx, "stream_socket_server");
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_client(address)` and records the connected host for TLS defaults.
pub(crate) fn lower_stream_socket_client(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_socket_client", 1)?;
    let address = expect_operand(inst, 0)?;
    load_string_to_result(ctx, address, "stream_socket_client address")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve scratch storage for the original address string
            ctx.emitter.instruction("str x1, [sp, #0]");                        // save the address pointer across connect
            ctx.emitter.instruction("str x2, [sp, #8]");                        // save the address byte length across connect
            ctx.emitter.instruction("mov x0, x1");                              // pass the socket address pointer as the first runtime argument
            ctx.emitter.instruction("mov x1, x2");                              // pass the socket address byte length as the second runtime argument
            abi::emit_call_label(ctx.emitter, "__rt_stream_socket_client");
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // reload the address pointer for host stashing
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // reload the address byte length for host stashing
            abi::emit_call_label(ctx.emitter, "__rt_stash_connect_host");
            if address_may_select_tls(ctx, address) {
                emit_stream_client_crypto_on_connect_aarch64(ctx);
            }
            ctx.emitter.instruction("add sp, sp, #16");                         // release the address scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve scratch storage for the original address string
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // save the address pointer across connect
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // save the address byte length across connect
            ctx.emitter.instruction("mov rdi, rax");                            // pass the socket address pointer as the first runtime argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the socket address byte length as the second runtime argument
            abi::emit_call_label(ctx.emitter, "__rt_stream_socket_client");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the connected fd to the host-stash helper
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");            // reload the address pointer for host stashing
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // reload the address byte length for host stashing
            abi::emit_call_label(ctx.emitter, "__rt_stash_connect_host");
            if address_may_select_tls(ctx, address) {
                emit_stream_client_crypto_on_connect_x86_64(ctx);
            }
            ctx.emitter.instruction("add rsp, 16");                             // release the address scratch storage
        }
    }
    box_stream_fd_or_false_result(ctx, "stream_socket_client");
    store_if_result(ctx, inst)
}

/// Returns the compile-time string a value holds, when it comes from a `ConstStr`.
///
/// The lowering uses the same predicate as the builtin requirements resolver so
/// a literal plaintext address neither emits a handshake nor links the TLS bridge.
fn const_string_operand<'m>(ctx: &FunctionContext<'m>, value: ValueId) -> Option<&'m str> {
    let instruction = ctx
        .function
        .instructions
        .iter()
        .find(|instruction| instruction.result == Some(value))?;
    if instruction.op != crate::ir::Op::ConstStr {
        return None;
    }
    let crate::ir::Immediate::Data(data) = instruction.immediate.as_ref()? else {
        return None;
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Reports whether an address operand may select one of PHP's crypto transports.
///
/// Dynamic addresses need the runtime scheme probe because their transport is not
/// known until execution. Literal addresses can avoid both it and the TLS bridge.
fn address_may_select_tls(ctx: &FunctionContext<'_>, address: ValueId) -> bool {
    match const_string_operand(ctx, address) {
        Some(literal) => crate::builtins::address_selects_tls_transport(literal),
        None => true,
    }
}

/// PHP's `default_socket_timeout` default, in seconds.
const DEFAULT_SOCKET_TIMEOUT_SECONDS: u32 = 60;

/// Applies the timeout PHP gives crypto transports before their connect handshake.
///
/// The descriptor sits in the attach frame's first word. The helper may fail for
/// non-socket resources; attach then reports its own failure without a downgrade.
fn emit_crypto_handshake_deadline(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // load the descriptor staged for TLS attachment
            ctx.emitter.instruction(&format!("mov x1, #{}", DEFAULT_SOCKET_TIMEOUT_SECONDS)); // apply PHP's default_socket_timeout in seconds
            ctx.emitter.instruction("mov x2, #0");                              // request no sub-second timeout component
            abi::emit_call_label(ctx.emitter, "__rt_stream_set_timeout");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // load the descriptor staged for TLS attachment
            ctx.emitter.instruction(&format!("mov rsi, {}", DEFAULT_SOCKET_TIMEOUT_SECONDS)); // apply PHP's default_socket_timeout in seconds
            ctx.emitter.instruction("mov rdx, 0");                              // request no sub-second timeout component
            abi::emit_call_label(ctx.emitter, "__rt_stream_set_timeout");
        }
    }
}

/// Emits the AArch64 handshake-on-connect path for crypto socket transports.
///
/// php-src enables crypto while creating `ssl://`, `sslv3://`, `tls://`, and
/// `tlsv1.*://` streams. A failed or incomplete blocking handshake must therefore
/// close the descriptor and return false rather than expose plaintext I/O.
fn emit_stream_client_crypto_on_connect_aarch64(ctx: &mut FunctionContext<'_>) {
    let plain = ctx.next_label("ssc_plain");
    let restore = ctx.next_label("ssc_restore_fd");
    let attached = ctx.next_label("ssc_attached");
    let failed = ctx.next_label("ssc_tls_failed");
    ctx.emitter.instruction("cmp x0, #0");                                      // did the TCP connection itself fail?
    ctx.emitter.instruction(&format!("b.lt {}", plain));                        // no descriptor exists to negotiate on failure
    ctx.emitter.instruction("mov x3, x0");                                      // retain the connected descriptor while probing its scheme
    ctx.emitter.instruction("ldr x0, [sp, #0]");                                // pass the original address pointer to the scheme helper
    ctx.emitter.instruction("ldr x1, [sp, #8]");                                // pass the original address byte length to the scheme helper
    ctx.emitter.instruction("str x3, [sp, #0]");                                // replace the scratch address with the descriptor for attachment
    abi::emit_call_label(ctx.emitter, "__rt_addr_tls_crypto_method");
    ctx.emitter.instruction(&format!("cbz x0, {}", restore));                   // leave plaintext transports unmodified
    ctx.emitter.instruction("mov x4, x0");                                      // retain the transport-selected PHP crypto method
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction("ldr x9, [sp, #32]");                               // reload the descriptor from the caller scratch frame
    ctx.emitter.instruction("str x9, [sp, #0]");                                // use the descriptor slot expected by crypto attachment
    ctx.emitter.instruction("str x4, [sp, #8]");                                // preserve the transport's explicit crypto method
    ctx.emitter.instruction("mov x9, #1");                                      // mark the transport crypto method explicitly supplied
    ctx.emitter.instruction("str x9, [sp, #16]");                               // preserve explicit zero separately from omission
    ctx.emitter.instruction("str xzr, [sp, #24]");                              // do not resume another stream's TLS session
    emit_crypto_handshake_deadline(ctx);
    lower_stream_socket_enable_crypto_attach_aarch64(ctx, &attached);
    ctx.emitter.label(&attached);
    ctx.emitter.instruction("cmp x0, #0");                                      // only a completed handshake can expose this stream
    ctx.emitter.instruction(&format!("b.le {}", failed));                       // fail closed on pending or terminal TLS status
    ctx.emitter.label(&restore);
    ctx.emitter.instruction("ldr x0, [sp, #0]");                                // return the staged descriptor rather than probe metadata
    ctx.emitter.instruction(&format!("b {}", plain));                           // join the common return path
    ctx.emitter.label(&failed);
    ctx.emitter.instruction("ldr x0, [sp, #0]");                                // close the descriptor rejected by the TLS handshake
    ctx.emitter.bl_c("close");                                                  // release the failed socket through the target C ABI
    ctx.emitter.instruction("mov x0, #-1");                                     // expose PHP false through stream-resource boxing
    ctx.emitter.label(&plain);
}

/// Emits the x86_64 handshake-on-connect path for crypto socket transports.
fn emit_stream_client_crypto_on_connect_x86_64(ctx: &mut FunctionContext<'_>) {
    let plain = ctx.next_label("ssc_plain_x");
    let restore = ctx.next_label("ssc_restore_fd_x");
    let attached = ctx.next_label("ssc_attached_x");
    let failed = ctx.next_label("ssc_tls_failed_x");
    ctx.emitter.instruction("cmp rax, 0");                                      // did the TCP connection itself fail?
    ctx.emitter.instruction(&format!("jl {}", plain));                          // no descriptor exists to negotiate on failure
    ctx.emitter.instruction("mov rcx, rax");                                    // retain the connected descriptor while probing its scheme
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                    // pass the original address pointer to the scheme helper
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                    // pass the original address byte length to the scheme helper
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rcx");                    // replace the scratch address with the descriptor for attachment
    abi::emit_call_label(ctx.emitter, "__rt_addr_tls_crypto_method");
    ctx.emitter.instruction("test rax, rax");                                   // did the address select a crypto transport?
    ctx.emitter.instruction(&format!("jz {}", restore));                        // leave plaintext transports unmodified
    ctx.emitter.instruction("mov r8, rax");                                     // retain the transport-selected PHP crypto method
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 32]");                    // reload the descriptor from the caller scratch frame
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], r9");                     // use the descriptor slot expected by crypto attachment
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r8");                     // preserve the transport's explicit crypto method
    ctx.emitter.instruction("mov QWORD PTR [rsp + 16], 1");                     // preserve explicit method presence separately from zero
    ctx.emitter.instruction("mov QWORD PTR [rsp + 24], 0");                     // do not resume another stream's TLS session
    emit_crypto_handshake_deadline(ctx);
    lower_stream_socket_enable_crypto_attach_x86_64(ctx, &attached);
    ctx.emitter.label(&attached);
    ctx.emitter.instruction("cmp rax, 0");                                      // only a completed handshake can expose this stream
    ctx.emitter.instruction(&format!("jle {}", failed));                        // fail closed on pending or terminal TLS status
    ctx.emitter.label(&restore);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                    // return the staged descriptor rather than probe metadata
    ctx.emitter.instruction(&format!("jmp {}", plain));                         // join the common return path
    ctx.emitter.label(&failed);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                    // pass the failed descriptor to the target close shim
    ctx.emitter.emit_call_c("close");                                           // close sockets correctly on Unix and Windows
    ctx.emitter.instruction("mov rax, -1");                                     // expose PHP false through stream-resource boxing
    ctx.emitter.label(&plain);
}

/// Lowers `stream_socket_accept(server, timeout?, peer_name?)`.
pub(crate) fn lower_stream_socket_accept(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_socket_accept", 1, 3)?;
    let server = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, server, "stream_socket_accept")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    lower_stream_socket_accept_timeout(ctx, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass timeout microseconds as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass timeout microseconds as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_accept");
    let tls_skip = ctx.next_label("ssa_tls_skip");
    let tls_failed = ctx.next_label("ssa_tls_failed");
    let tls_attach_done = ctx.next_label("ssa_tls_attach_done");
    let tls_done = ctx.next_label("ssa_tls_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_reserve_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve stream-socket arguments across TLS setup
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_accepted_stream_tls_method", 0);
            ctx.emitter.instruction("str x9, [sp, #8]");                        // preserve stream-socket arguments across TLS setup
            ctx.emitter.instruction("mov x9, #1");                              // prepare stream-socket or TLS attach arguments
            ctx.emitter.instruction("str x9, [sp, #16]");                       // preserve stream-socket arguments across TLS setup
            ctx.emitter.instruction("str xzr, [sp, #24]");                      // preserve stream-socket arguments across TLS setup
            // The method value is reloaded because the scratch register was
            // reused for the explicit-presence marker above.
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_accepted_stream_tls_method", 0);
            ctx.emitter.instruction(&format!("cbz x9, {}", tls_skip));          // skip TLS setup when no crypto method is configured
            lower_stream_socket_enable_crypto_attach_aarch64(ctx, &tls_attach_done);
            ctx.emitter.label(&tls_attach_done);
            ctx.emitter.instruction("cmp x0, #0");                              // check the value before selecting the corresponding outcome
            ctx.emitter.instruction(&format!("b.le {}", tls_failed));           // route failed TLS attachment to cleanup
            ctx.emitter.instruction(&format!("b {}", tls_done));                // continue after successful TLS attachment
            ctx.emitter.label(&tls_failed);
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // restore stream-socket arguments after TLS setup
            ctx.emitter.bl_c("close");
            ctx.emitter.instruction("mov x0, #-1");                             // report failed TLS setup to the caller
            ctx.emitter.label(&tls_skip);
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // restore stream-socket arguments after TLS setup
            ctx.emitter.label(&tls_done);
            abi::emit_release_temporary_stack(ctx.emitter, 32);
        }
        Arch::X86_64 => {
            abi::emit_reserve_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // stage the descriptor for TLS attachment
            abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_accepted_stream_tls_method", 0);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r9");             // stage the configured TLS method
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], 1");             // request a client-side TLS attachment
            ctx.emitter.instruction("mov QWORD PTR [rsp + 24], 0");             // clear the optional TLS context argument
            ctx.emitter.instruction(&format!("test r9, r9\njz {}", tls_skip));  // test the helper result before selecting its outcome
            lower_stream_socket_enable_crypto_attach_x86_64(ctx, &tls_attach_done);
            ctx.emitter.label(&tls_attach_done);
            ctx.emitter.instruction("test rax, rax");                           // check the value before selecting the corresponding outcome
            ctx.emitter.instruction(&format!("jle {}", tls_failed));            // route failed TLS attachment to cleanup
            ctx.emitter.instruction(&format!("jmp {}", tls_done));              // continue after successful TLS attachment
            ctx.emitter.label(&tls_failed);
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // restore stream-socket arguments after TLS setup
            ctx.emitter.emit_call_c("close");
            ctx.emitter.instruction("mov rax, -1");                             // report failed TLS setup to the caller
            ctx.emitter.label(&tls_skip);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore stream-socket arguments after TLS setup
            ctx.emitter.label(&tls_done);
            abi::emit_release_temporary_stack(ctx.emitter, 32);
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_accepted_stream_context");
            ctx.emitter.instruction("str xzr, [x9]");                           // preserve stream-socket arguments across TLS setup
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_accepted_stream_context");
            ctx.emitter.instruction("mov QWORD PTR [r9], 0");                   // prepare stream-socket or TLS attach arguments
        }
    }
    box_stream_fd_or_false_result(ctx, "stream_socket_accept");
    if inst.operands.len() == 3 {
        let peer = expect_operand(inst, 2)?;
        store_accept_peer_name(ctx, peer)?;
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_pair(domain, type, protocol)` and boxes `array|false`.
pub(crate) fn lower_stream_socket_pair(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_socket_pair", 3)?;
    let domain = expect_operand(inst, 0)?;
    let socket_type = expect_operand(inst, 1)?;
    let protocol = expect_operand(inst, 2)?;
    ctx.load_value_to_result(domain)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(socket_type)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(protocol)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x2, x0");                              // pass protocol as the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdx, rax");                            // pass protocol as the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_pair");
    box_stream_socket_pair_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_get_name(socket, remote)` and boxes `string|false`.
pub(crate) fn lower_stream_socket_get_name(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_socket_get_name", 2)?;
    let socket = expect_operand(inst, 0)?;
    let remote = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, socket, "stream_socket_get_name")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(remote)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the remote flag as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the remote flag as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_get_name");
    box_owned_string_or_false_result(ctx, "stream_socket_get_name");
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_shutdown(stream, mode)`.
pub(crate) fn lower_stream_socket_shutdown(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_socket_shutdown", 2)?;
    let stream = expect_operand(inst, 0)?;
    let mode = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_socket_shutdown")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(mode)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the shutdown mode as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the shutdown mode as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_shutdown");
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_enable_crypto(stream, enable, method?, session_stream?)`.
pub(crate) fn lower_stream_socket_enable_crypto(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_socket_enable_crypto", 2, 4)?;
    let stream = expect_operand(inst, 0)?;
    let enable = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_socket_enable_crypto")?;
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve the descriptor beside TLS argument metadata
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the descriptor beside TLS argument metadata
        }
    }
    require_int_or_bool(
        ctx.load_value_to_result(enable)?.codegen_repr(),
        "stream_socket_enable_crypto enable",
    )?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str xzr, [sp, #24]");                      // default the optional crypto-method value to zero
            ctx.emitter.instruction("str xzr, [sp, #32]");                      // distinguish a missing method from explicit zero
            ctx.emitter.instruction("str xzr, [sp, #40]");                      // default the optional source TLS session to none
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 24], 0");             // default the optional crypto-method value to zero
            ctx.emitter.instruction("mov QWORD PTR [rsp + 32], 0");             // distinguish a missing method from explicit zero
            ctx.emitter.instruction("mov QWORD PTR [rsp + 40], 0");             // default the optional source TLS session to none
        }
    }
    if inst.operands.len() >= 3 {
        let method = expect_operand(inst, 2)?;
        if !is_nullish_value(ctx, method)? {
            require_int(
                ctx.load_value_to_result(method)?.codegen_repr(),
                "stream_socket_enable_crypto crypto_method",
            )?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("str x0, [sp, #24]");               // preserve the requested crypto method beside the descriptor
                    ctx.emitter.instruction("mov x9, #1");                      // mark the explicit method argument present
                    ctx.emitter.instruction("str x9, [sp, #32]");               // preserve explicit zero separately from absence
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rax");   // preserve the requested crypto method beside the descriptor
                    ctx.emitter.instruction("mov QWORD PTR [rsp + 32], 1");     // preserve explicit zero separately from absence
                }
            }
        }
    }
    if inst.operands.len() >= 4 {
        let session_stream = expect_operand(inst, 3)?;
        if !is_nullish_value(ctx, session_stream)? {
            load_stream_fd_to_result(
                ctx,
                session_stream,
                "stream_socket_enable_crypto session_stream",
            )?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_call_label(ctx.emitter, "__rt_tls_session_get");
                    ctx.emitter.instruction("str x0, [sp, #40]");               // preserve the reusable source TLS session handle
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov rdi, rax");                    // pass the source descriptor to the TLS session registry
                    abi::emit_call_label(ctx.emitter, "__rt_tls_session_get");
                    ctx.emitter.instruction("mov QWORD PTR [rsp + 40], rax");   // preserve the reusable source TLS session handle
                }
            }
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_pop_reg(ctx.emitter, "x0"),
        Arch::X86_64 => abi::emit_pop_reg(ctx.emitter, "rax"),
    }
    let enable_label = ctx.next_label("ssec_enable");
    let done_label = ctx.next_label("ssec_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", enable_label));     // enable=true enters the TLS attach path
            ctx.emitter.instruction("ldr x0, [sp]");                            // reload the stashed descriptor for TLS teardown
            emit_tls_session_teardown_for_current_fd(ctx);
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction("mov x0, #1");                              // disabling crypto succeeds even when no session exists
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the TLS attach path
            ctx.emitter.label(&enable_label);
            lower_stream_socket_enable_crypto_attach_aarch64(ctx, &done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the caller request TLS enablement?
            ctx.emitter.instruction(&format!("jnz {}", enable_label));          // enable=true enters the TLS attach path
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // reload the stashed descriptor for TLS teardown
            emit_tls_session_teardown_for_current_fd(ctx);
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction("mov eax, 1");                              // disabling crypto succeeds even when no session exists
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the TLS attach path
            ctx.emitter.label(&enable_label);
            lower_stream_socket_enable_crypto_attach_x86_64(ctx, &done_label);
        }
    }
    ctx.emitter.label(&done_label);
    box_stream_socket_enable_crypto_status(ctx);
    store_if_result(ctx, inst)
}

/// Boxes the TLS bridge's `true` / retryable `0` / terminal-error status for PHP.
fn box_stream_socket_enable_crypto_status(ctx: &mut FunctionContext<'_>) {
    let complete = ctx.next_label("ssec_status_complete");
    let progress = ctx.next_label("ssec_status_progress");
    let done = ctx.next_label("ssec_status_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // only a completed rustls handshake maps to PHP true
            ctx.emitter.instruction(&format!("b.eq {}", complete));             // preserve retryable zero before terminal error boxing
            ctx.emitter.instruction(&format!("cbz x0, {}", progress));          // retain PHP's integer-zero retry signal
            ctx.emitter.instruction("mov x0, #0");                              // normalize terminal TLS errors to PHP false
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the remaining boxing paths
            ctx.emitter.label(&progress);
            ctx.emitter.instruction("mov x0, #0");                              // preserve the integer-zero retry payload
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the completed-handshake boxing path
            ctx.emitter.label(&complete);
            ctx.emitter.instruction("mov x0, #1");                              // represent a completed TLS session as PHP true
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // only a completed rustls handshake maps to PHP true
            ctx.emitter.instruction(&format!("je {}", complete));               // preserve retryable zero before terminal error boxing
            ctx.emitter.instruction("test rax, rax");                           // is the bridge reporting nonblocking progress?
            ctx.emitter.instruction(&format!("jz {}", progress));               // retain PHP's integer-zero retry signal
            ctx.emitter.instruction("xor eax, eax");                            // normalize terminal TLS errors to PHP false
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the remaining boxing paths
            ctx.emitter.label(&progress);
            ctx.emitter.instruction("xor eax, eax");                            // preserve the integer-zero retry payload
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the completed-handshake boxing path
            ctx.emitter.label(&complete);
            ctx.emitter.instruction("mov eax, 1");                              // represent a completed TLS session as PHP true
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.label(&done);
        }
    }
}

/// Lowers `stream_socket_recvfrom(socket, length, flags?, address?)`.
pub(crate) fn lower_stream_socket_recvfrom(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_socket_recvfrom", 2, 4)?;
    let socket = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, socket, "stream_socket_recvfrom")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(
        ctx.load_value_to_result(length)?.codegen_repr(),
        "stream_socket_recvfrom length",
    )?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if inst.operands.len() >= 3 {
        let flags = expect_operand(inst, 2)?;
        ctx.load_value_to_result(flags)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x2, x0");                              // pass receive flags as the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdx, rax");                            // pass receive flags as the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_recvfrom");
    box_owned_string_or_false_result(ctx, "stream_socket_recvfrom");
    if inst.operands.len() == 4 {
        let address = expect_operand(inst, 3)?;
        store_recvfrom_address(ctx, address)?;
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_sendto(socket, data, flags?, address?)` and boxes `int|false`.
pub(crate) fn lower_stream_socket_sendto(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_socket_sendto", 2, 4)?;
    let socket = expect_operand(inst, 0)?;
    let data = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, socket, "stream_socket_sendto")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, data, "stream_socket_sendto data")?;
            abi::emit_push_reg(ctx.emitter, "x1");
            abi::emit_push_reg(ctx.emitter, "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, data, "stream_socket_sendto data")?;
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_push_reg(ctx.emitter, "rdx");
        }
    }
    if inst.operands.len() >= 3 {
        let flags = expect_operand(inst, 2)?;
        ctx.load_value_to_result(flags)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if inst.operands.len() >= 4 {
                let address = expect_operand(inst, 3)?;
                load_string_to_result(ctx, address, "stream_socket_sendto address")?;
                ctx.emitter.instruction("mov x4, x1");                          // pass the destination address pointer as the fifth runtime argument
                ctx.emitter.instruction("mov x5, x2");                          // pass the destination address length as the sixth runtime argument
            } else {
                ctx.emitter.instruction("mov x4, #0");                          // omitted destination address uses the connected peer
                ctx.emitter.instruction("mov x5, #0");                          // omitted destination address has zero byte length
            }
            abi::emit_pop_reg(ctx.emitter, "x3");
            abi::emit_pop_reg(ctx.emitter, "x2");
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            if inst.operands.len() >= 4 {
                let address = expect_operand(inst, 3)?;
                load_string_to_result(ctx, address, "stream_socket_sendto address")?;
                ctx.emitter.instruction("mov r8, rax");                         // pass the destination address pointer as the fifth runtime argument
                ctx.emitter.instruction("mov r9, rdx");                         // pass the destination address length as the sixth runtime argument
            } else {
                ctx.emitter.instruction("xor r8d, r8d");                        // omitted destination address uses the connected peer
                ctx.emitter.instruction("xor r9d, r9d");                        // omitted destination address has zero byte length
            }
            abi::emit_pop_reg(ctx.emitter, "rcx");
            abi::emit_pop_reg(ctx.emitter, "rdx");
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_sendto");
    box_negative_int_or_false_result(ctx, "stream_socket_sendto");
    store_if_result(ctx, inst)
}
