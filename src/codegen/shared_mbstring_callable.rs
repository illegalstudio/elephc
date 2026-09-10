//! Purpose:
//! Resolves native mbregex callbacks using the program's typed callable descriptor machinery.
//!
//! Called from:
//! - `block_emit::emit_module()` before user functions are emitted.
//!
//! Key details:
//! - The runtime borrows a boxed value and receives an independently owned descriptor.
//! - A dedicated spill below the saved nested-call register survives inline wrapper emission.

use super::{abi, data_section::DataSection, emit::Emitter, shared_state::SharedCodegenState, Result};
use crate::{ir::Module, types::PhpType};

/// Emits the program-specific resolver only when the shared mbregex runtime can reference it.
pub(super) fn emit(module: &Module, emitter: &mut Emitter, data: &mut DataSection,
    shared: &mut SharedCodegenState) -> Result<()> {
    if !module.required_runtime_features.mbregex { return Ok(()); }
    let label = "_eir_shared_mbstring_callable";
    emitter.raw(&format!(".global {label}"));
    super::shared_helper::emit_shared_helper(module, emitter, data, shared, false, label,
        PhpType::Callable, "resolve a native replacement callback", |ctx| {
            let value = super::shared_helper::helper_value();
            abi::emit_reserve_temporary_stack(ctx.emitter, 16);
            ctx.placement.slot_of.insert(value, 32);
            ctx.store_int_result_value(value)?;
            super::lower_inst::callables::emit_runtime_unary_callback_descriptor_value(
                ctx, value, "mb_ereg_replace_callback",
                "mb_ereg_replace_callback(): Argument #2 ($callback) must be a valid callback, function not found or invalid function name")?;
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            Ok(())
        })?;
    let label = "_eir_mbstring_callback_invalid";
    emitter.raw(&format!(".global {label}"));
    super::shared_helper::emit_shared_helper(module, emitter, data, shared, false, label,
        PhpType::Void, "reject an invalid replacement callback", |ctx| {
            super::lower_inst::exceptions::emit_type_error(ctx,
                "mb_ereg_replace_callback(): Argument #2 ($callback) must be a valid callback");
            Ok(())
        })
}
