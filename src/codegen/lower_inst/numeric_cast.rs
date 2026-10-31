//! Purpose:
//! Extracts integer payloads directly from proven numeric frame values.
//!
//! Called from:
//! - Explicit integer cast lowering after warning-sensitive cast routes are handled.
//!
//! Key details:
//! - Existing frame-cell and whole-slot proofs exclude null, aliases and unknown tags.
//! - Float payloads retain the original runtime conversion, including out-of-range behavior.
//! - The proof is immutable; this emitter introduces no new graph walks or scalar assumptions.

use crate::codegen::{abi, context::FunctionContext, platform::Arch, stack_numeric, Result};
use crate::ir::{Immediate, Instruction, IrHeapKind, IrType, ValueId};
use crate::types::PhpType;

/// Uses a guarded payload load only when existing frame proofs cover the entire numeric value.
pub(super) fn try_lower_int(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, value: ValueId,
) -> Result<bool> {
    if inst.immediate != Some(Immediate::CastTarget(IrType::I64))
        || inst.result_php_type != PhpType::Int
        || ctx.value_ir_type(value)? != IrType::Heap(IrHeapKind::Mixed)
        || ctx.raw_value_php_type(value)?.codegen_repr() != PhpType::Mixed
        || !(stack_numeric::is_stack_value(ctx.function, &ctx.numeric_cell_offsets, value)
            || stack_numeric::numeric_local_read(ctx.function, &ctx.numeric_local_slots, value))
    {
        return Ok(false);
    }
    let slow = ctx.next_label("frame_numeric_int_cast_slow");
    let done = ctx.next_label("frame_numeric_int_cast_done");
    ctx.load_value_to_result(value)?;
    let result = abi::int_result_reg(ctx.emitter);
    let tag = abi::symbol_scratch_reg(ctx.emitter);

    // -- preserve a promoted float while extracting an integer without a runtime call --
    abi::emit_load_from_address(ctx.emitter, tag, result, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbnz {tag}, {slow}")), // preserve the original boxed float conversion on the cold path
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {tag}, {tag}"));             // distinguish the proven integer and float tags
            ctx.emitter.instruction(&format!("jnz {slow}"));                    // preserve the original boxed float conversion on the cold path
        }
    }
    abi::emit_load_from_address(ctx.emitter, result, result, 8);
    ctx.emitter.label(&done);
    super::store_if_result(ctx, inst)?;
    ctx.emit_cold_path(|ctx| {
        ctx.emitter.label(&slow);
        super::load_value_to_first_int_arg(ctx, value)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        abi::emit_jump(ctx.emitter, &done);
        Ok(())
    })?;
    Ok(true)
}
