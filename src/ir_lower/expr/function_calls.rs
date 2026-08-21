//! Purpose:
//! Direct function and builtin call lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers a direct function, builtin, or extern call.
pub(super) fn lower_function_call(ctx: &mut LoweringContext<'_, '_>, name: &Name, args: &[Expr], expr: &Expr) -> LoweredValue {
    constants::register_static_define_call(ctx, name, args);
    if let Some(value) = constants::lower_static_defined_call(ctx, name, args, expr) {
        return value;
    }
    if let Some(value) = constants::lower_static_constant_call(ctx, name, args, expr) {
        return value;
    }
    let canonical = name.as_str();
    if let Some(value) = lower_lazy_isset(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_lazy_empty(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_desugared_dynamic_method_call(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_static_call_user_func(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_dynamic_call_user_func(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_dynamic_call_user_func_array(ctx, canonical, args, expr) {
        return value;
    }
    // A mutating builtin whose by-reference array argument is a property, static property, or
    // container element is rewritten to `$tmp = <place>; f($tmp, ...); <place> = $tmp;` before
    // any builtin fast path runs, so the rewritten call reaches the local-variable
    // by-reference lowering that actually stores the copy-on-write result back.
    if let Some(value) = ref_place_args::lower_builtin_ref_place_call(ctx, name, args, expr) {
        return value;
    }
    if let Some(value) = lower_static_array_map(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_static_array_reduce(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_static_array_walk(ctx, canonical, args, expr) {
        return value;
    }
    if php_symbol_key(canonical.trim_start_matches('\\')) == "unset" {
        if let Some(value) = lower_unset_locals(ctx, args, expr) {
            return value;
        }
    }
    if let Some(value) = lower_static_settype(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_static_array_push(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_array_internal_pointer(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_static_is_callable(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_eval_function_probe(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_eval_class_probe(ctx, canonical, args, expr) {
        return value;
    }
    let extension_builtin = source_prefers_extension_builtin(canonical);
    let sig = call_signature(ctx, canonical, extension_builtin);
    if let Some(opcode) = crate::ir_lower::internal_extensions::function_opcode(canonical) {
        let operands = lower_internal_extension_args(ctx, sig.as_ref(), args, false);
        let (operands, argument_guards) =
            prepare_internal_extension_arguments_for_throw(ctx, sig.as_ref(), operands, expr.span);
        let php_type = sig
            .as_ref()
            .map(|signature| normalize_value_php_type(signature.return_type.clone()))
            .unwrap_or_else(|| fallback_expr_type(expr));
        let call = crate::ir_lower::internal_extensions::emit_call(
            ctx,
            opcode,
            internal_extension_result_flags(&php_type),
            operands.clone(),
            php_type,
            expr.span,
        );
        clear_owning_call_arg_temporary_guards(ctx, &argument_guards, expr.span);
        release_owned_call_arg_temporaries(
            ctx,
            &operands,
            Some(call.value),
            &internal_extension_function_return_arg_alias(canonical),
            expr.span,
        );
        return call;
    }
    let is_extern = ctx.extern_functions.contains_key(canonical);
    let is_user_function = ctx.functions.contains_key(canonical) && !extension_builtin;
    let operands = if is_extern || is_user_function {
        lower_args_with_signature(ctx, sig.as_ref(), args)
    } else {
        lower_builtin_call_args(ctx, canonical, sig.as_ref(), args)
    };
    let php_type = if is_extern || is_user_function {
        call_return_type(ctx, canonical, &operands)
    } else if let Some(php_type) =
        registry_builtin_result_type(ctx, canonical, args, &operands, expr.span)
    {
        php_type
    } else {
        call_return_type(ctx, canonical, &operands)
    };
    if is_extern {
        let data = ctx.intern_function_name(canonical);
        let call = ctx.emit_value(
            Op::ExternCall,
            operands.clone(),
            Some(Immediate::Data(data)),
            php_type,
            Op::ExternCall.default_effects(),
            Some(expr.span),
        );
        // Plain extern calls release owned argument temporaries the same way method
        // and builtin calls do, so a fresh owned temporary passed as an argument is
        // not leaked once per call. The alias guard keeps a pass-through result alive.
        release_owned_call_arg_temporaries(
            ctx,
            &operands,
            Some(call.value),
            &ReturnArgAlias::Unknown,
            expr.span,
        );
        return call;
    }
    if is_user_function {
        let data = ctx.intern_function_name(canonical);
        let call = ctx.emit_value(
            Op::Call,
            operands.clone(),
            Some(Immediate::Data(data)),
            php_type,
            effects_lookup::user_call_effects(canonical),
            Some(expr.span),
        );
        // Plain user calls release owned argument temporaries the same way method and
        // builtin calls do. The alias guard keeps a passthrough result (e.g. a function
        // that returns its own array argument typed `iterable`) from being freed.
        let return_alias = ctx
            .return_alias_summaries
            .function(canonical)
            .cloned()
            .unwrap_or(ReturnArgAlias::Unknown);
        release_owned_call_arg_temporaries_with_signature(
            ctx,
            &operands,
            Some(call.value),
            &return_alias,
            sig.as_ref(),
            expr.span,
        );
        return call;
    }
    if ctx.has_eval_barrier()
        && plain_positional_call_args(args)
        && canonical_builtin_function_name(canonical).is_none()
    {
        let dynamic_name = php_symbol_key(canonical.trim_start_matches('\\'));
        let data = ctx.intern_function_name(&dynamic_name);
        return ctx.emit_value(
            Op::EvalFunctionCall,
            operands,
            Some(Immediate::Data(data)),
            PhpType::Mixed,
            Op::EvalFunctionCall.default_effects(),
            Some(expr.span),
        );
    }
    let eval_literal = eval_literal_fragment(canonical, args);
    emit_builtin_call_value(ctx, canonical, operands, php_type, expr.span, eval_literal)
}

/// Lowers bridge arguments while retaining PHP's named, variadic, and stringable contracts.
pub(super) fn lower_internal_extension_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
    preserve_omitted_defaults: bool,
) -> Vec<crate::ir::ValueId> {
    let lowering_signature = sig.map(internal_extension_argument_lowering_signature);
    let sig = lowering_signature.as_ref();
    if preserve_omitted_defaults && has_static_call_spread_args(args) {
        let expanded = expand_static_call_spread_args(args);
        return lower_internal_extension_args(ctx, sig, &expanded, true);
    }
    if preserve_omitted_defaults && !args.iter().any(is_spread_arg) {
        if crate::types::call_args::has_named_args(args) {
            let Some(signature) = sig else {
                return lower_args(ctx, args);
            };
            return coerce_operands_to_params(
                ctx,
                signature,
                lower_named_args_with_signature(ctx, signature, args),
            );
        }
        let operands = args
            .iter()
            .enumerate()
            .map(|(index, argument)| match sig {
                Some(signature) => lower_arg_with_signature(ctx, signature, index, argument),
                None => lower_expr(ctx, argument).value,
            })
            .collect::<Vec<_>>();
        return match sig {
            Some(signature) => coerce_operands_to_params(ctx, signature, operands),
            None => operands,
        };
    }
    if sig.is_some_and(|signature| {
        signature.variadic.is_some()
            && crate::types::call_args::regular_param_count(signature) == 0
    }) && !crate::types::call_args::has_named_args(args)
        && !args.iter().any(is_spread_arg)
    {
        return args.iter().map(|argument| lower_expr(ctx, argument).value).collect();
    }
    lower_args_with_signature(ctx, sig, args)
}

/// Defers stringable conversion to native preflight so bridge calls share one cleanup boundary.
fn internal_extension_argument_lowering_signature(signature: &FunctionSig) -> FunctionSig {
    let mut lowering_signature = signature.clone();
    for (_, parameter_type) in &mut lowering_signature.params {
        if parameter_type.codegen_repr() == PhpType::Str {
            *parameter_type = PhpType::Mixed;
        }
    }
    lowering_signature
}

/// Moves owned bridge arguments into cleanup-visible slots before native work can throw.
pub(super) fn prepare_internal_extension_arguments_for_throw(
    ctx: &mut LoweringContext<'_, '_>,
    _signature: Option<&FunctionSig>,
    args: Vec<crate::ir::ValueId>,
    span: Span,
) -> (Vec<crate::ir::ValueId>, Vec<(crate::ir::ValueId, String)>) {
    let mut guarded_values = Vec::new();
    let mut guards = Vec::new();
    for value in args.iter().copied() {
        if guarded_values.contains(&value) {
            continue;
        }
        let php_type = ctx.builder.value_php_type(value);
        let lowered = LoweredValue { value, ir_type: value_ir_type(&php_type) };
        let provisional_local_load = matches!(
            ctx.builder.value_defining_op(value),
            Some(Op::LoadLocal | Op::LoadStaticLocal)
        ) && !ctx.value_is_owned_temp_load(value);
        if provisional_local_load || !ctx.value_is_owning_temporary(lowered) {
            continue;
        }
        let guard = ctx.declare_owned_hidden_temp(php_type.clone());
        ctx.store_local(&guard, lowered, php_type, Some(span));
        guarded_values.push(value);
        guards.push((value, guard));
    }
    (args, guards)
}

/// Moves an owning bridge receiver into an unwind-visible slot before native work can throw.
pub(super) fn guard_owning_receiver_temporary_for_throw(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    span: Span,
) -> Option<String> {
    let provisional_local_load = matches!(
        ctx.builder.value_defining_op(receiver.value),
        Some(Op::LoadLocal | Op::LoadStaticLocal)
    ) && !ctx.value_is_owned_temp_load(receiver.value);
    if provisional_local_load || !ctx.value_is_owning_temporary(receiver) {
        return None;
    }
    let receiver_type = ctx.builder.value_php_type(receiver.value);
    let guard = ctx.declare_owned_hidden_temp(receiver_type.clone());
    ctx.store_local(&guard, receiver, receiver_type, Some(span));
    Some(guard)
}

/// Clears bridge-argument unwind guards after a successful native call.
pub(super) fn clear_owning_call_arg_temporary_guards(
    ctx: &mut LoweringContext<'_, '_>,
    guards: &[(crate::ir::ValueId, String)],
    span: Span,
) {
    for (_, guard) in guards {
        ctx.clear_owned_hidden_temp(guard, Some(span));
    }
}

/// Clears an owning receiver guard and releases the consumed temporary after native dispatch.
pub(super) fn release_guarded_owning_receiver_temporary(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    guard: Option<&str>,
    span: Span,
) {
    if let Some(guard) = guard {
        ctx.clear_owned_hidden_temp(guard, Some(span));
    }
    release_owning_receiver_temporary(ctx, receiver, span);
}

/// Returns bridge result flags inferred from the exact PHP result shape.
pub(super) fn internal_extension_result_flags(result_type: &PhpType) -> u32 {
    let mut flags = 0;
    let mut classify = |class_name: &str| {
        if crate::internal_extensions::is_native_wrapper_class(class_name) {
            flags |= crate::ir_lower::internal_extensions::FLAG_WRAPPER_RESULT;
        }
        if crate::internal_extensions::is_native_value_object_class(class_name) {
            flags |= crate::ir_lower::internal_extensions::FLAG_VALUE_OBJECT_RESULT;
        }
    };
    match result_type {
        PhpType::Object(class_name) => classify(class_name),
        PhpType::Union(members) => {
            for member in members {
                if let PhpType::Object(class_name) = member {
                    classify(class_name);
                }
            }
        }
        _ => {}
    }
    flags
}

/// Returns the bridge's proven return-to-argument alias contract for native functions.
fn internal_extension_function_return_arg_alias(name: &str) -> ReturnArgAlias {
    match php_symbol_key(name.trim_start_matches('\\')) .as_str() {
        "simplexml_import_dom" => ReturnArgAlias::None,
        _ => ReturnArgAlias::Unknown,
    }
}

/// Emits a builtin call and releases owned temporary arguments after the call consumes them.
pub(super) fn emit_builtin_call_value(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    operands: Vec<crate::ir::ValueId>,
    php_type: PhpType,
    span: Span,
    eval_literal: Option<&str>,
) -> LoweredValue {
    if eval_literal.is_none() {
        if let Some(def) = crate::builtins::registry::lookup(name) {
            let lowered = crate::builtins::semantics::lower_registry_call(
                ctx,
                def,
                &operands,
                &php_type,
                span,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "checked builtin {} failed backend-neutral EIR lowering at {}:{}: {}",
                    def.name,
                    span.line,
                    span.col,
                    error,
                )
            });
            let call = LoweredValue {
                value: lowered.value,
                ir_type: ctx.builder.value_type(lowered.value),
            };
            let return_alias = match def.spec.semantics.result_ownership {
                crate::builtins::semantics::BuiltinResultOwnership::NonHeap
                | crate::builtins::semantics::BuiltinResultOwnership::Fresh
                | crate::builtins::semantics::BuiltinResultOwnership::Independent => {
                    ReturnArgAlias::None
                }
                crate::builtins::semantics::BuiltinResultOwnership::Aliases(indexes) => {
                    ReturnArgAlias::Parameters(indexes.iter().copied().collect())
                }
                crate::builtins::semantics::BuiltinResultOwnership::Borrowed
                | crate::builtins::semantics::BuiltinResultOwnership::MayAliasArguments => {
                    ReturnArgAlias::Unknown
                }
            };
            release_owned_call_arg_temporaries(
                ctx,
                &operands,
                Some(call.value),
                &return_alias,
                span,
            );
            return call;
        }
    }
    let (op, immediate, effects) = if let Some(fragment) = eval_literal {
        (
            Op::EvalLiteralCall,
            Some(Immediate::ProfiledData {
                data: ctx.intern_string(fragment),
                strict_php: crate::strict_php::is_enabled(),
            }),
            Op::EvalLiteralCall.default_effects(),
        )
    } else {
        let data = ctx.intern_function_name(name);
        let immediate = if php_symbol_key(name.trim_start_matches('\\')) == "eval" {
            Immediate::ProfiledData {
                data,
                strict_php: crate::strict_php::is_enabled(),
            }
        } else {
            Immediate::Data(data)
        };
        (
            Op::LanguageConstructCall,
            Some(immediate),
            effects_lookup::language_construct_effects(name),
        )
    };
    let call = ctx.emit_value(
        op,
        operands.clone(),
        immediate,
        php_type,
        effects,
        Some(span),
    );
    release_owned_call_arg_temporaries(
        ctx,
        &operands,
        Some(call.value),
        &ReturnArgAlias::Unknown,
        span,
    );
    let eval_needs_barrier = match eval_literal {
        Some(fragment) => eval_literal_needs_barrier(ctx, fragment),
        None => true,
    };
    if php_symbol_key(name.trim_start_matches('\\')) == "eval" {
        ctx.mark_eval_executed();
        if eval_needs_barrier {
            ctx.apply_eval_barrier();
        } else if let Some(write_names) = eval_literal
            .and_then(|fragment| eval_literal_scope_barrier_writes(ctx, fragment))
        {
            ctx.apply_eval_scope_barrier(&write_names);
        }
    }
    call
}

/// Resolves a migrated registry builtin's result type from the same descriptor as the checker.
///
/// Used for a builtin lowered at its own call site, so the checker's per-span result type is
/// authoritative and is passed through to the resolver.
pub(super) fn registry_builtin_result_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    operands: &[crate::ir::ValueId],
    span: Span,
) -> Option<PhpType> {
    // Synthetic builtin-class and prelude AST nodes share the dummy 0:0
    // span, so the checker map cannot identify an individual call there.
    // Use the typed runtime target's representation-safe fallback instead
    // of accepting whichever synthetic call last occupied that key.
    let checked = if span.line != 0 {
        ctx.builtin_call_types
            .get(&span)
            .map(|checked| normalize_value_php_type(checked.clone()))
    } else {
        None
    };
    resolve_registry_builtin_result_type(ctx, name, args, operands, span, checked)
}

/// Is a builtin's declared return type an acceptable stand-in for what the checker inferred?
///
/// Narrowing is fine in one direction only. A checked `int` falling back to a declared `mixed`
/// costs precision: `mixed` is the universal boxed representation of a PHP *value*, and lowering
/// knows how to carry any value in one. A checked `pointer` or `buffer` falling back to `mixed`
/// is not a loss of precision but a change of REPRESENTATION — a raw descriptor is not a boxed
/// cell — and codegen has no way to notice. Six builtins declared `mixed` while checking as
/// `Pointer` or `Callable`, which stayed harmless only for as long as every one of their call
/// sites could be found in the checker's per-span map; the day the PDO prelude stopped being
/// parsed, none of them could be, and all 276 PDO tests failed at once.
fn declared_type_is_a_safe_fallback(declared: &PhpType, checked: Option<&PhpType>) -> bool {
    let Some(checked) = checked else {
        return true;
    };
    fn is_a_php_value(ty: &PhpType) -> bool {
        !matches!(
            ty,
            PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) | PhpType::Callable
        )
    }
    is_a_php_value(checked) == is_a_php_value(declared)
}

/// Resolves a registry builtin's result type from its descriptor, its lowered operands, and the
/// checker's result type for this very call when one is available.
///
/// `checked` must be `None` whenever the caller cannot prove the checker examined *this* builtin
/// at `span`; the resolver then derives a representation-safe type from the typed runtime target
/// instead of trusting a type that may describe a different call.
pub(super) fn resolve_registry_builtin_result_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    operands: &[crate::ir::ValueId],
    span: Span,
    checked: Option<PhpType>,
) -> Option<PhpType> {
    let def = crate::builtins::registry::lookup(name)?;
    debug_assert!(
        declared_type_is_a_safe_fallback(&def.return_type, checked.as_ref()),
        "builtin `{name}` checks as {:?} but declares {:?}. Every path that cannot name this \
         call — a callable dispatched at runtime, a span the checker never filed — falls back to \
         the DECLARED type, so a declaration of a different representation is a miscompile \
         waiting for one of them.",
        checked,
        def.return_type,
    );
    let arg_types = operands
        .iter()
        .map(|operand| ctx.builder.value_php_type(*operand))
        .collect::<Vec<_>>();
    let input = crate::builtins::semantics::BuiltinSemanticInput {
        name: def.name,
        args,
        arg_types: &arg_types,
        span,
    };
    let resolved = match def.spec.semantics.result_type {
        crate::builtins::semantics::BuiltinResultType::Checked => {
            let crate::builtins::semantics::BuiltinLowering::Runtime(
                crate::ir::RuntimeCallTarget::Function(target),
            ) = def.spec.semantics.lowering
            else {
                return checked;
            };
            // The checker types an untyped user-function parameter from its call sites, while EIR
            // gives it the dynamic boxed-Mixed ABI contract, so a checked type can describe a
            // narrower element layout than the operands actually carry. A runtime target that
            // copies an argument's layout rejects such a type and re-derives its own.
            if let Some(checked) = checked {
                if target.checked_result_type_fits_operands(&arg_types, &checked) {
                    return Some(checked);
                }
            }
            target.fallback_result_type(&arg_types, &def.return_type)
        }
        crate::builtins::semantics::BuiltinResultType::Declared => def.return_type.clone(),
        crate::builtins::semantics::BuiltinResultType::Shared(resolve) => resolve(&input),
    };
    Some(normalize_value_php_type(resolved))
}
