//! Purpose:
//! Promotes a retained false parent before a nested static array update.
//!
//! Called from:
//! - `super::nested_array_writes::lower_static_update_key_and_guard()`.
//!
//! Key details:
//! - The writable cell is updated before the deprecation callback observes it.
//! - A throwing callback leaves the promoted array empty and skips the leaf update.
//! - Existing receiver owners stay published across notification on every target.

use super::*;

/// Converts only false, leaving true and other invalid scalar parents for the ordinary guard.
pub(super) fn normalize_for_update(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, span: Span,
) {
    if ctx.builder.value_php_type(parent.value).codegen_repr() != PhpType::Mixed { return; }
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
    ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
    ctx.builder.position_at_end(done);
}
