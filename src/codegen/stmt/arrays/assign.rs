//! Purpose:
//! Dispatches array element assignment to associative, indexed, buffer, and specialized paths.
//! Chooses the storage strategy from the target expression and inferred container type.
//!
//! Called from:
//! - `crate::codegen::stmt::arrays`
//!
//! Key details:
//! - Write paths must evaluate targets and values once while preserving container ownership updates.

mod assoc;
mod buffer;
mod indexed;

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub(super) fn emit_array_assign_stmt(
    array: &str,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("${}[...] = ...", array));
    if let Some((current, default)) = super::super::null_coalesce_array_target(array, index, value)
    {
        if matches!(default.kind, ExprKind::Null) {
            emitter.comment("literal null fallback leaves the array slot unchanged");
            return;
        }
        let current_ty = crate::codegen::expr::emit_expr(current, emitter, ctx, data);
        if current_ty != PhpType::Void {
            let keep_label = ctx.next_label("nca_keep");
            super::super::emit_branch_if_result_non_null(&current_ty, &keep_label, emitter);
            emit_array_assign_stmt(array, index, default, emitter, ctx, data);
            emitter.label(&keep_label);
        } else {
            emit_array_assign_stmt(array, index, default, emitter, ctx, data);
        }
        return;
    }

    let var = match ctx.variables.get(array) {
        Some(v) => v,
        None => {
            emitter.comment(&format!("WARNING: undefined variable ${}", array));
            return;
        }
    };
    let var_ty = var.ty.clone();
    let var_static_ty = var.static_ty.clone();
    let offset = var.stack_offset;
    let is_ref = ctx.ref_params.contains(array);
    if crate::codegen::expr::arrays::type_is_array_access_object(&var_static_ty, ctx) {
        let object = Expr::new(ExprKind::Variable(array.to_string()), index.span);
        crate::codegen::expr::arrays::emit_array_access_offset_set(
            &object, index, value, emitter, ctx, data,
        );
        return;
    }
    let target = ArrayAssignTarget {
        array,
        offset,
        is_ref,
        elem_ty: match &var_ty {
            PhpType::Array(t) => *t.clone(),
            PhpType::AssocArray { value: v, .. } => *v.clone(),
            PhpType::Buffer(t) => *t.clone(),
            _ => PhpType::Int,
        },
    };

    match &var_ty {
        PhpType::Buffer(_) => {
            buffer::emit_buffer_array_assign(&target, index, value, emitter, ctx, data);
        }
        PhpType::AssocArray { .. } => {
            assoc::emit_assoc_array_assign(&target, index, value, emitter, ctx, data);
        }
        _ => {
            indexed::emit_indexed_array_assign(&target, index, value, emitter, ctx, data);
        }
    }
}

#[derive(Clone)]
pub(super) struct ArrayAssignTarget<'a> {
    pub array: &'a str,
    pub offset: usize,
    pub is_ref: bool,
    pub elem_ty: PhpType,
}
