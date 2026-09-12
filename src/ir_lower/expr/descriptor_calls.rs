//! Purpose:
//! Literal and dynamic callable descriptor expression calls.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers direct calls to literal callable arrays through descriptor metadata.
pub(super) fn lower_literal_callable_array_expr_call(
    ctx: &mut LoweringContext<'_, '_>,
    callee: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let ExprKind::ArrayLiteral(items) = &callee.kind else {
        return None;
    };
    if let Some(StaticCallableBinding::StaticMethodDescriptor { receiver, method }) =
        static_array_callable_descriptor_target(ctx, items)
    {
        return Some(lower_static_method_descriptor_call(ctx, &receiver, &method, args, expr));
    }
    let target = instance_array_callable_target(ctx, items)?;
    let signature = signature_for_static_callable_binding(ctx, target);
    let lowered_callee = lower_expr(ctx, callee);
    let result_type = signature
        .as_ref()
        .map(|sig| descriptor_invoker_result_type(Some(sig)))
        .unwrap_or_else(|| dynamic_callable_result_type(ctx, lowered_callee.value, expr));
    let lowered_callee = root_descriptor_callback(ctx, lowered_callee, result_type, expr.span);
    let arg_container = lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span);
    Some(emit_callable_descriptor_invoke(
        ctx,
        lowered_callee,
        arg_container,
        expr.span,
    ))
}

/// Lowers an expression call once the callable expression is already evaluated.
pub(super) fn lower_expr_call_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    callee: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let result_type = dynamic_callable_result_type(ctx, callee.value, expr);
    let callee = root_descriptor_callback(ctx, callee, result_type, expr.span);
    let arg_container = lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span);
    emit_callable_descriptor_invoke(ctx, callee, arg_container, expr.span)
}

/// Lowers explicit named arguments for signature-unknown descriptor invocations.
///
/// Every argument shape has a runtime container form, so this never declines: named arguments
/// and spreads build a key-normalized boxed hash, and plain positional arguments build an
/// indexed array. Callers rely on that totality, because the callback is already published in
/// the unwind chain by the time the container is built and there is no shape to fall back to.
pub(super) fn lower_untyped_descriptor_invoker_arg_container(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    span: Span,
) -> LoweredValue {
    if crate::types::call_args::has_named_args(args)
        || descriptor_args_need_runtime_unpack_keys(args)
    {
        return lower_untyped_descriptor_invoker_hash_container(ctx, args, span);
    }
    lower_untyped_descriptor_invoker_indexed_container(ctx, args, span)
}

/// Builds an indexed descriptor-invoker container for signature-unknown calls.
pub(super) fn lower_untyped_descriptor_invoker_indexed_container(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    span: Span,
) -> LoweredValue {
    let elem_ty = PhpType::Mixed;
    let array_ty = PhpType::Array(Box::new(elem_ty.clone()));
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(args.len() as u32)),
        array_ty.clone(),
        Op::ArrayNew.default_effects(),
        Some(span),
    );
    let owner = publish_constructed_container(ctx, array, span);
    for arg in args {
        let value = lower_untyped_descriptor_invoker_arg_value(ctx, arg);
        let array = load_published_container(
            ctx,
            owner,
            array_ty.clone(),
            arg.span,
        );
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(arg.span),
        );
        crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, Some(&elem_ty), value, arg.span);
    }
    take_published_container(ctx, owner, array_ty, span)
}

/// Builds an associative descriptor-invoker container for named or named/spread calls.
pub(super) fn lower_untyped_descriptor_invoker_hash_container(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    span: Span,
) -> LoweredValue {
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(args.len() as u32)),
        hash_ty.clone(),
        Op::HashNew.default_effects(),
        Some(span),
    );
    let owner = publish_constructed_container(ctx, hash, span);
    let state = begin_descriptor_unpack(ctx, owner, hash_ty.clone(), span);
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                let key = lower_string_literal(ctx, name, arg);
                let value = lower_untyped_descriptor_invoker_arg_value(ctx, value);
                bind_descriptor_unpack_named(ctx, &state, key, value, arg.span);
            }
            ExprKind::Spread(inner) => {
                let source = lower_expr(ctx, inner);
                lower_descriptor_unpack_source(ctx, &state, source, arg.span);
            }
            _ => {
                let value = lower_untyped_descriptor_invoker_arg_value(ctx, arg);
                bind_descriptor_unpack_positional(ctx, &state, value, arg.span);
            }
        }
    }
    let hash = load_published_container(ctx, owner, hash_ty, span);
    let boxed = ctx.box_value_as_mixed(hash, PhpType::Mixed, Some(span));
    retire_owned_call_operand(ctx, owner, span);
    boxed
}

/// Lowers one untyped descriptor argument, preserving variables as ref markers.
pub(super) fn lower_untyped_descriptor_invoker_arg_value(
    ctx: &mut LoweringContext<'_, '_>,
    arg: &Expr,
) -> LoweredValue {
    let value = match &arg.kind {
        ExprKind::Variable(name) => lower_invoker_ref_arg_marker(ctx, name, arg.span),
        _ => lower_expr(ctx, arg),
    };
    coerce_descriptor_invoker_mixed_value(ctx, value, arg.span)
}

/// Boxes a descriptor-invoker argument value into the Mixed slot shape.
pub(super) fn coerce_descriptor_invoker_mixed_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Mixed {
        return value;
    }
    ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span))
}

/// Returns the result storage type for an indirect callable with no static signature.
pub(super) fn dynamic_callable_result_type(
    ctx: &LoweringContext<'_, '_>,
    callable: ValueId,
    expr: &Expr,
) -> PhpType {
    match ctx.builder.value_php_type(callable).codegen_repr() {
        PhpType::Callable | PhpType::Str | PhpType::Array(_) | PhpType::Mixed | PhpType::Union(_) => PhpType::Mixed,
        _ => fallback_expr_type(expr),
    }
}

/// Resolves an assignment-expression callee whose assigned value is a static callable.
pub(super) fn static_assignment_callable_target(
    ctx: &LoweringContext<'_, '_>,
    callee: &Expr,
) -> Option<StaticCallableBinding> {
    let ExprKind::Assignment { target, value, .. } = &callee.kind else {
        return None;
    };
    if !matches!(target.kind, ExprKind::Variable(_)) {
        return None;
    }
    static_callable_binding_for_expr(ctx, value).and_then(direct_static_callable_binding)
}

/// Lowers direct invocation of a literal first-class callable target.
pub(super) fn lower_first_class_callable_expr_call(
    ctx: &mut LoweringContext<'_, '_>,
    callee: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    match &callee.kind {
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            Some(lower_function_call(ctx, name, args, expr))
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            Some(lower_static_method_call(ctx, receiver, method, args, expr))
        }
        ExprKind::FirstClassCallable(target @ CallableTarget::Method { .. }) => {
            let signature = static_callable_binding_for_expr(ctx, callee)
                .and_then(|target| signature_for_static_callable_binding(ctx, target));
            let callable = lower_first_class_callable(ctx, target, callee);
            let result_type = signature
                .as_ref()
                .map(|signature| normalize_value_php_type(signature.return_type.codegen_repr()))
                .unwrap_or_else(|| dynamic_callable_result_type(ctx, callable.value, expr));
            let callable = root_descriptor_callback(ctx, callable, result_type, expr.span);
            let arg_container =
                lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span);
            Some(emit_callable_descriptor_invoke(
                ctx,
                callable,
                arg_container,
                expr.span,
            ))
        }
        _ => None,
    }
}
