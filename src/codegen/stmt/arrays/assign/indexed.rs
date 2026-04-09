mod extend;
mod normalize;
mod prepare;
mod store;

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;

use super::ArrayAssignTarget;

pub(super) fn emit_indexed_array_assign(
    target: &ArrayAssignTarget<'_>,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let state = prepare::prepare_indexed_array_assign(target, index, value, emitter, ctx, data);
    normalize::normalize_indexed_array_layout(&state, emitter, ctx);
    store::store_indexed_array_value(target, &state, emitter, ctx);
    extend::extend_indexed_array_if_needed(&state, emitter, ctx);
}
