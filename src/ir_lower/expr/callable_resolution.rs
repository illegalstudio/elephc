//! Purpose:
//! Static callable resolution, Closure binding, and builtin signature lookup.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Returns whether `lower_static_callable_call` can lower this binding without descriptor state.
///
/// `LoadRefCell` is the conservative source marker for a capture whose stable cell is not
/// present in the binding. It always covers by-reference captures, and can also match a
/// by-value capture read from an already reference-bound local; descriptor fallback is safe for
/// that extra case. Re-materializing a matched value for a later direct call asks the backend
/// for the source local's current storage address. After `unset($source)`, however, that local
/// has detached from the cell retained by the closure descriptor, so the direct call would read
/// the cleared or rebound local instead of the captured cell. Until static bindings carry the
/// stable captured-cell pointer, that shape has to route through descriptor invocation.
///
/// Callers that must emit owner bookkeeping around the direct call consult this first, so the
/// decision is made before any instruction is emitted rather than by abandoning a half-lowered
/// call.
pub(super) fn static_callable_call_lowers_directly(
    ctx: &LoweringContext<'_, '_>,
    target: &StaticCallableBinding,
) -> bool {
    match target {
        StaticCallableBinding::InstanceMethod { direct_call, .. } => *direct_call,
        StaticCallableBinding::Closure { captures, .. } => !captures.iter().any(|capture| {
            ctx.builder.value_defining_op(capture.value) == Some(Op::LoadRefCell)
        }),
        _ => true,
    }
}

/// Lowers one resolved static callable target to the corresponding EIR call opcode.
pub(super) fn lower_static_callable_call(
    ctx: &mut LoweringContext<'_, '_>,
    target: StaticCallableBinding,
    callback_args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !static_callable_call_lowers_directly(ctx, &target) {
        return None;
    }
    match target {
        StaticCallableBinding::UserFunction(function_name) => {
            let sig = ctx.functions.get(&function_name).cloned();
            let php_type = call_return_type(ctx, &function_name, &[]);
            let return_alias = ctx.return_alias_summaries.function(&function_name)
                .cloned().unwrap_or(ReturnArgAlias::Unknown);
            let reference_staging = begin_reference_return_call(ctx, sig.as_ref(), expr.span);
            // The callable form gets the same result staging the identical direct call gets: the
            // argument roots and evaluation intermediates retire after the call and run PHP
            // destructors that can throw.
            let result_staging = prepublish_user_call_result(
                ctx, sig.as_ref(), &return_alias, &php_type, expr.span,
            );
            begin_call_argument_evaluation(ctx);
            let mut operands = lower_args_with_signature(ctx, sig.as_ref(), callback_args);
            let evaluation_intermediates = finish_call_argument_evaluation(ctx, &mut operands);
            let roots = root_user_call_operands(
                ctx, &mut operands, sig.as_ref(), &return_alias, &php_type, expr.span,
            );
            let data = ctx.intern_function_name(&function_name);
            let call = ctx.emit_value(
                Op::Call,
                operands.clone(),
                Some(Immediate::Data(data)),
                php_type,
                effects_lookup::user_call_effects(&function_name),
                Some(expr.span),
            );
            let call = finish_reference_return_call(
                ctx, call, sig.as_ref(), reference_staging.as_ref(), expr.span,
            );
            stage_call_result(ctx, result_staging.as_ref(), call, expr.span);
            release_owned_call_arg_temporaries_with_roots(
                ctx, &operands, Some(call.value), &return_alias, sig.as_ref(), &roots, expr.span,
            );
            retire_call_argument_intermediates(ctx, &evaluation_intermediates);
            let call = take_prepublished_call_result(ctx, result_staging, call, expr.span);
            Some(finish_reference_return_value(ctx, call, reference_staging, expr.span))
        }
        StaticCallableBinding::ExternFunction(function_name) => {
            let sig = ctx
                .extern_functions
                .get(&function_name)
                .map(function_sig_from_extern_for_descriptor);
            begin_call_argument_evaluation(ctx);
            let mut operands = lower_args_with_signature(ctx, sig.as_ref(), callback_args);
            let php_type = call_return_type(ctx, &function_name, &operands);
            let evaluation_intermediates = finish_call_argument_evaluation(ctx, &mut operands);
            let data = ctx.intern_function_name(&function_name);
            let call = ctx.emit_value(
                Op::ExternCall,
                operands.clone(),
                Some(Immediate::Data(data)),
                php_type,
                Op::ExternCall.default_effects(),
                Some(expr.span),
            );
            // A statically resolved extern callable consumes its operands exactly like the
            // direct extern call in `lower_function_call`, so a fresh owned argument, such
            // as a string built for a `call_user_func('c_fn', ...)` or a first-class callable
            // invocation, is released here instead of leaking once per call. The alias guard
            // keeps a pass-through result alive, because an extern result may point into the
            // very buffer this call passed in.
            release_owned_call_arg_temporaries(
                ctx,
                &operands,
                Some(call.value),
                &ReturnArgAlias::Unknown,
                expr.span,
            );
            retire_call_argument_intermediates(ctx, &evaluation_intermediates);
            Some(call)
        }
        StaticCallableBinding::Builtin(function_name) => {
            if let Some(value) =
                lower_class_introspection(ctx, &function_name, callback_args, expr)
            {
                return Some(value);
            }
            if let Some(value) =
                boxed_user_sort::lower_boxed_usort(ctx, &function_name, callback_args, expr)
            {
                return Some(value);
            }
            let sig = call_signature(
                ctx,
                &function_name,
                source_prefers_extension_builtin(&function_name),
            );
            // Source-order argument evaluation publishes each owned argument before the next one
            // is evaluated, so a later argument that throws cannot strand an earlier one. The
            // ledger must be balanced on this path too: `emit_builtin_call_value` releases the
            // final operands after a successful call, and only the intermediates it did not
            // consume are retired here.
            begin_call_argument_evaluation(ctx);
            let mut operands =
                lower_builtin_call_args(ctx, &function_name, sig.as_ref(), callback_args);
            let php_type = static_callable_builtin_result_type(
                ctx,
                &function_name,
                &operands,
                expr.span,
            );
            let evaluation_intermediates = finish_call_argument_evaluation(ctx, &mut operands);
            let call = emit_builtin_call_value(
                ctx,
                &function_name,
                operands,
                php_type,
                expr.span,
                None,
            );
            retire_call_argument_intermediates(ctx, &evaluation_intermediates);
            Some(call)
        }
        StaticCallableBinding::Closure {
            name,
            signature,
            captures,
        } => {
            // A capture without a stable cell already refused this binding above, so the direct
            // call below can be lowered without abandoning any emitted instruction.
            let reference_staging = begin_reference_return_call(ctx, Some(&signature), expr.span);
            begin_call_argument_evaluation(ctx);
            let mut arg_values = lower_args_with_signature(ctx, Some(&signature), callback_args);
            let php_type = normalize_value_php_type(signature.return_type.codegen_repr());
            let evaluation_intermediates = finish_call_argument_evaluation(ctx, &mut arg_values);
            let roots = root_user_call_operands(
                ctx, &mut arg_values, Some(&signature), &ReturnArgAlias::Unknown, &php_type, expr.span,
            );
            let mut operands = arg_values.clone();
            append_closure_capture_operands(&mut operands, &captures);
            let data = ctx.intern_function_name(&name);
            let call = ctx.emit_value(
                Op::Call,
                operands,
                Some(Immediate::Data(data)),
                php_type,
                effects_lookup::user_call_effects(&name),
                Some(expr.span),
            );
            let call = finish_reference_return_call(
                ctx, call, Some(&signature), reference_staging.as_ref(), expr.span,
            );
            release_owned_call_arg_temporaries_with_roots(
                ctx, &arg_values, Some(call.value), &ReturnArgAlias::Unknown,
                Some(&signature), &roots, expr.span,
            );
            retire_call_argument_intermediates(ctx, &evaluation_intermediates);
            Some(finish_reference_return_value(ctx, call, reference_staging, expr.span))
        }
        StaticCallableBinding::StaticMethod { receiver, method } => {
            Some(lower_static_method_call(ctx, &receiver, &method, callback_args, expr))
        }
        StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            Some(lower_static_method_descriptor_call(
                ctx,
                &receiver,
                &method,
                callback_args,
                expr,
            ))
        }
        StaticCallableBinding::InstanceMethod {
            object,
            method,
            direct_call: true,
            ..
        } => Some(lower_method_call(ctx, &object, &method, callback_args, Op::MethodCall, expr)),
        StaticCallableBinding::InstanceMethod { .. } => None,
    }
}

/// Resolves a PHP string callback using case-insensitive function lookup rules.
pub(super) fn resolve_static_string_callable(
    ctx: &LoweringContext<'_, '_>,
    callback: &str,
) -> Option<StaticCallableBinding> {
    let callback = callback.trim_start_matches('\\');
    if let Some((class_name, method)) = callback.rsplit_once("::") {
        let class_name = lookup_folded_name(ctx.classes.keys(), class_name.trim_start_matches('\\'))?;
        return resolve_static_method_callable(
            ctx,
            StaticReceiver::Named(Name::from(class_name)),
            method.to_string(),
        );
    }
    if let Some(function_name) = lookup_folded_name(ctx.extern_functions.keys(), callback) {
        return Some(StaticCallableBinding::ExternFunction(function_name));
    }
    if let Some(function_name) = canonical_builtin_function_name(callback) {
        return Some(StaticCallableBinding::Builtin(function_name));
    }
    if let Some(function_name) = lookup_folded_name(ctx.functions.keys(), callback) {
        return Some(StaticCallableBinding::UserFunction(function_name));
    }
    None
}

/// Appends captured closure values after caller-visible operands for hidden ABI params.
pub(super) fn append_closure_capture_operands(operands: &mut Vec<ValueId>, captures: &[ClosureCapture]) {
    operands.extend(captures.iter().map(|capture| capture.value));
}

/// Resolves a static method callback when class and method are compile-time known.
pub(super) fn resolve_static_method_callable(
    ctx: &LoweringContext<'_, '_>,
    receiver: StaticReceiver,
    method: String,
) -> Option<StaticCallableBinding> {
    static_method_implementation_signature(ctx, &receiver, &method)?;
    Some(StaticCallableBinding::StaticMethod { receiver, method })
}

/// Resolves a first-class instance-method callable to signature metadata only.
pub(super) fn resolve_instance_method_callable(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    method: String,
    direct_call: bool,
) -> Option<StaticCallableBinding> {
    let class_name = instance_callable_object_class(ctx, object)?;
    let method_key = php_symbol_key(&method);
    let signature = class_method_signature(ctx, &class_name, &method_key)?.clone();
    Some(StaticCallableBinding::InstanceMethod {
        object: Box::new(object.clone()),
        method,
        signature,
        direct_call,
    })
}

/// Returns a static callable only when it can be lowered without descriptor state.
pub(super) fn direct_static_callable_binding(target: StaticCallableBinding) -> Option<StaticCallableBinding> {
    if matches!(target, StaticCallableBinding::InstanceMethod { .. }) {
        None
    } else {
        Some(target)
    }
}

/// Resolves the concrete class for an object expression used in an instance FCC.
/// Returns the property type referenced by a `Closure::bind(fn () => $this->prop, $newThis, …)`
/// when the closure body is exactly `return $this->prop`: the bound object's property type.
/// The closure's own `$this` is Mixed (it is bound dynamically), so the type comes from the
/// bind's receiver argument.
pub(super) fn closure_bind_property_return_type(
    ctx: &LoweringContext<'_, '_>,
    callee: &Expr,
) -> Option<PhpType> {
    let ExprKind::StaticMethodCall { receiver, method, args } = &callee.kind else {
        return None;
    };
    if !method.eq_ignore_ascii_case("bind") {
        return None;
    }
    let crate::parser::ast::StaticReceiver::Named(name) = receiver else {
        return None;
    };
    if !name
        .as_str()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("Closure")
    {
        return None;
    }
    let ExprKind::Closure { body, .. } = &args.first()?.kind else {
        return None;
    };
    let new_this_class = instance_callable_object_class(ctx, args.get(1)?)?;
    let [stmt] = body.as_slice() else {
        return None;
    };
    let StmtKind::Return(Some(ret)) = &stmt.kind else {
        return None;
    };
    let ExprKind::PropertyAccess { object, property } = &ret.kind else {
        return None;
    };
    if !matches!(object.kind, ExprKind::This) {
        return None;
    }
    let info = ctx.classes.get(new_this_class.trim_start_matches('\\'))?;
    info.properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
}

/// Lowers `Closure::bind(fn &() => $this->prop, $newThis, scope)()` as a direct call to the
/// closure with `$newThis` boxed as its `$this` capture.
///
/// `Closure::bind` rebinds the closure's receiver; invoking the result through the generic
/// runtime descriptor invoker boxes the closure's return value as Mixed, which cannot carry a
/// by-reference property cell pointer. Calling the closure directly (as `$f()` does) passes the
/// cell pointer through. The call result is typed from the bound receiver's property so a
/// by-reference array return binds correctly. Only the auto-captured `$this` shape (the
/// `fn &() => $this->prop` form) is handled; other captures fall back to the generic path.
/// The bound descriptor the binding was derived from is not an operand of the direct call, but
/// it OWNS the boxed receiver that call passes as the closure's `$this`, so it is published in
/// the unwind chain before the arguments are lowered and retired once the call is complete.
/// Retiring it there is also what frees the receiver at the PHP-observable point: the bound
/// `Closure` of `Closure::bind(...)()` is a temporary, and its destruction is the last reference
/// an otherwise unreferenced receiver has. A refused direct lowering retires it too, leaving no
/// owner record and no leaked descriptor behind for the generic path the caller falls back to.
pub(super) fn lower_bound_closure_immediate_call(
    ctx: &mut LoweringContext<'_, '_>,
    callee: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !bound_closure_binding_shape_is_supported(ctx, callee) {
        return None;
    }
    let (bound, closure_value) = build_bound_closure_binding(ctx, callee, expr)?;
    if !static_callable_call_lowers_directly(ctx, &bound) {
        // The bound binding carries a boxed `$this` value, never a `LoadRefCell` capture, so
        // this is unreachable today. Retire the descriptor rather than leak it if it ever is.
        let (_, owner) = root_owned_call_operand(ctx, closure_value, expr.span);
        if let Some(slot) = owner {
            retire_owned_call_operand(ctx, slot, expr.span);
        }
        return None;
    }
    // The copied result of a by-value call is produced by the direct lowering, but the
    // descriptor's retirement below destroys the closure's captured environment and can throw.
    // Stage the result OUTSIDE the descriptor record so that throw cannot strand it.
    let result_staging = prepublish_static_callable_call_result(ctx, &bound, expr.span);
    let (_, descriptor_owner) = root_owned_call_operand(ctx, closure_value, expr.span);
    let result = lower_static_callable_call(ctx, bound, args, expr)
        .expect("a directly callable bound closure lowers its call");
    stage_call_result(ctx, result_staging.as_ref(), result, expr.span);
    if let Some(slot) = descriptor_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
    Some(take_prepublished_call_result(ctx, result_staging, result, expr.span))
}

/// Returns whether `build_bound_closure_binding` can complete without abandoning emitted IR.
///
/// Pure structural check. `closure_bind_property_return_type` already proves the callee is
/// `Closure::bind(<closure literal whose body is `return $this->prop`>, $newThis, …)`. What is
/// left is the capture count: a non-static closure whose body uses `$this` and declares no
/// `use` list captures exactly `$this`, which is the single shape the direct lowering supports.
/// Deciding it here keeps the closure literal from being lowered, abandoned and then lowered a
/// second time by the caller's generic fallback.
pub(super) fn bound_closure_binding_shape_is_supported(
    ctx: &LoweringContext<'_, '_>,
    callee: &Expr,
) -> bool {
    if closure_bind_property_return_type(ctx, callee).is_none() {
        return false;
    }
    let ExprKind::StaticMethodCall { args: bind_args, .. } = &callee.kind else {
        return false;
    };
    if bind_args.get(1).is_none() {
        return false;
    }
    matches!(
        bind_args.first().map(|arg| &arg.kind),
        Some(ExprKind::Closure { captures, is_static, .. }) if captures.is_empty() && !is_static
    )
}

/// Publishes result staging for a direct static-callable call whose result type is predictable.
///
/// Only the user-function and closure bindings are staged: their result type is read from the
/// signature alone, so the staged slot and the emitted call provably agree. A builtin binding's
/// result type depends on the lowered operands, and a static-method binding may be retyped by
/// late static binding, so neither can be declared before the arguments exist.
///
/// An enclosing reference assignment is excluded as well. Its own staging already owns the cell
/// the direct lowering hands back, and that raw cell is not the payload this slot would release.
pub(super) fn prepublish_static_callable_call_result(
    ctx: &mut LoweringContext<'_, '_>,
    target: &StaticCallableBinding,
    span: Span,
) -> Option<PrepublishedCallResult> {
    if ctx
        .reference_call_context
        .as_ref()
        .is_some_and(|context| context.depth == ctx.expression_depth)
    {
        return None;
    }
    let result_type = match target {
        StaticCallableBinding::UserFunction(name) => call_return_type(ctx, name, &[]),
        StaticCallableBinding::Closure { signature, .. } => {
            normalize_value_php_type(signature.return_type.codegen_repr())
        }
        _ => return None,
    };
    prepublish_call_result(ctx, &result_type, span)
}

/// Builds the static-callable binding for `Closure::bind(fn &() => $this->prop, $newThis, scope)`.
///
/// Lowers the closure literal ONCE, with the bound receiver's property type supplied as the
/// closure's contextual result type and its `$this` capture forced to the bind's `Mixed`
/// representation, then builds the BOUND descriptor from that same compiled closure. Returns the
/// bound descriptor together with the direct-call binding. `None` unless the call is the single
/// auto-captured `$this` shape, the only form whose `$this` is fully known at compile time.
/// Shared by the immediate-invoke path (`Closure::bind(...)()`) and the variable-assignment path
/// (`$b = Closure::bind(...)`), which materialize identically.
///
/// ONE RECEIVER BOX, ONE OWNER. The bound descriptor is a second `closure_new` over the same
/// compiled function, whose only capture is the receiver boxed as `Mixed`, the identical value
/// the direct call passes as the closure's hidden `$this` argument. The descriptor owns that box
/// and the call borrows it, so there is no second receiver reference anywhere: the box (and with
/// it the receiver) is freed exactly when the descriptor retires, which is what makes the bound
/// receiver's destructor run at the PHP-observable point. Routing through the runtime
/// `closure_bind` instead would box the receiver a SECOND time inside the descriptor and leave
/// this lowering's box owned by nobody.
///
/// ORDER. The literal is lowered first, in source order, so the closure's object handle is drawn
/// before anything `$newThis` evaluates allocates. It is then rooted across that evaluation,
/// which can throw. The bound descriptor's own staging slot is published before either root, so
/// the owner chain still unwinds strictly last-in-first-out: result slot, original descriptor,
/// receiver, then back out again.
///
/// Both callers gate on `bound_closure_binding_shape_is_supported` first, so the `None` arms
/// after the closure literal is lowered are unreachable defence rather than a live fallback that
/// would abandon an emitted descriptor and re-evaluate the whole expression.
pub(super) fn build_bound_closure_binding(
    ctx: &mut LoweringContext<'_, '_>,
    callee: &Expr,
    expr: &Expr,
) -> Option<(StaticCallableBinding, LoweredValue)> {
    let result_type = closure_bind_property_return_type(ctx, callee)?;
    let ExprKind::StaticMethodCall { args: bind_args, .. } = &callee.kind else {
        return None;
    };
    let closure_lit = bind_args.first()?;
    let new_this = bind_args.get(1)?.clone();
    // Lower the closure literal to obtain its static binding (function name + captures). The
    // bound property's type reaches the body, so the closure's own `return $this->prop` is
    // lowered against that representation instead of the untyped `Mixed` its `Mixed` `$this`
    // would otherwise infer.
    let original = lower_bound_this_closure_literal(ctx, closure_lit, &result_type)?;
    let Some(StaticCallableBinding::Closure {
        name,
        signature,
        captures,
    }) = ctx.take_pending_static_callable_result()
    else {
        return None;
    };
    // Only the single auto-captured `$this` shape is supported here.
    if captures.len() != 1 {
        return None;
    }
    // Published before the operand roots, so the descriptor this builds is reachable from the
    // unwind chain while those roots are retired and their destructors run.
    let result_staging = prepublish_call_result(ctx, &PhpType::Callable, expr.span);
    let (_, original_owner) = root_owned_call_operand(ctx, original, expr.span);
    let new_this_value = lower_expr(ctx, &new_this);
    let (new_this_value, receiver_owner) = root_owned_call_operand(ctx, new_this_value, expr.span);
    // The scope argument is still a PHP expression, even though this specialization already
    // knows the property layout. Evaluate it once in source order and keep its owner through
    // descriptor construction, just like the receiver argument.
    let scope_owner = bind_args.get(2).and_then(|scope| {
        let value = lower_expr(ctx, scope);
        root_owned_call_operand(ctx, value, scope.span).1
    });
    // Acquire before boxing so the box owns a reference of its own: the rooted receiver is still
    // owned by its operand-owner slot, which is retired separately below.
    let receiver_for_capture =
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, new_this_value, Some(expr.span));
    // Boxing a value that ALREADY represents as `Mixed` (a nullable `?C` receiver) emits nothing
    // and then releases its source, which would hand the descriptor a reference no box took.
    // Such a receiver is already in the capture's representation, so it is captured as it stands.
    let boxed_this =
        if ctx.builder.value_php_type(receiver_for_capture.value).codegen_repr() == PhpType::Mixed {
            receiver_for_capture
        } else {
            ctx.box_value_as_mixed(receiver_for_capture, PhpType::Mixed, Some(expr.span))
        };
    let data = ctx.intern_string(&name);
    let bound_descriptor = ctx.emit_value(
        Op::ClosureNew,
        vec![boxed_this.value],
        Some(Immediate::Data(data)),
        PhpType::Callable,
        Op::ClosureNew.default_effects(),
        Some(expr.span),
    );
    stage_call_result(ctx, result_staging.as_ref(), bound_descriptor, expr.span);
    if let Some(slot) = scope_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
    if let Some(slot) = receiver_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
    if let Some(slot) = original_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
    let bound_descriptor =
        take_prepublished_call_result(ctx, result_staging, bound_descriptor, expr.span);
    let bound = StaticCallableBinding::Closure {
        name,
        signature,
        captures: vec![ClosureCapture {
            value: boxed_this.value,
        }],
    };
    Some((bound, bound_descriptor))
}

/// Returns true when an assignment value is a by-reference `Closure::bind` of the auto-`$this`
/// shape, so the assignment should track the result as a static callable (routing a later
/// `$b()` through the direct-call path that carries the by-reference cell pointer).
///
/// Read-only structural check (no IR emitted) used to set `direct_closure` before lowering.
pub(crate) fn is_bound_closure_assignment_shape(ctx: &LoweringContext<'_, '_>, value: &Expr) -> bool {
    let ExprKind::StaticMethodCall { args, .. } = &value.kind else {
        return false;
    };
    let Some(ExprKind::Closure { by_ref_return, .. }) = args.first().map(|arg| &arg.kind) else {
        return false;
    };
    *by_ref_return && closure_bind_property_return_type(ctx, value).is_some()
}

/// Lowers `$b = Closure::bind(fn &() => $this->prop, $newThis, scope)` for assignment: builds
/// the bound-closure binding, publishes it as the pending static callable so the assignment
/// registers `$b` for later direct `$b()` calls, and returns the BOUND closure descriptor to
/// store in `$b`. Storing the bound descriptor is what keeps a use of `$b` that bypasses the
/// direct-call path (`call_user_func($b)`, a callable argument, a stored callback) running
/// against the same receiver the direct call uses; it also makes `unset($b)` release the
/// receiver, since the descriptor is the sole owner of the boxed `$this`. `None` for any
/// non-matching shape so normal assignment lowering applies.
pub(crate) fn lower_bound_closure_for_assignment(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
) -> Option<LoweredValue> {
    if !bound_closure_binding_shape_is_supported(ctx, value) {
        return None;
    }
    let (bound, closure_value) = build_bound_closure_binding(ctx, value, value)?;
    ctx.set_pending_static_callable_result(bound);
    Some(closure_value)
}

/// Resolves the statically-known class name of an object expression used as an instance-call
/// receiver, including declared property and chained-call results.
pub(super) fn instance_callable_object_class(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
) -> Option<String> {
    instance_callable_object_class_and_nullability(ctx, object).map(|(class_name, _)| class_name)
}

/// Resolves one instance-call receiver class and whether its expression may produce `null`.
pub(super) fn instance_callable_object_class_and_nullability(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
) -> Option<(String, bool)> {
    let object_type = match &object.kind {
        ExprKind::Variable(name) => ctx.local_types.get(name).cloned()?,
        ExprKind::This => PhpType::Object(ctx.current_class.clone()?),
        ExprKind::NewObject { class_name, .. } => PhpType::Object(class_name.to_string()),
        ExprKind::NewDynamicObject { fallback_class, .. } => {
            PhpType::Object(fallback_class.to_string())
        }
        ExprKind::FunctionCall { name, .. } => ctx
            .functions
            .get(name.as_str())
            .map(|sig| sig.return_type.clone())?,
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)?
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            nullsafe_property_access_expr_type_for_ir(ctx, object, property)?
        }
        ExprKind::MethodCall { object, method, .. } => {
            method_call_expr_type_for_ir(ctx, object, method)?
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            nullsafe_method_call_expr_type_for_ir(ctx, object, method)?
        }
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } => static_method_call_expr_type_for_ir(ctx, receiver, method)?,
        _ => infer_expr_type_syntactic(object),
    };
    let (class_name, nullable) = singular_object_class(&object_type)?;
    normalized_class_name(class_name).map(|class_name| (class_name, nullable))
}

/// Trims PHP's optional leading namespace separator from class metadata names.
pub(super) fn normalized_class_name(class_name: &str) -> Option<String> {
    let class_name = class_name.trim_start_matches('\\');
    if class_name.is_empty() {
        None
    } else {
        Some(class_name.to_string())
    }
}

/// Looks up a PHP function name case-insensitively and returns the canonical candidate.
pub(super) fn lookup_folded_name<'a, I>(names: I, requested: &str) -> Option<String>
where
    I: IntoIterator<Item = &'a String>,
{
    let requested = php_symbol_key(requested);
    names
        .into_iter()
        .find(|candidate| php_symbol_key(candidate) == requested)
        .cloned()
}

/// Returns the caller-visible signature used to normalize direct call operands.
pub(super) fn call_signature(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    prefer_extension_builtin: bool,
) -> Option<FunctionSig> {
    if prefer_extension_builtin {
        return builtin_call_signature(name);
    }
    if let Some(sig) = ctx.functions.get(name) {
        return Some(sig.clone());
    }
    if let Some(sig) = ctx.extern_functions.get(name) {
        return Some(function_sig_from_extern_for_descriptor(sig));
    }
    builtin_call_signature(name)
}

/// Returns whether the active source profile must prefer an elephc extension over a shadow.
pub(super) fn source_prefers_extension_builtin(name: &str) -> bool {
    !crate::strict_php::is_enabled()
        && crate::types::checker::builtins::catalog::strict_php_hidden_builtin_for_profile(
            &php_symbol_key(name.trim_start_matches('\\')),
            true,
        )
}

/// Looks up a PHP builtin call signature using the normalized global builtin name.
pub(super) fn builtin_call_signature(name: &str) -> Option<FunctionSig> {
    crate::types::builtin_call_sig(&php_symbol_key(name.trim_start_matches('\\')))
}

/// Looks up precise first-class builtin metadata using the normalized global builtin name.
pub(super) fn first_class_builtin_signature(name: &str) -> Option<FunctionSig> {
    crate::types::first_class_callable_builtin_sig(&php_symbol_key(name.trim_start_matches('\\')))
}
