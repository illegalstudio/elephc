//! Purpose:
//! Preserves catchable PHP errors when a nested write selects a scalar parent.
//!
//! Called from:
//! - Nested offset assignment and array-push statement lowering.
//!
//! Key details:
//! - Guards run after the RHS and borrow operands published by the caller.
//! - Arrays, objects, null and supported string offsets keep their existing lowering.
//! - Rejected appends release an owned string RHS that no container can retain.

use super::*;

/// Rejects scalar write parents without replacing legal autovivification or string offsets.
/// A missing index denotes append, which always rejects a selected string.
pub(super) fn guard_nested_write_parent(
    ctx: &mut LoweringContext<'_, '_>,
    parent: LoweredValue,
    index: Option<&Expr>,
    span: Span,
) -> bool {
    guard_nested_write_parent_with_string_cleanup(ctx, parent, index, None, &[], span)
}

/// Guards an append parent and releases an owned string RHS on every rejected branch.
pub(super) fn guard_array_push_parent(
    ctx: &mut LoweringContext<'_, '_>,
    parent: LoweredValue,
    value: LoweredValue,
    pins: &[crate::ir_lower::expr::PinnedInFlightOwner],
    span: Span,
) -> bool {
    let string = (ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Str)
        .then_some(value);
    guard_nested_write_parent_with_string_cleanup(ctx, parent, None, string, pins, span)
}

/// Rejects invalid parents and optionally balances an owned string that the write cannot consume.
fn guard_nested_write_parent_with_string_cleanup(
    ctx: &mut LoweringContext<'_, '_>,
    parent: LoweredValue,
    index: Option<&Expr>,
    error_string: Option<LoweredValue>,
    error_string_pins: &[crate::ir_lower::expr::PinnedInFlightOwner],
    span: Span,
) -> bool {
    let invalid_string_offset = index.is_none() || index.is_some_and(|index| {
        matches!(&index.kind, ExprKind::StringLiteral(key)
            if crate::numeric_string::scan_numeric_prefix(key).is_none())
    });
    let parent_ty = ctx.builder.value_php_type(parent.value).codegen_repr();
    if parent_ty == PhpType::Str && invalid_string_offset {
        release_error_string(ctx, error_string, error_string_pins, span);
        throw_string_write_error(ctx, index.is_none(), span);
        return false;
    }
    if matches!(parent_ty, PhpType::Int | PhpType::Float | PhpType::Resource(_)) {
        release_error_string(ctx, error_string, error_string_pins, span);
        // Keep expression continuations well-formed after the catchable scalar-write Error.
        let _ = super::control_exit::lower_throw_access_error_expr(
            ctx, "Cannot use a scalar value as an array", span,
        );
        return false;
    }
    if !matches!(parent_ty, PhpType::Mixed | PhpType::Bool) {
        return true;
    }
    if invalid_string_offset {
        let initialized = ctx.initialized_slots_snapshot();
        let is_string = parent_type_predicate(ctx, parent, crate::ir::PhpTypePredicate::String, span);
        let string_error = ctx.builder.create_named_block("nested.write.string.error", Vec::new());
        let scalar_check = ctx.builder.create_named_block("nested.write.scalar.check", Vec::new());
        branch_on_guard(ctx, is_string, string_error, scalar_check);
        ctx.builder.position_at_end(string_error);
        release_error_string(ctx, error_string, error_string_pins, span);
        throw_string_write_error(ctx, index.is_none(), span);
        ctx.builder.position_at_end(scalar_check);
        ctx.restore_initialized_slots(initialized);
    }
    let mut invalid = parent_type_predicate(ctx, parent, crate::ir::PhpTypePredicate::Int, span);
    for predicate in [crate::ir::PhpTypePredicate::Float, crate::ir::PhpTypePredicate::Resource] {
        let other = parent_type_predicate(ctx, parent, predicate, span);
        invalid = combine_guard(ctx, Op::IBitOr, invalid, other, span);
    }
    // PHP permits false-to-array autovivification, but true is an invalid scalar parent.
    let is_bool = parent_type_predicate(ctx, parent, crate::ir::PhpTypePredicate::Bool, span);
    let truth = ctx.emit_value(
        Op::Cast, vec![parent.value], Some(Immediate::CastTarget(IrType::I64)),
        PhpType::Bool, Op::Cast.default_effects(), Some(span),
    );
    let true_scalar = combine_guard(ctx, Op::IBitAnd, is_bool, truth, span);
    invalid = combine_guard(ctx, Op::IBitOr, invalid, true_scalar, span);
    let scalar_error = ctx.builder.create_named_block("nested.write.scalar.error", Vec::new());
    let writable = ctx.builder.create_named_block("nested.write.parent", Vec::new());
    let initialized = ctx.initialized_slots_snapshot();
    branch_on_guard(ctx, invalid, scalar_error, writable);
    ctx.builder.position_at_end(scalar_error);
    release_error_string(ctx, error_string, error_string_pins, span);
    lower_throw_access_error(ctx, "Cannot use a scalar value as an array", span);
    ctx.builder.position_at_end(writable);
    ctx.restore_initialized_slots(initialized);
    true
}

/// Releases a string temporary that no rejected append branch can transfer into a container.
fn release_error_string(
    ctx: &mut LoweringContext<'_, '_>,
    value: Option<LoweredValue>,
    pins: &[crate::ir_lower::expr::PinnedInFlightOwner],
    span: Span,
) {
    if let Some(value) = value {
        crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins.to_vec(), span);
        super::array_write_core::release_persisted_string_operand(ctx, value, span);
    }
}

/// Tests a boxed parent's runtime type without unboxing or consuming its storage.
fn parent_type_predicate(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue,
    predicate: crate::ir::PhpTypePredicate, span: Span,
) -> LoweredValue {
    ctx.emit_value(
        Op::TypePredicate, vec![parent.value], Some(Immediate::TypePredicate(predicate)),
        PhpType::Bool, Op::TypePredicate.default_effects(), Some(span),
    )
}

/// Combines boolean guards through target-neutral integer EIR operations.
fn combine_guard(
    ctx: &mut LoweringContext<'_, '_>, op: Op,
    left: LoweredValue, right: LoweredValue, span: Span,
) -> LoweredValue {
    ctx.emit_value(op, vec![left.value, right.value], None, PhpType::Bool, op.default_effects(), Some(span))
}

/// Splits control flow without transferring the checked parent's ownership.
fn branch_on_guard(
    ctx: &mut LoweringContext<'_, '_>, condition: LoweredValue,
    rejected: BlockId, accepted: BlockId,
) {
    ctx.builder.terminate(Terminator::CondBr {
        cond: condition.value, then_target: rejected, then_args: Vec::new(),
        else_target: accepted, else_args: Vec::new(),
    });
}

/// Throws the distinct PHP exception class and message for string append or invalid offset.
fn throw_string_write_error(ctx: &mut LoweringContext<'_, '_>, append: bool, span: Span) {
    let (class, message) = if append {
        ("Error", "[] operator not supported for strings")
    } else {
        ("TypeError", "Cannot access offset of type string on string")
    };
    let exception = lower_expr(ctx, &Expr::new(ExprKind::NewObject {
        class_name: crate::names::Name::unqualified(class),
        args: vec![Expr::new(ExprKind::StringLiteral(message.to_string()), span)],
    }, span));
    terminate_throw(ctx, exception.value);
}
