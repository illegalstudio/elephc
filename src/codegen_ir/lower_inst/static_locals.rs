//! Purpose:
//! Lowers function static-local loads, stores, and one-time initializers for
//! the Phase 04 EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Static locals are backed by `.comm` symbols and an initialization marker,
//!   so their values persist across function calls without using frame slots.
//! - Initializers transfer their freshly-created owner into the static slot;
//!   assignments retain refcounted values before publishing a second owner.

use crate::codegen::abi;
use crate::ir::{Instruction, LocalSlot, LocalSlotId, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_local_slot, expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

/// Resolved function static-local metadata for symbol-backed storage.
struct StaticLocalSlot {
    name: String,
    php_type: PhpType,
    symbol: String,
    init_symbol: String,
}

/// Lowers a static-local read into the current result register(s).
pub(super) fn lower_load_static_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = resolve_static_local_slot(ctx, inst)?;
    ensure_static_local_type_supported(&slot, inst)?;
    abi::emit_load_symbol_to_result(ctx.emitter, &slot.symbol, &slot.php_type);
    store_if_result(ctx, inst)
}

/// Lowers a static-local assignment from one SSA operand into symbol-backed storage.
pub(super) fn lower_store_static_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let slot = resolve_static_local_slot(ctx, inst)?;
    ensure_static_local_type_supported(&slot, inst)?;
    ensure_static_local_value_supported(ctx, &slot, value, inst)?;
    let loaded_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if loaded_ty.is_refcounted() {
        abi::emit_incref_if_refcounted(ctx.emitter, &loaded_ty);
    }
    abi::emit_store_result_to_symbol(ctx.emitter, &slot.symbol, &slot.php_type, true);
    clear_static_local_high_word_if_needed(ctx, &slot);
    Ok(())
}

/// Lowers a static-local declaration initializer guarded by the per-slot marker.
pub(super) fn lower_init_static_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let slot = resolve_static_local_slot(ctx, inst)?;
    ensure_static_local_type_supported(&slot, inst)?;
    ensure_static_local_value_supported(ctx, &slot, value, inst)?;
    let initialized_label = ctx.next_label("static_local_initialized");
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), &slot.init_symbol, 0);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &initialized_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, abi::int_result_reg(ctx.emitter), &slot.init_symbol, 0);
    ctx.load_value_to_result(value)?;
    abi::emit_store_result_to_symbol(ctx.emitter, &slot.symbol, &slot.php_type, false);
    clear_static_local_high_word_if_needed(ctx, &slot);
    ctx.emitter.label(&initialized_label);
    Ok(())
}

/// Resolves a local-slot immediate into static-local symbol metadata.
fn resolve_static_local_slot(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<StaticLocalSlot> {
    let slot = expect_local_slot(inst)?;
    let local = local_slot(ctx, slot)?;
    let name = local.name.clone().ok_or_else(|| {
        CodegenIrError::invalid_module(format!("{} static local is missing a source name", inst.op.name()))
    })?;
    let php_type = local.php_type.codegen_repr();
    let function_fragment = static_local_function_fragment(&ctx.function.name);
    let symbol = format!("_static_{}_{}", function_fragment, name);
    let init_symbol = format!("{}_init", symbol);
    ctx.data.add_comm(symbol.clone(), 16);
    ctx.data.add_comm(init_symbol.clone(), 8);
    // Record this static so the `--web` `__rt_web_reset` routine can release and
    // zero it between requests. Deduped by symbol inside the recorder, so the
    // repeated resolves on every load/store/init of this static cost nothing.
    ctx.data.record_static_local(crate::codegen::data_section::StaticLocalRecord {
        symbol: symbol.clone(),
        init_symbol: init_symbol.clone(),
        php_type: php_type.clone(),
    });
    Ok(StaticLocalSlot {
        name,
        php_type,
        symbol,
        init_symbol,
    })
}

/// Returns the EIR local slot metadata for one static-local instruction.
fn local_slot<'a>(
    ctx: &'a FunctionContext<'_>,
    slot: LocalSlotId,
) -> Result<&'a LocalSlot> {
    ctx.function
        .locals
        .get(slot.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("local slot", slot.as_raw()))
}

/// Verifies that this backend slice knows how to represent the static-local type.
fn ensure_static_local_type_supported(slot: &StaticLocalSlot, inst: &Instruction) -> Result<()> {
    let ty = slot.php_type.codegen_repr();
    if matches!(ty, PhpType::Bool | PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Void)
        || ty.is_refcounted()
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
            "{} for static local ${} with PHP type {:?}",
            inst.op.name(),
            slot.name,
            slot.php_type
    )))
}

/// Verifies the assigned value already has the static-local storage representation.
fn ensure_static_local_value_supported(
    ctx: &FunctionContext<'_>,
    slot: &StaticLocalSlot,
    value: ValueId,
    inst: &Instruction,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    let slot_ty = slot.php_type.codegen_repr();
    if static_local_value_type_matches(&value_ty, &slot_ty) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} assigning PHP type {:?} to static local ${} with PHP type {:?}",
        inst.op.name(),
        value_ty,
        slot.name,
        slot.php_type
    )))
}

/// Returns true when a stored value can use the static-local symbol layout.
fn static_local_value_type_matches(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    if value_ty == slot_ty {
        return true;
    }
    matches!(
        (value_ty, slot_ty),
        (PhpType::Array(value_elem), PhpType::Array(_))
            if matches!(value_elem.codegen_repr(), PhpType::Never | PhpType::Void)
    )
}

/// Clears the unused second word for non-string static-local storage.
fn clear_static_local_high_word_if_needed(ctx: &mut FunctionContext<'_>, slot: &StaticLocalSlot) {
    if !matches!(slot.php_type.codegen_repr(), PhpType::Str | PhpType::TaggedScalar) {
        abi::emit_store_zero_to_symbol(ctx.emitter, &slot.symbol, 8);
    }
}

/// Builds an assembly-safe function fragment for a static-local storage symbol.
fn static_local_function_fragment(name: &str) -> String {
    let mut fragment = String::new();
    for ch in name.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' => fragment.push(ch),
            '_' => fragment.push_str("_u_"),
            '\\' => fragment.push_str("_N_"),
            ':' => fragment.push_str("_C_"),
            _ => fragment.push('_'),
        }
    }
    fragment
}
