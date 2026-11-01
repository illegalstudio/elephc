//! Purpose:
//! Restores locals and globals from boxed eval scope cells.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Ownership retention and missing-entry fallbacks remain type-aware.
//! - Global reload publishes the replacement before releasing the prior owner.

use super::*;

/// Converts a scope Mixed cell back to the local's native storage type.
pub(super) fn store_mixed_scope_cell_to_local(
    ctx: &mut FunctionContext<'_>,
    local: &EvalSyncLocal,
    pending_throw: Option<usize>,
) -> Result<()> {
    match local.ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            if ctx.local_stores_ref_cell_pointer(local.slot) {
                // `$r =& $h` in the fragment turns `$h`'s entry into an eval reference wrapper.
                // Stored through a reference-bound slot, the wrapper lands in the shared cell
                // itself (a global's, or another local's), and native reads of it see NULL. The
                // cell takes the referenced value; the fragment-side link ends with the eval.
                abi::emit_call_label(ctx.emitter, "__rt_mixed_deref");
            }
            abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
            replace_owned_eval_local(ctx, local, pending_throw)?;
        }
        PhpType::Int => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Bool => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Float => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Str => {
            let string = ctx.next_label("eval_reload_string_payload");
            let persist = ctx.next_label("eval_reload_persist_string");
            abi::emit_owned_mixed_string(ctx.emitter, &string, &persist);
            replace_owned_eval_local(ctx, local, pending_throw)?;
        }
        PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            // Objects, arrays, and hashes are heap pointers boxed in the
            // scope cell; unbox and store the raw payload pointer.
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            let payload_reg = match ctx.emitter.target.arch {
                Arch::AArch64 => "x1",
                Arch::X86_64 => "rdi",
            };
            let result_reg = abi::int_result_reg(ctx.emitter);
            ctx.emitter
                .instruction(&format!("mov {}, {}", result_reg, payload_reg)); // move the unboxed heap pointer into the local-store result register
            abi::emit_incref_if_refcounted(ctx.emitter, &local.ty);
            replace_owned_eval_local(ctx, local, pending_throw)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "eval scope reload for PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Converts a scope Mixed cell back to a program-global storage symbol.
pub(super) fn store_mixed_scope_cell_to_global(
    ctx: &mut FunctionContext<'_>,
    global: &EvalSyncGlobal,
    pending_throw: Option<usize>,
) -> Result<()> {
    let symbol = ir_global_symbol(&global.name);
    let ty = global.ty.codegen_repr();
    ctx.data.add_comm(symbol.clone(), ty.stack_size().max(8));
    match &ty {
        PhpType::Mixed | PhpType::Union(_) => {
            if ctx.shared.uses_shared_ref_cell(ctx.module, &global.name) {
                // The replaced owner is released even if this is the same borrowed cell.
                // Acquire the new share first, and retire the old one on the guarded path.
                abi::emit_call_label(ctx.emitter, "__rt_incref");
                super::super::super::globals_constants::lower_replace_shared_global(
                    ctx, &symbol, &PhpType::Mixed,
                )?;
                retire_eval_replacement_owner(ctx.emitter, &PhpType::Mixed, pending_throw);
                return Ok(());
            }
            abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
            replace_owned_eval_global(ctx.emitter, &symbol, &ty, pending_throw);
        }
        PhpType::Int => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Int, false);
        }
        PhpType::Bool => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Bool, false);
        }
        PhpType::Float => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Float, false);
        }
        PhpType::Str => {
            let string = ctx.next_label("eval_reload_global_string_payload");
            let persist = ctx.next_label("eval_reload_global_persist_string");
            abi::emit_owned_mixed_string(ctx.emitter, &string, &persist);
            replace_owned_eval_global(ctx.emitter, &symbol, &ty, pending_throw);
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            let payload_reg = match ctx.emitter.target.arch {
                Arch::AArch64 => "x1",
                Arch::X86_64 => "rdi",
            };
            let result_reg = abi::int_result_reg(ctx.emitter);
            ctx.emitter
                .instruction(&format!("mov {}, {}", result_reg, payload_reg)); // move the unboxed array payload into the ABI result register
            abi::emit_incref_if_refcounted(ctx.emitter, &ty);
            if ctx.shared.uses_shared_ref_cell(ctx.module, &global.name) {
                super::super::super::globals_constants::lower_replace_shared_global(
                    ctx,
                    &symbol,
                    &ty,
                )?;
                retire_eval_replacement_owner(ctx.emitter, &ty, pending_throw);
                return Ok(());
            }
            replace_owned_eval_global(ctx.emitter, &symbol, &ty, pending_throw);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "eval global reload for PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Publishes an acquired global owner before retiring its old payload, including identical pointers.
fn replace_owned_eval_global(
    emitter: &mut crate::codegen_support::emit::Emitter,
    symbol: &str,
    ty: &PhpType,
    pending_throw: Option<usize>,
) {
    emitter.comment("publish eval global replacement before retiring the previous owner");
    abi::emit_push_result_value(emitter, ty);
    abi::emit_load_symbol_to_result(emitter, symbol, ty);
    abi::emit_push_result_value(emitter, ty);
    if *ty == PhpType::Str {
        let (ptr, len) = abi::string_result_regs(emitter);
        abi::emit_load_temporary_stack_slot(emitter, ptr, 16);
        abi::emit_load_temporary_stack_slot(emitter, len, 24);
    } else {
        abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    }
    abi::emit_store_result_to_symbol(emitter, symbol, ty, false);
    // Only the retired pointer is needed for release, including a string's first saved word.
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_release_temporary_stack(emitter, 32);
    retire_eval_replacement_owner(emitter, ty, pending_throw);
    emitter.comment("eval global replacement owns its native payload");
}

/// Publishes an acquired replacement before releasing the old raw or reference-cell payload.
fn replace_owned_eval_local(
    ctx: &mut FunctionContext<'_>,
    local: &EvalSyncLocal,
    pending_throw: Option<usize>,
) -> Result<()> {
    let ty = local.ty.codegen_repr();
    ctx.emitter.comment("publish eval local replacement before retiring the previous owner");
    abi::emit_push_result_value(ctx.emitter, &ty);
    ctx.load_local_to_result(local.slot)?;
    abi::emit_push_result_value(ctx.emitter, &ty);
    load_saved_eval_local_result(ctx, &ty, 16);
    ctx.store_current_result_to_local(local.slot)?;
    load_saved_eval_local_result(ctx, &ty, 0);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    if ty == PhpType::Str {
        let (ptr, _) = abi::string_result_regs(ctx.emitter);
        abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), ptr);
    }
    retire_eval_replacement_owner(ctx.emitter, &ty, pending_throw);
    ctx.emitter.comment("eval local replacement owns its native payload");
    Ok(())
}

/// Retires one published replacement's old owner without interrupting guarded scope writeback.
pub(super) fn retire_eval_replacement_owner(
    emitter: &mut crate::codegen_support::emit::Emitter,
    ty: &PhpType,
    pending_throw: Option<usize>,
) {
    let entry = if *ty == PhpType::Str {
        Some("__rt_heap_free_safe")
    } else {
        abi::refcount_release_helper(ty)
    };
    let Some(entry) = entry else { return; };
    if let Some(offset) = pending_throw {
        let payload = abi::int_arg_reg_name(emitter.target, 1);
        abi::emit_reg_move(emitter, payload, abi::int_result_reg(emitter));
        abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 0), entry);
        abi::emit_temporary_stack_address(emitter, abi::int_arg_reg_name(emitter.target, 2), offset);
        abi::emit_call_label(emitter, "__rt_cleanup_invoke");
    } else {
        abi::emit_call_label(emitter, entry);
    }
}

/// Restores a heap pointer or string pair from a saved eval replacement operand.
fn load_saved_eval_local_result(ctx: &mut FunctionContext<'_>, ty: &PhpType, offset: usize) {
    if *ty == PhpType::Str {
        let (ptr, len) = abi::string_result_regs(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, ptr, offset);
        abi::emit_load_temporary_stack_slot(ctx.emitter, len, offset + 8);
    } else {
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    }
}

/// Stores the local fallback used when eval unsets or removes a synchronized local.
pub(super) fn store_missing_scope_entry_to_local(
    ctx: &mut FunctionContext<'_>,
    local: &EvalSyncLocal,
    pending_throw: Option<usize>,
) -> Result<()> {
    // A reference-bound local (`global $g`, `$a = &$b`) shares its cell with another name, and
    // eval's `unset($g)` only broke that binding. Writing the null fallback through the cell
    // cleared the global or the referenced variable itself. The binding is kept instead: a later
    // read of the name in this frame still sees the shared value, where php would see it unset.
    if ctx.local_ref_cell_representation_is_definite(local.slot) {
        return Ok(());
    }
    // A slot promoted at run time (`global $g` in a function) carries its state in a flag.
    let still_bound = ctx.next_label("eval_scope_missing_keeps_reference");
    let skip_when_bound = ctx.ref_cell_state_offset(local.slot);
    if let Some(state_offset) = skip_when_bound {
        let state_reg = abi::secondary_scratch_reg(ctx.emitter);
        abi::load_at_offset(ctx.emitter, state_reg, state_offset);
        match ctx.emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cbnz {state_reg}, {still_bound}")); // a reference binding is only broken, never written through
            }
            crate::codegen::platform::Arch::X86_64 => {
                ctx.emitter.instruction(&format!("test {state_reg}, {state_reg}")); // is the slot a promoted reference?
                ctx.emitter.instruction(&format!("jne {still_bound}"));          // a reference binding is only broken, never written through
            }
        }
    }
    store_missing_scope_entry_to_unbound_local(ctx, local, pending_throw)?;
    if skip_when_bound.is_some() {
        ctx.emitter.label(&still_bound);
    }
    Ok(())
}

/// Stores the null fallback into a local slot that is not a reference binding.
fn store_missing_scope_entry_to_unbound_local(
    ctx: &mut FunctionContext<'_>,
    local: &EvalSyncLocal,
    pending_throw: Option<usize>,
) -> Result<()> {
    let ty = local.ty.codegen_repr();
    if (ty == PhpType::Str || ty.is_refcounted())
        && !ctx.local_ref_cell_representation_is_definite(local.slot)
    {
        // Removing the eval entry does not consume the raw native slot's owner.
        // That slot still owns its previous payload. Use the existing
        // cleanup guard so a runtime-promoted reference is not treated as raw.
        let offset = ctx.local_offset(local.slot)?;
        crate::codegen::frame::emit_owned_local_cleanup(ctx, local.slot, offset, &ty);
    }
    match local.ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            ensure_eval_local_writeback_owns_value(ctx, local)?;
            ctx.release_local_before_refcounted_writeback(local.slot)?;
            let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(ctx.emitter, &symbol);
            replace_owned_eval_local(ctx, local, pending_throw)?;
        }
        PhpType::Int | PhpType::Bool => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Float => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
            replace_owned_eval_local(ctx, local, pending_throw)?;
        }
        PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            // Heap-pointer locals fall back to the null pointer when eval
            // removed the entry, matching the object fallback.
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            replace_owned_eval_local(ctx, local, pending_throw)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "eval scope missing reload for PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Rejects a reload whose destination has no owner for its stored Mixed value.
fn ensure_eval_local_writeback_owns_value(
    ctx: &FunctionContext<'_>,
    local: &EvalSyncLocal,
) -> Result<()> {
    if ctx.owns_eval_local_writeback_target(local.slot) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "eval scope reload target ${} has no local owner",
        local.name
    )))
}



/// Stores the program-global fallback for a missing eval global entry.
pub(super) fn store_missing_scope_entry_to_global(
    ctx: &mut FunctionContext<'_>,
    global: &EvalSyncGlobal,
    pending_throw: Option<usize>,
) -> Result<()> {
    if ctx.shared.uses_shared_ref_cell(ctx.module, &global.name)
        && !crate::superglobals::is_superglobal(&global.name)
    {
        super::super::super::globals_constants::unset_global_name(ctx, &global.name);
        return Ok(());
    }
    let symbol = ir_global_symbol(&global.name);
    let ty = global.ty.codegen_repr();
    ctx.data.add_comm(symbol.clone(), ty.stack_size().max(8));
    match &ty {
        PhpType::Mixed | PhpType::Union(_) => {
            let symbol_name = ctx.emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(ctx.emitter, &symbol_name);
            replace_owned_eval_global(ctx.emitter, &symbol, &ty, pending_throw);
        }
        PhpType::Int => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Int, false);
        }
        PhpType::Bool => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Bool, false);
        }
        PhpType::Float => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Float, false);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
            replace_owned_eval_global(ctx.emitter, &symbol, &ty, pending_throw);
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            if ctx.shared.uses_shared_ref_cell(ctx.module, &global.name) {
                return super::super::super::globals_constants::lower_store_shared_global(
                    ctx,
                    &symbol,
                    &ty,
                );
            }
            replace_owned_eval_global(ctx.emitter, &symbol, &ty, pending_throw);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "eval global missing reload for PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::replace_owned_eval_global;
    use crate::codegen_support::{emit::Emitter, platform::Target};
    use crate::types::PhpType;

    /// Guarded writeback routes every retired heap representation through the non-escaping boundary.
    #[test]
    fn eval_global_replacement_accumulates_cleanup_throws_on_all_targets() {
        let cases = [
            PhpType::Mixed, PhpType::Str, PhpType::Object("Exception".into()),
            PhpType::Array(Box::new(PhpType::Mixed)),
            PhpType::AssocArray { key: Box::new(PhpType::Mixed), value: Box::new(PhpType::Mixed) },
        ];
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for ty in &cases {
                let mut emitter = Emitter::new(Target::parse(target).unwrap());
                replace_owned_eval_global(&mut emitter, "_eval_guarded_reload_global", ty, Some(88));
                let asm = emitter.output();
                let publication = asm.rfind("_eval_guarded_reload_global").unwrap();
                let cleanup = asm.find("__rt_cleanup_invoke").unwrap();
                assert!(publication < cleanup, "{target}: {ty}: {asm}");
                let pending: &[&str] = if target == "linux-x86_64" {
                    &["lea rdx, [rsp + 88]"]
                } else {
                    // `emit_sp_address` forms a near stack-slot address in one instruction.
                    &["add x2, sp, #88"]
                };
                for &instruction in pending {
                    assert!(asm.contains(instruction), "{target}: {asm}");
                    assert!(asm.find(instruction).unwrap() < cleanup, "{target}: {asm}");
                }
                assert!(!asm.contains("__rt_throw_current"), "{target}: {asm}");
            }
        }
    }

    /// Every supported target publishes global replacements before releasing the old typed owner.
    #[test]
    fn eval_global_replacement_retires_all_heap_storage_types_after_publication() {
        let cases = [
            (PhpType::Mixed, "__rt_decref_mixed"),
            (PhpType::Str, "__rt_heap_free_safe"),
            // `array<mixed>` may have been promoted to hash storage, so it releases by heap kind.
            (PhpType::Array(Box::new(PhpType::Mixed)), "__rt_decref_any"),
            (PhpType::AssocArray { key: Box::new(PhpType::Str), value: Box::new(PhpType::Str) }, "__rt_decref_hash"),
        ];
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for (ty, release) in &cases {
                let mut emitter = Emitter::new(Target::parse(target).unwrap());
                replace_owned_eval_global(&mut emitter, "_eval_reload_test_global", ty, None);
                let asm = emitter.output();
                let publication = asm.rfind("_eval_reload_test_global").unwrap();
                let cleanup = asm.find(release).unwrap();
                assert!(publication < cleanup, "{target}: {ty}: {asm}");
                assert_eq!(asm.matches(release).count(), 1, "{target}: {ty}: {asm}");
                let restored = if target == "linux-x86_64" { "add rsp, 32" } else { "add sp, sp, #32" };
                assert!(asm.find(restored).unwrap() < cleanup, "{target}: cleanup must not strand temporary stack owners");
                assert!(!asm.contains("__rt_incref"), "{target}: the replacement already owns its payload");
            }
        }
    }
}
