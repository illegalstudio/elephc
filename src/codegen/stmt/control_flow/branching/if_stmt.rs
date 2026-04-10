use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::codegen::stmt::emit_stmt;
use crate::parser::ast::{Expr, Stmt};

pub(super) fn emit_if_stmt(
    condition: &Expr,
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: &Option<Vec<Stmt>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let end_label = ctx.next_label("if_end");

    emitter.blank();
    emitter.comment("if");
    let cond_ty = emit_expr(condition, emitter, ctx, data);
    crate::codegen::expr::coerce_to_truthiness(emitter, ctx, &cond_ty);
    let mut next_label = ctx.next_label("if_else");
    crate::codegen::abi::emit_branch_if_int_result_zero(emitter, &next_label);

    for s in then_body {
        emit_stmt(s, emitter, ctx, data);
    }
    abi::emit_jump(emitter, &end_label);                                        // unconditional jump past all else/elseif branches

    for (cond, body) in elseif_clauses {
        emitter.label(&next_label);
        emitter.comment("elseif");
        let cond_ty = emit_expr(cond, emitter, ctx, data);
        crate::codegen::expr::coerce_to_truthiness(emitter, ctx, &cond_ty);
        next_label = ctx.next_label("if_else");
        crate::codegen::abi::emit_branch_if_int_result_zero(emitter, &next_label);

        for s in body {
            emit_stmt(s, emitter, ctx, data);
        }
        abi::emit_jump(emitter, &end_label);                                    // unconditional jump past remaining branches
    }

    emitter.label(&next_label);
    if let Some(body) = else_body {
        emitter.comment("else");
        for s in body {
            emit_stmt(s, emitter, ctx, data);
        }
    }

    emitter.label(&end_label);
}
