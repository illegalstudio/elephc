//! Purpose:
//! Filter ids, params, user filters, and boxed stream resources.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Maps runtime-supported built-in stream filter names to byte-table ids.
pub(super) fn stream_filter_id(name: &str) -> Option<u8> {
    match name {
        "string.toupper" => Some(1),
        "string.tolower" => Some(2),
        "string.rot13" => Some(3),
        "string.strip_tags" => Some(4),
        "dechunk" => Some(5),
        "convert.base64-encode" => Some(6),
        "convert.base64-decode" => Some(7),
        "convert.quoted-printable-encode" => Some(8),
        "convert.quoted-printable-decode" => Some(9),
        _ => None,
    }
}

/// Reads a compile-time integer filter parameter from the fourth builtin operand.
pub(super) fn const_int_filter_param(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    _key: &str,
    primary: bool,
    min: i64,
    max: i64,
) -> Result<Option<i64>> {
    if !primary {
        return Ok(None);
    }
    let Some(value) = inst.operands.get(3).copied() else {
        return Ok(None);
    };
    Ok(optional_const_i64_operand(ctx, value)?.map(|n| n.clamp(min, max)))
}

/// Returns a literal integer operand when the value was produced by `ConstI64`.
pub(super) fn optional_const_i64_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<i64>> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstI64 {
        return Ok(None);
    }
    match inst_ref.immediate {
        Some(Immediate::I64(value)) => Ok(Some(value)),
        _ => Err(CodegenIrError::invalid_module(
            "integer literal operand has no i64 immediate",
        )),
    }
}

/// Attaches a built-in stream filter by writing its id into per-fd direction tables.
pub(super) fn lower_builtin_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    id: u8,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    materialize_stream_filter_mode(ctx, inst)?;
    let skip_read = ctx.next_label("sf_skip_read");
    let skip_write = ctx.next_label("sf_skip_write");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x1");
            ctx.emitter.instruction("tst x0, #1");                              // test whether STREAM_FILTER_READ is enabled
            ctx.emitter.instruction(&format!("b.eq {}", skip_read));            // skip the read-filter table when the read bit is clear
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_read_filters");
            ctx.emitter.instruction(&format!("mov w10, #{}", id));              // materialize the built-in stream-filter id
            ctx.emitter.instruction("strb w10, [x9, x1]");                      // record the read filter for this descriptor
            ctx.emitter.label(&skip_read);
            ctx.emitter.instruction("tst x0, #2");                              // test whether STREAM_FILTER_WRITE is enabled
            ctx.emitter.instruction(&format!("b.eq {}", skip_write));           // skip the write-filter table when the write bit is clear
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_filters");
            ctx.emitter.instruction(&format!("mov w10, #{}", id));              // materialize the built-in stream-filter id
            ctx.emitter.instruction("strb w10, [x9, x1]");                      // record the write filter for this descriptor
            ctx.emitter.label(&skip_write);
            ctx.emitter.instruction("mov x0, x1");                              // move the descriptor into the resource payload register
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rcx");
            ctx.emitter.instruction("test rax, 1");                             // test whether STREAM_FILTER_READ is enabled
            ctx.emitter.instruction(&format!("jz {}", skip_read));              // skip the read-filter table when the read bit is clear
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_read_filters"); // read-filter table base
            ctx.emitter.instruction(
                &format!("mov BYTE PTR [r9 + rcx], {}", id)
            );                                                                  // record the read filter for this descriptor
            ctx.emitter.label(&skip_read);
            ctx.emitter.instruction("test rax, 2");                             // test whether STREAM_FILTER_WRITE is enabled
            ctx.emitter.instruction(&format!("jz {}", skip_write));             // skip the write-filter table when the write bit is clear
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_write_filters"); // write-filter table base
            ctx.emitter.instruction(
                &format!("mov BYTE PTR [r9 + rcx], {}", id)
            );                                                                  // record the write filter for this descriptor
            ctx.emitter.label(&skip_write);
            ctx.emitter.instruction("mov rax, rcx");                            // move the descriptor into the resource payload register
        }
    }
    emit_boxed_stream_resource(ctx);
    store_if_result(ctx, inst)
}

/// Materializes the stream-filter mode operand, defaulting to STREAM_FILTER_ALL.
pub(super) fn materialize_stream_filter_mode(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() >= 3 {
        let mode = expect_operand(inst, 2)?;
        require_int_or_bool(
            ctx.load_value_to_result(mode)?.codegen_repr(),
            "stream_filter_append mode",
        )?;
        return Ok(());
    }
    emit_fd_result(ctx, 3);
    Ok(())
}

/// Materializes the optional stream-filter params operand as an owned boxed
/// Mixed cell, defaulting to PHP null when the caller omitted it.
pub(super) fn materialize_stream_filter_params(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() < 4 {
        emit_null_mixed(ctx);
        return Ok(());
    }
    let params = expect_operand(inst, 3)?;
    let params_ty = ctx.value_php_type(params)?.codegen_repr();
    ctx.load_value_to_result(params)?;
    if matches!(params_ty, PhpType::Mixed | PhpType::Union(_)) {
        if !ctx.value_can_own_mixed_box_source(params)? {
            abi::emit_incref_if_refcounted(ctx.emitter, &params_ty);
        }
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &params_ty);
    }
    Ok(())
}

/// Attaches a user-defined stream filter through the runtime registry.
pub(super) fn lower_user_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    let filter = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_string_to_result(ctx, filter, "stream_filter_append filter")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            materialize_stream_filter_mode(ctx, inst)?;
            abi::emit_push_reg(ctx.emitter, "x0");
            materialize_stream_filter_params(ctx, inst)?;
            ctx.emitter.instruction("mov x4, x0");                              // pass the boxed stream-filter params to the attach helper
            abi::emit_pop_reg(ctx.emitter, "x3");
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("ldr x0, [sp]");                            // pass the saved stream descriptor without popping it yet
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            materialize_stream_filter_mode(ctx, inst)?;
            abi::emit_push_reg(ctx.emitter, "rax");
            materialize_stream_filter_params(ctx, inst)?;
            ctx.emitter.instruction("mov r8, rax");                             // pass the boxed stream-filter params to the attach helper
            abi::emit_pop_reg(ctx.emitter, "rcx");
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // pass the saved stream descriptor without popping it yet
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_filter_attach_user");
    let fail = ctx.next_label("sfau_false");
    let done = ctx.next_label("sfau_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", fail));              // unknown filter or failed onCreate returns PHP false
            ctx.emitter.instruction("ldr x0, [sp]");                            // reload the descriptor for the returned filter resource
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_boxed_stream_resource(ctx);
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the PHP false boxing path
            ctx.emitter.label(&fail);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_boxed_bool(ctx, false);
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the attach helper report success?
            ctx.emitter.instruction(&format!("jz {}", fail));                   // unknown filter or failed onCreate returns PHP false
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // reload the descriptor for the returned filter resource
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_boxed_stream_resource(ctx);
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the PHP false boxing path
            ctx.emitter.label(&fail);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_boxed_bool(ctx, false);
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Boxes the current integer result as a PHP stream-filter resource Mixed cell.
///
/// Mints a fresh resource id first: like a descriptor, a filter handle can repeat a
/// number a previous, now-released filter used, and PHP never hands the same
/// resource id out twice.
pub(super) fn emit_boxed_stream_resource(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_resource_id_mint");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // use the descriptor as the resource payload
            ctx.emitter.instruction("mov x2, #9");                              // resource subtype 9 identifies a stream filter
            ctx.emitter.instruction("mov x0, #9");                              // runtime tag 9 = resource
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // use the descriptor as the resource payload
            ctx.emitter.instruction("mov esi, 9");                              // resource subtype 9 identifies a stream filter
            ctx.emitter.instruction("mov eax, 9");                              // runtime tag 9 = resource
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
}

/// Boxes a PHP boolean Mixed cell in the current result register.
pub(super) fn emit_boxed_bool(ctx: &mut FunctionContext<'_>, value: bool) {
    emit_bool_result(ctx, value);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
}

/// Boxes a PHP null Mixed cell in the current result register.
pub(super) fn emit_null_mixed(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #0");                              // null has no payload
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor eax, eax");                            // null has no payload
        }
    }
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}
