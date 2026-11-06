//! Purpose:
//! Scopes PHP legacy reference sharing around serialization and hydration callbacks.
//!
//! Called from:
//! - Serialize and unserialize emitters before invoking PHP hook bodies.
//!
//! Key details:
//! - Modern hooks isolate nested wire operations; Serializable hooks share their registry.
//! - Public operation boundaries restore the saved mode when a callback throws.

use crate::codegen_support::{abi, emit::Emitter};

/// Emits typed callback adapters that retain arguments and all native return registers.
pub(super) fn emit(emitter: &mut Emitter) {
    for (name, symbol, mode, params) in [
        ("__rt_serialize_invoke_legacy", "_ser_legacy_active", 1, 1),
        ("__rt_serialize_invoke_modern", "_ser_legacy_active", 0, 1),
        ("__rt_unserialize_invoke_legacy", "_unser_legacy_active", 1, 3),
        ("__rt_unserialize_invoke_modern", "_unser_legacy_active", 0, 2),
        ("__rt_unserialize_invoke_wakeup", "_unser_legacy_active", 0, 1),
    ] {
        emit_hook(emitter, name, symbol, mode, params);
    }
}

/// Sets one hook's registry mode while preserving its receiver, data, target, and result ABI.
fn emit_hook(emitter: &mut Emitter, name: &str, symbol: &str, mode: i64, params: usize) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    // -- stage callback operands and publish this hook family's reference-sharing mode --
    emitter.blank();
    emitter.label_global(name);
    abi::emit_frame_prologue(emitter, 80);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, params), 16);
    for index in 0..params {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), 24 + index * 8);
    }
    abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_load_int_immediate(emitter, result, mode);
    abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    for index in 0..params {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), 24 + index * 8);
    }
    abi::load_at_offset(emitter, scratch, 16);
    abi::emit_call_reg(emitter, scratch);
    // -- preserve every native result word while restoring the enclosing mode --
    abi::store_at_offset(emitter, result, 48);
    let (pointer, length) = abi::string_result_regs(emitter);
    abi::store_at_offset(emitter, pointer, 56);
    abi::store_at_offset(emitter, length, 64);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    abi::load_at_offset(emitter, result, 48);
    abi::load_at_offset(emitter, pointer, 56);
    abi::load_at_offset(emitter, length, 64);
    abi::emit_frame_restore(emitter, 80);
    abi::emit_return(emitter);
}
