//! Purpose:
//! While, do-while, and for-loop CFG lowering.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Lowers a `while` loop.
pub(super) fn lower_while(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    body: &[Stmt],
    loop_span: Span,
) {
    apply_loop_storage_contracts(ctx, loop_span, Some(condition.span));
    let header = ctx.builder.create_named_block("while.cond", Vec::new());
    let body_block = ctx.builder.create_named_block("while.body", Vec::new());
    let exit = ctx.builder.create_named_block("while.exit", Vec::new());
    branch_to(ctx, header);

    ctx.builder.position_at_end(header);
    // The back edge re-enters the condition, so a store inside it — `while (($s = f()) !== '')`,
    // the idiomatic read loop — overwrites the previous iteration's value and must release it.
    ctx.enter_loop_back_edge_expression();
    let cond = lower_expr(ctx, condition);
    let cond = ctx.truthy_consuming(cond, Some(condition.span));
    ctx.leave_loop_back_edge_expression();
    ctx.builder.terminate(Terminator::CondBr {
        cond: cond.value,
        then_target: body_block,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.clear_static_callable_locals();
    ctx.builder.position_at_end(body_block);
    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: header,
        cleanup: None,
        source_pin: None,
    });
    lower_block(ctx, body);
    ctx.loop_stack.pop();
    branch_to(ctx, header);
    ctx.builder.position_at_end(exit);
    ctx.clear_static_callable_locals();
}

/// Lowers a `do while` loop.
pub(super) fn lower_do_while(
    ctx: &mut LoweringContext<'_, '_>,
    body: &[Stmt],
    condition: &Expr,
    loop_span: Span,
) {
    apply_loop_storage_contracts(ctx, loop_span, Some(condition.span));
    let body_block = ctx.builder.create_named_block("do.body", Vec::new());
    let cond_block = ctx.builder.create_named_block("do.cond", Vec::new());
    let exit = ctx.builder.create_named_block("do.exit", Vec::new());
    branch_to(ctx, body_block);

    ctx.builder.position_at_end(body_block);
    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: cond_block,
        cleanup: None,
        source_pin: None,
    });
    lower_block(ctx, body);
    ctx.loop_stack.pop();
    branch_to(ctx, cond_block);

    ctx.builder.position_at_end(cond_block);
    ctx.enter_loop_back_edge_expression();
    let cond = lower_expr(ctx, condition);
    let cond = ctx.truthy_consuming(cond, Some(condition.span));
    ctx.leave_loop_back_edge_expression();
    ctx.builder.terminate(Terminator::CondBr {
        cond: cond.value,
        then_target: body_block,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });
    ctx.clear_static_callable_locals();
    ctx.builder.position_at_end(exit);
    ctx.clear_static_callable_locals();
}

/// Lowers a `for` loop after establishing its loop-carried storage representation.
///
/// The fixed-point region starts below the initializer because an array created by the initializer
/// does not exist at the statement entry and therefore cannot be discovered by the outer
/// statement-level representation scan.
pub(super) fn lower_for(
    ctx: &mut LoweringContext<'_, '_>,
    init: Option<&Stmt>,
    condition: Option<&Expr>,
    update: Option<&Stmt>,
    body: &[Stmt],
    loop_span: Span,
) {
    if let Some(init) = init {
        lower_stmt(ctx, init);
    }
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    let contract_span = condition
        .map(|c| c.span)
        .or_else(|| body.first().map(|s| s.span));
    apply_loop_storage_contracts(ctx, loop_span, contract_span);

    repr_fixpoint::lower_for_body_at_type_fixpoint(
        ctx,
        loop_span,
        condition,
        update,
        body,
        |ctx| lower_for_once(ctx, condition, update, body),
    );
}

/// Emits the control-flow graph, body, and update of a `for` loop exactly once.
fn lower_for_once(
    ctx: &mut LoweringContext<'_, '_>,
    condition: Option<&Expr>,
    update: Option<&Stmt>,
    body: &[Stmt],
) {
    let header = ctx.builder.create_named_block("for.cond", Vec::new());
    let body_block = ctx.builder.create_named_block("for.body", Vec::new());
    let update_block = ctx.builder.create_named_block("for.update", Vec::new());
    let exit = ctx.builder.create_named_block("for.exit", Vec::new());
    branch_to(ctx, header);

    ctx.builder.position_at_end(header);
    let cond = if let Some(condition) = condition {
        ctx.enter_loop_back_edge_expression();
        let cond = lower_expr(ctx, condition);
        let cond = ctx.truthy_consuming(cond, Some(condition.span));
        ctx.leave_loop_back_edge_expression();
        cond
    } else {
        emit_const_bool(ctx, true, None)
    };
    ctx.builder.terminate(Terminator::CondBr {
        cond: cond.value,
        then_target: body_block,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.clear_static_callable_locals();
    ctx.builder.position_at_end(body_block);
    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: update_block,
        cleanup: None,
        source_pin: None,
    });
    lower_block(ctx, body);
    ctx.loop_stack.pop();
    branch_to(ctx, update_block);

    ctx.builder.position_at_end(update_block);
    if let Some(update) = update {
        ctx.enter_loop_back_edge_expression();
        lower_stmt(ctx, update);
        ctx.leave_loop_back_edge_expression();
    }
    branch_to(ctx, header);
    ctx.builder.position_at_end(exit);
    ctx.clear_static_callable_locals();
}
