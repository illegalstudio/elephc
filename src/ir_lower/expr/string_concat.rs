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
pub(crate) fn persist_concat_lhs_if_rhs_can_reset(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: LoweredValue,
    rhs: &Expr,
    span: Span,
) -> LoweredValue {
    if !string_value_needs_persistence(ctx, lhs) || !expr_can_reset_concat_storage(rhs) {
        return lhs;
    }
    // A builtin's FRESH heap result cannot be overwritten by the right operand; copying it left
    // the original unreleased (`mb_strtoupper($a) . f()` leaked one block per evaluation).
    if ctx
        .builder
        .value_defining_instruction(lhs.value)
        .is_some_and(runtime_call_string_is_fresh)
    {
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
    op == Op::Cast || string_op_uses_scratch_storage(op)
}

/// Returns whether a string-producing opcode exposes scratch or borrowed string storage.
pub(crate) fn string_op_uses_scratch_storage(op: Op) -> bool {
    matches!(
        op,
        Op::IToStr
            | Op::FToStr
            | Op::BoolToStr
            | Op::ResourceToStr
            | Op::MixedCastString
            | Op::StrConcat
            | Op::StrBitAnd
            | Op::StrBitOr
            | Op::StrBitXor
            | Op::StrSetOffset
            | Op::StrInterpolate
            | Op::RuntimeCall
    )
    // Not `StrCharAt`: `$s[$i]` is an owned one-byte copy (`lower_str_char_at` persists it), so
    // persisting it again on a return or a store orphaned the first copy.
}

/// Returns whether a runtime call hands back a FRESH string the caller owns.
///
/// `string_op_uses_scratch_storage` answers by opcode, and `RuntimeCall` covers both builtins that
/// format into the shared scratch buffer and builtins that allocate (`mb_strtoupper()`,
/// `iconv()`, ...). Persisting a fresh result again at a return or a wrapper boundary orphaned
/// the first copy: every `return mb_strtoupper($s);` leaked one block.
pub(crate) fn runtime_call_string_is_fresh(inst: &crate::ir::Instruction) -> bool {
    use crate::builtins::semantics::BuiltinResultOwnership;
    use crate::ir::{Immediate, RuntimeCallTarget};
    if inst.op != Op::RuntimeCall {
        return false;
    }
    match &inst.immediate {
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))) => {
            matches!(target.result_ownership(), BuiltinResultOwnership::Fresh)
        }
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction { target, .. })) => {
            matches!(target.result_ownership(), BuiltinResultOwnership::Fresh)
        }
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Pcntl(target))) => {
            matches!(target.result_ownership(), BuiltinResultOwnership::Fresh)
        }
        // `UnaryString` targets (`strtoupper`, `trim`, ...) format into the concat arena: scratch,
        // whatever the release side calls them.
        _ => false,
    }
}

/// Returns whether a string is a `Cast` of a non-string value, whose result is either scratch or
/// a fresh heap copy depending on the run-time tag.
///
/// `__rt_mixed_cast_string` formats an int, float or bool into the shared concat buffer and hands
/// back a fresh owned copy for a boxed string. Either way the value must be persisted before it
/// crosses a function boundary, and the source released afterwards: the release frees the owned
/// copy and ignores scratch (`__rt_heap_free_safe`). `strval($mixed)` returned the arena pointer,
/// so `array_map('strval', $list)` printed `["[","["]` once json_encode() reused the buffer.
/// A cast FROM a string may return the source itself and is not included.
pub(crate) fn cast_string_needs_owned_persist(
    builder: &crate::ir::Builder<'_>,
    value: crate::ir::ValueId,
) -> bool {
    let Some(inst) = builder.value_defining_instruction(value) else {
        return false;
    };
    if inst.op != Op::Cast || inst.result_php_type.codegen_repr() != crate::types::PhpType::Str {
        return false;
    }
    inst.operands.first().is_some_and(|operand| {
        builder.value_php_type(*operand).codegen_repr() != crate::types::PhpType::Str
    })
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
        ExprKind::DynamicStaticPropertyAccess { property, .. } => {
            expr_can_reset_concat_storage(property)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_can_reset_concat_storage(left) || expr_can_reset_concat_storage(right)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_can_reset_concat_storage(value)
                || matches!(target, InstanceOfTarget::Expr(inner) if expr_can_reset_concat_storage(inner))
        }
        ExprKind::Negate(inner)
        | ExprKind::ArrayReference(inner)
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
        ExprKind::DynamicScopedConstantAccess { receiver, .. } => {
            expr_can_reset_concat_storage(receiver)
        }
    }
}

/// Returns whether constructing a callable target evaluates an expression that can reset concat.
pub(super) fn callable_target_can_reset_concat_storage(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => false,
        CallableTarget::Method { object, .. } => expr_can_reset_concat_storage(object),
    }
}
