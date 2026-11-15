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
