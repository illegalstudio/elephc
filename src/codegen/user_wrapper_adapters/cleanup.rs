//! Purpose:
//! Tracks and releases caller-owned values created by userspace stream-wrapper adapters.
//! Handles both ordinary returns and callback Throwables contained by the adapter boundary.
//!
//! Called from:
//! - `crate::codegen::user_wrapper_adapters::emit_user_wrapper_adapter()`.
//!
//! Key details:
//! - Cleanup slots are initialized before `setjmp` so partial materialization is safe.
//! - An ordinary return may transfer an argument owner when both values alias.

use crate::codegen::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::types::PhpType;

use super::adapter_slot_offset;

/// One owned adapter value that must be released after the callback returns or throws.
#[derive(Clone, Debug)]
pub(super) enum WrapperCleanup {
    Value(PhpType),
    RefCell(PhpType),
    BorrowedRefCell(PhpType),
}

/// Returns whether a borrowed callback reference cell can own a releasable payload.
pub(super) fn ref_cell_payload_needs_cleanup(value_ty: &PhpType) -> bool {
    let value_ty = value_ty.codegen_repr();
    value_ty == PhpType::Str || value_ty == PhpType::Callable || value_ty.is_refcounted()
}

/// Returns the ABI type stored in the cleanup slot for this owner.
pub(super) fn storage_type(cleanup: &WrapperCleanup) -> &PhpType {
    match cleanup {
        WrapperCleanup::Value(temp_ty) => temp_ty,
        WrapperCleanup::RefCell(_) | WrapperCleanup::BorrowedRefCell(_) => &PhpType::Int,
    }
}

/// Saves one caller-owned adapter temporary without adding another owner.
pub(super) fn store_temp(emitter: &mut Emitter, temp_ty: &PhpType, offset: usize) {
    match temp_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::store_at_offset(emitter, ptr_reg, offset);
            abi::store_at_offset(emitter, len_reg, offset - 8);
        }
        PhpType::Float => {
            abi::store_at_offset(emitter, abi::float_result_reg(emitter), offset);
        }
        _ => {
            abi::store_at_offset(emitter, abi::int_result_reg(emitter), offset);
        }
    }
}

/// Initializes every optional callback owner slot so exceptional cleanup is idempotent.
pub(super) fn initialize_slots(emitter: &mut Emitter, cleanup_base: usize, count: usize) {
    for index in 0..count {
        abi::emit_store_zero_to_local_slot(
            emitter,
            adapter_slot_offset(cleanup_base + index),
        );
    }
}

/// Releases every adapter-owned value, optionally preserving a returned alias.
pub(super) fn emit_all(
    emitter: &mut Emitter,
    cleanup_entries: &[WrapperCleanup],
    cleanup_base: usize,
    label_prefix: &str,
    returned: Option<(&PhpType, usize)>,
) {
    for (index, cleanup) in cleanup_entries.iter().enumerate() {
        let cleanup_offset = adapter_slot_offset(cleanup_base + index);
        let done_label = format!("{label_prefix}_cleanup_done_{index}");
        match cleanup {
            WrapperCleanup::Value(cleanup_ty) => {
                abi::emit_load(emitter, cleanup_ty, cleanup_offset);
                emit_skip_if_null(emitter, cleanup_ty, &done_label);
                if let Some((return_ty, return_offset)) = returned {
                    emit_skip_on_return_alias(
                        emitter,
                        return_ty,
                        cleanup_ty,
                        return_offset,
                        &done_label,
                    );
                }
                release_temp(emitter, cleanup_ty);
            }
            WrapperCleanup::RefCell(value_ty) => {
                let cell_reg = abi::secondary_scratch_reg(emitter);
                abi::load_at_offset(emitter, cell_reg, cleanup_offset);
                emit_branch_if_register_zero(emitter, cell_reg, &done_label);
                abi::emit_release_local_ref_cell(emitter, cell_reg, value_ty);
            }
            WrapperCleanup::BorrowedRefCell(value_ty) => {
                let cell_reg = abi::secondary_scratch_reg(emitter);
                abi::load_at_offset(emitter, cell_reg, cleanup_offset);
                emit_branch_if_register_zero(emitter, cell_reg, &done_label);
                release_borrowed_ref_cell_payload(emitter, cell_reg, value_ty);
            }
        }
        emitter.label(&done_label);
    }
}

/// Releases the throwaway wrapper instance owned by path/open dispatch after a Throwable.
pub(super) fn emit_owned_receiver_on_throw(
    emitter: &mut Emitter,
    slot: usize,
    receiver_ty: &PhpType,
) {
    if !matches!(slot, 0 | 9 | 14..=19) {
        return;
    }
    abi::emit_load(emitter, receiver_ty, adapter_slot_offset(0));
    if slot == 0 {
        abi::emit_call_label(emitter, "__rt_object_free_deep");
    } else {
        abi::emit_call_label(emitter, "__rt_decref_any");
    }
}

/// Releases runtime-created callback inputs whose native caller was skipped by `longjmp`.
pub(super) fn emit_owned_sources_on_throw(
    emitter: &mut Emitter,
    slot: usize,
    incoming_types: &[PhpType],
) {
    if slot != 14 {
        return;
    }
    let metadata_value = incoming_types
        .get(3)
        .expect("stream_metadata runtime contract includes its boxed value");
    abi::emit_load(emitter, metadata_value, adapter_slot_offset(3));
    abi::emit_call_label(emitter, "__rt_decref_any");
}

/// Releases one caller-owned adapter conversion temporary.
fn release_temp(emitter: &mut Emitter, temp_ty: &PhpType) {
    if temp_ty.codegen_repr() == PhpType::Str {
        let (ptr_reg, _) = abi::string_result_regs(emitter);
        let int_reg = abi::int_result_reg(emitter);
        if ptr_reg != int_reg {
            emitter.instruction(&format!("mov {}, {}", int_reg, ptr_reg));      // move the owned converted-string pointer into the heap-free ABI register
        }
        abi::emit_call_label(emitter, "__rt_heap_free_safe");
        return;
    }
    abi::emit_decref_if_refcounted(emitter, temp_ty);
}

/// Releases and clears a payload stored in a caller-owned reference cell.
fn release_borrowed_ref_cell_payload(
    emitter: &mut Emitter,
    cell_reg: &str,
    value_ty: &PhpType,
) {
    abi::emit_push_reg(emitter, cell_reg);
    match value_ty.codegen_repr() {
        PhpType::Str => {
            abi::emit_load_from_address(
                emitter,
                abi::int_result_reg(emitter),
                cell_reg,
                0,
            );
            abi::emit_call_label(emitter, "__rt_heap_free_safe");
        }
        PhpType::Callable => {
            abi::emit_load_from_address(
                emitter,
                abi::int_result_reg(emitter),
                cell_reg,
                0,
            );
            crate::codegen::callable_descriptor::emit_release_current_descriptor(emitter);
        }
        refcounted if refcounted.is_refcounted() => {
            abi::emit_load_from_address(
                emitter,
                abi::int_result_reg(emitter),
                cell_reg,
                0,
            );
            abi::emit_decref_if_refcounted(emitter, &refcounted);
        }
        _ => {}
    }
    abi::emit_pop_reg(emitter, cell_reg);
    abi::emit_store_zero_to_address(emitter, cell_reg, 0);
    abi::emit_store_zero_to_address(emitter, cell_reg, 8);
}

/// Skips one owner cleanup when its pointer slot was never initialized.
fn emit_skip_if_null(emitter: &mut Emitter, temp_ty: &PhpType, done_label: &str) {
    let pointer_reg = if temp_ty.codegen_repr() == PhpType::Str {
        abi::string_result_regs(emitter).0
    } else {
        abi::int_result_reg(emitter)
    };
    emit_branch_if_register_zero(emitter, pointer_reg, done_label);
}

/// Branches when an arbitrary integer register contains a null pointer.
fn emit_branch_if_register_zero(emitter: &mut Emitter, register: &str, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz {}, {}", register, label));       // skip an adapter owner that was never materialized
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", register, register));   // check whether the optional adapter owner exists
            emitter.instruction(&format!("jz {}", label));                      // skip an adapter owner that was never materialized
        }
    }
}

/// Skips cleanup when a method transferred the same owner as its return value.
fn emit_skip_on_return_alias(
    emitter: &mut Emitter,
    return_ty: &PhpType,
    cleanup_ty: &PhpType,
    return_offset: usize,
    alias_label: &str,
) {
    if return_ty.codegen_repr() != cleanup_ty.codegen_repr()
        || !matches!(
            cleanup_ty.codegen_repr(),
            PhpType::Str
                | PhpType::Mixed
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_)
                | PhpType::Iterable
        )
    {
        return;
    }
    let returned = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, returned, return_offset);
    let cleanup_reg = if cleanup_ty.codegen_repr() == PhpType::Str {
        abi::string_result_regs(emitter).0
    } else {
        abi::int_result_reg(emitter)
    };
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, {}", cleanup_reg, returned)); // compare the caller-owned temporary with the returned owner pointer
            emitter.instruction(&format!("b.eq {}", alias_label));              // transferred return ownership keeps the aliased callback argument alive
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", cleanup_reg, returned)); // compare the caller-owned temporary with the returned owner pointer
            emitter.instruction(&format!("je {}", alias_label));                // transferred return ownership keeps the aliased callback argument alive
        }
    }
}
