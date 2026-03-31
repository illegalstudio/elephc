use crate::codegen::context::{Context, LoopLabels};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_result_to_type, emit_expr};
use crate::parser::ast::{Expr, Stmt};

pub(super) fn emit_do_while_stmt(
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
        super::super::emit_stmt(s, emitter, ctx, data);
    }
    ctx.loop_stack.pop();

    emitter.label(&loop_cond);
    let cond_ty = emit_expr(condition, emitter, ctx, data);
    crate::codegen::expr::coerce_to_truthiness(emitter, ctx, &cond_ty);
    emitter.instruction("cmp x0, #0");                                          // test if do-while condition is zero (falsy)
    emitter.instruction(&format!("b.ne {}", loop_start));                       // loop back to start if condition is nonzero (truthy)
    emitter.label(&loop_end);
}

pub(super) fn emit_while_stmt(
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
    emitter.instruction("cmp x0, #0");                                          // test if while condition is zero (falsy)
    emitter.instruction(&format!("b.eq {}", loop_end));                         // exit loop if condition is false

    ctx.loop_stack.push(LoopLabels {
        continue_label: loop_start.clone(),
        break_label: loop_end.clone(),
        sp_adjust: 0,
    });
    for s in body {
        super::super::emit_stmt(s, emitter, ctx, data);
    }
    ctx.loop_stack.pop();

    emitter.instruction(&format!("b {}", loop_start));                          // unconditional branch back to loop start
    emitter.label(&loop_end);
}

pub(super) fn emit_for_stmt(
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
        super::super::emit_stmt(s, emitter, ctx, data);
    }

    emitter.label(&loop_start);

    if let Some(cond) = condition {
        let cond_ty = emit_expr(cond, emitter, ctx, data);
        crate::codegen::expr::coerce_to_truthiness(emitter, ctx, &cond_ty);
        emitter.instruction("cmp x0, #0");                                      // test if for-loop condition is zero (falsy)
        emitter.instruction(&format!("b.eq {}", loop_end));                     // exit loop if condition is false
    }

    ctx.loop_stack.push(LoopLabels {
        continue_label: loop_continue.clone(),
        break_label: loop_end.clone(),
        sp_adjust: 0,
    });
    for s in body {
        super::super::emit_stmt(s, emitter, ctx, data);
    }
    ctx.loop_stack.pop();

    emitter.label(&loop_continue);
    if let Some(s) = update {
        super::super::emit_stmt(s, emitter, ctx, data);
    }
    emitter.instruction(&format!("b {}", loop_start));                          // unconditional branch back to loop start
    emitter.label(&loop_end);
}

pub(super) fn emit_break_stmt(emitter: &mut Emitter, ctx: &Context) {
    let labels = ctx
        .loop_stack
        .last()
        .expect("codegen bug: break statement outside loop (should have been caught by type checker)");
    if !ctx.finally_stack.is_empty() {
        super::emit_branch_through_finally(emitter, ctx, &labels.break_label);
    } else {
        emitter.instruction(&format!("b {}", labels.break_label));              // unconditional branch to loop exit label
    }
}

pub(super) fn emit_return_stmt(
    expr: &Option<Expr>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment("return");
    if let Some(e) = expr {
        let ty = emit_expr(e, emitter, ctx, data);
        let target_ty = ctx.return_type.clone();
        coerce_result_to_type(emitter, ctx, data, &ty, &target_ty);
        if ty == target_ty {
            super::super::retain_borrowed_heap_result(emitter, e, &ty);
        }
    }
    if let Some(label) = &ctx.return_label {
        let sp_total: usize = ctx.loop_stack.iter().map(|l| l.sp_adjust).sum();
        if sp_total > 0 {
            emitter.instruction(&format!("add sp, sp, #{}", sp_total));         // pop switch subjects before returning
        }
        if !ctx.finally_stack.is_empty() {
            super::exceptions::emit_return_through_finally(emitter, ctx);
        } else {
            emitter.instruction(&format!("b {}", label));                       // branch to function epilogue for stack cleanup and ret
        }
    }
}

pub(super) fn emit_continue_stmt(emitter: &mut Emitter, ctx: &Context) {
    let labels = ctx
        .loop_stack
        .last()
        .expect("codegen bug: continue statement outside loop (should have been caught by type checker)");
    if !ctx.finally_stack.is_empty() {
        super::emit_branch_through_finally(emitter, ctx, &labels.continue_label);
    } else {
        emitter.instruction(&format!("b {}", labels.continue_label));           // unconditional branch to loop continue label
    }
}
