//! Purpose:
//! Static callable resolution, Closure binding, and builtin signature lookup.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers one resolved static callable target to the corresponding EIR call opcode.
pub(super) fn lower_static_callable_call(
    ctx: &mut LoweringContext<'_, '_>,
    target: StaticCallableBinding,
    callback_args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    match target {
        StaticCallableBinding::UserFunction(function_name) => {
            let sig = ctx.functions.get(&function_name).cloned();
            let operands = lower_args_with_signature(ctx, sig.as_ref(), callback_args);
            let php_type = call_return_type(ctx, &function_name, &operands);
            let data = ctx.intern_function_name(&function_name);
            Some(ctx.emit_value(
                Op::Call,
                operands,
                Some(Immediate::Data(data)),
                php_type,
                effects_lookup::user_call_effects(&function_name),
                Some(expr.span),
            ))
        }
        StaticCallableBinding::ExternFunction(function_name) => {
            let sig = ctx
                .extern_functions
                .get(&function_name)
                .map(function_sig_from_extern_for_descriptor);
            let operands = lower_args_with_signature(ctx, sig.as_ref(), callback_args);
            let php_type = call_return_type(ctx, &function_name, &operands);
            let data = ctx.intern_function_name(&function_name);
            Some(ctx.emit_value(
                Op::ExternCall,
                operands,
                Some(Immediate::Data(data)),
                php_type,
                Op::ExternCall.default_effects(),
                Some(expr.span),
            ))
        }
        StaticCallableBinding::Builtin(function_name) => {
            let sig = call_signature(
                ctx,
                &function_name,
                source_prefers_extension_builtin(&function_name),
            );
            let operands = lower_builtin_call_args(ctx, &function_name, sig.as_ref(), callback_args);
            let php_type = static_callable_builtin_result_type(
                ctx,
                &function_name,
                &operands,
                expr.span,
            );
            Some(emit_builtin_call_value(
                ctx,
                &function_name,
                operands,
                php_type,
                expr.span,
                None,
            ))
        }
        StaticCallableBinding::Closure {
            name,
            signature,
            captures,
        } => {
            let mut operands = lower_args_with_signature(ctx, Some(&signature), callback_args);
            append_closure_capture_operands(&mut operands, &captures);
            let php_type = normalize_value_php_type(signature.return_type.codegen_repr());
            let data = ctx.intern_function_name(&name);
            Some(ctx.emit_value(
                Op::Call,
                operands,
                Some(Immediate::Data(data)),
                php_type,
                effects_lookup::user_call_effects(&name),
                Some(expr.span),
            ))
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
pub(super) fn lower_bound_closure_immediate_call(
    ctx: &mut LoweringContext<'_, '_>,
    callee: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let (bound, _closure_value) = build_bound_closure_binding(ctx, callee, expr)?;
    lower_static_callable_call(ctx, bound, args, expr)
}

/// Builds the static-callable binding for `Closure::bind(fn &() => $this->prop, $newThis, scope)`.
///
/// Lowers the closure literal (once), boxes `$newThis` as the closure's `$this` capture, and
/// overrides the binding's return type with the bound receiver's property type so a
/// by-reference return binds correctly. Returns the binding together with the lowered closure
/// descriptor value (the still-unbound `closure_new`), which callers may store in the assigned
/// variable. `None` unless the call is the single auto-captured `$this` shape — the only form
/// whose `$this` is fully known at compile time. Shared by the immediate-invoke path
/// (`Closure::bind(...)()`) and the variable-assignment path (`$b = Closure::bind(...)`).
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
    if !matches!(closure_lit.kind, ExprKind::Closure { .. }) {
        return None;
    }
    let new_this = bind_args.get(1)?.clone();
    // Lower the closure literal to obtain its static binding (function name + captures).
    let closure_value = lower_expr(ctx, closure_lit);
    let Some(StaticCallableBinding::Closure {
        name,
        mut signature,
        captures,
    }) = ctx.take_pending_static_callable_result()
    else {
        return None;
    };
    // Only the single auto-captured `$this` shape is supported here.
    if captures.len() != 1 {
        return None;
    }
    let new_this_value = lower_expr(ctx, &new_this);
    let boxed_this = ctx.box_value_as_mixed(new_this_value, PhpType::Mixed, Some(expr.span));
    signature.return_type = result_type;
    let bound = StaticCallableBinding::Closure {
        name,
        signature,
        captures: vec![ClosureCapture {
            value: boxed_this.value,
        }],
    };
    Some((bound, closure_value))
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
/// registers `$b` for later direct `$b()` calls, and returns the closure descriptor to store
/// in `$b`. `None` for any non-matching shape so normal assignment lowering applies.
pub(crate) fn lower_bound_closure_for_assignment(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
) -> Option<LoweredValue> {
    let (bound, closure_value) = build_bound_closure_binding(ctx, value, value)?;
    ctx.set_pending_static_callable_result(bound);
    Some(closure_value)
}

/// Resolves the statically-known class name of an object expression used as an instance-call
/// receiver, including declared property and chained-call results.
pub(crate) fn instance_callable_object_class(
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
