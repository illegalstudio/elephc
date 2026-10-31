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
    let object = refine_member_receiver(ctx, object, expr);
    if let Some(call) = enum_interface_calls::lower_enum_interface_call(ctx, object, method, args, op, expr) {
        return call;
    }
    lower_method_call_with_receiver_direct(ctx, object, method, args, op, expr)
}

/// Lowers an already selected concrete or fallback receiver without redispatching its interface.
pub(super) fn lower_method_call_with_receiver_direct(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, method: &str,
    args: &[Expr], op: Op, expr: &Expr,
) -> LoweredValue {
    lower_method_call_with_binding(ctx, object, method, args, op, expr, false)
}

/// Checks the selected enum signature without changing direct reference-return adoption.
pub(super) fn lower_selected_enum_method_call(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, method: &str,
    args: &[Expr], op: Op, expr: &Expr,
) -> LoweredValue {
    lower_method_call_with_binding(ctx, object, method, args, op, expr, true)
}

/// Keeps selected parameter validation inside the published argument and result-owner scope.
fn lower_method_call_with_binding(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, method: &str,
    args: &[Expr], op: Op, expr: &Expr, selected_enum: bool,
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
    let sig = method_signature(ctx, object.value, dispatch_method);
    promote_pdo_binding_ref_argument(ctx, object.value, dispatch_method, args);
    // A by-reference-returning method transfers a lease that must outlive this caller's
    // argument and receiver cleanup, so its staging is published before the arguments.
    let reference_staging = begin_reference_return_call(ctx, sig.as_ref(), expr.span);
    // An ordinary owned result gets the same protection as the reference lease: argument roots,
    // evaluation intermediates and an owning receiver all retire after the call and run PHP
    // destructors that can throw. Receiver and method alone decide the result type and the alias
    // summary, so both are available before any argument expression runs.
    let return_alias = method_return_arg_alias(ctx, object.value, dispatch_method);
    let result_staging = prepublish_user_call_result(
        ctx, sig.as_ref(), &return_alias, &result_type, expr.span,
    );
    let receiver_class = match ctx.builder.value_php_type(object.value).codegen_repr() {
        PhpType::Object(class_name) => Some(class_name.trim_start_matches('\\').to_string()),
        _ => None,
    };
    // A concrete local load may later unbox physical Mixed storage after eval widens
    // its slot. Acquiring a root is safe for both borrowed and independently owned loads.
    let (object, receiver_owner) = root_method_receiver(ctx, object, expr.span);
    let mut operands = vec![object.value];
    begin_call_argument_evaluation(ctx);
    let enum_args = singular_object_class(&ctx.builder.value_php_type(object.value))
        .and_then(|(class, _)| ctx.enums.contains_key(class).then(|| class.to_string()))
        .and_then(|class| sig.as_ref().and_then(|signature|
            enum_method_args::lower_arguments(ctx, signature, args, &class, dispatch_method, expr.span)
                .or_else(|| selected_enum.then(|| enum_static_args::lower_arguments(
                    ctx, signature, args, &class, dispatch_method, expr.span,
                )).flatten())));
    let call_description = receiver_class
        .map(|class_name| format!("{class_name}::{dispatch_method}"))
        .unwrap_or_else(|| dispatch_method.to_string());
    let previous_description = ctx.call_description.replace(call_description);
    let ref_place_rewrite = if enum_args.is_none() {
        sig.as_ref().and_then(|signature| {
            super::ref_place_args::rewrite_typed_property_ref_args(ctx, signature, args)
        })
    } else {
        None
    };
    let call_args = ref_place_rewrite
        .as_ref()
        .map_or(args, |rewrite| rewrite.args.as_slice());
    let arg_values =
        enum_args.unwrap_or_else(|| lower_args_with_signature(ctx, sig.as_ref(), call_args));
    ctx.call_description = previous_description;
    let mut arg_values = arg_values;
    let evaluation_intermediates = finish_call_argument_evaluation(ctx, &mut arg_values);
    let roots = root_user_call_operands(
        ctx,
        &mut arg_values,
        sig.as_ref(),
        &return_alias,
        &result_type,
        expr.span,
    );
    // Selected enum implementations may have fewer parameters than another implementor.
    // PHP ignores surplus positionals, but their values still belong to ordinary cleanup.
    let operand_limit = sig.as_ref().filter(|sig| sig.variadic.is_none())
        .filter(|_| singular_object_class(&ctx.builder.value_php_type(object.value))
            .is_some_and(|(class, _)| ctx.enums.contains_key(class)))
        .map_or(arg_values.len(), |sig| sig.params.len());
    operands.extend(arg_values.iter().take(operand_limit).copied());
    let data = ctx.intern_string(dispatch_method);
    let call = ctx.emit_value(
        op,
        operands,
        Some(Immediate::Data(data)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    let call = finish_reference_return_call(
        ctx, call, sig.as_ref(), reference_staging.as_ref(), expr.span,
    );
    stage_call_result(ctx, result_staging.as_ref(), call, expr.span);
    release_owned_call_arg_temporaries_with_roots(
        ctx,
        &arg_values,
        Some(call.value),
        &return_alias,
        sig.as_ref(),
        &roots,
        expr.span,
    );
    retire_call_argument_intermediates(ctx, &evaluation_intermediates);
    if let Some(rewrite) = ref_place_rewrite {
        super::ref_place_args::finish_ref_place_rewrite(ctx, rewrite, expr.span);
    }
    if let Some(owner) = receiver_owner {
        retire_owned_call_operand(ctx, owner, expr.span);
    } else {
        release_owning_receiver_temporary(ctx, object, expr.span);
    }
    let call = take_prepublished_call_result(ctx, result_staging, call, expr.span);
    finish_reference_return_value(ctx, call, reference_staging, expr.span)
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
    let receiver_type = ctx.builder.value_php_type(object.value);
    let receiver_name = ctx.declare_hidden_temp(receiver_type.clone());
    // The hidden slot is an owner, not a borrow of the caller's variable. Keep
    // the source representation so boxed nullable receivers are not moved as raw objects.
    let retained = crate::ir_lower::ownership::acquire_if_refcounted(ctx, object, Some(expr.span));
    ctx.store_local(&receiver_name, retained, receiver_type, Some(expr.span));
    if ctx.value_is_owning_temporary(object) {
        crate::ir_lower::ownership::release_if_owned(ctx, object, Some(expr.span));
    }
    let receiver_slot = ctx.local_slots[&receiver_name];
    register_owned_call_operand(ctx, receiver_slot, expr.span);
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
    let result = lower_expr(ctx, &call);
    retire_owned_call_operand(ctx, receiver_slot, expr.span);
    result
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
    release_owned_call_arg_temporaries_with_roots(
        ctx, args, result, return_alias, signature, &[], span,
    );
}

/// Retires scoped roots and ordinary temporaries in parameter order without renumbering alias facts.
pub(super) fn release_owned_call_arg_temporaries_with_roots(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[crate::ir::ValueId],
    result: Option<crate::ir::ValueId>,
    return_alias: &ReturnArgAlias,
    signature: Option<&FunctionSig>,
    roots: &[(usize, crate::ir::LocalSlotId)],
    span: Span,
) {
    let owned_object_result = result.is_some_and(|result| {
        matches!(ctx.builder.value_php_type(result).codegen_repr(), PhpType::Object(_))
            && signature.is_some_and(|signature| !signature.by_ref_return)
            && matches!(ctx.builder.value_defining_op(result),
                Some(Op::Call | Op::MethodCall | Op::NullsafeMethodCall | Op::StaticMethodCall))
    });
    let guarded_result = result.filter(|_| owned_object_result && args.iter().any(|value| {
        ctx.value_is_owning_temporary(LoweredValue {
            value: *value,
            ir_type: ctx.builder.value_type(*value),
        })
    }));
    if let Some(value) = guarded_result {
        guard_descriptor_container(ctx, LoweredValue {
            value,
            ir_type: ctx.builder.value_type(value),
        }, span);
    }
    for (parameter_index, value) in args.iter().enumerate() {
        ctx.unguard_call_argument(*value, span);
        if let Some((_, slot)) = roots.iter().find(|(index, _)| *index == parameter_index) {
            let pin_only = ctx.builder.value_defining_op(*value) == Some(Op::EnsureOwned)
                || (ctx.builder.value_defining_op(*value) == Some(Op::Acquire)
                    && ctx.builder.local_kind(*slot) == LocalKind::OwnedTemp);
            if !pin_only {
                retire_owned_call_operand(ctx, *slot, span);
                continue;
            }
            // Claimed strings and persisted byte views are only PINNED for the call: retire the
            // pin without releasing, then decide the release against the result below.
            unregister_owned_call_operand(ctx, *slot, span);
            ctx.emit_void(
                Op::UnsetLocal,
                Vec::new(),
                Some(Immediate::LocalSlot(*slot)),
                Op::UnsetLocal.default_effects(),
                Some(span),
            );
        }
        let reference_place_view_already_released = signature.is_some_and(|signature| {
            signature
                .ref_params
                .get(parameter_index)
                .copied()
                .unwrap_or(false)
        }) && matches!(
            ctx.builder.value_defining_op(*value),
            Some(Op::LoadLocal | Op::LoadRefCell)
        ) && ctx.builder.function().instructions.iter().any(|inst| {
            inst.op == Op::Release && inst.operands == [*value]
        });
        if reference_place_view_already_released {
            // Source-order evaluation already transferred this incidental value view into an
            // unwind-visible intermediate. The call operand remains the original load solely so
            // ABI materialization can recover its local slot; releasing that view again after a
            // successful call would free the same detached Mixed-to-string result twice.
            continue;
        }
        let php_type = ctx.builder.value_php_type(*value);
        let lowered = LoweredValue {
            value: *value,
            ir_type: value_ir_type(&php_type),
        };
        if ctx.value_needs_release_after_use(lowered) {
            // PHP callees acquire by-value array/hash/Mixed parameters into owning shadow slots.
            // By-value returns from reference parameters also acquire or clone a separate owner.
            // Their result therefore cannot be an unretained alias of the caller's argument.
            let callee_owns = signature
                .is_some_and(|signature| signature.by_ref_return
                    || signature.returned_parameter_has_independent_owner(parameter_index));
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
            if !owned_object_result && !callee_owns && !independently_boxed && result_reuses_arg {
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
                // A string argument the caller owns outright (claimed with `EnsureOwned`, an
                // acquired copy, or a `StrPersist` of a normalized string cast) is decided the
                // same way when a USER function produced the result. A `Str` result compares
                // by payload pointer, the string's whole
                // identity: a PHP function persists every builtin result it returns, so an equal
                // pointer means the callee handed the block back and a different one means the
                // caller still owns it. Any other result that can alias it is a boxed `Mixed`
                // (`: mixed`, `?string`, `string|int`), whose return coercion persists the string
                // into a cell of its own, so the argument is never the result's storage and is
                // released outright. A builtin result is excluded: `ltrim`, `substr` and friends
                // answer with a view INTO the argument, a different pointer that still borrows
                // the argument's bytes.
                if let Some(result) = result {
                    let arg_repr = ctx.builder.value_php_type(lowered.value).codegen_repr();
                    let result_repr = ctx.builder.value_php_type(result).codegen_repr();
                    let owned_string_arg = arg_repr == PhpType::Str
                        && matches!(
                            ctx.builder.value_defining_op(lowered.value),
                            Some(Op::EnsureOwned | Op::Acquire | Op::StrPersist)
                        );
                    let user_call_result = matches!(
                        ctx.builder.value_defining_op(result),
                        Some(
                            Op::Call
                                | Op::FunctionVariantCall
                                | Op::MethodCall
                                | Op::NullsafeMethodCall
                                | Op::StaticMethodCall
                                | Op::ClosureCall
                                | Op::ExprCall
                                | Op::CallableDescriptorInvoke
                        )
                    );
                    // A boxed user return owns its persisted string independently of the raw
                    // argument, including temporary calls and string casts from Mixed storage.
                    // Provisional local releases remain subject to backend ownership filtering.
                    if arg_repr == PhpType::Str && result_repr == PhpType::Mixed && user_call_result {
                        crate::ir_lower::ownership::release_if_owned(ctx, lowered, Some(span));
                        continue;
                    }
                    if owned_string_arg && user_call_result {
                        if result_repr == PhpType::Str {
                            ctx.emit_void(
                                Op::ReleaseUnlessAliases,
                                vec![lowered.value, result],
                                None,
                                Op::ReleaseUnlessAliases.default_effects(),
                                Some(span),
                            );
                        } else {
                            crate::ir_lower::ownership::release_if_owned(ctx, lowered, Some(span));
                        }
                        continue;
                    }
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
                    // `trim((string)$s)` is not itself the owned copy. The copy is the
                    // cast underneath it. A user call that may return its argument skips
                    // releasing `trim`'s result, so the copy has to be released here
                    // unless that call handed the same block back.
                    release_marked_cast_copies_unless_returned(ctx, lowered, result, span);
                }
                continue;
            }
            crate::ir_lower::ownership::release_if_owned(ctx, lowered, Some(span));
        }
    }
    if let Some(value) = guarded_result {
        ctx.unguard_call_argument(value, span);
    }
}

/// Releases a normalized string-cast copy nested under a call argument.
///
/// `id(trim((string)$s))` skips releasing `trim`'s result because the callee may
/// return that argument. The owned copy is the cast underneath `trim`. Free that
/// copy when the callee returned a different block, and leave it alone when the
/// callee handed the same block back.
fn release_marked_cast_copies_unless_returned(
    ctx: &mut LoweringContext<'_, '_>,
    argument: LoweredValue,
    result: ValueId,
    span: Span,
) {
    let Some(inst) = ctx.builder.value_defining_instruction(argument.value) else {
        return;
    };
    if inst.op != Op::RuntimeCall {
        return;
    }
    let copies = inst
        .operands
        .iter()
        .copied()
        .filter(|operand| {
            ctx.builder
                .value_defining_instruction(*operand)
                .is_some_and(|producer| {
                    producer.op == Op::StrPersist
                        && producer.immediate == Some(Immediate::Bool(true))
                })
        })
        .collect::<Vec<_>>();
    for copy in copies {
        ctx.emit_void(
            Op::ReleaseUnlessAliases,
            vec![copy, result],
            None,
            Op::ReleaseUnlessAliases.default_effects(),
            Some(span),
        );
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
pub(super) fn release_owning_receiver_temporary(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    span: Span,
) {
    if ctx.value_is_owning_temporary(receiver) {
        crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(span));
    }
}
