//! Purpose:
//! Lowers post-eval symbol, constant, and static-property queries.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - The persistent-context guard remains authoritative for fallback dispatch.

use super::*;

/// Returns true when the current function owns an eval context local.
pub(in crate::codegen::lower_inst::builtins) fn has_eval_context(ctx: &FunctionContext<'_>) -> bool {
    eval_context_slot(ctx).is_ok()
}

/// Lowers a post-eval dynamic function existence probe to the eval bridge ABI.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_function_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let function_name = ctx.function_name_data(expect_data(inst)?)?.to_string();
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    let (name_label, name_len) = ctx.data.add_string(function_name.as_bytes());
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    emit_loaded_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_function_exists",
        &[PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int],
    );
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    box_eval_bool_result_if_mixed(ctx, inst);
    store_if_result(ctx, inst)
}

/// Lowers a post-eval dynamic class existence probe to the eval bridge ABI.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_class_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (name_label, name_len) = ctx.intern_class_name_data(expect_data(inst)?)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    emit_loaded_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_dynamic_class_exists",
        &[PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int],
    );
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    box_eval_bool_result_if_mixed(ctx, inst);
    store_if_result(ctx, inst)
}

/// Lowers a post-eval dynamic constant existence probe to the eval bridge ABI.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_constant_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let constant_name = ctx.global_name_data(expect_data(inst)?)?.to_string();
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    let (name_label, name_len) = ctx.data.add_string(constant_name.as_bytes());
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    emit_loaded_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_constant_exists",
        &[PhpType::Pointer(None), PhpType::Pointer(None), PhpType::Int],
    );
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    box_eval_bool_result_if_mixed(ctx, inst);
    store_if_result(ctx, inst)
}

/// Lowers a post-eval dynamic constant fetch to the eval bridge ABI.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_constant_fetch(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let constant_name = ctx.global_name_data(expect_data(inst)?)?.to_string();
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    let (name_label, name_len) = ctx.data.add_string(constant_name.as_bytes());
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    emit_loaded_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_constant_fetch",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
        ],
    );
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers a post-eval dynamic class-like constant fetch to the eval bridge ABI.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_class_constant_fetch(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    constant_name: &str,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    stage_eval_native_context(ctx);
    stage_eval_native_string(ctx, class_name);
    stage_eval_native_string(ctx, constant_name);
    stage_eval_native_stack_address(ctx, 0);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_class_constant_fetch",
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
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers a post-eval dynamic static-property read to the eval bridge ABI.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_static_property_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    property_name: &str,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    stage_eval_native_context(ctx);
    stage_eval_native_string(ctx, class_name);
    stage_eval_native_string(ctx, property_name);
    stage_eval_native_stack_address(ctx, 0);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_static_property_get",
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
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers a post-eval dynamic static-property write to the eval bridge ABI.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_static_property_set(
    ctx: &mut FunctionContext<'_>,
    _inst: &Instruction,
    value: ValueId,
    class_name: &str,
    property_name: &str,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_eval_mixed_operand_at(ctx, value, EVAL_TEMP_CELL_OFFSET)?;
    ensure_eval_context(ctx)?;
    let target = format!("{}::{}", class_name, property_name);
    stage_eval_native_context(ctx);
    stage_eval_native_string(ctx, &target);
    stage_eval_native_stack_word(ctx, EVAL_TEMP_CELL_OFFSET);
    stage_eval_native_stack_address(ctx, 0);
    emit_eval_native_c_abi_call(
        ctx,
        "__elephc_eval_static_property_set",
        &[
            PhpType::Pointer(None),
            PhpType::Pointer(None),
            PhpType::Int,
            PhpType::Pointer(None),
            PhpType::Pointer(None),
        ],
    );
    emit_eval_status_check(ctx);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    Ok(())
}
