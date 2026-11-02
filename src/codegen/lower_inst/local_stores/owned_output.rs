//! Purpose:
//! Publishes owned builtin outputs before retiring their previous local or reference payload.
//!
//! Called from:
//! - Native PCNTL status, usage and signal-array output lowerers.
//!
//! Key details:
//! - Runtime representation flags select raw or reference storage at publication.
//! - Reference stores preserve an existing persistent PHPRef's child identity.

use super::*;

/// Consumes the current owned result and atomically replaces one builtin output destination.
pub(in crate::codegen) fn publish_current_owned_output(
    ctx: &mut FunctionContext<'_>, slot: LocalSlotId, produced_type: &PhpType,
) -> Result<()> {
    let ty = ctx.local_php_type(slot)?.codegen_repr();
    let dynamic = ctx.local_ref_cell_representation_is_dynamic(slot);
    let raw = dynamic.then(|| ctx.next_label("owned_output_raw"));
    let done = dynamic.then(|| ctx.next_label("owned_output_done"));
    if let Some(raw) = &raw {
        let state = ctx.ref_cell_state_offset(slot).ok_or_else(||
            CodegenIrError::invalid_module("dynamic output has no reference-state flag"))?;
        let scratch = abi::secondary_scratch_reg(ctx.emitter);
        abi::load_at_offset(ctx.emitter, scratch, state);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cbz {scratch}, {raw}"));      // publish raw storage only when no reference is attached
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("test {scratch}, {scratch}")); // inspect the output's current representation
                ctx.emitter.instruction(&format!("jz {raw}"));                  // publish raw storage only when no reference is attached
            }
        }
    }
    if ctx.local_stores_ref_cell_pointer(slot) {
        let payload = ctx.ref_cell_payload_type(slot)?.codegen_repr();
        if payload != PhpType::Mixed && payload != produced_type.codegen_repr() {
            return Err(CodegenIrError::unsupported(format!(
                "builtin output {:?} cannot retype a native reference with {:?} payload", produced_type, payload,
            )));
        }
        if payload == PhpType::Mixed && ty != PhpType::Mixed {
            crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, produced_type);
        } else {
            coerce_ref_cell_store_value(ctx, &ty, &payload)?;
        }
        publish_current_ref_cell_value(ctx, slot, &payload, RefCellStorePrevious::RetireAcquired)?;
        if let Some(done) = &done { abi::emit_jump(ctx.emitter, done); }
    }
    if let Some(raw) = &raw { ctx.emitter.label(raw); }
    if !ctx.local_ref_cell_representation_is_definite(slot) {
        if matches!(ty, PhpType::Str | PhpType::Callable) || ty.is_refcounted() {
            abi::emit_push_result_value(ctx.emitter, &ty);
            let previous = abi::tertiary_scratch_reg(ctx.emitter);
            let offset = ctx.local_offset(slot)?;
            abi::load_at_offset(ctx.emitter, previous, offset);
            pop_ref_cell_store_value(ctx, &ty);
            abi::emit_push_reg(ctx.emitter, previous);
            ctx.store_current_result_to_local(slot)?;
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            let release = match ty {
                PhpType::Str => "__rt_heap_free_safe",
                PhpType::Callable => "__rt_callable_descriptor_release",
                _ => "__rt_decref_any",
            };
            abi::emit_unary_cleanup_preserving_exception(ctx.emitter, release,
                abi::int_result_reg(ctx.emitter));
        } else {
            ctx.store_current_result_to_local(slot)?;
        }
    }
    if let Some(done) = &done { ctx.emitter.label(done); }
    Ok(())
}
