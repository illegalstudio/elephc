//! Purpose:
//! Stream wrapper registration, context state, and stream content reads.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `stream_wrapper_register(protocol, class, flags?)`.
pub(crate) fn lower_stream_wrapper_register(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_wrapper_register", 2, 3)?;
    let protocol = expect_operand(inst, 0)?;
    let class = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, protocol, "stream_wrapper_register protocol")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, class, "stream_wrapper_register class")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_pop_reg_pair(ctx.emitter, "x2", "x3");
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, protocol, "stream_wrapper_register protocol")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, class, "stream_wrapper_register class")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_register");
    store_if_result(ctx, inst)
}

/// Lowers `stream_wrapper_unregister(protocol)`.
pub(crate) fn lower_stream_wrapper_unregister(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_wrapper_unregister", 1)?;
    let protocol = expect_operand(inst, 0)?;
    load_string_to_result(ctx, protocol, "stream_wrapper_unregister protocol")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the protocol pointer as the first runtime argument
            ctx.emitter.instruction("mov x1, x2");                              // pass the protocol byte length as the second runtime argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the protocol pointer as the first runtime argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the protocol byte length as the second runtime argument
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_unregister");
    store_if_result(ctx, inst)
}

/// Lowers `stream_wrapper_restore(protocol)` as a successful no-op.
pub(crate) fn lower_stream_wrapper_restore(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_wrapper_restore", 1)?;
    let protocol = expect_operand(inst, 0)?;
    load_string_to_result(ctx, protocol, "stream_wrapper_restore protocol")?;
    emit_bool_result(ctx, true);
    store_if_result(ctx, inst)
}

/// Captures a literal `notification` callable from stream context params into runtime global state.
pub(super) fn capture_stream_notification_callback(
    ctx: &mut FunctionContext<'_>,
    params: Option<ValueId>,
) -> Result<()> {
    let Some(params) = params else {
        return Ok(());
    };
    let Some(callback) = notification_callback_value(ctx, params)? else {
        clear_stream_notification_callback(ctx);
        return Ok(());
    };
    if !is_capturable_notification_callable(ctx, callback)? {
        clear_stream_notification_callback(ctx);
        return Ok(());
    }
    ctx.load_value_to_result(callback)?;
    callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    store_current_result_as_stream_notification_callback(ctx);
    Ok(())
}

/// Returns the last literal `notification` value inserted into a static params hash.
pub(super) fn notification_callback_value(
    ctx: &FunctionContext<'_>,
    params: ValueId,
) -> Result<Option<ValueId>> {
    if !value_is_static_hash_new(ctx, params)? {
        return Ok(None);
    }
    let mut found = None;
    for instruction in &ctx.function.instructions {
        if instruction.op != Op::HashSet || instruction.operands.len() != 3 {
            continue;
        }
        if instruction.operands[0] != params {
            continue;
        }
        if value_is_string_literal(ctx, instruction.operands[1], "notification")? {
            found = Some(instruction.operands[2]);
        }
    }
    Ok(found)
}

/// Returns true when `value` is produced by a literal hash allocation in this function.
pub(super) fn value_is_static_hash_new(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let Some(inst) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    Ok(inst.op == Op::HashNew)
}

/// Returns true when `value` is a constant string equal to `expected`.
pub(super) fn value_is_string_literal(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    expected: &str,
) -> Result<bool> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let Some(inst) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if inst.op != Op::ConstStr {
        return Ok(false);
    }
    let Some(Immediate::Data(data_id)) = inst.immediate else {
        return Ok(false);
    };
    let Some(value) = ctx.module.data.strings.get(data_id.as_raw() as usize) else {
        return Err(CodegenIrError::missing_entry(
            "data string",
            data_id.as_raw(),
        ));
    };
    Ok(value == expected)
}

/// Returns true for literal callables that expose the descriptor invoker slot.
pub(super) fn is_capturable_notification_callable(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let Some(inst) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    Ok(matches!(inst.op, Op::ClosureNew | Op::FirstClassCallableNew))
}

/// Stores the loaded callable descriptor into `_stream_notification_callback`.
pub(super) fn store_current_result_as_stream_notification_callback(ctx: &mut FunctionContext<'_>) {
    let addr_reg = abi::symbol_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, addr_reg, "_stream_notification_callback");
    abi::emit_store_to_address(ctx.emitter, result_reg, addr_reg, 0);
}

/// Clears `_stream_notification_callback` so later transfers do not fire stale callbacks.
pub(super) fn clear_stream_notification_callback(ctx: &mut FunctionContext<'_>) {
    let addr_reg = abi::symbol_scratch_reg(ctx.emitter);
    let zero_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, addr_reg, "_stream_notification_callback");
    abi::emit_load_int_immediate(ctx.emitter, zero_reg, 0);
    abi::emit_store_to_address(ctx.emitter, zero_reg, addr_reg, 0);
}

/// Lowers `stream_get_contents(stream, length?, offset?)` to `string|false`.
pub(crate) fn lower_stream_get_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_get_contents", 1, 3)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_get_contents")?;
    if inst.operands.len() == 1 {
        lower_stream_get_contents_read_all(ctx);
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
        return store_if_result(ctx, inst);
    }

    let read_all = ctx.next_label("sgc_read_all");
    let skip_seek = ctx.next_label("sgc_skip_seek");
    let wrap_seek = ctx.next_label("sgc_wrap_seek");
    let seek_failed = ctx.next_label("sgc_seek_failed");
    let done = ctx.next_label("sgc_done");

    emit_stream_get_contents_frame_enter(ctx);
    emit_stream_get_contents_save_fd(ctx);
    let length = expect_operand(inst, 1)?;
    require_optional_int(
        ctx.load_value_to_result(length)?.codegen_repr(),
        "stream_get_contents length",
    )?;
    emit_stream_get_contents_save_length(ctx);

    if inst.operands.len() == 3 {
        let offset = expect_operand(inst, 2)?;
        require_int(
            ctx.load_value_to_result(offset)?.codegen_repr(),
            "stream_get_contents offset",
        )?;
        lower_stream_get_contents_seek(ctx, &skip_seek, &wrap_seek, &seek_failed);
    }

    lower_stream_get_contents_bounded_or_all(ctx, &read_all, &done);
    ctx.emitter.label(&read_all);
    lower_stream_get_contents_reload_fd_and_leave_frame(ctx);
    lower_stream_get_contents_read_all(ctx);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the seek-failure false result after reading successfully
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the seek-failure false result after reading successfully
        }
    }
    ctx.emitter.label(&seek_failed);
    emit_stream_get_contents_frame_leave(ctx);
    emit_bool_result(ctx, false);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}
