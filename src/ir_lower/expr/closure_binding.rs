//! Purpose:
//! Lowers native closure rebinding and its explicit visibility scope.
//!
//! Called from:
//! - Method-call and static-method-call EIR lowering.
//!
//! Key details:
//! - Binding operands stay rooted across later argument evaluation and callback unwinding.
//! - Known closure identities use direct scope clones; runtime identities carry typed scope metadata.

use super::*;

/// Lowers the `Closure` rebinding methods on a closure (`Callable`) receiver:
/// `$closure->bindTo($newThis [, $scope])` and `$closure->call($newThis, ...$args)`.
/// Returns `None` for any other method so normal dispatch (and its diagnostics)
/// still apply. A statically known explicit `$scope` receives its own closure
/// function specialization, preserving PHP member visibility without changing
/// the original closure's lexical scope.
pub(super) fn lower_closure_bind_method(
    ctx: &mut LoweringContext<'_, '_>,
    closure_expr: &Expr,
    closure: &LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let method = php_symbol_key(method);
    let result_type = match method.as_str() {
        "bindto" => PhpType::Callable,
        "call" => PhpType::Mixed,
        _ => return None,
    };
    if method == "bindto" {
        return Some(lower_native_closure_rebinding(
            ctx, closure_expr, *closure, args.first(), args.get(1), false, expr,
        ));
    }
    // An escaped closure, such as a global replaced by a destructor or an error handler, is
    // stored in a boxed Mixed cell once its compile-time callable identity is invalidated. The
    // runtime binder consumes the raw callable descriptor, not the address of that Mixed box.
    // Extract an owning descriptor view at the same boundary used for declared callable
    // parameters before publishing the bind operands for unwind cleanup.
    let closure = if matches!(
        ctx.builder.value_php_type(closure.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        unbox_callable_param_storage(ctx, *closure, Some(closure_expr.span))
    } else {
        *closure
    };
    // Binding retains its own receiver and environment. Original temporary inputs still
    // need their own retirement, including a throw while evaluating the new receiver or
    // invocation arguments. The result must outlive both original-input cleanup records.
    let result_staging = prepublish_call_result(ctx, &result_type, expr.span);
    let (closure, closure_owner) = root_closure_bind_operand(ctx, closure, expr.span);
    let new_this = match args.first() {
        Some(arg) => lower_expr(ctx, arg),
        None => lower_null(ctx, expr),
    };
    let (new_this, receiver_owner) = root_closure_bind_operand(ctx, new_this, expr.span);
    let bound = emit_closure_bind_with_scope(
        ctx, closure.value, new_this.value, None, None, expr,
    );
    let result = if method == "call" {
        let call_args = &args[args.len().min(1)..];
        let bound = root_descriptor_callback(ctx, bound, PhpType::Mixed, expr.span);
        let arg_container =
            lower_untyped_descriptor_invoker_arg_container(ctx, call_args, expr.span);
        emit_callable_descriptor_invoke(ctx, bound, arg_container, false, expr.span)
    } else {
        bound
    };
    stage_call_result(ctx, result_staging.as_ref(), result, expr.span);
    if let Some(slot) = receiver_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
    if let Some(slot) = closure_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
    Some(take_prepublished_call_result(ctx, result_staging, result, expr.span))
}

/// A statically resolved closure function cloned for one explicit PHP visibility scope.
pub(super) struct ScopedClosureBindPlan {
    name: String,
    signature: FunctionSig,
    return_alias: ReturnArgAlias,
}

/// Plans a scope-specialized binding without emitting EIR or evaluating source expressions.
///
/// The generic runtime binder copies a descriptor, so it cannot alter the already compiled
/// function's lexical member-access scope. When both the closure and scope class are statically
/// known, clone that EIR function and assign the requested lexical class to the clone. The source
/// function remains unchanged for unbound calls and bindings using another scope.
pub(super) fn plan_scoped_closure_bind(
    ctx: &mut LoweringContext<'_, '_>,
    closure_expr: &Expr,
    scope_expr: &Expr,
) -> Option<ScopedClosureBindPlan> {
    let binding = static_callable_binding_for_expr(ctx, closure_expr)
        .or_else(|| ctx.take_pending_static_callable_result())?;
    let StaticCallableBinding::Closure {
        name,
        signature,
        captures,
        return_alias,
        ..
    } = binding
    else {
        return None;
    };
    let requested_scope = static_callable_class_name(ctx, scope_expr)?;
    let scope = lookup_folded_name(
        ctx.classes.keys(),
        requested_scope.trim_start_matches('\\'),
    )?;
    if ctx.classes.get(&scope)?.is_internal { return None; }
    let mut function = ctx.closure_function(&name)?.clone();
    let capture_params = function
        .params
        .get(function.params.len().checked_sub(captures.len())?..)?;
    let [this_param] = capture_params else {
        return None;
    };
    if this_param.name != "this"
        || this_param.php_type.codegen_repr() != PhpType::Mixed
        || this_param.by_ref
    {
        return None;
    }
    let scoped_name = ctx.next_closure_name();
    function.name = scoped_name.clone();
    function.lexical_class = Some(scope);
    ctx.extend_closures([function]);
    Some(ScopedClosureBindPlan {
        name: scoped_name,
        signature,
        return_alias,
    })
}

/// Lowers static binding while keeping eval-owned Closure objects on their existing bridge.
pub(super) fn lower_static_closure_bind(
    ctx: &mut LoweringContext<'_, '_>, args: &[Expr], expr: &Expr,
) -> LoweredValue {
    let closure = lower_expr(ctx, &args[0]);
    if ctx.has_eval_barrier() && matches!(
        ctx.builder.value_php_type(closure.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        let mut operands = vec![closure.value];
        operands.extend(args.iter().skip(1).map(|arg| lower_expr(ctx, arg).value));
        let data = ctx.intern_string("Closure::bind");
        return ctx.emit_value(Op::EvalStaticMethodCall, operands, Some(Immediate::Data(data)),
            PhpType::Mixed, Op::EvalStaticMethodCall.default_effects(), Some(expr.span));
    }
    lower_native_closure_rebinding(ctx, &args[0], closure, args.get(1), args.get(2), true, expr)
}

/// Publishes empty argument owners in reverse cleanup order before later arguments can throw.
fn lower_native_closure_rebinding(
    ctx: &mut LoweringContext<'_, '_>,
    closure_expr: &Expr,
    closure: LoweredValue,
    receiver_expr: Option<&Expr>,
    scope_expr: Option<&Expr>,
    static_form: bool,
    expr: &Expr,
) -> LoweredValue {
    let source_type = ctx.builder.value_php_type(closure.value).clone();
    if !matches!(source_type.codegen_repr(), PhpType::Callable | PhpType::Mixed) {
        crate::ir_lower::diagnostics::refuse(closure_expr.span,
            "Native closure binding requires a closure value");
        return lower_null(ctx, expr);
    }
    let plan = scope_expr.and_then(|scope| plan_scoped_closure_bind(ctx, closure_expr, scope));
    let result_staging = prepublish_call_result(ctx, &PhpType::Callable, expr.span);
    // PHP static parameters retire source first. A method receiver instead outlives its arguments.
    let (source_owner, receiver_owner, scope_owner) = if static_form {
        let scope = scope_expr.and_then(|_| prepublish_call_result(ctx, &PhpType::Mixed, expr.span));
        let receiver = prepublish_call_result(ctx, &PhpType::Mixed, expr.span).unwrap();
        let source = prepublish_call_result(ctx, &source_type, expr.span).unwrap();
        (source, receiver, scope)
    } else {
        let source = prepublish_call_result(ctx, &source_type, expr.span).unwrap();
        let scope = scope_expr.and_then(|_| prepublish_call_result(ctx, &PhpType::Mixed, expr.span));
        let receiver = prepublish_call_result(ctx, &PhpType::Mixed, expr.span).unwrap();
        (source, receiver, scope)
    };
    let source = if source_type.codegen_repr() == PhpType::Mixed {
        crate::ir_lower::ownership::copy_assignment_value(ctx, closure, Some(expr.span))
    } else {
        let pinned = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, closure, Some(expr.span));
        if ctx.value_is_owning_temporary(closure) {
            crate::ir_lower::ownership::release_if_owned(ctx, closure, Some(expr.span));
        }
        pinned
    };
    stage_call_result(ctx, Some(&source_owner), source, expr.span);
    let receiver = receiver_expr.map(|arg| lower_expr(ctx, arg)).unwrap_or_else(|| lower_null(ctx, expr));
    let receiver_shape = ctx.builder.value_php_type(receiver.value).codegen_repr();
    let plan = plan.filter(|_| matches!(receiver_shape, PhpType::Object(_)));
    let receiver = snapshot_bind_argument(ctx, receiver, expr.span);
    stage_call_result(ctx, Some(&receiver_owner), receiver, expr.span);
    let scope = scope_expr.map(|arg| {
        let value = lower_expr(ctx, arg);
        let original_type = ctx.builder.value_php_type(value.value).clone();
        let value = snapshot_bind_argument(ctx, value, arg.span);
        stage_call_result(ctx, scope_owner.as_ref(), value, arg.span);
        (borrow_prepublished_operand(ctx, scope_owner.as_ref().unwrap(), arg.span), original_type)
    });
    let (scope_immediate, scope_operand) = classify_closure_bind_scope(ctx, scope_expr, scope);
    let receiver = borrow_prepublished_operand(ctx, &receiver_owner, expr.span);
    let source = borrow_prepublished_operand(ctx, &source_owner, expr.span);
    let mut descriptor_owner = None;
    let mut pending_binding = None;
    let bound = if let Some(plan) = plan {
        let capture = crate::ir_lower::ownership::acquire_if_refcounted(ctx, receiver, Some(expr.span));
        let captures = vec![ClosureCapture { value: capture.value, by_ref_local: None }];
        let data = ctx.intern_string(&plan.name);
        let bound = ctx.emit_value(Op::ClosureNew, vec![capture.value], Some(Immediate::Data(data)),
            PhpType::Callable, Op::ClosureNew.default_effects(), Some(expr.span));
        pending_binding = Some(StaticCallableBinding::Closure {
            name: plan.name, signature: plan.signature, captures, return_alias: plan.return_alias,
            loop_guarded: false, numeric_loop_guarded: false,
        });
        bound
    } else {
        let source = if source_type.codegen_repr() == PhpType::Mixed {
            let descriptor = unbox_callable_param_storage(ctx, source, Some(expr.span));
            let (descriptor, owner) = root_owned_call_operand(ctx, descriptor, expr.span);
            descriptor_owner = owner;
            descriptor
        } else { source };
        emit_closure_bind_with_scope(ctx, source.value, receiver.value, scope_immediate, scope_operand, expr)
    };
    stage_call_result(ctx, result_staging.as_ref(), bound, expr.span);
    if let Some(owner) = descriptor_owner { retire_owned_call_operand(ctx, owner, expr.span); }
    if static_form {
        retire_prepublished_operand(ctx, source_owner, expr.span);
        retire_prepublished_operand(ctx, receiver_owner, expr.span);
        if let Some(scope) = scope_owner { retire_prepublished_operand(ctx, scope, expr.span); }
    } else {
        retire_prepublished_operand(ctx, receiver_owner, expr.span);
        if let Some(scope) = scope_owner { retire_prepublished_operand(ctx, scope, expr.span); }
        retire_prepublished_operand(ctx, source_owner, expr.span);
    }
    let bound = take_prepublished_call_result(ctx, result_staging, bound, expr.span);
    if let Some(binding) = pending_binding { ctx.set_pending_static_callable_result(binding); }
    bound
}

/// Copies mutable boxes and retains raw payloads before a later argument may replace storage.
fn snapshot_bind_argument(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Mixed {
        crate::ir_lower::ownership::copy_assignment_value(ctx, value, Some(span))
    } else {
        ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span))
    }
}

/// Emits a native bind with an optional explicit class or global visibility scope.
pub(super) fn emit_closure_bind_with_scope(
    ctx: &mut LoweringContext<'_, '_>,
    closure: crate::ir::ValueId,
    new_this: crate::ir::ValueId,
    scope: Option<Immediate>,
    scope_operand: Option<ValueId>,
    expr: &Expr,
) -> LoweredValue {
    let mut operands = vec![closure, new_this];
    operands.extend(scope_operand);
    ctx.emit_value(
        Op::ClosureBind,
        operands,
        scope,
        PhpType::Callable,
        Op::ClosureBind.default_effects(),
        Some(expr.span),
    )
}

/// Pins a selected binding operand before later argument evaluation can replace its storage.
pub(super) fn root_closure_bind_operand(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<LocalSlotId>) {
    let value = if ctx.value_is_owning_temporary(value) { value } else {
        crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, value, Some(span))
    };
    root_owned_call_operand(ctx, value, span)
}

/// Classifies the already-evaluated scope without repeating its source expression.
fn classify_closure_bind_scope(
    ctx: &mut LoweringContext<'_, '_>, scope: Option<&Expr>, value: Option<(LoweredValue, PhpType)>,
) -> (Option<Immediate>, Option<ValueId>) {
    let Some(scope) = scope else { return (None, None); };
    let (value, original_type) = value.expect("an explicit scope is evaluated once");
    if matches!(scope.kind, ExprKind::Null) {
        return (Some(Immediate::ClosureBindScope(None)), None);
    }
    if let Some(requested) = static_callable_class_name(ctx, scope) {
        if requested.eq_ignore_ascii_case("static") { return (None, None); }
        if let Some(name) = lookup_folded_name(ctx.classes.keys(), requested.trim_start_matches('\\')) {
            if let Some(id) = ctx.classes.get(&name).filter(|class| !class.is_internal)
                .and_then(|class| u32::try_from(class.class_id).ok()) {
                return (Some(Immediate::ClosureBindScope(Some(id))), None);
            }
        }
    } else {
        match original_type.codegen_repr() {
            PhpType::Void => return (Some(Immediate::ClosureBindScope(None)), None),
            PhpType::Object(name) => {
                let base = ctx.classes.get(&name).and_then(|class| u32::try_from(class.class_id).ok());
                return (Some(Immediate::ClosureBindObjectScope(base)), Some(value.value));
            }
            PhpType::Mixed | PhpType::Union(_) => {
                return (Some(Immediate::ClosureBindObjectScope(None)), Some(value.value));
            }
            _ => {}
        }
    }
    crate::ir_lower::diagnostics::refuse(scope.span,
        "Native closure binding needs a literal user class scope, Class::class, an object, null, or 'static'");
    (None, None)
}
