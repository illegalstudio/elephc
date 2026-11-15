//! Purpose:
//! Publishes typed scalar property writes into already canonical managed cells.
//!
//! Called from:
//! - Shared reference-property publication after ordinary declaration validation.
//!
//! Key details:
//! - Each write checks the current physical shape and preserves the wrapper identity.
//! - Raw cells retain their existing payload layout and cleanup path.

use super::*;

/// Publishes a selected scalar path, returning false when the ordinary writer remains authoritative.
pub(super) fn publish(ctx: &mut FunctionContext<'_>, pointer_reg: &str, prop_ty: &PhpType,
    incoming_result_ty: &PhpType) -> Result<bool> {
    if !ctx.module.required_runtime_features.native_reference_validation
        || !crate::types::canonical_property_reference_payload(prop_ty).is_some_and(|payload|
            payload != crate::types::CanonicalReferencePayload::Boxed)
    { return Ok(false); }
    let prop_ty = prop_ty.codegen_repr();
    let raw = ctx.next_label("scalar_property_reference_raw");
    let done = ctx.next_label("scalar_property_reference_done");
    abi::emit_push_result_value(ctx.emitter, &incoming_result_ty.codegen_repr());
    abi::emit_push_reg(ctx.emitter, pointer_reg);
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), pointer_reg);
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_canonical");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &raw);
    let child = abi::symbol_scratch_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("add {child}, x0, #8")), // replace the canonical child while preserving its identity
        Arch::X86_64 => ctx.emitter.instruction(&format!("lea {child}, [rax + 8]")), // select the owning child slot of the persistent wrapper
    }
    abi::emit_store_to_sp(ctx.emitter, child, 0);
    abi::emit_pop_reg(ctx.emitter, pointer_reg);
    restore_property_store_result(ctx, &incoming_result_ty.codegen_repr());
    abi::emit_push_reg(ctx.emitter, pointer_reg);
    emit_box_current_owned_value_as_mixed(ctx.emitter, &prop_ty);
    abi::emit_pop_reg(ctx.emitter, pointer_reg);
    publish_raw_reference_cell_result(ctx, pointer_reg, &PhpType::Mixed, &PhpType::Mixed)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&raw);
    abi::emit_pop_reg(ctx.emitter, pointer_reg);
    restore_property_store_result(ctx, &incoming_result_ty.codegen_repr());
    publish_raw_reference_cell_result(ctx, pointer_reg, &prop_ty, incoming_result_ty)?;
    ctx.emitter.label(&done);
    Ok(true)
}
