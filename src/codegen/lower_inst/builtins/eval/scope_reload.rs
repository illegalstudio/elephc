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
            ensure_eval_local_writeback_owns_value(ctx, local)?;
            let unchanged = ctx.next_label("eval_scope_reload_unchanged");
            emit_branch_if_scope_cell_matches_local(ctx, local, &unchanged)?;
            let result_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.release_local_before_refcounted_writeback(local.slot)?;
            abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
            ctx.store_current_result_to_local(local.slot)?;
            ctx.emitter.label(&unchanged);
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
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            ctx.store_current_result_to_local(local.slot)?;
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
            ctx.store_current_result_to_local(local.slot)?;
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
            emit_retain_scope_cell_if_owned(ctx);
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Mixed, false);
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

/// Retains a scope-owned Mixed cell before storing it into a native local owner.
pub(super) fn emit_retain_scope_cell_if_owned(ctx: &mut FunctionContext<'_>) {
    let flags_reg = abi::secondary_scratch_reg(ctx.emitter);
    let skip = ctx.next_label("eval_scope_reload_borrowed");
    abi::emit_load_temporary_stack_slot(ctx.emitter, flags_reg, 8);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("tst {}, #{}", flags_reg, EVAL_SCOPE_FLAG_OWNED)); // check whether the scope keeps its own Mixed-cell owner
            ctx.emitter.instruction(&format!("b.eq {}", skip));                 // borrowed scope entries can be copied back without retaining
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", flags_reg, EVAL_SCOPE_FLAG_OWNED)); // check whether the scope keeps its own Mixed-cell owner
            ctx.emitter.instruction(&format!("je {}", skip));                   // borrowed scope entries can be copied back without retaining
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&skip);
}

/// Stores the local fallback used when eval unsets or removes a synchronized local.
pub(super) fn store_missing_scope_entry_to_local(
    ctx: &mut FunctionContext<'_>,
    local: &EvalSyncLocal,
) -> Result<()> {
    match local.ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            ensure_eval_local_writeback_owns_value(ctx, local)?;
            ctx.release_local_before_refcounted_writeback(local.slot)?;
            let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(ctx.emitter, &symbol);
            ctx.store_current_result_to_local(local.slot)?;
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
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            // Heap-pointer locals fall back to the null pointer when eval
            // removed the entry, matching the object fallback.
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            ctx.store_current_result_to_local(local.slot)?;
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

/// Branches when the scope cell is already the exact value held by the local storage.
fn emit_branch_if_scope_cell_matches_local(
    ctx: &mut FunctionContext<'_>,
    local: &EvalSyncLocal,
    label: &str,
) -> Result<()> {
    ctx.load_local_to_result(local.slot)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let scope_cell_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, scope_cell_reg, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", result_reg, scope_cell_reg)); // compare the current local owner with the fetched scope cell
            ctx.emitter.instruction(&format!("b.eq {}", label));                // preserve an unchanged owner without another retain
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", result_reg, scope_cell_reg)); // compare the current local owner with the fetched scope cell
            ctx.emitter.instruction(&format!("je {}", label));                  // preserve an unchanged owner without another retain
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
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Mixed, false);
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
