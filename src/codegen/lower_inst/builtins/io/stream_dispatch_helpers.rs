//! Purpose:
//! Directory, timeout, socket output, and callback dispatch helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Dispatches a directory handle to libc/glob runtime helpers or userspace wrappers.
pub(super) fn lower_directory_handle_dispatch(
    ctx: &mut FunctionContext<'_>,
    runtime_label: &str,
    wrapper_label: &str,
    label_prefix: &str,
) {
    let wrapper = ctx.next_label(&format!("{}_wrapper", label_prefix));
    let after = ctx.next_label(&format!("{}_after", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("b.ge {}", wrapper));              // dispatch synthetic handles to the wrapper directory runtime
            abi::emit_call_label(ctx.emitter, runtime_label);
            ctx.emitter.instruction(&format!("b {}", after));                   // skip the wrapper path after the native directory call
            ctx.emitter.label(&wrapper);
            abi::emit_call_label(ctx.emitter, wrapper_label);
            ctx.emitter.label(&after);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("jge {}", wrapper));               // dispatch synthetic handles to the wrapper directory runtime
            ctx.emitter.instruction("mov rdi, rax");                            // pass the native directory descriptor to the runtime helper
            abi::emit_call_label(ctx.emitter, runtime_label);
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip the wrapper path after the native directory call
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rdi, rax");                            // pass the synthetic wrapper descriptor to the runtime helper
            abi::emit_call_label(ctx.emitter, wrapper_label);
            ctx.emitter.label(&after);
        }
    }
}

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

/// Calls the read-all `stream_get_contents` runtime helper for the loaded fd.
pub(super) fn lower_stream_get_contents_read_all(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the stream descriptor to the read-all helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_get_contents");
}

/// Materializes `stream_socket_accept` timeout as microseconds or `-1`.
pub(super) fn lower_stream_socket_accept_timeout(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let Some(timeout) = inst.operands.get(1).copied() else {
        emit_fd_result(ctx, -1);
        return Ok(());
    };
    if matches!(
        ctx.raw_value_php_type(timeout)?.codegen_repr(),
        PhpType::Void | PhpType::Never
    ) {
        emit_fd_result(ctx, -1);
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

/// Stores the local `$error_code` and `$error_message` outputs of a socket-connect builtin.
///
/// Shared by `fsockopen`/`pfsockopen` (out-parameters at operands 2 and 3) and by
/// `stream_socket_client` (operands 1 and 2), because PHP gives all three the same pair of
/// by-reference outputs with the same meaning. The indices are parameters rather than baked in
/// precisely so the two signatures can share one implementation.
///
/// Both outputs are written on EVERY outcome, which is what PHP does: `0` and the empty string
/// when the connection succeeded, the error code and its text when it did not. Writing only the
/// failure case would leave a caller's `$errno` undefined after a successful connect, and PHP
/// programs read it unconditionally.
///
/// Called with the connection result still live in the canonical result register (`x0`/`rax`),
/// which it preserves across the stores; `__rt_stash_connect_host` returns its `fd` argument
/// precisely so `stream_socket_client` can call this straight afterwards.
pub(super) fn store_socket_error_outputs(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    error_code_operand: usize,
    error_message_operand: usize,
) -> Result<()> {
    let errno_slot = if inst.operands.len() > error_code_operand {
        source_load_local_slot(ctx, expect_operand(inst, error_code_operand)?)?
    } else {
        None
    };
    let errstr_slot = if inst.operands.len() > error_message_operand {
        source_load_local_slot(ctx, expect_operand(inst, error_message_operand)?)?
    } else {
        None
    };
    if errno_slot.is_none() && errstr_slot.is_none() {
        return Ok(());
    }
    let (empty_sym, _) = ctx.data.add_string(b"");
    let (msg_sym, msg_len) = ctx.data.add_string(b"Connection refused");
    let econnrefused = ctx.emitter.platform.econnrefused();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the fsockopen connection succeeded
            ctx.emitter.instruction("mov x9, #0");                              // success error code is zero
            ctx.emitter.instruction(&format!("mov x10, #{}", econnrefused));    // failure error code is ECONNREFUSED
            ctx.emitter.instruction("csel x9, x9, x10, ge");                    // choose the error code for the connection outcome
            abi::emit_symbol_address(ctx.emitter, "x10", &msg_sym);
            abi::emit_symbol_address(ctx.emitter, "x11", &empty_sym);
            ctx.emitter.instruction("csel x10, x11, x10, ge");                  // choose the error-message pointer for the outcome
            ctx.emitter.instruction("mov x11, #0");                             // success error-message length is zero
            ctx.emitter.instruction(&format!("mov x12, #{}", msg_len));         // failure error-message byte length
            ctx.emitter.instruction("csel x11, x11, x12, ge");                  // choose the error-message length for the outcome
            if let Some(slot) = errstr_slot {
                let preserve_errno = errno_slot.is_some()
                    && ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed;
                if preserve_errno {
                    abi::emit_push_reg(ctx.emitter, "x9");
                }
                store_string_output_to_local(ctx, slot, "x10", "x11")?;
                if preserve_errno {
                    abi::emit_pop_reg(ctx.emitter, "x9");
                }
            }
            if let Some(slot) = errno_slot {
                store_int_output_to_local(ctx, slot, "x9")?;
            }
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("cmp rax, 0");                              // test whether the fsockopen connection succeeded
            ctx.emitter.instruction(&format!("mov r9, {}", econnrefused));      // failure error code is ECONNREFUSED
            ctx.emitter.instruction("mov r10, 0");                              // success error code is zero without clobbering compare flags
            ctx.emitter.instruction("cmovge r9, r10");                          // choose the error code for the connection outcome
            abi::emit_symbol_address(ctx.emitter, "r10", &msg_sym);
            abi::emit_symbol_address(ctx.emitter, "r11", &empty_sym);
            ctx.emitter.instruction("cmovge r10, r11");                         // choose the error-message pointer for the outcome
            ctx.emitter.instruction(&format!("mov r11, {}", msg_len));          // failure error-message byte length
            ctx.emitter.instruction("mov rcx, 0");                              // success error-message length is zero without clobbering compare flags
            ctx.emitter.instruction("cmovge r11, rcx");                         // choose the error-message length for the outcome
            if let Some(slot) = errstr_slot {
                let preserve_errno = errno_slot.is_some()
                    && ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed;
                if preserve_errno {
                    abi::emit_push_reg(ctx.emitter, "r9");
                }
                store_string_output_to_local(ctx, slot, "r10", "r11")?;
                if preserve_errno {
                    abi::emit_pop_reg(ctx.emitter, "r9");
                }
            }
            if let Some(slot) = errno_slot {
                store_int_output_to_local(ctx, slot, "r9")?;
            }
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    Ok(())
}

/// Stores an integer output into a local slot, boxing it when the slot is `Mixed`.
pub(super) fn store_int_output_to_local(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    value_reg: &str,
) -> Result<()> {
    let offset = ctx.local_offset(slot)?;
    if ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed {
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
pub(super) fn store_string_output_to_local(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let offset = ctx.local_offset(slot)?;
    if ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed {
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

