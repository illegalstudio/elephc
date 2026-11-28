//! Purpose:
//! Keeps intentional frame probes separate from ordinary initialized-read requirements.
//!
//! Called from:
//! - Lazy isset, empty and null-coalescing value lowering.
//!
//! Key details:
//! - Presence is tested before reading storage, including loop-carried bindings.
//! - Missing coalesce operands become owned null snapshots without evaluating a fallback twice.

use super::*;

/// The two boolean constructs that intentionally accept an absent source name.
#[derive(Clone, Copy)]
pub(super) enum Probe { Isset, Empty }

/// Short-circuits an absent simple variable before a null/truthiness probe reads its payload.
pub(super) fn boolean(ctx: &mut LoweringContext<'_, '_>, arg: &Expr, probe: Probe, expr: &Expr)
    -> Option<LoweredValue> {
    let ExprKind::Variable(name) = &arg.kind else { return None; };
    let present = crate::ir_lower::initialization::binding_presence(ctx, name, arg.span)?;
    let temp = ctx.declare_hidden_temp(PhpType::Bool);
    let ready = ctx.builder.create_named_block("probe.present", Vec::new());
    let missing = ctx.builder.create_named_block("probe.absent", Vec::new());
    let merge = ctx.builder.create_named_block("probe.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: present.value,
        then_target: ready, then_args: Vec::new(), else_target: missing, else_args: Vec::new() });
    ctx.builder.position_at_end(ready);
    ctx.begin_quiet_undefined_reads(arg);
    let value = lower_expr(ctx, arg);
    ctx.end_quiet_undefined_reads();
    let result = match probe {
        Probe::Isset => probe_value_is_set(ctx, value, arg),
        Probe::Empty => lower_empty_of_probed_value(ctx, value, "empty", expr),
    };
    store_value_into_temp(ctx, &temp, PhpType::Bool, result, expr.span);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(missing);
    let result = lower_bool_literal(ctx, matches!(probe, Probe::Empty), expr);
    store_value_into_temp(ctx, &temp, PhpType::Bool, result, expr.span);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    Some(take_owned_temp(ctx, &temp, expr.span))
}

/// Materializes an independent present value or null without loading absent frame storage.
pub(super) fn coalesce_value(ctx: &mut LoweringContext<'_, '_>, arg: &Expr) -> Option<LoweredValue> {
    let ExprKind::Variable(name) = &arg.kind else { return None; };
    // Native storage keeps its existing null representation and must not be boxed as a PHP value.
    if matches!(ctx.local_type(name).codegen_repr(), PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_)) {
        return None;
    }
    let present = crate::ir_lower::initialization::binding_presence(ctx, name, arg.span)?;
    let snapshot_type = normalize_union_members(vec![
        if ctx.reads_from_eval_scope(name) { PhpType::Mixed } else { ctx.local_type(name) }, PhpType::Void,
    ]).expect("a coalesce snapshot has at least its null alternative");
    let temp = ctx.declare_owned_hidden_temp(snapshot_type.clone());
    let ready = ctx.builder.create_named_block("coalesce.present", Vec::new());
    let missing = ctx.builder.create_named_block("coalesce.absent", Vec::new());
    let merge = ctx.builder.create_named_block("coalesce.presence.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: present.value,
        then_target: ready, then_args: Vec::new(), else_target: missing, else_args: Vec::new() });
    ctx.builder.position_at_end(ready);
    ctx.begin_quiet_undefined_reads(arg);
    let value = lower_expr(ctx, arg);
    ctx.end_quiet_undefined_reads();
    store_value_into_temp(ctx, &temp, snapshot_type.clone(), value, arg.span);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(missing);
    let null = lower_null(ctx, arg);
    store_value_into_temp(ctx, &temp, snapshot_type, null, arg.span);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    Some(take_owned_temp(ctx, &temp, arg.span))
}
