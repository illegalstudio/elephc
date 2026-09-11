//! Purpose:
//! Protects native temporaries allocated while adapting PHP calls.
//!
//! Called from:
//! - Callable lowering after normalizing and saving the argument container.
//! - Dynamic constructor argument materialization after creating caller-owned conversions.
//!
//! Key details:
//! - The argument box and optional descriptor already have one caller-owned reference.
//! - Stack records transfer those references only during exceptional unwinding.
//! - Conversion guards are published immediately so a later argument conversion may throw safely.
//! - Failed dynamic construction marks the raw object as destructor-suppressed before release.

use super::*;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::try_handlers::EXCEPTION_GUARD_SLOT_SIZE;

/// Reserves one exceptional owner record for each backend-created call argument.
pub(super) fn reserve_argument_temp_guards(emitter: &mut Emitter, count: usize) -> usize {
    let bytes = EXCEPTION_GUARD_SLOT_SIZE * count;
    if bytes > 0 {
        abi::emit_reserve_temporary_stack(emitter, bytes);
    }
    bytes
}

/// Guards one converted argument immediately after its caller-owned pointer is saved.
pub(super) fn register_argument_temp_guard(
    emitter: &mut Emitter,
    staged_arg_bytes: usize,
    cleanup_bytes: usize,
    cleanup_offset: usize,
    guard_index: usize,
) {
    let guard_offset = staged_arg_bytes + cleanup_bytes
        + guard_index * EXCEPTION_GUARD_SLOT_SIZE;
    let owner_offset = staged_arg_bytes + cleanup_offset;
    let previous_offset = (guard_index > 0).then(|| {
        staged_arg_bytes + cleanup_bytes + (guard_index - 1) * EXCEPTION_GUARD_SLOT_SIZE
    });
    guard_saved_owner_after(emitter, owner_offset, guard_offset, previous_offset);
}

/// Guards an owner and record already stored in a caller-reserved stack block.
pub(super) fn guard_saved_owner(
    emitter: &mut Emitter,
    owner_offset: usize,
    guard_offset: usize,
) {
    guard_saved_owner_after(emitter, owner_offset, guard_offset, None);
}

/// Publishes one saved owner after an optional preceding record in the same stack block.
fn guard_saved_owner_after(
    emitter: &mut Emitter,
    owner_offset: usize,
    guard_offset: usize,
    previous_offset: Option<usize>,
) {
    let scratch = abi::temp_int_reg(emitter.target);
    let guard_reg = abi::int_arg_reg_name(emitter.target, 0);
    let previous_reg = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_temporary_stack_slot(emitter, scratch, owner_offset);
    abi::emit_temporary_stack_address(emitter, guard_reg, guard_offset);
    abi::emit_store_to_address(
        emitter,
        scratch,
        guard_reg,
        crate::codegen_support::try_handlers::EXCEPTION_GUARD_OWNER_OFFSET,
    );
    abi::emit_symbol_address(emitter, scratch, "__rt_exception_release_owned");
    abi::emit_store_to_address(emitter, scratch, guard_reg, 8);
    if let Some(previous_offset) = previous_offset {
        abi::emit_temporary_stack_address(emitter, previous_reg, previous_offset);
    } else {
        abi::emit_load_int_immediate(emitter, previous_reg, 0);
    }
    abi::emit_call_label(emitter, "__rt_exception_guard_owned");
}

/// Removes a saved-owner record while leaving its owner for normal cleanup.
pub(super) fn unguard_saved_owner(emitter: &mut Emitter, guard_offset: usize) {
    abi::emit_temporary_stack_address(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        guard_offset,
    );
    abi::emit_call_label(emitter, "__rt_exception_unguard_owned");
}

/// Returns one guarded conversion owner to its normal cleanup path.
pub(super) fn remove_argument_temp_guard(
    emitter: &mut Emitter,
    cleanup_bytes: usize,
    guard_index: usize,
) {
    unguard_saved_owner(
        emitter,
        cleanup_bytes + guard_index * EXCEPTION_GUARD_SLOT_SIZE,
    );
}

/// Guards the saved argument box and optional descriptor, returning the added stack size.
pub(super) fn begin(emitter: &mut Emitter, owns_descriptor: bool) -> usize {
    let bytes = EXCEPTION_GUARD_SLOT_SIZE * if owns_descriptor { 2 } else { 1 };
    abi::emit_reserve_temporary_stack(emitter, bytes);
    if owns_descriptor {
        register(emitter, EXCEPTION_GUARD_SLOT_SIZE, bytes + 16, "__rt_exception_release_callable");
    }
    register(emitter, 0, bytes, "__rt_exception_release_owned");
    bytes
}

/// Guards a raw object until its dynamic constructor completes successfully.
pub(super) fn begin_unconstructed_object(emitter: &mut Emitter) -> usize {
    let bytes = EXCEPTION_GUARD_SLOT_SIZE;
    abi::emit_reserve_temporary_stack(emitter, bytes);
    register(
        emitter,
        0,
        bytes,
        "__rt_exception_release_unconstructed_object",
    );
    bytes
}

/// Removes temporary guards while preserving the successful boxed result and original owners.
pub(super) fn end(emitter: &mut Emitter, bytes: usize) {
    abi::emit_push_result_value(emitter, &PhpType::Mixed);
    for offset in (0..bytes).step_by(EXCEPTION_GUARD_SLOT_SIZE) {
        abi::emit_temporary_stack_address(emitter, abi::int_arg_reg_name(emitter.target, 0), offset + 16);
        abi::emit_call_label(emitter, "__rt_exception_unguard_owned");
    }
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_release_temporary_stack(emitter, bytes);
}

/// Removes a raw-object construction guard after the constructor returns normally.
pub(super) fn end_unconstructed_object(emitter: &mut Emitter, bytes: usize) {
    abi::emit_temporary_stack_address(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        0,
    );
    abi::emit_call_label(emitter, "__rt_exception_unguard_owned");
    abi::emit_release_temporary_stack(emitter, bytes);
}

/// Publishes one cleanup record for an owner already preserved in the temporary stack.
fn register(emitter: &mut Emitter, record: usize, owner: usize, cleanup: &str) {
    let scratch = abi::temp_int_reg(emitter.target);
    let base = match emitter.target.arch { Arch::AArch64 => "sp", Arch::X86_64 => "rsp" };
    abi::emit_load_temporary_stack_slot(emitter, scratch, owner);
    abi::emit_store_to_address(emitter, scratch, base, record + 24);
    abi::emit_symbol_address(emitter, scratch, cleanup);
    abi::emit_store_to_address(emitter, scratch, base, record + 8);
    abi::emit_temporary_stack_address(emitter, abi::int_arg_reg_name(emitter.target, 0), record);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 1), 0);
    abi::emit_call_label(emitter, "__rt_exception_guard_owned");
}
