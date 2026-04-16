mod catches;
mod finally;
mod handlers;

use crate::codegen::abi;
use crate::codegen::context::{Context, FinallyContext};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{CatchClause, Expr, Stmt};

pub(super) use finally::{emit_branch_through_finally, emit_return_through_finally};

const PENDING_RETURN: u64 = 1;
const PENDING_BRANCH: u64 = 2;
const PENDING_RETHROW: u64 = 3;

pub(super) fn emit_throw_stmt(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment("throw");
    let thrown_ty = emit_expr(expr, emitter, ctx, data);
    super::super::helpers::retain_borrowed_heap_result(emitter, expr, &thrown_ty);
    abi::emit_store_reg_to_symbol(emitter, abi::int_result_reg(emitter), "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_throw_current");                           // unwind to the nearest active exception handler
}

pub(super) fn emit_try_stmt(
    try_body: &[Stmt],
    catches: &[CatchClause],
    finally_body: &Option<Vec<Stmt>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let handler_offset = ctx.next_try_slot();
    let handler_resume = ctx.next_label("try_resume");
    let normal_done = ctx.next_label("try_normal_done");
    let end_label = ctx.next_label("try_end");
    let finally_label = finally_body.as_ref().map(|_| ctx.next_label("try_finally"));
    let catch_end_label = ctx.next_label("try_catch_end");

    emitter.blank();
    emitter.comment("try");
    handlers::emit_try_handler_push(emitter, ctx, handler_offset);
    handlers::emit_handler_jmpbuf_address(emitter, handler_offset, abi::int_arg_reg_name(emitter.target, 0));
    emitter.bl_c("setjmp");                                                        // snapshot the current stack/register state for this try handler
    abi::emit_branch_if_int_result_nonzero(emitter, &handler_resume);              // resume at catch dispatch after a longjmp into this handler

    if let Some(label) = &finally_label {
        ctx.finally_stack.push(FinallyContext {
            entry_label: label.clone(),
        });
    }

    for stmt in try_body {
        super::super::emit_stmt(stmt, emitter, ctx, data);
    }

    if finally_label.is_some() {
        ctx.finally_stack.pop();
    }

    handlers::emit_try_handler_pop(emitter, handler_offset);
    if let Some(label) = &finally_label {
        abi::emit_jump(emitter, label);                                            // run finally after the try body completes normally
    } else {
        abi::emit_jump(emitter, &end_label);                                       // skip catch dispatch after a normal try-body completion
    }

    emitter.label(&handler_resume);
    handlers::emit_try_handler_pop(emitter, handler_offset);
    catches::emit_catch_dispatch(
        catches,
        finally_label.as_deref(),
        &catch_end_label,
        emitter,
        ctx,
        data,
    );

    emitter.label(&catch_end_label);
    abi::emit_jump(emitter, &end_label);                                           // join point after try/catch when no finally is present

    if let Some(label) = finally_label {
        let dispatch_return = ctx.next_label("finally_dispatch_return");
        let dispatch_branch = ctx.next_label("finally_dispatch_branch");
        let dispatch_rethrow = ctx.next_label("finally_dispatch_rethrow");
        emitter.label(&label);
        if let Some(body) = finally_body {
            for stmt in body {
                super::super::emit_stmt(stmt, emitter, ctx, data);
            }
        }
        finally::emit_finally_dispatch(
            emitter,
            ctx,
            &normal_done,
            &dispatch_return,
            &dispatch_branch,
            &dispatch_rethrow,
        );
        emitter.label(&normal_done);
    }

    emitter.label(&end_label);
}
