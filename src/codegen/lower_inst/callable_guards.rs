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

/// Removes a saved-owner record while leaving its owner for normal cleanup.
pub(super) fn unguard_saved_owner(emitter: &mut Emitter, guard_offset: usize) {
    abi::emit_temporary_stack_address(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        guard_offset,
    );
    abi::emit_call_label(emitter, "__rt_exception_unguard_owned");
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
