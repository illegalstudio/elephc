//! Purpose:
//! Owns descriptor argument conversions through a guarded, lazily allocated Mixed array.
//!
//! Called from:
//! - Native callable invoker setup, typed argument staging, and return boxing.
//!
//! Key details:
//! - Captured values transfer one owner; the ABI argument registers keep borrowed views.
//! - The guard follows array growth and participates in the existing exception activation chain.
//! - A separate result guard protects the return value if argument destruction throws.

use super::*;
use crate::codegen_support::try_handlers::EXCEPTION_GUARD_OWNER_OFFSET;

/// Registers an empty ledger after the eval boundary has snapshotted its caller activation.
pub(super) fn initialize(emitter: &mut Emitter) {
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    register(emitter, INVOKER_ARGUMENT_GUARD);
}

/// Transfers the current owned argument into the ledger while preserving its borrowed ABI representation.
pub(super) fn capture(emitter: &mut Emitter, ctx: &mut InvokerEmitContext, ty: &PhpType) {
    abi::emit_push_result_value(emitter, ty);
    emit_box_current_owned_value_as_mixed(emitter, ty);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    let ready = ctx.next_label("invoker_argument_owners_ready");
    load_owner(emitter, INVOKER_ARGUMENT_GUARD);
    abi::emit_branch_if_int_result_nonzero(emitter, &ready);
    let target = emitter.target;
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 0), 4);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 1), 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(emitter, abi::int_result_reg(emitter), &PhpType::Mixed);
    store_owner(emitter, INVOKER_ARGUMENT_GUARD);
    emitter.label(&ready);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(target, 0), abi::int_result_reg(emitter));
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(target, 1));
    // The scalar append transfers the boxed owner without retaining it a second time.
    abi::emit_call_label(emitter, "__rt_array_push_int");
    store_owner(emitter, INVOKER_ARGUMENT_GUARD);
    if *ty == PhpType::Str {
        let (pointer, length) = abi::string_result_regs(emitter);
        abi::emit_pop_reg_pair(emitter, pointer, length);
    } else {
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    }
}

/// Releases argument conversions after boxing the independent return value, including throwing cleanup.
pub(super) fn finish(emitter: &mut Emitter) {
    register(emitter, INVOKER_RESULT_GUARD);
    detach(emitter, INVOKER_ARGUMENT_GUARD);
    abi::emit_call_label(emitter, "__rt_decref_array");
    detach(emitter, INVOKER_RESULT_GUARD);
}

/// Publishes a guard with the current pointer as its sole exceptional owner.
fn register(emitter: &mut Emitter, guard: usize) {
    store_owner(emitter, guard);
    let scratch = abi::temp_int_reg(emitter.target);
    abi::emit_symbol_address(emitter, scratch, "__rt_exception_release_owned");
    abi::store_at_offset(emitter, scratch, guard - 8);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 1), 0);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), guard);
    abi::emit_call_label(emitter, "__rt_exception_guard_owned");
}

/// Removes one exact activation and returns its transferred owner without executing destructors.
fn detach(emitter: &mut Emitter, guard: usize) {
    load_owner(emitter, guard);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), guard);
    abi::emit_call_label(emitter, "__rt_exception_unguard_owned");
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
}

/// Reads the current ledger or return owner from its stack-local guard.
fn load_owner(emitter: &mut Emitter, guard: usize) {
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), guard - EXCEPTION_GUARD_OWNER_OFFSET);
}

/// Updates the guard immediately after allocation or ledger growth transfers the pointer.
fn store_owner(emitter: &mut Emitter, guard: usize) {
    abi::store_at_offset(emitter, abi::int_result_reg(emitter), guard - EXCEPTION_GUARD_OWNER_OFFSET);
}
