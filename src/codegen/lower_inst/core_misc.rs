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
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_concat_off", 0);
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

/// Lowers a PHP-visible collector control or status operation through typed runtime helpers.
pub(super) fn lower_gc_control(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let Some(Immediate::I64(raw_op)) = inst.immediate else {
        return Err(CodegenIrError::invalid_module(
            "gc_control requires a typed selector immediate",
        ));
    };
    let op = crate::ir::GcControlOp::from_i64(raw_op).ok_or_else(|| {
        CodegenIrError::invalid_module(format!("unknown gc_control selector {raw_op}"))
    })?;
    match op {
        crate::ir::GcControlOp::Collect => {
            abi::emit_call_label(ctx.emitter, "__rt_gc_collect_cycles_explicit");
        }
        crate::ir::GcControlOp::Disable => {
            abi::emit_call_label(ctx.emitter, "__rt_gc_disable");
        }
        crate::ir::GcControlOp::Enable => {
            abi::emit_call_label(ctx.emitter, "__rt_gc_enable");
        }
        crate::ir::GcControlOp::Enabled => {
            abi::emit_call_label(ctx.emitter, "__rt_gc_enabled");
        }
        crate::ir::GcControlOp::MemCaches => {
            abi::emit_call_label(ctx.emitter, "__rt_gc_mem_caches");
        }
        crate::ir::GcControlOp::Running
        | crate::ir::GcControlOp::Protected
        | crate::ir::GcControlOp::Runs
        | crate::ir::GcControlOp::Collected
        | crate::ir::GcControlOp::Roots => {
            match ctx.emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    abi::emit_load_int_immediate(ctx.emitter, "x0", op.as_i64());
                }
                crate::codegen::platform::Arch::X86_64 => {
                    abi::emit_load_int_immediate(ctx.emitter, "rdi", op.as_i64());
                }
            }
            abi::emit_call_label(ctx.emitter, "__rt_gc_status_metric");
        }
        crate::ir::GcControlOp::ApplicationTime
        | crate::ir::GcControlOp::CollectorTime
        | crate::ir::GcControlOp::DestructorTime
        | crate::ir::GcControlOp::FreeTime => {
            match ctx.emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    abi::emit_load_int_immediate(ctx.emitter, "x0", op.as_i64());
                    abi::emit_call_label(ctx.emitter, "__rt_gc_status_metric");
                    ctx.emitter.instruction("fmov d0, x0");
                }
                crate::codegen::platform::Arch::X86_64 => {
                    abi::emit_load_int_immediate(ctx.emitter, "rdi", op.as_i64());
                    abi::emit_call_label(ctx.emitter, "__rt_gc_status_metric");
                    ctx.emitter.instruction("movq xmm0, rax");
                }
            }
        }
    }
    store_if_result(ctx, inst)
}
