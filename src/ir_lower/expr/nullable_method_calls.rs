//! Purpose:
//! Nullable method calls and ownership-aware call cleanup.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Emits the PHP fatal terminator for an ordinary method call on null.
pub(super) fn terminate_method_call_on_null(ctx: &mut LoweringContext<'_, '_>, method: &str) {
    let message = format!("Call to a member function {}() on null", method);
    let message = ctx.intern_string(&message);
    ctx.emit_void(
        Op::ThrowError,
        Vec::new(),
        Some(Immediate::Data(message)),
        Op::ThrowError.default_effects(),
        None,
    );
    ctx.builder.terminate(Terminator::Unreachable);
}

/// Lowers a nullsafe method call with lazy argument evaluation for nullable receivers.
pub(super) fn lower_nullsafe_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object);
    let object_ty = ctx.builder.value_php_type(object.value);
    if value_is_definitely_null(ctx, object.value) {
        return lower_boxed_null(ctx, expr);
    }
    let Some((_, true)) = singular_object_class(&object_ty) else {
        return lower_method_call_with_receiver(
            ctx,
            object,
            method,
            args,
            Op::NullsafeMethodCall,
            expr,
        );
    };
    let result_type = method_call_result_type(
        ctx,
        object.value,
        method,
        Op::NullsafeMethodCall,
        expr,
    );
    let temp_name = ctx.declare_hidden_temp(result_type.clone());
    let null_block = ctx.builder.create_named_block("nullsafe.method.null", Vec::new());
    let call_block = ctx.builder.create_named_block("nullsafe.method.call", Vec::new());
    let merge = ctx.builder.create_named_block("nullsafe.method.merge", Vec::new());
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
        then_target: null_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    let null_value = lower_null(ctx, expr);
    let null_value = if result_type.codegen_repr() == PhpType::Mixed {
        ctx.box_value_as_mixed(null_value, result_type.clone(), Some(expr.span))
    } else {
        null_value
    };
    store_value_into_temp(ctx, &temp_name, result_type.clone(), null_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(call_block);
    let call = lower_method_call_with_receiver(
        ctx,
        object,
        method,
        args,
        Op::NullsafeMethodCall,
        expr,
    );
    store_value_into_temp(ctx, &temp_name, result_type.clone(), call, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(expr.span))
}

/// Lowers a method call using an already evaluated receiver value.
pub(super) fn lower_method_call_with_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    if op == Op::MethodCall && is_reflection_class_new_instance_call(ctx, object.value, method) {
        return lower_reflection_class_new_instance(ctx, None, object, args, expr);
    }
    if op == Op::MethodCall && is_reflection_class_new_instance_args_call(ctx, object.value, method)
    {
        return lower_reflection_class_new_instance_args(ctx, None, object, args, expr);
    }
    if op == Op::MethodCall
        && is_reflection_class_new_instance_without_constructor_call(ctx, object.value, method)
    {
        return lower_reflection_class_new_instance_without_constructor(ctx, object, args, expr);
    }
    let magic_args;
    let (dispatch_method, args) =
        if let Some(args) = magic_call_dispatch_args(ctx, object.value, method, args, expr.span) {
            magic_args = args;
            ("__call", magic_args.as_slice())
        } else {
            (method, args)
        };
    let result_type = method_call_result_type(ctx, object.value, dispatch_method, op, expr);
    let mut operands = vec![object.value];
    let sig = method_signature(ctx, object.value, dispatch_method);
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

/// Lowers a nullsafe dynamic instance method call after the receiver was evaluated and guarded.
///
/// The non-null receiver is stored in a hidden temp so the existing
/// `call_user_func([$obj, $method], ...)` lowering can be reused without
/// evaluating the original receiver expression again.
pub(in crate::ir_lower) fn lower_dynamic_method_call_with_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let receiver_type = strip_void_from_union(ctx.builder.value_php_type(object.value));
    let receiver_name = ctx.declare_hidden_temp(receiver_type.clone());
    ctx.store_local(&receiver_name, object, receiver_type, Some(expr.span));
    let receiver = Expr::new(ExprKind::Variable(receiver_name), expr.span);
    let callback = Expr::new(
        ExprKind::ArrayLiteral(vec![receiver, method.clone()]),
        Span::dummy(),
    );
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(callback);
    call_args.extend(args.iter().cloned());
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("call_user_func"),
            args: call_args,
        },
        expr.span,
    );
    lower_expr(ctx, &call)
}

/// Releases normalized call arguments that cannot be returned by this call.
pub(super) fn release_owned_call_arg_temporaries(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[crate::ir::ValueId],
    result: Option<crate::ir::ValueId>,
    return_alias: &ReturnArgAlias,
    span: Span,
) {
    release_owned_call_arg_temporaries_with_signature(
        ctx,
        args,
        result,
        return_alias,
        None,
        span,
    );
}

/// Releases call arguments while accounting for fresh Mixed boxes created by the ABI.
pub(super) fn release_owned_call_arg_temporaries_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[crate::ir::ValueId],
    result: Option<crate::ir::ValueId>,
    return_alias: &ReturnArgAlias,
    signature: Option<&FunctionSig>,
    span: Span,
) {
    for (parameter_index, value) in args.iter().enumerate() {
        let php_type = ctx.builder.value_php_type(*value);
        let lowered = LoweredValue {
            value: *value,
            ir_type: value_ir_type(&php_type),
        };
        if ctx.value_is_owning_temporary(lowered) {
            // PHP callees acquire by-value array/hash parameters into owning COW shadow slots.
            // Their result therefore cannot be an unretained alias of the caller's argument.
            let callee_owns = signature
                .is_some_and(|signature| signature.param_is_callee_owned(parameter_index));
            let independently_boxed = signature.is_some_and(|signature| {
                call_arg_gets_independent_mixed_box(signature, parameter_index, &php_type)
            });
            // The call result reuses this argument's payload — so the argument release
            // must be suppressed and the ownership left to flow through the result —
            // in either of two cases:
            //  - a summary that *may* return this parameter, under the conservative
            //    alias check (which excludes fresh checked-arithmetic boxes so an
            //    unproven callee still releases them, issue #486); or
            //  - a callee *proven* to return this parameter, where even a fresh
            //    boxed `$i + 1` argument is handed straight back and must not be
            //    released twice (issue #604).
            // `ReturnArgAlias::Parameters` is a MAY summary (a union over branches), so
            // `proven_aliases_parameter` also holds for a callee that returns the
            // parameter only conditionally (`if ($c) return $x; return 7;`). Suppressing
            // the argument release on every path then leaks the owned box on the runtime
            // paths that do not return it — the same deliberate leak-over-crash trade-off
            // the `may_alias` suppression already makes for array/hash arguments. A
            // follow-up issue tracks runtime alias disambiguation.
            // Whether the runtime can settle the question itself: two boxed payloads compare
            // as single pointers, so `ReleaseUnlessAliases` can decide per call. Where that
            // holds, the leak-versus-double-free trade-off below does not have to be made at
            // COMPILE time at all, which is what lets the fresh-box exclusion be lifted for a
            // MAY summary — see the third disjunct.
            let conditionally_releasable = result.is_some_and(|result| {
                let arg_repr = ctx.builder.value_php_type(*value).codegen_repr();
                let result_repr = ctx.builder.value_php_type(result).codegen_repr();
                matches!(arg_repr, PhpType::Mixed | PhpType::Union(_))
                    && matches!(result_repr, PhpType::Mixed | PhpType::Union(_))
            });
            let result_reuses_arg = result.is_some_and(|result| {
                (return_alias.may_alias_parameter(parameter_index)
                    && ctx.call_result_may_alias_arg(*value, result))
                    || (return_alias.proven_aliases_parameter(parameter_index)
                        && ctx.arg_and_result_types_can_alias(*value, result))
                    // A MAY summary whose argument is a fresh checked-arithmetic box.
                    // `call_result_may_alias_arg` excludes that shape on purpose: with no proof
                    // the callee hands it back, an unconditional SKIP leaks it once per call
                    // (issue #486). But the choice it was avoiding is only forced when the
                    // release has to be decided statically. `fib($n - 1)` is exactly this: the
                    // summary is `Unknown` because the body calls itself, the argument is a
                    // fresh box, and the base case `return $n;` hands it straight back — so the
                    // caller released the argument AND the result, the same cell twice, and
                    // `fib()` answered its own predecessor.
                    || (return_alias.may_alias_parameter(parameter_index)
                        && conditionally_releasable
                        && ctx.arg_and_result_types_can_alias(*value, result))
            });
            if !callee_owns && !independently_boxed && result_reuses_arg {
                // Both suppression reasons above are MAY facts, so an unconditional skip is
                // right only on the calls that actually hand the payload back. Emitting a
                // conditional release instead lets each call decide at runtime: the codegen
                // compares the returned payload against this argument and releases it only
                // when they differ (issue #619). On a callee that genuinely always returns
                // the parameter the comparison always matches, so the #604 behaviour is
                // unchanged.
                // Only when the two payloads are directly comparable as single pointers. A
                // boxed `Mixed` result that *wraps* the argument's container holds a different
                // pointer than the container itself, so comparing them would read "not aliased"
                // for a value the result does own, and release it twice. Same restriction the
                // ABI-side cleanup slots already apply via `call_result_can_alias_mixed_temp`.
                if let Some(result) = result {
                    let arg_repr = ctx.builder.value_php_type(lowered.value).codegen_repr();
                    let result_repr = ctx.builder.value_php_type(result).codegen_repr();
                    let comparable = matches!(arg_repr, PhpType::Mixed | PhpType::Union(_))
                        && matches!(result_repr, PhpType::Mixed | PhpType::Union(_));
                    if comparable {
                        ctx.emit_void(
                            Op::ReleaseUnlessAliases,
                            vec![lowered.value, result],
                            None,
                            Op::ReleaseUnlessAliases.default_effects(),
                            Some(span),
                        );
                    }
                }
                continue;
            }
            crate::ir_lower::ownership::release_if_owned(ctx, lowered, Some(span));
        }
    }
}

/// Returns true when ABI materialization wraps a concrete argument in fresh Mixed storage.
pub(super) fn call_arg_gets_independent_mixed_box(
    signature: &FunctionSig,
    parameter_index: usize,
    source_type: &PhpType,
) -> bool {
    if signature
        .ref_params
        .get(parameter_index)
        .copied()
        .unwrap_or(false)
    {
        return false;
    }
    signature
        .params
        .get(parameter_index)
        .is_some_and(|(_, parameter_type)| {
            parameter_type.codegen_repr() == PhpType::Mixed
                && !matches!(
                    source_type.codegen_repr(),
                    PhpType::Mixed | PhpType::Union(_)
                )
        })
}

/// Makes a borrowed read result independent from an owning receiver before releasing it.
///
/// Property and indexed reads can return strings, arrays, objects, or callables
/// borrowed from the receiver. When that receiver is an owned temporary — notably
/// an object retained while unboxing a Mixed local — releasing it first could
/// destroy the result payload. Reads that already materialize an independent owned
/// value must not be acquired a second time.
pub(super) fn stabilize_borrowed_result_and_release_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    result: LoweredValue,
    span: Span,
) -> LoweredValue {
    if !ctx.value_is_owning_temporary(receiver) {
        return result;
    }
    let result = if ctx.value_is_owning_temporary(result) {
        result
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, result, Some(span))
    };
    crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(span));
    result
}

/// Releases the receiver of a method call when it was an owning temporary.
///
/// A method borrows its receiver, so a receiver that is itself a temporary — the
/// result of a prior chained call (`$o->a()->b()`) or an inline `new X()->m()` —
/// has no owner once the call returns and would otherwise never reach refcount
/// zero (a leak; its destructor never runs). A plain local or `$this` receiver is
/// not an owning temporary and is left to normal scope cleanup. This must run
/// after the call is emitted (and after `return $this` has acquired its own
/// reference) so the released reference is the receiver's, not the result's.
pub(crate) fn release_owning_receiver_temporary(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    span: Span,
) {
    if ctx.value_is_owning_temporary(receiver) {
        crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(span));
    }
}
