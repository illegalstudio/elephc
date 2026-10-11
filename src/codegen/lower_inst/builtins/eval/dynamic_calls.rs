//! Purpose:
//! Lowers eval-created functions, objects, methods, and late-static members.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - All dynamic dispatch paths keep the existing scratch-frame contract.

use super::*;

/// Lowers a native positional call to a function declared by a prior `eval()` call.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_function_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let function_name = ctx.function_name_data(expect_data(inst)?)?.to_string();
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_function_call_stack_bytes(inst.operands.len());
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    ensure_eval_context(ctx)?;
    store_eval_function_call_args(ctx, inst, args_offset)?;
    stage_eval_native_context(ctx);
    stage_eval_native_string(ctx, &function_name);
    if inst.operands.is_empty() {
        stage_eval_native_int(ctx, 0);
    } else {
        stage_eval_native_stack_address(ctx, args_offset);
    }
    stage_eval_native_int(ctx, inst.operands.len() as i64);
    stage_eval_native_stack_address(ctx, 0);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_call_function",
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
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Lowers a native call to a prior eval-declared function using an argument array/hash.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_function_call_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "eval function call array", 1)?;
    let function_name = ctx.function_name_data(expect_data(inst)?)?.to_string();
    let arg_array = expect_operand(inst, 0)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    let ty = ctx.load_value_to_result(arg_array)?.codegen_repr();
    if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(ctx.emitter, &ty);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    stage_eval_native_context(ctx);
    stage_eval_native_string(ctx, &function_name);
    stage_eval_native_stack_word(ctx, EVAL_TEMP_CELL_OFFSET);
    stage_eval_native_stack_address(ctx, 0);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_call_function_array",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Pointer(None),
        ],
    );
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers native construction of a class declared by a prior eval fragment.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_object_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (name_label, name_len) = ctx.intern_class_name_data(expect_data(inst)?)?;
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_function_call_stack_bytes(inst.operands.len());
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    ensure_eval_context(ctx)?;
    store_eval_function_call_args(ctx, inst, args_offset)?;
    stage_eval_native_context(ctx);
    stage_eval_native_symbol_address(ctx, &name_label);
    stage_eval_native_int(ctx, name_len as i64);
    if inst.operands.is_empty() {
        stage_eval_native_int(ctx, 0);
    } else {
        stage_eval_native_stack_address(ctx, args_offset);
    }
    stage_eval_native_int(ctx, inst.operands.len() as i64);
    stage_eval_native_stack_address(ctx, 0);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_new_object",
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
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Lowers fallback `new $class` construction through eval dynamic metadata.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_object_new_dynamic_fallback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    miss_label: &str,
) -> Result<()> {
    let constructor_args = inst.operands.get(1..).ok_or_else(|| {
        CodegenIrError::invalid_module("eval dynamic object new missing class operand")
    })?;
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_function_call_stack_bytes(constructor_args.len());
    let eval_miss_label = ctx.next_label("eval_dynamic_new_missing_class");
    let done_label = ctx.next_label("eval_dynamic_new_done");
    let name_ptr_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let name_len_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_ptr_reg, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_len_reg, 8);
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_store_to_sp(ctx.emitter, name_ptr_reg, EVAL_CODE_PTR_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, name_len_reg, EVAL_CODE_LEN_OFFSET);
    ensure_eval_context(ctx)?;
    store_eval_function_call_operands(ctx, constructor_args, args_offset)?;
    stage_eval_native_context(ctx);
    stage_eval_native_stack_word(ctx, EVAL_CODE_PTR_OFFSET);
    stage_eval_native_stack_word_as(ctx, EVAL_CODE_LEN_OFFSET, PhpType::Int);
    if constructor_args.is_empty() {
        stage_eval_native_int(ctx, 0);
    } else {
        stage_eval_native_stack_address(ctx, args_offset);
    }
    stage_eval_native_int(ctx, constructor_args.len() as i64);
    stage_eval_native_stack_address(ctx, 0);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_try_new_object",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
        ],
    );
    emit_branch_if_eval_c_int_negative(ctx, &eval_miss_label);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&eval_miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_jump(ctx.emitter, miss_label);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a method call that may dispatch to an eval-created dynamic object.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    let arg_count = inst.operands.len().saturating_sub(1);
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_method_call_stack_bytes(arg_count);
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    ensure_eval_context(ctx)?;
    let object_ty = ctx.load_value_to_result(object)?.codegen_repr();
    if !matches!(object_ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(ctx.emitter, &object_ty);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    store_eval_method_call_arg_pack(ctx, inst, args_offset)?;
    stage_eval_native_context(ctx);
    stage_eval_native_stack_word(ctx, EVAL_TEMP_CELL_OFFSET);
    stage_eval_native_string(ctx, method_name);
    stage_eval_native_stack_address(ctx, args_offset);
    stage_eval_native_stack_address(ctx, 0);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_method_call",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Pointer(None),
        ],
    );
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Lowers a native static-method call to an eval-declared dynamic class.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_static_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    method_name: &str,
) -> Result<()> {
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_static_method_call_stack_bytes(inst.operands.len());
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    ensure_eval_context(ctx)?;
    store_eval_static_method_call_arg_pack(ctx, inst, args_offset)?;
    let target = format!("{}::{}", class_name, method_name);
    stage_eval_native_context(ctx);
    stage_eval_native_string(ctx, &target);
    stage_eval_native_stack_address(ctx, args_offset);
    stage_eval_native_stack_address(ctx, 0);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_static_method_call",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Pointer(None),
        ],
    );
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Lowers a late-static AOT-frame static method call through an active eval override.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_native_frame_static_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    frame_class: &str,
    method_name: &str,
    no_override_label: &str,
    done_label: &str,
) -> Result<()> {
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_static_method_call_stack_bytes(inst.operands.len());
    let miss_stack_label = ctx.next_label("eval_native_frame_static_method_miss");
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    emit_eval_native_frame_override_probe(ctx, frame_class, &miss_stack_label);
    store_eval_static_method_call_arg_pack(ctx, inst, args_offset)?;
    stage_eval_native_string(ctx, frame_class);
    stage_eval_native_string(ctx, method_name);
    stage_eval_native_stack_address(ctx, args_offset);
    stage_eval_native_stack_address(ctx, 0);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_native_frame_static_method_call",
        &[
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Pointer(None),
        ],
    );
    emit_branch_if_eval_c_int_negative(ctx, &miss_stack_label);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    emit_eval_result_as_type(ctx, &inst.result_php_type)?;
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, done_label);

    ctx.emitter.label(&miss_stack_label);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_jump(ctx.emitter, no_override_label);
    Ok(())
}

/// Lowers a late-static AOT-frame static-property read through an active eval override.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_native_frame_static_property_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    frame_class: &str,
    property_name: &str,
    no_override_label: &str,
    done_label: &str,
) -> Result<()> {
    let miss_stack_label = ctx.next_label("eval_native_frame_static_prop_get_miss");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    emit_eval_native_frame_override_probe(ctx, frame_class, &miss_stack_label);
    stage_eval_native_string(ctx, frame_class);
    stage_eval_native_string(ctx, property_name);
    stage_eval_native_stack_address(ctx, 0);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_native_frame_static_property_get",
        &[
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
        ],
    );
    emit_branch_if_eval_c_int_negative(ctx, &miss_stack_label);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    emit_eval_result_as_type(ctx, &inst.result_php_type)?;
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, done_label);

    ctx.emitter.label(&miss_stack_label);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, no_override_label);
    Ok(())
}

/// Lowers a late-static AOT-frame static-property write through an active eval override.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_native_frame_static_property_set(
    ctx: &mut FunctionContext<'_>,
    _inst: &Instruction,
    value: ValueId,
    frame_class: &str,
    property_name: &str,
    no_override_label: &str,
    done_label: &str,
) -> Result<()> {
    let miss_stack_label = ctx.next_label("eval_native_frame_static_prop_set_miss");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    emit_eval_native_frame_override_probe(ctx, frame_class, &miss_stack_label);
    store_eval_mixed_operand_at(ctx, value, EVAL_TEMP_CELL_OFFSET)?;
    stage_eval_native_string(ctx, frame_class);
    stage_eval_native_string(ctx, property_name);
    stage_eval_native_stack_word(ctx, EVAL_TEMP_CELL_OFFSET);
    stage_eval_native_stack_address(ctx, 0);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_native_frame_static_property_set",
        &[
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Pointer(None),
        ],
    );
    emit_branch_if_eval_c_int_negative(ctx, &miss_stack_label);
    emit_eval_status_check(ctx);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, done_label);

    ctx.emitter.label(&miss_stack_label);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, no_override_label);
    Ok(())
}
