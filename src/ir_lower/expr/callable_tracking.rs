//! Purpose:
//! Static callable and reflection binding tracking plus callable-array assignment.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Resolves a literal first-class callable expression to a static local binding.
pub(crate) fn static_callable_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<StaticCallableBinding> {
    match &expr.kind {
        ExprKind::StringLiteral(name) => resolve_static_string_callable(ctx, name),
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            resolve_static_string_callable(ctx, name.as_str())
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            resolve_static_method_callable(ctx, receiver.clone(), method.clone())
        }
        ExprKind::ArrayLiteral(items) => static_array_callable_descriptor_target(ctx, items)
            .or_else(|| instance_array_callable_target(ctx, items)),
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => {
            resolve_instance_method_callable(ctx, object, method.clone(), false)
        }
        ExprKind::Variable(name) => ctx.static_callable_local(name),
        _ => None,
    }
}

/// Returns the reflected class captured by a statically-known `ReflectionClass` expression.
pub(crate) fn reflection_class_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<String> {
    reflection_class_new_instance_reflected_class(ctx, expr)
}

/// Returns the reflected function captured by a statically-known `ReflectionFunction` expression.
pub(crate) fn reflection_function_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<String> {
    reflection_function_reflected_target(ctx, expr)
}

/// Returns the reflected property captured by a statically-known `ReflectionProperty` expression.
pub(crate) fn reflection_property_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<(String, String)> {
    reflection_property_reflected_target(ctx, expr)
}

/// Returns the reflected method captured by a statically-known `ReflectionMethod` expression.
pub(crate) fn reflection_method_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<(String, String)> {
    reflection_method_reflected_target(ctx, expr)
}

/// Returns a safe static argument array that can be replayed for reflection forwarding.
pub(crate) fn reflection_arg_array_binding_for_expr(expr: &Expr) -> Option<Vec<Expr>> {
    let args = reflection_class_new_instance_args_value_without_locals(expr)?;
    if args.iter().all(reflection_arg_expr_can_track) {
        Some(args)
    } else {
        None
    }
}

/// Returns true when replaying an argument expression cannot duplicate side effects.
pub(super) fn reflection_arg_expr_can_track(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => true,
        ExprKind::DynamicScopedConstantAccess { receiver, .. } => {
            reflection_arg_expr_can_track(receiver)
        }
        ExprKind::Negate(inner) => matches!(
            &inner.kind,
            ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
        ),
        ExprKind::NamedArg { value, .. } => reflection_arg_expr_can_track(value),
        ExprKind::ArrayLiteral(items) => items.iter().all(reflection_arg_expr_can_track),
        ExprKind::ArrayLiteralAssoc(entries) => entries.iter().all(|(key, value)| {
            reflection_arg_array_key_can_track(key) && reflection_arg_expr_can_track(value)
        }),
        _ => false,
    }
}

/// Returns true when an associative array key is stable enough for replay.
pub(super) fn reflection_arg_array_key_can_track(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::FloatLiteral(_)
    )
}

/// EIR value and callable binding produced by a callable-array assignment.
pub(crate) struct LoweredCallableArrayAssignment {
    pub(crate) value: LoweredValue,
    pub(crate) target: StaticCallableBinding,
}

/// Lowers a callable-array assignment while preserving its PHP array value.
pub(crate) fn lower_callable_array_for_assignment(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    target: Option<&StaticCallableBinding>,
) -> Option<LoweredCallableArrayAssignment> {
    let ExprKind::ArrayLiteral(items) = &value.kind else {
        return None;
    };
    let StaticCallableBinding::InstanceMethod {
        object,
        method,
        signature,
        ..
    } = target? else {
        return None;
    };
    let receiver = lower_expr(ctx, object);
    let receiver_ty = ctx.builder.value_php_type(receiver.value);
    let hidden_name = ctx.declare_hidden_temp(receiver_ty.clone());
    store_value_into_temp(
        ctx,
        &hidden_name,
        receiver_ty,
        receiver,
        object.span,
    );
    let receiver = ctx.load_local(&hidden_name, Some(object.span));
    let array = lower_callable_array_literal_with_receiver(ctx, items, value, receiver);
    let hidden_object = Expr::new(ExprKind::Variable(hidden_name), object.span);
    let target = StaticCallableBinding::InstanceMethod {
        object: Box::new(hidden_object),
        method: method.clone(),
        signature: signature.clone(),
        direct_call: true,
    };
    Some(LoweredCallableArrayAssignment { value: array, target })
}

/// Lowers a callable-array literal after its receiver has already been captured.
pub(super) fn lower_callable_array_literal_with_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    items: &[Expr],
    expr: &Expr,
    receiver: LoweredValue,
) -> LoweredValue {
    let array_ty = array_literal_type_for_ir(ctx, items, expr);
    let elem_ty = indexed_array_literal_element_type(&array_ty);
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        array_ty,
        Op::ArrayNew.default_effects(),
        Some(expr.span),
    );
    ctx.emit_void(
        Op::ArrayPush,
        vec![array.value, receiver.value],
        None,
        Op::ArrayPush.default_effects(),
        Some(expr.span),
    );
    crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), receiver, expr.span);
    for item in items.iter().skip(1) {
        let value = lower_expr(ctx, item);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value, item.span);
    }
    array
}

/// Resolves a static callable array literal as a descriptor-backed static method.
pub(super) fn static_array_callable_descriptor_target(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
) -> Option<StaticCallableBinding> {
    static_array_callable_parts(ctx, items).map(|(receiver, method)| {
        StaticCallableBinding::StaticMethodDescriptor { receiver, method }
    })
}

/// Resolves a literal `[$object, "method"]` callable array as an instance method.
pub(super) fn instance_array_callable_target(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
) -> Option<StaticCallableBinding> {
    let [object, method_expr] = items else {
        return None;
    };
    let ExprKind::StringLiteral(method) = &method_expr.kind else {
        return None;
    };
    // A method the current scope cannot reach is not a static binding: php resolves the array
    // against the CALLING scope, and an unreachable method is either routed to `__call()` or
    // rejected. Binding it here called a protected method from outside its class.
    if inaccessible_instance_callable(ctx, object, method).is_some() {
        return None;
    }
    resolve_instance_method_callable(ctx, object, method.clone(), true)
}

/// A statically known `[$object, 'method']` whose method the current scope cannot call.
pub(super) struct InaccessibleInstanceCallable {
    /// The receiver's class as the diagnostic names it.
    pub(super) class_name: String,
    /// `protected` or `private`.
    pub(super) visibility: &'static str,
    /// The method as declared, case included.
    pub(super) method: String,
    /// Whether the class has a `__call()` that php falls back to.
    pub(super) has_magic_call: bool,
}

/// Classifies `[object, 'method']` as unreachable from the class being lowered, when the
/// receiver's class and the method are known at compile time.
///
/// `protected` is reachable from the declaring class and every class related to it, `private`
/// only from the declaring class itself: the rule php applies to a callable array resolved in
/// the calling scope. A method the receiver's class does not hold under that name (an inherited
/// private one, resolved through the lexical scope) is not reported.
pub(super) fn inaccessible_instance_callable(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    method: &str,
) -> Option<InaccessibleInstanceCallable> {
    let class_name = instance_callable_object_class(ctx, object)?;
    let class_name = class_name.trim_start_matches('\\');
    let info = ctx.classes.get(class_name)?;
    let method_key = php_symbol_key(method);
    let visibility = info.method_visibilities.get(&method_key)?;
    let declaring = info
        .method_declaring_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    let current = ctx.current_class.as_deref();
    let reachable = match visibility {
        Visibility::Public => true,
        Visibility::Private => {
            current.is_some_and(|scope| php_symbol_key(scope) == php_symbol_key(declaring))
        }
        Visibility::Protected => current.is_some_and(|scope| {
            class_extends_class(ctx, scope, declaring) || class_extends_class(ctx, declaring, scope)
        }),
    };
    if reachable {
        return None;
    }
    let mut declared = method.to_string();
    let mut owner = Some(info);
    while let Some(candidate) = owner {
        if let Some(decl) = candidate
            .method_decls
            .iter()
            .find(|decl| php_symbol_key(&decl.name) == method_key)
        {
            declared = decl.name.clone();
            break;
        }
        owner = candidate
            .parent
            .as_deref()
            .and_then(|parent| ctx.classes.get(parent.trim_start_matches('\\')));
    }
    Some(InaccessibleInstanceCallable {
        class_name: class_name.to_string(),
        visibility: if matches!(visibility, Visibility::Private) {
            "private"
        } else {
            "protected"
        },
        method: declared,
        has_magic_call: info.methods.contains_key("__call"),
    })
}

/// Resolves the named static receiver and method from a static callable array literal.
pub(super) fn static_array_callable_parts(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
) -> Option<(StaticReceiver, String)> {
    let [class_expr, method_expr] = items else {
        return None;
    };
    let class_name = static_callable_class_name(ctx, class_expr)?;
    let ExprKind::StringLiteral(method) = &method_expr.kind else {
        return None;
    };
    let class_name = lookup_folded_name(ctx.classes.keys(), class_name.trim_start_matches('\\'))?;
    let receiver = StaticReceiver::Named(Name::from(class_name));
    static_method_implementation_signature(ctx, &receiver, method)?;
    Some((receiver, method.clone()))
}

/// Extracts a compile-time class name for a static callable array.
pub(super) fn static_callable_class_name(
    ctx: &LoweringContext<'_, '_>,
    class_expr: &Expr,
) -> Option<String> {
    match &class_expr.kind {
        ExprKind::StringLiteral(name) => Some(name.clone()),
        ExprKind::ClassConstant { receiver } => static_receiver_class_name(ctx, receiver),
        _ => None,
    }
}

/// Builtins that only INVOKE their callback before returning, with the callback's position.
///
/// `usize::MAX` names the last positional argument (`array_udiff(..., $callback)`). A builtin
/// that stores its callback (`spl_autoload_register`, `ob_start`, `set_error_handler`...) is
/// absent on purpose: the stored value is observable again, and it must stay the array.
const INVOKE_ONLY_CALLBACK_BUILTINS: &[(&str, usize)] = &[
    ("array_all", 1),
    ("array_any", 1),
    ("array_filter", 1),
    ("array_find", 1),
    ("array_find_key", 1),
    ("array_diff_uassoc", usize::MAX),
    ("array_diff_ukey", usize::MAX),
    ("array_intersect_uassoc", usize::MAX),
    ("array_intersect_ukey", usize::MAX),
    ("array_map", 0),
    ("array_reduce", 1),
    ("array_udiff", usize::MAX),
    ("array_udiff_assoc", usize::MAX),
    ("array_uintersect", usize::MAX),
    ("array_uintersect_assoc", usize::MAX),
    ("array_walk", 1),
    ("array_walk_recursive", 1),
    ("iterator_apply", 1),
    ("preg_replace_callback", 1),
    ("uasort", 1),
    ("uksort", 1),
    ("usort", 1),
];

/// Rewrites a literal `[Class::class, 'method']` callback of an invoke-only builtin into the
/// equivalent `Class::method(...)`, when the class and static method are known at compile time
/// and the method is visible from the calling class.
///
/// php resolves a callable array against the CALLING scope, so `[__CLASS__, 'helper']` reaches a
/// private static `helper()` from inside its class. The array form reached the runtime callable
/// table instead, which only lists public methods and knows no caller scope, so every such call
/// failed ("callable array did not resolve to an invokable target"), and the sort builtins
/// refused the array at compile time even for a public method. A first-class callable is bound
/// where it is written and already carries that scope. The builtin never hands the callback back,
/// so nothing can observe that it was not an array.
///
/// `static::class` keeps its array form: its class is the runtime called class.
pub(super) fn invoke_only_callback_args_with_static_method_callables(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
) -> Option<Vec<Expr>> {
    if ctx.functions.contains_key(name) {
        return None;
    }
    let key = php_symbol_key(name.trim_start_matches('\\'));
    let &(_, position) = INVOKE_ONLY_CALLBACK_BUILTINS
        .iter()
        .find(|(builtin, _)| *builtin == key)?;
    if args.is_empty()
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let index = if position == usize::MAX {
        args.len() - 1
    } else {
        position
    };
    let callback = args.get(index)?;
    let rewritten = visible_static_method_callable_for_array(ctx, callback)?;
    let mut args = args.to_vec();
    args[index] = rewritten;
    Some(args)
}

/// Replaces a call of an invoke-only builtin by php's `TypeError` when its literal
/// `[$object, 'method']` callback names an instance method the current scope cannot reach.
///
/// php validates the callback before running the builtin: `usort($xs, [$box, 'hidden'])` from
/// outside `Box` throws `usort(): Argument #2 ($callback) must be a valid callback, cannot access
/// protected method Box::hidden()`. The callback is known statically here, so the whole call
/// becomes that throw. A class with `__call()` is left alone: php sends the call there.
/// The arguments are not evaluated, which php does before validating.
pub(super) fn inaccessible_callback_rejection(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Option<Expr> {
    // Several of these builtins are implemented as prelude functions (`array_find_key()`,
    // `array_filter()` with a callback, ...), so a declared function of the name is no reason to
    // skip: php 8.5 owns every name in the table, and the diagnostic is php's.
    let key = php_symbol_key(name.trim_start_matches('\\'));
    let &(_, position) = INVOKE_ONLY_CALLBACK_BUILTINS
        .iter()
        .find(|(builtin, _)| *builtin == key)?;
    if args.is_empty()
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let index = if position == usize::MAX {
        args.len() - 1
    } else {
        position
    };
    let ExprKind::ArrayLiteral(items) = &args.get(index)?.kind else {
        return None;
    };
    let [object, method_expr] = items.as_slice() else {
        return None;
    };
    let ExprKind::StringLiteral(method) = &method_expr.kind else {
        return None;
    };
    let inaccessible = inaccessible_instance_callable(ctx, object, method)?;
    if inaccessible.has_magic_call {
        return None;
    }
    // php's own spelling: most callbacks are `($callback)`, the `array_u*` comparators have no
    // parameter name, and only `array_map()` / `array_filter()` accept null.
    let label = if is_unnamed_callback_parameter(&key) { "" } else { " ($callback)" };
    let or_null = if matches!(key.as_str(), "array_map" | "array_filter") {
        " or null"
    } else {
        ""
    };
    let message = format!(
        "{key}(): Argument #{}{label} must be a valid callback{or_null}, cannot access {} method {}::{}()",
        index + 1,
        inaccessible.visibility,
        inaccessible.class_name,
        inaccessible.method,
    );
    let error = crate::synthetic_class::e_new_fq(
        "TypeError",
        vec![crate::synthetic_class::e_str(&message)],
    );
    Some(Expr::new(ExprKind::Throw(Box::new(error)), span))
}

/// Whether php words this builtin's callback argument without a parameter name.
///
/// The comparator families take a trailing variadic callback, so php prints `Argument #3 must be
/// a valid callback` and omits `($callback)`.
fn is_unnamed_callback_parameter(key: &str) -> bool {
    key.starts_with("array_u")
        || matches!(
            key,
            "array_diff_uassoc" | "array_diff_ukey" | "array_intersect_uassoc" | "array_intersect_ukey"
        )
}

/// Returns `Class::method(...)` for a literal `[<class>, 'method']` naming a static method the
/// current class may call.
fn visible_static_method_callable_for_array(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<Expr> {
    let ExprKind::ArrayLiteral(items) = &callback.kind else {
        return None;
    };
    let [class_expr, method_expr] = items.as_slice() else {
        return None;
    };
    let ExprKind::StringLiteral(method) = &method_expr.kind else {
        return None;
    };
    let class_name = match &class_expr.kind {
        ExprKind::StringLiteral(name) => name.clone(),
        ExprKind::ClassConstant { receiver } if !matches!(receiver, StaticReceiver::Static) => {
            static_receiver_class_name(ctx, receiver)?
        }
        _ => return None,
    };
    let class_name = lookup_folded_name(ctx.classes.keys(), class_name.trim_start_matches('\\'))?;
    let class_info = ctx.classes.get(&class_name)?;
    let method_key = php_symbol_key(method);
    let receiver = StaticReceiver::Named(Name::from(class_name.clone()));
    static_method_implementation_signature(ctx, &receiver, method)?;
    let declaring_class = class_info
        .static_method_declaring_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| class_name.clone());
    let visible = match class_info.static_method_visibilities.get(&method_key)? {
        Visibility::Public => true,
        Visibility::Private => ctx
            .current_class
            .as_deref()
            .is_some_and(|current| php_symbol_key(current) == php_symbol_key(&declaring_class)),
        Visibility::Protected => ctx.current_class.as_deref().is_some_and(|current| {
            class_extends_class(ctx, current, &declaring_class)
                || class_extends_class(ctx, &declaring_class, current)
        }),
    };
    if !visible {
        return None;
    }
    Some(Expr::new(
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
            receiver,
            method: method.clone(),
        }),
        callback.span,
    ))
}

/// Returns the static `is_callable()` result for a literal static-method callback array.
pub(super) fn static_array_callable_is_callable(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
) -> Option<bool> {
    let [class_expr, method_expr] = items else {
        return None;
    };
    let class_name = static_callable_class_name(ctx, class_expr)?;
    let ExprKind::StringLiteral(method) = &method_expr.kind else {
        return None;
    };
    Some(static_method_callback_is_callable(ctx, &class_name, method))
}

/// Returns true when a compile-time class/method pair names a public static method.
pub(super) fn static_method_callback_is_callable(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
) -> bool {
    let Some(class_name) = lookup_folded_name(ctx.classes.keys(), class_name.trim_start_matches('\\')) else {
        return false;
    };
    let Some(class_info) = ctx.classes.get(&class_name) else {
        return false;
    };
    let method_key = php_symbol_key(method);
    if !class_info.static_methods.contains_key(&method_key) {
        return false;
    }
    class_info.static_method_visibilities.get(&method_key) == Some(&Visibility::Public)
}

/// Converts a static `call_user_func_array()` argument array into call arguments.
pub(super) fn static_call_user_func_array_args(arg_array: &Expr) -> Option<Vec<Expr>> {
    match &arg_array.kind {
        ExprKind::ArrayLiteral(items) => Some(items.clone()),
        ExprKind::ArrayLiteralAssoc(pairs) => static_call_user_func_array_assoc_args(pairs),
        _ => None,
    }
}

/// Converts literal associative callback arrays into positional or named call args.
pub(super) fn static_call_user_func_array_assoc_args(pairs: &[(Expr, Expr)]) -> Option<Vec<Expr>> {
    let mut args = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        match &key.kind {
            ExprKind::StringLiteral(name) => {
                args.push(Expr::new(
                    ExprKind::NamedArg {
                        name: name.clone(),
                        value: Box::new(value.clone()),
                    },
                    value.span,
                ));
            }
            ExprKind::IntLiteral(_) => args.push(value.clone()),
            _ => return None,
        }
    }
    Some(args)
}
