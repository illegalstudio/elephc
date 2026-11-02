//! Purpose:
//! Retires application-worker roots while keeping PHP cleanup callbacks and catches valid.
//!
//! Called from:
//! - The generated application lifecycle shutdown entry.
//!
//! Key details:
//! - Each root is detached before protected release, including throwing destructors.
//! - Superglobals stay valid arrays until PHP callbacks finish.
//! - The process owns the final arena reclamation; live exception frames are never reset.

use super::*;

/// Finishes all root releases before returning a possible cleanup exception to the PHP adapter.
pub(super) fn emit(emitter: &mut Emitter, module: &Module, data: &DataSection) {
    let entry = emitter.target.extern_symbol("elephc_web_application_shutdown_cleanup");
    emitter.label_global(&entry);
    abi::emit_frame_prologue(emitter, 64);
    let scope = Scope { arm: 0, x86: 16 };
    let value = abi::int_result_reg(emitter);
    scope.begin(emitter);
    // -- detach persistent roots before their destructors can reenter application code --
    for (index, record) in data.static_locals().iter().enumerate() {
        let skip = format!("__rt_web_app_shutdown_static_{index}");
        abi::emit_load_symbol_to_reg(emitter, value, &record.init_symbol, 0);
        abi::emit_branch_if_int_result_zero(emitter, &skip);
        abi::emit_store_zero_to_symbol(emitter, &record.init_symbol, 0);
        release(emitter, scope, &record.symbol, &record.php_type.codegen_repr(), false);
        emitter.label(&skip);
    }
    let (properties, hosting) = static_properties_hosting_last(module);
    for (index, (symbol, ty)) in properties.into_iter().chain(hosting).enumerate() {
        let skip = format!("__rt_web_app_shutdown_property_{index}");
        abi::emit_load_symbol_to_reg(emitter, value, &symbol, 8);
        super::super::emit_branch_if_equals_sentinel(emitter, &skip);
        release(emitter, scope, &symbol, &ty.codegen_repr(), true);
        emitter.label(&skip);
    }
    for name in &module.data.global_names {
        if !superglobals::is_superglobal(name) && !module.extern_globals.contains_key(name) {
            release(emitter, scope, &ir_global_symbol(name), &PhpType::Mixed, false);
        }
    }
    if crate::codegen::context::module_uses_pcntl_signal_handlers(module) {
        scope.call(emitter, "__rt_pcntl_release_handlers", false);
    }
    // -- retire registrations while every callback still sees live superglobal containers --
    for kind in ["error", "exception"] {
        let again = format!("__rt_web_app_shutdown_{kind}_handler");
        emitter.label(&again);
        scope.call(emitter, &format!("__rt_core_{kind}_handler_pop"), false);
        for suffix in ["stack", "value", "callable", "context", "context_release"] {
            abi::emit_load_symbol_to_reg(emitter, value, &format!("_php_{kind}_handler_{suffix}"), 0);
            abi::emit_branch_if_int_result_nonzero(emitter, &again);
        }
    }
    // Allow final cycle collection once no persistent owner points at an incomplete object.
    scope.finish(emitter);
    let end = emitter.target.extern_symbol("elephc_web_application_request_end");
    scope.call(emitter, &end, false);
    if module.required_runtime_features.mbstring || module.required_runtime_features.eval_bridge {
        scope.call(emitter, "__rt_mbstring_release_catalog", false);
    }
    scope.call(emitter, "__rt_stream_registry_cleanup", false);
    if crate::codegen::frame::module_binds_extern_callbacks(module) {
        // Native registrations remain callable until every PHP/resource destructor has finished.
        scope.call(emitter, crate::codegen::frame::EXTERN_CALLBACK_RELEASE_LABEL, false);
        // Retiring the last native lease can detach cycles and resources captured by callbacks.
        scope.call(emitter, "__rt_gc_collect_cycles_explicit", false);
        scope.call(emitter, "__rt_web_inventory_prune", false);
    }
    // -- preserve a pending Throwable until the generated PHP catch can log it --
    scope.finish(emitter);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_web_app_shutdown_clean");
    abi::emit_frame_restore(emitter, 64);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label("__rt_web_app_shutdown_clean");
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}

/// Clears one symbol before releasing its single owned payload under the shutdown scope.
fn release(emitter: &mut Emitter, scope: Scope, symbol: &str, ty: &PhpType, property: bool) {
    let helper = if *ty == PhpType::Str { Some("__rt_heap_free_safe") }
        else { abi::refcount_release_helper(ty) };
    let Some(helper) = helper else { return };
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), symbol, 0);
    abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    if property {
        let marker = abi::secondary_scratch_reg(emitter);
        abi::emit_load_int_immediate(emitter, marker, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
        abi::emit_store_reg_to_symbol(emitter, marker, symbol, 8);
    } else {
        abi::emit_store_zero_to_symbol(emitter, symbol, 8);
    }
    scope.call(emitter, helper, false);
}
