//! Purpose:
//! String concatenation lowering and scratch-storage lifetime analysis.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers string concatenation.
pub(super) fn lower_concat(ctx: &mut LoweringContext<'_, '_>, left: &Expr, right: &Expr, expr: &Expr) -> LoweredValue {
    let lhs = lower_expr(ctx, left);
    let lhs = coerce_to_string(ctx, lhs, expr);
    let lhs = crate::ir_lower::ownership::persist_string_byte_view(ctx, lhs, Some(left.span));
    let lhs = persist_concat_lhs_if_rhs_can_reset(ctx, lhs, right, expr.span);
    let mut pending = [lhs];
    let (rhs, lhs_pins) = lower_with_pending_owners(ctx, &mut pending, left.span, |ctx| {
        let rhs = lower_expr(ctx, right);
        coerce_to_string(ctx, rhs, expr)
    });
    let lhs = pending[0];
    unpin_in_flight_owners(ctx, lhs_pins, left.span);
    if lhs.ir_type == IrType::Str && rhs.ir_type == IrType::Str {
        let result = ctx.emit_value(
            Op::StrConcat,
            vec![lhs.value, rhs.value],
            None,
            PhpType::Str,
            Op::StrConcat.default_effects(),
            Some(expr.span),
        );
        release_binary_operand_temporary(ctx, lhs, expr.span);
        if rhs.value != lhs.value {
            release_binary_operand_temporary(ctx, rhs, expr.span);
        }
        return result;
    }
    let result = ctx.emit_value(
        Op::RuntimeCall,
        vec![lhs.value, rhs.value],
        None,
        PhpType::Str,
        effects_lookup::runtime_effects(),
        Some(expr.span),
    );
    release_binary_operand_temporary(ctx, lhs, expr.span);
    if rhs.value != lhs.value {
        release_binary_operand_temporary(ctx, rhs, expr.span);
    }
    result
}

/// Persists scratch-backed concat LHS values before a call-like RHS can reset concat storage.
/// A heap-owned LHS already survives the call and remains the binary operand's cleanup owner.
pub(super) fn persist_concat_lhs_if_rhs_can_reset(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: LoweredValue,
    rhs: &Expr,
    span: Span,
) -> LoweredValue {
    if !string_value_needs_persistence(ctx, lhs) || !expr_can_reset_concat_storage(rhs) {
        return lhs;
    }
    persist_pending_string(ctx, lhs, span)
}

/// Recognizes views that can expose scratch storage, including scalar arms of Mixed casts.
pub(crate) fn string_value_needs_persistence(ctx: &LoweringContext<'_, '_>, value: LoweredValue) -> bool {
    if value.ir_type != IrType::Str || ctx.builder.value_ownership(value.value) == Ownership::Owned {
        return false;
    }
    let Some(op) = ctx.builder.value_defining_op(value.value) else { return false; };
    matches!(
        op,
        Op::Cast
            | Op::IToStr
            | Op::FToStr
            | Op::BoolToStr
            | Op::ResourceToStr
            | Op::MixedCastString
            | Op::StrConcat
            | Op::StrCharAt
            | Op::StrInterpolate
            | Op::RuntimeCall
    )
}

/// Copies a pending view and retires any detached string owned by its original producer.
/// Mixed casts allocate for string inputs but borrow formatter storage for numeric inputs.
pub(crate) fn persist_pending_string(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    let persisted = ctx.emit_value(
        Op::StrPersist, vec![value.value], None, PhpType::Str,
        Op::StrPersist.default_effects(), Some(span),
    );
    release_binary_operand_temporary(ctx, value, span);
    persisted
}

/// Returns whether evaluating an expression can reset the caller's concat scratch storage.
pub(super) fn expr_can_reset_concat_storage(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Reference(inner) => expr_can_reset_concat_storage(inner),
        // `IncludeValue` is a transient parser node fully expanded by the resolver;
        // it can never reach this pass.
        ExprKind::IncludeValue { .. } => unreachable!(
            "ExprKind::IncludeValue must be expanded by the resolver"
        ),
        ExprKind::FunctionCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::NullsafeDynamicMethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::NewGeneric { .. }
        | ExprKind::NewDynamic { .. }
        | ExprKind::NewDynamicObject { .. }
        | ExprKind::NewScopedObject { .. }
        | ExprKind::Clone(_)
        | ExprKind::Pipe { .. }
        | ExprKind::Yield { .. }
        | ExprKind::YieldFrom(_) => true,
        ExprKind::BinaryOp { left, right, .. } => {
            expr_can_reset_concat_storage(left) || expr_can_reset_concat_storage(right)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_can_reset_concat_storage(value)
                || matches!(target, InstanceOfTarget::Expr(inner) if expr_can_reset_concat_storage(inner))
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::BufferNew { len: inner, .. }
        | ExprKind::ObjectClassName { object: inner } => {
            expr_can_reset_concat_storage(inner)
        }
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_can_reset_concat_storage(value) || expr_can_reset_concat_storage(default)
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            !prelude.is_empty()
                || expr_can_reset_concat_storage(target)
                || expr_can_reset_concat_storage(value)
                || result_target
                    .as_ref()
                    .is_some_and(|target| expr_can_reset_concat_storage(target))
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_can_reset_concat_storage),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .any(|(key, value)| expr_can_reset_concat_storage(key) || expr_can_reset_concat_storage(value)),
        ExprKind::ArrayLiteralMixed(entries) => entries
            .iter()
            .flat_map(|entry| entry.exprs())
            .any(expr_can_reset_concat_storage),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_can_reset_concat_storage(subject)
                || arms.iter().any(|(conditions, result)| {
                    conditions.iter().any(expr_can_reset_concat_storage)
                        || expr_can_reset_concat_storage(result)
                })
                || default
                    .as_ref()
                    .is_some_and(|default| expr_can_reset_concat_storage(default))
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_can_reset_concat_storage(array) || expr_can_reset_concat_storage(index)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_can_reset_concat_storage(condition)
                || expr_can_reset_concat_storage(then_expr)
                || expr_can_reset_concat_storage(else_expr)
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_can_reset_concat_storage(object) || expr_can_reset_concat_storage(property)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            expr_can_reset_concat_storage(object)
        }
        ExprKind::FirstClassCallable(target) => callable_target_can_reset_concat_storage(target),
        ExprKind::Closure { .. }
        | ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => false,
    }
}

/// Returns whether constructing a callable target evaluates an expression that can reset concat.
pub(super) fn callable_target_can_reset_concat_storage(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => false,
        CallableTarget::Method { object, .. } => expr_can_reset_concat_storage(object),
    }
}
