//! Purpose:
//! Materializes callable descriptor values for closures and first-class callables.
//! Handles static descriptor records, generated runtime invokers, and runtime
//! capture slots for callable environments.
//!
//! Called from:
//! - `crate::codegen_support::expr::calls::closure`
//! - `crate::codegen_support::expr::calls::first_class`
//!
//! Key details:
//! - Runtime descriptors preserve the static descriptor header, then append
//!   16-byte capture value slots consumed by uniform descriptor invokers.

use crate::codegen_support::callable_descriptor::{
    self, CallableDescriptorInvocation,
};
use crate::codegen_support::context::{Context, HeapOwnership};
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, callable_dispatch};
use crate::types::{FunctionSig, PhpType};

use super::args;

/// Emits a callable descriptor value, allocating runtime capture storage when needed.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_callable_descriptor_value(
    entry_label: &str,
    php_name: Option<&str>,
    kind: u64,
    sig: &FunctionSig,
    captures: &[(String, PhpType, bool)],
    hidden_params: &[(String, PhpType, bool)],
    invocation: CallableDescriptorInvocation,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let invoker_label = callable_dispatch::ensure_runtime_descriptor_invoker(ctx, hidden_params, sig);
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
        data,
        entry_label,
        php_name,
        kind,
        Some(sig),
        captures,
        hidden_params,
        invocation,
        invoker_label.as_deref(),
    );

    if captures.is_empty() {
        abi::emit_symbol_address(emitter, abi::int_result_reg(emitter), &descriptor_label);
        return;
    }

    emit_runtime_descriptor_with_captures(
        &descriptor_label,
        captures,
        emitter,
        ctx,
    );
}

/// Allocates a runtime descriptor, copies the static header, and stores capture values.
fn emit_runtime_descriptor_with_captures(
    descriptor_label: &str,
    captures: &[(String, PhpType, bool)],
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let descriptor_reg = abi::nested_call_reg(emitter);
    let total_bytes =
        callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + captures.len() * 16;

    emitter.comment("callable descriptor: allocate runtime capture storage");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), total_bytes as i64);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction(&format!("mov {}, {}", descriptor_reg, abi::int_result_reg(emitter))); // keep the runtime descriptor pointer while storing captures
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        emitter,
        descriptor_reg,
        descriptor_label,
    );

    for (idx, (capture_name, capture_ty, by_ref)) in captures.iter().enumerate() {
        emitter.comment(&format!("callable descriptor: store capture ${}", capture_name));
        if matches!(capture_ty.codegen_repr(), PhpType::Callable) {
            ctx.mark_fcc_used(capture_name);
        }
        if *by_ref {
            promote_by_ref_capture_to_heap_cell(capture_name, capture_ty, emitter, ctx);
            if !args::emit_ref_arg_variable_address(
                capture_name,
                "callable descriptor capture ref",
                emitter,
                ctx,
            ) {
                emitter.comment(&format!(
                    "WARNING: captured callable variable ${} not found",
                    capture_name
                ));
                continue;
            }
            callable_descriptor::emit_store_current_result_to_runtime_capture(
                emitter,
                descriptor_reg,
                idx,
                &PhpType::Int,
            );
            continue;
        }

        let Some(capture_info) = ctx.variables.get(capture_name) else {
            emitter.comment(&format!(
                "WARNING: captured callable variable ${} not found",
                capture_name
            ));
            continue;
        };
        abi::emit_load(emitter, capture_ty, capture_info.stack_offset);
        if matches!(capture_ty.codegen_repr(), PhpType::Str) {
            abi::emit_call_label(emitter, "__rt_str_persist");
            callable_descriptor::emit_store_current_result_to_runtime_capture(
                emitter,
                descriptor_reg,
                idx,
                capture_ty,
            );
            continue;
        }
        callable_descriptor::emit_store_current_result_to_runtime_capture(
            emitter,
            descriptor_reg,
            idx,
            capture_ty,
        );
        retain_runtime_capture_result(emitter, capture_ty);
    }

    if descriptor_reg != abi::int_result_reg(emitter) {
        emitter.instruction(&format!("mov {}, {}", abi::int_result_reg(emitter), descriptor_reg)); // return the runtime callable descriptor pointer
    }
}

/// Promotes a local by-reference capture into a stable heap cell before descriptor storage.
///
/// Plain locals normally live in frame slots, but an escaped closure can outlive that
/// frame. The promotion copies the current local value into a 16-byte heap reference
/// cell, rewrites the local slot to hold the cell address, and marks the variable as a
/// reference so later writes update the same storage captured by the closure.
fn promote_by_ref_capture_to_heap_cell(
    capture_name: &str,
    capture_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    if ctx.global_vars.contains(capture_name)
        || ctx.static_vars.contains(capture_name)
        || ctx.ref_params.contains(capture_name)
    {
        return;
    }
    let Some((slot_offset, current_ty, current_static_ty, current_ownership)) =
        ctx.variables.get(capture_name).map(|var| {
            (
                var.stack_offset,
                var.ty.clone(),
                var.static_ty.clone(),
                var.ownership,
            )
        })
    else {
        emitter.comment(&format!(
            "WARNING: captured callable variable ${} not found",
            capture_name
        ));
        return;
    };

    emitter.comment(&format!(
        "callable descriptor: promote by-ref capture ${} to heap cell",
        capture_name
    ));
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    let cell_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", cell_reg, abi::int_result_reg(emitter))); // keep the promoted capture cell while moving the local value into it
    copy_local_value_to_ref_cell(&current_ty, slot_offset, cell_reg, emitter);
    release_owned_local_value_after_ref_cell_copy(
        &current_ty,
        current_ownership,
        slot_offset,
        cell_reg,
        emitter,
    );
    abi::store_at_offset_scratch(
        emitter,
        cell_reg,
        slot_offset,
        abi::temp_int_reg(emitter.target),
    );
    ctx.ref_params.insert(capture_name.to_string());
    ctx.update_var_type_static_and_ownership(
        capture_name,
        capture_ty.codegen_repr(),
        current_static_ty,
        HeapOwnership::borrowed_alias_for_type(capture_ty),
    );
}

/// Copies the current local value into a promoted heap reference cell.
///
/// Strings are persisted so the cell owns stable storage, callable descriptors are
/// retained, and refcounted heap payloads are incref'd. The local slot is not mutated
/// here; the caller stores the cell pointer after any old local owner is released.
fn copy_local_value_to_ref_cell(
    value_ty: &PhpType,
    slot_offset: usize,
    cell_reg: &str,
    emitter: &mut Emitter,
) {
    let temp_reg = abi::temp_int_reg(emitter.target);
    match value_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::load_at_offset_scratch(emitter, ptr_reg, slot_offset, temp_reg);
            abi::load_at_offset_scratch(emitter, len_reg, slot_offset - 8, temp_reg);
            abi::emit_push_reg(emitter, cell_reg);                              // preserve the promoted capture cell across string persistence
            abi::emit_call_label(emitter, "__rt_str_persist");                 // detach the captured string before storing it in the reference cell
            abi::emit_pop_reg(emitter, cell_reg);                               // restore the promoted capture cell after string persistence
            abi::emit_store_to_address(emitter, ptr_reg, cell_reg, 0);
            abi::emit_store_to_address(emitter, len_reg, cell_reg, 8);
        }
        PhpType::Float => {
            abi::load_at_offset_scratch(
                emitter,
                abi::float_result_reg(emitter),
                slot_offset,
                temp_reg,
            );
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), cell_reg, 0);
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
        PhpType::Callable => {
            abi::load_at_offset_scratch(emitter, abi::int_result_reg(emitter), slot_offset, temp_reg);
            abi::emit_push_reg(emitter, cell_reg);                              // preserve the promoted capture cell while retaining the callable descriptor
            callable_descriptor::emit_retain_current_descriptor(emitter);
            abi::emit_pop_reg(emitter, cell_reg);                               // restore the promoted capture cell after retaining the callable descriptor
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), cell_reg, 0);
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
        ty if ty.is_refcounted() => {
            abi::load_at_offset_scratch(emitter, abi::int_result_reg(emitter), slot_offset, temp_reg);
            abi::emit_push_reg(emitter, cell_reg);                              // preserve the promoted capture cell across the payload retain
            abi::emit_incref_if_refcounted(emitter, &ty);
            abi::emit_pop_reg(emitter, cell_reg);                               // restore the promoted capture cell after retaining the payload
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), cell_reg, 0);
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
        _ => {
            abi::load_at_offset_scratch(
                emitter,
                temp_reg,
                slot_offset,
                abi::secondary_scratch_reg(emitter),
            );
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
    }
}

/// Releases a replaced owned local value after copying it into a reference cell.
///
/// Only owned strings, callable descriptors, and refcounted heap payloads need release.
/// Borrowed or scalar locals remain untouched because the new cell owns its copy/retain.
fn release_owned_local_value_after_ref_cell_copy(
    value_ty: &PhpType,
    ownership: HeapOwnership,
    slot_offset: usize,
    cell_reg: &str,
    emitter: &mut Emitter,
) {
    if ownership != HeapOwnership::Owned {
        return;
    }
    if !matches!(value_ty.codegen_repr(), PhpType::Str | PhpType::Callable)
        && !value_ty.is_refcounted()
    {
        return;
    }

    abi::emit_push_reg(emitter, cell_reg);                                      // preserve the promoted capture cell while releasing the replaced local owner
    abi::load_at_offset_scratch(
        emitter,
        abi::int_result_reg(emitter),
        slot_offset,
        abi::temp_int_reg(emitter.target),
    );
    if matches!(value_ty.codegen_repr(), PhpType::Str) {
        abi::emit_call_label(emitter, "__rt_heap_free_safe");                  // release the old local string now that the capture cell owns a persisted copy
    } else if matches!(value_ty.codegen_repr(), PhpType::Callable) {
        callable_descriptor::emit_release_current_descriptor(emitter);
    } else {
        abi::emit_decref_if_refcounted(emitter, value_ty);
    }
    abi::emit_pop_reg(emitter, cell_reg);                                       // restore the promoted capture cell for storage in the local slot
}

/// Retains a by-value capture stored in a runtime descriptor.
fn retain_runtime_capture_result(emitter: &mut Emitter, capture_ty: &PhpType) {
    match capture_ty.codegen_repr() {
        PhpType::Str => {}
        PhpType::Callable => {
            callable_descriptor::emit_retain_current_descriptor(emitter);
        }
        other if other.is_refcounted() => {
            abi::emit_incref_if_refcounted(emitter, &other);
        }
        _ => {}
    }
}
