//! Purpose:
//! Lowers direct reads and writes against materialized eval scopes.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Static names and bridge handles retain their existing ABI layout.

use super::*;

/// Lowers an EIR eval-scope read for a static variable name.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_scope_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "eval scope get", 1)?;
    let scope = expect_operand(inst, 0)?;
    let name = eval_scope_instruction_name(ctx, inst)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    load_eval_scope_operand_to_arg(ctx, scope, 0)?;
    emit_eval_scope_get_for_loaded_scope(ctx, &name, 0, 8);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers an EIR eval-scope write for a static variable name.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_scope_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "eval scope set", 2)?;
    let scope = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let name = eval_scope_instruction_name(ctx, inst)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    let flags = if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_incref");
        EVAL_SCOPE_FLAG_OWNED
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
        scope_set_flags_for_type(&value_ty)
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    load_eval_scope_operand_to_arg(ctx, scope, 0)?;
    emit_eval_scope_set_for_loaded_scope(ctx, &name, flags);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    Ok(())
}

/// Returns the static PHP variable name attached to an eval-scope instruction.
pub(super) fn eval_scope_instruction_name(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<String> {
    let data = expect_global_name(inst)?;
    ctx.module
        .data
        .global_names
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("global name", data.as_raw()))
}

/// Loads an eval-scope handle operand into the requested ABI argument register.
pub(super) fn load_eval_scope_operand_to_arg(
    ctx: &mut FunctionContext<'_>,
    scope: ValueId,
    arg_index: usize,
) -> Result<()> {
    let arg = abi::int_arg_reg_name(ctx.emitter.target, arg_index);
    let ty = ctx.load_value_to_reg(scope, arg)?.codegen_repr();
    if ty == PhpType::Int {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "eval scope handle operand for PHP type {:?}",
        ty
    )))
}

/// Calls `__elephc_eval_scope_get` using an already-loaded scope handle arg.
pub(super) fn emit_eval_scope_get_for_loaded_scope(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    out_cell_offset: usize,
    out_flags_offset: usize,
) {
    stage_eval_native_loaded_word(ctx, 0, PhpType::Pointer(None));
    stage_eval_native_string(ctx, name);
    stage_eval_native_stack_address(ctx, out_cell_offset);
    stage_eval_native_stack_address(ctx, out_flags_offset);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_scope_get",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Pointer(None),
        ],
    );
    emit_eval_status_check(ctx);
}

/// Calls `__elephc_eval_scope_set` using an already-loaded scope handle arg.
pub(super) fn emit_eval_scope_set_for_loaded_scope(ctx: &mut FunctionContext<'_>, name: &str, flags: i64) {
    stage_eval_native_loaded_word(ctx, 0, PhpType::Pointer(None));
    stage_eval_native_string(ctx, name);
    stage_eval_native_stack_word(ctx, EVAL_TEMP_CELL_OFFSET);
    stage_eval_native_int(ctx, flags);
    stage_eval_native_stack_address(ctx, EVAL_RESULT_ERROR_OFFSET);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_scope_set",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
        ],
    );
    emit_eval_status_check(ctx);
}

/// Returns the literal fragment attached to an `EvalLiteralCall`, if this is one.
pub(super) fn eval_literal_fragment(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<Option<String>> {
    if inst.op != Op::EvalLiteralCall {
        return Ok(None);
    }
    let data = match inst.immediate {
        Some(Immediate::Data(data)) | Some(Immediate::ProfiledData { data, .. }) => data,
        _ => return Ok(None),
    };
    let fragment = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    Ok(Some(fragment.clone()))
}

/// Emits an assembly marker for literal eval fragments that still use the bridge fallback.
pub(super) fn emit_eval_literal_aot_marker(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let Some(fragment) = eval_literal_fragment(ctx, inst)? else {
        return Ok(());
    };
    let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
        &fragment,
        ctx.module.source_path.as_deref(),
        super::super::instruction_strict_php_profile(inst),
        |name, args| eval_literal_static_function_supported_by_codegen(ctx, name, args),
        |receiver, method, args| {
            eval_literal_static_method_supported_by_codegen(ctx, receiver, method, args)
        },
    );
    let reason = plan
        .fallback_reason()
        .map(crate::eval_aot::EvalAotFallbackReason::description)
        .unwrap_or("bridge fallback required");
    ctx.emitter.comment(&format!(
        "eval literal AOT fallback: {} ({} bytes), using bridge fallback",
        reason,
        fragment.len(),
    ));
    Ok(())
}

/// Updates eval context source metadata for file, directory, and call-site line magic constants.
pub(super) fn set_eval_call_site(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    let Some(source_path) = ctx.module.source_path.as_deref() else {
        return;
    };
    stage_eval_native_context(ctx);
    let (file_label, file_len) = ctx.data.add_string(source_path.as_bytes());
    abi::emit_symbol_address(ctx.emitter, abi::int_result_reg(ctx.emitter), &file_label);
    stage_eval_native_word(ctx, PhpType::Pointer(None));
    stage_eval_native_int(ctx, file_len as i64);
    let dir = Path::new(source_path)
        .parent()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let (dir_label, dir_len) = ctx.data.add_string(dir.as_bytes());
    abi::emit_symbol_address(ctx.emitter, abi::int_result_reg(ctx.emitter), &dir_label);
    stage_eval_native_word(ctx, PhpType::Pointer(None));
    stage_eval_native_int(ctx, dir_len as i64);
    let line = inst
        .span
        .and_then(|span| i64::try_from(span.line).ok())
        .unwrap_or(0);
    stage_eval_native_int(ctx, line);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_context_set_call_site",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Int,
        ],
    );
    emit_eval_status_check(ctx);
}
