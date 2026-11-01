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
///
/// The bridge borrows both cells rather than taking them, so the callback box and the argument
/// container box are still this frame's to release once the dispatch has answered.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_callable_call_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
    arg_array: ValueId,
) -> Result<()> {
    lower_eval_callable_call_array_with_context(ctx, inst, callback, arg_array, true)
}

/// Lowers a callable-array dispatch through a request-global fallback context.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_global_callable_call_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
    arg_array: ValueId,
) -> Result<()> {
    lower_eval_callable_call_array_with_context(ctx, inst, callback, arg_array, false)
}

/// Shared callable-array bridge lowering for local-context and request-global dispatch.
fn lower_eval_callable_call_array_with_context(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
    arg_array: ValueId,
    use_local_context: bool,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    if use_local_context {
        ensure_eval_context(ctx)?;
    }
    // Dynamic method syntax lowers through a callable array. Preserve the native frame's class
    // scope around that bridge call so the callback validator applies PHP protected/private
    // access rules to the original caller rather than to global scope.
    let pushed_class_scope = if use_local_context {
        push_eval_context_class_scope(ctx)?
    } else {
        false
    };
    // Without a context to forward, the frame's lexical class is still the calling scope:
    // `call_user_func_array([$this, 'privateMethod'], ...)` inside its own class was refused
    // "cannot access private method", and so was every `ReflectionMethod::invoke()` body.
    let pushed_caller_class = if use_local_context {
        false
    } else {
        push_native_caller_class(ctx)?
    };
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_mixed_operand_at(ctx, callback, EVAL_TEMP_CELL_OFFSET)?);
    boxed.extend(store_eval_mixed_operand_at(
        ctx,
        arg_array,
        EVAL_CALLABLE_ARG_ARRAY_OFFSET,
    )?);
    if use_local_context {
        load_eval_context_to_arg(ctx, 0);
    } else {
        let context_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
        abi::emit_load_int_immediate(ctx.emitter, context_arg, 0);
    }
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, callback_arg, EVAL_TEMP_CELL_OFFSET);
    let arg_array_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg_array_arg, EVAL_CALLABLE_ARG_ARRAY_OFFSET);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_callable_call_array");
    abi::emit_call_label(ctx.emitter, &symbol);
    pop_eval_context_class_scope(ctx, pushed_class_scope);
    pop_native_caller_class(ctx, pushed_caller_class);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers an `is_callable()` probe through eval dynamic callable metadata.
///
/// The probe borrows the callback cell rather than taking it, so the box this frame made is still
/// this frame's to release before the scratch frame goes away.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_is_callable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_mixed_operand_at(ctx, callback, EVAL_TEMP_CELL_OFFSET)?);
    load_eval_context_to_arg(ctx, 0);
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, callback_arg, EVAL_TEMP_CELL_OFFSET);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_is_callable");
    abi::emit_call_label(ctx.emitter, &symbol);
    retire_eval_metadata_operand_boxes(ctx, &[(callback, EVAL_TEMP_CELL_OFFSET)])?;
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    box_eval_bool_result_if_mixed(ctx, inst);
    store_if_result(ctx, inst)
}

/// Registers an AOT SPL callback in the same persistent eval context used by runtime includes.
///
/// The registry takes its own reference (`register_spl_autoload_callback_unchecked` retains before
/// it stores), so the box this frame made is still this frame's to release once registration has
/// answered — including the already-registered shortcut, which retains nothing at all.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_spl_autoload_register(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "spl_autoload_register", 1, 3)?;
    let callback = expect_operand(inst, 0)?;
    // Normalized before the eval frame is reserved: the original array rides on the temporary
    // stack until the restore below.
    let normalized = matches!(ctx.value_php_type(callback)?.codegen_repr(), PhpType::Array(element) if element.codegen_repr() != PhpType::Mixed);
    if normalized {
        crate::codegen::lower_inst::callables::normalize_typed_callable_array_to_mixed(ctx, callback)?;
    }
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_mixed_operand_at(ctx, callback, EVAL_TEMP_CELL_OFFSET)?);
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, callback_arg, EVAL_TEMP_CELL_OFFSET);
    let prepend_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    if let Some(prepend) = inst.operands.get(2).copied() {
        let prepend_ty = ctx.load_value_to_result(prepend)?.codegen_repr();
        if prepend_ty != PhpType::Bool {
            return Err(CodegenIrError::unsupported(format!(
                "spl_autoload_register() prepend lowering for PHP type {:?}",
                prepend_ty
            )));
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        if prepend_arg != result_reg {
            ctx.emitter
                .instruction(&format!("mov {}, {}", prepend_arg, result_reg));
        }
    } else {
        abi::emit_load_int_immediate(ctx.emitter, prepend_arg, 0);
    }
    // The registration ABI allocates an independent request-scoped context for AOT callers.
    // Loading `$prepend` uses the result register, so write the null context last.
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_spl_autoload");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_release_eval_boxed_operands_keeping_int_answer(ctx, &boxed);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    if normalized {
        crate::codegen::lower_inst::callables::restore_typed_callable_array_after_mixed_use(ctx, callback)?;
    }
    box_eval_bool_result_if_mixed(ctx, inst);
    store_if_result(ctx, inst)
}

/// Unregisters an AOT SPL callback from the eval context that retains it.
///
/// The mirror of [`lower_eval_spl_autoload_register`]. It used to be a stub that evaluated its
/// argument for side effects and loaded the immediate `1`: `spl_autoload_unregister()` answered
/// `true` and removed nothing, so a loader stayed installed for the life of the process.
///
/// MEASURED against php 8.5.10 on a loader registered, used through `class_exists()`, then
/// unregistered: php calls it once, elephc called it for every later lookup as well. Symfony's
/// `ClassExistenceResource` installs a THROWING loader and removes it in a `finally`, so the
/// leftover turned every subsequent `class_exists()` in the process into a throw.
///
/// The registry released its own reference, so the box this frame made is still this frame's to
/// release once the call has answered.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_spl_autoload_unregister(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "spl_autoload_unregister", 1)?;
    let callback = expect_operand(inst, 0)?;
    let normalized = matches!(ctx.value_php_type(callback)?.codegen_repr(), PhpType::Array(element) if element.codegen_repr() != PhpType::Mixed);
    if normalized {
        crate::codegen::lower_inst::callables::normalize_typed_callable_array_to_mixed(ctx, callback)?;
    }
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_mixed_operand_at(ctx, callback, EVAL_TEMP_CELL_OFFSET)?);
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, callback_arg, EVAL_TEMP_CELL_OFFSET);
    // A null context is the same request-scoped arrangement registration uses; the callback is
    // looked up across every context that holds autoload callbacks either way.
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_unregister_spl_autoload");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_release_eval_boxed_operands_keeping_int_answer(ctx, &boxed);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    if normalized {
        crate::codegen::lower_inst::callables::restore_typed_callable_array_after_mixed_use(ctx, callback)?;
    }
    box_eval_bool_result_if_mixed(ctx, inst);
    store_if_result(ctx, inst)
}

/// Lowers member-existence introspection through eval dynamic metadata.
///
/// The probe borrows the target and member cells rather than taking them, so both boxes are still
/// this frame's to release before the scratch frame goes away.
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
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_mixed_operand_at(ctx, target, EVAL_TEMP_CELL_OFFSET)?);
    boxed.extend(store_eval_mixed_operand_at(ctx, member, EVAL_CODE_PTR_OFFSET)?);
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
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_member_exists");
    abi::emit_call_label(ctx.emitter, &symbol);
    retire_eval_metadata_operand_boxes(ctx, &[
        (target, EVAL_TEMP_CELL_OFFSET), (member, EVAL_CODE_PTR_OFFSET),
    ])?;
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    box_eval_bool_result_if_mixed(ctx, inst);
    store_if_result(ctx, inst)
}

/// Emits a null-context `method_exists()` / `property_exists()` probe for an object an eval
/// context may own, branching to `matched_label` when it answers yes.
///
/// For a frame with no eval context of its own: the bridge resolves the object's owner context
/// and answers 0 for an object no eval context made, so falling through to the native class
/// tables stays correct for every compiled object.
pub(in crate::codegen::lower_inst::builtins) fn emit_eval_member_exists_owner_probe(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    member: ValueId,
    name: &str,
    matched_label: &str,
) -> Result<()> {
    let lookup_kind = eval_member_lookup_kind(name)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_mixed_operand_at(ctx, target, EVAL_TEMP_CELL_OFFSET)?);
    boxed.extend(store_eval_mixed_operand_at(ctx, member, EVAL_CODE_PTR_OFFSET)?);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    let target_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, target_arg, EVAL_TEMP_CELL_OFFSET);
    let member_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, member_arg, EVAL_CODE_PTR_OFFSET);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        lookup_kind,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_member_exists");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_release_eval_boxed_operands_keeping_int_answer(ctx, &boxed);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, matched_label);
    Ok(())
}

/// Lowers class/interface/trait relation introspection through eval dynamic metadata.
///
/// The bridge borrows the target cell rather than taking it, so the box this frame made is still
/// this frame's to release once the relation map has been published.
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
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_class_relation");
    abi::emit_call_label(ctx.emitter, &symbol);
    retire_eval_metadata_operand_boxes(ctx, &[(target, EVAL_TEMP_CELL_OFFSET)])?;
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Resolves an object's class name through its owning eval context when one exists.
///
/// The object payload pointer is expected in the architecture's integer result register
/// (`x0`/`rax`) and is restored there on the fallback path so the compiled class-name tables
/// can answer for ordinary AOT objects. A successful lookup leaves the PHP string in the
/// target's string result registers and jumps to `done_label`; `fallback_label` continues the
/// caller's table lookup.
pub(in crate::codegen::lower_inst::builtins) fn emit_context_free_object_class_name(
    ctx: &mut FunctionContext<'_>,
    lookup_kind: i64,
    fallback_label: &str,
    done_label: &str,
) {
    let arch = ctx.emitter.target.arch;
    let status_reg = abi::int_result_reg(ctx.emitter);
    let found_label = ctx.next_label("eval_dynamic_class_found");
    let not_found_label = ctx.next_label("eval_dynamic_class_not_found");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_store_to_sp(ctx.emitter, status_reg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_reg_move(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        status_reg,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        lookup_kind,
    );
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        0,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_dynamic_object_class_name_result");
    abi::emit_call_label(ctx.emitter, &symbol);
    match arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {status_reg}, #0"));          // status 0 resolves an eval-declared class
            ctx.emitter.instruction(&format!("b.eq {found_label}"));            // publish the eval class name
            ctx.emitter.instruction(&format!("cmp {status_reg}, #1"));          // status 1 means no owning eval context
            ctx.emitter.instruction(&format!("b.eq {not_found_label}"));        // restore the object and fall back
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {status_reg}, 0"));           // status 0 resolves an eval-declared class
            ctx.emitter.instruction(&format!("je {found_label}"));              // publish the eval class name
            ctx.emitter.instruction(&format!("cmp {status_reg}, 1"));           // status 1 means no owning eval context
            ctx.emitter.instruction(&format!("je {not_found_label}"));          // restore the object and fall back
        }
    }
    emit_eval_status_check(ctx);
    abi::emit_jump(ctx.emitter, &not_found_label);

    ctx.emitter.label(&not_found_label);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        status_reg,
        EVAL_TEMP_CELL_OFFSET,
    );
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, fallback_label);

    ctx.emitter.label(&found_label);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        status_reg,
        EVAL_RESULT_VALUE_CELL_OFFSET,
    );
    emit_owned_eval_class_name_string(ctx);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, done_label);
}

/// Asks the eval bridge for a class relation from a frame that owns no eval context.
///
/// The compile-time relation registry knows only compiled classes, so a helper such as Laravel's
/// `class_uses_recursive($model)` answered an empty relation for a model class an eval declared.
/// The bridge is called with a null context and answers from the process-global class metadata.
/// The boxed relation (or `false`) is left in the result register for the caller to store.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_global_class_relation_to_result(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    name: &str,
) -> Result<()> {
    let relation_kind = eval_class_relation_kind(name)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_mixed_operand_at(ctx, target, EVAL_TEMP_CELL_OFFSET)?);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    let target_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, target_arg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        relation_kind,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_class_relation");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    Ok(())
}

/// Asks the eval bridge's shared context `defined($name)` (`want_value == false`) or
/// `constant($name)` for a runtime name the compiled constant registry missed.
///
/// Leaves the published boxed Mixed result (a bool for `defined`, the value for `constant`) in the
/// integer result register. A `constant()` of an unknown name rethrows the bridge's `Error`.
pub(in crate::codegen::lower_inst::builtins) fn emit_eval_dynamic_constant_null_context(
    ctx: &mut FunctionContext<'_>,
    name: ValueId,
    want_value: bool,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_mixed_operand_at(ctx, name, EVAL_TEMP_CELL_OFFSET)?);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_arg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        i64::from(want_value),
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_dynamic_constant");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    Ok(())
}

/// Transfers boxed class-name results or detaches native strings before retiring their bridge cell.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_object_class_name(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    name: &str,
) -> Result<()> {
    let boxed_result = inst.result_php_type.codegen_repr() == PhpType::Mixed;
    emit_eval_object_class_name(ctx, object, name, boxed_result)?;
    store_if_result(ctx, inst)
}

/// Leaves the class name of `object` -- as the eval bridge answers it, so an interpreter-made
/// object or closure reports its PHP class (`Closure`, an eval-declared class) rather than the
/// native carrier's class id -- in the string result registers, or boxed as Mixed when
/// `boxed_result`. A non-object yields the empty string.
pub(in crate::codegen::lower_inst::builtins) fn emit_eval_object_class_name(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    name: &str,
    boxed_result: bool,
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
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_object_class_name");
    abi::emit_call_label(ctx.emitter, &symbol);
    retire_eval_metadata_operand_boxes(ctx, &[(object, EVAL_TEMP_CELL_OFFSET)])?;
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
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
    Ok(())
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
///
/// The bridge borrows the receiver cell rather than taking it, so the box this frame made is
/// still this frame's to release, on the answered path and on the non-object shortcut alike.
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
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_object_operand(ctx, object)?);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_eval_unboxed_not_object(ctx, &false_label);
    load_eval_context_to_arg(ctx, 0);
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, EVAL_TEMP_CELL_OFFSET);
    let (target_label, target_len) = ctx.data.add_string(target_class.as_bytes());
    let target_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, target_arg, &target_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        target_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        i64::from(exclude_self),
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_object_is_a");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_release_eval_boxed_operands_keeping_int_answer(ctx, &boxed);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&false_label);
    emit_release_eval_boxed_operands(ctx, &boxed);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Branches on an eval-owned object's named class or interface relation without a local context.
///
/// Nominal boundaries in an AOT function may receive an object from a request-global eval
/// autoloader.  Passing a null context lets the bridge recover that object's registered owner,
/// while a false result deliberately falls through to the ordinary native metadata matcher.
///
/// The bridge borrows the receiver cell rather than taking it, so the box this frame made is
/// released before the scratch frame goes away, on the matched and the fall-through answer alike.
///
/// An answer of 2 means the bridge owns the object and it is NOT an instance: the native matcher
/// only sees such an object's carrier header (`stdClass`), so it must not be asked. That answer
/// branches to `definite_no_label`.
pub(in crate::codegen::lower_inst::builtins) fn emit_eval_object_is_a_named_fallback(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    target_class: &str,
    matched_label: &str,
    native_fallback_label: &str,
    definite_no_label: &str,
) -> Result<()> {
    emit_eval_object_is_a_named_fallback_with(
        ctx,
        |ctx, _| store_eval_object_operand(ctx, object),
        target_class,
        matched_label,
        native_fallback_label,
        definite_no_label,
    )
}

/// Same probe for an object that is already a boxed Mixed cell in the int result register.
///
/// The shared typed-property guard uses this: its helper body has no SSA value to name, so the
/// cell arrives in a register instead. The bridge borrows the cell, so the caller's own
/// reference is untouched by the probe.
pub(in crate::codegen::lower_inst) fn emit_eval_object_is_a_named_fallback_from_result(
    ctx: &mut FunctionContext<'_>,
    target_class: &str,
    matched_label: &str,
    native_fallback_label: &str,
    definite_no_label: &str,
) -> Result<()> {
    emit_eval_object_is_a_named_fallback_with(
        ctx,
        |ctx, offset| {
            abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
            Ok(None)
        },
        target_class,
        matched_label,
        native_fallback_label,
        definite_no_label,
    )
}

/// Shared body of the named-fallback probes. `store_operand` receives the cell's offset in the
/// scratch frame and returns it again only when it boxed the cell itself and so owes its release.
fn emit_eval_object_is_a_named_fallback_with(
    ctx: &mut FunctionContext<'_>,
    store_operand: impl FnOnce(&mut FunctionContext<'_>, usize) -> Result<Option<usize>>,
    target_class: &str,
    matched_label: &str,
    native_fallback_label: &str,
    definite_no_label: &str,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_operand(ctx, EVAL_TEMP_CELL_OFFSET)?);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, EVAL_TEMP_CELL_OFFSET);
    let (target_label, target_len) = ctx.data.add_string(target_class.as_bytes());
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), &target_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        target_len as i64,
    );
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 4), 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_object_is_a");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_release_eval_boxed_operands_keeping_int_answer(ctx, &boxed);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #2");                              // 2 = a bridge-owned object that is definitely not an instance
            ctx.emitter.instruction(&format!("b.eq {}", definite_no_label));    // never ask the native matcher about its carrier header
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 2");                              // 2 = a bridge-owned object that is definitely not an instance
            ctx.emitter.instruction(&format!("je {}", definite_no_label));      // never ask the native matcher about its carrier header
        }
    }
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, matched_label);
    abi::emit_jump(ctx.emitter, native_fallback_label);
    Ok(())
}

/// Branches on an eval-owned object's relation to a RUNTIME class-name string, with no local context.
///
/// `fn ($value) => $value instanceof $class` compiled into a function that owns no eval context
/// resolves `$class` through the compile-time class table, which has never heard of a class the
/// interpreter declared (a controller, a model): the name does not resolve and the answer was
/// `false` for an object that IS one. Laravel's `ResolvesRouteDependencies::alreadyInParameters()`
/// is exactly that shape, so a route-bound model was dropped and a fresh empty one injected.
///
/// Only a string target (a `string` operand, or a boxed string) can be asked this way; an object
/// target or any other value goes straight to the native path, which keeps its invalid-target
/// fatal. The bridge borrows the receiver cell, so the box this frame made is released on every
/// exit, matched or not.
pub(in crate::codegen::lower_inst::builtins) fn emit_eval_object_is_a_dynamic_name_fallback(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    target: ValueId,
    target_ty: &PhpType,
    matched_label: &str,
    native_fallback_label: &str,
) -> Result<()> {
    let skip_label = ctx.next_label("eval_object_is_a_dynamic_name_skip");
    let ask_label = ctx.next_label("eval_object_is_a_dynamic_name_ask");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_object_operand(ctx, object)?);
    match target_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(target, ptr_reg, len_reg)?;
            save_eval_code_string(ctx);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(target, abi::int_result_reg(ctx.emitter))?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #1");                      // runtime tag 1 means the boxed target is a string
                    ctx.emitter.instruction(&format!("b.ne {}", skip_label));   // only a string names a class the bridge can look up
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 1");                      // runtime tag 1 means the boxed target is a string
                    ctx.emitter.instruction(&format!("jne {}", skip_label));    // only a string names a class the bridge can look up
                }
            }
            emit_eval_unboxed_string_result(ctx);
            save_eval_code_string(ctx);
        }
        _ => {
            abi::emit_jump(ctx.emitter, &skip_label);
        }
    }
    abi::emit_jump(ctx.emitter, &ask_label);
    ctx.emitter.label(&skip_label);
    emit_release_eval_boxed_operands(ctx, &boxed);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, native_fallback_label);

    ctx.emitter.label(&ask_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        EVAL_CODE_PTR_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        EVAL_CODE_LEN_OFFSET,
    );
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 4), 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_object_is_a");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_release_eval_boxed_operands_keeping_int_answer(ctx, &boxed);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    // Only 1 is "an instance": 2 is the bridge's "owned here and definitely not", which the
    // native matcher below also answers no to. Branching on any non-zero answer made
    // `$q instanceof $c` true for every class name an eval declared.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // 1 = the object is an instance of the named class
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // a match needs no native check
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // 1 = the object is an instance of the named class
            ctx.emitter.instruction(&format!("je {}", matched_label));          // a match needs no native check
        }
    }
    abi::emit_jump(ctx.emitter, native_fallback_label);
    Ok(())
}

/// Lowers object/class relation predicates whose target is a runtime string or object cell.
///
/// The bridge borrows the receiver and target cells rather than taking them, so both boxes are
/// released at the shared answer label, which the predicate and the non-object shortcut both
/// reach. The invalid-target path is not one of them: `__rt_instanceof_invalid_target` writes its
/// TypeError and exits the process, so its boxed operands outlive nothing.
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
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_mixed_operand_at(ctx, object, EVAL_TEMP_CELL_OFFSET)?);
    boxed.extend(store_eval_mixed_operand_at(ctx, target, EVAL_CODE_PTR_OFFSET)?);
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
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_object_is_a_dynamic");
    abi::emit_call_label(ctx.emitter, &symbol);
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
/// Lowers `highlight_string()`/`highlight_file()` through the interpreter's PHP highlighter.
///
/// The bridge borrows the boxed subject cell (so an object argument still converts through
/// `__toString()` inside the interpreter), echoes or returns through the same renderer the eval
/// builtin uses, and answers one owned boxed `string|bool` cell, which is exactly the `Mixed`
/// representation of the declared union.
pub(in crate::codegen::lower_inst) fn lower_eval_highlight(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    if !(1..=2).contains(&inst.operands.len()) {
        return Err(CodegenIrError::invalid_module(format!(
            "{name} expected 1..=2 args, got {}",
            inst.operands.len()
        )));
    }
    let subject = expect_operand(inst, 0)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_mixed_operand_at(ctx, subject, EVAL_TEMP_CELL_OFFSET)?);
    // `$return` is PHP's second parameter: absent means the default `false`.
    let flag_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    if let Some(flag) = inst.operands.get(1).copied() {
        let flag_ty = ctx.load_value_to_result(flag)?.codegen_repr();
        if flag_ty != PhpType::Bool {
            return Err(CodegenIrError::unsupported(format!(
                "{name} $return lowering for PHP type {flag_ty:?}"
            )));
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        if flag_arg != result_reg {
            ctx.emitter
                .instruction(&format!("mov {flag_arg}, {result_reg}")); // pass PHP's `$return` flag as the bridge's second argument
        }
    } else {
        abi::emit_load_int_immediate(ctx.emitter, flag_arg, 0);
    }
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        EVAL_TEMP_CELL_OFFSET,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol(&format!("__elephc_eval_{name}"));
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_release_eval_boxed_operands_keeping_int_answer(ctx, &boxed);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

