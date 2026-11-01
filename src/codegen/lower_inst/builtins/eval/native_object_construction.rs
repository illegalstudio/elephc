//! Purpose:
//! Materializes statically typed native objects through the shared runtime construction bridge.
//!
//! Called from:
//! - Reflection owner lowering when constructor metadata depends on runtime values.
//!
//! Key details:
//! - Positional operands cross the bridge as boxed Mixed cells.
//! - The returned Mixed wrapper is released after its object payload receives an owned reference.

use super::*;

/// Constructs a statically typed object through runtime metadata and stores its raw object payload.
///
/// The bridge borrows the constructor argument cells rather than taking them, so every box this
/// frame made is released once the constructed object has been published.
pub(in crate::codegen::lower_inst) fn lower_eval_native_object_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (name_label, name_len) = ctx.intern_class_name_data(expect_data(inst)?)?;
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_function_call_stack_bytes(inst.operands.len());
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    let boxed = store_eval_function_call_args(ctx, inst, args_offset)?;
    // A body with no eval of its own passes a null handle, which the bridge answers from the
    // request's declared classes. `new \ReflectionObject($evalObject)` written in an ordinary
    // compiled method is the case that needs it: the method never evaluates anything, yet the
    // object it reflects was declared by an include the interpreter ran.
    load_eval_context_or_null(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let args_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    if inst.operands.is_empty() {
        abi::emit_load_int_immediate(ctx.emitter, args_arg, 0);
    } else {
        abi::emit_temporary_stack_address(ctx.emitter, args_arg, args_offset);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        inst.operands.len() as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_reflection_new_object");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);

    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    crate::codegen::lower_inst::reference_arguments::emit_unbox_mixed_to_owned_refcounted_result(
        ctx,
        &inst.result_php_type.codegen_repr(),
    );
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_CODE_PTR_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_CODE_PTR_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Probes the eval registry and its autoload callbacks before native construction fails.
///
/// The fixed `ObjectNew` path can carry type-checker metadata for a class whose body is
/// intentionally materialized only by a runtime include.  In that case the eval bridge owns
/// the concrete object, while a miss must leave the caller free to retain the normal native
/// missing-class diagnostic.
///
/// The probe borrows the constructor argument cells rather than taking them, so every box this
/// frame made is released on the constructed exit and on the miss that falls back to native
/// construction alike.
pub(in crate::codegen::lower_inst) fn lower_eval_native_object_new_fallback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    miss_label: &str,
) -> Result<()> {
    let (name_label, name_len) = ctx.intern_class_name_data(expect_data(inst)?)?;
    let args_offset = EVAL_STACK_BYTES;
    let site_offset = args_offset + inst.operands.len() * 8;
    let stack_bytes = eval_function_call_stack_bytes(inst.operands.len() + 5);
    let eval_miss_label = ctx.next_label("eval_native_new_missing_class");
    let done_label = ctx.next_label("eval_native_new_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    let boxed = store_eval_function_call_args(ctx, inst, args_offset)?;
    load_eval_context_or_null(ctx)?;
    emit_eval_construction_site(ctx, inst, inst.operands.len(), site_offset, false);
    // The construction is written inside this frame's class body, which is the calling scope
    // php judges the constructor's visibility from (`BigNumber` building a `BigInteger` with a
    // protected constructor).
    let pushed_caller_class = push_native_caller_class(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let args_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    if inst.operands.is_empty() {
        abi::emit_load_int_immediate(ctx.emitter, args_arg, 0);
    } else {
        abi::emit_temporary_stack_address(ctx.emitter, args_arg, args_offset);
    }
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        site_offset,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_try_new_object_at");
    abi::emit_call_label(ctx.emitter, &symbol);
    pop_native_caller_class(ctx, pushed_caller_class);
    emit_branch_if_eval_c_int_negative(ctx, &eval_miss_label);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);

    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    crate::codegen::lower_inst::reference_arguments::emit_unbox_mixed_to_owned_refcounted_result(
        ctx,
        &inst.result_php_type.codegen_repr(),
    );
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_CODE_PTR_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_CODE_PTR_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&eval_miss_label);
    emit_release_eval_boxed_operands(ctx, &boxed);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_jump(ctx.emitter, miss_label);
    ctx.emitter.label(&done_label);
    Ok(())
}
