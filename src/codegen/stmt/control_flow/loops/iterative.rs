//! Purpose:
//! Lowers while, do-while, and for loop label emission.
//! Works with loop labels stored in codegen context during nested body emission.
//!
//! Called from:
//! - `crate::codegen::stmt::control_flow::loops`
//!
//! Key details:
//! - Loop exits must jump to the correct depth while preserving cleanup for skipped constructs.

use crate::codegen::context::{Context, LoopLabels};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{Expr, Stmt};

/// Emits a do-while loop.
///
/// # Inputs
/// - `body`: statements executed each iteration before the condition check
/// - `condition`: expression evaluated after each body execution; result coerced to truthiness
///
/// # Side effects
/// - Allocates three labels: `dowhile_start`, `dowhile_cond`, `dowhile_end`
/// - Pushes loop labels to `ctx.loop_stack` with `continue→dowhile_cond`, `break→dowhile_end`
/// - Pops loop labels after body emission completes
/// - Mutates `emitter` to emit labels and branch instructions
/// - `condition` result is left in the current result register after truthiness coercion
///
/// # PHP semantics
/// - Body executes at least once before condition is evaluated
pub(crate) fn emit_do_while_stmt(
    body: &[Stmt],
    condition: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let loop_start = ctx.next_label("dowhile_start");
    let loop_end = ctx.next_label("dowhile_end");
    let loop_cond = ctx.next_label("dowhile_cond");

    emitter.blank();
    emitter.comment("do...while");
    emitter.label(&loop_start);

    ctx.loop_stack.push(LoopLabels {
        continue_label: loop_cond.clone(),
        break_label: loop_end.clone(),
        sp_adjust: 0,
    });
    for s in body {
        super::super::super::emit_stmt(s, emitter, ctx, data);
    }
    ctx.loop_stack.pop();

    emitter.label(&loop_cond);
    let cond_ty = emit_expr(condition, emitter, ctx, data);
    crate::codegen::expr::coerce_to_truthiness(emitter, ctx, &cond_ty);
    crate::codegen::abi::emit_branch_if_int_result_nonzero(emitter, &loop_start);
    emitter.label(&loop_end);
}

/// Emits a while loop.
///
/// # Inputs
/// - `condition`: expression evaluated before each iteration; result coerced to truthiness
/// - `body`: statements executed each iteration while condition is non-zero
///
/// # Side effects
/// - Allocates two labels: `while_start`, `while_end`
/// - Pushes loop labels to `ctx.loop_stack` with `continue→while_start`, `break→while_end`
/// - Pops loop labels after body emission completes
/// - Mutates `emitter` to emit labels and branch instructions
/// - `condition` result is left in the current result register after truthiness coercion
///
/// # PHP semantics
/// - Condition is evaluated before the first iteration; body never executes if condition is initially zero
pub(crate) fn emit_while_stmt(
    condition: &Expr,
    body: &[Stmt],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let loop_start = ctx.next_label("while_start");
    let loop_end = ctx.next_label("while_end");

    emitter.blank();
    emitter.comment("while");
    emitter.label(&loop_start);
    let cond_ty = emit_expr(condition, emitter, ctx, data);
    crate::codegen::expr::coerce_to_truthiness(emitter, ctx, &cond_ty);
    crate::codegen::abi::emit_branch_if_int_result_zero(emitter, &loop_end);

    ctx.loop_stack.push(LoopLabels {
        continue_label: loop_start.clone(),
        break_label: loop_end.clone(),
        sp_adjust: 0,
    });
    for s in body {
        super::super::super::emit_stmt(s, emitter, ctx, data);
    }
    ctx.loop_stack.pop();

    crate::codegen::abi::emit_jump(emitter, &loop_start);                       // unconditional branch back to loop start
    emitter.label(&loop_end);
}

/// Emits a for loop.
///
/// # Inputs
/// - `init`: statement executed once before the loop (typically variable assignment); may be None
/// - `condition`: expression evaluated before each iteration; skipped if None; result coerced to truthiness
/// - `update`: statement executed after each iteration body; may be None
/// - `body`: statements executed each iteration while condition is non-zero
///
/// # Side effects
/// - Allocates three labels: `for_start`, `for_cont`, `for_end`
/// - Pushes loop labels to `ctx.loop_stack` with `continue→for_cont`, `break→for_end`
/// - Pops loop labels after body emission completes
/// - Mutates `emitter` to emit labels and branch instructions
/// - `condition` result is left in the current result register after truthiness coercion
///
/// # PHP semantics
/// - `init` executes once before any condition check
/// - If `condition` is None, loop runs indefinitely (no PHP exit mechanism here; caller must arrange alternative control flow)
/// - `continue` jumps to `update` (not to `condition`)
pub(crate) fn emit_for_stmt(
    init: &Option<Box<Stmt>>,
    condition: &Option<Expr>,
    update: &Option<Box<Stmt>>,
    body: &[Stmt],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let loop_start = ctx.next_label("for_start");
    let loop_continue = ctx.next_label("for_cont");
    let loop_end = ctx.next_label("for_end");

    emitter.blank();
    emitter.comment("for");

    if let Some(s) = init {
        super::super::super::emit_stmt(s, emitter, ctx, data);
    }

    emitter.label(&loop_start);

    if let Some(cond) = condition {
        let cond_ty = emit_expr(cond, emitter, ctx, data);
        crate::codegen::expr::coerce_to_truthiness(emitter, ctx, &cond_ty);
        crate::codegen::abi::emit_branch_if_int_result_zero(emitter, &loop_end);
    }

    ctx.loop_stack.push(LoopLabels {
        continue_label: loop_continue.clone(),
        break_label: loop_end.clone(),
        sp_adjust: 0,
    });
    for s in body {
        super::super::super::emit_stmt(s, emitter, ctx, data);
    }
    ctx.loop_stack.pop();

    emitter.label(&loop_continue);
    if let Some(s) = update {
        super::super::super::emit_stmt(s, emitter, ctx, data);
    }
    crate::codegen::abi::emit_jump(emitter, &loop_start);                       // unconditional branch back to loop start
    emitter.label(&loop_end);
}
