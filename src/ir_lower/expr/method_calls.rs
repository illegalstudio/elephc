//! Purpose:
//! Core instance-method dispatch and Closure rebinding methods.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers an object method call.
pub(super) fn lower_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    method: &str,
    args: &[Expr],
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    // A statically-decided private/protected method access from an inaccessible
    // scope raises a catchable `Error` in PHP rather than a compile-time error,
    // but the receiver expression must still be evaluated first.
    let throw_access_message = if op == Op::MethodCall {
        ctx.throw_access_sites.get(&expr.span).and_then(|info| {
            if let ThrowAccessKind::PrivateMethod {
                visibility,
                class_name,
                method: m,
            } = &info.kind
            {
                Some(format!(
                    "Call to {} method {}::{}() from global scope",
                    visibility, class_name, m
                ))
            } else {
                None
            }
        })
    } else {
        None
    };
    let object_expr = object;
    let object = lower_expr(ctx, object_expr);
    if let Some(message) = throw_access_message {
        release_owning_receiver_temporary(ctx, object, expr.span);
        return crate::ir_lower::stmt::lower_throw_access_error_expr(ctx, &message, expr.span);
    }
    if op == Op::MethodCall && value_is_definitely_null(ctx, object.value) {
        let null_value = lower_null(ctx, expr);
        terminate_method_call_on_null(ctx, method);
        return null_value;
    }
    if op == Op::MethodCall {
        if let Some(value) =
            lower_reflection_function_invoke_call(ctx, Some(object_expr), method, args, expr)
        {
            return value;
        }
        if let Some(value) =
            lower_reflection_method_invoke_call(ctx, Some(object_expr), method, args, expr)
        {
            return value;
        }
    }
    if op == Op::MethodCall
        && (value_is_nullable(ctx, object.value)
            || value_may_carry_container_miss(ctx, object.value))
    {
        return lower_nullable_regular_method_call(ctx, object, method, args, expr);
    }
    if op == Op::MethodCall && is_reflection_class_new_instance_call(ctx, object.value, method) {
        return lower_reflection_class_new_instance(ctx, Some(object_expr), object, args, expr);
    }
    if op == Op::MethodCall && is_reflection_class_new_instance_args_call(ctx, object.value, method)
    {
        return lower_reflection_class_new_instance_args(
            ctx,
            Some(object_expr),
            object,
            args,
            expr,
        );
    }
    if op == Op::MethodCall
        && is_reflection_class_new_instance_without_constructor_call(ctx, object.value, method)
    {
        return lower_reflection_class_new_instance_without_constructor(ctx, object, args, expr);
    }
    if op == Op::MethodCall {
        if let Some(value) = lower_reflection_class_static_property_value_call(
            ctx,
            Some(object_expr),
            method,
            args,
            expr,
        ) {
            return value;
        }
    }
    if op == Op::MethodCall {
        if let Some(value) =
            lower_reflection_class_member_list_call(ctx, Some(object_expr), method, args, expr)
        {
            return value;
        }
    }
    if op == Op::MethodCall {
        if let Some(value) =
            lower_reflection_property_value_call(ctx, Some(object_expr), method, args, expr)
        {
            return value;
        }
    }
    if matches!(
        ctx.builder.value_php_type(object.value).codegen_repr(),
        PhpType::Callable
    ) {
        if let Some(result) = lower_closure_bind_method(ctx, &object, method, args, expr) {
            return result;
        }
    }
    if matches!(op, Op::MethodCall | Op::NullsafeMethodCall) {
        let object_type = ctx.builder.value_php_type(object.value);
        if let Some((class_name, _)) = singular_object_class(&object_type) {
            let method_key = php_symbol_key(method);
            let has_synthetic_body = ctx.classes.get(class_name).is_some_and(|class_info| {
                class_info.method_decls.iter().any(|declaration| {
                    php_symbol_key(&declaration.name) == method_key
                        && declaration.has_body
                        && !declaration.body.is_empty()
                })
            });
            if !has_synthetic_body {
                if let Some(opcode) =
                    crate::ir_lower::internal_extensions::method_opcode(ctx, class_name, method)
                {
                    let receiver_guard =
                        guard_owning_receiver_temporary_for_throw(ctx, object, expr.span);
                    let result_type = method_call_result_type(ctx, object.value, method, op, expr);
                    let sig = method_call_argument_signature(ctx, object_expr, object.value, method);
                    let preserve_omitted_defaults = matches!(
                        class_name.trim_start_matches('\\'),
                        "DOMXPath" | "Dom\\XPath"
                    ) && matches!(method.to_ascii_lowercase().as_str(), "evaluate" | "query");
                    let arguments = if preserve_omitted_defaults && args.iter().any(is_spread_arg)
                    {
                        sig.as_ref()
                            .and_then(|signature| {
                                lower_dom_xpath_dynamic_spread_args(
                                    ctx,
                                    signature,
                                    args,
                                    expr.span,
                                )
                            })
                            .unwrap_or_else(|| {
                                lower_internal_extension_args(
                                    ctx,
                                    sig.as_ref(),
                                    args,
                                    preserve_omitted_defaults,
                                )
                            })
                    } else {
                        lower_internal_extension_args(
                            ctx,
                            sig.as_ref(),
                            args,
                            preserve_omitted_defaults,
                        )
                    };
                    let (arguments, argument_guards) =
                        prepare_internal_extension_arguments_for_throw(
                            ctx,
                            sig.as_ref(),
                            arguments,
                            expr.span,
                        );
                    let mut operands = Vec::with_capacity(arguments.len() + 1);
                    operands.push(object.value);
                    operands.extend(arguments.iter().copied());
                    let call = crate::ir_lower::internal_extensions::emit_call(
                        ctx,
                        opcode,
                        crate::ir_lower::internal_extensions::FLAG_RECEIVER
                            | internal_extension_result_flags(&result_type),
                        operands,
                        result_type,
                        expr.span,
                    );
                    clear_owning_call_arg_temporary_guards(ctx, &argument_guards, expr.span);
                    release_owned_call_arg_temporaries_with_signature(
                        ctx,
                        &arguments,
                        Some(call.value),
                        &ReturnArgAlias::Unknown,
                        sig.as_ref(),
                        expr.span,
                    );
                    release_guarded_owning_receiver_temporary(
                        ctx,
                        object,
                        receiver_guard.as_deref(),
                        expr.span,
                    );
                    return call;
                }
            }
        }
    }
    let magic_args;
    let (dispatch_method, args) = if let Some(args) =
        magic_call_dispatch_args(ctx, object.value, method, args, object_expr.span)
    {
        magic_args = args;
        ("__call", magic_args.as_slice())
    } else {
        (method, args)
    };
    let result_type = method_call_result_type(ctx, object.value, dispatch_method, op, expr);
    let mut operands = vec![object.value];
    let sig = method_call_argument_signature(ctx, object_expr, object.value, dispatch_method);
    promote_pdo_binding_ref_argument(ctx, object.value, dispatch_method, args);
    let arg_values = lower_args_with_signature(ctx, sig.as_ref(), args);
    operands.extend(arg_values.iter().copied());
    let data = ctx.intern_string(dispatch_method);
    let call = ctx.emit_value(
        op,
        operands,
        Some(Immediate::Data(data)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    let return_alias = method_return_arg_alias(ctx, object.value, dispatch_method);
    release_owned_call_arg_temporaries_with_signature(
        ctx,
        &arg_values,
        Some(call.value),
        &return_alias,
        sig.as_ref(),
        expr.span,
    );
    release_owning_receiver_temporary(ctx, object, expr.span);
    call
}

/// Lowers dynamic XPath spreads while retaining whether `$registerNodeNS` was supplied.
///
/// The native bridge receives a fourth, compiler-private presence operand only when argument
/// unpacking forced the three PHP-visible optional parameters to be materialized. That lets it
/// distinguish an omitted third argument from an explicitly supplied false value.
fn lower_dom_xpath_dynamic_spread_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
    span: crate::span::Span,
) -> Option<Vec<crate::ir::ValueId>> {
    const REGISTER_NODE_NS_PARAM: usize = 2;
    let expanded_args = has_static_call_spread_args(args).then(|| expand_static_call_spread_args(args));
    let args = expanded_args.as_deref().unwrap_or(args);
    if !args.iter().any(is_spread_arg) {
        return None;
    }

    if crate::types::call_args::has_named_args(args) {
        let assoc_spread_sources = assoc_spread_sources(ctx, args);
        let regular_param_count = crate::types::call_args::regular_param_count(sig);
        let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
            sig,
            args,
            span,
            regular_param_count,
            false,
            true,
            &assoc_spread_sources,
        )
        .ok()?;
        let lowered = lower_dom_xpath_named_args_with_spread_plan(
            ctx,
            sig,
            &plan,
            &assoc_spread_sources,
        )
        .or_else(|| lower_dom_xpath_dynamic_named_spread_args(ctx, sig, &plan))?;
        let presence = dom_xpath_planned_regular_arg_presence(
            ctx,
            plan.regular_args.get(REGISTER_NODE_NS_PARAM)?,
            &lowered.prefix_temp,
            span,
        );
        return Some(finalize_dom_xpath_dynamic_spread_args(
            ctx,
            sig,
            lowered.operands,
            presence,
            span,
        ));
    }

    if let Some(lowered) = lower_dom_xpath_multiple_assoc_spread_args(ctx, sig, args) {
        let presence = dom_xpath_multiple_assoc_spread_param_presence(
            ctx,
            &lowered.spread_exprs,
            sig.params.get(REGISTER_NODE_NS_PARAM)?.0.as_str(),
            span,
        );
        return Some(finalize_dom_xpath_dynamic_spread_args(
            ctx,
            sig,
            lowered.operands,
            presence,
            span,
        ));
    }

    if let Some(lowered) = lower_dom_xpath_positional_spread_args(ctx, sig, args) {
        let presence = dom_xpath_positional_spread_param_presence(
            ctx,
            lowered.spread,
            lowered.first_spread_param_idx,
            REGISTER_NODE_NS_PARAM,
            span,
        );
        return Some(finalize_dom_xpath_dynamic_spread_args(
            ctx,
            sig,
            lowered.operands,
            presence,
            span,
        ));
    }

    if let Some(lowered) = lower_dom_xpath_multiple_dynamic_spread_args(ctx, sig, args) {
        let presence = dom_xpath_merged_spread_param_presence(
            ctx,
            &lowered.spread_expr,
            REGISTER_NODE_NS_PARAM,
            sig.params.get(REGISTER_NODE_NS_PARAM)?.0.as_str(),
            lowered.has_named_keys,
            span,
        );
        return Some(finalize_dom_xpath_dynamic_spread_args(
            ctx,
            sig,
            lowered.operands,
            presence,
            span,
        ));
    }

    let lowered = lower_dom_xpath_assoc_spread_only_args(ctx, sig, args)?;
    let param_name = sig.params.get(REGISTER_NODE_NS_PARAM)?.0.clone();
    let key = Expr::new(ExprKind::StringLiteral(param_name), span);
    let presence_expr = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("array_key_exists"),
            args: vec![key, lowered.spread_expr],
        },
        span,
    );
    let presence = lower_expr(ctx, &presence_expr).value;
    Some(finalize_dom_xpath_dynamic_spread_args(
        ctx,
        sig,
        lowered.operands,
        presence,
        span,
    ))
}

/// Coerces the XPath third argument and appends its compiler-private presence marker.
fn finalize_dom_xpath_dynamic_spread_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    operands: Vec<crate::ir::ValueId>,
    presence: crate::ir::ValueId,
    span: crate::span::Span,
) -> Vec<crate::ir::ValueId> {
    let mut operands = coerce_operands_to_params(ctx, sig, operands);
    coerce_dom_xpath_register_node_ns_operand(ctx, &mut operands, span);
    operands.push(presence);
    operands
}

/// Multiple materialized associative spreads and their normalized XPath operands.
struct LoweredDomXPathMultipleAssocSpreadArgs {
    operands: Vec<crate::ir::ValueId>,
    spread_exprs: Vec<Expr>,
}

/// Lowers multiple associative XPath spreads once while retaining every named source.
fn lower_dom_xpath_multiple_assoc_spread_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> Option<LoweredDomXPathMultipleAssocSpreadArgs> {
    if sig.variadic.is_some()
        || args.len() < 2
        || !args.iter().all(|arg| {
            let ExprKind::Spread(inner) = &arg.kind else {
                return false;
            };
            is_assoc_spread_source(ctx, inner)
        })
    {
        return None;
    }

    let mut spread_exprs = Vec::with_capacity(args.len());
    for arg in args {
        let ExprKind::Spread(inner) = &arg.kind else {
            return None;
        };
        let spread = lower_expr(ctx, inner);
        let spread_type = ctx.builder.value_php_type(spread.value);
        let temp_name = ctx.declare_hidden_temp(spread_type.clone());
        store_value_into_temp(ctx, &temp_name, spread_type, spread, arg.span);
        spread_exprs.push(Expr::new(ExprKind::Variable(temp_name), inner.span));
    }

    for (param_name, _) in &sig.params {
        emit_dom_xpath_multiple_assoc_spread_duplicate_guard(
            ctx,
            &spread_exprs,
            param_name,
            args[0].span,
        );
    }

    let mut operands = Vec::with_capacity(sig.params.len());
    for (param_idx, (param_name, _)) in sig.params.iter().enumerate() {
        let default = sig.defaults.get(param_idx).and_then(|default| default.as_ref());
        let expr = dom_xpath_multiple_assoc_spread_param_expr(
            &spread_exprs,
            param_name,
            default,
            args[0].span,
        );
        operands.push(lower_expr(ctx, &expr).value);
    }
    Some(LoweredDomXPathMultipleAssocSpreadArgs {
        operands,
        spread_exprs,
    })
}

/// Builds one named-parameter read across separately materialized associative XPath spreads.
fn dom_xpath_multiple_assoc_spread_param_expr(
    spread_exprs: &[Expr],
    param_name: &str,
    default: Option<&Expr>,
    span: crate::span::Span,
) -> Expr {
    let key = Expr::new(ExprKind::StringLiteral(param_name.to_string()), span);
    let mut selected = default.cloned().unwrap_or_else(|| {
        let fallback = spread_exprs
            .first()
            .cloned()
            .unwrap_or_else(|| Expr::new(ExprKind::ArrayLiteral(Vec::new()), span));
        Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(fallback),
                index: Box::new(key.clone()),
            },
            span,
        )
    });
    for spread_expr in spread_exprs.iter().rev() {
        let access = Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(spread_expr.clone()),
                index: Box::new(key.clone()),
            },
            span,
        );
        selected = Expr::new(
            ExprKind::Ternary {
                condition: Box::new(Expr::new(
                    ExprKind::FunctionCall {
                        name: Name::unqualified("array_key_exists"),
                        args: vec![key.clone(), spread_expr.clone()],
                    },
                    span,
                )),
                then_expr: Box::new(access),
                else_expr: Box::new(selected),
            },
            span,
        );
    }
    selected
}

/// Emits php-src's catchable duplicate-named-parameter guard across associative spreads.
fn emit_dom_xpath_multiple_assoc_spread_duplicate_guard(
    ctx: &mut LoweringContext<'_, '_>,
    spread_exprs: &[Expr],
    param_name: &str,
    span: crate::span::Span,
) {
    for left_idx in 0..spread_exprs.len() {
        for right_idx in (left_idx + 1)..spread_exprs.len() {
            let key = || Expr::new(ExprKind::StringLiteral(param_name.to_string()), span);
            let exists = |spread_expr: &Expr| {
                Expr::new(
                    ExprKind::FunctionCall {
                        name: Name::unqualified("array_key_exists"),
                        args: vec![key(), spread_expr.clone()],
                    },
                    span,
                )
            };
            let duplicate = Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(exists(&spread_exprs[left_idx])),
                    op: BinOp::And,
                    right: Box::new(exists(&spread_exprs[right_idx])),
                },
                span,
            );
            let duplicate = lower_expr(ctx, &duplicate);
            let fatal = ctx
                .builder
                .create_named_block("call.assoc_spread.duplicate", Vec::new());
            let ok = ctx
                .builder
                .create_named_block("call.assoc_spread.duplicate.ok", Vec::new());
            ctx.builder.terminate(Terminator::CondBr {
                cond: duplicate.value,
                then_target: fatal,
                then_args: Vec::new(),
                else_target: ok,
                else_args: Vec::new(),
            });
            ctx.builder.position_at_end(fatal);
            terminate_dom_xpath_named_parameter_overwrite_error(ctx, param_name, span);
            ctx.builder.position_at_end(ok);
        }
    }
}

/// Terminates one XPath duplicate-named-parameter path through PHP's catchable `Error`.
fn terminate_dom_xpath_named_parameter_overwrite_error(
    ctx: &mut LoweringContext<'_, '_>,
    param_name: &str,
    span: crate::span::Span,
) {
    let message = ctx.intern_string(&format!(
        "Named parameter ${} overwrites previous argument",
        param_name
    ));
    ctx.emit_void(
        Op::ThrowError,
        Vec::new(),
        Some(Immediate::Data(message)),
        Op::ThrowError.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::Unreachable);
}

/// Returns whether any associative XPath spread supplied one named parameter.
fn dom_xpath_multiple_assoc_spread_param_presence(
    ctx: &mut LoweringContext<'_, '_>,
    spread_exprs: &[Expr],
    param_name: &str,
    span: crate::span::Span,
) -> crate::ir::ValueId {
    let mut presence = Expr::new(ExprKind::BoolLiteral(false), span);
    for spread_expr in spread_exprs {
        let exists = Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("array_key_exists"),
                args: vec![
                    Expr::new(ExprKind::StringLiteral(param_name.to_string()), span),
                    spread_expr.clone(),
                ],
            },
            span,
        );
        presence = Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(presence),
                op: BinOp::Or,
                right: Box::new(exists),
            },
            span,
        );
    }
    lower_expr(ctx, &presence).value
}

/// One materialized prefix formed from multiple dynamic XPath call spreads.
struct LoweredDomXPathMergedSpreadArgs {
    operands: Vec<crate::ir::ValueId>,
    spread_expr: Expr,
    has_named_keys: bool,
}

/// Merges multiple dynamic XPath spreads once and reads fixed parameters by name or position.
fn lower_dom_xpath_multiple_dynamic_spread_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> Option<LoweredDomXPathMergedSpreadArgs> {
    if sig.variadic.is_some()
        || args
            .iter()
            .filter(|arg| matches!(arg.kind, ExprKind::Spread(_)))
            .count()
            < 1
        || crate::types::call_args::has_named_args(args)
    {
        return None;
    }

    let span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let has_named_keys = args.iter().any(|arg| {
        let ExprKind::Spread(inner) = &arg.kind else {
            return false;
        };
        is_assoc_spread_source(ctx, inner)
    });
    let mut materialized_args = Vec::with_capacity(args.len());
    for arg in args {
        let (source, is_spread) = match &arg.kind {
            ExprKind::Spread(inner) => (inner.as_ref(), true),
            _ => (arg, false),
        };
        let lowered = lower_expr(ctx, source);
        let source_type = ctx.builder.value_php_type(lowered.value);
        let temp_name = ctx.declare_hidden_temp(source_type.clone());
        store_value_into_temp(ctx, &temp_name, source_type, lowered, source.span);
        let temp = Expr::new(ExprKind::Variable(temp_name), source.span);
        materialized_args.push(if is_spread {
            Expr::new(ExprKind::Spread(Box::new(temp)), arg.span)
        } else {
            temp
        });
    }
    let merged_expr = Expr::new(ExprKind::ArrayLiteral(materialized_args), span);
    let merged = lower_expr(ctx, &merged_expr);
    let merged_type = ctx.builder.value_php_type(merged.value);
    let temp_name = ctx.declare_hidden_temp(merged_type.clone());
    store_value_into_temp(ctx, &temp_name, merged_type, merged, span);
    let spread_expr = Expr::new(ExprKind::Variable(temp_name), span);

    let mut operands = Vec::with_capacity(sig.params.len());
    for (param_idx, (param_name, _)) in sig.params.iter().enumerate() {
        let default = sig.defaults.get(param_idx).and_then(|default| default.as_ref());
        let expr = dom_xpath_merged_spread_param_expr(
            &spread_expr,
            param_idx,
            param_name,
            default,
            has_named_keys,
            span,
        );
        operands.push(lower_expr(ctx, &expr).value);
    }
    Some(LoweredDomXPathMergedSpreadArgs {
        operands,
        spread_expr,
        has_named_keys,
    })
}

/// Reads one merged XPath spread parameter, preferring its named key over its numeric slot.
fn dom_xpath_merged_spread_param_expr(
    spread_expr: &Expr,
    param_idx: usize,
    param_name: &str,
    default: Option<&Expr>,
    has_named_keys: bool,
    span: crate::span::Span,
) -> Expr {
    if !has_named_keys {
        return default.map_or_else(
            || spread_element_expr_for_ir(spread_expr, param_idx, None, false, span),
            |default| {
                spread_element_or_default_expr_for_ir(
                    spread_expr,
                    param_idx,
                    None,
                    false,
                    default.clone(),
                    span,
                )
            },
        );
    }
    let named_key = Expr::new(ExprKind::StringLiteral(param_name.to_string()), span);
    let numeric_key = Expr::new(ExprKind::IntLiteral(param_idx as i64), span);
    let named_access = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(spread_expr.clone()),
            index: Box::new(named_key.clone()),
        },
        span,
    );
    let numeric_access = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(spread_expr.clone()),
            index: Box::new(numeric_key.clone()),
        },
        span,
    );
    let numeric_or_default = default.map_or(numeric_access.clone(), |default| {
        Expr::new(
            ExprKind::Ternary {
                condition: Box::new(Expr::new(
                    ExprKind::FunctionCall {
                        name: Name::unqualified("array_key_exists"),
                        args: vec![numeric_key, spread_expr.clone()],
                    },
                    span,
                )),
                then_expr: Box::new(numeric_access),
                else_expr: Box::new(default.clone()),
            },
            span,
        )
    });
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("array_key_exists"),
                    args: vec![named_key, spread_expr.clone()],
                },
                span,
            )),
            then_expr: Box::new(named_access),
            else_expr: Box::new(numeric_or_default),
        },
        span,
    )
}

/// Returns whether a merged XPath spread supplied one parameter by name or numeric slot.
fn dom_xpath_merged_spread_param_presence(
    ctx: &mut LoweringContext<'_, '_>,
    spread_expr: &Expr,
    param_idx: usize,
    param_name: &str,
    has_named_keys: bool,
    span: crate::span::Span,
) -> crate::ir::ValueId {
    if !has_named_keys {
        let numeric_exists = Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("array_key_exists"),
                args: vec![
                    Expr::new(ExprKind::IntLiteral(param_idx as i64), span),
                    spread_expr.clone(),
                ],
            },
            span,
        );
        return lower_expr(ctx, &numeric_exists).value;
    }
    let named_exists = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("array_key_exists"),
            args: vec![
                Expr::new(ExprKind::StringLiteral(param_name.to_string()), span),
                spread_expr.clone(),
            ],
        },
        span,
    );
    let numeric_exists = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("array_key_exists"),
            args: vec![
                Expr::new(ExprKind::IntLiteral(param_idx as i64), span),
                spread_expr.clone(),
            ],
        },
        span,
    );
    let presence = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(named_exists),
            op: BinOp::Or,
            right: Box::new(numeric_exists),
        },
        span,
    );
    lower_expr(ctx, &presence).value
}

/// One materialized trailing indexed XPath spread and its normalized operands.
struct LoweredDomXPathPositionalSpreadArgs {
    operands: Vec<crate::ir::ValueId>,
    spread: crate::ir::ValueId,
    first_spread_param_idx: usize,
}

/// Lowers one trailing indexed XPath spread in a fixed-arity positional call.
fn lower_dom_xpath_positional_spread_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> Option<LoweredDomXPathPositionalSpreadArgs> {
    if sig.variadic.is_some() {
        return None;
    }
    let spread_idx = single_trailing_indexed_spread_arg(ctx, args)?;
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    if spread_idx > regular_param_count {
        return None;
    }
    let first_spread_param_idx = spread_idx;
    let required_len = required_positional_spread_len(sig, first_spread_param_idx, regular_param_count);
    let ExprKind::Spread(inner) = &args[spread_idx].kind else {
        return None;
    };
    if static_indexed_spread_len(inner).is_some_and(|len| len >= required_len) {
        return None;
    }

    let mut operands = Vec::with_capacity(regular_param_count);
    for (index, arg) in args[..spread_idx].iter().enumerate() {
        operands.push(lower_arg_with_signature(ctx, sig, index, arg));
    }

    let spread_type = indexed_spread_source_type(ctx, inner)?;
    let spread = lower_expr(ctx, inner);
    let temp_name = ctx.declare_hidden_temp(spread_type.clone());
    store_value_into_temp(ctx, &temp_name, spread_type, spread, args[spread_idx].span);
    let spread_expr = Expr::new(ExprKind::Variable(temp_name), inner.span);
    let spread_value = lower_expr(ctx, &spread_expr);
    emit_positional_spread_min_len_guard(
        ctx,
        spread_value.value,
        required_len,
        args[spread_idx].span,
    );

    for param_idx in first_spread_param_idx..regular_param_count {
        let element_idx = param_idx - first_spread_param_idx;
        let default = sig.defaults.get(param_idx).and_then(|default| default.as_ref());
        let expr = if let Some(default) = default {
            if element_idx < required_len {
                spread_element_expr_for_ir(
                    &spread_expr,
                    element_idx,
                    None,
                    false,
                    args[spread_idx].span,
                )
            } else {
                spread_element_or_default_expr_for_ir(
                    &spread_expr,
                    element_idx,
                    None,
                    false,
                    default.clone(),
                    args[spread_idx].span,
                )
            }
        } else {
            spread_element_expr_for_ir(
                &spread_expr,
                element_idx,
                None,
                false,
                args[spread_idx].span,
            )
        };
        operands.push(lower_expr(ctx, &expr).value);
    }

    Some(LoweredDomXPathPositionalSpreadArgs {
        operands,
        spread: spread_value.value,
        first_spread_param_idx,
    })
}

/// Returns whether one positional XPath spread supplies a selected regular parameter.
fn dom_xpath_positional_spread_param_presence(
    ctx: &mut LoweringContext<'_, '_>,
    spread: crate::ir::ValueId,
    first_spread_param_idx: usize,
    param_idx: usize,
    span: crate::span::Span,
) -> crate::ir::ValueId {
    if param_idx < first_spread_param_idx {
        return emit_bool_literal(ctx, true, Some(span)).value;
    }
    let element_idx = param_idx - first_spread_param_idx;
    let len = ctx.emit_value(
        Op::ArrayLen,
        vec![spread],
        None,
        PhpType::Int,
        Op::ArrayLen.default_effects(),
        Some(span),
    );
    let index = emit_i64_at_span(ctx, element_idx as i64, span);
    ctx.emit_value(
        Op::ICmp,
        vec![len.value, index.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Sgt)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    )
    .value
}

/// One materialized associative XPath spread and its normalized operands.
struct LoweredDomXPathAssocSpreadArgs {
    operands: Vec<crate::ir::ValueId>,
    spread_expr: Expr,
}

/// Lowers a single associative XPath spread as named parameter reads by key.
fn lower_dom_xpath_assoc_spread_only_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> Option<LoweredDomXPathAssocSpreadArgs> {
    let [arg] = args else {
        return None;
    };
    let ExprKind::Spread(inner) = &arg.kind else {
        return None;
    };
    if !is_assoc_spread_source(ctx, inner) || sig.variadic.is_some() {
        return None;
    }
    let spread = lower_expr(ctx, inner);
    let spread_type = ctx.builder.value_php_type(spread.value);
    let temp_name = ctx.declare_hidden_temp(spread_type.clone());
    store_value_into_temp(ctx, &temp_name, spread_type, spread, arg.span);
    let spread_expr = Expr::new(ExprKind::Variable(temp_name), inner.span);
    let mut operands = Vec::with_capacity(sig.params.len());
    for (idx, (param_name, _)) in sig.params.iter().enumerate() {
        let default = sig.defaults.get(idx).and_then(|default| default.as_ref());
        let param_expr = assoc_spread_param_expr(&spread_expr, param_name, default, arg.span);
        operands.push(lower_expr(ctx, &param_expr).value);
    }
    Some(LoweredDomXPathAssocSpreadArgs {
        operands,
        spread_expr,
    })
}

/// One materialized positional prefix used by a named XPath spread argument plan.
struct LoweredDomXPathNamedSpreadArgs {
    operands: Vec<crate::ir::ValueId>,
    prefix_temp: Expr,
}

/// Lowers named XPath spreads without re-evaluating their dynamic prefix expressions.
fn lower_dom_xpath_named_args_with_spread_plan(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    assoc_spread_sources: &[bool],
) -> Option<LoweredDomXPathNamedSpreadArgs> {
    if assoc_spread_sources.iter().any(|is_assoc| *is_assoc) {
        return None;
    }
    let call_span = plan
        .source_args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let first_named_pos = plan.first_named_pos?;
    let prefix_expr = plan.positional_prefix_expr(call_span)?;
    let static_variadic_prefix_len = static_indexed_variadic_prefix_len(&prefix_expr);
    if sig.variadic.is_some() && static_variadic_prefix_len.is_none() {
        return None;
    }
    let prefix = lower_expr(ctx, &prefix_expr);
    let prefix_type = ctx.builder.value_php_type(prefix.value);
    let prefix_temp_name = ctx.declare_hidden_temp(prefix_type.clone());
    store_value_into_temp(ctx, &prefix_temp_name, prefix_type, prefix, prefix_expr.span);
    let single_prefix_spread = !matches!(prefix_expr.kind, ExprKind::ArrayLiteral(_));
    let prefix_temp = Expr::new(ExprKind::Variable(prefix_temp_name), prefix_expr.span);

    let mut source_values = vec![None; plan.source_args.len()];
    for (source_index, source_arg) in plan.source_args.iter().enumerate().skip(first_named_pos) {
        if matches!(source_arg.kind, ExprKind::Spread(_)) {
            return None;
        }
        source_values[source_index] = Some(lower_call_source_arg(ctx, source_arg));
    }
    if single_prefix_spread {
        if let [check] = plan.spread_bounds_checks.as_slice() {
            let prefix_value = lower_expr(ctx, &prefix_temp);
            emit_dom_xpath_named_spread_bounds_guard(ctx, prefix_value.value, check, call_span);
        }
    }

    let mut operands = Vec::with_capacity(plan.regular_args.len());
    for (param_idx, arg) in plan.regular_args.iter().enumerate() {
        match arg {
            crate::types::call_args::PlannedRegularArg::Source { source_index, .. } => {
                if *source_index < first_named_pos {
                    let expr = spread_element_expr_for_ir(
                        &prefix_temp,
                        param_idx,
                        None,
                        false,
                        plan.source_args
                            .get(*source_index)
                            .map(|arg| arg.span)
                            .unwrap_or(call_span),
                    );
                    operands.push(lower_expr(ctx, &expr).value);
                } else {
                    operands.push(source_values.get(*source_index).copied().flatten()?);
                }
            }
            crate::types::call_args::PlannedRegularArg::Default(default) => {
                operands.push(lower_expr(ctx, default).value);
            }
            crate::types::call_args::PlannedRegularArg::SpreadElement {
                prefix_element_idx,
                param_name,
                prefer_named_key,
                default,
                guaranteed_present,
                spread_span,
                ..
            } => {
                let expr = if let Some(default) = default {
                    if *guaranteed_present {
                        spread_element_expr_for_ir(
                            &prefix_temp,
                            *prefix_element_idx,
                            param_name.as_deref(),
                            *prefer_named_key,
                            *spread_span,
                        )
                    } else {
                        spread_element_or_default_expr_for_ir(
                            &prefix_temp,
                            *prefix_element_idx,
                            param_name.as_deref(),
                            *prefer_named_key,
                            default.clone(),
                            *spread_span,
                        )
                    }
                } else {
                    spread_element_expr_for_ir(
                        &prefix_temp,
                        *prefix_element_idx,
                        param_name.as_deref(),
                        *prefer_named_key,
                        *spread_span,
                    )
                };
                operands.push(lower_expr(ctx, &expr).value);
            }
        }
    }
    if sig.variadic.is_some() {
        let regular_param_count = crate::types::call_args::regular_param_count(sig);
        let tail = lower_named_spread_static_variadic_tail_hash(
            ctx,
            sig,
            &prefix_temp,
            static_variadic_prefix_len.unwrap_or(regular_param_count),
            regular_param_count,
            plan,
            &source_values,
            first_named_pos,
            call_span,
        );
        operands.push(tail.value);
    }
    Some(LoweredDomXPathNamedSpreadArgs {
        operands,
        prefix_temp,
    })
}

/// Lowers a dynamic associative XPath prefix before later named arguments.
fn lower_dom_xpath_dynamic_named_spread_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
) -> Option<LoweredDomXPathNamedSpreadArgs> {
    if !plan.prefix_has_dynamic_named_spread {
        return None;
    }
    let call_span = plan
        .source_args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let first_named_pos = plan.first_named_pos?;
    let prefix_expr = plan.positional_prefix_expr(call_span)?;
    let prefix = lower_expr(ctx, &prefix_expr);
    if !matches!(
        ctx.builder.value_php_type(prefix.value).codegen_repr(),
        PhpType::AssocArray { .. }
    ) {
        return None;
    }
    let prefix_type = ctx.builder.value_php_type(prefix.value);
    let prefix_temp_name = ctx.declare_hidden_temp(prefix_type.clone());
    store_value_into_temp(ctx, &prefix_temp_name, prefix_type, prefix, prefix_expr.span);
    let prefix_temp = Expr::new(ExprKind::Variable(prefix_temp_name), prefix_expr.span);

    let mut source_values = vec![None; plan.source_args.len()];
    for (source_index, source_arg) in plan.source_args.iter().enumerate().skip(first_named_pos) {
        if matches!(source_arg.kind, ExprKind::Spread(_)) {
            return None;
        }
        source_values[source_index] = Some(lower_call_source_arg(ctx, source_arg));
    }
    emit_dom_xpath_dynamic_named_prefix_duplicate_guards(
        ctx,
        sig,
        plan,
        &prefix_temp,
        first_named_pos,
    );

    let mut operands = Vec::with_capacity(plan.regular_args.len() + 1);
    for (param_idx, arg) in plan.regular_args.iter().enumerate() {
        match arg {
            crate::types::call_args::PlannedRegularArg::Source { source_index, .. } => {
                if *source_index < first_named_pos {
                    let expr = spread_element_expr_for_ir(
                        &prefix_temp,
                        param_idx,
                        None,
                        false,
                        plan.source_args
                            .get(*source_index)
                            .map(|arg| arg.span)
                            .unwrap_or(call_span),
                    );
                    operands.push(lower_expr(ctx, &expr).value);
                } else {
                    operands.push(source_values.get(*source_index).copied().flatten()?);
                }
            }
            crate::types::call_args::PlannedRegularArg::Default(default) => {
                operands.push(lower_expr(ctx, default).value);
            }
            crate::types::call_args::PlannedRegularArg::SpreadElement {
                prefix_element_idx,
                param_name,
                prefer_named_key,
                default,
                guaranteed_present,
                spread_span,
                ..
            } => {
                let expr = if let Some(default) = default {
                    if *guaranteed_present {
                        spread_element_expr_for_ir(
                            &prefix_temp,
                            *prefix_element_idx,
                            param_name.as_deref(),
                            *prefer_named_key,
                            *spread_span,
                        )
                    } else {
                        spread_element_or_default_expr_for_ir(
                            &prefix_temp,
                            *prefix_element_idx,
                            param_name.as_deref(),
                            *prefer_named_key,
                            default.clone(),
                            *spread_span,
                        )
                    }
                } else {
                    spread_element_expr_for_ir(
                        &prefix_temp,
                        *prefix_element_idx,
                        param_name.as_deref(),
                        *prefer_named_key,
                        *spread_span,
                    )
                };
                operands.push(lower_expr(ctx, &expr).value);
            }
        }
    }
    if sig.variadic.is_some() {
        operands.push(lower_variadic_tail_array(ctx, sig, &[]).value);
    }
    Some(LoweredDomXPathNamedSpreadArgs {
        operands,
        prefix_temp,
    })
}

/// Emits catchable duplicate guards for numeric dynamic-prefix keys overwritten by named args.
fn emit_dom_xpath_dynamic_named_prefix_duplicate_guards(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    prefix_temp: &Expr,
    first_named_pos: usize,
) {
    for source in &plan.source_values {
        if source.source_index() < first_named_pos {
            continue;
        }
        let Some(param_idx) = source.param_idx() else {
            continue;
        };
        let Some((param_name, _)) = sig.params.get(param_idx) else {
            continue;
        };
        emit_dom_xpath_dynamic_named_prefix_duplicate_guard(
            ctx,
            prefix_temp,
            param_idx,
            param_name,
            source.expr().span,
        );
    }
}

/// Emits one catchable duplicate guard for a numeric key in a dynamic XPath prefix.
fn emit_dom_xpath_dynamic_named_prefix_duplicate_guard(
    ctx: &mut LoweringContext<'_, '_>,
    prefix_temp: &Expr,
    param_idx: usize,
    param_name: &str,
    span: crate::span::Span,
) {
    let exists_expr = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("array_key_exists"),
            args: vec![
                Expr::new(ExprKind::IntLiteral(param_idx as i64), span),
                prefix_temp.clone(),
            ],
        },
        span,
    );
    let exists = lower_expr(ctx, &exists_expr);
    let ok = ctx
        .builder
        .create_named_block("call.dynamic_named_prefix.ok", Vec::new());
    let fatal = ctx
        .builder
        .create_named_block("call.dynamic_named_prefix.fatal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: exists.value,
        then_target: fatal,
        then_args: Vec::new(),
        else_target: ok,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal);
    terminate_dom_xpath_named_parameter_overwrite_error(ctx, param_name, span);

    ctx.builder.position_at_end(ok);
}

/// Emits named-after-spread bounds checks with catchable duplicate-overwrite errors.
fn emit_dom_xpath_named_spread_bounds_guard(
    ctx: &mut LoweringContext<'_, '_>,
    spread: crate::ir::ValueId,
    check: &crate::types::call_args::SpreadBoundsCheck,
    span: crate::span::Span,
) {
    if check.min_len == 0 && check.max_len.is_none() {
        return;
    }
    let len = ctx.emit_value(
        Op::ArrayLen,
        vec![spread],
        None,
        PhpType::Int,
        Op::ArrayLen.default_effects(),
        Some(span),
    );
    if check.min_len != 0 {
        let min = emit_i64_at_span(ctx, check.min_len as i64, span);
        let has_required_args = ctx.emit_value(
            Op::ICmp,
            vec![len.value, min.value],
            Some(Immediate::CmpPredicate(CmpPredicate::Sge)),
            PhpType::Bool,
            Op::ICmp.default_effects(),
            Some(span),
        );
        let ok = ctx
            .builder
            .create_named_block("call.named_spread.min.ok", Vec::new());
        let fatal = ctx
            .builder
            .create_named_block("call.named_spread.min.fatal", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: has_required_args.value,
            then_target: ok,
            then_args: Vec::new(),
            else_target: fatal,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(fatal);
        let message = ctx.intern_string("Fatal error: named argument spread length mismatch\n");
        ctx.builder.terminate(Terminator::Fatal { message });
        ctx.builder.position_at_end(ok);
    }
    let Some(max_len) = check.max_len else {
        return;
    };
    let max = emit_i64_at_span(ctx, max_len as i64, span);
    let within_bound = ctx.emit_value(
        Op::ICmp,
        vec![len.value, max.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Sle)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    let ok = ctx
        .builder
        .create_named_block("call.named_spread.max.ok", Vec::new());
    let fatal = ctx
        .builder
        .create_named_block("call.named_spread.max.fatal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: within_bound.value,
        then_target: ok,
        then_args: Vec::new(),
        else_target: fatal,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(fatal);
    if let Some(param_name) = check.max_len_param_name.as_deref() {
        terminate_dom_xpath_named_parameter_overwrite_error(ctx, param_name, span);
    } else {
        let message = ctx.intern_string("Fatal error: named argument spread length mismatch\n");
        ctx.builder.terminate(Terminator::Fatal { message });
    }
    ctx.builder.position_at_end(ok);
}

/// Returns whether one planned XPath regular argument originated from PHP source at runtime.
fn dom_xpath_planned_regular_arg_presence(
    ctx: &mut LoweringContext<'_, '_>,
    argument: &crate::types::call_args::PlannedRegularArg,
    prefix_temp: &Expr,
    span: crate::span::Span,
) -> crate::ir::ValueId {
    match argument {
        crate::types::call_args::PlannedRegularArg::Source { .. } => {
            emit_bool_literal(ctx, true, Some(span)).value
        }
        crate::types::call_args::PlannedRegularArg::Default(_) => {
            emit_bool_literal(ctx, false, Some(span)).value
        }
        crate::types::call_args::PlannedRegularArg::SpreadElement {
            prefix_element_idx,
            param_name,
            prefer_named_key,
            guaranteed_present,
            ..
        } => {
            if *guaranteed_present {
                return emit_bool_literal(ctx, true, Some(span)).value;
            }
            if *prefer_named_key {
                let key = param_name
                    .as_ref()
                    .map(|name| Expr::new(ExprKind::StringLiteral(name.clone()), span))
                    .unwrap_or_else(|| {
                        Expr::new(ExprKind::IntLiteral(*prefix_element_idx as i64), span)
                    });
                let exists = Expr::new(
                    ExprKind::FunctionCall {
                        name: Name::unqualified("array_key_exists"),
                        args: vec![key, prefix_temp.clone()],
                    },
                    span,
                );
                return lower_expr(ctx, &exists).value;
            }
            let prefix = lower_expr(ctx, prefix_temp);
            let len = ctx.emit_value(
                Op::ArrayLen,
                vec![prefix.value],
                None,
                PhpType::Int,
                Op::ArrayLen.default_effects(),
                Some(span),
            );
            let index = emit_i64_at_span(ctx, *prefix_element_idx as i64, span);
            ctx.emit_value(
                Op::ICmp,
                vec![len.value, index.value],
                Some(Immediate::CmpPredicate(CmpPredicate::Sgt)),
                PhpType::Bool,
                Op::ICmp.default_effects(),
                Some(span),
            )
            .value
        }
    }
}

/// Converts a dynamically unpacked third XPath operand to PHP boolean storage.
fn coerce_dom_xpath_register_node_ns_operand(
    ctx: &mut LoweringContext<'_, '_>,
    operands: &mut [crate::ir::ValueId],
    span: crate::span::Span,
) {
    let Some(value) = operands.get(2).copied() else {
        return;
    };
    if !matches!(
        ctx.builder.value_php_type(value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return;
    }
    let lowered = lowered_value_from_id(ctx, value);
    operands[2] = ctx.truthy(lowered, Some(span)).value;
}

/// Lowers the `Closure` rebinding methods on a closure (`Callable`) receiver:
/// `$closure->bindTo($newThis [, $scope])` and `$closure->call($newThis, ...$args)`.
/// Returns `None` for any other method so normal dispatch (and its diagnostics)
/// still apply. The `$scope` argument is accepted and ignored — visibility is
/// resolved at compile time in elephc's closed-world model.
pub(super) fn lower_closure_bind_method(
    ctx: &mut LoweringContext<'_, '_>,
    closure: &LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    match php_symbol_key(method).as_str() {
        "bindto" => {
            let new_this = match args.first() {
                Some(arg) => lower_expr(ctx, arg),
                None => lower_null(ctx, expr),
            };
            Some(emit_closure_bind(ctx, closure.value, new_this.value, expr))
        }
        "call" => {
            // `$closure->call($newThis, ...$args)`: bind `$this` then invoke the
            // bound closure with the remaining arguments in one step.
            let new_this = match args.first() {
                Some(arg) => lower_expr(ctx, arg),
                None => lower_null(ctx, expr),
            };
            let bound = emit_closure_bind(ctx, closure.value, new_this.value, expr);
            let call_args = &args[args.len().min(1)..];
            let arg_container =
                lower_untyped_descriptor_invoker_arg_container(ctx, call_args, expr.span)?;
            Some(ctx.emit_value(
                Op::CallableDescriptorInvoke,
                vec![bound.value, arg_container.value],
                callable_profile_immediate(),
                PhpType::Mixed,
                Op::CallableDescriptorInvoke.default_effects(),
                Some(expr.span),
            ))
        }
        _ => None,
    }
}

/// Emits the `closure_bind` runtime call that rebinds a closure's captured
/// `$this`, yielding a new closure (`Callable`) descriptor.
pub(super) fn emit_closure_bind(
    ctx: &mut LoweringContext<'_, '_>,
    closure: crate::ir::ValueId,
    new_this: crate::ir::ValueId,
    expr: &Expr,
) -> LoweredValue {
    ctx.emit_value(
        Op::ClosureBind,
        vec![closure, new_this],
        None,
        PhpType::Callable,
        Op::ClosureBind.default_effects(),
        Some(expr.span),
    )
}

/// Builds synthetic `__call` arguments when a class lacks the requested method.
pub(super) fn magic_call_dispatch_args(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
    args: &[Expr],
    span: Span,
) -> Option<Vec<Expr>> {
    if method_signature(ctx, object, method).is_some() {
        return None;
    }
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, _)) = singular_object_class(&object_ty) else {
        return None;
    };
    let normalized = class_name.trim_start_matches('\\');
    class_method_signature(ctx, normalized, &php_symbol_key("__call"))?;
    Some(vec![
        Expr::new(ExprKind::StringLiteral(method.to_string()), span),
        Expr::new(ExprKind::ArrayLiteral(args.to_vec()), span),
    ])
}

/// Returns the signature to use for method-call argument normalization.
pub(super) fn method_call_argument_signature(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
    object: crate::ir::ValueId,
    method: &str,
) -> Option<FunctionSig> {
    if method_is_fiber_start(ctx, object, method) {
        return crate::ir_lower::fibers::start_sig_for_expr(ctx, object_expr);
    }
    method_signature(ctx, object, method)
}

/// Returns true when a method call targets PHP's built-in `Fiber::start()`.
pub(super) fn method_is_fiber_start(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
) -> bool {
    if php_symbol_key(method) != "start" {
        return false;
    }
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, _)) = singular_object_class(&object_ty) else {
        return false;
    };
    php_symbol_key(class_name.trim_start_matches('\\')) == "fiber"
}

/// Lowers `?Object->method()` calls so null receivers fatal before argument evaluation.
pub(super) fn lower_nullable_regular_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let result_type = method_call_result_type(ctx, object.value, method, Op::MethodCall, expr);
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let fatal_block = ctx
        .builder
        .create_named_block("method.null.fatal", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("method.non_null.call", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("method.nullable.merge", Vec::new());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![object.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: fatal_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal_block);
    terminate_method_call_on_null(ctx, method);

    ctx.builder.position_at_end(call_block);
    let call = lower_method_call_with_receiver(ctx, object, method, args, Op::MethodCall, expr);
    store_value_into_temp(ctx, &temp_name, result_type.clone(), call, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}
