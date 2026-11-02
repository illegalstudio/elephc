//! Purpose:
//! Lowers Fiber construction and the public dynamic-new entry points.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Callable descriptors and eval fallback ordering retain their existing contracts.

use super::*;

/// Keeps the direct AOT fast path bounded when the eval bridge can materialize class strings.
const EVAL_DYNAMIC_NEW_AOT_CANDIDATE_LIMIT: usize = 24;

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
    // An interpreted subclass has no AOT case (and usually no compiled class id): `new static`
    // inside an inherited builtin method must still build it, so the class-string is kept for
    // the eval bridge instead of dying on the factory fatal.
    let eval_bridge_available = builtins::has_eval_context(ctx)
        || ctx.module.required_runtime_features.eval_bridge;
    if candidates.is_empty() && !eval_bridge_available {
        return Err(CodegenIrError::unsupported(format!(
            "dynamic object construction for {} without EIR-lowered candidates",
            required_parent
        )));
    }
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("dynamic_object_new missing result value"))?;
    emit_dynamic_new_class_lookup(ctx, class_name_value, &required_parent, eval_bridge_available)?;
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
    let matched_ids = candidates.iter().map(|candidate| candidate.class_id).collect::<Vec<_>>();
    let line = inst.span.map_or(0, |span| span.source_line());
    let refusals = dynamic_new_constrained_refusals(
        ctx,
        &required_parent,
        constructor_args.len(),
        line,
        &matched_ids,
    );
    let refusal_labels = refusals
        .iter()
        .map(|(class_id, _)| {
            let label = ctx.next_label("dynamic_new_refused");
            emit_compare_dynamic_new_class_id(ctx, *class_id, &label);
            label
        })
        .collect::<Vec<_>>();
    abi::emit_jump(ctx.emitter, &unmatched_label);

    // Each arm diverges into the unwinder with php's `ArgumentCountError`; see
    // `dynamic_new_constrained_refusals`.
    let refusal_location = ctx.module.source_path.clone().map(|file| (file, line));
    for ((_, refusal), label) in refusals.iter().zip(refusal_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        if eval_bridge_available {
            abi::emit_release_temporary_stack(ctx.emitter, 16);
        }
        super::super::exceptions::emit_argument_count_error(
            ctx,
            &refusal.message,
            refusal_location.clone(),
        );
    }

    ctx.emitter.label(&unmatched_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    if eval_bridge_available {
        abi::emit_jump(ctx.emitter, &invalid_label);
    } else {
        emit_dynamic_new_fatal(ctx, &required_parent);
    }

    ctx.emitter.label(&invalid_label);
    if eval_bridge_available {
        let eval_miss_label = ctx.next_label("dynamic_new_eval_miss");
        builtins::lower_eval_object_new_dynamic_fallback(ctx, inst, &eval_miss_label)?;
        ctx.store_result_value(result)?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&eval_miss_label);
    }
    emit_dynamic_new_fatal(ctx, &required_parent);

    for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        if eval_bridge_available {
            abi::emit_release_temporary_stack(ctx.emitter, 16);
        }
        emit_dynamic_new_candidate(ctx, candidate, constructor_args, result, inst.span.map_or(0, |span| span.source_line()))?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&done_label);
    Ok(())
}

/// The builtin compact-payload exception classes a generic `new $class(...)` can build directly.
///
/// Only when the site's arguments fit the constructor statically: at most message, code and
/// previous, the code an `int` and the previous an object -- the message is converted like any
/// string argument. Classes already matched as declared-slot candidates are left to them.
fn builtin_throwable_mixed_cases(
    ctx: &FunctionContext<'_>,
    constructor_args: &[ValueId],
    matched: &[String],
) -> Result<Vec<(String, u64)>> {
    if constructor_args.len() > 3 {
        return Ok(Vec::new());
    }
    if let Some(message) = constructor_args.first() {
        // What `load_value_as_string_to_regs` converts; an array or object message is another
        // class's constructor argument at this site, not a Throwable message.
        let message_ty = ctx.value_php_type(*message)?;
        let convertible = matches!(message_ty, PhpType::Resource(_))
            || matches!(
                message_ty.codegen_repr(),
                PhpType::Str
                    | PhpType::Int
                    | PhpType::Float
                    | PhpType::Bool
                    | PhpType::Void
                    | PhpType::TaggedScalar
                    | PhpType::Mixed
                    | PhpType::Union(_)
            );
        if !convertible {
            return Ok(Vec::new());
        }
    }
    if let Some(code) = constructor_args.get(1) {
        if ctx.value_php_type(*code)?.codegen_repr() != PhpType::Int {
            return Ok(Vec::new());
        }
    }
    if let Some(previous) = constructor_args.get(2) {
        if !matches!(ctx.value_php_type(*previous)?.codegen_repr(), PhpType::Object(_)) {
            return Ok(Vec::new());
        }
    }
    let mut cases = ctx
        .module
        .class_infos
        .iter()
        .filter(|(class_name, _)| {
            is_builtin_throwable_payload_class(class_name)
                && !matched.iter().any(|name| name == *class_name)
        })
        .map(|(class_name, class_info)| (class_name.clone(), class_info.class_id))
        .collect::<Vec<_>>();
    cases.sort_by_key(|(_, class_id)| *class_id);
    Ok(cases)
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
    let eval_bridge_available = builtins::has_eval_context(ctx)
        || ctx.module.required_runtime_features.eval_bridge;
    let defer_aot_candidates_to_eval = !uses_runtime_arg_container
        && eval_bridge_available
        && dynamic_new_mixed_aot_candidate_count(ctx) > EVAL_DYNAMIC_NEW_AOT_CANDIDATE_LIMIT;
    let all_candidates = if defer_aot_candidates_to_eval {
        // The eval bridge preserves positional constructor arguments without emitting one
        // compare-and-constructor block per unrelated AOT class in the module.
        Vec::new()
    } else {
        dynamic_new_mixed_candidates(
            ctx,
            (!uses_runtime_arg_container).then_some(constructor_args.len()),
            inst,
        )?
    };
    // The ladder is outlined (see `outlined_dynamic_new`): the shared helper holds every candidate
    // that is not a Throwable -- a Throwable's creation site records THIS statement's file and
    // line, which the helper cannot know -- and answers `0` for a name it does not hold. The site
    // keeps the Throwable arms, the refusals, the registry fallback and the fatal.
    let in_ctor_helper = super::super::outlined_dynamic_new::inside_dynamic_new_ctor_helper(ctx);
    // The arms the site keeps for itself: a class whose constructor takes a parameter by
    // reference, which binds the CALLER's variable while the helper receives values. Such a class
    // used to keep the site's WHOLE ladder inline -- one constructor with a by-reference parameter
    // anywhere in the program put every same-arity site back inline. A Throwable no longer stays
    // here: the site publishes its file and line for the helper (`publish_creation_site`), and the
    // ~40-200 Throwable arms each site kept were most of the 48 inline sites of ~22 000
    // instructions in the Symfony console. The runtime-argument-container form passes no caller
    // variable, so its helper holds them all.
    let site_only = |_ctx: &FunctionContext<'_>, candidate: &DynamicNewCandidate| {
        !uses_runtime_arg_container
            && candidate
                .constructor_impl
                .as_ref()
                .is_some_and(|target| target.ref_params.iter().any(|by_ref| *by_ref))
    };
    let helper_arm_count = all_candidates
        .iter()
        .filter(|candidate| !site_only(ctx, candidate))
        .count();
    let outline_candidates = !in_ctor_helper
        && helper_arm_count >= super::super::outlined_dynamic_new::MIN_CTOR_ARMS
        && (uses_runtime_arg_container || !builtins::has_eval_context(ctx))
        && !defer_aot_candidates_to_eval;
    let mut site_candidates_are_throwables_only = false;
    if outline_candidates
        && super::super::outlined_dynamic_new::emit_outlined_dynamic_new_call(
            ctx,
            inst,
            super::super::outlined_dynamic_new::CTOR_PREFIX,
        )?
    {
        // A boxed object means the helper constructed it; `0` is a miss.
        ctx.load_value_to_result(result)?;
        let missed = ctx.next_label("dynamic_new_mixed_helper_missed");
        abi::emit_branch_if_int_result_zero(ctx.emitter, &missed);
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&missed);
        site_candidates_are_throwables_only = true;
    }
    if !emit_generic_dynamic_new_class_string(ctx, class_name_value, &non_string_label)? {
        emit_dynamic_new_invalid_class_name_fatal(ctx);
        return Ok(());
    }
    abi::emit_push_result_value(ctx.emitter, &PhpType::Str);

    let fallback_label = ctx.next_label("dynamic_new_mixed_fallback");
    // Refusals exclude every class some arm already holds, the helper's included.
    let all_names = all_candidates
        .iter()
        .map(|candidate| candidate.class_name.clone())
        .collect::<Vec<_>>();
    let mut candidates = Vec::with_capacity(all_candidates.len());
    for candidate in all_candidates {
        let kept_by_site = site_only(ctx, &candidate);
        let keep = if in_ctor_helper {
            !kept_by_site
        } else if site_candidates_are_throwables_only {
            kept_by_site
        } else {
            true
        };
        if keep {
            candidates.push(candidate);
        }
    }
    // The runtime-argument-container form describes most classes by a table record instead of
    // one compare-and-construct arm each (see `dynamic_new_table`).
    let (table_candidates, candidates) = if uses_runtime_arg_container {
        super::dynamic_new_table::split_table_candidates(ctx, candidates, true)
    } else {
        (Vec::new(), candidates)
    };
    let table = if table_candidates.is_empty() {
        None
    } else {
        Some((
            super::dynamic_new_table::build_table(ctx, &table_candidates, true)?,
            table_candidates.len(),
        ))
    };
    let table_hit_label = ctx.next_label("dynamic_new_mixed_table_hit");
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
    let mut matched = all_names;
    // Builtin exceptions are no declared-slot candidate: they use the compact Throwable payload,
    // and the registry fallback allocates one without running the constructor -- no message, no
    // file or line. They get the direct `new`'s own construction instead, when the site's
    // arguments fit it statically; any other shape keeps the paths below.
    let throwable_cases = if in_ctor_helper || uses_runtime_arg_container || eval_bridge_available {
        Vec::new()
    } else {
        builtin_throwable_mixed_cases(ctx, constructor_args, &matched)?
    };
    let throwable_labels = throwable_cases
        .iter()
        .map(|(class_name, _)| {
            let label = ctx.next_label("dynamic_new_mixed_throwable");
            emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, class_name, &label);
            label
        })
        .collect::<Vec<_>>();
    matched.extend(throwable_cases.iter().map(|(class_name, _)| class_name.clone()));
    let refusals = if in_ctor_helper || uses_runtime_arg_container || builtins::has_eval_context(ctx) {
        Vec::new()
    } else {
        dynamic_new_mixed_refusals(
            ctx,
            constructor_args.len(),
            inst.span.map_or(0, |span| span.source_line()),
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
    if let Some((table_label, count)) = &table {
        super::dynamic_new_table::emit_table_lookup(ctx, table_label, *count, &table_hit_label);
    }
    abi::emit_jump(ctx.emitter, &fallback_label);

    if table.is_some() {
        ctx.emitter.label(&table_hit_label);
        super::dynamic_new_table::emit_table_tail(
            ctx,
            constructor_arg_container,
            result,
            &done_label,
            inst.span.map_or(0, |span| span.source_line()),
        )?;
    }

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
            inst.span.map_or(0, |span| span.source_line()),
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    for ((class_name, class_id), label) in throwable_cases.iter().zip(throwable_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        emit_throwable_allocation(
            ctx,
            class_name,
            *class_id,
            inst.span.map_or(0, |span| span.source_line()),
            inst.span.is_some_and(|span| !span.is_from_source()),
        );
        preserve_throwable_for_init(ctx);
        emit_throwable_message_fields(ctx, constructor_args.first().copied())?;
        emit_throwable_code_field(ctx, constructor_args.get(1).copied())?;
        emit_throwable_previous_field(ctx, constructor_args.get(2).copied())?;
        restore_throwable_after_init(ctx);
        emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Object(class_name.clone()));
        ctx.store_result_value(result)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    // Each arm diverges into the unwinder, so none of them falls through to `done_label` and none
    // stores a result: a refused `new` leaves no object behind.
    let refusal_location = ctx
        .module
        .source_path
        .clone()
        .map(|file| (file, inst.span.map_or(0, |span| span.source_line())));
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
    if in_ctor_helper {
        // A name this helper does not hold: answer `0` and let the site run its own remaining
        // arms, the registry fallback and the class-not-found fatal.
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        ctx.store_result_value(result)?;
        abi::emit_jump(ctx.emitter, &done_label);
    } else if eval_bridge_available {
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
    if in_ctor_helper {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        ctx.store_result_value(result)?;
        abi::emit_jump(ctx.emitter, &done_label);
    } else {
        emit_dynamic_new_invalid_class_name_fatal(ctx);
    }

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
    if super::super::outlined_dynamic_new::try_lower_outlined_dynamic_new(
        ctx,
        inst,
        super::super::outlined_dynamic_new::NOCTOR_PREFIX,
    )? {
        return Ok(());
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
    let (table_candidates, candidates) =
        super::dynamic_new_table::split_table_candidates(ctx, candidates, false);
    let table = if table_candidates.is_empty() {
        None
    } else {
        Some((
            super::dynamic_new_table::build_table(ctx, &table_candidates, false)?,
            table_candidates.len(),
        ))
    };
    let table_hit_label = ctx.next_label("dynamic_new_no_ctor_mixed_table_hit");
    let case_labels = candidates
        .iter()
        .map(|candidate| {
            let label = ctx.next_label("dynamic_new_no_ctor_mixed_case");
            emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, &candidate.class_name, &label);
            label
        })
        .collect::<Vec<_>>();
    if let Some((table_label, count)) = &table {
        super::dynamic_new_table::emit_table_lookup(ctx, table_label, *count, &table_hit_label);
    }
    abi::emit_jump(ctx.emitter, &fallback_label);

    if table.is_some() {
        ctx.emitter.label(&table_hit_label);
        super::dynamic_new_table::emit_table_tail_without_constructor(ctx, result, &done_label)?;
    }

    for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        emit_dynamic_new_without_constructor_mixed_candidate(ctx, candidate, result)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&fallback_label);
    // A class only the interpreter declared has no compiled case: the polyfill-deepclone
    // rebuild of a cached Doctrine `ParserResult` reached here and died "Class not found".
    if builtins::has_eval_context(ctx) || ctx.module.required_runtime_features.eval_bridge {
        let eval_miss_label = ctx.next_label("dynamic_new_no_ctor_mixed_eval_miss");
        builtins::lower_eval_object_new_without_constructor_fallback(ctx, inst, &eval_miss_label)?;
        ctx.store_result_value(result)?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&eval_miss_label);
    }
    emit_dynamic_new_class_not_found_fatal(ctx);

    ctx.emitter.label(&non_string_label);
    emit_dynamic_new_invalid_class_name_fatal(ctx);

    ctx.emitter.label(&done_label);
    Ok(())
}
