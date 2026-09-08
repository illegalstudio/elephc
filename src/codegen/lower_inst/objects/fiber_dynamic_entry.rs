//! Purpose:
//! Lowers Fiber construction and the public dynamic-new entry points.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Callable descriptors and eval fallback ordering retain their existing contracts.

use super::*;

/// Lowers `new Fiber($callable)` through the runtime-managed Fiber constructor.
pub(super) fn lower_fiber_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let class_id = ctx
        .module
        .class_infos
        .get("Fiber")
        .map(|class| class.class_id)
        .unwrap_or(0);
    let callable_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    if let Some(callable) = inst.operands.first().copied() {
        let callable_ty = ctx.value_php_type(callable)?.codegen_repr();
        if callable_ty == PhpType::Str {
            callables::emit_runtime_string_descriptor_value(
                ctx,
                callable,
                callable_arg,
                "fiber_constructor",
                false,
            )?;
        } else if matches!(
            &callable_ty,
            PhpType::Array(elem) if matches!(elem.codegen_repr(), PhpType::Mixed | PhpType::Str)
        ) {
            callables::emit_runtime_callable_array_descriptor_value(
                ctx,
                callable,
                "fiber_constructor",
            )?;
            move_fiber_callable_result_to_arg(ctx, callable_arg);
        } else if let PhpType::Object(class_name) = callable_ty {
            callables::emit_invokable_object_descriptor_value(
                ctx,
                callable,
                &class_name,
                "fiber_constructor",
            )?;
            move_fiber_callable_result_to_arg(ctx, callable_arg);
        } else if callable_ty == PhpType::Callable {
            ctx.load_value_to_result(callable)?;
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
            move_fiber_callable_result_to_arg(ctx, callable_arg);
        } else {
            ctx.load_value_to_reg(callable, callable_arg)?;
        }
    } else {
        abi::emit_load_int_immediate(ctx.emitter, callable_arg, 0);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        class_id as i64,
    );
    let wrapper_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    if let Some(wrapper) = fibers::wrapper_for_fiber_new(ctx.module, ctx.function, inst) {
        abi::emit_symbol_address(ctx.emitter, wrapper_arg, &wrapper.label);
    } else {
        abi::emit_load_int_immediate(ctx.emitter, wrapper_arg, 0);
    }
    abi::emit_call_label(ctx.emitter, "__rt_fiber_construct");
    store_if_result(ctx, inst)
}

/// Moves a descriptor materialized in the result register into Fiber constructor arg 1.
pub(super) fn move_fiber_callable_result_to_arg(ctx: &mut FunctionContext<'_>, callable_arg: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    if result_reg == callable_arg {
        return;
    }
    ctx.emitter
        .instruction(&format!("mov {}, {}", callable_arg, result_reg)); // pass selected callable descriptor to Fiber constructor
}

/// Lowers constrained runtime class-string object construction.
pub(in crate::codegen::lower_inst) fn lower_dynamic_object_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (_fallback_class, required_parent) = dynamic_object_new_metadata(ctx, inst)?;
    let class_name_value = expect_operand(inst, 0)?;
    let constructor_args = inst.operands.get(1..).ok_or_else(|| {
        CodegenIrError::invalid_module("dynamic_object_new missing class operand")
    })?;
    let candidates = dynamic_new_candidates(ctx, &required_parent, constructor_args.len(), inst)?;
    if candidates.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "dynamic object construction for {} without EIR-lowered candidates",
            required_parent
        )));
    }
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("dynamic_object_new missing result value"))?;
    emit_dynamic_new_class_lookup(ctx, class_name_value, &required_parent)?;
    let invalid_label = ctx.next_label("dynamic_new_invalid");
    let unmatched_label = ctx.next_label("dynamic_new_unmatched");
    let done_label = ctx.next_label("dynamic_new_done");
    emit_branch_if_dynamic_new_lookup_invalid(ctx, &invalid_label);
    emit_push_dynamic_new_class_id(ctx);
    let case_labels = candidates
        .iter()
        .map(|candidate| {
            let label = ctx.next_label("dynamic_new_case");
            emit_compare_dynamic_new_class_id(ctx, candidate.class_id, &label);
            label
        })
        .collect::<Vec<_>>();
    abi::emit_jump(ctx.emitter, &unmatched_label);

    ctx.emitter.label(&unmatched_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    emit_dynamic_new_fatal(ctx, &required_parent);

    ctx.emitter.label(&invalid_label);
    emit_dynamic_new_fatal(ctx, &required_parent);

    for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        emit_dynamic_new_candidate(ctx, candidate, constructor_args, result)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers generic PHP `new $class(...)` into AOT candidates plus the runtime registry fallback.
pub(in crate::codegen::lower_inst) fn lower_dynamic_object_new_mixed(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let class_name_value = expect_operand(inst, 0)?;
    let uses_runtime_arg_container = matches!(inst.immediate, Some(Immediate::Bool(true)));
    let constructor_arg_container = if uses_runtime_arg_container {
        Some(*inst.operands.get(1).ok_or_else(|| {
            CodegenIrError::invalid_module(
                "dynamic_object_new_mixed missing runtime constructor argument container",
            )
        })?)
    } else {
        None
    };
    let constructor_args = if uses_runtime_arg_container {
        &inst.operands[0..0]
    } else {
        inst.operands.get(1..).ok_or_else(|| {
            CodegenIrError::invalid_module("dynamic_object_new_mixed missing class operand")
        })?
    };
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("dynamic_object_new_mixed missing result value")
    })?;
    let done_label = ctx.next_label("dynamic_new_mixed_done");
    let non_string_label = ctx.next_label("dynamic_new_mixed_non_string");
    if !emit_generic_dynamic_new_class_string(ctx, class_name_value, &non_string_label)? {
        emit_dynamic_new_invalid_class_name_fatal(ctx);
        return Ok(());
    }
    abi::emit_push_result_value(ctx.emitter, &PhpType::Str);

    let fallback_label = ctx.next_label("dynamic_new_mixed_fallback");
    let candidates = dynamic_new_mixed_candidates(
        ctx,
        (!uses_runtime_arg_container).then_some(constructor_args.len()),
        inst,
    )?;
    let case_labels = candidates
        .iter()
        .map(|candidate| {
            let label = ctx.next_label("dynamic_new_mixed_case");
            emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, &candidate.class_name, &label);
            label
        })
        .collect::<Vec<_>>();
    // A class the ladder knows but whose constructor this site cannot satisfy has to REFUSE, not
    // fall through: the fallback allocates by name and skips the constructor entirely, which
    // answered with a half-built object. TWO questions are settled here — too FEW arguments, and
    // an argument the variadic collector cannot TAKE — and each raises its own throwable.
    //
    // NOT UNDER EVAL. The eval bridge resolves the class itself and raises its own diagnostic, and
    // these arms sit before the fallback that reaches it, so they would preempt a path measured
    // to work. NOT UNDER A RUNTIME ARG CONTAINER either, where the site's arity is not a constant.
    let matched = candidates
        .iter()
        .map(|candidate| candidate.class_name.clone())
        .collect::<Vec<_>>();
    let refusals = if uses_runtime_arg_container || builtins::has_eval_context(ctx) {
        Vec::new()
    } else {
        dynamic_new_mixed_refusals(
            ctx,
            constructor_args.len(),
            inst.span.map_or(0, |span| span.line),
            &matched,
        )
    };
    let refusal_labels = refusals
        .iter()
        .map(|refusal| {
            let label = ctx.next_label("dynamic_new_mixed_refused");
            emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, &refusal.class_name, &label);
            label
        })
        .collect::<Vec<_>>();
    abi::emit_jump(ctx.emitter, &fallback_label);

    for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        emit_dynamic_new_mixed_candidate(
            ctx,
            candidate,
            constructor_args,
            constructor_arg_container,
            class_name_value,
            result,
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    // Each arm diverges into the unwinder, so none of them falls through to `done_label` and none
    // stores a result: a refused `new` leaves no object behind.
    let refusal_location = ctx
        .module
        .source_path
        .clone()
        .map(|file| (file, inst.span.map_or(0, |span| span.line)));
    for (refusal, label) in refusals.iter().zip(refusal_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        if refusal.argument_count {
            super::super::exceptions::emit_argument_count_error(
                ctx,
                &refusal.message,
                refusal_location.clone(),
            );
        } else {
            super::super::exceptions::emit_type_error_at(
                ctx,
                &refusal.message,
                refusal_location.clone(),
            );
        }
    }

    ctx.emitter.label(&fallback_label);
    if builtins::has_eval_context(ctx) {
        let eval_miss_label = ctx.next_label("dynamic_new_mixed_eval_miss");
        builtins::lower_eval_object_new_dynamic_fallback(ctx, inst, &eval_miss_label)?;
        ctx.store_result_value(result)?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&eval_miss_label);
        emit_dynamic_new_class_not_found_fatal(ctx);
    } else {
        emit_dynamic_new_mixed_fallback(ctx);
        ctx.store_result_value(result)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&non_string_label);
    emit_dynamic_new_invalid_class_name_fatal(ctx);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers dynamic allocation that intentionally skips PHP constructor dispatch.
pub(in crate::codegen::lower_inst) fn lower_dynamic_object_new_without_constructor_mixed(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let class_name_value = expect_operand(inst, 0)?;
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(
            "dynamic_object_new_without_constructor_mixed expects only a class operand",
        ));
    }
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module(
            "dynamic_object_new_without_constructor_mixed missing result value",
        )
    })?;
    let done_label = ctx.next_label("dynamic_new_no_ctor_mixed_done");
    let non_string_label = ctx.next_label("dynamic_new_no_ctor_mixed_non_string");
    if !emit_generic_dynamic_new_class_string(ctx, class_name_value, &non_string_label)? {
        emit_dynamic_new_invalid_class_name_fatal(ctx);
        return Ok(());
    }
    abi::emit_push_result_value(ctx.emitter, &PhpType::Str);

    let fallback_label = ctx.next_label("dynamic_new_no_ctor_mixed_fallback");
    let candidates = dynamic_new_without_constructor_mixed_candidates(ctx, inst)?;
    let case_labels = candidates
        .iter()
        .map(|candidate| {
            let label = ctx.next_label("dynamic_new_no_ctor_mixed_case");
            emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, &candidate.class_name, &label);
            label
        })
        .collect::<Vec<_>>();
    let abstract_classes = dynamic_new_without_constructor_abstract_classes(ctx);
    let abstract_labels = abstract_classes
        .iter()
        .map(|class_name| {
            let label = ctx.next_label("dynamic_new_no_ctor_mixed_abstract");
            emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, class_name, &label);
            label
        })
        .collect::<Vec<_>>();
    abi::emit_jump(ctx.emitter, &fallback_label);

    for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        emit_dynamic_new_without_constructor_mixed_candidate(ctx, candidate, result)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    for (class_name, label) in abstract_classes.iter().zip(abstract_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        super::super::exceptions::emit_error(
            ctx,
            &format!("Cannot instantiate abstract class {}", class_name),
        );
    }

    ctx.emitter.label(&fallback_label);
    emit_dynamic_new_class_not_found_fatal(ctx);

    ctx.emitter.label(&non_string_label);
    emit_dynamic_new_invalid_class_name_fatal(ctx);

    ctx.emitter.label(&done_label);
    Ok(())
}
