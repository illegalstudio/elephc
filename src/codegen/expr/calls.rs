//! Purpose:
//! Dispatches function-like expression calls including direct, indirect, closure, method-adjacent, and first-class forms.
//! Coordinates call signatures, argument lowering, and result typing for expression consumers.
//!
//! Called from:
//! - `crate::codegen::expr::emit_expr()`
//!
//! Key details:
//! - Argument evaluation must preserve PHP source order before ABI materialization happens in call-argument helpers.

pub(crate) mod args;
mod closure;
mod first_class;
mod function;
mod indirect;
mod pipe;

use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::Expr;
use crate::parser::ast::TypeExpr;
use crate::span::Span;
use crate::types::PhpType;

/// Emits a direct or namespaced function call by name.
pub(super) fn emit_function_call(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    function::emit_function_call(name, args, emitter, ctx, data)
}

/// Emits a closure (anonymous function) definition with captures.
pub(super) fn emit_closure(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: &Option<String>,
    return_type: &Option<TypeExpr>,
    body: &[crate::parser::ast::Stmt],
    captures: &[String],
    capture_refs: &[String],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    closure::emit_closure(
        params,
        variadic,
        return_type,
        body,
        captures,
        capture_refs,
        emitter,
        ctx,
        data,
    )
}

/// Emits a closure call expression (e.g., `$closure(...)`).
pub(super) fn emit_closure_call(
    var: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    closure::emit_closure_call(var, args, emitter, ctx, data)
}

/// Emits an indirect call where the callee is a runtime-loaded expression.
pub(super) fn emit_loaded_expr_call(
    callee: &Expr,
    args: &[Expr],
    loaded_callee_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    indirect::emit_loaded_expr_call(callee, args, loaded_callee_ty, emitter, ctx, data)
}

/// Emits a first-class callable expression (e.g., `$fn(...)()`).
pub(super) fn emit_first_class_callable(
    target: &crate::parser::ast::CallableTarget,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    first_class::emit_first_class_callable(target, emitter, ctx, data)
}

/// Returns the function signature for a first-class callable target.
pub(crate) fn first_class_callable_sig(
    target: &crate::parser::ast::CallableTarget,
    ctx: &Context,
) -> Option<crate::types::FunctionSig> {
    first_class::first_class_callable_sig(target, ctx)
}

/// Generates a unique temp name for the receiver of an inline first-class callable.
pub(crate) fn first_class_method_receiver_temp_name(span: Span) -> String {
    first_class::method_receiver_temp_name(span)
}

/// Generates a unique temp name for the pipe value in an arrow-function pipeline.
pub(crate) fn pipe_value_temp_name(span: Span) -> String {
    format!("__elephc_pipe_value_{}_{}", span.line, span.col)
}

/// Emits a pipe expression (first-class callable pipeline).
pub(super) fn emit_pipe(
    value: &Expr,
    callable: &Expr,
    span: Span,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    pipe::emit_pipe(value, callable, span, emitter, ctx, data)
}
