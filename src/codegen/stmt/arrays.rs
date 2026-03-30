mod assign;
mod push;
mod unpack;

use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use crate::parser::ast::Expr;

pub(super) fn emit_array_assign_stmt(
    array: &str,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    assign::emit_array_assign_stmt(array, index, value, emitter, ctx, data)
}

pub(super) fn emit_array_push_stmt(
    array: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    push::emit_array_push_stmt(array, value, emitter, ctx, data)
}

pub(super) fn emit_list_unpack_stmt(
    vars: &[String],
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    unpack::emit_list_unpack_stmt(vars, value, emitter, ctx, data)
}
