//! Purpose:
//! Lowers function static-local loads, stores, and one-time initializers for
//! the Phase 04 EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Static locals are backed by `.comm` symbols and an initialization marker,
//!   so their values persist across function calls without using frame slots.
//! - Initializers transfer their freshly-created owner into the static slot;
//!   assignments receive an already-acquired owner from EIR lowering; the store is a
//!   plain overwrite, mirroring StoreGlobal.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{emit_box_current_value_as_mixed, emit_box_current_owned_value_as_mixed};
use crate::ir::{Instruction, LocalSlot, LocalSlotId, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{coerce_loaded_local_to_result_type, expect_local_slot, expect_operand};
use crate::codegen::{CodegenIrError, Result};

/// Resolved function static-local metadata for symbol-backed storage.
struct StaticLocalSlot {
    name: String,
    php_type: PhpType,
    symbol: String,
    init_symbol: String,
}

/// Lowers a static-local read into the current result register(s).
///
/// If the slot widened to Mixed and the SSA result type is concrete, the loaded
/// Mixed cell is unboxed/coerced to the result type; the coerce is a no-op when
/// storage types already match (i.e. the slot did not widen to Mixed).
pub(super) fn lower_load_static_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = resolve_static_local_slot(ctx, inst)?;
    ensure_static_local_type_supported(&slot, inst)?;
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("load_static_local missing result value")
    })?;
    abi::emit_load_symbol_to_result(ctx.emitter, &slot.symbol, &slot.php_type);
    let result_ty = ctx.value_php_type(result)?;
    let result_owned = matches!(ctx.value_ownership(result)?, crate::ir::Ownership::Owned);
    coerce_loaded_local_to_result_type(ctx, &slot.php_type, &result_ty, result_owned)?;
    ctx.store_result_value(result)
}

/// Lowers a static-local assignment from one SSA operand into symbol-backed storage.
/// EIR lowering acquires ownership for refcounted values (and persists strings),
/// so this store is a plain overwrite, mirroring `lower_store_global`.
///
/// When the slot widened to Mixed but the stored value is a concrete type, the
/// acquired owner is boxed into the Mixed cell representation before the overwrite
/// (the EIR Acquire makes the box net-correct).
pub(super) fn lower_store_static_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let slot = resolve_static_local_slot(ctx, inst)?;
    ensure_static_local_type_supported(&slot, inst)?;
    ensure_static_local_value_supported(ctx, &slot, value, inst)?;
    let source_ty = ctx.load_value_to_result(value)?;
    let mut loaded_ty = source_ty.codegen_repr();
    // Narrow Mixed to Int when the static local slot is Int-typed
    // (from checked integer arithmetic that may overflow to float).
    if matches!(slot.php_type.codegen_repr(), PhpType::Int)
        && matches!(loaded_ty, PhpType::Mixed)
    {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                // x0 already holds the Mixed pointer
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            }
            Arch::X86_64 => {
                // rax holds the Mixed pointer; __rt_mixed_cast_int expects rdi
                ctx.emitter.instruction("mov rdi, rax");                         // move the Mixed pointer into the first SysV argument register
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            }
        }
        // Release the original Mixed box after narrowing.
        // The value SSA still holds the Mixed pointer; reload it and decref.
        // But the narrowed int is in the result reg, so save it first.
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        ctx.load_value_to_result(value)?;
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        loaded_ty = PhpType::Int;
    }
    if loaded_ty.is_refcounted() {
        abi::emit_incref_if_refcounted(ctx.emitter, &loaded_ty);
    }
    box_current_result_for_static_slot(ctx, &slot, value, &source_ty)?;
    abi::emit_store_result_to_symbol(ctx.emitter, &slot.symbol, &slot.php_type, true);
    clear_static_local_high_word_if_needed(ctx, &slot);
    Ok(())
}

/// Lowers a static-local declaration initializer guarded by the per-slot marker.
///
/// When the slot widened to Mixed but the initializer is a concrete type, the
/// result is boxed into the Mixed cell representation before the one-time store;
/// the marker guard ensures the box runs at most once per request.
pub(super) fn lower_init_static_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let slot = resolve_static_local_slot(ctx, inst)?;
    ensure_static_local_type_supported(&slot, inst)?;
    ensure_static_local_value_supported(ctx, &slot, value, inst)?;
    let initialized_label = ctx.next_label("static_local_initialized");
    let skip_release_label = ctx.next_label("static_local_skip_release");
    // The initializer operand (e.g. `[]`) was lowered before this instruction,
    // so its heap allocation already ran this call regardless of the marker. On
    // the init path (marker == 0) ownership transfers into the static slot; on
    // the skip path (marker != 0, already initialized this process) the freshly
    // allocated initializer temp is orphaned and must be released here — the
    // function epilogue intentionally does not clean it up because ownership is
    // considered transferred to the static, which is only true on the init path.
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), &slot.init_symbol, 0);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &skip_release_label);
    // -- init path: first-time initialization this process --
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, abi::int_result_reg(ctx.emitter), &slot.init_symbol, 0);
    let source_ty = ctx.load_value_to_result(value)?;
    let mut loaded_ty = source_ty.codegen_repr();
    // Narrow Mixed to Int when the static local slot is Int-typed.
    if matches!(slot.php_type.codegen_repr(), PhpType::Int)
        && matches!(loaded_ty, PhpType::Mixed)
    {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                         // move the Mixed pointer into the first SysV argument register
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            }
        }
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        ctx.load_value_to_result(value)?;
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        loaded_ty = PhpType::Int;
    }
    box_current_result_for_static_slot(ctx, &slot, value, &source_ty)?;
    let store_ty = slot.php_type.codegen_repr();
    abi::emit_store_result_to_symbol(ctx.emitter, &slot.symbol, &store_ty, false);
    clear_static_local_high_word_if_needed(ctx, &slot);
    abi::emit_jump(ctx.emitter, &initialized_label);
    // -- skip path: already initialized; release the orphaned initializer temp --
    ctx.emitter.label(&skip_release_label);
    release_orphaned_initializer_temp(ctx, value)?;
    ctx.emitter.label(&initialized_label);
    Ok(())
}

/// Releases the initializer operand on the init-marker skip path, where the
/// freshly allocated temp (e.g. the `[]` from `Op::ArrayNew`) was never stored
/// to the static slot and is not cleaned up by the function epilogue. Mirrors
/// the per-type release shape of `emit_release_symbol_value` in
/// `codegen_ir/web.rs`: strings free their payload through the validating
/// heap-free helper (a safe no-op for pooled/.rodata string literals), other
/// refcounted kinds decref through the type-specific helper, and non-refcounted
/// scalars (int/bool/float) own no heap and are a no-op.
fn release_orphaned_initializer_temp(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let source_ty = ctx.load_value_to_result(value)?;
    match source_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, _) = abi::string_result_regs(ctx.emitter);
            abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), ptr_reg);
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
        _ => {
            abi::emit_decref_if_refcounted(ctx.emitter, &source_ty);
        }
    }
    Ok(())
}

/// Boxes the current result into the Mixed slot representation when the static
/// slot widened to Mixed but the stored value is a concrete type. Mirrors
/// `store_value_to_local`'s ownership-aware box selection.
fn box_current_result_for_static_slot(
    ctx: &mut FunctionContext<'_>,
    slot: &StaticLocalSlot,
    value: ValueId,
    source_ty: &PhpType,
) -> Result<()> {
    if slot.php_type.codegen_repr() == PhpType::Mixed && *source_ty != PhpType::Mixed {
        if ctx.value_can_own_mixed_box_source(value)? {
            emit_box_current_owned_value_as_mixed(ctx.emitter, source_ty);
        } else {
            emit_box_current_value_as_mixed(ctx.emitter, source_ty);
        }
    }
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
    let symbol = crate::names::static_local_symbol(&ctx.function.name, &name);
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
/// A Mixed slot accepts any boxable value, since concrete values are boxed into
/// the Mixed cell representation at store time.
fn static_local_value_type_matches(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    if matches!(slot_ty, PhpType::Mixed) {
        return true;
    }
    if value_ty == slot_ty {
        return true;
    }
    // PHP coercive mode: Mixed (from checked arithmetic) can be narrowed to Int,
    // and Int/Bool/Void/Float can be boxed to Mixed for init.
    if matches!(slot_ty, PhpType::Int) && matches!(value_ty, PhpType::Mixed) {
        return true;
    }
    if matches!(slot_ty, PhpType::Mixed)
        && matches!(value_ty, PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Float)
    {
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
