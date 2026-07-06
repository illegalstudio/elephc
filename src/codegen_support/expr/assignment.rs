//! Purpose:
//! Lowers assignment expressions that appear where an expression result is required.
//! Bridges statement assignment machinery with expression result preservation.
//!
//! Called from:
//! - `crate::codegen_support::expr::emit_expr()`
//!
//! Key details:
//! - Writes must happen once and the assigned value must remain available in the expected result registers.

use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::types::PhpType;

/// Emits an assignment expression that also serves as an expression result.
///
/// For simple local variable targets, emits the assignment statement then emits
/// the result target (or the variable itself if no result target is specified).
/// For non-local targets (array access, property access), delegates to the
/// non-local machinery which writes first then evaluates the result expression.
///
/// The `prelude` contains any leading assignment statements (e.g., from spread
/// argument preprocessing). The `conditional_value_temp` is set when a null
/// coalescing assignment has a non-null current value that must be preserved
/// across the default branch.
///
/// Returns the PHP type of the result expression.
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
        Some(other) if !is_same_local(other, name) => {
            super::emit_expr(other, emitter, ctx, data)
        }
        _ => super::variables::emit_variable(name, emitter, ctx),
    }
}

/// Returns true if `expr` is a Variable node with the same name as `name`.
fn is_same_local(expr: &Expr, name: &str) -> bool {
    matches!(&expr.kind, ExprKind::Variable(other) if other == name)
}

/// Emits an assignment expression with a non-local target (array access, property, etc.).
///
/// Writes the value to the target first, then evaluates and returns the result expression
/// (or the target itself if no result target is given). Unlike local variable assignment,
/// the write must occur before the result is computed because the target may involve
/// intermediate expressions or memory that would be clobbered by result evaluation.
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

/// Emits a null-coalescing assignment expression with a non-local target.
///
/// Handles the case where the right-hand side is a NullCoalesce node, and the current
/// value may be a Mixed or Union type requiring special handling. If the current value
/// is non-null, emits the result target (preserving the current value via a temporary
/// on the stack for Mixed/Union types) and jumps to done. If the current value is null,
/// evaluates the default, assigns it to `temp_name`, writes it to the non-local target,
/// and uses the default as the result.
///
/// Returns `None` if the value is not a NullCoalesce expression (caller should fall back
/// to a regular non-local assignment). Returns `Some(PhpType)` with the widened result type.
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
    let keep_label = (current_ty != PhpType::Void)
        .then(|| ctx.next_label("nca_expr_keep"));
    let done_label = keep_label
        .as_ref()
        .map(|_| ctx.next_label("nca_expr_done"));
    let saved_current_bytes = if matches!(current_ty, PhpType::Mixed | PhpType::Union(_)) {
        crate::codegen_support::abi::emit_push_reg(
            emitter,
            crate::codegen_support::abi::int_result_reg(emitter),
        ); // preserve the boxed current value while the null check unboxes its tag
        16
    } else {
        0
    };
    if let Some(label) = &keep_label {
        super::super::stmt::emit_branch_if_result_non_null(&current_ty, label, emitter);
    }
    super::super::stmt::emit_assign_stmt(temp_name, default, emitter, ctx, data);
    let temp_value = Expr::new(ExprKind::Variable(temp_name.to_string()), default.span);
    emit_non_local_assignment_write(target, &temp_value, emitter, ctx, data);
    let default_ty = super::emit_expr(&temp_value, emitter, ctx, data);
    let result_ty = super::widen_codegen_type(&current_ty, &default_ty);
    super::coerce_result_to_type(emitter, ctx, data, &default_ty, &result_ty);
    if saved_current_bytes != 0 {
        crate::codegen_support::abi::emit_release_temporary_stack(emitter, saved_current_bytes); // discard the saved null value on the default-assignment path
    }
    if let Some(label) = &done_label {
        crate::codegen_support::abi::emit_jump(emitter, label);                         // keep the just-assigned default value as the expression result
    }
    if let Some(label) = &keep_label {
        emitter.label(label);
        if saved_current_bytes != 0 {
            crate::codegen_support::abi::emit_pop_reg(
                emitter,
                crate::codegen_support::abi::int_result_reg(emitter),
            ); // restore the original boxed current value for the keep-existing path
        }
        super::coerce_result_to_type(emitter, ctx, data, &current_ty, &result_ty);
    }
    if let Some(label) = &done_label {
        emitter.label(label);
    }

    Some(result_ty)
}

/// Emits the write half of a non-local assignment expression.
///
/// Dispatches to the appropriate statement emitter based on the target expression kind:
/// - ArrayAccess on a Variable: `emit_array_assign_stmt`
/// - ArrayAccess on a PropertyAccess: `emit_property_array_assign_stmt`
/// - ArrayAccess on a StaticPropertyAccess: `emit_static_property_array_assign_stmt`
/// - ArrayAccess on a nested expression: `emit_nested_array_assign_stmt`
/// - PropertyAccess: `emit_property_assign_stmt`
/// - StaticPropertyAccess: `emit_static_property_assign_stmt`
/// Falls through to a warning comment for unsupported targets.
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
                super::super::stmt::emit_nested_array_assign_stmt(
                    target, value, emitter, ctx, data,
                );
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

/// Emits any leading statements that precede the assignment expression.
///
/// Iterates over `prelude` statements and emits each one. Assign statements are emitted
/// via `emit_assign_stmt` directly. Synthetic statements are handled recursively.
/// All other statement kinds are emitted via the standard statement emitter.
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
