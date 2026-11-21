//! Purpose:
//! Selects the unified task execution domain from a runtime Execution enum.
//!
//! Called from:
//! - `super::method_calls::lower_method_call()` for checked TaskGroup submissions.
//!
//! Key details:
//! - Evaluates receiver and source arguments once using unwind-visible PHP-local owners.
//! - Shared call plans normalize arguments before the selected executor submits/registers work.
//! - Parallel safety is checked before either branch reaches EIR lowering.

use super::*;

/// Dispatches source-order snapshots without re-entering TaskGroup method interception.
pub(super) fn lower_runtime_task_submission(
    ctx: &mut LoweringContext<'_, '_>,
    hub: LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let hub_type = ctx.builder.value_php_type(hub.value);
    let signature = method_signature(ctx, hub.value, method)
        .expect("checked TaskGroup submission has a method signature");
    let result_type = method_call_result_type(ctx, hub.value, method, Op::MethodCall, expr);
    let staging = prepublish_call_result(ctx, &result_type, expr.span);
    // Branches and retirement read this slot more than once; an OwnedTemp load
    // would consume its owner rather than expose a reusable receiver alias.
    let hub_temp = ctx.declare_synthetic_php_local(hub_type.clone());
    ctx.store_local(&hub_temp, hub, hub_type, Some(expr.span));
    let hub_slot = ctx.local_slots[&hub_temp];
    register_owned_call_operand(ctx, hub_slot, expr.span);
    let snapshots = super::task_args::snapshot_task_arguments(ctx, &signature, args, expr.span);
    let result = if let Some(parallel) = snapshots.execution {
        submit_snapshot(ctx, &hub_temp, method, &snapshots.arguments, parallel, expr)
    } else {
        let condition = crate::synthetic_class::e_binop(
            snapshots.arguments[0].clone(), BinOp::StrictEq,
            crate::synthetic_class::e_class_const("Elephc\\Task\\Execution", "Parallel"),
        );
        let condition = lower_expr(ctx, &condition);
        let result_temp = ctx.declare_owned_hidden_temp(result_type.clone());
        let split = ctx.initialized_slots_snapshot();
        let parallel = ctx.builder.create_named_block("task.parallel", Vec::new());
        let asynchronous = ctx.builder.create_named_block("task.async", Vec::new());
        let merge = ctx.builder.create_named_block("task.merge", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: condition.value, then_target: parallel, then_args: Vec::new(),
            else_target: asynchronous, else_args: Vec::new(),
        });
        ctx.builder.position_at_end(parallel);
        ctx.restore_initialized_slots(split.clone());
        let value = submit_snapshot(ctx, &hub_temp, method, &snapshots.arguments, true, expr);
        store_value_into_temp(ctx, &result_temp, result_type.clone(), value, expr.span);
        let parallel_reachable = !ctx.builder.insertion_block_is_terminated();
        let parallel_initialized = ctx.initialized_slots_snapshot();
        branch_to(ctx, merge);
        ctx.builder.position_at_end(asynchronous);
        ctx.restore_initialized_slots(split.clone());
        let value = submit_snapshot(ctx, &hub_temp, method, &snapshots.arguments, false, expr);
        store_value_into_temp(ctx, &result_temp, result_type, value, expr.span);
        let async_reachable = !ctx.builder.insertion_block_is_terminated();
        let async_initialized = ctx.initialized_slots_snapshot();
        branch_to(ctx, merge);
        ctx.builder.position_at_end(merge);
        ctx.restore_initialized_slots(merge_initialized_slots_for_expr(
            &split, parallel_initialized, parallel_reachable, async_initialized, async_reachable,
        ));
        take_owned_temp(ctx, &result_temp, expr.span)
    };
    stage_call_result(ctx, staging.as_ref(), result, expr.span);
    for (slot, rooted) in snapshots.owners.into_iter().rev() {
        if rooted {
            retire_owned_call_operand(ctx, slot, expr.span);
        } else {
            ctx.emit_void(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(slot)),
                Op::ReleaseLocalSlot.default_effects(), Some(expr.span));
        }
    }
    retire_owned_call_operand(ctx, hub_slot, expr.span);
    take_prepublished_call_result(ctx, staging, result, expr.span)
}

/// Uses the ordinary checked method path for Async and the checked worker path for Parallel.
fn submit_snapshot(
    ctx: &mut LoweringContext<'_, '_>, hub: &str, method: &str, args: &[Expr],
    parallel: bool, expr: &Expr,
) -> LoweredValue {
    let mut args = args.to_vec();
    args[0] = crate::synthetic_class::e_class_const("Elephc\\Task\\Execution",
        if parallel { "Parallel" } else { "Async" });
    let hub = ctx.load_local(hub, Some(expr.span));
    if !parallel {
        return lower_method_call_with_receiver(ctx, hub, method, &args, Op::MethodCall, expr);
    }
    let result = if php_symbol_key(method) == "every" {
        lower_parallel_every(ctx, hub, &args, expr)
    } else {
        lower_unified_parallel_spawn(ctx, hub, &args, expr)
    };
    release_owning_receiver_temporary(ctx, hub, expr.span);
    result
}
