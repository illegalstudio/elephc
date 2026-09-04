//! Purpose:
//! Directory, timeout, socket output, and callback dispatch helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

use crate::codegen_support::runtime::io::socket_errno::SOCKET_ERRNO_SYMBOL;

/// Dispatches `stream_set_timeout` to native fd handling or wrapper `stream_set_option`.
pub(super) fn lower_stream_timeout_dispatch(ctx: &mut FunctionContext<'_>) {
    let wrapper = ctx.next_label("set_timeout_wrapper");
    let after = ctx.next_label("set_timeout_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("b.ge {}", wrapper));              // dispatch synthetic handles to stream_set_option
            abi::emit_call_label(ctx.emitter, "__rt_stream_set_timeout");
            ctx.emitter.instruction(&format!("b {}", after));                   // skip wrapper dispatch after the native fd update
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov x3, x2");                              // pass microseconds as wrapper option arg2
            ctx.emitter.instruction("mov x2, x1");                              // pass seconds as wrapper option arg1
            ctx.emitter.instruction(
                &format!("mov x1, #{}", STREAM_OPTION_READ_TIMEOUT)
            );                                                                  // select STREAM_OPTION_READ_TIMEOUT
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_set_option");
            ctx.emitter.label(&after);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rdi, r9");                             // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("jge {}", wrapper));               // dispatch synthetic handles to stream_set_option
            abi::emit_call_label(ctx.emitter, "__rt_stream_set_timeout");
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip wrapper dispatch after the native fd update
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rcx, rdx");                            // pass microseconds as wrapper option arg2
            ctx.emitter.instruction("mov rdx, rsi");                            // pass seconds as wrapper option arg1
            ctx.emitter.instruction(
                &format!("mov rsi, {}", STREAM_OPTION_READ_TIMEOUT)
            );                                                                  // select STREAM_OPTION_READ_TIMEOUT
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_set_option");
            ctx.emitter.label(&after);
        }
    }
}

/// Calls the read-all `stream_get_contents` runtime helper for the loaded handle.
pub(super) fn lower_stream_get_contents_read_all(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the opaque stream handle to the read-all helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_get_contents");
}

/// php's `default_socket_timeout`, in microseconds: the wait an OMITTED `$timeout` gets.
///
/// NOT the helper's `-1`, which means "do not select at all, just call accept()". That is
/// equivalent to waiting only while the listener is BLOCKING. php's own gh8472 sets the listener
/// non-blocking first, and there accept() returns EAGAIN the instant no connection happens to be
/// queued — so whether it succeeded depended on whether the client's connect had landed, and the
/// corpus test flipped between SAME and DIFF across runs with no code change at all.
///
/// php does not wait forever either: `stream_socket_accept()` documents its default as the
/// `default_socket_timeout` ini, 60 seconds, so that is what an omitted argument means here.
const DEFAULT_SOCKET_TIMEOUT_US: i64 = 60 * 1_000_000;

/// Materializes `stream_socket_accept` timeout as microseconds.
pub(super) fn lower_stream_socket_accept_timeout(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let Some(timeout) = inst.operands.get(1).copied() else {
        emit_fd_result(ctx, DEFAULT_SOCKET_TIMEOUT_US);
        return Ok(());
    };
    if matches!(
        ctx.raw_value_php_type(timeout)?.codegen_repr(),
        PhpType::Void | PhpType::Never
    ) {
        emit_fd_result(ctx, DEFAULT_SOCKET_TIMEOUT_US);
        return Ok(());
    }
    require_int(
        ctx.load_value_to_result(timeout)?.codegen_repr(),
        "stream_socket_accept timeout",
    )?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, #0x4240");                         // load low bits of 1_000_000 microseconds per second
            ctx.emitter.instruction("movk x9, #0xF, lsl #16");                  // complete the 1_000_000 multiplier
            ctx.emitter.instruction("mul x0, x0, x9");                          // convert timeout seconds to microseconds
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("imul rax, rax, 1000000");                  // convert timeout seconds to microseconds
        }
    }
    Ok(())
}

/// Releases the peer address `stream_socket_accept()` stashed when nobody asked for it.
///
/// The runtime helper renders the peer into owned storage on every accept, because it cannot see
/// whether the caller passed `&$peer_name`. Only the three-argument lowering reads that stash, so
/// a two-argument accept left one owned block per connection with no owner — invisible to every
/// functional test, and unbounded in a server loop.
pub(super) fn emit_release_accept_peer_stash(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");                              // the boxed result outlives the release
            abi::emit_symbol_address(ctx.emitter, "x9", "_accept_peer_ptr");
            ctx.emitter.instruction("ldr x0, [x9]");                            // the stashed peer address
            ctx.emitter.instruction("str xzr, [x9]");                           // detach before the free
            abi::emit_symbol_address(ctx.emitter, "x9", "_accept_peer_len");
            ctx.emitter.instruction("str xzr, [x9]");                           // and clear its length
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");           // release only live heap-backed storage
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");                             // the boxed result outlives the release
            abi::emit_symbol_address(ctx.emitter, "r9", "_accept_peer_ptr");
            ctx.emitter.instruction("mov rax, QWORD PTR [r9]");                 // the stashed peer address
            ctx.emitter.instruction("mov QWORD PTR [r9], 0");                   // detach before the free
            abi::emit_symbol_address(ctx.emitter, "r9", "_accept_peer_len");
            ctx.emitter.instruction("mov QWORD PTR [r9], 0");                   // and clear its length
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");           // release only live heap-backed storage
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
}

/// Stores `_accept_peer_*` into a local string slot while preserving the result.
pub(super) fn store_accept_peer_name(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let Some(slot) = source_load_local_slot(ctx, value)? else {
        return Err(CodegenIrError::unsupported(
            "stream_socket_accept peer_name output for non-local arguments",
        ));
    };
    let offset = ctx.local_offset(slot)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_accept_peer_ptr");
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the accepted peer address pointer
            abi::emit_symbol_address(ctx.emitter, "x9", "_accept_peer_len");
            ctx.emitter.instruction("ldr x11, [x9]");                           // load the accepted peer address byte length
            abi::store_at_offset(ctx.emitter, "x10", offset);
            abi::store_at_offset(ctx.emitter, "x11", offset - 8);
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_symbol_address(ctx.emitter, "r9", "_accept_peer_ptr");
            ctx.emitter.instruction("mov r10, QWORD PTR [r9]");                 // load the accepted peer address pointer
            abi::emit_symbol_address(ctx.emitter, "r9", "_accept_peer_len");
            ctx.emitter.instruction("mov r11, QWORD PTR [r9]");                 // load the accepted peer address byte length
            abi::store_at_offset(ctx.emitter, "r10", offset);
            abi::store_at_offset(ctx.emitter, "r11", offset - 8);
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    Ok(())
}

/// Stores `stream_socket_recvfrom`'s sender address into a local output slot.
pub(super) fn store_recvfrom_address(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let Some(slot) = source_load_local_slot(ctx, value)? else {
        return Err(CodegenIrError::unsupported(
            "stream_socket_recvfrom address output for non-local arguments",
        ));
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_recvfrom_addr_ptr");
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the stashed sender-address pointer
            abi::emit_symbol_address(ctx.emitter, "x9", "_recvfrom_addr_len");
            ctx.emitter.instruction("ldr x11, [x9]");                           // load the stashed sender-address byte length
            store_string_output_to_local(ctx, slot, "x10", "x11")?;
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_symbol_address(ctx.emitter, "r9", "_recvfrom_addr_ptr");
            ctx.emitter.instruction("mov r10, QWORD PTR [r9]");                 // load the stashed sender-address pointer
            abi::emit_symbol_address(ctx.emitter, "r9", "_recvfrom_addr_len");
            ctx.emitter.instruction("mov r11, QWORD PTR [r9]");                 // load the stashed sender-address byte length
            store_string_output_to_local(ctx, slot, "r10", "r11")?;
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    Ok(())
}

/// Stores the `&$error_code` / `&$error_message` outputs of a socket-opening builtin.
///
/// Runs immediately after the runtime call, with the descriptor-or-`-1` still in the result
/// register. On failure the error number comes from `_socket_errno`, which the socket helpers
/// publish at the exact syscall that failed; the message is libc's `strerror` for that number,
/// which is where php-src gets its text too. A successful call reports `0` and an empty message,
/// as PHP does.
///
/// This replaced a hardcoded `ECONNREFUSED` / `"Connection refused"` pair, which was right only
/// for a refused TCP connect and silently mislabelled every timeout, permission error, and
/// unreachable host.
///
/// `report_error_number` is false for `stream_socket_server()`, which is measurably the odd one
/// out: php-src leaves its `&$error_code` at `0` for every bind and listen failure and describes
/// the failure through `&$error_message` alone. Reporting the real `errno` there would be more
/// informative and would not match PHP.
pub(super) fn store_socket_error_outputs(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    errno_arg: usize,
    errstr_arg: usize,
    report_error_number: bool,
) -> Result<()> {
    let errno_slot = if inst.operands.len() > errno_arg {
        source_load_local_slot(ctx, expect_operand(inst, errno_arg)?)?
    } else {
        None
    };
    let errstr_slot = if inst.operands.len() > errstr_arg {
        source_load_local_slot(ctx, expect_operand(inst, errstr_arg)?)?
    } else {
        None
    };
    if errno_slot.is_none() && errstr_slot.is_none() {
        return Ok(());
    }
    let done_label = ctx.next_label("socket_error_code");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x9, #0");                              // a successful call reports error code 0
            ctx.emitter.instruction("cmp x0, #0");                              // did the socket call fail?
            ctx.emitter.instruction(&format!("b.ge {}", done_label));           // it succeeded: keep error code 0
            abi::emit_symbol_address(ctx.emitter, "x10", SOCKET_ERRNO_SYMBOL);
            ctx.emitter.instruction("ldr x9, [x10]");                           // load the failure reason the helper published
            ctx.emitter.label(&done_label);
            abi::emit_push_reg(ctx.emitter, "x9");                              // preserve the error code across the message lookup
            ctx.emitter.instruction("mov x0, x9");                              // pass the error number to the message helper
            abi::emit_call_label(ctx.emitter, "__rt_socket_strerror");
            ctx.emitter.instruction("mov x10, x0");                             // hold the message pointer
            ctx.emitter.instruction("mov x11, x1");                             // hold the message byte length
            if let Some(slot) = errstr_slot {
                store_string_output_to_local(ctx, slot, "x10", "x11")?;
            }
            abi::emit_pop_reg(ctx.emitter, "x9");
            if !report_error_number {
                ctx.emitter.instruction("mov x9, #0");                          // this builtin reports its failure through the message alone
            }
            if let Some(slot) = errno_slot {
                store_int_output_to_local(ctx, slot, "x9")?;
            }
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("xor r9d, r9d");                            // a successful call reports error code 0
            ctx.emitter.instruction("cmp rax, 0");                              // did the socket call fail?
            ctx.emitter.instruction(&format!("jge {}", done_label));            // it succeeded: keep error code 0
            abi::emit_symbol_address(ctx.emitter, "r10", SOCKET_ERRNO_SYMBOL);
            ctx.emitter.instruction("mov r9, QWORD PTR [r10]");                 // load the failure reason the helper published
            ctx.emitter.label(&done_label);
            abi::emit_push_reg(ctx.emitter, "r9");                              // preserve the error code across the message lookup
            ctx.emitter.instruction("mov rdi, r9");                             // pass the error number to the message helper
            abi::emit_call_label(ctx.emitter, "__rt_socket_strerror");
            ctx.emitter.instruction("mov r10, rax");                            // hold the message pointer
            ctx.emitter.instruction("mov r11, rdx");                            // hold the message byte length
            if let Some(slot) = errstr_slot {
                store_string_output_to_local(ctx, slot, "r10", "r11")?;
            }
            abi::emit_pop_reg(ctx.emitter, "r9");
            if !report_error_number {
                ctx.emitter.instruction("xor r9d, r9d");                        // this builtin reports its failure through the message alone
            }
            if let Some(slot) = errno_slot {
                store_int_output_to_local(ctx, slot, "r9")?;
            }
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    Ok(())
}

/// Emits PHP's "Unable to connect to …" Warning when a socket-opening builtin failed.
///
/// Runs immediately after the runtime call, with the descriptor-or-`-1` still in the result
/// register. PHP prints this whether or not the caller passed `&$errno`/`&$errstr`, so it is not
/// tied to `store_socket_error_outputs`; only `@` suppresses it, which `__rt_diag_warning` handles.
///
/// `port` is `Some` only for `fsockopen()`, whose address argument is a bare host: PHP spells the
/// endpoint `host:port`, so the runtime appends the suffix. The other builtins take an address
/// that already carries its port and pass `-1` to skip it.
pub(super) fn emit_socket_open_failure_warning(
    ctx: &mut FunctionContext<'_>,
    address: ValueId,
    port: Option<ValueId>,
    kind: i64,
) -> Result<()> {
    let done_label = ctx.next_label("socket_open_warning_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // did the socket call fail?
            ctx.emitter.instruction(&format!("b.ge {}", done_label));           // a live descriptor warns about nothing
            abi::emit_push_reg(ctx.emitter, "x0");                              // the result outlives the diagnostic
            match port {
                Some(port) => {
                    ctx.load_value_to_result(port)?;                            // the port PHP appends to the host
                }
                None => ctx.emitter.instruction("mov x0, #-1"),                 // the address already carries its port
            }
            abi::emit_push_reg(ctx.emitter, "x0");
            load_string_to_result(ctx, address, "socket address")?;             // re-materialize the endpoint the caller wrote
            abi::emit_pop_reg(ctx.emitter, "x3");                               // the port argument
            ctx.emitter.instruction(&format!("mov x0, #{}", kind));             // which builtin is reporting
            abi::emit_call_label(ctx.emitter, "__rt_socket_connect_warning");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // did the socket call fail?
            ctx.emitter.instruction(&format!("jge {}", done_label));            // a live descriptor warns about nothing
            abi::emit_push_reg(ctx.emitter, "rax");                             // the result outlives the diagnostic
            match port {
                Some(port) => {
                    ctx.load_value_to_result(port)?;                            // the port PHP appends to the host
                }
                None => ctx.emitter.instruction("mov rax, -1"),                 // the address already carries its port
            }
            abi::emit_push_reg(ctx.emitter, "rax");
            load_string_to_result(ctx, address, "socket address")?;             // re-materialize the endpoint the caller wrote
            ctx.emitter.instruction("mov rsi, rax");                            // the address pointer
            abi::emit_pop_reg(ctx.emitter, "rcx");                              // the port argument
            ctx.emitter.instruction(&format!("mov rdi, {}", kind));             // which builtin is reporting
            abi::emit_call_label(ctx.emitter, "__rt_socket_connect_warning");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Stores an integer output into a local slot, boxing it when the slot is `Mixed`.
///
/// Refuses a slot that holds neither an integer nor a boxed value. The raw store below writes a
/// single machine word, so a `string` slot would keep its length half while its pointer half
/// became a small integer — a NULL dereference on the next read, with no diagnostic anywhere.
/// The checker binds each out-parameter to its declared written type, which is what normally
/// makes the slot an integer; this guard is what stops a future divergence from being silent.
pub(in crate::codegen::lower_inst::builtins) fn store_int_output_to_local(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    value_reg: &str,
) -> Result<()> {
    let offset = ctx.local_offset(slot)?;
    let slot_repr = ctx.local_php_type(slot)?.codegen_repr();
    if !matches!(slot_repr, PhpType::Int | PhpType::Bool | PhpType::Mixed) {
        return Err(CodegenIrError::unsupported(&format!(
            "by-ref integer output written into a {} slot",
            slot_repr
        )));
    }
    if slot_repr == PhpType::Mixed {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("mov x0, {}", value_reg));     // move the error code into the canonical integer result register
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("mov rax, {}", value_reg));    // move the error code into the canonical integer result register
            }
        }
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
        abi::store_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
        return Ok(());
    }
    abi::store_at_offset_scratch(ctx.emitter, value_reg, offset, "x13");
    Ok(())
}

/// Stores a string output into a local slot, boxing it when the slot is `Mixed`.
///
/// Refuses a slot that holds neither a string nor a boxed value, for the reason spelled out on
/// [`store_int_output_to_local`]: the raw store writes a pointer and a length into two adjacent
/// words, which in a scalar slot would overwrite the neighbouring local.
pub(super) fn store_string_output_to_local(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let offset = ctx.local_offset(slot)?;
    let slot_repr = ctx.local_php_type(slot)?.codegen_repr();
    if !matches!(slot_repr, PhpType::Str | PhpType::Mixed) {
        return Err(CodegenIrError::unsupported(&format!(
            "by-ref string output written into a {} slot",
            slot_repr
        )));
    }
    if slot_repr == PhpType::Mixed {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("mov x1, {}", ptr_reg));       // move the error-message pointer into the canonical string result register
                ctx.emitter.instruction(&format!("mov x2, {}", len_reg));       // move the error-message length into the canonical string result register
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("mov rax, {}", ptr_reg));      // move the error-message pointer into the canonical string result register
                ctx.emitter.instruction(&format!("mov rdx, {}", len_reg));      // move the error-message length into the canonical string result register
            }
        }
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
        abi::store_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
        return Ok(());
    }
    abi::store_at_offset_scratch(ctx.emitter, ptr_reg, offset, "x13");
    abi::store_at_offset_scratch(ctx.emitter, len_reg, offset - 8, "x13");
    Ok(())
}

