//! Purpose:
//! Materializes callable descriptors from values declared `callable`.
//!
//! Called from:
//! - Callable invocation sites (pipe, descriptor calls) and argument coercion into
//!   descriptor-typed parameter slots.
//!
//! Key details:
//! - A declared `callable` (`PhpType::DeclaredCallable`) keeps the PHP value the caller
//!   passed in a boxed `Mixed` cell, so `var_dump()`, `gettype()` and `ctype_*()` observe the
//!   original string, array, object or Closure.
//! - Only a site that invokes the value, or stores it where a `Callable` descriptor is
//!   expected, converts it with `Op::NormalizeCallable`; the conversion yields an owned
//!   descriptor and retires an owned source value.

use super::*;

/// Returns an owned `Callable` descriptor for a `DeclaredCallable` value, or `value` itself
/// for every other type.
pub(crate) fn normalize_declared_callable(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if ctx.builder.value_php_type(value.value) != PhpType::DeclaredCallable {
        return value;
    }
    let descriptor = ctx.emit_owned_value(
        Op::NormalizeCallable,
        vec![value.value],
        None,
        PhpType::Callable,
        Op::NormalizeCallable.default_effects(),
        Some(span),
    );
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    descriptor
}

/// Returns whether `expr` statically evaluates to a value declared `callable`: a local or
/// parameter of that type, or a call to a user function whose return type is `callable`.
pub(crate) fn expr_is_declared_callable(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => ctx.local_type(name) == PhpType::DeclaredCallable,
        ExprKind::FunctionCall { name, .. } => ctx
            .functions
            .get(&php_symbol_key(name.as_str().trim_start_matches('\\')))
            .or_else(|| ctx.functions.get(name.as_str()))
            .is_some_and(|sig| sig.return_type == PhpType::DeclaredCallable),
        _ => super::merge_temps::materialized_expr_type_for_merge(ctx, expr)
            == PhpType::DeclaredCallable,
    }
}

/// Lowers `$callable->bindTo(...)` / `$callable->call(...)` on a declared `callable` value.
///
/// Only a Closure has the rebinding methods. The value is published in a `mixed` temp once,
/// then `instanceof Closure` selects the descriptor path (`Closure` rebinding) or ordinary
/// dynamic method dispatch, which runs an invokable object's own method and raises PHP's
/// `Call to a member function bindTo() on string` for a string or array.
pub(crate) fn lower_declared_callable_closure_method(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let span = expr.span;
    let temp = ctx.declare_hidden_temp(PhpType::Mixed);
    store_value_into_temp(ctx, &temp, PhpType::Mixed, object, span);
    let receiver = Expr::new(ExprKind::Variable(temp), span);
    let condition = Expr::new(
        ExprKind::InstanceOf {
            value: Box::new(receiver.clone()),
            target: InstanceOfTarget::Name(Name::unqualified("Closure")),
        },
        span,
    );
    let descriptor = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("__elephc_normalize_callable"),
            args: vec![receiver.clone()],
        },
        span,
    );
    let closure_call = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(descriptor),
            method: method.to_string(),
            args: args.to_vec(),
        },
        span,
    );
    let dynamic_call = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(receiver),
            method: method.to_string(),
            args: args.to_vec(),
        },
        span,
    );
    let selected = Expr::new(
        ExprKind::Ternary {
            condition: Box::new(condition),
            then_expr: Box::new(closure_call),
            else_expr: Box::new(dynamic_call),
        },
        span,
    );
    lower_expr(ctx, &selected)
}
