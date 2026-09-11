//! Purpose:
//! Protects native temporaries allocated while adapting a descriptor invocation.
//!
//! Called from:
//! - Callable lowering after normalizing and saving the argument container.
//!
//! Key details:
//! - The argument box and optional descriptor already have one caller-owned reference.
//! - Stack records transfer those references only during exceptional unwinding.
//! - Failed dynamic construction marks the raw object as destructor-suppressed before release.

use super::*;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::try_handlers::EXCEPTION_GUARD_SLOT_SIZE;

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
