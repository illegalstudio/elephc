mod assoc;
mod indexed;

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{Expr, Stmt};
use crate::types::PhpType;

pub(super) fn emit_foreach_stmt(
    array: &Expr,
    key_var: &Option<String>,
    value_var: &str,
    body: &[Stmt],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let loop_start = ctx.next_label("foreach_start");
    let loop_end = ctx.next_label("foreach_end");
    let loop_cont = ctx.next_label("foreach_cont");

    emitter.blank();
    emitter.comment("foreach");

    let arr_ty = emit_expr(array, emitter, ctx, data);

    match &arr_ty {
        PhpType::AssocArray { value, .. } => {
            assoc::emit_assoc_foreach(
                key_var,
                value_var,
                body,
                &loop_start,
                &loop_end,
                &loop_cont,
                &*value.clone(),
                emitter,
                ctx,
                data,
            );
        }
        _ => {
            let elem_ty = match &arr_ty {
                PhpType::Array(t) => *t.clone(),
                _ => PhpType::Int,
            };
            indexed::emit_indexed_foreach(
                key_var,
                value_var,
                body,
                &loop_start,
                &loop_end,
                &loop_cont,
                &elem_ty,
                emitter,
                ctx,
                data,
            );
        }
    }
}
