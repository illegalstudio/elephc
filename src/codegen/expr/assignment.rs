use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
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
        return emit_non_local_assignment_expr(target, value, emitter, ctx, data);
    };

    super::super::stmt::emit_assign_stmt(name, value, emitter, ctx, data);
    super::variables::emit_variable(name, emitter, ctx)
}

pub(super) fn emit_non_local_assignment_expr(
    target: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    match &target.kind {
        ExprKind::ArrayAccess { array, index } => match &array.kind {
            ExprKind::Variable(array) => {
                super::super::stmt::emit_array_assign_stmt(array, index, value, emitter, ctx, data);
            }
            ExprKind::PropertyAccess { object, property } => {
                super::super::stmt::emit_property_array_assign_stmt(
                    object, property, index, value, emitter, ctx, data,
                );
            }
            ExprKind::StaticPropertyAccess { receiver, property } => {
                super::super::stmt::emit_static_property_array_assign_stmt(
                    receiver, property, index, value, emitter, ctx, data,
                );
            }
            _ => {
                emitter.comment("WARNING: assignment expression target is not supported in codegen");
                return PhpType::Int;
            }
        },
        ExprKind::PropertyAccess { object, property } => {
            super::super::stmt::emit_property_assign_stmt(
                object, property, value, emitter, ctx, data,
            );
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            super::super::stmt::emit_static_property_assign_stmt(
                receiver, property, value, emitter, ctx, data,
            );
        }
        _ => {
            emitter.comment("WARNING: assignment expression target is not supported in codegen");
            return PhpType::Int;
        }
    }

    super::emit_expr(target, emitter, ctx, data)
}
