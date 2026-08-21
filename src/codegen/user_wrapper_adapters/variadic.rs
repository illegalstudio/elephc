//! Purpose:
//! Packs fixed userspace stream-wrapper runtime arguments into PHP variadic arrays.
//! Preserves source order, element-reference aliases, and transferred element owners.
//!
//! Called from:
//! - `crate::codegen::user_wrapper_adapters`.
//!
//! Key details:
//! - All element type preflights run before the variadic array is allocated.
//! - The opened-path reference is observed as null by value and as its real cell by reference.

use crate::codegen::{
    DataSection, abi, emit_box_current_owned_value_as_mixed,
    emit_box_current_value_as_mixed,
};
use crate::codegen_support::emit::Emitter;
use crate::ir::Module;
use crate::parser::ast::TypeExpr;
use crate::types::{FunctionSig, PhpType};

use super::{
    adapter_slot_offset, cleanup, coercion,
    contract::{
        load_wrapper_source, wrapper_source_is_reference_cell, wrapper_source_type,
    },
    references,
};

/// Builds the variadic array argument from every runtime callback value after fixed parameters.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_wrapper_variadic_array(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: usize,
    adapter: &str,
    impl_class: &str,
    method_name: &str,
    signature: &FunctionSig,
    incoming_types: &[PhpType],
    variadic_index: usize,
    variadic_ty: &PhpType,
    element_cleanup_base: usize,
) {
    let element_ty = match variadic_ty.codegen_repr() {
        PhpType::Array(element_ty) => *element_ty,
        _ => PhpType::Mixed,
    };
    let variadic_signature_index = variadic_index.saturating_sub(1);
    let declared = signature
        .declared_params
        .get(variadic_signature_index)
        .copied()
        .unwrap_or(false);
    let type_expr = signature
        .param_type_exprs
        .get(variadic_signature_index)
        .and_then(Option::as_ref);
    let by_ref = signature
        .ref_params
        .get(variadic_signature_index)
        .copied()
        .unwrap_or(false);
    let entry_ty = variadic_entry_type(&element_ty, type_expr, declared);

    for source_index in variadic_index..incoming_types.len() {
        let source_ty = wrapper_source_type(slot, source_index, false, incoming_types)
            .expect("variadic wrapper source exists");
        load_wrapper_source(emitter, source_index, &source_ty);
        coercion::emit_wrapper_arg_preflight(
            module,
            emitter,
            data,
            &format!("{adapter}_variadic_{source_index}"),
            impl_class,
            method_name,
            source_index,
            None,
            &source_ty,
            &entry_ty,
            type_expr,
            declared,
            false,
        );
    }

    let tail_count = incoming_types.len().saturating_sub(variadic_index);
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        tail_count.max(4) as i64,
    );
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 1),
        element_ty.stack_size() as i64,
    );
    abi::emit_call_label(emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(
        emitter,
        abi::int_result_reg(emitter),
        if by_ref { &PhpType::Mixed } else { &element_ty },
    );
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));

    for (tail_index, source_index) in (variadic_index..incoming_types.len()).enumerate() {
        if by_ref && wrapper_source_is_reference_cell(slot, source_index, true) {
            load_wrapper_source(emitter, source_index, &PhpType::Int);
            cleanup::store_temp(
                emitter,
                &PhpType::Int,
                adapter_slot_offset(element_cleanup_base + tail_index),
            );
            references::emit_wrapper_ref_marker(emitter, &PhpType::Mixed);
            store_variadic_element(emitter, &PhpType::Mixed, tail_index);
            continue;
        }
        let source_ty = wrapper_source_type(slot, source_index, false, incoming_types)
            .expect("variadic wrapper source exists");
        load_wrapper_source(emitter, source_index, &source_ty);
        coercion::emit_wrapper_arg_conversion(
            module,
            emitter,
            &format!("{adapter}_variadic_{source_index}"),
            source_index,
            &source_ty,
            &entry_ty,
            type_expr,
            declared,
            false,
        );
        if by_ref {
            box_variadic_entry_as_mixed(
                emitter,
                &source_ty,
                &entry_ty,
                type_expr,
                declared,
            );
            references::emit_wrapper_temp_ref_cell(emitter, &PhpType::Mixed, true);
            cleanup::store_temp(
                emitter,
                &PhpType::Int,
                adapter_slot_offset(element_cleanup_base + tail_index),
            );
            references::emit_wrapper_ref_marker(emitter, &PhpType::Mixed);
            store_variadic_element(emitter, &PhpType::Mixed, tail_index);
            continue;
        }
        if source_ty.codegen_repr() == PhpType::Str && element_ty.codegen_repr() == PhpType::Str {
            abi::emit_call_label(emitter, "__rt_str_persist");
        }
        store_variadic_element(emitter, &element_ty, tail_index);
    }

    let array_reg = abi::secondary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, array_reg, 0);
    let len_reg = abi::tertiary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, len_reg, tail_count as i64);
    abi::emit_store_to_address(emitter, len_reg, array_reg, 0);
    if array_reg != abi::int_result_reg(emitter) {
        emitter.instruction(&format!(
            "mov {}, {}",
            abi::int_result_reg(emitter),
            array_reg
        )); // return the completed variadic array in the canonical result register
    }
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Resolves the value contract applied before an element enters a variadic reference cell.
fn variadic_entry_type(
    fallback: &PhpType,
    type_expr: Option<&TypeExpr>,
    declared: bool,
) -> PhpType {
    if !declared {
        return fallback.codegen_repr();
    }
    let Some(type_expr) = type_expr else {
        return fallback.codegen_repr();
    };
    if matches!(
        type_expr,
        TypeExpr::Union(_) | TypeExpr::Nullable(_) | TypeExpr::Intersection(_)
    ) {
        return PhpType::Mixed;
    }
    super::type_contract::simple_declared_storage_type(type_expr).codegen_repr()
}

/// Boxes one validated variadic reference entry and transfers conversion-owned payloads.
fn box_variadic_entry_as_mixed(
    emitter: &mut Emitter,
    source_ty: &PhpType,
    entry_ty: &PhpType,
    type_expr: Option<&TypeExpr>,
    declared: bool,
) {
    if entry_ty.codegen_repr() == PhpType::Mixed {
        return;
    }
    let conversion_is_owned = coercion::wrapper_arg_temp_type(
        Some(source_ty),
        entry_ty,
        type_expr,
        declared,
        false,
    )
    .is_some();
    if conversion_is_owned {
        emit_box_current_owned_value_as_mixed(emitter, &entry_ty.codegen_repr());
    } else {
        emit_box_current_value_as_mixed(emitter, &entry_ty.codegen_repr());
    }
}

/// Transfers the current converted value into one preallocated variadic-array element.
fn store_variadic_element(emitter: &mut Emitter, element_ty: &PhpType, index: usize) {
    let array_reg = abi::secondary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, array_reg, 0);
    let offset = 24 + index * element_ty.stack_size();
    match element_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(
                emitter,
                abi::float_result_reg(emitter),
                array_reg,
                offset,
            );
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_store_to_address(emitter, ptr_reg, array_reg, offset);
            abi::emit_store_to_address(emitter, len_reg, array_reg, offset + 8);
        }
        _ => {
            abi::emit_store_to_address(
                emitter,
                abi::int_result_reg(emitter),
                array_reg,
                offset,
            );
        }
    }
}
