//! Purpose:
//! Computes and emits stack-frame setup/teardown for the EIR backend.
//! Reuses the target-aware ABI frame helpers from the legacy backend.
//!
//! Called from:
//! - `crate::codegen_ir::block_emit`.
//!
//! Key details:
//! - Frame size is value-placement bytes plus the target frame footer, rounded to 16 bytes.
//! - Main currently exits through the process syscall, matching the legacy entry path.

use std::collections::HashMap;

use crate::codegen::abi;
use crate::codegen::context::TRY_HANDLER_SLOT_SIZE;
use crate::codegen::platform::Arch;
use crate::ir::{Function, Immediate, LocalSlotId, Op};
use crate::names::function_symbol;

use super::context::FunctionContext;
use super::value_placement::{self, ValuePlacement};

const FRAME_FOOTER_BYTES: usize = 16;

/// Complete fixed frame layout for Phase 04 spill slots and addressable locals.
pub(super) struct FrameLayout {
    pub(super) value_placement: ValuePlacement,
    pub(super) local_offsets: HashMap<LocalSlotId, usize>,
    pub(super) try_handler_offsets: HashMap<i64, usize>,
    pub(super) frame_size: usize,
}

/// Computes fixed stack slots for SSA values and EIR locals.
pub(super) fn layout_for_function(function: &Function) -> FrameLayout {
    let value_placement = value_placement::allocate(function);
    let mut local_offsets = HashMap::new();
    let mut offset = value_placement.total_slot_bytes;
    for local in &function.locals {
        let bytes = value_placement::bytes_for(local.ir_type)
            .max(local.php_type.codegen_repr().stack_size());
        if bytes == 0 {
            continue;
        }
        offset += bytes;
        local_offsets.insert(local.id, offset);
    }
    let mut try_handler_offsets = HashMap::new();
    for token in try_handler_tokens(function) {
        offset += TRY_HANDLER_SLOT_SIZE;
        try_handler_offsets.insert(token, offset);
    }
    let frame_size = align_to_16(offset + FRAME_FOOTER_BYTES);
    FrameLayout {
        value_placement,
        local_offsets,
        try_handler_offsets,
        frame_size,
    }
}

/// Returns the unique try-handler tokens used by EIR `try_push_handler` opcodes.
fn try_handler_tokens(function: &Function) -> Vec<i64> {
    let mut tokens = Vec::new();
    for inst in &function.instructions {
        if inst.op != Op::TryPushHandler {
            continue;
        }
        let Some(Immediate::I64(token)) = inst.immediate else {
            continue;
        };
        if !tokens.contains(&token) {
            tokens.push(token);
        }
    }
    tokens
}

/// Emits the process-entry prologue for the EIR main function.
pub(super) fn emit_main_prologue(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::AArch64 {
        ctx.emitter.raw(".align 2");
    }
    ctx.emitter.blank();
    ctx.emitter.entry_label();
    abi::emit_frame_prologue(ctx.emitter, ctx.frame_size);
    ctx.emitter.comment("save argc/argv to globals");
    abi::emit_store_process_args_to_globals(ctx.emitter);
    store_argc_local_if_present(ctx);
}

/// Emits a direct-callable user function prologue and stores incoming params.
pub(super) fn emit_function_prologue(ctx: &mut FunctionContext<'_>) -> crate::codegen_ir::Result<()> {
    let label = function_symbol(&ctx.function.name);
    emit_function_prologue_with_label(ctx, &label)
}

/// Emits a callable function prologue using an already-resolved entry label.
pub(super) fn emit_function_prologue_with_label(
    ctx: &mut FunctionContext<'_>,
    entry_label: &str,
) -> crate::codegen_ir::Result<()> {
    if ctx.emitter.target.arch == Arch::AArch64 {
        ctx.emitter.raw(".align 2");
    }
    ctx.emitter.blank();
    ctx.emitter.label_global(entry_label);
    abi::emit_frame_prologue(ctx.emitter, ctx.frame_size);
    let mut incoming_args = abi::IncomingArgCursor::for_target(ctx.emitter.target, 0);
    for (index, param) in ctx.function.params.iter().enumerate() {
        let slot = LocalSlotId::from_raw(index as u32);
        let offset = ctx.local_offset(slot)?;
        abi::emit_store_incoming_param(
            ctx.emitter,
            &param.name,
            &param.php_type,
            offset,
            param.by_ref,
            &mut incoming_args,
        );
    }
    Ok(())
}

/// Emits frame teardown and exits the process with status 0.
pub(super) fn emit_main_epilogue(ctx: &mut FunctionContext<'_>) {
    if ctx.epilogue_emitted {
        return;
    }
    ctx.emitter.blank();
    ctx.emitter.comment("epilogue + exit(0)");
    abi::emit_frame_restore(ctx.emitter, ctx.frame_size);
    abi::emit_exit(ctx.emitter, 0);
    ctx.epilogue_emitted = true;
}

/// Emits the shared epilogue for a direct-callable user function.
pub(super) fn emit_function_epilogue(ctx: &mut FunctionContext<'_>) {
    if ctx.epilogue_emitted {
        return;
    }
    let label = ctx
        .epilogue_label
        .clone()
        .expect("codegen_ir bug: user function has no epilogue label");
    ctx.emitter.label(&label);
    abi::emit_frame_restore(ctx.emitter, ctx.frame_size);
    abi::emit_return(ctx.emitter);
    ctx.epilogue_emitted = true;
}

/// Rounds a byte count up to a 16-byte stack alignment boundary.
fn align_to_16(bytes: usize) -> usize {
    (bytes + 15) & !15
}

/// Stores the OS argument count into `$argc` when the EIR main function has that local.
fn store_argc_local_if_present(ctx: &mut FunctionContext<'_>) {
    let Some(argc_slot) = ctx
        .function
        .locals
        .iter()
        .find(|local| local.name.as_deref() == Some("argc"))
        .map(|local| local.id)
    else {
        return;
    };
    let Ok(offset) = ctx.local_offset(argc_slot) else {
        return;
    };
    abi::store_at_offset(ctx.emitter, abi::process_argc_reg(ctx.emitter.target), offset);
}
