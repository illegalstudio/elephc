//! Purpose:
//! Lowers small instruction-boundary, NOP, concat-reset, and GC-safe-point operations.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers a statement-boundary concat-buffer reset.
pub(super) fn lower_concat_reset(ctx: &mut FunctionContext<'_>) -> Result<()> {
    reset_concat_to_frame_base(ctx);
    Ok(())
}

/// Restores `_concat_off` to the offset inherited by this EIR frame.
pub(super) fn reset_concat_to_frame_base(ctx: &mut FunctionContext<'_>) {
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    abi::load_at_offset(ctx.emitter, scratch, ctx.concat_base_offset);
    crate::codegen_support::runtime::ctx::emit_concat_off_store(ctx.emitter, scratch);
}

/// Lowers metadata-only NOPs, emitting data-backed messages as assembly comments.
pub(super) fn lower_nop(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return Ok(());
    };
    let message = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    ctx.emitter.comment(message);
    Ok(())
}

/// Lowers a closure capture marker after call operands already recorded the captured value.
pub(super) fn lower_closure_capture(_ctx: &mut FunctionContext<'_>, _inst: &Instruction) -> Result<()> {
    Ok(())
}

/// Lowers an explicit cycle-collection safe point.
pub(super) fn lower_gc_collect(ctx: &mut FunctionContext<'_>) -> Result<()> {
    abi::emit_call_label(ctx.emitter, "__rt_gc_collect_cycles");
    Ok(())
}

