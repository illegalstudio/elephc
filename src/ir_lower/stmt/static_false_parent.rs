//! Purpose:
//! Promotes retained null and false parents across nested array write contexts.
//!
//! Called from:
//! - `super::nested_array_writes::lower_static_update_key_and_guard()`.
//! - `super::nested_false_errors::capture()` for plain stores and appends.
//!
//! Key details:
//! - The writable cell is updated before the deprecation callback observes it.
//! - Update-only null conversion is silent and precedes missing-key diagnostics.
//! - Static updates stop on callback throws; plain writes preserve opcode completion separately.
//! - Existing receiver owners stay published across notification on every target.
//! - A sole pending owner after notification means the original parent was detached.

use super::*;

/// Converts a retained null update receiver before its write-context dimension is read.
pub(super) fn normalize_null_for_update(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, span: Span,
) {
    if ctx.builder.value_php_type(parent.value).codegen_repr() != PhpType::Mixed {
        return;
    }
    let null = ctx.emit_value(Op::IsNull, vec![parent.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    let promote = ctx.builder.create_named_block("static.update.null.promote", Vec::new());
    let done = ctx.builder.create_named_block("static.update.null.done", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: null.value, then_target: promote, then_args: Vec::new(),
        else_target: done, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(promote);
    let empty = lower_expr(ctx, &Expr::new(ExprKind::ArrayLiteral(Vec::new()), span));
    ctx.emit_void(Op::RuntimeCall, vec![parent.value, empty.value], None,
        effects_lookup::runtime_effects(), Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, empty, Some(span));
    branch_to(ctx, done);
    ctx.builder.position_at_end(done);
}

/// Converts false and reports whether the selected cell remains attached after notification.
/// Non-false parents retain the caller's ordinary read and scalar-guard behavior.
pub(super) fn normalize_for_update(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, span: Span,
) -> LoweredValue {
    let attached = ctx.declare_synthetic_php_local(PhpType::Bool);
    let present = lower_expr(ctx, &Expr::new(ExprKind::BoolLiteral(true), span));
    ctx.store_local(&attached, present, PhpType::Bool, Some(span));
    if ctx.builder.value_php_type(parent.value).codegen_repr() != PhpType::Mixed {
        return present;
    }
    let boolean = ctx.emit_value(Op::TypePredicate, vec![parent.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Bool)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let truth = ctx.emit_value(Op::IsTruthy, vec![parent.value], None, PhpType::Bool,
        Op::IsTruthy.default_effects(), Some(span));
    let zero = ctx.builder.emit_const_i64(0);
    let falsy = ctx.emit_value(Op::ICmp, vec![truth.value, zero],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)), PhpType::Bool,
        Op::ICmp.default_effects(), Some(span));
    let false_parent = ctx.emit_value(Op::IBitAnd, vec![boolean.value, falsy.value], None,
        PhpType::Bool, Op::IBitAnd.default_effects(), Some(span));
    let promote = ctx.builder.create_named_block("static.update.false.promote", Vec::new());
    let done = ctx.builder.create_named_block("static.update.false.done", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: false_parent.value, then_target: promote, then_args: Vec::new(),
        else_target: done, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(promote);
    let empty = lower_expr(ctx, &Expr::new(ExprKind::ArrayLiteral(Vec::new()), span));
    ctx.emit_void(Op::RuntimeCall, vec![parent.value, empty.value], None,
        effects_lookup::runtime_effects(), Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, empty, Some(span));
    let message = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral(
        "Deprecated: Automatic conversion of false to array is deprecated\n".into()), span));
    ctx.emit_void(Op::RuntimeCall, vec![message.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::WriteDiagnostic)),
        effects_lookup::runtime_effects(), Some(span));
    let retained = ctx.emit_value(Op::RuntimeCall, vec![parent.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::MixedCellHasOtherOwner)),
        PhpType::Bool, effects_lookup::runtime_effects(), Some(span));
    ctx.store_local(&attached, retained, PhpType::Bool, Some(span));
    ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
    ctx.builder.position_at_end(done);
    ctx.load_local(&attached, Some(span))
}

/// Skips a detached promoted parent's missing-key read without changing the update result.
pub(super) fn read_update_leaf(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, key: &Expr, expr: &Expr,
    attached: LoweredValue, span: Span,
) -> LoweredValue {
    let selected = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    ctx.clear_owned_hidden_temp(&selected, Some(span));
    let initialized = ctx.initialized_slots_snapshot();
    let read = ctx.builder.create_named_block("static.update.attached.read", Vec::new());
    let detached = ctx.builder.create_named_block("static.update.detached.null", Vec::new());
    let done = ctx.builder.create_named_block("static.update.leaf.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: attached.value, then_target: read, then_args: Vec::new(),
        else_target: detached, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(read);
    let value = lower_array_access_from_lowered_receiver(ctx, parent, key, expr);
    let owned = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    ctx.store_call_argument_local(&selected, owned, PhpType::Mixed, Some(span));
    if owned.value != value.value && ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    branch_to(ctx, done);
    ctx.builder.position_at_end(detached);
    ctx.restore_initialized_slots(initialized);
    let null = lower_expr(ctx, &Expr::new(ExprKind::Null, span));
    let null = ctx.box_value_as_mixed(null, PhpType::Mixed, Some(span));
    ctx.store_call_argument_local(&selected, null, PhpType::Mixed, Some(span));
    branch_to(ctx, done);
    ctx.builder.position_at_end(done);
    let value = ctx.load_local(&selected, Some(span));
    ctx.clear_owned_hidden_temp(&selected, Some(span));
    value
}
