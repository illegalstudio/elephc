//! Purpose:
//! Lowers the PHP 8.5 pipe operator (`value |> callable`) into the equivalent direct call.
//! Delegates to the most specific existing call emitter based on the static shape of the RHS.
//!
//! Called from:
//! - `crate::codegen::expr::emit_expr()` via `super::calls::emit_pipe`.
//!
//! Key details:
//! - The synthesized call carries the pipe operator's span so diagnostics point at `|>`.
//! - Argument planning, ABI materialization, and ownership are handled by the downstream emitter; this module does not duplicate that logic.

use crate::parser::ast::{CallableTarget, Expr, ExprKind};
use crate::span::Span;
use crate::types::PhpType;

use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;

pub(super) fn emit_pipe(
    value: &Expr,
    callable: &Expr,
    span: Span,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let temp_name = super::pipe_value_temp_name(span);
    crate::codegen::stmt::emit_assign_stmt(&temp_name, value, emitter, ctx, data);
    let temp_value = Expr::new(ExprKind::Variable(temp_name), value.span);
    let synth_args = vec![temp_value];
    let synthetic = match &callable.kind {
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => Expr::new(
            ExprKind::FunctionCall {
                name: name.clone(),
                args: synth_args,
            },
            span,
        ),
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            Expr::new(
                ExprKind::StaticMethodCall {
                    receiver: receiver.clone(),
                    method: method.clone(),
                    args: synth_args,
                },
                span,
            )
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => Expr::new(
            ExprKind::MethodCall {
                object: object.clone(),
                method: method.clone(),
                args: synth_args,
            },
            span,
        ),
        ExprKind::Variable(var) => Expr::new(
            ExprKind::ClosureCall {
                var: var.clone(),
                args: synth_args,
            },
            span,
        ),
        _ => Expr::new(
            ExprKind::ExprCall {
                callee: Box::new(callable.clone()),
                args: synth_args,
            },
            span,
        ),
    };
    super::super::emit_expr(&synthetic, emitter, ctx, data)
}
