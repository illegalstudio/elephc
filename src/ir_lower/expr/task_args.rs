//! Purpose:
//! Snapshots unified TaskGroup submission arguments before executor selection.
//!
//! Called from:
//! - `super::task_dispatch::lower_runtime_task_submission()`.
//!
//! Key details:
//! - Shared call plans own parameter matching, unpack ordering and duplicate-key semantics.
//! - Reusable PHP-local snapshots retain callable metadata and unwind-visible owners.
//! - Static array unpacks evaluate every value, including overwritten entries, once.

use super::*;

/// Source-order snapshots and the canonical parameter-order projection of those snapshots.
pub(super) struct TaskArguments {
    pub arguments: Vec<Expr>,
    pub execution: Option<bool>,
    pub owners: Vec<(LocalSlotId, bool)>,
}

/// Evaluates source arguments once while retaining the shared planner's semantic mapping.
pub(super) fn snapshot_task_arguments(
    ctx: &mut LoweringContext<'_, '_>,
    signature: &FunctionSig,
    args: &[Expr],
    span: Span,
) -> TaskArguments {
    let original = crate::types::call_args::plan_call_args(signature, args, span, false, true)
        .expect("TaskGroup argument syntax was checked before lowering").normalized_args();
    let original = crate::types::call_args::expand_planned_positional_spreads(&original).unwrap_or(original);
    let execution = original.first().and_then(crate::task_prelude::is_parallel_execution);
    let mut owners = Vec::new();
    let mut staged = Vec::new();
    for arg in args {
        staged.push(match &arg.kind {
            ExprKind::NamedArg { name, value } => Expr::new(ExprKind::NamedArg {
                name: name.clone(), value: Box::new(snapshot_value(ctx, value, &mut owners)),
            }, arg.span),
            ExprKind::Spread(inner) => {
                let projection = crate::types::call_args::expand_static_assoc_spread_args(std::slice::from_ref(arg));
                let staged_inner = match &inner.kind {
                    ExprKind::ArrayLiteral(items) if crate::types::call_args::expand_planned_positional_spreads(
                        std::slice::from_ref(arg)).is_some() => {
                        Expr::new(ExprKind::ArrayLiteral(items.iter().map(|item| {
                            snapshot_value(ctx, item, &mut owners)
                        }).collect()), inner.span)
                    }
                    ExprKind::ArrayLiteralAssoc(pairs) if !projection.iter().any(|expr| matches!(expr.kind, ExprKind::Spread(_))) => {
                        snapshot_assoc_unpack(ctx, pairs, inner.span, &mut owners)
                    }
                    _ => snapshot_value(ctx, inner, &mut owners),
                };
                Expr::new(ExprKind::Spread(Box::new(staged_inner)), arg.span)
            }
            _ => snapshot_value(ctx, arg, &mut owners),
        });
    }
    let planned = crate::types::call_args::plan_call_args(signature, &staged, span, false, true)
        .expect("snapshot projection preserves checked TaskGroup syntax").normalized_args();
    let arguments = crate::types::call_args::expand_planned_positional_spreads(&planned).unwrap_or(planned);
    TaskArguments { arguments, execution, owners }
}

/// Holds one source value in a reusable local and preserves its checked static callback target.
fn snapshot_value(
    ctx: &mut LoweringContext<'_, '_>, expr: &Expr, owners: &mut Vec<(LocalSlotId, bool)>,
) -> Expr {
    let tracked = static_callable_binding_for_expr(ctx, expr);
    let value = lower_expr(ctx, expr);
    let callable = tracked.or_else(|| ctx.take_pending_static_callable_result());
    let php_type = ctx.builder.value_php_type(value.value);
    let rooted = Ownership::php_type_needs_lifetime_tracking(&php_type);
    let name = ctx.declare_synthetic_php_local(php_type.clone());
    ctx.store_call_argument_local(&name, value, php_type, Some(expr.span));
    if let Some(callable) = callable { ctx.bind_static_callable_local(&name, callable); }
    let slot = ctx.local_slots[&name];
    if rooted { register_owned_call_operand(ctx, slot, expr.span); }
    owners.push((slot, rooted));
    Expr::new(ExprKind::Variable(name), expr.span)
}

/// Preserves evaluation and overwrite retirement before shared static named-unpack expansion.
fn snapshot_assoc_unpack(
    ctx: &mut LoweringContext<'_, '_>, pairs: &[(Expr, Expr)], span: Span,
    owners: &mut Vec<(LocalSlotId, bool)>,
) -> Expr {
    let mut staged_pairs = Vec::new();
    let mut live = std::collections::HashSet::new();
    for (key, value) in pairs {
        let value = snapshot_value(ctx, value, owners);
        let ExprKind::Variable(name) = &value.kind else { unreachable!("snapshot is a local"); };
        live.insert(name.clone());
        staged_pairs.push((key.clone(), value));
        let prefix = Expr::new(ExprKind::Spread(Box::new(
            Expr::new(ExprKind::ArrayLiteralAssoc(staged_pairs.clone()), span))), span);
        let projected = crate::types::call_args::expand_static_assoc_spread_args(&[prefix]);
        let selected: std::collections::HashSet<_> = projected.iter().filter_map(|expr| {
            let value = match &expr.kind { ExprKind::NamedArg { value, .. } => value.as_ref(), _ => expr };
            match &value.kind { ExprKind::Variable(name) => Some(name.clone()), _ => None }
        }).collect();
        for retired in live.difference(&selected) {
            // Leave its published record in place: it now points at a zeroed
            // slot and must still be detached in the original LIFO order.
            ctx.emit_void(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(ctx.local_slots[retired])),
                Op::ReleaseLocalSlot.default_effects(), Some(span));
            ctx.clear_static_callable_local(retired);
        }
        live = selected;
    }
    Expr::new(ExprKind::ArrayLiteralAssoc(staged_pairs), span)
}
