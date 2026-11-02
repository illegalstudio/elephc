//! Purpose:
//! Emits the one module-level helper that seeds a fresh eval context with the
//! AOT native catalog (classes, methods, properties, constants, functions).
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm` through `emit_eval_inventory_helper`.
//!
//! Key details:
//! - The catalog is registered once per program, so its emission cost does not
//!   scale with the number of `eval()` call sites that share it.
//! - The helper takes the eval context handle in argument register 0 (`x0` /
//!   `rdi`) and parks it in a module-level symbol. Its frame is the fixed one the
//!   shared-helper prologue builds (32 bytes on AArch64, 24 on x86_64), with no
//!   room for a frame-pointer local, and an `sp`-relative slot is unusable here
//!   because the body displaces `sp` while staging constant values.
//! - The symbol is process-global, so the transaction must not re-enter itself:
//!   none of the registration calls it makes executes user PHP or suspends a
//!   fiber, and `--web` workers are processes. If a registration path ever gains
//!   a user callback, this has to become a per-thread slot.
//! - The body is function-independent: the registration emitters read only the
//!   module catalog, the data section, and the shared codegen state.

use crate::codegen::context::FunctionContext;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::shared_helper::emit_global_shared_helper;
use crate::codegen::shared_state::SharedCodegenState;
use crate::codegen::Result;
use crate::ir::Module;
use crate::types::PhpType;

/// Label of the helper that registers the native inventory into an eval context.
pub(in crate::codegen) const EVAL_INVENTORY_LABEL: &str = "__elephc_eval_register_inventory";

/// Symbol holding the eval context handle while the inventory helper runs.
pub(in crate::codegen) const EVAL_INVENTORY_CONTEXT_SYMBOL: &str = "_elephc_eval_inventory_context";

/// Emits the module-level eval inventory registration helper, if the program needs one.
///
/// The helper is emitted only when the program actually links the eval bridge,
/// because its body references the `__elephc_eval_register_native_*` bridge
/// symbols. Returns true when the helper was emitted.
pub(in crate::codegen) fn emit_eval_inventory_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<bool> {
    if !module.required_runtime_features.eval_bridge {
        return Ok(false);
    }
    emit_global_shared_helper(
        module,
        emitter,
        data,
        shared,
        regalloc_linear,
        EVAL_INVENTORY_LABEL,
        PhpType::Int,
        "--- eval bridge: native inventory registration ---",
        |ctx: &mut FunctionContext<'_>| {
            crate::codegen::lower_inst::builtins::register_eval_inventory(ctx)
        },
    )?;
    Ok(true)
}
