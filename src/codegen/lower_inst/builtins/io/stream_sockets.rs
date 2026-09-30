//! Purpose:
//! Stream socket creation, connection, crypto, and datagram calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;
use crate::codegen_support::runtime::io::{SOCKET_WARNING_CLIENT, SOCKET_WARNING_SERVER};
use crate::types::stream_constants::STREAM_SERVER_DEFAULT_FLAGS;

/// Lowers `stream_socket_server(address)` and boxes `resource|false`.
pub(crate) fn lower_stream_socket_server(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "stream_socket_server", 1, 6)?;
    let address = expect_operand(inst, 0)?;
    // The flags travel to the runtime because only they say whether the caller wants a listening
    // socket, and PHP refuses that on a datagram transport. They are loaded first: materializing
    // the address overwrites the result register.
    if inst.operands.len() >= 4 {
        let flags = expect_operand(inst, 3)?;
        require_int(
            ctx.load_value_to_result(flags)?.codegen_repr(),
            "stream_socket_server flags",
        )?;
    } else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            STREAM_SERVER_DEFAULT_FLAGS,
        );
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_string_to_result(ctx, address, "stream_socket_server address")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the socket address pointer as the first runtime argument
            ctx.emitter.instruction("mov x1, x2");                              // pass the socket address byte length as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "x2");                               // pass the server flags as the third runtime argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the socket address pointer as the first runtime argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the socket address byte length as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "rdx");                              // pass the server flags as the third runtime argument
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_server");
    emit_socket_open_failure_warning(ctx, address, None, SOCKET_WARNING_SERVER)?;
    store_socket_error_outputs(ctx, inst, 1, 2, false)?;
    box_stream_fd_or_false_result(ctx, "stream_socket_server");
    emit_record_stream_transport_after_boxed(ctx, Some(address), 0)?;
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_client(address)` and records the connected host for TLS defaults.
pub(crate) fn lower_stream_socket_client(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "stream_socket_client", 1, 7)?;
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
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve scratch storage for the original address string
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // save the address pointer across connect
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // save the address byte length across connect
            ctx.emitter.instruction("mov rdi, rax");                            // pass the socket address pointer as the first runtime argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the socket address byte length as the second runtime argument
            abi::emit_call_label(ctx.emitter, "__rt_stream_socket_client");
        }
    }
    // php says nothing at all when the connect was asked for as PERSISTENT: measured on `php -n`
    // 8.5.6 against the four other spellings, which all warn. The `&$error_code` /
    // `&$error_message` outputs below are still written — only the diagnostic is suppressed.
    let quiet = ctx.next_label("ssc_persistent_quiet");
    if let Some(flags) = inst.operands.get(4).copied() {
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_push_reg(ctx.emitter, result_reg);                            // the descriptor-or-(-1) outlives the flag test
        require_int(
            ctx.load_value_to_result(flags)?.codegen_repr(),
            "stream_socket_client flags",
        )?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("and x9, x0, #1");                      // STREAM_CLIENT_PERSISTENT is bit 0
                abi::emit_pop_reg(ctx.emitter, "x0");
                ctx.emitter.instruction(&format!("cbnz x9, {}", quiet));
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("and rax, 1");                          // STREAM_CLIENT_PERSISTENT is bit 0
                ctx.emitter.instruction("mov r9, rax");
                abi::emit_pop_reg(ctx.emitter, "rax");
                ctx.emitter.instruction("test r9, r9");
                ctx.emitter.instruction(&format!("jnz {}", quiet));
            }
        }
    }
    emit_socket_open_failure_warning(ctx, address, None, SOCKET_WARNING_CLIENT)?;
    ctx.emitter.label(&quiet);
    store_socket_error_outputs(ctx, inst, 1, 2, true)?;
    box_stream_fd_or_false_result(ctx, "stream_socket_client");
    emit_record_stream_transport_after_boxed(ctx, Some(address), 0)?;
    emit_stash_connect_host_after_boxed_stashed(ctx);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
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
    emit_inherit_stream_transport_after_boxed(ctx, server)?;
    if inst.operands.len() == 3 {
        let peer = expect_operand(inst, 2)?;
        store_accept_peer_name(ctx, peer)?;
    } else {
        emit_release_accept_peer_stash(ctx);
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

/// php-src's verbatim `ValueError` wording for a `stream_socket_shutdown()` `$mode` outside
/// the three `STREAM_SHUT_*` constants.
const STREAM_SOCKET_SHUTDOWN_BAD_MODE_MESSAGE: &str =
    "stream_socket_shutdown(): Argument #2 ($mode) must be one of STREAM_SHUT_RD, \
     STREAM_SHUT_WR, or STREAM_SHUT_RDWR";

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
    // php-src accepts only the three `STREAM_SHUT_*` constants (0, 1, 2) and raises a
    // catchable ValueError for anything else. Every other mode used to reach the runtime
    // helper, whose failed `shutdown(2)` answered a plain `false` — indistinguishable from a
    // legal mode that the kernel refused.
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedInRange(abi::int_result_reg(ctx.emitter), 0, 2),
        STREAM_SOCKET_SHUTDOWN_BAD_MODE_MESSAGE,
    );
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
    load_open_stream_handle_to_result(ctx, stream, "stream_socket_enable_crypto")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {}
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the opaque handle to the descriptor resolver
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_fd");
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
            // The stream handle sits under the descriptor at [sp, #16]; the session is
            // reached through it, never through the descriptor.
            emit_tls_session_present_flag(ctx, 16);                             // probe before the teardown detaches the session
            abi::emit_push_reg(ctx.emitter, "x0");                              // stash the flag across the teardown
            emit_tls_session_teardown_for_handle(ctx, 32);                      // handle moved down by the stashed flag
            abi::emit_pop_reg(ctx.emitter, "x9");                               // recover the flag
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            // A stream that really had a session answers false: php-src shuts it down
            // and still falls through to `return -1`. One that never had crypto answers
            // true, through the NOTIMPL `default:` arm. See emit_tls_session_present_flag.
            ctx.emitter.instruction("cmp x9, #0");
            ctx.emitter.instruction("cset x0, eq");                             // no session → true, torn-down session → false
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the TLS attach path
            ctx.emitter.label(&enable_label);
            lower_stream_socket_enable_crypto_attach_aarch64(ctx, &done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the caller request TLS enablement?
            ctx.emitter.instruction(&format!("jnz {}", enable_label));          // enable=true enters the TLS attach path
            // See the AArch64 counterpart: the handle at [rsp + 16] carries the session.
            emit_tls_session_present_flag(ctx, 16);                             // probe before the teardown detaches the session
            abi::emit_push_reg(ctx.emitter, "rax");                             // stash the flag across the teardown
            emit_tls_session_teardown_for_handle(ctx, 32);                      // handle moved down by the stashed flag
            abi::emit_pop_reg(ctx.emitter, "r10");                              // recover the flag
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            // See the AArch64 counterpart: a torn-down session answers false, a stream
            // that never had crypto answers true.
            ctx.emitter.instruction("test r10, r10");
            ctx.emitter.instruction("sete al");                                 // no session → true, torn-down session → false
            ctx.emitter.instruction("movzx rax, al");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the TLS attach path
            ctx.emitter.label(&enable_label);
            lower_stream_socket_enable_crypto_attach_x86_64(ctx, &done_label);
        }
    }
    ctx.emitter.label(&done_label);
    // php-src declares `int|bool` here: a non-blocking socket mid-handshake answers `0`. Every
    // arm above produces a boolean, and elephc's TLS attach is synchronous, so the `0` is not
    // reachable from this runtime — but the DECLARED type admits it, so the boolean must be
    // boxed into the wider slot the contract now promises.
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
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

