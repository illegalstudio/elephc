use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::super::functions;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub(super) fn emit_assignment_expr(
    target: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let ExprKind::Variable(name) = &target.kind else {
        emitter.comment("WARNING: assignment expression target is not supported in codegen");
        return PhpType::Int;
    };

    super::super::stmt::emit_assign_stmt(name, value, emitter, ctx, data);
    ctx.variables
        .get(name)
        .map(|var| var.ty.clone())
        .unwrap_or_else(|| functions::infer_contextual_type(value, ctx))
}
