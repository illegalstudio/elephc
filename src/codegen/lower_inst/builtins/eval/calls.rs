//! Purpose:
//! Lowers eval entry points and literal EIR AOT fast paths.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - The bridge fallback preserves PHP source order and result ownership.

use super::*;

/// Lowers `eval($code)` through internal EIR AOT or the bridge ABI and leaves its result in registers.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "eval", 1)?;
    if let Some(fragment) = eval_literal_fragment(ctx, inst)? {
        if lower_eval_literal_eir_function(ctx, inst, &fragment)? {
            return Ok(());
        }
    }
    emit_eval_literal_aot_marker(ctx, inst)?;
    let parse_site = eval_parse_site(ctx, inst)?;
    let code = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(code)?.codegen_repr();
    if ty != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "eval() argument lowering for PHP type {:?}",
            ty
        )));
    }

    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    save_eval_code_string(ctx);
    ensure_eval_context(ctx)?;
    mark_eval_strict_php(ctx, inst);
    mark_eval_php_version(ctx);
    set_eval_call_site(ctx, inst);
    ensure_eval_scope(ctx)?;
    ensure_eval_global_scope(ctx)?;
    let sync_locals = eval_sync_locals(ctx);
    let sync_globals = eval_sync_globals(ctx);
    let global_aliases = eval_global_aliases(ctx);
    flush_eval_scope_locals(ctx, &sync_locals)?;
    flush_eval_global_scope(ctx, &sync_globals)?;
    mark_eval_scope_global_aliases(ctx, &global_aliases);
    set_eval_context_global_scope(ctx);
    let pushed_class_scope = push_eval_context_class_scope(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    load_eval_scope_to_arg(ctx, 1);
    move_saved_eval_code_to_eval_args(ctx);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_execute");
    abi::emit_call_label(ctx.emitter, &symbol);
    pop_eval_context_class_scope(ctx, pushed_class_scope);
    emit_eval_status_check_at(ctx, parse_site);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    reload_eval_scope_locals(ctx, &sync_locals)?;
    reload_eval_global_scope(ctx, &sync_globals)?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Resolves where php would say THIS `eval()` failed to parse.
///
/// The call line comes from the instruction span, the same source `set_eval_call_site` uses for
/// `__LINE__`. The line inside the fragment is only knowable for a literal one, and only by
/// parsing it again here — the bridge answers a status code and nothing else. A fragment that
/// is not a literal, or one elephc's own parser accepts, reports line 1: php's own answer for
/// every single-line fragment, which is nearly all of them.
fn eval_parse_site(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<Option<EvalParseSite>> {
    let Some(call_line) = inst.span.map(|span| span.line) else {
        return Ok(None);
    };
    let fragment_line = match eval_literal_fragment(ctx, inst)? {
        Some(fragment) => crate::eval_aot::literal_fragment_parse_error_line(
            &fragment,
            super::super::instruction_strict_php_profile(inst),
        )
        .unwrap_or(1),
        None => 1,
    };
    Ok(Some(EvalParseSite {
        call_line,
        fragment_line,
    }))
}

/// Calls a pre-lowered internal EIR function for no-scope literal eval fragments.
pub(super) fn lower_eval_literal_eir_function(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    fragment: &str,
) -> Result<bool> {
    let strict_php = super::super::instruction_strict_php_profile(inst);
    let function_name = crate::eval_aot::eir_function_name(fragment, strict_php);
    if let Some(callee) = ctx.callable_function_by_name(&function_name) {
        if callee.params.is_empty() && callee.return_php_type.codegen_repr() == PhpType::Mixed {
            ctx.emitter
                .comment("eval literal AOT compiled EIR function");
            let caller_stack_pad_bytes = abi::outgoing_call_stack_pad_bytes(ctx.emitter.target, 0);
            abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
            abi::emit_call_label(ctx.emitter, &function_symbol(&function_name));
            abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
            store_if_result(ctx, inst)?;
            return Ok(true);
        }
    }
    lower_eval_literal_scope_eir_function(ctx, inst, fragment)
}

/// Calls a pre-lowered internal EIR function that uses direct params or eval scope.
pub(super) fn lower_eval_literal_scope_eir_function(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    fragment: &str,
) -> Result<bool> {
    let strict_php = super::super::instruction_strict_php_profile(inst);
    let function_name = crate::eval_aot::eir_scope_function_name(fragment, strict_php);
    let Some(callee) = ctx.callable_function_by_name(&function_name) else {
        return Ok(false);
    };
    let param_types = callee
        .params
        .iter()
        .map(|param| param.php_type.codegen_repr())
        .collect::<Vec<_>>();
    let return_type = callee.return_php_type.codegen_repr();
    let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
        fragment,
        ctx.module.source_path.as_deref(),
        strict_php,
        |name, args| eval_literal_static_function_supported_by_codegen(ctx, name, args),
        |receiver, method, args| {
            eval_literal_static_method_supported_by_codegen(ctx, receiver, method, args)
        },
    );
    if plan.uses_scope_read_params() {
        return lower_eval_literal_scope_read_param_eir_function(
            ctx,
            inst,
            &function_name,
            &param_types,
            &return_type,
            plan.reads(),
            plan.array_read_constraints(),
            plan.assoc_array_read_constraints(),
            plan.float_predicate_read_constraints(),
        );
    }
    if !eval_scope_read_constraints_supported(
        ctx,
        plan.array_read_constraints(),
        plan.assoc_array_read_constraints(),
        plan.float_predicate_read_constraints(),
    ) {
        return Ok(false);
    }
    if param_types.len() != 1 || return_type != PhpType::Mixed {
        return Ok(false);
    }
    ctx.emitter
        .comment("eval literal AOT compiled EIR function with eval scope");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_scope(ctx)?;
    let read_names = plan.reads().clone();
    let write_names = plan.writes().clone();
    let mut flush_names = read_names.clone();
    flush_names.extend(write_names.iter().cloned());
    let sync_locals = eval_sync_locals(ctx);
    let sync_globals = eval_sync_globals(ctx);
    let flush_locals = filter_eval_sync_locals_by_name(sync_locals.clone(), &flush_names);
    let flush_globals = filter_eval_sync_globals_by_name(sync_globals.clone(), &flush_names);
    let reload_locals = filter_eval_sync_locals_by_name(sync_locals, &write_names);
    let reload_globals = filter_eval_sync_globals_by_name(sync_globals, &write_names);
    flush_eval_scope_locals(ctx, &flush_locals)?;
    flush_eval_globals_to_local_scope(ctx, &flush_globals);
    load_eval_scope_to_arg(ctx, 0);
    abi::emit_call_label(ctx.emitter, &function_symbol(&function_name));
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    reload_eval_scope_locals(ctx, &reload_locals)?;
    reload_eval_globals_from_local_scope(ctx, &reload_globals)?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Calls a read-only scope eval AOT function by passing direct boxed Mixed params.
pub(super) fn lower_eval_literal_scope_read_param_eir_function(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    function_name: &str,
    param_types: &[PhpType],
    return_type: &PhpType,
    read_names: &BTreeSet<String>,
    array_read_constraints: &BTreeSet<String>,
    assoc_array_read_constraints: &BTreeSet<String>,
    float_predicate_read_constraints: &BTreeSet<String>,
) -> Result<bool> {
    if param_types.len() != read_names.len()
        || param_types
            .iter()
            .any(|ty| ty.codegen_repr() != PhpType::Mixed)
        || return_type.codegen_repr() != PhpType::Mixed
    {
        return Ok(false);
    }
    if !eval_scope_read_constraints_supported(
        ctx,
        array_read_constraints,
        assoc_array_read_constraints,
        float_predicate_read_constraints,
    ) {
        return Ok(false);
    }
    let Some(param_sources) = eval_scope_read_param_sources(ctx, read_names) else {
        return Ok(false);
    };
    ctx.emitter
        .comment("eval literal AOT compiled EIR function with direct read params");
    for source in &param_sources {
        emit_eval_scope_read_param_source(ctx, source)?;
        abi::emit_push_result_value(ctx.emitter, &PhpType::Mixed);
    }
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, param_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(ctx.emitter, &assignments);
    let caller_stack_pad_bytes =
        abi::outgoing_call_stack_pad_bytes(ctx.emitter.target, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &function_symbol(function_name));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Resolves read-only eval variables to direct local values or undefined null.
pub(super) fn eval_scope_read_param_sources(
    ctx: &FunctionContext<'_>,
    read_names: &BTreeSet<String>,
) -> Option<Vec<EvalScopeReadParamSource>> {
    let sync_locals = eval_sync_locals(ctx);
    read_names
        .iter()
        .map(|name| {
            if let Some(local) = sync_locals.iter().find(|local| local.name == *name) {
                return Some(EvalScopeReadParamSource::Local(local.clone()));
            }
            if ctx.function.locals.iter().any(|local| {
                local.name.as_deref() == Some(name.as_str())
                    && local.kind == LocalKind::PhpLocal
                    && !local_uses_eval_global_sync(ctx, local.name.as_deref())
                    && local.php_type.codegen_repr() == PhpType::Void
            }) {
                return Some(EvalScopeReadParamSource::Null);
            }
            let has_unsupported_local = ctx
                .function
                .locals
                .iter()
                .any(|local| local.name.as_deref() == Some(name.as_str()));
            (!has_unsupported_local).then_some(EvalScopeReadParamSource::Null)
        })
        .collect()
}

/// Returns true when constrained direct read params have compatible local sources.
pub(super) fn eval_scope_read_constraints_supported(
    ctx: &FunctionContext<'_>,
    array_read_constraints: &BTreeSet<String>,
    assoc_array_read_constraints: &BTreeSet<String>,
    float_predicate_read_constraints: &BTreeSet<String>,
) -> bool {
    let sync_locals = eval_sync_locals(ctx);
    array_read_constraints.iter().all(|name| {
        sync_locals
            .iter()
            .find(|local| local.name == *name)
            .is_some_and(|local| eval_scope_read_array_param_type_supported(&local.ty))
    }) && assoc_array_read_constraints.iter().all(|name| {
        sync_locals
            .iter()
            .find(|local| local.name == *name)
            .is_some_and(|local| eval_scope_read_assoc_array_param_type_supported(&local.ty))
    }) && float_predicate_read_constraints.iter().all(|name| {
        sync_locals
            .iter()
            .find(|local| local.name == *name)
            .is_some_and(|local| eval_scope_read_float_predicate_param_type_supported(&local.ty))
    })
}

/// Returns true when a direct read-param source has array-only semantics.
pub(super) fn eval_scope_read_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. }
    )
}

/// Returns true when a direct read-param source has associative-array-only semantics.
pub(super) fn eval_scope_read_assoc_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::AssocArray { .. })
}

/// Returns true when a direct read-param source can feed IEEE float predicates.
pub(super) fn eval_scope_read_float_predicate_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Int | PhpType::Float)
}

/// Emits one direct read-param value as a boxed Mixed result.
pub(super) fn emit_eval_scope_read_param_source(
    ctx: &mut FunctionContext<'_>,
    source: &EvalScopeReadParamSource,
) -> Result<()> {
    match source {
        EvalScopeReadParamSource::Local(local) => {
            let ty = ctx.load_local_to_result(local.slot)?.codegen_repr();
            if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
                emit_box_current_value_as_mixed(ctx.emitter, &ty);
            }
        }
        EvalScopeReadParamSource::Null => emit_core_mixed_null_cell(ctx),
    }
    Ok(())
}

/// Boxes PHP null with the core Mixed runtime helper.
pub(super) fn emit_core_mixed_null_cell(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #8");                              // materialize the core Mixed null runtime tag
            ctx.emitter.instruction("mov x1, #0");                              // null has no low payload word
            ctx.emitter.instruction("mov x2, #0");                              // null has no high payload word
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, 8");                              // materialize the core Mixed null runtime tag
            ctx.emitter.instruction("xor edi, edi");                            // null has no low payload word
            ctx.emitter.instruction("xor esi, esi");                            // null has no high payload word
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
}

/// Returns true when a static function call matches the EIR eval AOT codegen subset.
pub(super) fn eval_literal_static_function_supported_by_codegen(
    ctx: &FunctionContext<'_>,
    name: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let key = php_symbol_key(name.trim_start_matches('\\'));
    let Some(function) = ctx
        .module
        .functions
        .iter()
        .find(|function| php_symbol_key(function.name.trim_start_matches('\\')) == key)
    else {
        return false;
    };
    let signature = function_signature_from_eir(function);
    crate::eval_aot::static_function_signature_supported(&signature, args)
}

/// Returns true when a static method call matches the EIR eval AOT codegen subset.
pub(super) fn eval_literal_static_method_supported_by_codegen(
    ctx: &FunctionContext<'_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let StaticReceiver::Named(class_name) = receiver else {
        return false;
    };
    let class_name = class_name.as_str().trim_start_matches('\\');
    let method_key = php_symbol_key(method);
    let Some(receiver_info) = ctx.module.class_infos.get(class_name) else {
        return false;
    };
    if receiver_info
        .static_method_visibilities
        .get(&method_key)
        .unwrap_or(&Visibility::Public)
        != &Visibility::Public
    {
        return false;
    }
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    let Some(signature) = ctx
        .module
        .class_infos
        .get(impl_class)
        .and_then(|class_info| class_info.static_methods.get(&method_key))
    else {
        return false;
    };
    crate::eval_aot::static_function_signature_supported(signature, args)
}
