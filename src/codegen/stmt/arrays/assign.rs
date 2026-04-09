mod assoc;
mod buffer;
mod indexed;

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
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
    let var = match ctx.variables.get(array) {
        Some(v) => v,
        None => {
            emitter.comment(&format!("WARNING: undefined variable ${}", array));
            return;
        }
    };
    let target = ArrayAssignTarget {
        array,
        offset: var.stack_offset,
        is_ref: ctx.ref_params.contains(array),
        elem_ty: match &var.ty {
            PhpType::Array(t) => *t.clone(),
            PhpType::AssocArray { value: v, .. } => *v.clone(),
            PhpType::Buffer(t) => *t.clone(),
            _ => PhpType::Int,
        },
    };

    match &var.ty {
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
