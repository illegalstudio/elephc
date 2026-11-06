//! Purpose:
//! Lowers same-local string accumulation with an explicit destination-owner transfer.
//!
//! Called from:
//! - `local_assignments::lower_assign` before ordinary retaining assignment lowering.
//!
//! Key details:
//! - The first subset permits only string literals or initialized plain string RHS locals.
//! - RHS evaluation finishes before the adjacent load/append/store ownership transaction.
//! - Other expressions preserve ordinary concat evaluation and exception behavior.

use super::*;
use crate::parser::ast::BinOp;

/// Consumes one local owner only for a fully proved, side-effect-free append shape.
pub(super) fn try_lower(
    ctx: &mut LoweringContext<'_, '_>, name: &str, value: &Expr, span: Span,
) -> bool {
    let ExprKind::BinaryOp { op: BinOp::Concat, left, right } = &value.kind else {
        return false;
    };
    if !matches!(&left.kind, ExprKind::Variable(source) if source == name) { return false; }
    let Some(slot) = ctx.consuming_string_slot(name, span) else { return false; };
    match &right.kind {
        ExprKind::StringLiteral(_) => {}
        ExprKind::Variable(source) if rhs_is_plain_string(ctx, source) => {}
        _ => return false,
    }
    let rhs = lower_expr(ctx, right);
    debug_assert_eq!(rhs.ir_type, IrType::Str);
    let lhs = ctx.load_local(name, Some(span));
    let result = ctx.emit_owned_value(Op::StrAppend, vec![lhs.value, rhs.value],
        Some(Immediate::LocalSlot(slot)), PhpType::Str, Op::StrAppend.default_effects(), Some(span));
    ctx.store_consumed_string(name, slot, result, span);
    true
}

/// Allows borrowed string parameters on RHS without transferring their ownership.
fn rhs_is_plain_string(ctx: &LoweringContext<'_, '_>, name: &str) -> bool {
    let Some(&slot) = ctx.local_slots.get(name) else { return false; };
    let Some(local) = ctx.builder.function().locals.get(slot.as_raw() as usize) else { return false; };
    local.kind == LocalKind::PhpLocal && local.ir_type == IrType::Str && local.php_type == PhpType::Str
        && ctx.local_type(name) == PhpType::Str && ctx.slot_is_initialized(slot)
        && !ctx.is_ref_bound_local(name) && !ctx.local_uses_global_storage(name)
        && !ctx.extern_globals.contains_key(name)
}
