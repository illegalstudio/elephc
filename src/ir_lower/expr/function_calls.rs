//! Purpose:
//! Direct function and builtin call lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers a socket-connect builtin whose `$error_code`/`$error_message` outputs are places
/// rather than variables (`@pfsockopen($host, -1, $this->errno, $this->errstr, $t)` in Monolog's
/// `SocketHandler`): the builtin writes its outputs into local slots only, so each place gets a
/// hidden local for the call and is assigned from it afterwards, as php's reference would leave
/// it. Returns `None` for any other call.
fn lower_socket_call_with_place_outputs(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let outputs: [usize; 2] = match php_symbol_key(name.as_str().trim_start_matches('\\')).as_str() {
        "fsockopen" | "pfsockopen" => [2, 3],
        "stream_socket_client" => [1, 2],
        _ => return None,
    };
    let is_place = |arg: &Expr| {
        matches!(
            arg.kind,
            ExprKind::PropertyAccess { .. } | ExprKind::StaticPropertyAccess { .. } | ExprKind::ArrayAccess { .. }
        )
    };
    if !outputs.iter().any(|&index| args.get(index).is_some_and(is_place)) {
        return None;
    }
    let mut call_args = args.to_vec();
    let mut write_backs = Vec::new();
    for (position, &index) in outputs.iter().enumerate() {
        let Some(place) = args.get(index).filter(|arg| is_place(arg)) else {
            continue;
        };
        let (ty, initial) = if position == 0 {
            (PhpType::Int, ExprKind::IntLiteral(0))
        } else {
            (PhpType::Str, ExprKind::StringLiteral(String::new()))
        };
        let temp = ctx.declare_synthetic_php_local(ty);
        crate::ir_lower::stmt::lower_stmt(
            ctx,
            &Stmt::new(StmtKind::Assign { name: temp.clone(), value: Expr::new(initial, place.span) }, place.span),
        );
        let temp_expr = Expr::new(ExprKind::Variable(temp), place.span);
        call_args[index] = temp_expr.clone();
        write_backs.push((place.clone(), temp_expr));
    }
    let value = lower_function_call(ctx, name, &call_args, expr);
    for (place, temp) in write_backs {
        let store = match place.kind {
            ExprKind::PropertyAccess { object, property } => {
                StmtKind::PropertyAssign { object, property, value: temp }
            }
            ExprKind::StaticPropertyAccess { receiver, property } => {
                StmtKind::StaticPropertyAssign { receiver, property, value: temp }
            }
            ExprKind::ArrayAccess { array, index } => match array.kind {
                ExprKind::Variable(array) => StmtKind::ArrayAssign { array, index: *index, value: temp },
                kind => StmtKind::NestedArrayAssign {
                    target: Expr::new(
                        ExprKind::ArrayAccess { array: Box::new(Expr::new(kind, array.span)), index },
                        place.span,
                    ),
                    value: temp,
                },
            },
            _ => continue,
        };
        crate::ir_lower::stmt::lower_stmt(ctx, &Stmt::new(store, expr.span));
    }
    Some(value)
}

/// Lowers a direct function, builtin, or extern call.
pub(super) fn lower_function_call(ctx: &mut LoweringContext<'_, '_>, name: &Name, args: &[Expr], expr: &Expr) -> LoweredValue {
    if let Some(value) = lower_socket_call_with_place_outputs(ctx, name, args, expr) {
        return value;
    }
    super::late_bound_call::guard_runtime_function_binding(ctx, name.as_str(), expr);
    if let Some(value) = constants::lower_dynamic_constant_call(ctx, name, args, expr) {
        return value;
    }
    constants::register_static_define_call(ctx, name, args);
    if let Some(value) = constants::lower_static_defined_call(ctx, name, args, expr) {
        return value;
    }
    if let Some(value) = constants::lower_static_constant_call(ctx, name, args, expr) {
        return value;
    }
    // A generic call names its TEMPLATE in the source; the checker already decided which
    // monomorphic instantiation this particular call resolves to, from argument types lowering
    // cannot consult before it has a signature. Swap in that name before anything else looks at
    // it, so every path below — the introspection shortcuts included — sees an ordinary user
    // function.
    // Keyed by the body's loop-storage scope, which the checker spells the same way; the EIR
    // owner name differs from it inside a closure (`__eir_closure_…`).
    let instantiated = ctx
        .generic_call_sites
        .get(&(ctx.loop_storage_scope.clone(), expr.span))
        .cloned();
    let canonical = instantiated.as_deref().unwrap_or_else(|| name.as_str());
    if canonical == crate::names::DYNAMIC_INCLUDE_FUNCTION {
        let [path, once, required] = args else {
            panic!("checked internal dynamic include must carry three operands");
        };
        let ExprKind::BoolLiteral(once) = &once.kind else {
            panic!("checked internal dynamic include once flag must be a boolean literal");
        };
        let ExprKind::BoolLiteral(required) = &required.kind else {
            panic!("checked internal dynamic include required flag must be a boolean literal");
        };
        let path = lower_expr(ctx, path);
        let call = ctx.emit_value(
            Op::RuntimeCall,
            vec![path.value],
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::DynamicInclude {
                once: *once,
                required: *required,
                strict_php: crate::strict_php::is_enabled(),
                // An include in expression position: its value is the point of writing it.
                value_discarded: false,
            })),
            PhpType::Mixed,
            effects_lookup::runtime_effects(),
            Some(expr.span),
        );
        release_owned_call_arg_temporaries(
            ctx,
            &[path.value],
            Some(call.value),
            &ReturnArgAlias::None,
            expr.span,
        );
        // A closed-world include runs a compiled unit in its own scope (or none at all), so the
        // caller's locals cannot change and no interpreter context is needed around it.
        if !crate::closed_world::is_enabled() {
            ctx.mark_eval_executed();
            ctx.apply_eval_barrier();
        }
        return call;
    }
    if let Some(value) = lower_class_introspection(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_get_defined_vars(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = super::core_introspection::lower_closed_world_extract(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(rejection) = inaccessible_callback_rejection(ctx, canonical, args, expr.span) {
        // The throw ends the block, but the call's own consumers still need a value of a type
        // they accept, and a bare `throw` yields `void`. A boxed null is the gradual stand-in.
        let thrown = lower_expr(ctx, &rejection);
        return ctx.box_value_as_mixed(thrown, PhpType::Mixed, Some(expr.span));
    }
    let scoped_callback_args =
        invoke_only_callback_args_with_static_method_callables(ctx, canonical, args);
    let args = scoped_callback_args.as_deref().unwrap_or(args);
    if let Some(rewritten) = printf_spread_as_values_call(ctx, canonical, args, expr) {
        return lower_expr(ctx, &rewritten);
    }
    if let Some(rewritten) = printf_leading_spread_as_values_call(ctx, canonical, args, expr) {
        return lower_expr(ctx, &rewritten);
    }
    let listed_values_args = vsprintf_listed_values_args(ctx, canonical, args);
    let args = listed_values_args.as_deref().unwrap_or(args);
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
    if let Some(value) = boxed_user_sort::lower_boxed_usort(ctx, canonical, args, expr) {
        return value;
    }
    // A call whose by-reference array argument is a property, static property, or container
    // element is rewritten to `$tmp = <place>; f($tmp, ...); <place> = $tmp;` before argument
    // materialization, so copy-on-write separation is stored back into the caller's place.
    if let Some(value) = ref_place_args::lower_ref_place_function_call(ctx, name, args, expr) {
        return value;
    }
    if let Some(value) = lower_gradual_local_krsort(ctx, canonical, args, expr) {
        return value;
    }
    if php_symbol_key(canonical.trim_start_matches('\\')) == "mb_convert_variables"
        && args.iter().any(is_spread_arg)
        && !ctx.functions.contains_key(canonical)
        && !ctx.extern_functions.contains_key(canonical)
    {
        let callback = Expr::new(ExprKind::StringLiteral("mb_convert_variables".to_string()), expr.span);
        let sig = crate::types::first_class_callable_builtin_sig("mb_convert_variables");
        return lower_call_user_func_descriptor_invoke(ctx, &callback, args, sig.as_ref(), expr)
            .expect("mb_convert_variables descriptor invocation");
    }
    if let Some(value) =
        compat_preludes::lower_single_arg_assert(ctx, canonical, args, expr)
    {
        return value;
    }
    if let Some(value) =
        compat_preludes::lower_is_callable_with_options(ctx, canonical, args, expr)
    {
        return value;
    }
    if let Some(value) = compat_preludes::lower_unpack_whole_hex(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) =
        gradual_array_calls::lower_gradual_array_call(ctx, canonical, args, expr)
    {
        return value;
    }
    if let Some(value) =
        compat_preludes::lower_backend_gap_builtin_shape(ctx, canonical, args, expr)
    {
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
    if let Some(value) = array_push_spread::lower_array_push_spread(ctx, name, args, expr) {
        return value;
    }
    if let Some(value) = lower_static_array_push(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) =
        compat_preludes::lower_variadic_array_replace(ctx, canonical, args, expr)
    {
        return value;
    }
    if let Some(value) =
        compat_preludes::lower_gradual_array_merge(ctx, canonical, args, expr)
    {
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
    if let Some(value) = lower_static_filter_var(ctx, canonical, args, expr) {
        return value;
    }
    let extension_builtin = source_prefers_extension_builtin(canonical);
    let sig = call_signature(ctx, canonical, extension_builtin);
    let is_extern = ctx.extern_functions.contains_key(canonical);
    let is_user_function = ctx.functions.contains_key(canonical) && !extension_builtin;
    if is_user_function {
        if let Some(signature) = sig.as_ref() {
            if let Some(rewrite) =
                ref_place_args::rewrite_typed_property_ref_args(ctx, signature, args)
            {
                let previous = ctx.call_description.replace(canonical.to_string());
                let result = lower_function_call(ctx, name, &rewrite.args, expr);
                ctx.call_description = previous;
                ref_place_args::finish_ref_place_rewrite(ctx, rewrite, expr.span);
                return result;
            }
        }
    }
    let is_builtin =
        crate::types::checker::builtins::is_supported_builtin_function(canonical);
    if !is_extern && !is_user_function && !is_builtin {
        // A web handler can load a PHP file dynamically during request bootstrap. Such a file may
        // declare a function that a separately lowered AOT method calls later in the same request,
        // so preserve the call for the eval function-owner bridge instead of freezing an undefined
        // function Error before that file has executed. Non-web programs retain the eager PHP
        // undefined-call path unless their own frame already crossed an eval barrier.
        if !ctx.can_resolve_runtime_dynamic_functions() && !ctx.has_eval_barrier() {
            if let Some(value) =
                super::late_bound_call::lower_late_bound_undefined_call(ctx, canonical, expr)
            {
                return value;
            }
        }
    }
    // A function this program declares cannot be replaced by a runtime include or eval, so its
    // spread call keeps the native named-argument lowering: routing it through the function-owner
    // bridge made even a literal `eval('f(...["b" => 1])')` need the interpreter.
    if (ctx.has_eval_barrier() || ctx.can_resolve_runtime_dynamic_functions())
        && !is_user_function
        && !is_extern
        && dynamic_function_owner_fallback_candidate(canonical)
        && crate::builtins::registry::lookup(canonical).is_none()
    {
        if let Some(value) = lower_dynamic_function_owner_spread_call(ctx, canonical, args, expr) {
            return value;
        }
    }
    if !is_extern && !is_user_function {
        if let Some(call) = lower_packed_builtin_call(ctx, canonical, sig.as_ref(), args, expr) {
            return call;
        }
    }
    // A by-reference-returning callee hands back a lease that has to survive this caller's own
    // cleanup, so its staging is published before the arguments are evaluated. Only the direct
    // user-call branch below transfers a cell; an extern, builtin or eval-dispatched call with
    // the same name returns an ordinary value and must not publish a record nothing retires.
    let reference_staging = if is_user_function && !is_extern {
        begin_reference_return_call(ctx, sig.as_ref(), expr.span)
    } else {
        None
    };
    // A fresh owned result is owned by nothing the unwind chain can see while the argument
    // roots, the evaluation intermediates and their PHP destructors retire, so its staging is
    // published here too, OUTSIDE every root those steps publish. `call_return_type` reads the
    // signature alone, so the type declared here is the exact type the call is emitted with.
    let user_return_alias = is_user_function.then(|| {
        ctx.return_alias_summaries
            .function(canonical)
            .cloned()
            .unwrap_or(ReturnArgAlias::Unknown)
    });
    let result_staging = match (is_user_function && !is_extern, user_return_alias.as_ref()) {
        (true, Some(return_alias)) => prepublish_user_call_result(
            ctx,
            sig.as_ref(),
            return_alias,
            &call_return_type(ctx, canonical, &[]),
            expr.span,
        ),
        _ => None,
    };
    if !is_extern
        && (is_user_function
            || builtin_takes_array_by_reference_first(&php_symbol_key(
                canonical.trim_start_matches('\\'),
            )))
    {
        if let Some(call) =
            lower_by_ref_call_through_temps(ctx, name, sig.as_ref(), args, expr, is_user_function)
        {
            return call;
        }
    }
    // A by-value object parameter can reject a `Class|false`/`Class|null` argument at
    // binding time. Name the callee here so that guard's TypeError uses PHP's
    // `take(): Argument #1 ...` spelling instead of the generic `call()` fallback.
    let previous_description = is_user_function
        .then(|| ctx.call_description.replace(canonical.to_string()));
    begin_call_argument_evaluation(ctx);
    let mut operands = if is_user_function {
        // A source-declared `array` has packed-or-hash Mixed storage. Its unpack must walk
        // runtime keys and bind each boxed cell before entering the direct function ABI, just
        // like the same fixed signature reached through a builtin descriptor surface.
        sig.as_ref()
            .and_then(|signature| {
                dynamic_spreads::lower_boxed_spread_args(ctx, signature, args, canonical, false)
            })
            .unwrap_or_else(|| lower_user_args_with_signature(ctx, sig.as_ref(), args))
    } else if is_extern {
        lower_args_with_signature(ctx, sig.as_ref(), args)
    } else {
        let mut operands = lower_builtin_call_args(ctx, canonical, sig.as_ref(), args);
        narrow_single_array_min_max_operand(ctx, canonical, &mut operands, expr.span);
        operands
    };
    if !is_extern && !is_user_function {
        box_nullable_pointer_builtin_operands(ctx, sig.as_ref(), &mut operands, expr.span);
        retype_array_push_values_for_pointer_storage(ctx, canonical, &mut operands, expr.span);
    }
    // A packed record address or a `null` passed where the signature (extern, builtin or user)
    // declares a pointer parameter becomes that pointer word; for `?ptr<T>` a null is the word
    // 0. The declared parameter type decides, never the name. By-reference parameters receive
    // the caller's storage instead.
    if let Some(signature) = sig.as_ref() {
        for (index, (operand, (_, expected))) in
            operands.iter_mut().zip(&signature.params).enumerate()
        {
            if signature.ref_params.get(index).copied().unwrap_or(false) {
                continue;
            }
            let value = lowered_value_from_id(ctx, *operand);
            *operand = ctx
                .retype_packed_address_for_pointer_sink(value, expected, Some(expr.span))
                .value;
        }
    }
    if let Some(previous) = previous_description {
        ctx.call_description = previous;
    }
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
        let evaluation_intermediates = finish_call_argument_evaluation(ctx, &mut operands);
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
        // not leaked once per call. An extern's result never aliases a PHP argument: a
        // string crosses the C ABI as a NUL-terminated copy (`__rt_str_to_cstr`) and comes
        // back as a fresh string (`__rt_cstr_to_str`), and the other C types are not
        // refcounted. With `Unknown` here the guard kept every pinned string argument alive,
        // one leaked block per call (`highlight_string()` through `elephc_highlight_render`).
        // Scalar and pointer results own no PHP argument storage either.
        release_owned_call_arg_temporaries(
            ctx,
            &operands,
            Some(call.value),
            &ReturnArgAlias::None,
            expr.span,
        );
        retire_call_argument_intermediates(ctx, &evaluation_intermediates);
        return call;
    }
    if is_user_function {
        let return_alias = user_return_alias
            .expect("a user function call resolved its return-alias summary before its arguments");
        let evaluation_intermediates = finish_user_call_argument_evaluation(ctx, &mut operands);
        let roots = root_user_call_operands(
            ctx, &mut operands, sig.as_ref(), &return_alias, &php_type, expr.span,
        );
        let data = ctx.intern_function_name(canonical);
        let call = ctx.emit_value(
            Op::Call,
            operands.clone(),
            Some(Immediate::Data(data)),
            php_type,
            effects_lookup::user_call_effects(canonical),
            Some(expr.span),
        );
        ctx.invalidate_callable_user_function(canonical, sig.as_ref(), &operands);
        // Plain user calls release owned argument temporaries the same way method and
        // builtin calls do. The alias guard keeps a passthrough result (e.g. a function
        // that returns its own array argument typed `iterable`) from being freed.
        let call = finish_reference_return_call(
            ctx, call, sig.as_ref(), reference_staging.as_ref(), expr.span,
        );
        stage_call_result(ctx, result_staging.as_ref(), call, expr.span);
        release_owned_call_arg_temporaries_with_roots(
            ctx, &operands, Some(call.value), &return_alias, sig.as_ref(), &roots, expr.span,
        );
        retire_call_argument_intermediates(ctx, &evaluation_intermediates);
        let call = take_prepublished_call_result(ctx, result_staging, call, expr.span);
        return finish_reference_return_value(ctx, call, reference_staging, expr.span);
    }
    if (ctx.has_eval_barrier() || ctx.can_resolve_runtime_dynamic_functions())
        && plain_positional_call_args(args)
        && dynamic_function_owner_fallback_candidate(canonical)
        && crate::builtins::registry::lookup(canonical).is_none()
    {
        let evaluation_intermediates = finish_call_argument_evaluation(ctx, &mut operands);
        let dynamic_name = php_symbol_key(canonical.trim_start_matches('\\'));
        let data = ctx.intern_function_name(&dynamic_name);
        let call = ctx.emit_value(
            Op::EvalFunctionCall,
            operands.clone(),
            Some(Immediate::Data(data)),
            PhpType::Mixed,
            Op::EvalFunctionCall.default_effects(),
            Some(expr.span),
        );
        // The bridge boxes each operand with its own reference and answers with a retained
        // cell (as for `EvalFunctionCallArray`), so an argument temporary is still ours.
        release_owned_call_arg_temporaries(
            ctx,
            &operands,
            Some(call.value),
            &ReturnArgAlias::None,
            expr.span,
        );
        retire_call_argument_intermediates(ctx, &evaluation_intermediates);
        return call;
    }
    let eval_literal = eval_literal_fragment(canonical, args);
    // A closed-world build has no interpreter to evaluate a computed string with: the code
    // expression has been evaluated above (its side effects stand), then the eval itself raises.
    if eval_literal.is_none()
        && crate::closed_world::is_enabled()
        && php_symbol_key(canonical.trim_start_matches('\\')) == "eval"
    {
        finish_call_argument_evaluation(ctx, &mut operands);
        return super::late_bound_call::lower_closed_world_eval_unavailable(ctx, expr);
    }
    let evaluation_intermediates = finish_call_argument_evaluation(ctx, &mut operands);
    let call = emit_builtin_call_value(ctx, canonical, operands, php_type, expr.span, eval_literal);
    retire_call_argument_intermediates(ctx, &evaluation_intermediates);
    unbox_raw_word_builtin_result(ctx, call, expr.span)
}

/// Returns a boxed builtin result as the `?ptr<T>` or `buffer<T>` word the checker typed the
/// call as.
///
/// `array_pop()` and `array_shift()` hand every element out boxed, a pointer or buffer element
/// as an integer-like cell and the null word (or an empty array) as PHP null, while the checker
/// types a pointer element as `?ptr<T>` and a buffer element as `buffer<T>`. The box is cast
/// back to the word (null casts to 0: the null pointer, or the invalid buffer handle) so field
/// access, comparisons, `?ptr<T>` parameters and buffer element reads see the word.
fn unbox_raw_word_builtin_result(
    ctx: &mut LoweringContext<'_, '_>,
    call: LoweredValue,
    span: Span,
) -> LoweredValue {
    if span.line == 0 {
        return call;
    }
    let Some(checked) = ctx.builtin_call_type_at(span).cloned() else {
        return call;
    };
    let raw_word = crate::types::is_nullable_pointer_type(&checked)
        || matches!(checked, PhpType::Buffer(_));
    if !raw_word || ctx.builder.value_php_type(call.value).codegen_repr() != PhpType::Mixed {
        return call;
    }
    let word = coerce_to_int_at_span(ctx, call, Some(span));
    ctx.emit_value(Op::Move, vec![word.value], None, checked, Op::Move.default_effects(), Some(span))
}


/// Returns whether a name can resolve through a function declared by runtime include/eval.
///
/// A checker-recognized PHP function is not necessarily backed by a native EIR implementation:
/// compatibility declarations such as `trigger_deprecation()` may instead arrive from an included
/// PHP polyfill. Keep true language constructs on their dedicated lowering paths, but let every
/// other unresolved/registry-free name consult the request-local dynamic function owner first.
fn dynamic_function_owner_fallback_candidate(name: &str) -> bool {
    !matches!(
        php_symbol_key(name.trim_start_matches('\\')).as_str(),
        "eval"
            | "empty"
            | "unset"
            | "isset"
            | "exit"
            | "die"
            | "filter_var$default"
            | "filter_var$default_nof"
            | "filter_var$int"
            | "filter_var$int_nof"
            | "filter_var$int_range"
            | "filter_var$int_range_nof"
            | "filter_var$float"
            | "filter_var$float_nof"
            | "filter_var$bool"
            | "filter_var$bool_nof"
            | "filter_var$ip"
            | "filter_var$ip_nof"
            | "filter_var$ip4"
            | "filter_var$ip4_nof"
            | "filter_var$ip6"
            | "filter_var$ip6_nof"
    )
}

/// Lowers a single dynamic positional spread through Magician's function-array ABI.
///
/// `$function(...$args)` preserves the source array's positional/named keys, so it cannot use the
/// fixed native argument ABI. A runtime-declared function receives the original boxed container
/// and applies PHP's call-argument rules inside the eval owner context.
fn lower_dynamic_function_owner_spread_call(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let [Expr {
        kind: ExprKind::Spread(container),
        ..
    }] = args
    else {
        return None;
    };
    let dynamic_name = php_symbol_key(canonical.trim_start_matches('\\'));
    let data = ctx.intern_function_name(&dynamic_name);
    let container = lower_expr(ctx, container);
    let boxed_container =
        super::callable_probes::coerce_eval_function_arg_array(ctx, container, expr.span);
    let result = ctx.emit_value(
        Op::EvalFunctionCallArray,
        vec![boxed_container.value],
        Some(Immediate::Data(data)),
        PhpType::Mixed,
        Op::EvalFunctionCallArray.default_effects(),
        Some(expr.span),
    );
    release_owned_call_arg_temporaries(
        ctx,
        &[boxed_container.value],
        Some(result.value),
        &ReturnArgAlias::None,
        expr.span,
    );
    Some(result)
}

/// Promotes a boxed gradual local before descending key sort and republishes it afterwards.
///
/// A descending key order cannot live in packed storage. `MixedToHash` produces the mutable hash
/// passed to the runtime sort; after the call, the same SSA value contains any COW-adjusted hash
/// pointer and is boxed back into the local's original gradual slot. This also lets the enclosing
/// ref-place adapter write a valid boxed value back to a property or nested element.
fn lower_gradual_local_krsort(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(canonical.trim_start_matches('\\')) != "krsort" || args.len() != 1 {
        return None;
    }
    let receiver = match &args[0].kind {
        ExprKind::Variable(name) => Some((name.as_str(), args[0].span)),
        ExprKind::NamedArg { name, value } if name == "array" => match &value.kind {
            ExprKind::Variable(local) => Some((local.as_str(), value.span)),
            _ => None,
        },
        _ => None,
    }?;
    let (name, span) = receiver;
    if !ctx.has_local_slot(name)
        || !matches!(
            ctx.local_type(name).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    {
        return None;
    }

    let source = ctx.load_local(name, Some(span));
    let hash_type = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };
    let hash = ctx.emit_value(
        Op::MixedToHash,
        vec![source.value],
        None,
        hash_type,
        Op::MixedToHash.default_effects(),
        Some(span),
    );
    if ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }

    // The hash key-sort backend sorts its receiver IN PLACE through a local slot (it may have to
    // separate a shared table and publish the copy), so the converted hash is parked in a hidden
    // local, sorted there, and then stored back into the boxed caller local. Handing it the bare
    // conversion result was refused: "hash link sort for a by-reference receiver that is not a
    // local variable slot".
    let hash_type = ctx.builder.value_php_type(hash.value);
    let temp = ctx.declare_synthetic_php_local(hash_type.clone());
    ctx.store_local(&temp, hash, hash_type.clone(), Some(span));
    let receiver = ctx.load_local(&temp, Some(span));
    let definition = crate::builtins::registry::lookup("krsort")?;
    let lowered = crate::builtins::semantics::lower_registry_call(
        ctx,
        definition,
        &[receiver.value],
        &PhpType::Bool,
        expr.span,
    )
    .ok()?;
    let call = LoweredValue {
        value: lowered.value,
        ir_type: ctx.builder.value_type(lowered.value),
    };
    let sorted = lower_expr(ctx, &Expr::new(ExprKind::Variable(temp.clone()), span));
    ctx.store_local(name, sorted, PhpType::Mixed, Some(span));
    let slot = ctx.declare_local(&temp, hash_type);
    ctx.emit_void(
        Op::ReleaseLocalSlot,
        Vec::new(),
        Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(),
        Some(span),
    );
    Some(call)
}

/// Copies `substr()`'s result when it views an owned temporary argument, so that argument can go.
///
/// `substr()` is lowered inline as pointer arithmetic: its result is ALWAYS a view into its
/// subject, never a string of its own. Over a borrowed subject -- a local -- the view costs
/// nothing. Over an OWNED temporary one -- the copy a `mixed` cast made, another call's result --
/// the alias kept the argument from being released, and releasing the view cannot free it unless
/// the view starts at its first byte: `substr($method, 3)` over a `mixed` `$method` leaked the
/// cast's copy on every call. Persisting the result makes it independent of every argument, so all
/// of them are released as for a fresh result. Only a builtin whose result is never a fresh
/// string qualifies: `str_replace()` may return a new one, which the copy would leak.
///
/// The trim family is the same shape: `__rt_ltrim`/`__rt_rtrim` and their mask forms only move
/// the subject's pointer and length and never allocate, so `ltrim($mixed, "\\")` leaked the
/// cast's copy on every call exactly as `substr()` did.
///
/// `dirname()` and `basename()` too: `__rt_dirname` (and `__rt_dirname_levels`, which only
/// repeats it) and `__rt_basename` return a slice of the path or a static string. Here the
/// failure was the opposite one: the cast's copy WAS released, before the result was
/// persisted, so `dirname($pathOrFalse)` returned freed bytes. Symfony's
/// `getProjectDir()` takes `dirname()` of `ReflectionObject::getFileName()` (`string|false`), and
/// the corrupted project dir sent every request into a container rebuild in a missing directory.
fn persist_string_view_of_owned_args(
    ctx: &mut LoweringContext<'_, '_>,
    def: &crate::builtins::registry::BuiltinDef,
    call: LoweredValue,
    return_alias: ReturnArgAlias,
    operands: &[crate::ir::ValueId],
    span: Span,
) -> (LoweredValue, ReturnArgAlias) {
    if !matches!(
        php_symbol_key(def.name).as_str(),
        "substr" | "trim" | "ltrim" | "rtrim" | "chop" | "dirname" | "basename"
    ) || ctx.builder.value_php_type(call.value).codegen_repr() != PhpType::Str
    {
        return (call, return_alias);
    }
    let views_owned_string = operands.iter().enumerate().any(|(index, operand)| {
        let php_type = ctx.builder.value_php_type(*operand);
        php_type.codegen_repr() == PhpType::Str
            && return_alias.may_alias_parameter(index)
            && ctx.value_is_owning_temporary(LoweredValue {
                value: *operand,
                ir_type: value_ir_type(&php_type),
            })
    });
    if !views_owned_string {
        return (call, return_alias);
    }
    let call = crate::ir_lower::ownership::acquire_if_refcounted(ctx, call, Some(span));
    (call, ReturnArgAlias::None)
}

/// Retypes the values of an `array_push()` call into pointer storage, as `$a[] = ...` does.
///
/// The builtin is recognized by its runtime target, never its name. A packed record address
/// becomes the `ptr<T>` word, and `null` the word 0 when the receiver already stores pointers;
/// the receiver's own type decides, so a record address pushed into an `int` array is not
/// turned into a pointer slot.
fn retype_array_push_values_for_pointer_storage(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    operands: &mut [crate::ir::ValueId],
    span: Span,
) {
    let is_array_push = crate::builtins::registry::lookup(canonical.trim_start_matches('\\'))
        .is_some_and(|def| {
            matches!(
                def.spec.semantics.lowering,
                crate::builtins::semantics::BuiltinLowering::Runtime(
                    crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::ArrayPush)
                )
            )
        });
    let Some((receiver, values)) = operands.split_first_mut().filter(|_| is_array_push) else {
        return;
    };
    let container_ty = ctx.builder.value_php_type(*receiver);
    for value in values {
        let lowered = lowered_value_from_id(ctx, *value);
        *value = ctx
            .retype_value_for_container_element(&container_ty, lowered, Some(span))
            .value;
    }
}

/// Boxes each `?ptr<T>` operand a builtin binds to a by-value `mixed` parameter.
///
/// The boxer stores address 0 as PHP null and a live address as an integer, the Mixed shape
/// `json_encode`, `var_dump`, `serialize`, `is_numeric` and `intval` read a nullable value in.
/// Every route to a builtin (a direct call, a static callable such as `call_user_func`, a
/// variable function or a pipe) runs this, so they agree; the declared parameter type decides,
/// never the builtin's name. Operands past the declared parameters take a variadic last
/// parameter's type.
pub(super) fn box_nullable_pointer_builtin_operands(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    operands: &mut [crate::ir::ValueId],
    span: Span,
) {
    let Some(signature) = sig else {
        return;
    };
    for (index, operand) in operands.iter_mut().enumerate() {
        if signature.ref_params.get(index).copied().unwrap_or(false) {
            continue;
        }
        let expected = match signature.params.get(index) {
            Some((_, expected)) => expected,
            None if signature.variadic.is_some() => match signature.params.last() {
                Some((_, expected)) => expected,
                None => continue,
            },
            None => continue,
        };
        if expected.codegen_repr() != PhpType::Mixed {
            continue;
        }
        let value = lowered_value_from_id(ctx, *operand);
        if crate::types::is_nullable_pointer_type(&ctx.builder.value_php_type(value.value)) {
            *operand = ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span)).value;
        }
    }
}

/// Hands `min()`/`max()` their single array argument as hash storage when it is a boxed
/// declared `array`.
///
/// A parameter declared `array` is the list-or-hash union, stored boxed. The backend's
/// one-array reduction reads a concrete list or hash, so a boxed one fell through to the
/// variadic scalar comparison and `min($values)` answered the array's last element.
/// `MixedToHash` checks the runtime tag and keeps whatever keys the array had.
fn narrow_single_array_min_max_operand(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    operands: &mut [crate::ir::ValueId],
    span: Span,
) {
    let [operand] = operands else {
        return;
    };
    if !matches!(canonical, "min" | "max") {
        return;
    }
    let ty = ctx.builder.value_php_type(*operand);
    if ty.codegen_repr() != PhpType::Mixed || !ty.is_php_array() {
        return;
    }
    let lowered = LoweredValue { value: *operand, ir_type: ctx.builder.value_type(*operand) };
    let hash = PhpType::AssocArray { key: Box::new(PhpType::Mixed), value: Box::new(PhpType::Mixed) };
    *operand = crate::ir_lower::gradual_coercions::coerce_gradual_value_to_boundary(
        ctx,
        lowered,
        &hash,
        Some(span),
    )
    .value;
}

/// Rewrites `sprintf($f, $a, ...$rest)` as `vsprintf($f, [$a, ...$rest])` (likewise `printf` and
/// `fprintf`), which is what PHP's unpacking into the variadic `$values` means.
///
/// The packed sprintf lowering takes one record per source argument, so a spread was formatted
/// as a single array: "Array to string conversion" and then "too few arguments".
fn printf_spread_as_values_call(
    ctx: &LoweringContext<'_, '_>,
    canonical: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<Expr> {
    if ctx.functions.contains_key(canonical)
        || !args.iter().any(is_spread_arg)
        || crate::types::call_args::has_named_args(args)
    {
        return None;
    }
    let (values_name, fixed) = match php_symbol_key(canonical.trim_start_matches('\\')).as_str() {
        "sprintf" => ("vsprintf", 1),
        "printf" => ("vprintf", 1),
        "fprintf" => ("vfprintf", 2),
        _ => return None,
    };
    if args.len() < fixed || args[..fixed].iter().any(is_spread_arg) {
        return None;
    }
    let mut rewritten = args[..fixed].to_vec();
    rewritten.push(Expr::new(ExprKind::ArrayLiteral(args[fixed..].to_vec()), expr.span));
    Some(Expr::new(
        ExprKind::FunctionCall { name: Name::unqualified(values_name), args: rewritten },
        expr.span,
    ))
}

/// Rewrites `sprintf(...$args)` (the unpack supplies the FORMAT as well) as
/// `vsprintf($t[0], array_slice($t, 1))` over a single evaluation of the list `$t`; `printf` and
/// `fprintf` likewise, with the unpack starting at any of their fixed parameters.
///
/// `printf_spread_as_values_call` needs every fixed parameter spelled out, so this shape fell to
/// the packed lowering and printed "Array to string conversion" for the first value. Symfony's
/// `TextDescriptor` formats `sprintf(...array_slice($deprecated, 2))`, which printed the
/// parameter deprecation as `The "Array" parameter ...`.
fn printf_leading_spread_as_values_call(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    args: &[Expr],
    _expr: &Expr,
) -> Option<Expr> {
    if ctx.functions.contains_key(canonical)
        || crate::types::call_args::has_named_args(args)
        || args.is_empty()
    {
        return None;
    }
    let (values_name, fixed) = match php_symbol_key(canonical.trim_start_matches('\\')).as_str() {
        "sprintf" => ("vsprintf", 1usize),
        "printf" => ("vprintf", 1),
        "fprintf" => ("vfprintf", 2),
        _ => return None,
    };
    // Exactly one unpack, last, starting inside the fixed parameters.
    let spread_idx = args.len() - 1;
    if spread_idx >= fixed
        || !is_spread_arg(&args[spread_idx])
        || args[..spread_idx].iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::Spread(inner) = &args[spread_idx].kind else {
        return None;
    };
    // Synthesized nodes carry a dummy span: a span shared with the source call resolves to the
    // checker's type recorded for THAT call (the sprintf result), not the node's own.
    let span = Span::dummy();
    let listed = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("array_values"),
            args: vec![(**inner).clone()],
        },
        span,
    );
    let listed = lower_expr(ctx, &listed);
    let listed = root_evaluated_call_argument(ctx, listed, span);
    let listed_type = ctx.builder.value_php_type(listed.value);
    let temp_name = ctx.declare_hidden_temp(listed_type.clone());
    store_value_into_temp(ctx, &temp_name, listed_type, listed, span);
    let temp = Expr::new(ExprKind::Variable(temp_name), span);
    let mut rewritten = args[..spread_idx].to_vec();
    for index in 0..(fixed - spread_idx) {
        rewritten.push(Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(temp.clone()),
                index: Box::new(Expr::new(ExprKind::IntLiteral(index as i64), span)),
            },
            span,
        ));
    }
    rewritten.push(Expr::new(
        ExprKind::FunctionCall {
            name: Name::from_parts(crate::names::NameKind::FullyQualified, vec!["array_slice".to_string()]),
            args: vec![temp, Expr::new(ExprKind::IntLiteral((fixed - spread_idx) as i64), span)],
        },
        span,
    ));
    Some(Expr::new(
        ExprKind::FunctionCall { name: Name::unqualified(values_name), args: rewritten },
        span,
    ))
}

/// Wraps the values argument of the `v*printf()` family in `array_values()` unless it is
/// already a known list, so the formatter reads its elements in order.
///
/// `__rt_vsprintf` walks a packed array's slots. A source `array` parameter arrives as a boxed
/// cell and a keyed array as a hash, and both were read as slots: `vsprintf('%s+%s', $vals)`
/// inside `function b(string $f, array $vals)` printed two addresses. Rewriting the source
/// keeps every temporary on the ordinary call-argument ownership path.
fn vsprintf_listed_values_args(
    ctx: &LoweringContext<'_, '_>,
    canonical: &str,
    args: &[Expr],
) -> Option<Vec<Expr>> {
    if ctx.functions.contains_key(canonical) {
        return None;
    }
    let index = match php_symbol_key(canonical.trim_start_matches('\\')).as_str() {
        "vsprintf" | "vprintf" => 1,
        "vfprintf" => 2,
        _ => return None,
    };
    if args.len() != index + 1
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let values = &args[index];
    let known_list = match &values.kind {
        ExprKind::ArrayLiteral(items) => !items.iter().any(is_spread_arg),
        ExprKind::Variable(name) => {
            matches!(ctx.local_type(name).codegen_repr(), PhpType::Array(_))
        }
        _ => false,
    };
    if known_list {
        return None;
    }
    // The wrapper carries the synthetic 0:0 span: checked builtin result types are keyed by
    // span, and `values.span` belongs to the argument expression, so a wrapped `vals()` call or a
    // spread rewritten by `printf_spread_as_values_call` read back that call's type (`string`)
    // for `array_values()`. An array argument cannot make `array_values()` throw.
    let mut listed = args.to_vec();
    listed[index] = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("array_values"),
            args: vec![values.clone()],
        },
        Span::dummy(),
    );
    Some(listed)
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
            let mut operands = operands;
            super::array_builtin_args::validate_typed_array_push_operands(ctx, def, &operands, span);
            let roots = root_non_aliasing_callback_operands(ctx, def, &mut operands, &php_type, span);
            // A builtin whose effects carry MAY_THROW can reach a codegen guard that jumps
            // straight to __rt_throw_current while these operands are still pure SSA. Pin them
            // in the unwind chain for the duration of the call so a caught guard releases them
            // exactly once instead of stranding them.
            let pins = pin_throwing_builtin_operands(ctx, def, &operands, &roots, &php_type, span);
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
            let raw_call = LoweredValue {
                value: lowered.value,
                ir_type: ctx.builder.value_type(lowered.value),
            };
            // `substr()` can return an interior view into its source string. When source-order
            // evaluation transferred a pinned temporary into the final call operand, letting
            // that pin flow through the result would either leak its base allocation forever
            // or free an interior pointer. Persist the view before retiring the base owner so
            // the result has an ordinary independent string lifetime.
            let stabilizes_evaluation_pin = matches!(
                def.spec.semantics.lowering,
                crate::builtins::semantics::BuiltinLowering::Runtime(
                    crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::Substr)
                        | crate::ir::RuntimeCallTarget::ProfiledFunction {
                            target: crate::ir::RuntimeFnId::Substr,
                            ..
                        },
                ),
            ) && matches!(
                def.spec.semantics.result_ownership,
                crate::builtins::semantics::BuiltinResultOwnership::MayAliasArguments,
            ) && ctx.builder.value_php_type(raw_call.value).codegen_repr() == PhpType::Str
                && operands.first().is_some_and(|operand| {
                    value_is_call_argument_evaluation_pin(ctx, *operand)
                        || ctx.borrowed_string_call_operands.contains(operand)
                });
            let call = if stabilizes_evaluation_pin {
                ctx.emit_owned_value(
                    Op::StrPersist,
                    vec![raw_call.value],
                    None,
                    PhpType::Str,
                    Op::StrPersist.default_effects(),
                    Some(span),
                )
            } else {
                raw_call
            };
            // Consumers make lifetime decisions during lowering, before final
            // ownership refinement. In particular, Fresh strings already own
            // heap storage and must not be persisted again as concat scratch.
            if matches!(
                def.spec.semantics.result_ownership,
                crate::builtins::semantics::BuiltinResultOwnership::Fresh,
            ) && Ownership::php_type_needs_lifetime_tracking(
                &ctx.builder.value_php_type(call.value),
            ) {
                ctx.builder.set_value_ownership(call.value, Ownership::Owned);
            }
            let return_alias = if stabilizes_evaluation_pin {
                ReturnArgAlias::None
            } else {
                match def.spec.semantics.result_ownership {
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
                }
            };
            let (call, return_alias) =
                persist_string_view_of_owned_args(ctx, def, call, return_alias, &operands, span);
            unpin_in_flight_owners(ctx, pins, span);
            for (_, slot) in roots.iter().rev() {
                retire_owned_call_operand(ctx, *slot, span);
            }
            let unrooted = operands.iter().enumerate()
                .filter(|(index, _)| !roots.iter().any(|(root, _)| root == index))
                .filter(|(index, _)| !builtin_consumes_mutating_ref_operand(def, *index))
                .map(|(_, value)| *value).collect::<Vec<_>>();
            release_owned_call_arg_temporaries(
                ctx,
                &unrooted,
                Some(call.value),
                &return_alias,
                span,
            );
            if php_symbol_key(name.trim_start_matches('\\')) == "extract" {
                ctx.mark_eval_executed();
                ctx.apply_eval_barrier();
            }
            return call;
        }
    }
    let is_eval = php_symbol_key(name.trim_start_matches('\\')) == "eval";
    let mut operands = operands;
    let eval_source_owner = if is_eval {
        operands.first().copied().and_then(|code| {
            let source = LoweredValue { value: code, ir_type: ctx.builder.value_type(code) };
            let (source, owner) = root_owned_call_operand(ctx, source, span);
            operands[0] = source.value;
            owner
        })
    } else {
        None
    };
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
    let eval_needs_barrier = match eval_literal {
        Some(fragment) => eval_literal_needs_barrier(ctx, fragment),
        None => true,
    };
    // An owner slot already registered for the source (`eval_source_owner`) is released by the
    // unwinder through the call-operand owner stack; guarding the same value here as well made a
    // throw out of eval free it twice (free-list corruption in every catch around a dynamic eval).
    if is_eval && eval_source_owner.is_none() {
        ctx.begin_argument_guard_scope();
        for (parameter, value) in operands.iter().copied().enumerate() {
            let source = LoweredValue {
                value,
                ir_type: ctx.builder.value_type(value),
            };
            if ctx.value_is_owning_temporary(source)
                && !ctx.has_call_argument_guard(source.value)
            {
                ctx.guard_call_argument(source, parameter, span);
            }
        }
        ctx.end_argument_guard_scope();
    }
    let call = ctx.emit_value(
        op,
        operands.clone(),
        immediate,
        php_type,
        effects,
        Some(span),
    );
    // A specialized `filter_var$*` result is fresh (see `value_is_owning_builtin_temporary`), so
    // no argument has to outlive the call as a presumed alias.
    let fresh_filter_result = name.starts_with("filter_var$");
    // Scope widening can make the already-lowered source load an owned string cast.
    // Eval guards that owner through exceptional exits, then releases it normally here.
    if let Some(slot) = eval_source_owner {
        retire_owned_call_operand(ctx, slot, span);
    } else {
        release_owned_call_arg_temporaries(
            ctx,
            &operands,
            Some(call.value),
            if is_eval || fresh_filter_result { &ReturnArgAlias::None } else { &ReturnArgAlias::Unknown },
            span,
        );
    }
    if is_eval {
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

/// Returns whether a typed builtin backend consumes one by-reference operand owner.
///
/// `array_multisort()` is unusual among the mutating builtins: its backend separates and
/// republishes two receivers, and a concrete array read from widened Mixed local storage owns a
/// detached payload lease. The backend consumes that lease during COW/storeback, including the
/// explicit duplicate lease retirement when both arguments name the same place. Ordinary
/// post-call cleanup must therefore leave those two owners to the backend while retaining the
/// normal cleanup contract for every other builtin operand.
fn builtin_consumes_mutating_ref_operand(
    def: &crate::builtins::registry::BuiltinDef,
    parameter_index: usize,
) -> bool {
    if !def.ref_params.get(parameter_index).copied().unwrap_or(false) {
        return false;
    }
    matches!(
        def.spec.semantics.lowering,
        crate::builtins::semantics::BuiltinLowering::Runtime(
            crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::ArrayMultisort)
                | crate::ir::RuntimeCallTarget::ProfiledFunction {
                    target: crate::ir::RuntimeFnId::ArrayMultisort,
                    ..
                },
        )
    )
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
            .get(&(ctx.loop_storage_scope.clone(), span))
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
    /// Distinguishes PHP value representations from extension-only raw descriptors.
    fn is_a_php_value(ty: &PhpType) -> bool {
        !matches!(
            ty,
            PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) | PhpType::Callable
        )
    }
    is_a_php_value(checked) == is_a_php_value(declared)
}

/// Is a boxed builtin result cast back to the raw word the checker typed the call as?
///
/// `array_pop()` and `array_shift()` declare `mixed` (an empty array yields null) and check as
/// their element type, which for an array of pointers is `?ptr<T>` and for an array of buffers
/// `buffer<T>`: raw words, not boxed cells. The declared `mixed` is the representation the call
/// really produces, and `unbox_raw_word_builtin_result` casts that box back to the word after
/// every call the checker filed, so this pair is a conversion the lowering owns rather than a
/// fallback that changes representation.
fn raw_word_result_is_unboxed(
    def: &crate::builtins::registry::BuiltinDef,
    checked: Option<&PhpType>,
) -> bool {
    let Some(checked) = checked else {
        return false;
    };
    matches!(
        def.spec.semantics.lowering,
        crate::builtins::semantics::BuiltinLowering::Runtime(crate::ir::RuntimeCallTarget::Function(
            crate::ir::RuntimeFnId::ArrayPop | crate::ir::RuntimeFnId::ArrayShift
        ))
    ) && matches!(def.return_type, PhpType::Mixed)
        && (crate::types::is_nullable_pointer_type(checked) || matches!(checked, PhpType::Buffer(_)))
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
        declared_type_is_a_safe_fallback(&def.return_type, checked.as_ref())
            || raw_word_result_is_unboxed(def, checked.as_ref()),
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


/// Returns whether a by-reference receiver expression is a pure place: variables, properties and
/// elements addressed by literals, variables or constants, so reading it twice is unobservable.
fn splice_place_is_pure(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) => true,
        ExprKind::PropertyAccess { object, .. } => splice_place_is_pure(object),
        ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, index } => {
            splice_place_is_pure(array)
                && matches!(
                    index.kind,
                    ExprKind::StringLiteral(_) | ExprKind::IntLiteral(_) | ExprKind::Variable(_)
                )
        }
        _ => false,
    }
}

/// Returns whether a lowered receiver is a boxed value READ out of a nested place.
///
/// An element or property read through a boxed container yields a cell that hangs off a transient
/// copy of the parent, so splicing it in place loses the write and the backend has no slot to
/// publish a relocated array into. Receivers loaded straight from a variable, a reference cell, a
/// static or a global are real places and keep their in-place path.
fn splice_receiver_is_nested_boxed_read(ctx: &LoweringContext<'_, '_>, value: crate::ir::ValueId) -> bool {
    if !matches!(
        ctx.builder.value_php_type(value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return false;
    }
    ctx.builder.value_defining_instruction(value).is_some_and(|inst| {
        matches!(
            inst.op,
            Op::RuntimeCall
                | Op::PropGet
                | Op::DynamicPropGet
                | Op::ArrayGetMixedKey
                | Op::ArrayGetMixedKeySilent
                | Op::HashGet
                | Op::HashGetSilent
        )
    })
}

/// Builtins whose first parameter is the array they mutate in place (`array &$array`).
fn builtin_takes_array_by_reference_first(key: &str) -> bool {
    matches!(
        key,
        "array_splice"
            | "array_push"
            | "array_pop"
            | "array_shift"
            | "array_unshift"
            | "sort"
            | "rsort"
            | "usort"
            | "asort"
            | "arsort"
            | "uasort"
            | "ksort"
            | "krsort"
            | "uksort"
            | "natsort"
            | "natcasesort"
            | "shuffle"
            | "array_walk"
    )
}

/// Lowers a call whose by-reference arguments are nested boxed places through temporaries.
///
/// `array_splice($obj->vars['k'], ...)`, `sort($view->vars['l'])` or `fill($entry->vars['x'])`
/// with a boxed `$obj` read the place as a VALUE, so the callee mutated a transient copy and the
/// write was dropped (or refused by the backend). php binds the reference to the place itself; the
/// equivalent without reference cells is `$tmp = place; f($tmp, ...); place = $tmp`, which is how
/// element and property writes through a boxed object already lower.
///
/// Only pure places qualify (each is read once and written once), only user functions and the
/// by-reference array builtins in `builtin_takes_array_by_reference_first` are considered, and only
/// when a speculative lowering shows the argument landing on a nested boxed read, so every
/// receiver that already binds a real place keeps its in-place path. The temporary is read quietly
/// (`place ?? null`) because a by-reference binding creates a missing element rather than warning.
fn lower_by_ref_call_through_temps(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    sig: Option<&FunctionSig>,
    args: &[Expr],
    expr: &Expr,
    is_user_function: bool,
) -> Option<LoweredValue> {
    let signature = sig?;
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    let candidates: Vec<usize> = args
        .iter()
        .enumerate()
        .filter(|(index, arg)| {
            signature.ref_params.get(*index).copied().unwrap_or(false)
                && !matches!(arg.kind, ExprKind::Variable(_))
                && splice_place_is_pure(arg)
        })
        .map(|(index, _)| index)
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let snapshot = ctx.snapshot();
    begin_call_argument_evaluation(ctx);
    let mut operands = if is_user_function {
        lower_user_args_with_signature(ctx, sig, args)
    } else {
        lower_builtin_call_args(ctx, name.as_str(), sig, args)
    };
    let opaque: Vec<usize> = candidates
        .into_iter()
        .filter(|index| {
            operands
                .get(*index)
                .is_some_and(|operand| splice_receiver_is_nested_boxed_read(ctx, *operand))
        })
        .collect();
    let _ = finish_call_argument_evaluation(ctx, &mut operands);
    ctx.restore(snapshot);
    if opaque.is_empty() {
        return None;
    }
    let span = expr.span;
    let mut call_args = args.to_vec();
    let mut writebacks = Vec::new();
    for index in opaque {
        let place = &args[index];
        let temp = format!("__elephc_ref_recv_{}_{}_{}", span.line, span.col, index);
        crate::ir_lower::stmt::lower_stmt(
            ctx,
            &Stmt::new(
                StmtKind::Assign {
                    name: temp.clone(),
                    value: Expr::new(
                        ExprKind::NullCoalesce {
                            value: Box::new(place.clone()),
                            default: Box::new(Expr::new(ExprKind::Null, place.span)),
                        },
                        place.span,
                    ),
                },
                span,
            ),
        );
        call_args[index] = Expr::new(ExprKind::Variable(temp.clone()), place.span);
        writebacks.push((place.clone(), temp));
    }
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: name.clone(),
            args: call_args,
        },
        span,
    );
    let result = lower_expr(ctx, &call);
    for (place, temp) in writebacks {
        if let Some(kind) = assignments::non_local_assignment_stmt_kind(
            &place,
            &Expr::new(ExprKind::Variable(temp), place.span),
        ) {
            crate::ir_lower::stmt::lower_stmt(ctx, &Stmt::new(kind, span));
        }
    }
    Some(result)
}
