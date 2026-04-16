mod if_stmt;
mod switch_stmt;

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
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
    if_stmt::emit_if_stmt(
        condition,
        then_body,
        elseif_clauses,
        else_body,
        emitter,
        ctx,
        data,
    )
}

pub(super) fn emit_switch_stmt(
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    switch_stmt::emit_switch_stmt(subject, cases, default, emitter, ctx, data)
}
