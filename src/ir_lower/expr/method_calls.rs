//! Purpose:
//! Core instance-method dispatch and Closure rebinding methods.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    /// Ancestor/method descendant searches already answered for this lowering run.
    ///
    /// The class set is fixed for a whole program, so a repeated statically-missing call site
    /// would otherwise walk every descendant chain again. Cleared by `lower_program`.
    static DESCENDANT_CACHE: RefCell<HashMap<(String, String), bool>> = RefCell::new(HashMap::new());
}

/// Clears the per-program descendant-dispatch cache.
pub(crate) fn clear_descendant_cache() {
    DESCENDANT_CACHE.with(|cache| cache.borrow_mut().clear());
}

/// Returns a cached descendant answer, computing and storing it on a miss.
fn cached_descendant_answer(
    ancestor: &str,
    method: &str,
    compute: impl FnOnce() -> bool,
) -> bool {
    let key = (
        ancestor.trim_start_matches('\\').to_ascii_lowercase(),
        method.to_ascii_lowercase(),
    );
    if let Some(answer) = DESCENDANT_CACHE.with(|cache| cache.borrow().get(&key).copied()) {
        return answer;
    }
    let answer = compute();
    DESCENDANT_CACHE.with(|cache| {
        cache.borrow_mut().insert(key, answer);
    });
    answer
}

/// Synthesizes `__call(string $name, array $arguments)` when only a descendant declares it.
fn synthesized_magic_call_args(
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
    let (class_name, _) = singular_object_class(&object_ty)?;
    if !descendant_provides_call(ctx, class_name.trim_start_matches('\\')) {
        return None;
    }
    // Preserve positional, named and spread argument semantics in the forwarded array.
    let entries = args
        .iter()
        .map(|arg| match &arg.kind {
            ExprKind::NamedArg { name, value } => crate::parser::ast::ArrayEntry::Keyed(
                Expr::new(ExprKind::StringLiteral(name.clone()), arg.span),
                (**value).clone(),
            ),
            ExprKind::Spread(inner) => crate::parser::ast::ArrayEntry::Spread((**inner).clone()),
            _ => crate::parser::ast::ArrayEntry::Value(arg.clone()),
        })
        .collect();
    Some(vec![
        Expr::new(ExprKind::StringLiteral(method.to_string()), span),
        Expr::new(ExprKind::ArrayLiteralMixed(entries), span),
    ])
}

/// Reports whether `type_name` is `ancestor` or reaches it through `extends`/`implements`.
///
/// PHP's class ancestry has two edges: a class's `parent` and the interfaces it implements
/// (directly or through a parent class). Interface `extends` is a third, resolved by
/// [`interface_extends`]. Missing the `implements` edge makes a call through an interface
/// receiver look like an undefined method even when a descendant supplies it.
fn descends_from(ctx: &LoweringContext<'_, '_>, type_name: &str, ancestor: &str) -> bool {
    let ancestor_key = crate::names::php_symbol_key(ancestor.trim_start_matches('\\'));
    let mut current = Some(type_name.trim_start_matches('\\'));
    let mut visited = std::collections::HashSet::new();
    while let Some(class_name) = current {
        if !visited.insert(class_name) {
            return false;
        }
        if crate::names::php_symbol_key(class_name) == ancestor_key {
            return true;
        }
        let Some(info) = ctx.classes.get(class_name) else {
            return false;
        };
        if info
            .interfaces
            .iter()
            .any(|implemented| interface_extends(ctx, implemented, &ancestor_key))
        {
            return true;
        }
        current = info
            .parent
            .as_deref()
            .map(|parent| parent.trim_start_matches('\\'));
    }
    false
}

/// Reports whether `interface_name` is `ancestor_key` or extends it transitively.
fn interface_extends(
    ctx: &LoweringContext<'_, '_>,
    interface_name: &str,
    ancestor_key: &str,
) -> bool {
    let interface_name = interface_name.trim_start_matches('\\');
    if crate::names::php_symbol_key(interface_name) == ancestor_key {
        return true;
    }
    let Some(info) = ctx.interfaces.get(interface_name) else {
        return false;
    };
    info.parents
        .iter()
        .any(|parent| interface_extends(ctx, parent, ancestor_key))
}

/// Reports whether a descendant of `ancestor` declares `__call`.
fn descendant_provides_call(ctx: &LoweringContext<'_, '_>, ancestor: &str) -> bool {
    cached_descendant_answer(ancestor, "__call", || {
        let ancestor = ancestor.trim_start_matches('\\');
        ctx.classes.iter().any(|(name, info)| {
            info.methods.contains_key("__call") && descends_from(ctx, name, ancestor)
        })
    })
}

/// Reports whether a descendant of `ancestor` supplies `method` with public visibility.
///
/// A public descendant method is a real dynamic-dispatch candidate; the backend's mixed
/// receiver path selects the runtime class' override (or raises PHP's abstract-method
/// error). Protected and private descendants are not callable from an outside scope, so they
/// must leave the call to `__call` or the undefined-method path.
fn descendant_provides_public_method(
    ctx: &LoweringContext<'_, '_>,
    ancestor: &str,
    method: &str,
) -> bool {
    cached_descendant_answer(ancestor, &format!("public:{method}"), || {
        let ancestor = ancestor.trim_start_matches('\\');
        let method_key = crate::names::php_symbol_key(method);
        ctx.classes.iter().any(|(name, info)| {
            info.methods.contains_key(&method_key)
                && info
                    .method_visibilities
                    .get(&method_key)
                    .is_none_or(|visibility| {
                        *visibility == crate::parser::ast::Visibility::Public
                    })
                && descends_from(ctx, name, ancestor)
        })
    })
}

/// Returns the catchable `Error` message recorded for a private/protected method call at
/// `expr` that PHP only rejects at runtime, or `None` when the call is accessible.
pub(super) fn recorded_method_access_message(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
    op: Op,
) -> Option<String> {
    if !matches!(op, Op::MethodCall | Op::NullsafeMethodCall) {
        return None;
    }
    ctx.throw_access_site(expr.span).and_then(|info| match &info.kind {
        ThrowAccessKind::PrivateMethod {
            visibility,
            class_name,
            method,
        } => Some(format!(
            "Call to {} method {}::{}() from global scope",
            visibility, class_name, method
        )),
        ThrowAccessKind::UndefinedMethod { class_name, method } => Some(format!(
            "Call to undefined method {}::{}()",
            class_name, method
        )),
        _ => None,
    })
}

/// Returns the declaring descendant and PHP visibility word for a method that is not
/// callable from the current scope, when no public descendant provides it.
///
/// PHP reports `Call to private method Child::hook() from global scope` (or the protected
/// equivalent) instead of dispatching to an inaccessible descendant implementation.
fn descendant_hidden_method(
    ctx: &LoweringContext<'_, '_>,
    ancestor: &str,
    method: &str,
) -> Option<(String, &'static str)> {
    let ancestor = ancestor.trim_start_matches('\\');
    let method_key = crate::names::php_symbol_key(method);
    let mut candidates: Vec<_> = ctx.classes.iter()
        .filter(|(name, info)| {
            info.methods.contains_key(&method_key) && descends_from(ctx, name, ancestor)
        })
        .collect();
    // `ctx.classes` is a `HashMap`, whose iteration order is randomized per process. Sort so
    // two compiles of the same program name the same descendant when several declare the
    // hidden method.
    candidates.sort_by(|left, right| left.0.cmp(right.0));
    candidates.into_iter().find_map(|(name, info)| {
        match info.method_visibilities.get(&method_key) {
            Some(crate::parser::ast::Visibility::Private) => {
                Some((name.trim_start_matches('\\').to_string(), "private"))
            }
            Some(crate::parser::ast::Visibility::Protected)
                if !ctx.current_class.as_deref().is_some_and(|scope| {
                    descends_from(ctx, scope, ancestor) || descends_from(ctx, ancestor, scope)
                }) =>
            {
                Some((name.trim_start_matches('\\').to_string(), "protected"))
            }
            _ => None,
        }
    })
}

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
    let throw_access_message = recorded_method_access_message(ctx, expr, op);
    // A call through a resolved callable value asks for unrecorded scalar checks; the
    // receiver's own calls (`[new P($x), "m"]`) must not consume that request.
    let unrecorded_scalar_guards = std::mem::take(&mut ctx.scalar_guards_unrecorded);
    let object_expr = object;
    let mut object = lower_member_receiver(ctx, object_expr, expr);
    // PHP dispatches an instance method on the RUNTIME class. A method missing from the static
    // receiver but supplied by a closed-world descendant (or a descendant `__call`) must route
    // through dynamic dispatch instead of the recorded undefined-method error: the checker can
    // only judge the declared receiver type, while lowering sees the whole class set.
    let statically_missing = matches!(op, Op::MethodCall | Op::NullsafeMethodCall)
        && method_signature(ctx, object.value, method).is_none();
    let descendant_public_method = statically_missing
        && singular_object_class(&ctx.builder.value_php_type(object.value))
            .is_some_and(|(class_name, _)| {
                descendant_provides_public_method(
                    ctx,
                    class_name.trim_start_matches('\\'),
                    method,
                )
            });
    // A descendant `__call` is the only callable candidate when no public descendant method
    // exists. Synthesizing its argument array here preserves named/spread order; the runtime
    // receiver is boxed so the hook is selected on the actual class.
    let descendant_magic_call = if statically_missing && !descendant_public_method {
        synthesized_magic_call_args(ctx, object.value, method, args, expr.span)
    } else {
        None
    };
    let descendant_hidden_method = if statically_missing
        && !descendant_public_method
        && descendant_magic_call.is_none()
    {
        singular_object_class(&ctx.builder.value_php_type(object.value)).and_then(
            |(class_name, _)| {
                descendant_hidden_method(
                    ctx,
                    class_name.trim_start_matches('\\'),
                    method,
                )
            },
        )
    } else {
        None
    };
    let descendant_dispatch = descendant_public_method || descendant_magic_call.is_some();
    // PHP checks method visibility before it decides whether the call is instance or
    // static, so a recorded private/protected access must throw before the static-only
    // reroute below can run the method.
    if let Some(message) = throw_access_message.as_ref() {
        if !descendant_dispatch {
            release_owning_receiver_temporary(ctx, object, expr.span);
            return crate::ir_lower::stmt::lower_throw_access_error_expr(ctx, message, expr.span);
        }
    }
    // PHP permits `$object->staticMethod()` and resolves the static method from the
    // object's class. The checker accepts that shape alongside the instance table; here
    // the receiver is evaluated for its side effects, then the call is emitted through the
    // static-method path so codegen never passes the object as `$this`.
    if op == Op::MethodCall {
        ctx.scalar_guards_unrecorded = unrecorded_scalar_guards;
        let rerouted = try_static_only_method_through_receiver(ctx, object, method, args, expr);
        ctx.scalar_guards_unrecorded = false;
        if let Some(result) = rerouted {
            return result;
        }
    }
    // These reflection methods initialize the reflected class (deferred default errors and
    // property-default deprecations), like php-src does.
    if op == Op::MethodCall && args.is_empty()
        && matches!(php_symbol_key(method).as_str(),
            "getdefaultproperties" | "getstaticproperties" | "newinstancewithoutconstructor")
        && ctx.classes.keys().any(|class| crate::ir_lower::property_default_errors::has_initialization_effect(ctx, class))
    {
        if is_reflection_class_construction_receiver(ctx, object.value) {
            let owns_receiver = ctx.value_is_owning_temporary(object)
                && !ctx.value_is_owned_unboxed_local_load(object.value);
            if owns_receiver {
                ctx.begin_argument_guard_scope();
                ctx.guard_call_argument(object, 0, expr.span);
                ctx.end_argument_guard_scope();
            }
            // Inspect metadata without consuming an inline reflector needed by the real call.
            let borrowed = ctx.emit_value(Op::Borrow, vec![object.value], None,
                ctx.builder.value_php_type(object.value).clone(), Op::Borrow.default_effects(),
                Some(expr.span));
            let name = lower_property_get_from_value(ctx, borrowed, "__name", Op::PropGet, expr);
            crate::ir_lower::property_default_errors::for_dynamic_class(ctx, name, expr.span);
            if php_symbol_key(method) == "getdefaultproperties" {
                crate::ir_lower::property_default_errors::for_reflection_default_properties(
                    ctx, name, expr.span,
                );
            }
            if owns_receiver { ctx.unguard_call_argument(object.value, expr.span); }
        }
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
            || value_may_be_false(ctx, object.value)
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
        return lower_reflection_class_new_instance_without_constructor(ctx, Some(object_expr), object, args, expr);
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
    if op == Op::MethodCall && reflection_property_reflected_target(ctx, object_expr).is_some() {
        // Specialized property calls still evaluate a reflector receiver. Keep its
        // independent owner through argument/guard throws and retire it on success.
        if ctx.value_is_owned_unboxed_local_load(object.value) {
            let source = object;
            object = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, source, Some(expr.span));
            crate::ir_lower::ownership::release_if_owned(ctx, source, Some(expr.span));
        }
        let pins = pin_in_flight_owners(ctx, &[object.value], expr.span);
        let result = lower_reflection_property_value_call(ctx, Some(object_expr), method, args, expr);
        unpin_in_flight_owners(ctx, pins, expr.span);
        if let Some(value) = result {
            return stabilize_borrowed_result_and_release_receiver(ctx, object, value, expr.span);
        }
    }
    // A declared `callable` keeps its PHP value in a boxed cell. `__invoke` needs its
    // descriptor; the Closure methods apply only when the value is a Closure.
    if ctx.builder.value_php_type(object.value) == PhpType::DeclaredCallable {
        let method_key = php_symbol_key(method);
        if method_key == "__invoke" {
            object = super::declared_callables::normalize_declared_callable(ctx, object, expr.span);
        } else if matches!(method_key.as_str(), "bindto" | "call") {
            return super::declared_callables::lower_declared_callable_closure_method(
                ctx, object, method, args, expr,
            );
        }
    }
    if matches!(ctx.builder.value_php_type(object.value).codegen_repr(), PhpType::Callable) {
        if php_symbol_key(method) == "__invoke" {
            return lower_expr_call_from_value(ctx, object, args, expr);
        }
        if let Some(result) =
            lower_closure_bind_method(ctx, object_expr, &object, method, args, expr)
        {
            return result;
        }
    }
    if let Some(call) = enum_interface_calls::lower_enum_interface_call(ctx, object, method, args, op, expr) {
        return call;
    }
    if let Some(synthesized) = descendant_magic_call {
        // Only a descendant declares `__call`: forward the original name and arguments as
        // PHP does, through the runtime class's magic method.
        let boxed = ctx.box_value_as_mixed(object, PhpType::Mixed, Some(expr.span));
        return super::nullable_method_calls::lower_method_call_with_receiver_direct(
            ctx,
            boxed,
            "__call",
            &synthesized,
            op,
            expr,
        );
    }
    if let Some((class_name, visibility)) = descendant_hidden_method {
        // PHP evaluates the receiver and every argument before reporting the inaccessible
        // method, so release the argument temporaries and the receiver before throwing.
        let result_type = method_call_result_type(ctx, object.value, method, op, expr);
        let arg_values = lower_args_with_signature(ctx, None, args);
        release_owned_call_arg_temporaries(
            ctx,
            &arg_values,
            None,
            &ReturnArgAlias::None,
            expr.span,
        );
        release_owning_receiver_temporary(ctx, object, expr.span);
        let message = format!(
            "Call to {visibility} method {class_name}::{method}() from global scope"
        );
        return crate::ir_lower::stmt::lower_throw_access_error_expr_as(
            ctx,
            &message,
            expr.span,
            result_type,
        );
    }
    if let Some(call) = selected_native_methods::lower(ctx, object, method, args, op, expr) {
        return call;
    }
    if let Some(call) = magic_method_calls::lower(ctx, object, method, args, op, expr) {
        return call;
    }
    let dispatch_method = method;
    let result_type = method_call_result_type(ctx, object.value, dispatch_method, op, expr);
    let sig = method_call_argument_signature(ctx, object_expr, object.value, dispatch_method);
    promote_pdo_binding_ref_argument(ctx, object.value, dispatch_method, args);
    // A by-reference-returning method transfers a lease that must outlive this caller's
    // argument and receiver cleanup, so its staging is published before the arguments.
    let reference_staging = begin_reference_return_call(ctx, sig.as_ref(), expr.span);
    // An ordinary owned result needs the same protection: retiring the argument roots, the
    // evaluation intermediates and an owning receiver all run PHP destructors that can throw,
    // and the result is only an SSA temporary until this staging holds it. Both the result type
    // and the alias summary are resolved from the receiver and the method alone, so they are
    // available here, before any argument expression runs.
    let return_alias = method_return_arg_alias(ctx, object.value, dispatch_method);
    let result_staging = prepublish_user_call_result(
        ctx, sig.as_ref(), &return_alias, &result_type, expr.span,
    );
    // A nullable receiver (`?C`, as `C::parse()` returns) still names `C` in the scalar
    // parameter guard's `TypeError`, as PHP does.
    let receiver_class =
        crate::types::checker::single_object_class_name(&ctx.builder.value_php_type(object.value));
    // A concrete local load may later unbox physical Mixed storage after eval widens
    // its slot. Acquiring a root is safe for both borrowed and independently owned loads.
    let (object, receiver_owner) = root_method_receiver(ctx, object, expr.span);
    emit_native_factory_method_lookup(ctx, object, method, expr.span);
    let mut operands = vec![object.value];
    let reference_property_guard_mark = ctx.reference_property_guards.len();
    begin_call_argument_evaluation(ctx);
    ctx.call_argument_evaluation_scopes
        .last_mut()
        .expect("call argument scope")
        .live_property_reference_arguments = !args.iter().any(is_spread_arg);
    let call_description = receiver_class
        .map(|class_name| super::scalar_param_guards::method_guard_description(ctx, &class_name, dispatch_method, false))
        .unwrap_or_else(|| dispatch_method.to_string());
    let previous_description = ctx.call_description.replace(call_description);
    let previous_call_site = ctx.call_site_span.replace(expr.span);
    let ref_place_rewrite = sig.as_ref().and_then(|signature| {
        super::ref_place_args::rewrite_typed_property_ref_args(ctx, signature, args)
    });
    let call_args = ref_place_rewrite
        .as_ref()
        .map_or(args, |rewrite| rewrite.args.as_slice());
    ctx.scalar_guards_unrecorded = unrecorded_scalar_guards;
    let arg_values = lower_args_with_signature(ctx, sig.as_ref(), call_args);
    ctx.scalar_guards_unrecorded = false;
    ctx.call_description = previous_description;
    ctx.call_site_span = previous_call_site;
    // php-src initializes the reflected class once the arguments are evaluated, whether the
    // static property exists or not.
    if op == Op::MethodCall
        && matches!(php_symbol_key(dispatch_method).as_str(), "getstaticpropertyvalue" | "setstaticpropertyvalue")
    {
        initialize_reflected_class_from_receiver(ctx, object, expr);
    }
    let mut arg_values = arg_values;
    if method_is_fiber_start(ctx, object.value, dispatch_method) {
        for argument in &mut arg_values {
            let omitted = ctx.builder.value_defining_instruction(*argument).is_some_and(|inst| {
                inst.op == Op::ConstNull && inst.span.is_some_and(|span| span.line == 0 && span.col == 0)
            });
            if omitted { continue; }
            // The native start buffer borrows Mixed cells. Publish boxing in EIR
            // so ordinary argument cleanup owns it on return, suspension and throw.
            let value = lowered_value_from_id(ctx, *argument);
            let boxed = coerce_value_for_temp(ctx, value, &PhpType::Mixed, expr.span);
            *argument = root_evaluated_call_argument(ctx, boxed, expr.span).value;
        }
    }
    let evaluation_intermediates = finish_call_argument_evaluation(ctx, &mut arg_values);
    let roots = root_user_call_operands(
        ctx,
        &mut arg_values,
        sig.as_ref(),
        &return_alias,
        &result_type,
        expr.span,
    );
    operands.extend(arg_values.iter().copied());
    super::object_construction::publish_constructor_call_strictness(
        ctx,
        dispatch_method,
        expr.span,
    );
    let data = ctx.intern_string(dispatch_method);
    let call = ctx.emit_value(
        op,
        operands,
        Some(Immediate::Data(data)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    ctx.invalidate_callable_ref_argument_locals(sig.as_ref(), &arg_values);
    let call = finish_reference_return_call(
        ctx, call, sig.as_ref(), reference_staging.as_ref(), expr.span,
    );
    stage_call_result(ctx, result_staging.as_ref(), call, expr.span);
    super::enum_argument_guards::drain_reference_property_guards(
        ctx,
        reference_property_guard_mark,
        expr.span,
    );
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

/// Native factories can create abstract wrappers, whose method error precedes argument effects.
pub(super) fn emit_native_factory_method_lookup(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, method: &str, span: Span,
) {
    if !ctx.classes.contains_key("SimpleXMLElement") { return; }
    let key = php_symbol_key(method);
    let receiver_type = ctx.builder.value_php_type(object.value);
    if let Some((class, _)) = singular_object_class(&receiver_type) {
        if ctx.classes.get(class.trim_start_matches('\\'))
            .is_some_and(|info| info.method_impl_classes.contains_key(&key)) { return; }
    }
    let data = ctx.intern_string(method);
    ctx.emit_void(Op::MethodLookup, vec![object.value], Some(Immediate::Data(data)),
        Op::MethodLookup.default_effects(), Some(span));
}

/// Returns whether a receiver type carries PHP's `false` failure sentinel.
///
/// Inspect the semantic type, not `codegen_repr()`: a union lowers to `Mixed`, so reading the
/// representation here made every `Object|false` factory return look false-free and sent it
/// through the direct dispatcher's "on null" fatal.
fn value_may_be_false(ctx: &LoweringContext<'_, '_>, value: crate::ir::ValueId) -> bool {
    match ctx.builder.value_php_type(value) {
        PhpType::False => true,
        PhpType::Union(members) => members.contains(&PhpType::False),
        _ => false,
    }
}

/// Returns the single object class of a receiver, allowing `false`/`null` failure branches.
fn method_receiver_single_class(ty: &PhpType) -> Option<&str> {
    match ty {
        PhpType::Object(name) => Some(name.as_str()),
        PhpType::Union(members) => {
            let mut found: Option<&str> = None;
            for member in members {
                match member {
                    PhpType::Object(name) => {
                        if found.is_some_and(|existing| existing != name.as_str()) {
                            return None;
                        }
                        found = Some(name.as_str());
                    }
                    PhpType::Void | PhpType::False => {}
                    _ => return None,
                }
            }
            found
        }
        _ => None,
    }
}

/// Returns the receiver's class when `method` exists only in its static method table.
///
/// The checker accepts `$object->staticMethod()` because PHP resolves it statically; this
/// predicate is the lowering-side half of that rule. A class that declares an instance
/// method of the same name keeps ordinary instance dispatch.
pub(super) fn static_only_method_class(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
) -> Option<String> {
    let class_name = match ctx.builder.value_php_type(object).codegen_repr() {
        PhpType::Object(class_name) => class_name.trim_start_matches('\\').to_string(),
        _ => return None,
    };
    let key = php_symbol_key(method);
    let class_info = ctx.classes.get(class_name.as_str())?;
    if class_info.methods.contains_key(&key) {
        return None;
    }
    class_info.static_methods.contains_key(&key).then_some(class_name)
}

/// Builds the static receiver naming `class_name` for a static call via an instance.
pub(super) fn static_receiver_for_class_name(class_name: &str) -> crate::parser::ast::StaticReceiver {
    use crate::parser::ast::StaticReceiver;
    let parts = class_name
        .trim_start_matches('\\')
        .split('\\')
        .map(str::to_string)
        .collect();
    StaticReceiver::Named(crate::names::Name::qualified(parts))
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
    method_signature(ctx, object, method).or_else(|| narrowed_receiver_argument_signature(ctx, object, method))
}

/// Returns the signature that normalizes the arguments of a method the receiver's static class
/// does not declare: the checker accepted the call because it narrowed the receiver
/// (`if ($response instanceof Page) { $response->with($blocks) }`), and codegen dispatches it
/// on the runtime class as for a `mixed` receiver. Its arguments take the signature every
/// declaring class shares, as a `mixed` receiver's do, so an owned argument (`new Conn()`, a
/// retained property read) is released after the call; without one it was handed over raw and
/// never released. `None` for a class that routes the call through `__call`, or when the
/// declaring classes disagree.
fn narrowed_receiver_argument_signature(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
) -> Option<FunctionSig> {
    let object_ty = ctx.builder.value_php_type(object);
    // A factory-style `Object|false` receiver still has one object class after the success
    // branch. Accept it here so the call gets the callee signature and fills trailing
    // defaults; the call remains a dynamic dispatch that throws on the `false` branch.
    let class_name = method_receiver_single_class(&object_ty)?;
    let normalized = class_name.trim_start_matches('\\');
    if normalized.is_empty()
        || ctx.has_eval_barrier()
        || class_method_signature(ctx, normalized, &php_symbol_key("__call")).is_some()
    {
        return None;
    }
    if php_symbol_key(method) == "__construct" {
        class_method_signature(ctx, normalized, &php_symbol_key(method)).cloned()
    } else {
        common_dynamic_method_signature(ctx, &php_symbol_key(method))
    }
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
    // `?Object` and `Object|false` factory returns both need an object-tag check. Truthiness
    // is wrong here: an empty SimpleXMLElement converts to false even though it is an object.
    let is_object = ctx.emit_value(
        Op::TypePredicate,
        vec![object.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)),
        PhpType::Bool,
        Op::TypePredicate.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_object.value,
        then_target: call_block,
        then_args: Vec::new(),
        else_target: fatal_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal_block);
    // `?Object` only reaches null here, while `Object|false` factory returns carry false.
    // PHP names the actual value in the fatal, so split the two messages.
    let null_block = ctx
        .builder
        .create_named_block("method.null.fatal", Vec::new());
    let false_block = ctx
        .builder
        .create_named_block("method.false.fatal", Vec::new());
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
        else_target: false_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    terminate_method_call_on_null(ctx, method);

    ctx.builder.position_at_end(false_block);
    terminate_method_call_on_false(ctx, method);

    ctx.builder.position_at_end(call_block);
    // Re-entering a constructor on an existing `Object|false` view must dispatch against
    // the concrete class so omitted defaults are filled and `__construct` mutates `$this`.
    // Other nullable/interface methods keep their existing Mixed dispatcher.
    let call_object = if php_symbol_key(method) == "__construct" {
        match method_receiver_single_class(&ctx.builder.value_php_type(object.value)) {
            Some(class_name) => {
                let target = PhpType::Object(class_name.to_string());
                ctx.emit_owned_value(
                    Op::MixedUnbox,
                    vec![object.value],
                    None,
                    target.clone(),
                    Op::mixed_unbox_effects(&target),
                    Some(expr.span),
                )
            }
            None => object,
        }
    } else {
        object
    };
    let call = lower_method_call_with_receiver(ctx, call_object, method, args, Op::MethodCall, expr);
    store_value_into_temp(ctx, &temp_name, result_type.clone(), call, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Initializes the class a `ReflectionClass` receiver reflects (deferred default errors and
/// property-default deprecations), reading its runtime name without consuming the receiver.
fn initialize_reflected_class_from_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    expr: &Expr,
) {
    if !is_reflection_class_construction_receiver(ctx, object.value)
        || !ctx.classes.keys().any(|class| crate::ir_lower::property_default_errors::has_initialization_effect(ctx, class))
    {
        return;
    }
    let borrowed = ctx.emit_value(Op::Borrow, vec![object.value], None,
        ctx.builder.value_php_type(object.value).clone(), Op::Borrow.default_effects(),
        Some(expr.span));
    let name = lower_property_get_from_value(ctx, borrowed, "__name", Op::PropGet, expr);
    crate::ir_lower::property_default_errors::for_dynamic_class(ctx, name, expr.span);
}
