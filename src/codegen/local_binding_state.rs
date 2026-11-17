//! Purpose:
//! Materializes explicit EIR binding presence for native eval scope synchronization.
//!
//! Called from:
//! - Frame prologues, SetLocalBindingState lowering and eval snapshot/reload helpers.
//!
//! Key details:
//! - Zero-filled storage is never evidence that a PHP variable was initialized.
//! - All operations use the shared target ABI helpers and frame-assigned state offsets.

use super::{abi, context::FunctionContext, CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, LocalSlotId};

/// Starts native binding flags unbound before EIR parameter and source initialization events.
pub(super) fn initialize(ctx: &mut FunctionContext<'_>) {
    let mut offsets = ctx.binding_state_offsets.values().copied().collect::<Vec<_>>();
    offsets.sort_unstable();
    for offset in offsets { abi::emit_store_zero_to_local_slot(ctx.emitter, offset); }
}

/// Executes the source binding event without changing the local value or reference identity.
pub(super) fn lower(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return Err(CodegenIrError::invalid_module("binding state requires a source local slot"));
    };
    let value = inst.operands.first().copied().ok_or_else(||
        CodegenIrError::invalid_module("binding state requires a boolean value"))?;
    let offset = ctx.binding_state_offsets.get(&slot).copied().ok_or_else(||
        CodegenIrError::invalid_module("source binding has no frame state"))?;
    ctx.load_value_to_result(value)?;
    abi::store_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    Ok(())
}

/// Skips a native snapshot when its PHP variable is not bound on the executed source path.
pub(super) fn branch_if_uninitialized(ctx: &mut FunctionContext<'_>, slot: LocalSlotId, label: &str) {
    if let Some(offset) = ctx.binding_state_offsets.get(&slot).copied() {
        abi::load_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
        abi::emit_branch_if_int_result_zero(ctx.emitter, label);
    }
}

/// Records a successful scope reload or detach without perturbing the current value registers.
pub(super) fn mark(ctx: &mut FunctionContext<'_>, slot: LocalSlotId, initialized: bool) {
    if let Some(offset) = ctx.binding_state_offsets.get(&slot).copied() {
        if initialized {
            let scratch = abi::secondary_scratch_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, scratch, 1);
            abi::store_at_offset(ctx.emitter, scratch, offset);
        } else { abi::emit_store_zero_to_local_slot(ctx.emitter, offset); }
    }
}

#[cfg(test)]
mod tests {
    use crate::{codegen::{frame, platform::Target}, ir::{Builder, Function, IrType, LocalKind, Terminator}, types::PhpType};

    /// Presence words have independent frame storage even for a null binding with no payload words.
    #[test]
    fn binding_presence_frame_words_are_distinct_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut function = Function::new("bindingPresence".into(), IrType::Void, PhpType::Void);
            function.flags.tracks_local_binding_state = true;
            for (name, ty) in [("integer", PhpType::Int), ("nil", PhpType::Void),
                ("value", PhpType::Mixed), ("hidden#gen", PhpType::Mixed)] {
                function.add_local(Some(name.into()), IrType::from_php(&ty), ty, LocalKind::PhpLocal);
            }
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry); builder.position_at_end(entry);
            builder.terminate(Terminator::Return { value: None });
            let layout = frame::layout_for_function(&function, Target::parse(name).unwrap(), false, false, false);
            let mut offsets = layout.binding_state_offsets.values().copied().collect::<Vec<_>>();
            offsets.sort_unstable(); offsets.dedup();
            assert_eq!(offsets.len(), 3, "{name}: user bindings including null, no generated temp");
            for offset in offsets {
                assert!(!layout.local_offsets.values().any(|local| *local == offset), "{name}");
                assert!(offset < layout.frame_size, "{name}");
            }
        }
    }
}
