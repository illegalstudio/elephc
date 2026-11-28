//! Purpose:
//! Retires application-worker roots while keeping PHP cleanup callbacks and catches valid.
//!
//! Called from:
//! - The generated application lifecycle shutdown entry.
//!
//! Key details:
//! - Memoir drains reentrant roots and protects each destructor.
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
    if module.required_runtime_features.eval_bridge {
        let shutdown = emitter.target.extern_symbol("__elephc_eval_pcntl_shutdown");
        scope.call(emitter, &shutdown, false);
    }
    emit_root_stabilization(emitter, module, data, scope, "initial");
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
    let end = emitter.target.extern_symbol("elephc_web_application_request_end");
    scope.call(emitter, &end, false);
    scope.call(emitter, "__rt_stream_registry_cleanup", false);
    let releases_extern_callbacks = crate::codegen::frame::module_binds_extern_callbacks(module);
    if releases_extern_callbacks {
        // Native registrations remain callable until every PHP/resource destructor has finished.
        scope.call(emitter, crate::codegen::frame::EXTERN_CALLBACK_RELEASE_LABEL, false);
    }
    if module.required_runtime_features.mbstring || module.required_runtime_features.eval_bridge {
        scope.call(emitter, "__rt_mbstring_release_catalog", false);
    }
    emit_root_stabilization(emitter, module, data, scope, "final");
    if releases_extern_callbacks {
        // Releasing a callback can detach cycles whose destructors orphan resources. Collect
        // first, then prune those resources while the protected shutdown scope is still active.
        scope.call(emitter, "__rt_web_inventory_prune", false);
    }
    // -- preserve a pending Throwable until the generated PHP catch can log it --
    scope.finish(emitter);
    abi::emit_load_symbol_to_reg(emitter, value, "_exc_value", 0);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_web_app_shutdown_clean");
    abi::emit_frame_restore(emitter, 64);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label("__rt_web_app_shutdown_clean");
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}

/// Drains application roots, collects detached cycles, and repeats when callbacks republish storage.
fn emit_root_stabilization(
    emitter: &mut Emitter,
    module: &Module,
    data: &DataSection,
    scope: Scope,
    phase: &str,
) {
    let (mut properties, hosting) = static_properties_hosting_last(module);
    properties.extend(hosting);
    let owned_globals = module
        .data
        .global_names
        .iter()
        .filter_map(|name| {
            let symbol = ir_global_symbol(name);
            (!superglobals::is_superglobal(name)
                && !module.extern_globals.contains_key(name)
                && data.has_comm(&symbol))
            .then_some(symbol)
        })
        .collect::<Vec<_>>();
    let enum_slots = super::super::super::enum_singletons::enum_owner_symbols(module);
    let restart = format!("__rt_web_app_shutdown_{phase}_root_drain");
    elephc_memoir::terminal_cleanup::emit_fixed_point_pass(
        emitter,
        &restart,
        |emitter, restart| emitter.label(restart),
        |emitter| {
            for (index, record) in data.static_locals().iter().enumerate() {
                let skip = format!("__rt_web_app_shutdown_{phase}_static_{index}");
                abi::emit_load_symbol_to_reg(
                    emitter,
                    abi::int_result_reg(emitter),
                    &record.init_symbol,
                    0,
                );
                abi::emit_branch_if_int_result_zero(emitter, &skip);
                abi::emit_drain_static_owner(
                    emitter,
                    &record.symbol,
                    &record.php_type.codegen_repr(),
                    &format!("{skip}_drain"),
                    Some(0),
                );
                abi::emit_store_zero_to_symbol(emitter, &record.init_symbol, 0);
                emitter.label(&skip);
            }
            for (index, (symbol, ty)) in properties.iter().enumerate() {
                if module.required_runtime_features.native_reference_validation
                    && ty.codegen_repr() == PhpType::Mixed
                {
                    abi::emit_symbol_address(emitter, abi::int_result_reg(emitter), symbol);
                    scope.call(emitter, "__rt_reference_static_mixed_release", false);
                    continue;
                }
                let skip = format!("__rt_web_app_shutdown_{phase}_property_{index}");
                abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), symbol, 8);
                super::super::emit_branch_if_equals_sentinel(emitter, &skip);
                abi::emit_drain_static_owner(
                    emitter,
                    symbol,
                    &ty.codegen_repr(),
                    &format!("{skip}_drain"),
                    Some(UNINITIALIZED_TYPED_PROPERTY_SENTINEL),
                );
                emitter.label(&skip);
            }
            for (index, symbol) in owned_globals.iter().enumerate() {
                abi::emit_drain_static_owner(
                    emitter,
                    symbol,
                    &PhpType::Mixed,
                    &format!("__rt_web_app_shutdown_{phase}_global_{index}"),
                    None,
                );
            }
            super::super::super::enum_singletons::emit_enum_slot_cleanup(
                emitter,
                module,
                &format!("__rt_web_app_shutdown_{phase}_enum_case"),
            );
        },
        |emitter| {
            scope.allow_collection(emitter);
            scope.call(emitter, "__rt_gc_collect_cycles_explicit", false);
            scope.suppress_collection(emitter);
        },
        |emitter, restart| {
            let symbols = owned_globals
                .iter()
                .chain(properties.iter().map(|(symbol, _)| symbol))
                .chain(data.static_locals().iter().filter_map(|record| {
                    let ty = record.php_type.codegen_repr();
                    (ty == PhpType::Str || abi::refcount_release_helper(&ty).is_some())
                        .then_some(&record.symbol)
                }));
            for (index, symbol) in symbols.enumerate() {
                elephc_memoir::symbol_owners::emit_branch_if_published_owner(
                    emitter,
                    symbol,
                    restart,
                    &format!("__rt_web_app_shutdown_{phase}_root_{index}_empty"),
                );
            }
            for (index, symbol) in enum_slots.iter().enumerate() {
                elephc_memoir::symbol_owners::emit_branch_if_published_owner(
                    emitter,
                    symbol,
                    restart,
                    &format!("__rt_web_app_shutdown_{phase}_enum_{index}_empty"),
                );
            }
        },
    );
}
