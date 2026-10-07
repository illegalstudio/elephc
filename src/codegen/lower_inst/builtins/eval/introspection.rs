//! Purpose:
//! Lowers callable, member, class-relation, and object introspection through eval.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Predicate results preserve Mixed boxing and target-aware ABI handling.

use super::*;

/// Lowers a callable-array dispatch through the eval bridge.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_callable_call_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
    arg_array: ValueId,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_mixed_operand_at(ctx, callback, EVAL_TEMP_CELL_OFFSET)?;
    store_eval_mixed_operand_at(ctx, arg_array, EVAL_CALLABLE_ARG_ARRAY_OFFSET)?;
    load_eval_context_to_arg(ctx, 0);
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, callback_arg, EVAL_TEMP_CELL_OFFSET);
    let arg_array_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg_array_arg, EVAL_CALLABLE_ARG_ARRAY_OFFSET);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    emit_loaded_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_callable_call_array",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
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

/// Lowers an `is_callable()` probe through eval dynamic callable metadata.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_is_callable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_mixed_operand_at(ctx, callback, EVAL_TEMP_CELL_OFFSET)?;
    load_eval_context_to_arg(ctx, 0);
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, callback_arg, EVAL_TEMP_CELL_OFFSET);
    emit_loaded_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_is_callable",
        &[PhpType::Pointer(None), PhpType::Pointer(None)],
    );
    retire_eval_metadata_operand_boxes(ctx, &[(callback, EVAL_TEMP_CELL_OFFSET)])?;
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    box_eval_bool_result_if_mixed(ctx, inst);
    store_if_result(ctx, inst)
}

/// Lowers member-existence introspection through eval dynamic metadata.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_member_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: ValueId,
    member: ValueId,
    name: &str,
) -> Result<()> {
    let lookup_kind = eval_member_lookup_kind(name)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_mixed_operand_at(ctx, target, EVAL_TEMP_CELL_OFFSET)?;
    store_eval_mixed_operand_at(ctx, member, EVAL_CODE_PTR_OFFSET)?;
    load_eval_context_to_arg(ctx, 0);
    let target_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, target_arg, EVAL_TEMP_CELL_OFFSET);
    let member_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, member_arg, EVAL_CODE_PTR_OFFSET);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        lookup_kind,
    );
    emit_loaded_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_member_exists",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
        ],
    );
    retire_eval_metadata_operand_boxes(ctx, &[
        (target, EVAL_TEMP_CELL_OFFSET), (member, EVAL_CODE_PTR_OFFSET),
    ])?;
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    box_eval_bool_result_if_mixed(ctx, inst);
    store_if_result(ctx, inst)
}

/// Lowers class/interface/trait relation introspection through eval dynamic metadata.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_class_relation(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: ValueId,
    name: &str,
) -> Result<()> {
    let relation_kind = eval_class_relation_kind(name)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_mixed_operand_at(ctx, target, EVAL_TEMP_CELL_OFFSET)?;
    load_eval_context_to_arg(ctx, 0);
    let target_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, target_arg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        relation_kind,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    emit_loaded_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_class_relation",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
        ],
    );
    retire_eval_metadata_operand_boxes(ctx, &[(target, EVAL_TEMP_CELL_OFFSET)])?;
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Transfers boxed class-name results or detaches native strings before retiring their bridge cell.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_object_class_name(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    name: &str,
) -> Result<()> {
    let lookup_kind = eval_class_lookup_kind(name)?;
    let non_object_label = ctx.next_label("eval_object_class_non_object");
    let done_label = ctx.next_label("eval_object_class_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_object_operand(ctx, object)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_eval_unboxed_not_object(ctx, &non_object_label);
    load_eval_context_to_arg(ctx, 0);
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        lookup_kind,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    emit_loaded_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_object_class_name",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
        ],
    );
    retire_eval_metadata_operand_boxes(ctx, &[(object, EVAL_TEMP_CELL_OFFSET)])?;
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    let boxed_result = inst.result_php_type.codegen_repr() == PhpType::Mixed;
    if !boxed_result {
        emit_owned_eval_class_name_string(ctx);
    }
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&non_object_label);
    retire_eval_metadata_operand_boxes(ctx, &[(object, EVAL_TEMP_CELL_OFFSET)])?;
    emit_eval_string_result(ctx, b"");
    if boxed_result {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    }

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Copies a bridge string payload and releases its owned cell without invalidating the copy.
fn emit_owned_eval_class_name_string(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_eval_unboxed_string_result(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        EVAL_RESULT_VALUE_CELL_OFFSET + 16,
    );
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
}

/// Lowers object/class relation predicates through the eval bridge.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_object_is_a(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    target_class: &str,
    exclude_self: bool,
) -> Result<()> {
    let false_label = ctx.next_label("eval_object_is_a_false");
    let done_label = ctx.next_label("eval_object_is_a_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_object_operand(ctx, object)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_eval_unboxed_not_object(ctx, &false_label);
    stage_eval_native_context(ctx);
    stage_eval_native_stack_word(ctx, EVAL_TEMP_CELL_OFFSET);
    stage_eval_native_string(ctx, target_class);
    stage_eval_native_int(ctx, i64::from(exclude_self));
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_object_is_a",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Int,
        ],
    );
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);

    ctx.emitter.label(&done_label);
    retire_eval_metadata_operand_boxes(ctx, &[(object, EVAL_TEMP_CELL_OFFSET)])?;
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers object/class relation predicates whose target is a runtime string or object cell.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_object_is_a_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    target: ValueId,
    exclude_self: bool,
) -> Result<()> {
    let false_label = ctx.next_label("eval_object_is_a_dynamic_false");
    let invalid_label = ctx.next_label("eval_object_is_a_dynamic_invalid");
    let done_label = ctx.next_label("eval_object_is_a_dynamic_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_mixed_operand_at(ctx, object, EVAL_TEMP_CELL_OFFSET)?;
    store_eval_mixed_operand_at(ctx, target, EVAL_CODE_PTR_OFFSET)?;
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        EVAL_CODE_PTR_OFFSET,
    );
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_validate_eval_dynamic_instanceof_target(ctx, &invalid_label);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        EVAL_TEMP_CELL_OFFSET,
    );
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_eval_unboxed_not_object(ctx, &false_label);
    load_eval_context_to_arg(ctx, 0);
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, EVAL_TEMP_CELL_OFFSET);
    let target_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, target_arg, EVAL_CODE_PTR_OFFSET);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        i64::from(exclude_self),
    );
    emit_loaded_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_object_is_a_dynamic",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
        ],
    );
    emit_branch_if_eval_c_int_negative(ctx, &invalid_label);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&invalid_label);
    retire_eval_metadata_operand_boxes(ctx, &[
        (object, EVAL_TEMP_CELL_OFFSET), (target, EVAL_CODE_PTR_OFFSET),
    ])?;
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_call_label(ctx.emitter, "__rt_instanceof_invalid_target");

    ctx.emitter.label(&done_label);
    retire_eval_metadata_operand_boxes(ctx, &[
        (object, EVAL_TEMP_CELL_OFFSET), (target, EVAL_CODE_PTR_OFFSET),
    ])?;
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}
