//! Purpose:
//! Walks EIR basic blocks in function order and delegates instruction/terminator lowering.
//! Owns main-function setup for the initial Phase 04 backend path.
//!
//! Called from:
//! - `crate::codegen_ir::generate_user_asm_from_ir()`.
//!
//! Key details:
//! - This first backend increment supports straight-line main blocks and reports
//!   explicit unsupported-feature errors for control flow not lowered yet.

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::ir::{BasicBlock, Function, Module};

use super::context::FunctionContext;
use super::frame;
use super::lower_inst;
use super::lower_term;
use super::{CodegenIrError, Result};

/// Emits the EIR main function as the process entry point.
pub(super) fn emit_main_function(
    module: &Module,
    function: &Function,
    emitter: &mut Emitter,
    data: &mut DataSection,
) -> Result<()> {
    let layout = frame::layout_for_function(function);
    let mut ctx = FunctionContext::new(module, function, emitter, data, layout);
    frame::emit_main_prologue(&mut ctx);
    emit_blocks(&mut ctx)?;
    if !ctx.epilogue_emitted {
        frame::emit_main_epilogue(&mut ctx);
    }
    Ok(())
}

/// Emits every block in table order.
fn emit_blocks(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let blocks = ctx.function.blocks.clone();
    for block in blocks {
        emit_block(ctx, &block)?;
    }
    Ok(())
}

/// Emits one EIR basic block.
fn emit_block(ctx: &mut FunctionContext<'_>, block: &BasicBlock) -> Result<()> {
    if block.id != ctx.function.entry {
        ctx.emitter.label(&ctx.block_label(&block.name, block.id.as_raw()));
    }
    for inst_id in &block.instructions {
        lower_inst::lower_instruction(ctx, *inst_id)?;
    }
    let terminator = block
        .terminator
        .as_ref()
        .ok_or_else(|| CodegenIrError::invalid_module(format!("block '{}' has no terminator", block.name)))?;
    lower_term::lower_terminator(ctx, terminator)
}
