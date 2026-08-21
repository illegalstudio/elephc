//! Purpose:
//! Materializes temporary PHP reference cells for userspace-wrapper callback arguments.
//! Emits PHP's warning when an internal callback value is passed to a by-reference parameter.
//!
//! Called from:
//! - `crate::codegen::user_wrapper_adapters::emit_user_wrapper_adapter()`.
//!
//! Key details:
//! - `stream_open()`'s opened-path argument is already a real reference cell and is never copied.
//! - Every other supplied by-reference callback argument receives an isolated 16-byte cell.
//! - The cell owns its payload and is released after the callback, including after mutation.

use crate::codegen::{
    DataSection, abi, callable_invoker_args, emit_box_runtime_payload_as_mixed,
};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::types::{FunctionSig, PhpType};

use super::contract::{
    wrapper_arg_is_by_ref, wrapper_regular_param_count, wrapper_source_is_reference_cell,
};

/// Emits every suppressible by-reference warning required by one callback invocation.
pub(super) fn emit_wrapper_by_ref_warnings(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: usize,
    class_name: &str,
    method_name: &str,
    signature: &FunctionSig,
    incoming_types: &[PhpType],
) {
    let visible_count = incoming_types.len().saturating_sub(1);
    let regular_count = wrapper_regular_param_count(signature);
    for parameter_index in 0..visible_count.min(regular_count) {
        let adapter_index = parameter_index + 1;
        let by_ref = wrapper_arg_is_by_ref(signature, adapter_index);
        if !by_ref || wrapper_source_is_reference_cell(slot, adapter_index, by_ref) {
            continue;
        }
        let parameter_name = signature
            .params
            .get(parameter_index)
            .map(|(name, _)| name.as_str());
        emit_by_ref_warning(
            emitter,
            data,
            class_name,
            method_name,
            adapter_index,
            parameter_name,
        );
    }

    if signature.variadic.is_none() {
        return;
    }
    let variadic_index = regular_count + 1;
    if !wrapper_arg_is_by_ref(signature, variadic_index) {
        return;
    }
    for adapter_index in variadic_index..=visible_count {
        if wrapper_source_is_reference_cell(slot, adapter_index, true) {
            continue;
        }
        emit_by_ref_warning(
            emitter,
            data,
            class_name,
            method_name,
            adapter_index,
            None,
        );
    }
}

/// Transfers the current callback value into a fresh heap reference cell.
pub(super) fn emit_wrapper_temp_ref_cell(
    emitter: &mut Emitter,
    value_ty: &PhpType,
    value_is_owned: bool,
) {
    let value_repr = value_ty.codegen_repr();
    if !value_is_owned {
        if value_repr == PhpType::Str {
            abi::emit_call_label(emitter, "__rt_str_persist");
        } else {
            abi::emit_incref_if_refcounted(emitter, &value_repr);
        }
    }
    abi::emit_push_result_value(emitter, &value_repr);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    let cell_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!(
        "mov {}, {}",
        cell_reg,
        abi::int_result_reg(emitter)
    )); // keep the allocated callback reference-cell address across payload restoration
    store_wrapper_ref_cell_value(emitter, cell_reg, &value_repr);
    if cell_reg != abi::int_result_reg(emitter) {
        emitter.instruction(&format!(
            "mov {}, {}",
            abi::int_result_reg(emitter),
            cell_reg
        )); // pass the temporary reference-cell pointer through the compiled method ABI
    }
}

/// Boxes the current reference-cell pointer as an invoker variadic-element marker.
pub(super) fn emit_wrapper_ref_marker(emitter: &mut Emitter, value_ty: &PhpType) {
    let cell_reg = abi::secondary_scratch_reg(emitter);
    let marker_tag_reg = abi::tertiary_scratch_reg(emitter);
    let source_tag_reg = abi::symbol_scratch_reg(emitter);
    abi::emit_reg_move(
        emitter,
        cell_reg,
        abi::int_result_reg(emitter),
    );
    abi::emit_load_int_immediate(
        emitter,
        marker_tag_reg,
        callable_invoker_args::INVOKER_ARG_REF_CELL_TAG,
    );
    abi::emit_load_int_immediate(
        emitter,
        source_tag_reg,
        crate::codegen::runtime_value_tag(&value_ty.codegen_repr()) as i64,
    );
    emitter.comment("wrapper_variadic_ref_cell");
    emit_box_runtime_payload_as_mixed(emitter, marker_tag_reg, cell_reg, source_tag_reg);
}

/// Stores one pushed owned callback value into a 16-byte PHP reference cell.
fn store_wrapper_ref_cell_value(emitter: &mut Emitter, cell_reg: &str, value_ty: &PhpType) {
    match value_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
            abi::emit_store_to_address(emitter, ptr_reg, cell_reg, 0);
            abi::emit_store_to_address(emitter, len_reg, cell_reg, 8);
        }
        PhpType::TaggedScalar => {
            let tag_reg = crate::codegen::sentinels::tagged_scalar_tag_reg(emitter);
            abi::emit_pop_reg_pair(emitter, abi::int_result_reg(emitter), tag_reg);
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), cell_reg, 0);
            abi::emit_store_to_address(emitter, tag_reg, cell_reg, 8);
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
            abi::emit_store_to_address(
                emitter,
                abi::float_result_reg(emitter),
                cell_reg,
                0,
            );
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
        _ => {
            let payload_reg = abi::temp_int_reg(emitter.target);
            abi::emit_pop_reg(emitter, payload_reg);
            abi::emit_store_to_address(emitter, payload_reg, cell_reg, 0);
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
    }
}

/// Emits one static PHP by-reference warning through the suppressible diagnostic channel.
fn emit_by_ref_warning(
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_name: &str,
    method_name: &str,
    argument_index: usize,
    parameter_name: Option<&str>,
) {
    let parameter = parameter_name
        .map(|name| format!(" (${name})"))
        .unwrap_or_default();
    let message = format!(
        "Warning: {}::{}(): Argument #{}{} must be passed by reference, value given\n",
        class_name.trim_start_matches('\\'),
        method_name,
        argument_index,
        parameter,
    );
    let (label, len) = data.add_string(message.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", &label);
            abi::emit_load_int_immediate(emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rdi", &label);
            abi::emit_load_int_immediate(emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(emitter, "__rt_diag_warning");
}
