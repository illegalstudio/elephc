//! Purpose:
//! Lowers low-level property slot load/store helpers.
//! Shares receiver and property metadata with object expression lowering.
//!
//! Called from:
//! - `crate::codegen::stmt::assignments::properties`
//!
//! Key details:
//! - Property writes must respect declared types, visibility checks, and runtime object layout.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::types::PhpType;

const NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;
/// Sentinel value stored in a property slot's first word to represent `Void`
/// when the upper word is also zeroed. Chosen to be an invalid pointer
/// representation that avoids confusing null pointers with uninitialized slots.

/// Releases the previous value stored in a property slot before the slot is
/// overwritten. For `Str` properties, calls `__rt_heap_free_safe` on the
/// previous string pointer. For other refcounted types, loads the previous
/// pointer and calls `emit_decref_if_refcounted`. Non-refcounted scalars
/// require no cleanup. The `object_reg` must hold the object pointer;
/// `offset` is the byte offset of the property slot; `prop_ty` describes
/// the type already stored in the slot.
pub(super) fn release_previous_property_value(
    emitter: &mut Emitter,
    object_reg: &str,
    prop_ty: &PhpType,
    offset: usize,
) {
    if matches!(prop_ty, PhpType::Str) {
        abi::emit_push_reg(emitter, object_reg);
        abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
        abi::emit_call_label(emitter, "__rt_heap_free_safe");
        abi::emit_pop_reg(emitter, object_reg);
    } else if prop_ty.is_refcounted() {
        abi::emit_push_reg(emitter, object_reg);
        abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
        abi::emit_decref_if_refcounted(emitter, prop_ty);
        abi::emit_pop_reg(emitter, object_reg);
    }
}

/// Stores a value into a property slot at `offset` bytes from `object_reg`.
/// The value's type determines the storage strategy: scalars are stored
/// directly with a runtime type tag in the upper word (offset+8); strings
/// are persisted via `__rt_str_persist` before storing pointer+length;
/// floats use the float register; refcounted types (arrays, objects, Mixed)
/// store the pointer and tag 7. `Void` stores a null sentinel; `Never`
/// zeros both words. The value must already be on the temporary stack
/// (or in the appropriate float register for Float).
pub(super) fn store_property_value(emitter: &mut Emitter, object_reg: &str, val_ty: &PhpType, offset: usize) {
    let temp_reg = abi::temp_int_reg(emitter.target);
    match val_ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Callable
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset);
            abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
        }
        PhpType::Resource(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset);
            abi::emit_load_int_immediate(emitter, temp_reg, 9);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset + 8);
        }
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset);
            abi::emit_load_int_immediate(emitter, temp_reg, 7);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset + 8);
        }
        PhpType::Array(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset);
            abi::emit_load_int_immediate(emitter, temp_reg, 4);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset + 8);
        }
        PhpType::AssocArray { .. } => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset);
            abi::emit_load_int_immediate(emitter, temp_reg, 5);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset + 8);
        }
        PhpType::Object(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset);
            abi::emit_load_int_immediate(emitter, temp_reg, 6);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset + 8);
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), object_reg, offset);
            abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
            abi::emit_push_reg(emitter, object_reg);
            abi::emit_call_label(emitter, "__rt_str_persist");
            abi::emit_pop_reg(emitter, object_reg);
            abi::emit_store_to_address(emitter, ptr_reg, object_reg, offset);
            abi::emit_store_to_address(emitter, len_reg, object_reg, offset + 8);
        }
        PhpType::Void => {
            abi::emit_load_int_immediate(emitter, temp_reg, NULL_SENTINEL);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset);
            abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
        }
        PhpType::Never => {
            abi::emit_store_zero_to_address(emitter, object_reg, offset);
            abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
        }
    }
}

/// Stores a property reference address into a property slot. The pointer to
/// the reference cell is popped from the temporary stack and written directly
/// into the property slot at `offset`; the upper word (offset+8) is zeroed.
/// Used for reference property storage where the slot holds a pointer to
/// the variable rather than the variable's value directly.
pub(super) fn store_property_reference_address(
    emitter: &mut Emitter,
    object_reg: &str,
    offset: usize,
) {
    let pointer_reg = abi::temp_int_reg(emitter.target);
    abi::emit_pop_reg(emitter, pointer_reg);
    abi::emit_store_to_address(emitter, pointer_reg, object_reg, offset);
    abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
}

/// Releases the previous value held through a reference property before
/// the reference is updated. Loads the value at `pointer_reg + 0` and calls
/// `emit_decref_if_refcounted` for refcounted types or `__rt_heap_free_safe`
/// for strings. Skips release for scalar types. `incoming_ty` is the type
/// being stored; registers used during release are saved/restored around
/// the helper call to avoid clobbering result registers.
pub(super) fn release_previous_referenced_value(
    emitter: &mut Emitter,
    pointer_reg: &str,
    prop_ty: &PhpType,
    incoming_ty: &PhpType,
) {
    if matches!(prop_ty, PhpType::Str) {
        abi::emit_push_reg(emitter, pointer_reg);
        let needs_save_result = !matches!(incoming_ty, PhpType::Str | PhpType::Float);
        if needs_save_result {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
        }
        abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0);
        abi::emit_call_label(emitter, "__rt_heap_free_safe");
        if needs_save_result {
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
        }
        abi::emit_pop_reg(emitter, pointer_reg);
    } else if prop_ty.is_refcounted() {
        abi::emit_push_reg(emitter, pointer_reg);
        let needs_save_result = !matches!(incoming_ty, PhpType::Str | PhpType::Float);
        if needs_save_result {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
        }
        abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0);
        abi::emit_decref_if_refcounted(emitter, prop_ty);
        if needs_save_result {
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
        }
        abi::emit_pop_reg(emitter, pointer_reg);
    }
}

/// Stores a value through a reference pointer (already loaded from a
/// reference property slot). Pops the value from the temporary stack (or
/// float register for Float, register pair for Str) and writes it to
/// `pointer_reg + 0`. Strings are persisted via `__rt_str_persist` before
/// storing. The upper word is not modified since reference targets are
/// always single-slot values.
pub(super) fn store_referenced_value(
    emitter: &mut Emitter,
    pointer_reg: &str,
    val_ty: &PhpType,
) {
    let temp_reg = if pointer_reg == abi::temp_int_reg(emitter.target) {
        abi::symbol_scratch_reg(emitter)
    } else {
        abi::temp_int_reg(emitter.target)
    };
    // Reference targets may be one-word local slots. Only strings have a
    // guaranteed second word in both local slots and default heap ref cells.
    match val_ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Callable
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 0);
        }
        PhpType::Resource(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 0);
        }
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 0);
        }
        PhpType::Array(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 0);
        }
        PhpType::AssocArray { .. } => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 0);
        }
        PhpType::Object(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 0);
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), pointer_reg, 0);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
            abi::emit_push_reg(emitter, pointer_reg);
            abi::emit_call_label(emitter, "__rt_str_persist");
            abi::emit_pop_reg(emitter, pointer_reg);
            abi::emit_store_to_address(emitter, ptr_reg, pointer_reg, 0);
            abi::emit_store_to_address(emitter, len_reg, pointer_reg, 8);
        }
        PhpType::Void => {
            abi::emit_load_int_immediate(emitter, temp_reg, NULL_SENTINEL);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 0);
        }
        PhpType::Never => {
            abi::emit_store_zero_to_address(emitter, pointer_reg, 0);
        }
    }
}
