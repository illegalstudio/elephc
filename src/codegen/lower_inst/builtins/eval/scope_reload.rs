//! Purpose:
//! Restores locals and globals from boxed eval scope cells.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Ownership retention and missing-entry fallbacks remain type-aware.

use super::*;

/// Converts a scope Mixed cell back to the local's native storage type.
pub(super) fn store_mixed_scope_cell_to_local(
    ctx: &mut FunctionContext<'_>,
    local: &EvalSyncLocal,
) -> Result<()> {
    match local.ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
            replace_owned_eval_local(ctx, local)?;
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
            replace_owned_eval_local(ctx, local)?;
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
            replace_owned_eval_local(ctx, local)?;
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
) -> Result<()> {
    let symbol = ir_global_symbol(&global.name);
    let ty = global.ty.codegen_repr();
    ctx.data.add_comm(symbol.clone(), ty.stack_size().max(8));
    match &ty {
        PhpType::Mixed | PhpType::Union(_) => {
            replace_eval_mixed_global(ctx, &symbol, true);
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
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Str, false);
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
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &ty, false);
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

/// Publishes an independent global owner before retiring displaced storage, including identical cells.
fn replace_eval_mixed_global(ctx: &mut FunctionContext<'_>, symbol: &str, borrowed: bool) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_symbol_to_result(ctx.emitter, symbol, &PhpType::Mixed);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
    if borrowed {
        // Both owned and borrowed scope entries are borrowed by the native reload operation.
        abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    }
    abi::emit_store_result_to_symbol(ctx.emitter, symbol, &PhpType::Mixed, false);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
}

/// Publishes an acquired replacement before releasing the old raw or reference-cell payload.
fn replace_owned_eval_local(ctx: &mut FunctionContext<'_>, local: &EvalSyncLocal) -> Result<()> {
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
        abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    } else {
        abi::emit_decref_if_refcounted(ctx.emitter, &ty);
    }
    ctx.emitter.comment("eval local replacement owns its native payload");
    Ok(())
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
) -> Result<()> {
    match local.ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(ctx.emitter, &symbol);
            replace_owned_eval_local(ctx, local)?;
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
            replace_owned_eval_local(ctx, local)?;
        }
        PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            // Heap-pointer locals fall back to the null pointer when eval
            // removed the entry, matching the object fallback.
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            replace_owned_eval_local(ctx, local)?;
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

/// Stores the program-global fallback for a missing eval global entry.
pub(super) fn store_missing_scope_entry_to_global(
    ctx: &mut FunctionContext<'_>,
    global: &EvalSyncGlobal,
) -> Result<()> {
    let symbol = ir_global_symbol(&global.name);
    let ty = global.ty.codegen_repr();
    ctx.data.add_comm(symbol.clone(), ty.stack_size().max(8));
    match &ty {
        PhpType::Mixed | PhpType::Union(_) => {
            let symbol_name = ctx.emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(ctx.emitter, &symbol_name);
            replace_eval_mixed_global(ctx, &symbol, false);
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
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Str, false);
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &ty, false);
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
