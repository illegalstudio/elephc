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
//! - Those conversions carry `Immediate::Bool(true)`, which bounds runtime name resolution
//!   (see `lower_elephc_normalize_callable`) so a program does not link every builtin wrapper.

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
    // The immediate marks a declared callable: a runtime name it resolves is limited to user
    // functions, static methods and the names written as literals at declared callables
    // (`module.declared_callable_names`), not every builtin wrapper.
    let descriptor = ctx.emit_owned_value(
        Op::NormalizeCallable,
        vec![value.value],
        Some(Immediate::Bool(true)),
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
/// Only a Closure has the rebinding methods. `instanceof Closure` on the receiver selects the
/// Closure rebinding lowering, which owns the receiver exactly as it does for any boxed closure
/// (so the source is retired, and its destructor runs, at the same point), or ordinary dynamic
/// method dispatch, which runs an invokable object's own method and raises PHP's
/// `Call to a member function bindTo() on string` for a string or array. Both arms leave their
/// boxed result in one owned temp, moved out after the join.
pub(crate) fn lower_declared_callable_closure_method(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let span = expr.span;
    let class_data = ctx.intern_class_name("Closure");
    let is_closure = ctx.emit_value(
        Op::InstanceOf,
        vec![object.value],
        Some(Immediate::Data(class_data)),
        PhpType::Bool,
        Op::InstanceOf.default_effects(),
        Some(span),
    );
    let result_name = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let closure_block = ctx.builder.create_named_block("declared.callable.closure", Vec::new());
    let dynamic_block = ctx.builder.create_named_block("declared.callable.dynamic", Vec::new());
    let done = ctx.builder.create_named_block("declared.callable.method.done", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_closure.value,
        then_target: closure_block,
        then_args: Vec::new(),
        else_target: dynamic_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(closure_block);
    let receiver = Expr::new(ExprKind::Variable(String::new()), span);
    if let Some(bound) = lower_closure_bind_method(ctx, &receiver, &object, method, args, expr) {
        store_value_into_temp(ctx, &result_name, PhpType::Mixed, bound, span);
    }
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
    }

    ctx.builder.position_at_end(dynamic_block);
    let receiver_name = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    store_value_into_temp(ctx, &receiver_name, PhpType::Mixed, object, span);
    let dispatch = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(Expr::new(ExprKind::Variable(receiver_name.clone()), span)),
            method: method.to_string(),
            args: args.to_vec(),
        },
        span,
    );
    let dispatched = lower_expr(ctx, &dispatch);
    store_value_into_temp(ctx, &result_name, PhpType::Mixed, dispatched, span);
    ctx.release_owned_hidden_temp(&receiver_name, Some(span));
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
    }

    ctx.builder.position_at_end(done);
    // The result moves out of its temp, so its consumer owns the only reference.
    let result = ctx.load_local(&result_name, Some(span));
    ctx.clear_owned_hidden_temp(&result_name, Some(span));
    result
}
