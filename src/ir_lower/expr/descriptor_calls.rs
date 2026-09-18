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
    instance_array_callable_target(ctx, items)?;
    let lowered_callee = lower_expr(ctx, callee);
    let result_type = dynamic_callable_result_type(ctx, lowered_callee.value, expr);
    let arg_container = lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span)?;
    Some(emit_callable_descriptor_invoke(
        ctx,
        lowered_callee,
        arg_container,
        result_type,
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
    if let Some(arg_container) =
        lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span)
    {
        return emit_callable_descriptor_invoke(ctx, callee, arg_container, result_type, expr.span);
    }
    let mut operands = vec![callee.value];
    operands.extend(lower_args(ctx, args));
    ctx.emit_value(
        Op::ExprCall,
        operands,
        callable_profile_immediate(),
        result_type,
        Op::ExprCall.default_effects(),
        Some(expr.span),
    )
}

/// Lowers explicit named arguments for signature-unknown descriptor invocations.
///
/// The container kind has to follow what the invoker will find in it, not just the spelling of
/// the call. Named arguments obviously need the hash, but so does a SPREAD of a string-keyed
/// array: `new $c(...$named)` binds those keys by name exactly as `new $c(b: 8)` does. Choosing
/// on `has_named_args` alone sent that call down the indexed path, where the hash source hit
/// `OperandTypeMismatch { expected: "Heap(Array)", actual: Heap(Hash) }` (issue #684's
/// neighbour, #685). A literal `new $c(...["b" => 8])` escaped it only because the spread is
/// expanded into real named arguments before lowering; a variable operand cannot be.
pub(super) fn lower_untyped_descriptor_invoker_arg_container(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    span: Span,
) -> Option<LoweredValue> {
    if crate::types::call_args::has_named_args(args) || spreads_a_keyed_array(ctx, args) {
        return Some(lower_untyped_descriptor_invoker_hash_container(ctx, args, span));
    }
    Some(lower_untyped_descriptor_invoker_indexed_container(ctx, args, span))
}

/// Reports whether any argument spreads a value whose storage is an associative array.
///
/// Decided on the STATIC type, because the container is allocated before any argument runs. A
/// spread whose type is only known as `Mixed` reads as "not keyed" and keeps the indexed
/// container it has always had; that is the pre-existing behaviour for a boxed operand and is
/// not what this predicate was added to change.
pub(super) fn spreads_a_keyed_array(ctx: &LoweringContext<'_, '_>, args: &[Expr]) -> bool {
    args.iter().any(|arg| {
        let ExprKind::Spread(inner) = &arg.kind else {
            return false;
        };
        matches!(
            spread_operand_type(ctx, inner).codegen_repr(),
            PhpType::AssocArray { .. }
        )
    })
}

/// Returns the static PHP type of a spread operand.
///
/// A local reads its slot type, which is what the checker recorded for it; anything else falls
/// back to the syntactic inference the rest of this pass uses.
fn spread_operand_type(ctx: &LoweringContext<'_, '_>, operand: &Expr) -> PhpType {
    match &operand.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        _ => infer_expr_type_syntactic(operand),
    }
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
    for arg in args {
        if let ExprKind::Spread(inner) = &arg.kind {
            let source = lower_expr(ctx, inner);
            lower_indexed_array_spread_into_array(ctx, array, source, Some(&elem_ty), arg.span);
            continue;
        }
        let value = lower_untyped_descriptor_invoker_arg_value(ctx, arg);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(arg.span),
        );
        crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, Some(&elem_ty), value, arg.span);
    }
    array
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
        hash_ty,
        Op::HashNew.default_effects(),
        Some(span),
    );
    let mut next_positional_key = emit_i64_at_span(ctx, 0, span);
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                let key = lower_string_literal(ctx, name, arg);
                let value = lower_untyped_descriptor_invoker_arg_value(ctx, value);
                ctx.emit_void(
                    Op::HashSet,
                    vec![hash.value, key.value, value.value],
                    None,
                    Op::HashSet.default_effects(),
                    Some(arg.span),
                );
            }
            ExprKind::Spread(inner) => {
                let source = lower_expr(ctx, inner);
                if matches!(
                    ctx.builder.value_php_type(source.value).codegen_repr(),
                    PhpType::AssocArray { .. }
                ) {
                    // `__rt_hash_spread` preserves string keys and reindexes integer ones from
                    // the destination's own largest integer key, which is exactly the argument
                    // binding PHP performs. `next_positional_key` is deliberately left alone:
                    // a positional argument after a string-keyed spread is invalid PHP
                    // ("Cannot use positional argument after named argument"), so there is no
                    // correct value to advance it to.
                    lower_hash_spread_into_hash_from_value(ctx, hash, source, arg.span);
                } else {
                    next_positional_key = lower_untyped_descriptor_invoker_spread_into_hash(
                        ctx,
                        hash,
                        source,
                        next_positional_key,
                        arg.span,
                    );
                }
            }
            _ => {
                let key = next_positional_key;
                let value = lower_untyped_descriptor_invoker_arg_value(ctx, arg);
                ctx.emit_void(
                    Op::HashSet,
                    vec![hash.value, key.value, value.value],
                    None,
                    Op::HashSet.default_effects(),
                    Some(arg.span),
                );
                let one = emit_i64_at_span(ctx, 1, arg.span);
                next_positional_key = ctx.emit_value(
                    Op::IAdd,
                    vec![key.value, one.value],
                    None,
                    PhpType::Int,
                    Op::IAdd.default_effects(),
                    Some(arg.span),
                );
            }
        }
    }
    ctx.box_value_as_mixed(hash, PhpType::Mixed, Some(span))
}

/// Copies an indexed spread source into a descriptor-invoker hash with numeric keys.
pub(super) fn lower_untyped_descriptor_invoker_spread_into_hash(
    ctx: &mut LoweringContext<'_, '_>,
    hash: LoweredValue,
    source: LoweredValue,
    start_key: LoweredValue,
    span: Span,
) -> LoweredValue {
    let source_elem_ty = match ctx.builder.value_php_type(source.value).codegen_repr() {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr(),
        _ => PhpType::Mixed,
    };
    let len = ctx.emit_value(
        Op::ArrayLen,
        vec![source.value],
        None,
        PhpType::Int,
        Op::ArrayLen.default_effects(),
        Some(span),
    );
    let zero = emit_i64_at_span(ctx, 0, span);
    let header = ctx.builder.create_named_block("descriptor.spread.next", vec![(IrType::I64, PhpType::Int)]);
    let body = ctx.builder.create_named_block("descriptor.spread.body", Vec::new());
    let exit = ctx.builder.create_named_block("descriptor.spread.exit", Vec::new());
    ctx.builder.terminate(Terminator::Br { target: header, args: vec![zero.value] });

    ctx.builder.position_at_end(header);
    let index = ctx.builder.block_param(header, 0);
    let has_next = ctx.emit_value(
        Op::ICmp,
        vec![index, len.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_next.value,
        then_target: body,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(body);
    let key = ctx.emit_value(
        Op::IAdd,
        vec![start_key.value, index],
        None,
        PhpType::Int,
        Op::IAdd.default_effects(),
        Some(span),
    );
    let value = ctx.emit_value(
        Op::ArrayGet,
        vec![source.value, index],
        None,
        source_elem_ty,
        Op::ArrayGet.default_effects(),
        Some(span),
    );
    let value = coerce_descriptor_invoker_mixed_value(ctx, value, span);
    ctx.emit_void(
        Op::HashSet,
        vec![hash.value, key.value, value.value],
        None,
        Op::HashSet.default_effects(),
        Some(span),
    );
    release_value_after_retaining_insert(ctx, Some(&PhpType::Mixed), value, span);
    let one = emit_i64_at_span(ctx, 1, span);
    let next = ctx.emit_value(
        Op::IAdd,
        vec![index, one.value],
        None,
        PhpType::Int,
        Op::IAdd.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::Br { target: header, args: vec![next.value] });

    ctx.builder.position_at_end(exit);
    crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    ctx.emit_value(
        Op::IAdd,
        vec![start_key.value, len.value],
        None,
        PhpType::Int,
        Op::IAdd.default_effects(),
        Some(span),
    )
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
            let arg_container =
                lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span)?;
            Some(emit_callable_descriptor_invoke(
                ctx,
                callable,
                arg_container,
                result_type,
                expr.span,
            ))
        }
        _ => None,
    }
}

