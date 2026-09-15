//! Purpose:
//! Reflection constructor target normalization.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Extracts the known function name from an inline ReflectionFunction constructor.
pub(super) fn reflection_function_constructor_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<String> {
    let ExprKind::NewObject { class_name, args } = &object_expr.kind else {
        return None;
    };
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) != "reflectionfunction" {
        return None;
    }
    let function_arg = reflection_function_constructor_regular_arg(ctx, args)?;
    let ExprKind::StringLiteral(function_name) = function_arg.kind else {
        return None;
    };
    resolve_known_reflection_function_name(ctx, &function_name)
}

/// Resolves function names accepted by static `ReflectionFunction` metadata.
pub(super) fn resolve_known_reflection_function_name(
    ctx: &LoweringContext<'_, '_>,
    function_name: &str,
) -> Option<String> {
    resolve_known_function_name(ctx, function_name)
        .or_else(|| resolve_known_reflection_builtin_name(function_name))
}

/// Resolves a supported callable builtin name for `ReflectionFunction`.
pub(super) fn resolve_known_reflection_builtin_name(function_name: &str) -> Option<String> {
    let canonical = canonical_builtin_function_name(function_name.trim_start_matches('\\'))?;
    first_class_builtin_signature(&canonical).map(|_| canonical)
}

/// Extracts the known class and property name from an inline ReflectionProperty constructor.
pub(super) fn reflection_property_constructor_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::NewObject { class_name, args } = &object_expr.kind else {
        return None;
    };
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) != "reflectionproperty" {
        return None;
    }
    let (class_arg, property_arg) = reflection_property_constructor_regular_args(ctx, args)?;
    let raw_class_name = match &class_arg.kind {
        ExprKind::StringLiteral(value) => value.clone(),
        ExprKind::ClassConstant { receiver } => static_receiver_class_name(ctx, receiver)?,
        _ => return None,
    };
    let class_name = resolve_known_class_name(ctx, &raw_class_name)?;
    let ExprKind::StringLiteral(property) = property_arg.kind else {
        return None;
    };
    Some((class_name, property))
}

/// Extracts the known class and method name from an inline ReflectionMethod constructor.
pub(super) fn reflection_method_constructor_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::NewObject { class_name, args } = &object_expr.kind else {
        return None;
    };
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) != "reflectionmethod" {
        return None;
    }
    let (class_arg, method_arg) = reflection_method_constructor_regular_args(ctx, args)?;
    let raw_class_name = match &class_arg.kind {
        ExprKind::StringLiteral(value) => value.clone(),
        ExprKind::ClassConstant { receiver } => static_receiver_class_name(ctx, receiver)?,
        _ => return None,
    };
    let class_name = resolve_known_class_name(ctx, &raw_class_name)?;
    let ExprKind::StringLiteral(method) = method_arg.kind else {
        return None;
    };
    let method = resolve_known_class_method_name(ctx, &class_name, &method)?;
    Some((class_name, method))
}

/// Extracts the constructor target from inline `ReflectionClass::getConstructor()` calls.
pub(super) fn reflection_method_class_get_constructor_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::MethodCall {
        object,
        method,
        args,
    } = &object_expr.kind
    else {
        return None;
    };
    if php_symbol_key(method) != "getconstructor" {
        return None;
    }
    if !reflection_class_new_instance_args(args).is_empty() {
        return None;
    }
    let reflected_class = reflection_class_reflected_class(ctx, object)?;
    // PHP's `getConstructor()` reports the DECLARING class, which for a descendant of a class
    // with a private constructor is the ancestor: `ReflectionClass('Child')->getConstructor()`
    // is a `ReflectionMethod` on `Owner`, even though `method_exists('Child', '__construct')`
    // is `false`. A private method is not inherited, so the descendant's own `methods` map has
    // no entry and resolving only there returned `None` — `getConstructor()` answered `null`
    // where PHP answers a method (issue #869). Walking to the owner models both halves, and
    // leaves an inherited PUBLIC constructor exactly where it already resolved.
    let (owner_name, _) = crate::types::constructor_owner(
        ctx.classes,
        reflected_class.trim_start_matches('\\'),
    )?;
    let owner_name = owner_name.to_string();
    let method = resolve_known_class_method_name(ctx, &owner_name, "__construct")?;
    Some((owner_name, method))
}

/// Extracts the property target from inline `ReflectionClass::getProperty()` calls.
pub(super) fn reflection_property_class_get_property_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::MethodCall {
        object,
        method,
        args,
    } = &object_expr.kind
    else {
        return None;
    };
    if php_symbol_key(method) != "getproperty" {
        return None;
    }
    let class_name = reflection_class_reflected_class(ctx, object)?;
    let property = reflection_class_member_name_arg(args)?;
    Some((class_name, property))
}

/// Extracts the method target from inline `ReflectionClass::getMethod()` calls.
pub(super) fn reflection_method_class_get_method_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::MethodCall {
        object,
        method,
        args,
    } = &object_expr.kind
    else {
        return None;
    };
    if php_symbol_key(method) != "getmethod" {
        return None;
    }
    let class_name = reflection_class_reflected_class(ctx, object)?;
    let method = reflection_class_member_name_arg(args)?;
    let method = resolve_known_class_method_name(ctx, &class_name, &method)?;
    Some((class_name, method))
}

/// Returns the literal name argument passed to a ReflectionClass member lookup.
pub(super) fn reflection_class_member_name_arg(args: &[Expr]) -> Option<String> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let (name, _) = reflection_class_static_property_regular_args(&args, "name", None)?;
    reflection_class_static_property_name_arg(name.as_ref()?)
}

/// Returns normalized constructor args for `ReflectionFunction($function)`.
pub(super) fn reflection_function_constructor_regular_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Expr> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [function_arg] => Some(function_arg.clone()),
            _ => None,
        };
    }
    let sig = ctx
        .classes
        .get("ReflectionFunction")
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

/// Returns normalized constructor args for `ReflectionProperty($class, $property)`.
pub(super) fn reflection_property_constructor_regular_args(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<(Expr, Expr)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [class_arg, property_arg] => Some((class_arg.clone(), property_arg.clone())),
            _ => None,
        };
    }
    let sig = ctx
        .classes
        .get("ReflectionProperty")
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
    let class_arg = planned_regular_arg_expr(plan.regular_args.first()?)?.clone();
    let property_arg = planned_regular_arg_expr(plan.regular_args.get(1)?)?.clone();
    Some((class_arg, property_arg))
}

/// Returns normalized constructor args for `ReflectionMethod($class, $method)`.
pub(super) fn reflection_method_constructor_regular_args(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<(Expr, Expr)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if args.len() == 1 {
        return reflection_method_constructor_single_target(ctx, &args[0]);
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [class_arg, method_arg] => Some((class_arg.clone(), method_arg.clone())),
            _ => None,
        };
    }
    let sig = ctx
        .classes
        .get("ReflectionMethod")
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
    let class_arg = planned_regular_arg_expr(plan.regular_args.first()?)?.clone();
    let method_arg = planned_regular_arg_expr(plan.regular_args.get(1)?)?.clone();
    Some((class_arg, method_arg))
}

/// Splits deprecated `ReflectionMethod("Class::method")` constructor syntax.
pub(super) fn reflection_method_constructor_single_target(
    ctx: &LoweringContext<'_, '_>,
    arg: &Expr,
) -> Option<(Expr, Expr)> {
    let arg = match &arg.kind {
        ExprKind::NamedArg { name, value } if name == "class_name" => value.as_ref(),
        ExprKind::NamedArg { name, value } if name == "objectOrMethod" => value.as_ref(),
        ExprKind::NamedArg { .. } => return None,
        _ => arg,
    };
    let ExprKind::StringLiteral(target) = &arg.kind else {
        return None;
    };
    let (raw_class_name, raw_method_name) = target.rsplit_once("::")?;
    if raw_class_name.is_empty() || raw_method_name.is_empty() {
        return None;
    }
    let class_name = resolve_known_class_name(ctx, raw_class_name)?;
    let method_name = resolve_known_class_method_name(ctx, &class_name, raw_method_name)?;
    Some((
        Expr::new(ExprKind::StringLiteral(class_name), arg.span),
        Expr::new(ExprKind::StringLiteral(method_name), arg.span),
    ))
}

