mod branching;
mod exceptions;
mod foreach;
mod loops;

use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use crate::parser::ast::{Expr, Stmt};
use crate::parser::ast::CatchClause;

pub(super) fn emit_if_stmt(
    condition: &Expr,
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: &Option<Vec<Stmt>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    branching::emit_if_stmt(
        condition,
        then_body,
        elseif_clauses,
        else_body,
        emitter,
        ctx,
        data,
    )
}

pub(super) fn emit_foreach_stmt(
    array: &Expr,
    key_var: &Option<String>,
    value_var: &str,
    body: &[Stmt],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    foreach::emit_foreach_stmt(array, key_var, value_var, body, emitter, ctx, data)
}

pub(super) fn emit_do_while_stmt(
    body: &[Stmt],
    condition: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    loops::emit_do_while_stmt(body, condition, emitter, ctx, data)
}

pub(super) fn emit_while_stmt(
    condition: &Expr,
    body: &[Stmt],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    loops::emit_while_stmt(condition, body, emitter, ctx, data)
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
    loops::emit_for_stmt(init, condition, update, body, emitter, ctx, data)
}

pub(super) fn emit_break_stmt(emitter: &mut Emitter, ctx: &Context) {
    loops::emit_break_stmt(emitter, ctx)
}

pub(super) fn emit_branch_through_finally(emitter: &mut Emitter, ctx: &Context, target_label: &str) {
    exceptions::emit_branch_through_finally(emitter, ctx, target_label)
}

pub(super) fn emit_return_stmt(
    expr: &Option<Expr>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    loops::emit_return_stmt(expr, emitter, ctx, data)
}

pub(super) fn emit_throw_stmt(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    exceptions::emit_throw_stmt(expr, emitter, ctx, data)
}

pub(super) fn emit_try_stmt(
    try_body: &[Stmt],
    catches: &[CatchClause],
    finally_body: &Option<Vec<Stmt>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    exceptions::emit_try_stmt(try_body, catches, finally_body, emitter, ctx, data)
}

pub(super) fn emit_continue_stmt(emitter: &mut Emitter, ctx: &Context) {
    loops::emit_continue_stmt(emitter, ctx)
}

pub(super) fn emit_switch_stmt(
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    branching::emit_switch_stmt(subject, cases, default, emitter, ctx, data)
}
