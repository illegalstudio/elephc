//! Purpose:
//! Coordinates indexed array assignment preparation, normalization, extension, and value storage.
//! Keeps platform-sensitive substeps behind one statement-level entry point.
//!
//! Called from:
//! - `crate::codegen::stmt::arrays::assign`
//!
//! Key details:
//! - Index normalization and capacity extension must happen before storing the coerced element value.

mod extend;
mod normalize;
mod prepare;
mod store;

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;

use super::ArrayAssignTarget;

/// Orchestrates the four-phase indexed array assignment pipeline: prepare, normalize,
/// store, and extend. Dispatches to target-specific helpers and preserves register state
/// across phases using the prepared `IndexedAssignState`.
///
/// # Arguments
/// * `target` - the array being assigned into
/// * `index` - the integer index expression
/// * `value` - the value expression to assign
/// * `emitter` - target-specific instruction emitter
/// * `ctx` - codegen context (labels, locals, types)
/// * `data` - data section for literals and runtime metadata
pub(super) fn emit_indexed_array_assign(
    target: &ArrayAssignTarget<'_>,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let state = prepare::prepare_indexed_array_assign(target, index, value, emitter, ctx, data);
    super::super::push::update_callable_array_metadata(
        target.array,
        value,
        &state.val_ty,
        ctx,
    );
    normalize::normalize_indexed_array_layout(&state, emitter, ctx);
    store::store_indexed_array_value(target, &state, emitter, ctx);
    extend::extend_indexed_array_if_needed(&state, emitter, ctx);
}
