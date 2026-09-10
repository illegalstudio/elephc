//! Purpose:
//! Emits per-class stream-wrapper adapters from the runtime's fixed callback ABI to PHP method ABIs.
//! Keeps untyped wrapper parameters boxed as `Mixed` without changing direct PHP method calls.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` before user wrapper vtable data is emitted.
//!
//! Key details:
//! - Runtime callback argument shapes are fixed by the stream-wrapper dispatch helpers.
//! - Newly boxed `Mixed` arguments remain caller-owned and are released after the method returns.
//! - Both supported architectures use shared ABI planners for register and stack placement.

mod cleanup;
mod contract;
pub(super) mod coercion;
mod deprecations;
mod references;
mod returns;
mod throwable;
mod type_contract;
mod unwind;
mod variadic;

use std::collections::HashMap;

use crate::codegen::{abi, emit_box_current_value_as_mixed, DataSection};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::{
    user_wrapper_adapter_symbol, USER_WRAPPER_METHOD_NAMES,
};
use crate::ir::Module;
use crate::names::method_symbol;
use crate::types::{ClassInfo, FunctionSig, PhpType};

use contract::{
    emit_wrapper_arg_preflights, load_wrapper_source, wrapper_arg_is_by_ref,
    wrapper_arg_is_variadic, wrapper_method_arg_types, wrapper_runtime_arg_types,
    wrapper_semantic_arg_type, wrapper_source_is_reference_cell, wrapper_source_type,
};
use cleanup::WrapperCleanup;

/// Emits every public, implemented userspace stream-wrapper method adapter.
pub(super) fn emit_user_wrapper_adapters(
    module: &Module,
    classes: &HashMap<String, ClassInfo>,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let mut wrapper_classes = classes.iter().collect::<Vec<_>>();
    wrapper_classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in wrapper_classes {
        for (slot, method_name) in USER_WRAPPER_METHOD_NAMES.iter().enumerate() {
            let Some(signature) = class_info.methods.get(*method_name) else {
                continue;
            };
            let Some(impl_class) = class_info.method_impl_classes.get(*method_name) else {
                continue;
            };
            if !class_info
                .method_visibilities
                .get(*method_name)
                .is_some_and(|visibility| {
                    matches!(visibility, crate::parser::ast::Visibility::Public)
                })
            {
                continue;
            }
            emit_user_wrapper_adapter(
                module,
                emitter,
                class_name,
                class_info.class_id,
                slot,
                method_name,
                impl_class,
                signature,
                data,
            );
        }
    }
}

/// Emits one fixed-runtime-ABI to compiled-method-ABI adapter.
#[allow(clippy::too_many_arguments)]
fn emit_user_wrapper_adapter(
    module: &Module,
    emitter: &mut Emitter,
    class_name: &str,
    class_id: u64,
    slot: usize,
    method_name: &str,
    impl_class: &str,
    signature: &FunctionSig,
    data: &mut DataSection,
) {
    let incoming_types = wrapper_runtime_arg_types(slot, class_name);
    let actual_types = wrapper_method_arg_types(signature, impl_class);
    let arg_cleanups = actual_types
        .iter()
        .enumerate()
        .map(|(index, target)| {
            let semantic_target = wrapper_semantic_arg_type(signature, target, index);
            let by_ref = wrapper_arg_is_by_ref(signature, index);
            let type_expr = index.checked_sub(1).and_then(|signature_index| {
                signature
                    .param_type_exprs
                    .get(signature_index)
                    .and_then(Option::as_ref)
            });
            let declared = index.checked_sub(1).is_some_and(|signature_index| {
                signature
                    .declared_params
                    .get(signature_index)
                    .copied()
                    .unwrap_or(false)
            });
            let source_ty = if wrapper_arg_is_variadic(signature, index) {
                None
            } else {
                wrapper_source_type(
                    slot,
                    index,
                    by_ref,
                    &incoming_types,
                )
            };
            if wrapper_arg_is_variadic(signature, index) && by_ref {
                let mut cleanups = vec![WrapperCleanup::RefCell(
                    semantic_target.codegen_repr(),
                )];
                for source_index in index..incoming_types.len() {
                    if wrapper_source_is_reference_cell(slot, source_index, true) {
                        cleanups.push(WrapperCleanup::BorrowedRefCell(PhpType::Mixed));
                    } else {
                        cleanups.push(WrapperCleanup::RefCell(PhpType::Mixed));
                    }
                }
                return cleanups;
            }
            if by_ref
                && !wrapper_source_is_reference_cell(slot, index, by_ref)
            {
                return vec![WrapperCleanup::RefCell(semantic_target.codegen_repr())];
            }
            if by_ref
                && wrapper_source_is_reference_cell(slot, index, by_ref)
                && cleanup::ref_cell_payload_needs_cleanup(semantic_target)
            {
                return vec![WrapperCleanup::BorrowedRefCell(
                    semantic_target.codegen_repr(),
                )];
            }
            coercion::wrapper_arg_temp_type(
                source_ty.as_ref(),
                semantic_target,
                type_expr,
                declared,
                by_ref,
            )
            .map(WrapperCleanup::Value)
            .into_iter()
            .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let cleanup_entries = arg_cleanups
        .iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    let cleanup_count = cleanup_entries.len();
    let source_slot_count = incoming_types.len();
    let cleanup_base = source_slot_count;
    let return_slot = cleanup_base + cleanup_count;
    let eof_check_mode_slot = (slot == 4).then_some(return_slot + 1);
    let local_frame_size =
        adapter_frame_size(return_slot + 1 + usize::from(eof_check_mode_slot.is_some()));
    let frame_size = unwind::frame_size_with_boundary(local_frame_size);
    let handler_base = unwind::boundary_base_offset(frame_size);
    let adapter = user_wrapper_adapter_symbol(class_id, method_name);
    let escape_label = format!("{adapter}_callback_throw");

    emitter.blank();
    emitter.comment(&format!(
        "--- user stream-wrapper ABI adapter {}::{} ---",
        class_name, method_name
    ));
    emitter.label_global(&adapter);
    abi::emit_frame_prologue(emitter, frame_size);
    spill_wrapper_runtime_args(emitter, &incoming_types);
    if let Some(mode_slot) = eof_check_mode_slot {
        spill_stream_eof_check_mode(emitter, adapter_slot_offset(mode_slot));
    }
    cleanup::initialize_slots(emitter, cleanup_base, cleanup_count);
    unwind::emit_boundary_push(emitter, handler_base, &escape_label);
    references::emit_wrapper_by_ref_warnings(
        emitter,
        data,
        slot,
        impl_class,
        method_name,
        signature,
        &incoming_types,
    );
    emit_wrapper_arg_preflights(
        module,
        emitter,
        data,
        &adapter,
        impl_class,
        method_name,
        slot,
        signature,
        &incoming_types,
    );

    let assignments =
        abi::build_outgoing_arg_assignments_for_target(module.target, &actual_types, 0);
    let mut cleanup_index = 0usize;
    for (index, target_ty) in actual_types.iter().enumerate() {
        let semantic_target = wrapper_semantic_arg_type(signature, target_ty, index);
        let by_ref = wrapper_arg_is_by_ref(signature, index);
        let type_expr = index.checked_sub(1).and_then(|signature_index| {
            signature
                .param_type_exprs
                .get(signature_index)
                .and_then(Option::as_ref)
        });
        let declared = index.checked_sub(1).is_some_and(|signature_index| {
            signature
                .declared_params
                .get(signature_index)
                .copied()
                .unwrap_or(false)
        });
        let is_variadic = wrapper_arg_is_variadic(signature, index);
        let source_is_ref_cell =
            !is_variadic && wrapper_source_is_reference_cell(slot, index, by_ref);
        let arg_cleanup_count = arg_cleanups[index].len();
        let value_is_owned;
        if is_variadic {
            variadic::emit_wrapper_variadic_array(
                module,
                emitter,
                data,
                slot,
                &adapter,
                impl_class,
                method_name,
                signature,
                &incoming_types,
                index,
                semantic_target,
                cleanup_base + cleanup_index + usize::from(by_ref),
            );
            value_is_owned = true;
        } else if index < incoming_types.len() {
            let source_ty = wrapper_source_type(slot, index, by_ref, &incoming_types)
                .expect("runtime wrapper argument source exists");
            load_wrapper_source(emitter, index, &source_ty);
            coercion::emit_wrapper_arg_conversion(
                module,
                emitter,
                &format!("{adapter}_arg_{index}"),
                index,
                &source_ty,
                semantic_target,
                type_expr,
                declared,
                source_is_ref_cell,
            );
            value_is_owned = coercion::wrapper_arg_temp_type(
                Some(&source_ty),
                semantic_target,
                type_expr,
                declared,
                source_is_ref_cell,
            )
            .is_some();
        } else if signature
            .defaults
            .get(index.saturating_sub(1))
            .is_some_and(Option::is_some)
        {
            coercion::emit_wrapper_default(
                emitter,
                class_id,
                method_name,
                index,
            );
            value_is_owned = true;
        } else {
            emit_missing_wrapper_arg(emitter, target_ty);
            value_is_owned = semantic_target.codegen_repr() == PhpType::Mixed;
        }
        if by_ref && !source_is_ref_cell {
            references::emit_wrapper_temp_ref_cell(
                emitter,
                semantic_target,
                value_is_owned,
            );
        }
        if let Some(cleanup) = arg_cleanups[index].first() {
            cleanup::store_temp(
                emitter,
                cleanup::storage_type(cleanup),
                adapter_slot_offset(cleanup_base + cleanup_index),
            );
        }
        cleanup_index += arg_cleanup_count;
        abi::emit_push_result_value(emitter, target_ty);
    }
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);
    let call_pad = abi::outgoing_call_stack_pad_bytes(module.target, overflow_bytes);
    abi::emit_reserve_temporary_stack(emitter, call_pad);
    abi::emit_call_label(emitter, &method_symbol(impl_class, method_name));
    abi::emit_release_temporary_stack(emitter, call_pad);
    abi::emit_release_temporary_stack(emitter, overflow_bytes);
    unwind::emit_boundary_pop(emitter, handler_base);

    preserve_wrapper_return(emitter, &signature.return_type, adapter_slot_offset(return_slot));
    cleanup::emit_all(
        emitter,
        &cleanup_entries,
        cleanup_base,
        &adapter,
        Some((&signature.return_type, adapter_slot_offset(return_slot))),
    );
    restore_wrapper_return(emitter, &signature.return_type, adapter_slot_offset(return_slot));
    returns::emit_normalize_wrapper_return(
        emitter,
        data,
        slot,
        &signature.return_type,
        &adapter,
        class_name,
        method_name,
        eof_check_mode_slot.map(adapter_slot_offset),
    );
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);

    emitter.label(&escape_label);
    unwind::emit_boundary_pop(emitter, handler_base);
    cleanup::emit_all(
        emitter,
        &cleanup_entries,
        cleanup_base,
        &format!("{adapter}_throw"),
        None,
    );
    cleanup::emit_owned_sources_on_throw(emitter, slot, &incoming_types);
    cleanup::emit_owned_receiver_on_throw(emitter, slot, &incoming_types[0]);
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_jump(emitter, "__rt_throw_current");
}

/// Spills every runtime callback argument before boxing can clobber caller registers.
fn spill_wrapper_runtime_args(emitter: &mut Emitter, incoming_types: &[PhpType]) {
    let mut cursor = abi::IncomingArgCursor::for_target(emitter.target, 0);
    for (index, php_type) in incoming_types.iter().enumerate() {
        abi::emit_store_incoming_param(
            emitter,
            &format!("wrapper_arg_{index}"),
            php_type,
            adapter_slot_offset(index),
            false,
            &mut cursor,
        );
    }
}

/// Preserves the runtime-only strictness mode passed after the stream-eof receiver.
fn spill_stream_eof_check_mode(emitter: &mut Emitter, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => abi::store_at_offset(emitter, "x1", offset),
        Arch::X86_64 => abi::store_at_offset(emitter, "rsi", offset),
    }
}

/// Materializes PHP null for a wrapper method parameter beyond the runtime contract arity.
fn emit_missing_wrapper_arg(emitter: &mut Emitter, target_ty: &PhpType) {
    match target_ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
            emit_box_current_value_as_mixed(emitter, &PhpType::Void);
        }
        PhpType::Float => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("fmov d0, xzr");                        // materialize PHP null's numeric zero fallback for an extra float parameter
                }
                Arch::X86_64 => {
                    emitter.instruction("xorpd xmm0, xmm0");                    // materialize PHP null's numeric zero fallback for an extra float parameter
                }
            }
        }
        PhpType::Str | PhpType::TaggedScalar => {
            let (lo, hi) = abi::string_result_regs(emitter);
            abi::emit_load_int_immediate(emitter, lo, 0);
            abi::emit_load_int_immediate(emitter, hi, 0);
        }
        _ => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        }
    }
}

/// Saves a wrapper method return value across temporary Mixed releases without changing ownership.
fn preserve_wrapper_return(emitter: &mut Emitter, return_ty: &PhpType, offset: usize) {
    match return_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::store_at_offset(emitter, ptr_reg, offset);
            abi::store_at_offset(emitter, len_reg, offset - 8);
        }
        PhpType::TaggedScalar => {
            abi::store_at_offset(emitter, abi::int_result_reg(emitter), offset);
            abi::store_at_offset(
                emitter,
                crate::codegen::sentinels::tagged_scalar_tag_reg(emitter),
                offset - 8,
            );
        }
        PhpType::Float => {
            abi::store_at_offset(emitter, abi::float_result_reg(emitter), offset);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::store_at_offset(emitter, abi::int_result_reg(emitter), offset);
        }
    }
}

/// Restores the wrapper method return value after temporary Mixed releases.
fn restore_wrapper_return(emitter: &mut Emitter, return_ty: &PhpType, offset: usize) {
    if !matches!(return_ty.codegen_repr(), PhpType::Void | PhpType::Never) {
        abi::emit_load(emitter, &return_ty.codegen_repr(), offset);
    }
}

/// Returns the frame-pointer-relative offset for one 16-byte adapter slot.
fn adapter_slot_offset(index: usize) -> usize {
    (index + 1) * 16
}

/// Returns the aligned adapter frame size for the requested slot count.
fn adapter_frame_size(slot_count: usize) -> usize {
    ((slot_count + 1) * 16 + 15) & !15
}

#[cfg(test)]
mod tests;
