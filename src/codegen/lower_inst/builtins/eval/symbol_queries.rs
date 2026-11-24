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
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_function_exists");
    abi::emit_call_label(ctx.emitter, &symbol);
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
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_dynamic_class_exists");
    abi::emit_call_label(ctx.emitter, &symbol);
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
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_constant_exists");
    abi::emit_call_label(ctx.emitter, &symbol);
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
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_constant_fetch");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers a constant read that may come from either a compiled `define()` or `eval()`.
///
/// A compiled `define()` sets its per-name flag when it runs. Only that flag proves native
/// storage holds the value; the eval registry may instead be the only store, as when the
/// compiled call sits in a function that was never invoked.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_constant_fetch_or_global(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let constant_name = ctx.global_name_data(expect_data(inst)?)?.to_string();
    let native = expect_operand(inst, 0)?;
    let native_label = ctx.next_label("eval_constant_native");
    let done_label = ctx.next_label("eval_constant_done");
    // The compiled `define()` may sit in a function the optimizer removed, so this read
    // cannot rely on that lowering to declare the flag symbol.
    let flag_symbol = ctx
        .data
        .add_comm(crate::names::define_seen_symbol(&constant_name), 8);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &flag_symbol, 0);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &native_label);
    // -- eval registry: a runtime define() may be the only definer --
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
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_constant_fetch");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, &done_label);
    // -- native storage: the compiled define() ran and stored its value here --
    ctx.emitter.label(&native_label);
    ctx.load_value_to_result(native)?;
    let box_ty = ctx.raw_value_php_type(native)?;
    crate::codegen::lower_inst::globals_constants::box_loaded_value_as_mixed(
        ctx, native, &box_ty,
    );
    ctx.emitter.label(&done_label);
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
    load_eval_context_to_arg(ctx, 0);
    let (class_label, class_len) = ctx.data.add_string(class_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &class_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        class_len as i64,
    );
    let (constant_label, constant_len) = ctx.data.add_string(constant_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &constant_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        constant_len as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_class_constant_fetch");
    abi::emit_call_label(ctx.emitter, &symbol);
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
    load_eval_context_to_arg(ctx, 0);
    let (class_label, class_len) = ctx.data.add_string(class_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &class_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        class_len as i64,
    );
    let (property_label, property_len) = ctx.data.add_string(property_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &property_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        property_len as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_static_property_get");
    abi::emit_call_label(ctx.emitter, &symbol);
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
    load_eval_context_to_arg(ctx, 0);
    let target = format!("{}::{}", class_name, property_name);
    let (target_label, target_len) = ctx.data.add_string(target.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &target_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        target_len as i64,
    );
    let value_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_load_temporary_stack_slot(ctx.emitter, value_arg, EVAL_TEMP_CELL_OFFSET);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_static_property_set");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    Ok(())
}
