use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::types::PhpType;

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
        PhpType::Mixed | PhpType::Union(_) => {
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
            abi::emit_store_zero_to_address(emitter, object_reg, offset);
            abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
        }
    }
}

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
    match val_ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Callable
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 0);
            abi::emit_store_zero_to_address(emitter, pointer_reg, 8);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 7);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 8);
        }
        PhpType::Array(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 4);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 8);
        }
        PhpType::AssocArray { .. } => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 5);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 8);
        }
        PhpType::Object(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 6);
            abi::emit_store_to_address(emitter, temp_reg, pointer_reg, 8);
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), pointer_reg, 0);
            abi::emit_store_zero_to_address(emitter, pointer_reg, 8);
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
            abi::emit_store_zero_to_address(emitter, pointer_reg, 0);
            abi::emit_store_zero_to_address(emitter, pointer_reg, 8);
        }
    }
}
