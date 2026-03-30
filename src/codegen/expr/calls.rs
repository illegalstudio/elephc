mod args;
mod closure;
mod function;
mod indirect;

use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::Expr;
use crate::types::PhpType;

pub(super) fn emit_function_call(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    function::emit_function_call(name, args, emitter, ctx, data)
}

pub(super) fn emit_closure(
    params: &[(String, Option<Expr>, bool)],
    body: &[crate::parser::ast::Stmt],
    captures: &[String],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    closure::emit_closure(params, body, captures, emitter, ctx, data)
}

pub(super) fn emit_closure_call(
    var: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    closure::emit_closure_call(var, args, emitter, ctx, data)
}

pub(super) fn emit_expr_call(
    callee: &Expr,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    indirect::emit_expr_call(callee, args, emitter, ctx, data)
}
