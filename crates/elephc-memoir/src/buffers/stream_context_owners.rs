//! Purpose:
//! Owns temporary stream-context options and notification callbacks in application requests.
//!
//! Called from:
//! - Application request boundaries and stream-context lowering in application builds.
//!
//! Key details:
//! - A replacement is published before releasing the preceding owner, including on re-entry.
//! - Options retain a borrowed input; callback publication adopts an already retained descriptor.
//! - The caller selects application or legacy lifetime policy.

use crate::{abi, cleanup::Scope as CleanupScope, Emitter};

const OPTIONS: &str = "_stream_context_options";
const NOTIFICATION: &str = "_stream_notification_callback";

/// Materializes the address of the ambient stream-context options owner slot.
pub fn emit_options_address(emitter: &mut impl Emitter, destination: &str) {
    emitter.emit_symbol_address(destination, OPTIONS);
}

/// Loads the ambient stream-context options owner.
pub fn emit_load_options(emitter: &mut impl Emitter, destination: &str) {
    emitter.emit_load_symbol_to_reg(destination, OPTIONS, 0);
}

/// Publishes the ambient stream-context options owner.
pub fn emit_store_options(emitter: &mut impl Emitter, source: &str) {
    emitter.emit_store_reg_to_symbol(source, OPTIONS, 0);
}

/// Clears both ambient stream-context owner slots without invoking releases.
pub fn emit_clear_ambient_slots(emitter: &mut impl Emitter) {
    emitter.emit_store_zero_to_symbol(OPTIONS, 0);
    emitter.emit_store_zero_to_symbol(NOTIFICATION, 0);
}

/// Emits two ownership-aware stores and a resumable ambient request reset.
pub fn emit(emitter: &mut impl Emitter) {
    // Empty options can retain indexed-array storage, so release by runtime kind.
    store(emitter, "options", OPTIONS, "__rt_decref_any", true);
    store(emitter, "notification", NOTIFICATION, "__rt_callable_descriptor_release", false);
    let scope = CleanupScope { arm: 0, x86: 40 };
    let result = abi::int_result_reg(emitter);
    emitter.label_global("__rt_web_ambient_reset");
    abi::emit_frame_prologue(emitter, 64);
    scope.begin(emitter);
    emitter.label("__rt_web_ambient_reset_again");
    for (symbol, helper) in [
        (OPTIONS, "__rt_decref_any"),
        (NOTIFICATION, "__rt_callable_descriptor_release"),
    ] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
        scope.call(emitter, helper, false);
    }
    for symbol in [OPTIONS, NOTIFICATION] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::emit_branch_if_int_result_nonzero(emitter, "__rt_web_ambient_reset_again");
    }
    finish(emitter, scope, "__rt_web_ambient_reset_done");
}

/// Publishes one retained input before retiring the detached preceding value.
fn store(emitter: &mut impl Emitter, suffix: &str, symbol: &str, release: &str, retain: bool) {
    let result = abi::int_result_reg(emitter);
    let scope = CleanupScope { arm: 0, x86: 40 };
    emitter.label_global(&format!("__rt_web_context_{suffix}_store"));
    // -- retain inputs before a previous owner's destructor can re-enter this store --
    abi::emit_frame_prologue(emitter, 64);
    abi::store_at_offset(emitter, result, 16);
    if retain { abi::emit_call_label(emitter, "__rt_incref"); }
    abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
    abi::store_at_offset(emitter, result, 8);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    scope.begin(emitter);
    abi::load_at_offset(emitter, result, 8);
    scope.call(emitter, release, false);
    finish(emitter, scope, &format!("__rt_web_context_{suffix}_stored"));
}

/// Propagates a cleanup exception only after restoring the native frame and collector state.
fn finish(emitter: &mut impl Emitter, scope: CleanupScope, done: &str) {
    scope.finish(emitter);
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label(done);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}

/// Writes the legacy ambient callback slot, with either the prepared descriptor or zero.
pub fn emit_legacy_notification_store(emitter: &mut impl Emitter, clear: bool) {
    let address = emitter.symbol_scratch_reg();
    let value = if clear { emitter.secondary_scratch_reg() } else { emitter.int_result_reg() };
    emitter.emit_symbol_address(address, NOTIFICATION);
    if clear {
        emitter.emit_load_int_immediate(value, 0);
    }
    emitter.emit_store_to_address(value, address, 0);
}
