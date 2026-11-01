//! Purpose:
//! Emits the module's one copy of the eval bridge's "release the site's boxed cells, then fail"
//! entry, so a site that boxed operands for the bridge pays a few instructions on a non-zero status
//! instead of a release per cell.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()` before the module's own functions.
//! - `crate::codegen::lower_inst::builtins::eval::status::emit_eval_status_check_releasing` at every
//!   site that boxed operands.
//!
//! Key details:
//! - A site tests the status for zero inline and otherwise calls the helper with the status in
//!   the integer result register, its eval error cell in the second integer argument register, the
//!   address of its boxed cells and their slot mask in the next two. The helper releases the marked
//!   cells and then enters the module's one status dispatch (`__elephc_eval_check_status`, see
//!   `eval_status_helper`), which never returns: it throws or exits.
//! - A Throwable unwinds through the caller's activation record exactly as the inlined throw did:
//!   shared helpers own no cleanup-tracked locals.
//! - Emitted exactly when the module uses the eval bridge or eval scope (the same condition under
//!   which the status dispatch itself exists), so a site and the emitter cannot disagree about
//!   whether the label exists.

use crate::codegen::context::FunctionContext;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::shared_state::SharedCodegenState;
use crate::ir::Module;
use crate::types::PhpType;

use super::eval_status_helper::EVAL_STATUS_LABEL;
use super::lower_inst::emit_shared_eval_status_fail_cells_body;
use super::shared_helper::emit_shared_helper;
use super::Result;

/// Label of the helper that first releases the site's boxed cells, then enters the module's status dispatch.
const EVAL_STATUS_FAIL_CELLS_LABEL: &str = "_eir_shared_eval_status_fail_cells";

/// Whether this module carries the helper.
fn module_wants_helper(module: &Module) -> bool {
    let features = &module.required_runtime_features;
    features.eval_bridge || features.eval_scope
}

/// Returns the label of the releasing variant a status check that boxed operands should call.
pub(in crate::codegen) fn shared_cells_label(ctx: &mut FunctionContext<'_>) -> Option<&'static str> {
    if ctx.function.name == EVAL_STATUS_FAIL_CELLS_LABEL {
        return None;
    }
    let module = ctx.module;
    module_wants_helper(module).then_some(EVAL_STATUS_FAIL_CELLS_LABEL)
}

/// Emits the shared helper when the module uses it.
pub(super) fn emit_shared_eval_status(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    if !module_wants_helper(module) {
        return Ok(());
    }
    emit_shared_helper(
        module,
        emitter,
        data,
        shared,
        regalloc_linear,
        EVAL_STATUS_FAIL_CELLS_LABEL,
        PhpType::Void,
        &format!(
            "--- shared eval status failure releasing boxed cells: {} (status, error cell, cell base, cell mask) ---",
            EVAL_STATUS_FAIL_CELLS_LABEL
        ),
        |ctx| emit_shared_eval_status_fail_cells_body(ctx, EVAL_STATUS_LABEL),
    )
}
