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
    let native_ref = ctx.next_label("eval_scope_get_native_ref");
    let native_global = ctx.next_label("eval_scope_get_native_global");
    let done = ctx.next_label("eval_scope_get_done");
    branch_scope_binding(ctx, NATIVE_REF, &native_ref);
    branch_scope_binding(ctx, NATIVE_GLOBAL, &native_global);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&native_ref);
    load_native_scope_reference(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&native_global);
    load_global_to_result(ctx, &EvalSyncGlobal { name, ty: PhpType::Mixed });
    ctx.emitter.label(&done);
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
    load_eval_scope_operand_to_arg(ctx, scope, 0)?;
    emit_eval_scope_get_for_loaded_scope(ctx, &name, 0, 8);
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
    let native_ref = ctx.next_label("eval_scope_set_native_ref");
    let native_global = ctx.next_label("eval_scope_set_native_global");
    let done = ctx.next_label("eval_scope_set_done");
    branch_scope_binding(ctx, NATIVE_REF, &native_ref);
    branch_scope_binding(ctx, NATIVE_GLOBAL, &native_global);
    load_eval_scope_operand_to_arg(ctx, scope, 0)?;
    emit_eval_scope_set_for_loaded_scope(ctx, &name, flags);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&native_ref);
    store_native_scope_reference(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&native_global);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    let symbol = ir_global_symbol(&name);
    ctx.data.add_comm(symbol.clone(), 8);
    if ctx.shared.uses_shared_ref_cell(ctx.module, &name) {
        super::super::super::globals_constants::lower_store_shared_global(ctx, &symbol, &PhpType::Mixed)?;
    } else {
        abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Mixed, true);
    }
    ctx.emitter.label(&done);
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
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let out_cell_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_cell_arg, out_cell_offset);
    let out_flags_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_flags_arg, out_flags_offset);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_get");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Calls `__elephc_eval_scope_set` using an already-loaded scope handle arg.
pub(super) fn emit_eval_scope_set_for_loaded_scope(ctx: &mut FunctionContext<'_>, name: &str, flags: i64) {
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        EVAL_TEMP_CELL_OFFSET,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        flags,
    );
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_set");
    abi::emit_call_label(ctx.emitter, &symbol);
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
    let source_path = super::eval_source_path_for_function(ctx.module, ctx.function);
    let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
        &fragment,
        source_path,
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
    let Some(source_path) = super::eval_source_path_for_function(ctx.module, ctx.function) else {
        return;
    };
    load_eval_context_to_arg(ctx, 0);
    let (file_label, file_len) = ctx.data.add_string(source_path.as_bytes());
    let file_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, file_arg, &file_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        file_len as i64,
    );
    let dir = Path::new(source_path)
        .parent()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let (dir_label, dir_len) = ctx.data.add_string(dir.as_bytes());
    let dir_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_symbol_address(ctx.emitter, dir_arg, &dir_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        dir_len as i64,
    );
    let line = inst
        .span
        .and_then(|span| i64::try_from(span.source_line()).ok())
        .unwrap_or(0);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        line,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_context_set_call_site");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Writes the five-word ElephcEvalConstructionSite borrowed by the native-new ABI.
///
/// `spread_container` marks a single argument that is the array a `...$args` spread produced;
/// the bridge unpacks it, binding string keys as named arguments.
pub(super) fn emit_eval_construction_site(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    arg_count: usize,
    offset: usize,
    spread_container: bool,
) {
    let reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, reg, i64::from(spread_container));
    abi::emit_store_to_sp(ctx.emitter, reg, offset + 32);
    abi::emit_load_int_immediate(ctx.emitter, reg, arg_count as i64);
    abi::emit_store_to_sp(ctx.emitter, reg, offset);
    let source = super::eval_source_path_for_function(ctx.module, ctx.function).unwrap_or("");
    let (file_label, file_len) = ctx.data.add_string(source.as_bytes());
    abi::emit_symbol_address(ctx.emitter, reg, &file_label);
    abi::emit_store_to_sp(ctx.emitter, reg, offset + 8);
    abi::emit_load_int_immediate(ctx.emitter, reg, file_len as i64);
    abi::emit_store_to_sp(ctx.emitter, reg, offset + 16);
    abi::emit_load_int_immediate(ctx.emitter, reg, inst.span.map_or(0, |span| i64::from(span.source_line())));
    abi::emit_store_to_sp(ctx.emitter, reg, offset + 24);
}
