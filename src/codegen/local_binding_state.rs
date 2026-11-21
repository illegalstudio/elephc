//! Purpose:
//! Materializes explicit EIR binding presence for native eval scope synchronization.
//!
//! Called from:
//! - Frame prologues, SetLocalBindingState lowering and eval snapshot/reload helpers.
//!
//! Key details:
//! - Zero means unbound; native and eval origins use distinct nonzero words.
//! - Ordinary writes and reloads preserve the current origin until the binding is unset.
//! - All operations use the shared target ABI helpers and frame-assigned state offsets.

use super::{abi, context::FunctionContext, CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, LocalSlotId, Op};
use crate::codegen::platform::Arch;

const NATIVE_ORIGIN: i64 = 1;
const EVAL_ORIGIN: i64 = 2;

/// Starts native binding flags unbound before EIR parameter and source initialization events.
pub(super) fn initialize(ctx: &mut FunctionContext<'_>) {
    let mut offsets = ctx.binding_state_offsets.values().copied().collect::<Vec<_>>();
    offsets.sort_unstable();
    for offset in offsets { abi::emit_store_zero_to_local_slot(ctx.emitter, offset); }
    // Process entry initializes these payloads before the first source instruction.
    if ctx.function.flags.is_main && !ctx.module.web {
        for name in ["argc", "argv"] {
            let slot = ctx.function.locals.iter().find(|local| local.name.as_deref() == Some(name))
                .map(|local| local.id);
            if let Some(offset) = slot.and_then(|slot| ctx.binding_state_offsets.get(&slot).copied()) {
                let scratch = abi::secondary_scratch_reg(ctx.emitter);
                abi::emit_load_int_immediate(ctx.emitter, scratch, NATIVE_ORIGIN);
                abi::store_at_offset(ctx.emitter, scratch, offset);
            }
        }
    }
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
    let store = ctx.next_label("binding_native_store");
    let done = ctx.next_label("binding_native_keep_origin");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &store);
    let current = abi::secondary_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, current, offset);
    abi::emit_branch_if_zero_wide(ctx.emitter, current, &store);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&store);
    abi::store_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    ctx.emitter.label(&done);
    Ok(())
}

/// Reads only native provenance while eval-created bindings retain their independent source.
pub(super) fn lower_get(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return Err(CodegenIrError::invalid_module("binding state requires a source local slot"));
    };
    let offset = ctx.binding_state_offsets.get(&slot).copied().ok_or_else(||
        CodegenIrError::invalid_module("source binding has no frame state"))?;
    let result = inst.result.ok_or_else(|| CodegenIrError::invalid_module("binding state read has no result"))?;
    let register = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, register, offset);
    match (ctx.emitter.target.arch, inst.op) {
        (Arch::AArch64, Op::GetEvalBindingState) => ctx.emitter.instruction(&format!("lsr {register}, {register}, #1")), // normalize the eval-origin bit to Bool
        (Arch::X86_64, Op::GetEvalBindingState) => ctx.emitter.instruction(&format!("shr {register}, 1")), // native and absent states both become false
        (Arch::AArch64, _) => ctx.emitter.instruction(&format!("and {register}, {register}, #{NATIVE_ORIGIN}")), // exclude eval-created binding origins
        (Arch::X86_64, _) => ctx.emitter.instruction(&format!("and {register}, {NATIVE_ORIGIN}")), // return only the native-origin bit as Bool
    }
    ctx.store_result_value(result)
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
            let create = ctx.next_label("binding_eval_create_origin");
            let done = ctx.next_label("binding_eval_preserve_origin");
            let scratch = abi::secondary_scratch_reg(ctx.emitter);
            abi::load_at_offset(ctx.emitter, scratch, offset);
            abi::emit_branch_if_zero_wide(ctx.emitter, scratch, &create);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&create);
            abi::emit_load_int_immediate(ctx.emitter, scratch, EVAL_ORIGIN);
            abi::store_at_offset(ctx.emitter, scratch, offset);
            ctx.emitter.label(&done);
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

    /// Real assemblers accept native/eval origin updates and native-origin reads on every ABI.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn binding_origin_state_operations_assemble_on_every_target() {
        use crate::codegen::{context::FunctionContext, data_section::DataSection, emit::Emitter,
            shared_state::SharedCodegenState};
        use crate::ir::{Immediate, Module, Op, Ownership};
        use std::{io::Write, process::{Command, Stdio}};
        for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"),
            ("linux-aarch64", "aarch64-linux-gnu"), ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13"), ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
            let target = Target::parse(name).unwrap();
            let module = Module::new(target);
            let mut function = Function::new("originFlags".into(), IrType::I64, PhpType::Bool);
            function.flags.tracks_local_binding_state = true;
            let slot = function.add_local(Some("value".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry); builder.position_at_end(entry);
            let value = builder.emit(Op::ConstBool, Vec::new(), Some(Immediate::Bool(true)),
                IrType::I64, PhpType::Bool, Ownership::NonHeap).unwrap();
            builder.emit(Op::SetLocalBindingState, vec![value], Some(Immediate::LocalSlot(slot)),
                IrType::Void, PhpType::Void, Ownership::NonHeap);
            let result = builder.emit(Op::GetNativeBindingState, Vec::new(), Some(Immediate::LocalSlot(slot)),
                IrType::I64, PhpType::Bool, Ownership::NonHeap).unwrap();
            let eval_result = builder.emit(Op::GetEvalBindingState, Vec::new(), Some(Immediate::LocalSlot(slot)),
                IrType::I64, PhpType::Bool, Ownership::NonHeap).unwrap();
            let combined = builder.emit(Op::IBitOr, vec![result, eval_result], None,
                IrType::I64, PhpType::Bool, Ownership::NonHeap).unwrap();
            builder.terminate(Terminator::Return { value: Some(combined) });
            let layout = frame::layout_for_function(&function, target, false, false, false);
            let mut emitter = Emitter::new(target); let mut data = DataSection::new();
            let mut shared = SharedCodegenState::default();
            {
                let mut ctx = FunctionContext::new(&module, &function, &mut emitter, &mut data,
                    &mut shared, layout, false, false, false, None);
                super::abi::emit_load_int_immediate(ctx.emitter, super::abi::int_result_reg(ctx.emitter), 1);
                ctx.store_result_value(value).unwrap();
                super::lower(&mut ctx, &function.instructions[1]).unwrap();
                super::mark(&mut ctx, slot, true);
                super::mark(&mut ctx, slot, false);
                super::mark(&mut ctx, slot, true);
                super::lower_get(&mut ctx, &function.instructions[2]).unwrap();
                super::lower_get(&mut ctx, &function.instructions[3]).unwrap();
            }
            let syntax = if target.arch == crate::codegen::platform::Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
            let assembly = format!("{syntax}.text\n{}", emitter.output());
            let mut process = Command::new("clang").args(["-target", triple, "-x", "assembler", "-c", "-o", "/dev/null", "-"])
                .stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("clang is required for explicit target validation");
            process.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
            let output = process.wait_with_output().unwrap();
            assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }

}
