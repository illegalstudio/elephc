//! Purpose:
//! Lowers assignment expressions that appear where an expression result is required.
//! Bridges statement assignment machinery with expression result preservation.
//!
//! Called from:
//! - `crate::codegen::expr::emit_expr()`
//!
//! Key details:
//! - Writes must happen once and the assigned value must remain available in the expected result registers.

use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::types::PhpType;

pub(super) fn emit_assignment_expr(
    target: &Expr,
    value: &Expr,
    result_target: Option<&Expr>,
    prelude: &[Stmt],
    conditional_value_temp: Option<&str>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emit_assignment_prelude(prelude, emitter, ctx, data);

    if let Some(temp_name) = conditional_value_temp {
        if let Some(ty) = emit_conditional_non_local_null_coalesce_assignment(
            temp_name,
            target,
            value,
            result_target,
            emitter,
            ctx,
            data,
        ) {
            return ty;
        }
    }

    let ExprKind::Variable(name) = &target.kind else {
        return emit_non_local_assignment_expr(target, value, result_target, emitter, ctx, data);
    };

    super::super::stmt::emit_assign_stmt(name, value, emitter, ctx, data);
    match result_target {
        Some(target) => super::emit_expr(target, emitter, ctx, data),
        None => super::variables::emit_variable(name, emitter, ctx),
    }
}

pub(super) fn emit_non_local_assignment_expr(
    target: &Expr,
    value: &Expr,
    result_target: Option<&Expr>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emit_non_local_assignment_write(target, value, emitter, ctx, data);

    super::emit_expr(result_target.unwrap_or(target), emitter, ctx, data)
}

fn emit_conditional_non_local_null_coalesce_assignment(
    temp_name: &str,
    target: &Expr,
    value: &Expr,
    result_target: Option<&Expr>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let ExprKind::NullCoalesce {
        value: current,
        default,
    } = &value.kind
    else {
        return None;
    };

    if matches!(default.kind, ExprKind::Null) {
        return Some(super::emit_expr(result_target.unwrap_or(target), emitter, ctx, data));
    }

    let current_ty = super::emit_expr(current, emitter, ctx, data);
    if current_ty != PhpType::Void {
        let keep_label = ctx.next_label("nca_expr_keep");
        super::super::stmt::emit_branch_if_result_non_null(&current_ty, &keep_label, emitter);
        super::super::stmt::emit_assign_stmt(temp_name, default, emitter, ctx, data);
        let temp_value = Expr::new(ExprKind::Variable(temp_name.to_string()), default.span);
        emit_non_local_assignment_write(target, &temp_value, emitter, ctx, data);
        emitter.label(&keep_label);
    } else {
        super::super::stmt::emit_assign_stmt(temp_name, default, emitter, ctx, data);
        let temp_value = Expr::new(ExprKind::Variable(temp_name.to_string()), default.span);
        emit_non_local_assignment_write(target, &temp_value, emitter, ctx, data);
    }

    Some(super::emit_expr(result_target.unwrap_or(target), emitter, ctx, data))
}

fn emit_non_local_assignment_write(
    target: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
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
        }
    }
}

fn emit_assignment_prelude(
    prelude: &[Stmt],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    for stmt in prelude {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                super::super::stmt::emit_assign_stmt(name, value, emitter, ctx, data);
            }
            StmtKind::Synthetic(stmts) => {
                emit_assignment_prelude(stmts, emitter, ctx, data);
            }
            _ => {
                super::super::stmt::emit_stmt(stmt, emitter, ctx, data);
            }
        }
    }
}
