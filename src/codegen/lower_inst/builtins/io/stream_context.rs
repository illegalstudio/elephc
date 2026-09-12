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

/// Lowers `stream_context_create(options?, params?)`.
pub(crate) fn lower_stream_context_create(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_context_create", 0, 2)?;
    if let Some(options) = inst.operands.first().copied() {
        store_stream_context_options(ctx, options, true)?;
    }
    capture_stream_notification_callback(ctx, inst.operands.get(1).copied())?;
    emit_fd_result(ctx, 1);
    store_if_result(ctx, inst)
}

/// Lowers `stream_context_get_default(options?)`.
pub(crate) fn lower_stream_context_get_default(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_context_get_default", 0, 1)?;
    emit_fd_result(ctx, 0);
    store_if_result(ctx, inst)
}

/// Lowers `stream_context_set_default(options)`.
pub(crate) fn lower_stream_context_set_default(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_context_set_default", 1)?;
    emit_fd_result(ctx, 0);
    store_if_result(ctx, inst)
}

/// Lowers `stream_context_set_option(context, options)` and the four-argument form.
pub(crate) fn lower_stream_context_set_option(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_context_set_option", 2, 4)?;
    match inst.operands.len() {
        2 => {
            let options = expect_operand(inst, 1)?;
            store_stream_context_options(ctx, options, false)?;
            emit_bool_result(ctx, true);
        }
        4 => {
            lower_stream_context_set_option_4(ctx, inst)?;
        }
        _ => emit_bool_result(ctx, true),
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_context_set_params(context, params)` as an accepted parameter update.
pub(crate) fn lower_stream_context_set_params(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_context_set_params", 2)?;
    capture_stream_notification_callback(ctx, inst.operands.get(1).copied())?;
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
    let params_source = transparent_callback_source(ctx.function, params)?;
    if !value_is_static_hash_new(ctx, params_source)? {
        return Ok(None);
    }
    let mut found = None;
    for instruction in &ctx.function.instructions {
        if instruction.op != Op::HashSet || instruction.operands.len() != 3 {
            continue;
        }
        if transparent_callback_source(ctx.function, instruction.operands[0])? != params_source {
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
    let value = transparent_callback_source(ctx.function, value)?;
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
    let value = transparent_callback_source(ctx.function, value)?;
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

/// Follows identity-preserving ownership operations used to stage call operands.
fn transparent_callback_source(
    function: &crate::ir::Function,
    mut value: ValueId,
) -> Result<ValueId> {
    loop {
        let Some(value_ref) = function.value(value) else {
            return Err(CodegenIrError::missing_entry("value", value.as_raw()));
        };
        let ValueDef::Instruction { inst, .. } = value_ref.def else {
            return Ok(value);
        };
        let Some(inst) = function.instruction(inst) else {
            return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
        };
        if !matches!(inst.op, Op::Acquire | Op::Move | Op::Borrow) {
            return Ok(value);
        }
        value = expect_operand(inst, 0)?;
    }
}

#[cfg(test)]
mod producer_tests {
    use super::transparent_callback_source;
    use crate::ir::{Builder, Function, Immediate, IrType, Op, Ownership};
    use crate::types::PhpType;

    /// Stream notification discovery sees through argument-owner staging for both
    /// the params hash and the callback descriptor without changing either use operand.
    #[test]
    fn wrapped_notification_params_and_callback_keep_their_original_producers() {
        let mut function =
            Function::new("wrapped_notification".into(), IrType::Void, PhpType::Void);
        let hash_ty = PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Callable),
        };
        let (hash, wrapped_hash, callback, wrapped_callback) = {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let hash = builder
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(1)),
                    IrType::from_php(&hash_ty),
                    hash_ty.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            let wrapped_hash = wrap_argument_owner_chain(&mut builder, hash, &hash_ty);
            let callback = builder
                .emit(
                    Op::ClosureNew,
                    Vec::new(),
                    None,
                    IrType::from_php(&PhpType::Callable),
                    PhpType::Callable,
                    Ownership::Owned,
                )
                .unwrap();
            let wrapped_callback =
                wrap_argument_owner_chain(&mut builder, callback, &PhpType::Callable);
            (hash, wrapped_hash, callback, wrapped_callback)
        };

        assert_eq!(transparent_callback_source(&function, wrapped_hash).unwrap(), hash);
        assert_eq!(
            transparent_callback_source(&function, wrapped_callback).unwrap(),
            callback
        );
    }

    /// Builds the `Borrow(Acquire(original))` chain introduced by argument-owner staging.
    fn wrap_argument_owner_chain(
        builder: &mut Builder<'_>,
        source: crate::ir::ValueId,
        php_type: &PhpType,
    ) -> crate::ir::ValueId {
        let ir_type = IrType::from_php(php_type);
        let acquired = builder
            .emit(
                Op::Acquire,
                vec![source],
                None,
                ir_type,
                php_type.clone(),
                Ownership::Owned,
            )
            .unwrap();
        builder
            .emit(
                Op::Borrow,
                vec![acquired],
                None,
                ir_type,
                php_type.clone(),
                Ownership::Borrowed,
            )
            .unwrap()
    }
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

/// Lowers `stream_context_get_options(context)`.
pub(crate) fn lower_stream_context_get_options(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_context_get_options", 1)?;
    let empty_label = ctx.next_label("scgo_empty");
    let done_label = ctx.next_label("scgo_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("ldr x0, [x9]");                            // load the persisted stream-context options pointer
            ctx.emitter.instruction(&format!("cbz x0, {}", empty_label));       // allocate an empty hash when no context options exist
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the empty-hash fallback after retaining options
            ctx.emitter.label(&empty_label);
            ctx.emitter.instruction("mov x0, #1");                              // pass the empty fallback hash capacity
            ctx.emitter.instruction("mov x1, #7");                              // select Mixed values for the fallback hash
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov rax, QWORD PTR [r9]");                 // load the persisted stream-context options pointer
            ctx.emitter.instruction("test rax, rax");                           // test whether a context options pointer exists
            ctx.emitter.instruction(&format!("jz {}", empty_label));            // allocate an empty hash when no context options exist
            ctx.emitter.instruction("mov rdi, rax");                            // pass the options pointer to incref
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the empty-hash fallback after retaining options
            ctx.emitter.label(&empty_label);
            ctx.emitter.instruction("mov edi, 1");                              // pass the empty fallback hash capacity
            ctx.emitter.instruction("mov esi, 7");                              // select Mixed values for the fallback hash
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.label(&done_label);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_context_get_params(context)` to an empty associative hash.
pub(crate) fn lower_stream_context_get_params(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_context_get_params", 1)?;
    emit_empty_mixed_hash(ctx);
    store_if_result(ctx, inst)
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
