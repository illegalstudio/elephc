//! Purpose:
//! Emits request boundaries for an application-owned PHP heap without bulk reclamation.
//!
//! Called from:
//! - `super::emit_web_reset()` for application lifecycle builds only.
//!
//! Key details:
//! - Application globals and statics remain ordinary owning roots.
//! - Each request detaches its superglobals before releasing them.
//! - Resumable cleanup finishes every release before propagating a destructor exception.

use super::*;
use crate::codegen_support::runtime::CleanupScope as Scope;

mod resources;
mod ambient;
mod shutdown;
mod roots;

/// Emits the adapter's three native no-argument boundary functions.
pub(super) fn emit(emitter: &mut Emitter, module: &Module, data: &DataSection) {
    roots::emit(emitter, module, data);
    let visited = emitter.target.extern_symbol("__elephc_web_gc_visited");
    emitter.label_global(&visited);
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), "_gc_candidates_visited", 0);
    let proof_count = if emitter.target.arch == Arch::AArch64 { "x9" } else { "r10" };
    abi::emit_load_symbol_to_reg(emitter, proof_count, "_gc_proofs_visited", 0);
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { "add x0, x0, x9" } else { "add rax, r10" }); // include cache construction and current-path validation in measured GC work
    abi::emit_return(emitter);
    let cost = emitter.target.extern_symbol("__elephc_web_gc_rebuild_cost");
    emitter.label_global(&cost);
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), "_gc_proofs_cost", 0);
    abi::emit_return(emitter);
    let cache = emitter.target.extern_symbol("elephc_web_application_cache_roots");
    emitter.label_global(&cache);
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_call_label(emitter, "__rt_gc_proofs_refresh");
    abi::emit_call_label(emitter, "__rt_gc_clear_candidate_anchors");
    abi::emit_store_zero_to_symbol(emitter, "_gc_candidates_gathering", 0);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
    resources::emit(emitter);
    ambient::emit(emitter);
    emitter.blank();
    let begin = emitter.target.extern_symbol("elephc_web_application_request_begin");
    emitter.label_global(&begin);
    abi::emit_frame_prologue(emitter, RESET_FRAME_SIZE);
    abi::emit_call_label(emitter, "__rt_web_ambient_reset");
    // -- restore scalar request controls without disturbing application ownership or active catches --
    for symbol in [
        "_json_active_flags", "_json_active_depth", "_json_indent_depth",
        "_json_validate_idx", "_json_validate_ptr", "_json_validate_len",
        "_json_decode_assoc", "_json_error_source_ptr", "_json_error_source_len",
        "_json_error_candidate_ptr", "_json_error_location_active", "_json_last_error",
        "_json_error_line", "_json_error_column", "_json_error_scan_count", "_json_error_scan_bytes",
        "_ser_value_counter", "_ser_legacy_active", "_unser_legacy_active",
        "_ser_obj_count", "_unser_count", "_unser_active", "_php_backtrace_next_line",
        "_http_active_host_ptr", "_http_active_proxy_ptr",
    ] {
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
    abi::emit_store_imm_to_symbol(emitter, "_gc_enabled", 0, 1);
    if module.required_runtime_features.mbstring || module.required_runtime_features.eval_bridge {
        abi::emit_call_label(emitter, "__rt_mbstring_request_reset");
    }
    if module.required_runtime_features.eval_bridge {
        let restart = emitter.target.extern_symbol("__elephc_eval_opcache_apply_restart");
        abi::emit_call_label(emitter, &restart);
    }
    emit_concat_offset_reset(emitter);
    abi::emit_call_label(emitter, "__rt_gc_request_start");
    abi::emit_frame_restore(emitter, RESET_FRAME_SIZE);
    abi::emit_return(emitter);

    let end = emitter.target.extern_symbol("elephc_web_application_request_end");
    emitter.label_global(&end);
    abi::emit_frame_prologue(emitter, 64);
    let cleanup = Scope { arm: 0, x86: 16 };
    cleanup.begin(emitter);
    cleanup.call(emitter, "__rt_ob_flush_all", false);
    cleanup.call(emitter, "__rt_web_ambient_reset", false);
    for name in superglobals::SUPERGLOBALS {
        let symbol = ir_global_symbol(name);
        abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), &symbol, 0);
        abi::store_at_offset(emitter, abi::int_result_reg(emitter), 24);
        // Keep every typed Hash slot valid while destructors can read or write superglobals.
        abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 0);
        abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 1), 7);
        abi::emit_call_label(emitter, "__rt_hash_new");
        abi::emit_store_reg_to_symbol(emitter, abi::int_result_reg(emitter), &symbol, 0);
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), 24);
        cleanup.call(emitter, "__rt_decref_hash", false);
    }
    for symbol in ["_cstr_base", "_cstr2_base", "_print_r_base"] {
        abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), symbol, 0);
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
        cleanup.call(emitter, "__rt_heap_free_safe", false);
    }
    emit_cstr_buffer_reset(emitter);
    emit_print_r_capture_reset(emitter);
    emit_output_buffer_reset(emitter);
    // Collector callbacks need the real owner graph after every request root is detached.
    cleanup.finish(emitter);
    cleanup.call(emitter, "__rt_gc_collect_cycles_explicit", false);
    cleanup.call(emitter, "__rt_gc_proofs_refresh_pending", false);
    cleanup.call(emitter, "__rt_web_inventory_prune", false);
    cleanup.finish(emitter);
    let done = "__rt_web_application_request_clean";
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label(done);
    emit_concat_offset_reset(emitter);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);

    shutdown::emit(emitter, module, data);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Native callback owners retire after PHP and resource destructors on every supported ABI.
    #[test]
    fn application_shutdown_releases_extern_callbacks_after_resources() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut module = Module::new(target);
            module.extern_decls.push(crate::ir::ExternDecl {
                name: "signal".into(),
                params: vec![crate::ir::ExternParamDecl {
                    name: "handler".into(),
                    ir_type: crate::ir::IrType::I64,
                    php_type: PhpType::Callable,
                    abi: crate::types::ExternAbiType::Callable,
                }],
                return_type: crate::ir::IrType::I64,
                return_php_type: PhpType::Int,
                return_abi: crate::types::ExternAbiType::INT64,
                link_libs: vec![],
            });
            module.data.global_names.push("probe".into());
            let mut emitter = Emitter::new(target);
            emit(&mut emitter, &module, &DataSection::new());
            let assembly = emitter.output();
            let entry = assembly.find(&format!("{}:\n", target.extern_symbol("elephc_web_application_shutdown_cleanup"))).unwrap();
            let shutdown = &assembly[entry..];
            let release = shutdown.find(crate::codegen::frame::EXTERN_CALLBACK_RELEASE_LABEL).unwrap();
            let global = shutdown.find(&ir_global_symbol("probe")).unwrap();
            let resources = shutdown.find("__rt_stream_registry_cleanup").unwrap();
            assert!(global < resources && resources < release, "{name}: {shutdown}");
            let released = &shutdown[release..];
            let collect = released.find("__rt_gc_collect_cycles_explicit").unwrap();
            let prune = released.find("__rt_web_inventory_prune").unwrap();
            assert!(collect < prune, "{name}: {released}");
        }
    }

    /// Every supported ABI retains the arena and emits protected cleanup before cycle collection.
    #[test]
    fn application_boundaries_preserve_heap_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter, &Module::new(target), &DataSection::new());
            let assembly = emitter.output();
            let begin = assembly.find(&format!("{}:\n", target.extern_symbol("elephc_web_application_request_begin"))).unwrap();
            let end = assembly.find(&format!("{}:\n", target.extern_symbol("elephc_web_application_shutdown_cleanup"))).unwrap();
            let request = &assembly[begin..end];
            assert!(!request.contains("_heap_off"), "{name}");
            assert!(!request.contains("_heap_free_list"), "{name}");
            assert!(!request.contains("_gc_live"), "{name}");
            assert!(request.contains("__rt_cleanup_call"), "{name}");
            assert!(request.find("__rt_decref_hash").unwrap() < request.find("__rt_gc_collect_cycles_explicit").unwrap(), "{name}");
            assert!(request.contains("__rt_web_inventory_prune"), "{name}");
        }
    }
}
