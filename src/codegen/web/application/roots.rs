//! Purpose:
//! Publishes actual owning global/static roots to application candidate discovery.
//!
//! Called from:
//! - The cycle collector through the callback installed after the worker arena reset.
//!
//! Key details:
//! - Inactive statics and uninitialized properties do not prove a live owner.
//! - The visitor borrows roots and never invokes PHP or traverses container contents.

use super::*;

/// Emits the per-program root visitor whose layouts are known to the EIR backend.
pub(super) fn emit(emitter: &mut Emitter, module: &Module, data: &DataSection) {
    emitter.blank();
    emitter.comment("--- web application: candidate roots ---");
    emitter.label_global("__rt_web_candidate_roots");
    abi::emit_frame_prologue(emitter, 16);
    let value = abi::int_result_reg(emitter);
    for (index, record) in data.static_locals().iter().enumerate() {
        if abi::refcount_release_helper(&record.php_type.codegen_repr()).is_none() { continue; }
        let skip = format!("__rt_web_candidate_static_{index}");
        abi::emit_load_symbol_to_reg(emitter, value, &record.init_symbol, 0);
        abi::emit_branch_if_int_result_zero(emitter, &skip);
        borrow(emitter, &record.symbol);
        emitter.label(&skip);
    }
    let (properties, hosting) = static_properties_hosting_last(module);
    for (index, (symbol, _)) in properties.into_iter().chain(hosting).enumerate() {
        let skip = format!("__rt_web_candidate_property_{index}");
        abi::emit_load_symbol_to_reg(emitter, value, &symbol, 8);
        super::super::emit_branch_if_equals_sentinel(emitter, &skip);
        borrow(emitter, &symbol);
        emitter.label(&skip);
    }
    for name in &module.data.global_names {
        if !module.extern_globals.contains_key(name) {
            borrow(emitter, &ir_global_symbol(name));
        }
    }
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

/// Borrows one currently owned root; native validation rejects null and non-graph storage.
fn borrow(emitter: &mut Emitter, symbol: &str) {
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), symbol, 0);
    abi::emit_call_label(emitter, "__rt_gc_candidate_root");
}
