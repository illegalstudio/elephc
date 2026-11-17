//! Purpose:
//! ReflectionClass new-instance arguments and class resolution.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Returns the source arguments that can be forwarded to `new $class(...)`.
pub(super) fn reflection_class_new_instance_args(args: &[Expr]) -> Vec<Expr> {
    if has_static_call_spread_args(args) {
        return expand_static_call_spread_args(args);
    }
    args.to_vec()
}

/// Returns constructor arguments carried by a static `newInstanceArgs()` array argument.
pub(super) fn reflection_class_new_instance_args_array(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<Expr>> {
    let args = reflection_class_new_instance_args(args);
    match args.as_slice() {
        [] => Some(Vec::new()),
        [arg] => reflection_class_new_instance_args_value(ctx, arg),
        _ => None,
    }
}

/// Extracts the actual array value passed to the `newInstanceArgs()` `$args` parameter.
pub(super) fn reflection_class_new_instance_args_value(
    ctx: &LoweringContext<'_, '_>,
    arg: &Expr,
) -> Option<Vec<Expr>> {
    let array_expr = match &arg.kind {
        ExprKind::NamedArg { name, value } if php_symbol_key(name) == "args" => value.as_ref(),
        ExprKind::NamedArg { .. } => return None,
        _ => arg,
    };
    if let ExprKind::Variable(name) = &array_expr.kind {
        return ctx.reflection_arg_array_local(name);
    }
    reflection_class_new_instance_args_value_without_locals(array_expr)
}

/// Extracts an inline static array value passed to a reflection argument-array API.
pub(super) fn reflection_class_new_instance_args_value_without_locals(arg: &Expr) -> Option<Vec<Expr>> {
    let array_expr = match &arg.kind {
        ExprKind::NamedArg { name, value } if php_symbol_key(name) == "args" => value.as_ref(),
        ExprKind::NamedArg { .. } => return None,
        _ => arg,
    };
    match &array_expr.kind {
        ExprKind::ArrayLiteral(items) => Some(items.clone()),
        ExprKind::ArrayLiteralAssoc(entries) => reflection_class_new_instance_assoc_args(entries),
        _ => None,
    }
}

/// Converts a static associative argument array into positional and named call arguments.
pub(super) fn reflection_class_new_instance_assoc_args(entries: &[(Expr, Expr)]) -> Option<Vec<Expr>> {
    entries
        .iter()
        .map(|(key, value)| reflection_class_new_instance_assoc_arg(key, value))
        .collect()
}

/// Converts one `newInstanceArgs()` associative-array element into a constructor argument.
pub(super) fn reflection_class_new_instance_assoc_arg(key: &Expr, value: &Expr) -> Option<Expr> {
    match &key.kind {
        ExprKind::IntLiteral(_) | ExprKind::BoolLiteral(_) | ExprKind::FloatLiteral(_) => {
            Some(value.clone())
        }
        ExprKind::StringLiteral(name) if crate::types::is_php_integer_array_key(name) => {
            Some(value.clone())
        }
        ExprKind::StringLiteral(name) => Some(Expr::new(
            ExprKind::NamedArg {
                name: name.clone(),
                value: Box::new(value.clone()),
            },
            value.span,
        )),
        _ => None,
    }
}

/// Returns the reflected constructor signature when the ReflectionClass receiver
/// is an inline `new ReflectionClass(Known::class)` expression.
pub(super) fn reflection_class_new_instance_constructor_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    forwarded_args: &[Expr],
) -> Option<&'a FunctionSig> {
    let class_name = reflection_class_reflected_class(ctx, object_expr?)?;
    if forwarded_args.is_empty() && constructor_signature_for_class_name(ctx, &class_name).is_none()
    {
        return None;
    }
    constructor_signature_for_class_name(ctx, &class_name)
}

/// Resolves the target class from an inline `ReflectionClass` construction when
/// its constructor argument is a literal class string or `ClassName::class`.
pub(super) fn reflection_class_new_instance_reflected_class(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<String> {
    let ExprKind::NewObject { class_name, args } = &object_expr.kind else {
        return None;
    };
    match php_symbol_key(class_name.as_str().trim_start_matches('\\')).as_str() {
        "reflectionclass" => reflection_class_reflected_class_from_args(ctx, args),
        "reflectionobject" => reflection_object_reflected_class_from_args(ctx, args),
        _ => None,
    }
}

/// Resolves the target class from a static `ReflectionClass(...)` argument list.
pub(super) fn reflection_class_reflected_class_from_args(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<String> {
    let reflected_arg = reflection_class_constructor_class_arg(ctx, args)?;
    let raw_class_name = match &reflected_arg.kind {
        ExprKind::StringLiteral(value) => value.clone(),
        ExprKind::ClassConstant { receiver } => static_receiver_class_name(ctx, receiver)?,
        _ => return None,
    };
    resolve_known_class_name(ctx, &raw_class_name)
}

/// Resolves the target class from a static `ReflectionObject(...)` argument list.
pub(super) fn reflection_object_reflected_class_from_args(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<String> {
    let object_arg = reflection_object_constructor_object_arg(ctx, args)?;
    isset_object_expr_class(ctx, &object_arg).map(|(class_name, _)| class_name)
}

/// Resolves a reflected class from an inline constructor or tracked local receiver.
pub(super) fn reflection_class_reflected_class(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<String> {
    reflection_class_new_instance_reflected_class(ctx, object_expr).or_else(|| {
        let ExprKind::Variable(name) = &object_expr.kind else {
            return None;
        };
        ctx.reflection_class_local(name)
    })
}

/// Returns the `ReflectionClass::__construct()` class-name argument after static
/// spread and named-argument normalization.
pub(super) fn reflection_class_constructor_class_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Expr> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return args.first().cloned();
    }
    let sig = ctx
        .classes
        .get("ReflectionClass")
        .and_then(|class_info| class_info.methods.get("__construct"))?;
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        &args,
        call_span,
        crate::types::call_args::regular_param_count(sig),
        false,
        true,
        &assoc_spread_sources(ctx, &args),
    )
    .ok()?;
    if plan.has_spread_args() {
        return None;
    }
    planned_regular_arg_expr(plan.regular_args.first()?).cloned()
}

/// Returns the `ReflectionObject::__construct()` object argument after normalization.
pub(super) fn reflection_object_constructor_object_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Expr> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return args.first().cloned();
    }
    let sig = ctx
        .classes
        .get("ReflectionObject")
        .and_then(|class_info| class_info.methods.get("__construct"))?;
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        &args,
        call_span,
        crate::types::call_args::regular_param_count(sig),
        false,
        true,
        &assoc_spread_sources(ctx, &args),
    )
    .ok()?;
    if plan.has_spread_args() {
        return None;
    }
    planned_regular_arg_expr(plan.regular_args.first()?).cloned()
}

/// Resolves a PHP class name case-insensitively against known class metadata.
pub(super) fn resolve_known_class_name(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Option<String> {
    let key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.classes
        .keys()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == key)
        .cloned()
}

/// Resolves a PHP function name case-insensitively against known user functions.
pub(super) fn resolve_known_function_name(
    ctx: &LoweringContext<'_, '_>,
    function_name: &str,
) -> Option<String> {
    let key = php_symbol_key(function_name.trim_start_matches('\\'));
    ctx.functions
        .keys()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == key)
        .cloned()
}

/// Resolves a PHP method name case-insensitively against known class metadata.
pub(super) fn resolve_known_class_method_name(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
) -> Option<String> {
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    let key = php_symbol_key(method);
    class_info
        .methods
        .keys()
        .chain(class_info.static_methods.keys())
        .find(|candidate| php_symbol_key(candidate) == key)
        .cloned()
}

/// Returns constructor signature metadata for a known class name.
///
/// Resolved through [`crate::types::constructor_owner`] for the same reason
/// `object_construction::constructor_signature` is: a descendant of a class with a private
/// constructor carries no `__construct` entry of its own, and reading `methods` directly made
/// `ReflectionClass::newInstance()` on it see no constructor at all (issue #868).
pub(super) fn constructor_signature_for_class_name<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    class_name: &str,
) -> Option<&'a FunctionSig> {
    let key = php_symbol_key("__construct");
    crate::types::constructor_owner(ctx.classes, class_name.trim_start_matches('\\'))
        .and_then(|(_, class_info)| class_info.methods.get(&key))
}

/// Emits a runtime fatal for ReflectionClass newInstance argument forms not yet lowered.
pub(super) fn lower_reflection_class_new_instance_unsupported(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> LoweredValue {
    let result = lower_boxed_null(ctx, expr);
    let message = ctx.intern_string(
        "Fatal error: unsupported ReflectionClass::newInstance() argument forwarding\n",
    );
    ctx.builder.terminate(Terminator::Fatal { message });
    result
}

/// Emits a runtime fatal for unsupported `newInstanceArgs()` argument-array forms.
pub(super) fn lower_reflection_class_new_instance_args_unsupported(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> LoweredValue {
    let result = lower_boxed_null(ctx, expr);
    let message = ctx.intern_string(
        "Fatal error: unsupported ReflectionClass::newInstanceArgs() argument array\n",
    );
    ctx.builder.terminate(Terminator::Fatal { message });
    result
}

/// Emits a runtime fatal for unsupported `newInstanceWithoutConstructor()` argument forms.
pub(super) fn lower_reflection_class_new_instance_without_constructor_unsupported(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> LoweredValue {
    let result = lower_boxed_null(ctx, expr);
    let message = ctx.intern_string(
        "Fatal error: unsupported ReflectionClass::newInstanceWithoutConstructor() arguments\n",
    );
    ctx.builder.terminate(Terminator::Fatal { message });
    result
}

/// Returns true when a method call targets the built-in `ReflectionClass::newInstance()`.
pub(super) fn is_reflection_class_new_instance_call(
    ctx: &LoweringContext<'_, '_>,
    object: ValueId,
    method: &str,
) -> bool {
    if php_symbol_key(method) != "newinstance" {
        return false;
    }
    is_reflection_class_construction_receiver(ctx, object)
}

/// Returns true when a method call targets `ReflectionClass::newInstanceArgs()`.
pub(super) fn is_reflection_class_new_instance_args_call(
    ctx: &LoweringContext<'_, '_>,
    object: ValueId,
    method: &str,
) -> bool {
    if php_symbol_key(method) != "newinstanceargs" {
        return false;
    }
    is_reflection_class_construction_receiver(ctx, object)
}

/// Returns true when a method call targets `ReflectionClass::newInstanceWithoutConstructor()`.
pub(super) fn is_reflection_class_new_instance_without_constructor_call(
    ctx: &LoweringContext<'_, '_>,
    object: ValueId,
    method: &str,
) -> bool {
    if php_symbol_key(method) != "newinstancewithoutconstructor" {
        return false;
    }
    is_reflection_class_construction_receiver(ctx, object)
}

/// Returns true when a receiver can use ReflectionClass construction helper lowering.
pub(super) fn is_reflection_class_construction_receiver(
    ctx: &LoweringContext<'_, '_>,
    object: ValueId,
) -> bool {
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, false)) = singular_object_class(&object_ty) else {
        return false;
    };
    matches!(
        php_symbol_key(class_name.trim_start_matches('\\')).as_str(),
        "reflectionclass" | "reflectionobject"
    )
}

