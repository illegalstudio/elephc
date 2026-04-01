pub(crate) mod args;
mod closure;
mod first_class;
mod function;
mod indirect;

use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::Expr;
use crate::parser::ast::TypeExpr;
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
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: &Option<String>,
    body: &[crate::parser::ast::Stmt],
    captures: &[String],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    closure::emit_closure(params, variadic, body, captures, emitter, ctx, data)
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

pub(super) fn emit_first_class_callable(
    target: &crate::parser::ast::CallableTarget,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    first_class::emit_first_class_callable(target, emitter, ctx, data)
}

pub(super) fn first_class_callable_sig(
    target: &crate::parser::ast::CallableTarget,
    ctx: &Context,
) -> Option<crate::types::FunctionSig> {
    first_class::first_class_callable_sig(target, ctx)
}
