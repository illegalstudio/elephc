//! Purpose:
//! Lowers the EIR by-reference local ops (`PromoteLocalRefCell`, `AliasLocalRefCell`,
//! `LoadRefCell`, `StoreRefCell`, `ReleaseLocalRefCell`) and the by-reference foreach
//! binding (`IterCurrentValueRef`) to WebAssembly for the wasm32-wasi backend.
//!
//! Called from:
//! - `crate::codegen_wasm::inst::lower_instruction` dispatches the six ops here.
//! - `crate::codegen_wasm::context::emit_ref_cell_release` and
//!   `emit_ref_cell_owner_epilogue` delegate the cell release sequence here.
//!
//! Key details:
//! - An owned ref cell is a kind-7 refcounted heap block with a 16-byte payload:
//!   the PHP value at @0 plus a second word at @8 (Str length, or a Tagged tag).
//!   A by-reference foreach binding is instead a borrowed array-element address,
//!   so @8 is written ONLY for two-word reprs (Str, Tagged).
//! - The cell pointer (or element address) is carried in a dedicated i32 local per
//!   slot (`FnCtx::ref_cell_ptrs`); WASM locals are not addressable linear memory, so
//!   a slot's value repr cannot itself hold the pointer.
//! - Ownership: `PromoteLocalRefCell` records one owner; a closure capture retains
//!   another real cell reference. `AliasLocalRefCell` and `IterCurrentValueRef`
//!   add no owner. Release is kind-validated, refcounted, and idempotent.

use super::context::{FnCtx, Result};
use super::values::WasmRepr;
use super::wat::ValType;
use super::WasmError;
use crate::ir::{Immediate, Instruction, LocalSlotId};
use crate::types::PhpType;

/// Extracts a `LocalSlotId` from the instruction's immediate, or an error.
fn slot_immediate(inst: &Instruction) -> Result<LocalSlotId> {
    match &inst.immediate {
        Some(Immediate::LocalSlot(slot)) => Ok(*slot),
        _ => Err(WasmError::Unsupported(format!(
            "missing LocalSlot immediate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts a `(first, second)` slot pair from the instruction's immediate, or an error.
fn slot_pair_immediate(inst: &Instruction) -> Result<(LocalSlotId, LocalSlotId)> {
    match &inst.immediate {
        Some(Immediate::LocalSlotPair { first, second }) => Ok((*first, *second)),
        _ => Err(WasmError::Unsupported(format!(
            "missing LocalSlotPair immediate in {:?}",
            inst.op
        ))),
    }
}

/// Returns the instruction's first operand, or an error.
fn operand(inst: &Instruction, i: usize) -> Result<crate::ir::ValueId> {
    super::inst::operand(inst, i)
}

/// Returns the stable WASM payload tag used by ref-cell destruction.
///
/// This matches the non-by-ref closure capture tags. The runtime only interprets
/// tags `{1,4,5,6,7,10,12}` as owned heap pointers; all scalar tags skip the payload
/// load entirely, preventing integer bits from being mistaken for an address.
pub(super) fn payload_tag_for_php_type(ty: &PhpType) -> u8 {
    match ty.codegen_repr() {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool | PhpType::False => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed | PhpType::Union(_) => 7,
        PhpType::Void => 8,
        PhpType::Resource(_) => 9,
        PhpType::Callable => 10,
        PhpType::Pointer(_) => 11,
        PhpType::Iterable => 12,
        PhpType::Buffer(_) => 13,
        PhpType::Packed(_) => 14,
        PhpType::Never => 15,
        PhpType::TaggedScalar => 16,
    }
}

/// Returns the instruction's payload type as the backend representation sees it.
fn payload_type(inst: &Instruction) -> PhpType {
    inst.result_php_type.codegen_repr()
}

/// Allocates one 16-byte owned ref cell and stamps its dedicated kind-7 header.
///
/// The allocator initializes refcount 1. All owned ref-cell allocation sites use
/// this helper so ordinary promoted locals, closure captures, and transient call
/// cells share the same validated runtime contract.
pub(super) fn emit_ref_cell_allocation(ctx: &mut FnCtx, ptr_local: &str) {
    ctx.fb.ins("i32.const 16", "ref cell payload size");
    ctx.fb.ins("call $__rt_heap_alloc", "allocate owned ref cell");
    ctx.fb
        .ins(&format!("local.set {}", ptr_local), "save ref cell pointer");
    ctx.fb
        .ins(&format!("local.get {}", ptr_local), "ref cell pointer");
    ctx.fb.ins("i32.const 8", "kind header displacement");
    ctx.fb.ins("i32.sub", "address of ref cell kind header");
    ctx.fb.ins("i64.const 7", "dedicated ref cell heap kind");
    ctx.fb.ins("i64.store", "stamp ref cell kind 7");
}

/// Emits the typed cell store for a value whose components live in `repr`'s locals.
///
/// Writes the value word to @0 and, for the two-word reprs (Str, Tagged), the second
/// word to @8. Single-word reprs (I64, F64, Ptr) write @0 only — this is required for
/// by-reference foreach, where the pointer targets an in-place array element and an
/// @8 write would corrupt the neighbouring slot.
pub(super) fn emit_cell_store(ctx: &mut FnCtx, ptr_local: &str, repr: &WasmRepr) -> Result<()> {
    match repr {
        WasmRepr::I64(local) => {
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb
                .ins(&format!("local.get {}", local), "value word (i64)");
            ctx.fb.ins("i64.store offset=0", "store the value @ cell+0");
        }
        WasmRepr::F64(local) => {
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb
                .ins(&format!("local.get {}", local), "value word (f64)");
            ctx.fb.ins("f64.store offset=0", "store the float @ cell+0");
        }
        WasmRepr::Ptr(local) => {
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb
                .ins(&format!("local.get {}", local), "value word (pointer)");
            ctx.fb.ins("i32.store offset=0", "store the pointer @ cell+0");
        }
        WasmRepr::Str { ptr, len } => {
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb
                .ins(&format!("local.get {}", ptr), "string pointer");
            ctx.fb.ins("i32.store offset=0", "store the string ptr @ cell+0");
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb
                .ins(&format!("local.get {}", len), "string length");
            ctx.fb.ins("i64.store offset=8", "store the length @ cell+8");
        }
        WasmRepr::Tagged { payload, tag } => {
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb
                .ins(&format!("local.get {}", payload), "tagged payload");
            ctx.fb.ins("i64.store offset=0", "store the payload @ cell+0");
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins(&format!("local.get {}", tag), "tagged tag (i32)");
            ctx.fb.ins("i64.extend_i32_u", "zero-extend the tag to i64");
            ctx.fb.ins("i64.store offset=8", "store the tag @ cell+8");
        }
        WasmRepr::Void => {}
    }
    Ok(())
}

/// Emits the typed cell load, pushing the value components in canonical order.
///
/// Reads @0 (and @8 for Str/Tagged) and leaves the components on the WASM operand
/// stack in the order `emit_store_value` consumes (Str: ptr then len; Tagged: payload
/// then tag; single-word reprs: the one value). The caller then stores them into the
/// result value's local(s).
pub(super) fn emit_cell_load(ctx: &mut FnCtx, ptr_local: &str, repr: &WasmRepr) -> Result<()> {
    match repr {
        WasmRepr::I64(_) => {
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i64.load offset=0", "load the value @ cell+0");
        }
        WasmRepr::F64(_) => {
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("f64.load offset=0", "load the float @ cell+0");
        }
        WasmRepr::Ptr(_) => {
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i32.load offset=0", "load the pointer @ cell+0");
        }
        WasmRepr::Str { .. } => {
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i32.load offset=0", "load the string ptr @ cell+0");
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i64.load offset=8", "load the length @ cell+8");
        }
        WasmRepr::Tagged { .. } => {
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i64.load offset=0", "load the payload @ cell+0");
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i64.load offset=8", "load the tag @ cell+8");
            ctx.fb.ins("i32.wrap_i64", "narrow the tag to i32");
        }
        WasmRepr::Void => {}
    }
    Ok(())
}

/// Emits the release sequence for one ref-cell owner.
///
/// `ptr_local` is the i32 local holding the 16-byte cell pointer; `payload_type` is
/// the value type stored in the cell (already `codegen_repr`-applied). The sequence
/// calls the dedicated kind-7 release helper with a compile-time payload tag, then
/// zeroes the owner local. The runtime decrements the cell refcount and only the final
/// owner releases the typed payload and frees the block. Scalar tags never load @0 as
/// a pointer.
pub(super) fn emit_ref_cell_release_seq(
    ctx: &mut FnCtx,
    ptr_local: &str,
    payload_type: &PhpType,
) -> Result<()> {
    ctx.fb
        .ins(&format!("local.get {}", ptr_local), "owned ref cell pointer");
    ctx.fb.ins(
        &format!("i32.const {}", payload_tag_for_php_type(payload_type)),
        "ref cell payload release tag",
    );
    ctx.fb.ins(
        "call $__rt_ref_cell_release",
        "release one ref cell owner",
    );
    ctx.fb.ins("i32.const 0", "null cell ptr");
    ctx.fb
        .ins(&format!("local.set {}", ptr_local), "clear the owner slot");
    Ok(())
}

/// Lowers `Op::LoadRefCell`: dereference the slot's cell pointer and store the value.
///
/// The slot must be ref-bound (registered in `ref_cell_ptrs`); the result value's
/// `WasmRepr` selects the typed load. For a foreach alias the pointer targets the
/// array element storage, so this reads the element in place.
pub(super) fn lower_load_ref_cell(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let slot = slot_immediate(inst)?;
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("load_ref_cell without result".to_string()))?;
    let ptr_local = ctx.ref_cell_ptr(slot.as_raw())?.to_string();
    let result_repr = ctx.value_repr(result)?.clone();
    emit_cell_load(ctx, &ptr_local, &result_repr)?;
    super::inst::store_result(ctx, inst)
}

/// Lowers `Op::StoreRefCell`: store the operand value through the slot's cell pointer.
///
/// Does NOT release the cell's previous payload — the EIR emits the prior-value
/// release (load + release_if_owned) before this op. A concrete operand uses its
/// `WasmRepr`; an owned Mixed arithmetic result is first cast to the cell's payload
/// type and then released. Only Str/Tagged write @8, so a foreach alias into an
/// 8-byte scalar slot does not corrupt the neighbouring element.
pub(super) fn lower_store_ref_cell(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let slot = slot_immediate(inst)?;
    let value = operand(inst, 0)?;
    let ptr_local = ctx.ref_cell_ptr(slot.as_raw())?.to_string();
    let target_php = inst.result_php_type.codegen_repr();
    let source_repr = ctx.value_repr(value)?.clone();
    let source = ctx
        .function
        .value(value)
        .ok_or_else(|| WasmError::Unsupported(format!("ref-cell store value {:?} missing", value)))?;
    let source_php = source.php_type.codegen_repr();
    let release_owned_mixed = source.ownership == crate::ir::Ownership::Owned
        && source_php == PhpType::Mixed
        && target_php != PhpType::Mixed;

    let stored_repr = if source_php == PhpType::Mixed && target_php != PhpType::Mixed {
        coerce_mixed_ref_cell_store(ctx, &source_repr, &target_php)?
    } else {
        source_repr.clone()
    };
    emit_cell_store(ctx, &ptr_local, &stored_repr)?;
    if release_owned_mixed {
        let WasmRepr::Ptr(source_ptr) = source_repr else {
            return Err(WasmError::Unsupported(
                "owned Mixed ref-cell store source is not a pointer".to_string(),
            ));
        };
        ctx.fb
            .ins(&format!("local.get {}", source_ptr), "consumed Mixed store source");
        ctx.fb.ins(
            "call $__rt_decref_any",
            "release checked-arithmetic Mixed temporary after coercion",
        );
    }
    Ok(())
}

/// Converts one boxed-Mixed source into the concrete shape of a ref-cell store.
///
/// Checked integer arithmetic intentionally yields an owned Mixed cell so overflow
/// can promote to float. Stores through a statically typed reference must mirror the
/// native backend: cast to the alias type first, store the concrete value, then let
/// the caller release the consumed Mixed box. String casts return an owned copy.
fn coerce_mixed_ref_cell_store(
    ctx: &mut FnCtx,
    source_repr: &WasmRepr,
    target_php: &PhpType,
) -> Result<WasmRepr> {
    let WasmRepr::Ptr(source_ptr) = source_repr else {
        return Err(WasmError::Unsupported(
            "Mixed ref-cell store source is not a pointer".to_string(),
        ));
    };
    match target_php {
        PhpType::Int | PhpType::Bool => {
            let converted = ctx.fresh_temp(ValType::I64);
            ctx.fb
                .ins(&format!("local.get {}", source_ptr), "Mixed cell to cast");
            let helper = if *target_php == PhpType::Bool {
                "__rt_mixed_cast_bool"
            } else {
                "__rt_mixed_cast_int"
            };
            ctx.fb
                .ins(&format!("call ${}", helper), "cast Mixed ref-cell store value");
            ctx.fb
                .ins(&format!("local.set {}", converted), "save concrete i64 value");
            Ok(WasmRepr::I64(converted))
        }
        PhpType::Float => {
            let converted = ctx.fresh_temp(ValType::F64);
            ctx.fb
                .ins(&format!("local.get {}", source_ptr), "Mixed cell to cast");
            ctx.fb.ins(
                "call $__rt_mixed_cast_float",
                "cast Mixed ref-cell store to float bits",
            );
            ctx.fb
                .ins("f64.reinterpret_i64", "reinterpret cast bits as f64");
            ctx.fb
                .ins(&format!("local.set {}", converted), "save concrete float");
            Ok(WasmRepr::F64(converted))
        }
        PhpType::Str => {
            let len_i32 = ctx.fresh_temp(ValType::I32);
            let ptr = ctx.fresh_temp(ValType::I32);
            let len = ctx.fresh_temp(ValType::I64);
            ctx.fb
                .ins(&format!("local.get {}", source_ptr), "Mixed cell to cast");
            ctx.fb.ins(
                "call $__rt_mixed_cast_string",
                "cast Mixed ref-cell store to owned string",
            );
            ctx.fb
                .ins(&format!("local.set {}", len_i32), "save cast string length");
            ctx.fb
                .ins(&format!("local.set {}", ptr), "save cast string pointer");
            ctx.fb
                .ins(&format!("local.get {}", len_i32), "cast string length");
            ctx.fb.ins("i64.extend_i32_u", "widen string length to i64");
            ctx.fb
                .ins(&format!("local.set {}", len), "save widened string length");
            Ok(WasmRepr::Str { ptr, len })
        }
        _ => Err(WasmError::Unsupported(format!(
            "Mixed ref-cell store to {:?} is not supported on wasm32-wasi",
            target_php
        ))),
    }
}

/// Lowers `Op::PromoteLocalRefCell`: heap-alloc a 16-byte cell, retain the slot's
/// current value into it, and release the slot's old value.
///
/// Both the php-visible slot and the owner slot share one i32 pointer local. The
/// payload is retained so the cell holds a stable owning reference (string is
/// persisted to an owned copy; a callable/container is incref'd); the slot's old
/// value is then released through the same kind dispatcher, leaving the net refcount
/// unchanged. The owner is recorded for the `Return` epilogue.
pub(super) fn lower_promote_local_ref_cell(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let (php_slot, owner_slot) = slot_pair_immediate(inst)?;
    let payload = payload_type(inst);
    let slot_repr = ctx.slot_repr(php_slot)?.clone();

    // One i32 local carries the cell pointer for the php-visible slot and every
    // owner alias emitted for it. Reuse the local when another EIR path promotes
    // the same slot, but keep the allocation check at runtime: two mutually
    // exclusive branches are both lowered, and either branch may execute first.
    let (rc, first_promotion) = match ctx.ref_cell_ptrs.get(&php_slot.as_raw()) {
        Some(existing) => (existing.clone(), false),
        None => (ctx.fresh_temp(ValType::I32), true),
    };
    ctx.register_ref_cell_ptr(php_slot.as_raw(), rc.clone(), true);
    ctx.register_ref_cell_ptr(owner_slot.as_raw(), rc.clone(), true);
    if first_promotion {
        ctx.add_ref_cell_owner(owner_slot.as_raw(), payload.clone());
    }

    // Allocate and populate only while the shared pointer is null. Sequential
    // re-promotions therefore remain idempotent, while branch-local promotions
    // each retain a real initialization path.
    ctx.fb
        .ins(&format!("local.get {}", rc), "existing ref cell pointer");
    ctx.fb.ins("i32.eqz", "ref cell still uninitialized?");
    ctx.fb.raw("(if");
    ctx.fb.raw("(then");
    emit_ref_cell_allocation(ctx, &rc);

    // Retain the payload and store it into the cell, then release the slot's old value.
    retain_and_store_slot_value(ctx, &rc, &slot_repr, &payload)?;
    release_old_slot_value(ctx, &slot_repr, &payload)?;
    ctx.fb.raw(")");
    ctx.fb.raw(")");
    Ok(())
}

/// Retains the slot's current value into the cell.
///
/// Strings are persisted to an owned heap copy (the cell stores the new ptr+len);
/// callables and refcounted containers are incref'd (the cell stores the same
/// pointer, net +1 ref for the cell); scalars and tagged scalars need no retain.
/// The retained value is stored via the typed cell-store helper, reading it from the
/// slot's value locals (which the retain leaves untouched for non-string types).
pub(super) fn retain_and_store_slot_value(
    ctx: &mut FnCtx,
    rc: &str,
    slot_repr: &WasmRepr,
    payload: &PhpType,
) -> Result<()> {
    match slot_repr {
        WasmRepr::I64(local) => {
            if *payload == PhpType::Callable {
                // Callable: incref the descriptor (carried as a zero-extended i32).
                ctx.fb
                    .ins(&format!("local.get {}", local), "callable descriptor (i64)");
                ctx.fb.ins("i32.wrap_i64", "narrow the descriptor pointer to i32");
                ctx.fb
                    .ins("call $__rt_incref", "retain the callable descriptor for the cell");
            }
            emit_cell_store(ctx, rc, slot_repr)?;
        }
        WasmRepr::F64(_) => {
            emit_cell_store(ctx, rc, slot_repr)?;
        }
        WasmRepr::Ptr(local) => {
            // Refcounted container (array/hash/object/mixed): incref the pointer.
            ctx.fb
                .ins(&format!("local.get {}", local), "container pointer");
            ctx.fb
                .ins("call $__rt_incref", "retain the container for the cell");
            emit_cell_store(ctx, rc, slot_repr)?;
        }
        WasmRepr::Str { ptr, len } => {
            // String: persist to an owned copy so the cell holds a stable owner.
            ctx.fb.ins(&format!("local.get {}", ptr), "source string pointer");
            ctx.fb.ins(&format!("local.get {}", len), "source string length");
            ctx.fb
                .ins("call $__rt_str_persist", "persist the string to an owned heap copy");
            let new_len = ctx.fresh_temp(ValType::I64);
            let new_ptr = ctx.fresh_temp(ValType::I32);
            ctx.fb
                .ins(&format!("local.set {}", new_len), "captured owned string length");
            ctx.fb
                .ins(&format!("local.set {}", new_ptr), "captured owned string pointer");
            ctx.fb.ins(&format!("local.get {}", rc), "cell address");
            ctx.fb.ins(&format!("local.get {}", new_ptr), "owned string pointer");
            ctx.fb.ins("i32.store offset=0", "store the owned ptr @ cell+0");
            ctx.fb.ins(&format!("local.get {}", rc), "cell address");
            ctx.fb.ins(&format!("local.get {}", new_len), "owned string length");
            ctx.fb.ins("i64.store offset=8", "store the length @ cell+8");
        }
        WasmRepr::Tagged { .. } => {
            emit_cell_store(ctx, rc, slot_repr)?;
        }
        WasmRepr::Void => {
            return Err(WasmError::Unsupported("promote of a void local".to_string()));
        }
    }
    Ok(())
}

/// Releases the slot's pre-promotion value now that the cell owns a retained copy.
///
/// Strings free the original pointer through the range-guarded `__rt_heap_free_safe`
/// (a borrowed/data-segment literal is a no-op there); callables and refcounted
/// containers drop the old reference via `__rt_decref_any`; scalars and tagged scalars
/// own nothing. Reads the old value from the slot's value locals (untouched by retain
/// except for strings, where the original pointer is still there).
pub(super) fn release_old_slot_value(
    ctx: &mut FnCtx,
    slot_repr: &WasmRepr,
    payload: &PhpType,
) -> Result<()> {
    match slot_repr {
        WasmRepr::I64(local) => {
            if *payload == PhpType::Callable {
                ctx.fb
                    .ins(&format!("local.get {}", local), "old callable descriptor");
                ctx.fb.ins("i32.wrap_i64", "narrow the descriptor pointer to i32");
                ctx.fb
                    .ins("call $__rt_decref_any", "release the old callable descriptor (kind 6)");
            }
            // Int/Bool: no owned payload.
        }
        WasmRepr::F64(_) => {
            // Float: no owned payload.
        }
        WasmRepr::Ptr(local) => {
            ctx.fb
                .ins(&format!("local.get {}", local), "old container pointer");
            ctx.fb
                .ins("call $__rt_decref_any", "release the old container by kind");
        }
        WasmRepr::Str { ptr, .. } => {
            ctx.fb
                .ins(&format!("local.get {}", ptr), "old string pointer");
            ctx.fb
                .ins("call $__rt_heap_free_safe", "free the old string (skips non-heap)");
        }
        WasmRepr::Tagged { .. } => {
            // Tagged scalar: no owned payload.
        }
        WasmRepr::Void => {}
    }
    Ok(())
}

/// Lowers `Op::AliasLocalRefCell`: bind the target slot to the source slot's cell ptr.
///
/// The source must already be ref-bound. The target gets its own i32 local holding a
/// copy of the source's cell pointer (separate locals, same cell address — mirrors the
/// native distinct stack offsets). The target gets no owner: only the source's owner
/// releases the cell, so there is a single release and no double-free.
pub(super) fn lower_alias_local_ref_cell(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let (target_slot, source_slot) = slot_pair_immediate(inst)?;
    let src_rc = ctx.ref_cell_ptr(source_slot.as_raw())?.to_string();
    let owned = ctx.ref_cell_has_owner(source_slot.as_raw());
    let target_rc = ctx.fresh_temp(ValType::I32);
    ctx.register_ref_cell_ptr(target_slot.as_raw(), target_rc.clone(), owned);
    ctx.fb.ins(&format!("local.get {}", src_rc), "source cell pointer");
    ctx.fb
        .ins(&format!("local.set {}", target_rc), "target copies the cell pointer");
    Ok(())
}

/// Lowers `Op::ReleaseLocalRefCell`: release the owner's cell and clear the owner.
///
/// Null-guarded (a no-op if the owner was already cleared) and frees the 16-byte cell
/// after releasing its payload by kind. The php-visible slot is left untouched (the
/// variable may still hold a borrowed alias).
pub(super) fn lower_release_local_ref_cell(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let owner_slot = slot_immediate(inst)?;
    let payload = payload_type(inst);
    let ptr_local = ctx.ref_cell_ptr(owner_slot.as_raw())?.to_string();
    ctx.emit_ref_cell_release(&ptr_local, &payload)
}

/// Lowers `Op::IterCurrentValueRef`: bind the foreach value slot to the current
/// array element's in-place address.
///
/// The slot's cell-pointer local is set to `source + 24 + cursor * elem_size` (16-byte
/// slots for strings, 8-byte for scalars), so subsequent `LoadRefCell`/`StoreRefCell`
/// read and write the array element directly — PHP by-reference foreach semantics.
/// The binding is a borrow (no owner recorded), so the epilogue never frees it; the
/// array itself owns the element storage. Associative-array by-ref foreach is not
/// supported yet (clean diagnostic).
pub(super) fn lower_iter_current_value_ref(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let iter = operand(inst, 0)?;
    let slot = slot_immediate(inst)?;
    let slots = ctx.iter_slots(iter)?;
    if slots.is_hash {
        return Err(WasmError::Unsupported(
            "by-ref foreach over associative arrays (P7c0a: indexed arrays only)".to_string(),
        ));
    }
    let src = slots.source.clone();
    let cur = slots.cursor.clone();
    let elem_size: i64 = if slots.elem == PhpType::Str { 16 } else { 8 };

    let rc = ctx.fresh_temp(ValType::I32);
    ctx.register_ref_cell_ptr(slot.as_raw(), rc.clone(), false);
    ctx.fb.ins(&format!("local.get {}", src), "iterator source array");
    ctx.fb.ins("i32.const 24", "skip the indexed-array header (len/cap/elem_size)");
    ctx.fb.ins("i32.add", "source + header");
    ctx.fb.ins(&format!("local.get {}", cur), "current element cursor");
    ctx.fb
        .ins(&format!("i64.const {}", elem_size), "indexed element slot size");
    ctx.fb.ins("i64.mul", "cursor * elem_size (byte offset)");
    ctx.fb.ins("i32.wrap_i64", "narrow the byte offset to i32");
    ctx.fb
        .ins("i32.add", "element address = source + 24 + cursor*elem_size");
    ctx.fb
        .ins(&format!("local.set {}", rc), "bind the slot to the element address (borrow)");
    Ok(())
}
