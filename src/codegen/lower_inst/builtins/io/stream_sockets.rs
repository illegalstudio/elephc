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
    super::super::ensure_arg_count(inst, "stream_socket_server", 1)?;
    let address = expect_operand(inst, 0)?;
    load_string_to_result(ctx, address, "stream_socket_server address")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the socket address pointer as the first runtime argument
            ctx.emitter.instruction("mov x1, x2");                              // pass the socket address byte length as the second runtime argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the socket address pointer as the first runtime argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the socket address byte length as the second runtime argument
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_server");
    box_stream_fd_or_false_result(ctx, "stream_socket_server");
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_client(address, &error_code?, &error_message?, timeout?, flags?,
/// context?)` and records the connected host for TLS defaults.
///
/// Only the address reaches the runtime helper. `$timeout`, `$flags` and `$context` are accepted
/// and evaluated (their operands are already-computed IR values, so any side effect has happened)
/// but not yet honoured: the helper always makes a blocking `STREAM_CLIENT_CONNECT`. The two
/// by-reference outputs ARE honoured, through the same helper `fsockopen` uses, because a program
/// reads them whatever the connection did -- `symfony/var-dumper`'s `Server\Connection::connect()`
/// is written `stream_socket_client($this->host, $errno, $errstr, 3)`.
pub(crate) fn lower_stream_socket_client(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_socket_client", 1, 6)?;
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
            ctx.emitter.instruction("add sp, sp, #16");                         // release the address scratch storage
            abi::emit_call_label(ctx.emitter, "__rt_stash_connect_host");
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
            ctx.emitter.instruction("add rsp, 16");                             // release the address scratch storage
            abi::emit_call_label(ctx.emitter, "__rt_stash_connect_host");
        }
    }
    store_socket_error_outputs(ctx, inst, 1, 2)?;
    box_stream_fd_or_false_result(ctx, "stream_socket_client");
    store_if_result(ctx, inst)
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
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int_or_bool(
        ctx.load_value_to_result(enable)?.codegen_repr(),
        "stream_socket_enable_crypto enable",
    )?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    for index in 2..inst.operands.len() {
        let operand = expect_operand(inst, index)?;
        ctx.load_value_to_result(operand)?;
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
            abi::emit_release_temporary_stack(ctx.emitter, 16);
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
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction("mov eax, 1");                              // disabling crypto succeeds even when no session exists
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the TLS attach path
            ctx.emitter.label(&enable_label);
            lower_stream_socket_enable_crypto_attach_x86_64(ctx, &done_label);
        }
    }
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
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

