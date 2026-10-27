//! Purpose:
//! While, do-while, and for-loop CFG lowering.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;
use crate::ir_lower::loop_closures::{begin_loop_closure_facts, retain_loop_body_facts};

/// Lowers a `while` loop.
pub(super) fn lower_while(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    body: &[Stmt],
    loop_span: Span,
) {
    apply_loop_storage_contracts(ctx, loop_span, Some(condition.span));
    let closure_facts = begin_loop_closure_facts(ctx, Some(condition), None, body, true);
    let header = ctx.builder.create_named_block("while.cond", Vec::new());
    let body_block = ctx.builder.create_named_block("while.body", Vec::new());
    let exit = ctx.builder.create_named_block("while.exit", Vec::new());
    branch_to(ctx, header);

    // Conditions execute on every backedge, so their first lexical store can
    // overwrite a previous iteration's owner just like a store in the body.
    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: header,
        cleanup: None,
        source_owner: None,
        source_pin: None,
        iterator_owner: None,
        iterator_cleanup: None,
        receiver_pin: None,
    });
    ctx.builder.position_at_end(header);
    lower_condition(ctx, condition, body_block, exit);

    retain_loop_body_facts(ctx, &closure_facts);
    ctx.builder.position_at_end(body_block);
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
    let _ = begin_loop_closure_facts(ctx, Some(condition), None, body, false);
    let body_block = ctx.builder.create_named_block("do.body", Vec::new());
    let cond_block = ctx.builder.create_named_block("do.cond", Vec::new());
    let exit = ctx.builder.create_named_block("do.exit", Vec::new());
    branch_to(ctx, body_block);

    ctx.builder.position_at_end(body_block);
    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: cond_block,
        cleanup: None,
        source_owner: None,
        source_pin: None,
        iterator_owner: None,
        iterator_cleanup: None,
        receiver_pin: None,
    });
    lower_block(ctx, body);
    branch_to(ctx, cond_block);

    ctx.builder.position_at_end(cond_block);
    lower_condition(ctx, condition, body_block, exit);
    ctx.loop_stack.pop();
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
    let closure_facts = begin_loop_closure_facts(ctx, condition, update, body, true);
    let header = ctx.builder.create_named_block("for.cond", Vec::new());
    let body_block = ctx.builder.create_named_block("for.body", Vec::new());
    let update_block = ctx.builder.create_named_block("for.update", Vec::new());
    let exit = ctx.builder.create_named_block("for.exit", Vec::new());
    branch_to(ctx, header);

    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: update_block,
        cleanup: None,
        source_owner: None,
        source_pin: None,
        iterator_owner: None,
        iterator_cleanup: None,
        receiver_pin: None,
    });
    ctx.builder.position_at_end(header);
    if let Some(condition) = condition {
        lower_condition(ctx, condition, body_block, exit);
    } else {
        let cond = emit_const_bool(ctx, true, None);
        ctx.builder.terminate(Terminator::CondBr {
            cond: cond.value,
            then_target: body_block,
            then_args: Vec::new(),
            else_target: exit,
            else_args: Vec::new(),
        });
    }

    retain_loop_body_facts(ctx, &closure_facts);
    ctx.builder.position_at_end(body_block);
    lower_block(ctx, body);
    branch_to(ctx, update_block);

    ctx.builder.position_at_end(update_block);
    if let Some(update) = update {
        lower_stmt(ctx, update);
    }
    ctx.loop_stack.pop();
    branch_to(ctx, header);
    ctx.builder.position_at_end(exit);
    ctx.clear_static_callable_locals();
}
