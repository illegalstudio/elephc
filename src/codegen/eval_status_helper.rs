//! Purpose:
//! Emits the one module-level helper that dispatches an eval bridge status.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm` through `emit_eval_status_helper`.
//!
//! Key details:
//! - The dispatch is identical at every site that can observe a status, so one
//!   helper serves them all: the status arrives in the result register and the
//!   pending Throwable in argument register 1 (`x1` on AArch64, `rsi` on x86_64).
//! - The Throwable is passed as an argument rather than read from the caller's
//!   eval scratch frame, which this frame cannot address.
//! - The helper never returns for a non-zero status: every branch either throws
//!   or exits the process.

use crate::codegen::context::FunctionContext;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::shared_helper::emit_global_shared_helper;
use crate::codegen::shared_state::SharedCodegenState;
use crate::codegen::Result;
use crate::ir::Module;
use crate::types::PhpType;

/// Label of the helper that dispatches an eval bridge status.
pub(in crate::codegen) const EVAL_STATUS_LABEL: &str = "__elephc_eval_check_status";

/// Emits the module-level eval status dispatch helper, if the program needs one.
///
/// The helper is emitted whenever the program can reach a call that reports an
/// eval status — the BRIDGE itself, or the eval SCOPE accessors a literal-eval
/// program uses without linking the bridge at all. Both lower to
/// `emit_eval_status_check`, which is a call to this helper.
pub(in crate::codegen) fn emit_eval_status_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<bool> {
    let features = &module.required_runtime_features;
    if !features.eval_bridge && !features.eval_scope {
        return Ok(false);
    }
    emit_global_shared_helper(
        module,
        emitter,
        data,
        shared,
        regalloc_linear,
        EVAL_STATUS_LABEL,
        PhpType::Int,
        "--- eval bridge: status dispatch ---",
        |ctx: &mut FunctionContext<'_>| {
            crate::codegen::lower_inst::builtins::emit_eval_status_helper_body(ctx);
            Ok(())
        },
    )?;
    Ok(true)
}
