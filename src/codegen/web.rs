//! Purpose:
//! Generates the per-program `__rt_web_reset` routine for `--web` builds. The
//! routine resets all process-persistent state between requests so the prefork
//! worker can serve request N+1 with the same clean state request N saw: it
//! releases and zeroes function static locals (and their init markers, so their
//! initializers re-run), releases the previous value of refcounted static class
//! properties (their initializers re-run in the handler body and restore the
//! defaults), releases and zeroes ordinary globals plus request superglobals
//! ($_SERVER/$_GET/$_POST) that survive between requests, resets the native and
//! dynamic include-once bookkeeping, the concat-buffer write offset and
//! request-local collector controls, and clears output/print_r capture plus
//! request-scoped stream, wrapper, filter, parser, and exception state, and the
//! fixed runtime's request-scoped flags (`REQUEST_SCOPED_RUNTIME_FLAGS`),
//! before the heap arena is reclaimed.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()`, after every function and the
//!   `--web` handler body are emitted (so every static local has been recorded).
//! - The emitted handler prologue calls the label via `bl/call __rt_web_reset`.
//!
//! Key details:
//! - This is a per-program routine, not a fixed runtime helper: it needs the
//!   module's static set, so it is generated here rather than in `src/codegen/`.
//! - Refcounted release is memory-safety-critical: every release is guarded by
//!   the static's init marker / typed-property sentinel so an uninitialized slot
//!   (all-zero `.comm`) or a sentinel-as-pointer is never released.
//! - Function statics are zeroed (value + marker) so their initializers re-run.
//!   Static properties are NOT zeroed: the handler body re-runs their
//!   initializers after the reset, which rewrites both value and sentinel.
//! - The Magician include registry is reset only when the module links the eval bridge.
//! - The fixed runtime's `.comm` flags are process storage that PHP scopes to a REQUEST.
//!   They are zero at process start, so only a worker serving a second request can observe
//!   one left set; `_headers_sent` is the reason this list exists.

use crate::codegen::abi;
use crate::codegen::data_section::{DataSection, StaticLocalRecord};
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::ir::Module;
use crate::names::{classlike_activation_symbol, ir_global_symbol};
use std::collections::BTreeSet;
use super::runtime_metadata::static_properties_hosting_last;
use crate::superglobals;
use crate::types::PhpType;

mod application;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{AppleVariant, Platform, Target};
    use crate::ir::{Builder, Function, Immediate, IrType, Op, Ownership};

    #[test]
    fn native_include_reset_covers_catalog_cells_without_native_instructions() {
        let target = Target::new(Platform::MacOS, Arch::AArch64);
        let path = std::path::PathBuf::from("/dynamic-only.php");
        let catalog = crate::ir::SourceCatalog::from_units([crate::resolver::SourceUnit {
            canonical_path: path.clone(), mode: crate::source::SourceMode::Php, source: "<?php".into(),
        }]).unwrap();
        let module = Module::with_source_catalog(target, catalog);
        let mut data = DataSection::new();
        let symbol = super::super::source_units::include_guard_symbol(&path);
        data.add_comm(symbol.clone(), 8);
        let mut emitter = Emitter::new(target);
        emit_web_reset(&mut emitter, &module, &data);
        let asm = emitter.output();
        assert!(asm.find(&symbol).unwrap() < asm.find("_heap_off").unwrap());
    }

    /// The request boundary must zero the LIVE-BYTE counter together with the arena it drops.
    ///
    /// Nothing frees the abandoned blocks, so without this store `_gc_live` keeps request 1's
    /// total and every later request adds to it. That was invisible while only `--heap-debug`
    /// read the counter; `memory_get_usage()` reads it too, and the drift showed up as a
    /// `--web` worker answering `memory_get_usage(true) >= memory_get_usage(false)` on request 1
    /// and the opposite on request 2, because the real figure had correctly restarted at zero
    /// while the live figure had not.
    ///
    /// `_gc_peak` is asserted ABSENT on purpose: it is a process high-water mark, which is what
    /// php's `memory_get_peak_usage()` is too, and clearing it per request would silently turn
    /// it into something else.
    #[test]
    fn heap_arena_reset_clears_the_live_byte_counter_but_not_the_peak() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let catalog = crate::ir::SourceCatalog::from_units([crate::resolver::SourceUnit {
                canonical_path: "/fixture/entry.php".into(),
                mode: crate::source::SourceMode::Php,
                source: "<?php".into(),
            }])
            .unwrap();
            let module = Module::with_source_catalog(target, catalog);
            let data = DataSection::new();
            let mut emitter = Emitter::new(target);
            emit_web_reset(&mut emitter, &module, &data);
            let asm = emitter.output();
            let live = asm.find("_gc_live").expect("the boundary must clear _gc_live");
            let off = asm.find("_heap_off").expect("the boundary must clear _heap_off");
            // Both belong to the same arena drop, so they are cleared together.
            assert!(live > off, "_gc_live is cleared with the arena, not before it");
            assert!(!asm.contains("_gc_peak"), "the process peak must survive the boundary");
        }
    }

    #[test]
    fn native_include_guards_reset_without_eval_bridge() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
            Target { apple_variant: AppleVariant::IOS, ..Target::new(Platform::MacOS, Arch::AArch64) },
            Target { apple_variant: AppleVariant::IOSSimulator, ..Target::new(Platform::MacOS, Arch::AArch64) },
        ] {
            let path = std::path::PathBuf::from("/fixture/module.php");
            let symbol = super::super::source_units::include_guard_symbol(&path);
            let catalog = crate::ir::SourceCatalog::from_units([crate::resolver::SourceUnit {
                canonical_path: path.clone(), mode: crate::source::SourceMode::Php, source: "<?php".into(),
            }]).unwrap();
            let id = catalog.id_for_path(&path).unwrap();
            let mut module = Module::with_source_catalog(target, catalog);
            let mut function = Function::new("included".into(), IrType::Void, PhpType::Void);
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.emit(Op::IncludeOnceMark, vec![], Some(Immediate::Source(id)),
                IrType::Void, PhpType::Void, Ownership::NonHeap);
            // A method-only include must participate too, not just the entry body.
            module.class_methods.push(function);
            let mut data = DataSection::new();
            data.add_comm(symbol.clone(), 8);
            data.add_comm("_include_once_unrelated".into(), 8);
            let mut emitter = Emitter::new(target);
            emit_web_reset(&mut emitter, &module, &data);
            let asm = emitter.output();
            assert!(!asm.contains("__elephc_eval_include_request_reset"));
            assert!(!asm.contains("_include_once_unrelated"));
            let guard = asm.find(&symbol).expect("native guard must reset");
            assert!(guard < asm.find("_heap_off").unwrap());
        }
    }

    /// A program that can hold a generator hands its still-tracked fiber stacks back before the
    /// arena drop; one that cannot never references the helper, so it stays dead-stripped.
    #[test]
    fn request_boundary_releases_fiber_stacks_only_with_coroutines() {
        for generator in [false, true] {
            let target = Target::new(Platform::MacOS, Arch::AArch64);
            let catalog = crate::ir::SourceCatalog::from_units([crate::resolver::SourceUnit {
                canonical_path: "/fixture/entry.php".into(),
                mode: crate::source::SourceMode::Php,
                source: "<?php".into(),
            }])
            .unwrap();
            let mut module = Module::with_source_catalog(target, catalog);
            module.required_runtime_features.generator = generator;
            let mut emitter = Emitter::new(target);
            emit_web_reset(&mut emitter, &module, &DataSection::new());
            let asm = emitter.output();
            let release = asm.find("__rt_fiber_release_request_stacks");
            assert_eq!(release.is_some(), generator);
            if let Some(release) = release {
                assert!(release < asm.find("_heap_off").unwrap(), "stacks go back before the arena");
            }
        }
    }

    #[test]
    fn compiled_interface_activation_cells_reset_before_heap() {
        let target = Target::new(Platform::MacOS, Arch::AArch64);
        let catalog = crate::ir::SourceCatalog::from_units([crate::resolver::SourceUnit {
            canonical_path: "/fixture/interface.php".into(),
            mode: crate::source::SourceMode::Php,
            source: "<?php".into(),
        }])
        .unwrap();
        let mut module = Module::with_source_catalog(target, catalog);
        let name = module.data.intern_string("Fixture\\Probe");
        let mut function = Function::new("main".into(), IrType::Void, PhpType::Void);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.emit(
            Op::ClassLikeActivate,
            vec![],
            Some(Immediate::class_like_activation(
                crate::ir::SourceId::from_raw(0),
                crate::span::Span::dummy(),
                crate::parser::ast::ClassLikeKind::Interface,
                name,
            )),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.terminate(crate::ir::Terminator::Return { value: None });
        module.add_function(function);
        let cell = crate::names::classlike_activation_symbol(
            crate::parser::ast::ClassLikeKind::Interface,
            "Fixture\\Probe",
        );
        let mut data = DataSection::new();
        data.add_comm(cell.clone(), 8);
        let mut emitter = Emitter::new(target);
        emit_web_reset(&mut emitter, &module, &data);
        let asm = emitter.output();
        assert!(asm.find(&cell).unwrap() < asm.find("_heap_off").unwrap());
    }
}

/// Minimal frame: just the x29/x30 footer (AArch64) or `push rbp` (x86_64),
/// which keeps the stack 16-byte aligned across the runtime helper calls.
const RESET_FRAME_SIZE: usize = 16;

/// Monotonic per-routine label counter so the reset's internal skip labels never
/// collide with each other across the many static slots it touches.
struct LabelGen {
    next: usize,
}

impl LabelGen {
    /// Creates a fresh label generator starting at zero.
    fn new() -> Self {
        Self { next: 0 }
    }

    /// Returns a unique `__rt_web_reset`-scoped label with the given prefix.
    fn next(&mut self, prefix: &str) -> String {
        let label = format!("__rt_web_reset_{}_{}", prefix, self.next);
        self.next += 1;
        label
    }
}

/// Emits the `__rt_web_reset` routine for the module.
///
/// Always emitted in `--web` builds, even without statics, to reset shared request state.
/// Runs before the handler body's static-property/enum initializers, so it must
/// only RELEASE the previous refcounted property value, not rewrite it.
pub(super) fn emit_web_reset(emitter: &mut Emitter, module: &Module, data: &DataSection) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.raw(".align 2");
    }
    emitter.blank();
    emitter.comment("--- runtime: web per-request state reset ---");
    emitter.label_global("__rt_web_reset");
    abi::emit_frame_prologue(emitter, RESET_FRAME_SIZE);

    // A request cannot legitimately finish inside a boxed array callback. Clear the
    // stack-backed borrow chain before any reset cleanup can run user destructors, so a
    // failed request never leaves them scanning an address in its retired native stack.
    abi::emit_store_zero_to_symbol(emitter, "_rt_unmanaged_ref_borrow_top", 0);
    // A normal or throwing `__set` call unlinks its stack-backed recursion node locally. The web
    // reset is the final backstop for non-Throwable request escapes, before the retired request
    // stack can be reused and before any reset destructor performs another property write.
    abi::emit_store_zero_to_symbol(emitter, "_magic_set_guard_head", 0);
    abi::emit_store_zero_to_symbol(emitter, "_fiber_main_saved_magic_set_guard", 0);
    // The remaining transient owners point into the request heap. Clear them before the static
    // and global cleanup below can run user destructors: a destructor that echoes, throws, or
    // performs an HTTP call must never observe a previous request's buffer or callback state.
    emit_output_buffer_reset(emitter);
    emit_print_r_capture_reset(emitter);
    emit_transient_pointer_reset(emitter);
    let mut labels = LabelGen::new();
    emit_discard_frees_start(emitter);
    if super::context::module_uses_pcntl_signal_handlers(module) {
        abi::emit_call_label(emitter, "__rt_pcntl_release_handlers");
    }
    for record in data.static_locals() {
        emit_static_local_reset(emitter, record, &mut labels);
    }
    let (static_properties, hosting_tables) = static_properties_hosting_last(module);
    for (symbol, php_type) in static_properties {
        emit_static_property_release(emitter, &symbol, &php_type, &mut labels);
    }
    for name in &module.data.global_names {
        let symbol = ir_global_symbol(name);
        if !superglobals::is_superglobal(name)
            && !module.extern_globals.contains_key(name)
            && data.has_comm(&symbol)
        {
            if superglobals::uses_shared_ref_cell(module, name) {
                emit_shared_superglobal_reset(emitter, &symbol, &mut labels);
            } else {
                emit_ordinary_global_reset(emitter, &symbol, &mut labels);
            }
        }
    }
    // `$argc`/`$argv` get Mixed global storage from the eval global-sync hooks even when no
    // program code names them, so they are missing from `global_names`. The eval-to-native hook
    // stores the eval scope's box there; left alone, request N+1's hook released request N's
    // box, which the arena reset had already reclaimed.
    for name in ["argc", "argv"] {
        let symbol = ir_global_symbol(name);
        if !module.data.global_names.iter().any(|global| global == name) && data.has_comm(&symbol) {
            emit_ordinary_global_reset(emitter, &symbol, &mut labels);
        }
    }
    // Every `define()` guards its constant with a BSS "seen" flag. Constants are request state, so
    // the flag has to be clear again when request N+1 runs its own `define()`: with it left set,
    // the second request answered "Constant already defined", kept the first request's value in
    // the program global (Laravel's `LARAVEL_START` timing was one request stale) and never
    // published the constant to the eval bridge.
    for flag in data.comm_labels_with_prefix("_define_seen") {
        let zero = abi::int_result_reg(emitter);
        abi::emit_load_int_immediate(emitter, zero, 0);
        abi::emit_store_reg_to_symbol(emitter, zero, &flag, 0);
    }
    // Request superglobals ($_SERVER/$_GET/$_POST) live in `_eir_global_*` symbol
    // storage and are reassigned by the web prelude every request. Reset them here
    // so stale request arrays are gone before the next prelude builds replacements.
    for name in superglobals::SUPERGLOBALS {
        if superglobals::uses_shared_ref_cell(module, name) {
            emit_shared_superglobal_reset(emitter, &ir_global_symbol(name), &mut labels);
        } else {
            emit_superglobal_reset(emitter, &ir_global_symbol(name), &mut labels);
        }
    }
    // A hosted extension ends the previous request (RSHUTDOWN) once its destructors have run,
    // while the tables and callbacks it may still call back into are alive; the tables go right
    // after, as php-src frees request memory after RSHUTDOWN, in the main epilogue's order.
    if super::block_emit::module_hosts_php_ext(module) {
        let symbol = emitter.target.extern_symbol(super::block_emit::HOSTED_REQUEST_SHUTDOWN);
        abi::emit_call_label(emitter, &symbol);
    }
    for (symbol, php_type) in hosting_tables {
        emit_static_property_release(emitter, &symbol, &php_type, &mut labels);
    }
    // Last owner teardown, as in the main epilogue: a destructor above, or an engine object a
    // hosting table released, may still reach C code that calls a saved callback.
    if super::frame::module_binds_extern_callbacks(module) {
        abi::emit_call_label(emitter, super::frame::EXTERN_CALLBACK_RELEASE_LABEL);
    }

    if module.required_runtime_features.eval_bridge {
        emitter.comment("reset dynamic include_once state for the next request");
        let symbol = emitter
            .target
            .extern_symbol("__elephc_eval_include_request_reset");
        abi::emit_call_label(emitter, &symbol);
    }
    emit_native_include_resets(emitter, module, data);
    emit_deferred_class_load_resets(emitter, module, data);
    emit_native_classlike_activation_resets(emitter, module, data);

    // Clear every lazy enum case slot so request N+1 re-materializes its cases on
    // demand instead of reusing request N's object. This keeps the pre-existing
    // per-request lifecycle: the handler prologue used to re-run the eager enum
    // initializers and overwrite each slot every request, so a case object never
    // spanned two requests. Carrying the pointer over instead would be a new
    // hazard, because the per-request local cleanup can release a case that
    // reached a top-level local.
    super::enum_singletons::emit_enum_slot_resets(emitter, module);

    // Every root of the previous request is released above, so an object that only a
    // reference cycle keeps alive is garbage. PHP destructs it at request shutdown; a
    // collection here runs its destructor (a PDO connection closes) before the arena
    // wipe, instead of leaking its native resources into the next request.
    emitter.comment("collect the previous request's cycles so their destructors run");
    abi::emit_call_label(emitter, "__rt_gc_collect_cycles_explicit");

    emit_request_scoped_flag_resets(emitter);
    emit_define_flag_resets(emitter, module, data);

    emit_concat_offset_reset(emitter);
    emit_core_handler_reset(emitter);
    abi::emit_call_label(emitter, "__rt_resource_inventory_reset");
    abi::emit_call_label(emitter, "__rt_diag_reset");
    emit_gc_state_reset(emitter);

    // A generator still referenced here is dropped with the arena below, never freed, so its
    // fiber stack would leak a mapping per request; hand every stack still out back first.
    if module.required_runtime_features.fiber || module.required_runtime_features.generator {
        abi::emit_call_label(emitter, "__rt_fiber_release_request_stacks");
    }

    // The heap arena reset MUST be the final reset step: the static/global releases
    // above may run destructors or decref shared values, which require the arena to
    // still be valid. Wiping the allocator to pure-bump state last reclaims the whole
    // per-request arena at once. Application workers use this only before their post-fork
    // bootstrap. Their request and shutdown boundaries keep the heap and exception frames
    // valid until user callbacks finish.
    if module.required_runtime_features.mbstring || module.required_runtime_features.eval_bridge {
        abi::emit_call_label(emitter, "__rt_mbstring_release_catalog");
    }
    // Every cleanup above can run PHP: destructors may call `ob_start()`,
    // `stream_context_create()`, `json_decode()`, or reopen a stream. The early clears
    // kept those destructors from observing the previous request's state; these final
    // clears keep the state they create from surviving into the arena reset. Ordering
    // matters: the fd-keyed tables are cleared only after `__rt_resource_inventory_reset`
    // and all later cleanup has finished consuming them.
    emit_output_buffer_reset(emitter);
    emit_print_r_capture_reset(emitter);
    emit_transient_pointer_reset(emitter);
    emit_extension_request_state_reset(emitter);
    emit_cstr_buffer_reset(emitter);
    emit_heap_arena_reset(emitter);
    if module.web_application {
        abi::emit_store_imm_to_symbol(emitter, "_gc_candidates_enabled", 0, 1);
        abi::emit_symbol_address(emitter, abi::int_result_reg(emitter), "__rt_web_candidate_roots");
        abi::emit_store_reg_to_symbol(emitter, abi::int_result_reg(emitter), "_gc_candidate_root_scan_fn", 0);
    }
    abi::emit_call_label(emitter, "__rt_gc_request_start");

    abi::emit_frame_restore(emitter, RESET_FRAME_SIZE);
    abi::emit_return(emitter);
    if module.web_application {
        application::emit(emitter, module, data);
    }
}

/// Discards output buffers abandoned by the previous request.
///
/// `_ob_level` gates every `ob_*` access and every terminal write's capture branch, so clearing
/// it removes the only route to the retired heap buffers. `_ob_in_handler` and `_ob_flushing` are
/// guards that an exception escaping a user output handler can leave set; a stale guard would
/// discard or skip the next request's output. PHP force-flushes buffers at request shutdown; the
/// web epilogue does not, so a request that ends with an open buffer must not leave the stack
/// armed for its successor.
fn emit_output_buffer_reset(emitter: &mut Emitter) {
    emitter.comment("discard output buffers left open by the previous request");
    for symbol in [
        "_ob_level",
        "_ob_in_handler",
        "_ob_flushing",
        "_ob_implicit_flush",
    ] {
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
}

/// Restores the `print_r($value, true)` capture state to its process-start defaults.
///
/// Return-mode rendering sets `_print_r_mode` and accumulates bytes into the static
/// `_print_r_buf` through `_print_r_off`. A render interrupted before `__rt_pr_finish` would
/// otherwise leave the mode set, so the next request's ordinary output would be captured into
/// that stale buffer instead of reaching its response.
fn emit_print_r_capture_reset(emitter: &mut Emitter) {
    emitter.comment("reset any print_r return-mode capture abandoned by the previous request");
    abi::emit_store_zero_to_symbol(emitter, "_print_r_mode", 0);
    abi::emit_store_zero_to_symbol(emitter, "_print_r_off", 0);
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_symbol_address(emitter, scratch, "_print_r_buf");
    abi::emit_store_reg_to_symbol(emitter, scratch, "_print_r_base", 0);
    abi::emit_store_imm_to_symbol(emitter, "_print_r_cap", 0, 65536);
}

/// Clears transient request-owned pointers and counters that are not owned by static locals,
/// globals, superglobals, or the resource inventory.
///
/// These slots are written by a builtin during a request and are normally cleared by the same
/// builtin on its success path. A fatal error or an exception thrown from a callback can abandon
/// that operation, leaving a pointer into the request heap, a parser nesting counter, or an error
/// source pointer behind. Every entry is scalar state; heap owners are reclaimed by the arena
/// reset that follows.
fn emit_transient_pointer_reset(emitter: &mut Emitter) {
    emitter.comment("clear transient request-owned pointers abandoned by the previous request");
    for symbol in [
        "_stream_context_options",
        "_stream_notification_callback",
        "_exc_value",
        "_exc_handler_top",
        "_exc_call_frame_top",
        "_hash_write_guard_top",
        "_json_active_flags",
        "_json_active_depth",
        "_json_indent_depth",
        "_json_validate_idx",
        "_json_validate_ptr",
        "_json_validate_len",
        "_json_decode_assoc",
        "_json_error_source_ptr",
        "_json_error_source_len",
        "_json_error_candidate_ptr",
        "_json_error_location_active",
        "_json_error_line",
        "_json_error_column",
        "_json_error_scan_count",
        "_json_error_scan_bytes",
        "_json_last_error",
        "_ser_value_counter",
        "_ser_legacy_active",
        "_unser_legacy_active",
        "_ser_obj_count",
        "_unser_count",
        "_unser_active",
        "_http_active_host_ptr",
        "_http_active_proxy_ptr",
    ] {
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
}

/// Clears the request-scoped stream, wrapper, and filter tables after the resource inventory has
/// closed the previous request's handles.
///
/// Unlike the scalar slots above, these tables are fd-keyed or registration-keyed and are not
/// reset by any single builtin's success path. They hold heap pointers (context option hashes,
/// callback descriptors, wrapper and filter class strings, wrapper instances, glob:// state) and
/// native per-fd handles. Clearing them prevents a reused descriptor or a leaked registration
/// from resolving to storage the arena reset is about to reclaim. Native handles owned by the
/// inventory are closed by `__rt_resource_inventory_reset` before this runs; tables the reset did
/// not consume are cleared here rather than left dangling.
fn emit_extension_request_state_reset(emitter: &mut Emitter) {
    emitter.comment("clear request-scoped stream tables before the heap arena is reclaimed");
    for (symbol, byte_len) in [
        ("_user_wrappers", 2_048),
        ("_user_wrapper_handles", 2_048),
        ("_user_filter_registry", 4_096),
        ("_user_filter_instances", 4_096),
        ("_dir_handles", 2_048),
        ("_glob_handles", 2_048),
        ("_popen_files", 2_048),
        ("_iconv_handles", 2_048),
        ("_bzstream_handles", 2_048),
        ("_zstream_handles", 2_048),
        ("_stream_read_filters", 256),
        ("_stream_write_filters", 256),
        ("_eof_flags", 256),
        ("_stream_chunk_size", 2_048),
        ("_stream_connect_host", 4_096),
    ] {
        emit_zero_symbol_bytes(emitter, symbol, byte_len);
    }
}

/// Zero-fills one request-state symbol through libc `memset`.
///
/// The tables above are up to 4 KiB each and run on every web request. A byte-at-a-time loop
/// would add thousands of generated instructions and stores per request; one optimized libc call
/// keeps the reset proportional to its actual size. `__rt_web_reset` already owns a 16-byte frame,
/// so the call site keeps the C ABI's stack alignment on every supported target.
fn emit_zero_symbol_bytes(emitter: &mut Emitter, symbol: &str, byte_len: usize) {
    debug_assert!(byte_len > 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x0", symbol);
            abi::emit_load_int_immediate(emitter, "x1", 0);
            abi::emit_load_int_immediate(emitter, "x2", byte_len as i64);
            emitter.bl_c("memset");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rdi", symbol);
            emitter.instruction("xor esi, esi");                                // zero-fill byte for the memset call
            abi::emit_load_int_immediate(emitter, "rdx", byte_len as i64);
            emitter.bl_c("memset");
        }
    }
}

/// Restores the growable C-string buffers to their inline storage before the
/// request heap is reclaimed, so the next request cannot reuse a heap pointer
/// that now belongs to a different allocation.
fn emit_cstr_buffer_reset(emitter: &mut Emitter) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_symbol_address(emitter, scratch, "_cstr_buf");
    abi::emit_store_reg_to_symbol(emitter, scratch, "_cstr_base", 0);
    abi::emit_store_imm_to_symbol(emitter, "_cstr_cap", 0, 4096);
    abi::emit_symbol_address(emitter, scratch, "_cstr_buf2");
    abi::emit_store_reg_to_symbol(emitter, scratch, "_cstr2_base", 0);
    abi::emit_store_imm_to_symbol(emitter, "_cstr2_cap", 0, 4096);
}

/// Clears request-active compiled interface bindings after PHP-visible cleanup.
///
/// Discovery metadata remains immutable, but the cells consulted by existence
/// queries must not leak declaration activation from request N into request N+1.
fn emit_native_classlike_activation_resets(
    emitter: &mut Emitter,
    module: &Module,
    data: &DataSection,
) {
    let mut cells = BTreeSet::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for instruction in &function.instructions {
            let Some(crate::ir::Immediate::ClassLikeActivation(activation)) =
                &instruction.immediate
            else {
                continue;
            };
            let (kind, name) = (&activation.kind, &activation.name);
            if *kind != crate::parser::ast::ClassLikeKind::Interface {
                continue;
            }
            let Some(name) = module.data.strings.get(name.as_raw() as usize) else {
                continue;
            };
            let cell = classlike_activation_symbol(*kind, name);
            if data.has_comm(&cell) {
                cells.insert(cell);
            }
        }
    }
    if cells.is_empty() {
        return;
    }
    emitter.comment("reset request-active compiled interface bindings");
    let zero = abi::temp_int_reg(emitter.target);
    abi::emit_load_int_immediate(emitter, zero, 0);
    for cell in cells {
        abi::emit_store_reg_to_symbol(emitter, zero, &cell, 0);
    }
}

/// Clears emitted once guards after PHP-visible cleanup, including native-only builds.
/// The catalog includes cells accessed only through the native lookup callback too.
fn emit_native_include_resets(emitter: &mut Emitter, module: &Module, data: &DataSection) {
    if let Some(catalog) = module.source_catalog() {
        for (_, source) in catalog.iter() {
            let label = super::source_units::include_guard_symbol(&source.canonical_path);
            if data.has_comm(&label) { abi::emit_store_zero_to_symbol(emitter, &label, 0); }
        }
    }
    // The guards just went down, so the next prime has real work again.
    if data.has_comm(super::source_units::PRIMED) {
        abi::emit_store_zero_to_symbol(emitter, super::source_units::PRIMED, 0);
    }
}

/// Clears the flag of every `define()` so each request starts with those constants undefined.
///
/// php forgets a request's constants when it ends. With the flag left raised, request 2 read
/// `defined('X')` as true only where it was not folded, and every unguarded or folded
/// `define('X', ...)` warned "Constant already defined" -- dozens of warnings per Symfony
/// request from its polyfill bootstraps.
fn emit_define_flag_resets(emitter: &mut Emitter, module: &Module, data: &DataSection) {
    for name in &module.runtime_define_names {
        let label = crate::names::define_seen_symbol(name);
        if data.has_comm(&label) {
            abi::emit_store_zero_to_symbol(emitter, &label, 0);
        }
    }
}

/// Clears the LOAD flag of every class the closed world carries only to answer a probe.
///
/// `class_exists($n, false)` must report the same "not loaded yet" on request 2 as on request 1;
/// a flag a previous request raised would otherwise make the next one skip the branch that the
/// probe guards.
fn emit_deferred_class_load_resets(emitter: &mut Emitter, module: &Module, data: &DataSection) {
    for name in &module.deferred_class_loads {
        let label = super::source_units::deferred_class_symbol(name);
        if data.has_comm(&label) {
            abi::emit_store_zero_to_symbol(emitter, &label, 0);
        }
    }
}

/// Drains both handler stacks and their eval owners while the request heap is still valid.
fn emit_core_handler_reset(emitter: &mut Emitter) {
    let again = "__rt_web_reset_core_handlers";
    emitter.label(again);
    for kind in ["error", "exception"] {
        let next = format!("__rt_web_reset_{kind}_handler");
        emitter.label(&next);
        abi::emit_call_label(emitter, &format!("__rt_core_{kind}_handler_pop"));
        abi::emit_load_symbol_to_reg(
            emitter, abi::int_result_reg(emitter), &format!("_php_{kind}_handler_stack"), 0,
        );
        abi::emit_branch_if_int_result_nonzero(emitter, &next);
    }
    // The final pop can restore a bottom registration. Destructors can also
    // register a handler of either kind, so drain again until all owners are gone.
    for kind in ["error", "exception"] {
        for suffix in ["stack", "value", "callable", "context", "context_release"] {
            abi::emit_load_symbol_to_reg(
                emitter, abi::int_result_reg(emitter), &format!("_php_{kind}_handler_{suffix}"), 0,
            );
            abi::emit_branch_if_int_result_nonzero(emitter, again);
        }
    }
    abi::emit_store_zero_to_symbol(emitter, "_php_error_handler_mask", 0);
    abi::emit_load_int_immediate(
        emitter, abi::int_result_reg(emitter),
        crate::codegen::compile_php_version().error_reporting_mask(),
    );
    abi::emit_store_reg_to_symbol(emitter, abi::int_result_reg(emitter), "_php_error_reporting", 0);
}

/// Restores request-local cycle-collector controls and counters to process-start defaults.
fn emit_gc_state_reset(emitter: &mut Emitter) {
    emitter.comment("reset cycle collector controls and counters for the next request");
    abi::emit_call_label(emitter, "__rt_gc_drop_pins");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
    abi::emit_store_reg_to_symbol(
        emitter,
        abi::int_result_reg(emitter),
        "_gc_enabled",
        0,
    );
    for symbol in [
        "_gc_collecting",
        "_gc_pending_throw",
        "_gc_freeing_unreachable",
        "_gc_release_suppressed",
        "_gc_runs",
        "_gc_collected",
        "_gc_destructor_depth",
    ] {
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
}

/// Resets the PHP heap arena to a pristine bump-only state: `_heap_off = 0`, an empty
/// ordered free list, and empty exact size-class bins. Emitted as the final step of the
/// per-request `__rt_web_reset`, so the whole arena is reclaimed at once after every
/// refcounted per-request value has already been released above. Valid only under
/// `--web` full-reset semantics — nothing in the PHP arena legitimately survives a
/// request; Rust-side state (the PDO persistent-connection pool, bridge result cells)
/// lives outside `_heap_buf` and is unaffected.
/// Fills the reclaimed arena with a pattern under `--heap-debug`, before the bump pointer moves.
///
/// A pointer that outlives the request boundary is not detectable on its own: the next request
/// allocates over the same addresses and the stale value keeps reading plausible bytes, so the
/// failure surfaces tens of requests later as a corrupted container with an intact header. That
/// is how the Twig generator crash presents, and it took a register dump and a whole-table read
/// to even classify. Poisoning what the boundary reclaims turns the first stale READ into the
/// failure, which is a short walk from its holder.
///
/// Off unless `--heap-debug` asked for it: this writes every live byte of the arena.
fn emit_heap_arena_poison(emitter: &mut Emitter) {
    let skip_label = "__rt_web_reset_skip_arena_poison";
    let result_reg = abi::int_result_reg(emitter);
    emitter.comment("--heap-debug: poison the arena this boundary reclaims");
    abi::emit_load_symbol_to_reg(emitter, result_reg, "_heap_debug_enabled", 0);
    abi::emit_branch_if_int_result_zero(emitter, skip_label);
    let length_reg = abi::int_arg_reg_name(emitter.target, 2);
    abi::emit_load_symbol_to_reg(emitter, length_reg, "_heap_off", 0);
    let fill_reg = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_int_immediate(emitter, fill_reg, 0xde);
    let base_reg = abi::int_arg_reg_name(emitter.target, 0);
    abi::emit_symbol_address(emitter, base_reg, "_heap_buf");
    let memset = emitter.target.extern_symbol("memset");
    abi::emit_call_label(emitter, &memset);
    emitter.label(skip_label);
}

/// Unbinds every heap granule from the object handle it carried, before the arena is recycled.
///
/// `_obj_handle_index` holds one u32 handle per 16-byte granule of the arena, and a granule is
/// only unbound when its block is freed. The request boundary frees nothing -- it drops the whole
/// arena -- so without this the next request inherits a table full of last request's bindings.
/// `__rt_object_handle_release` runs for EVERY block freed, object or not, reads the granule's
/// binding and pushes whatever it finds onto the released-handle stack. An ordinary string that
/// lands on a stale granule therefore hands back a handle it never minted, the same handle can
/// come back from several granules, and two live objects end up sharing one. Object identity is
/// what the interpreter keys its per-object maps on, so that is enough to make it read one object
/// where it expects another.
///
/// Only granules below the previous high-water mark can be bound, so this clears `heap_off / 4`
/// bytes -- four per sixteen of arena actually used, a few hundred KB for a Symfony request, not
/// the whole-heap wipe the old note priced it at.
fn emit_object_handle_index_reset(emitter: &mut Emitter) {
    let skip_label = "__rt_web_reset_skip_handle_index";
    emitter.comment("unbind heap granules from the object handles they carried");
    let used_reg = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, used_reg, "_heap_off", 0);
    abi::emit_branch_if_int_result_zero(emitter, skip_label);
    let length_reg = abi::int_arg_reg_name(emitter.target, 2);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter
                .instruction(&format!("lsr {}, {}, #2", length_reg, used_reg));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, {}", length_reg, used_reg));
            emitter.instruction(&format!("shr {}, 2", length_reg));
        }
    }
    let fill_reg = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_int_immediate(emitter, fill_reg, 0);
    let base_reg = abi::int_arg_reg_name(emitter.target, 0);
    abi::emit_symbol_address(emitter, base_reg, "_obj_handle_index");
    let memset = emitter.target.extern_symbol("memset");
    abi::emit_call_label(emitter, &memset);
    emitter.label(skip_label);
}

/// Lets the releases below skip the allocator's free-list work.
///
/// The arena reset that ends this routine reclaims every block at once, so a block the static and
/// global releases free only needs its handle returned and its header marked; walking it into the
/// ordered free list was most of a Symfony request's teardown. `--heap-debug` keeps real frees, so
/// its double-free and live-free checks still see every teardown release.
fn emit_discard_frees_start(emitter: &mut Emitter) {
    let keep_label = "__rt_web_reset_keep_real_frees";
    let reg = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, reg, "_heap_debug_enabled", 0);
    abi::emit_branch_if_int_result_nonzero(emitter, keep_label);
    abi::emit_store_imm_to_symbol(emitter, "_heap_discard_frees", 0, 1);
    emitter.label(keep_label);
}

fn emit_heap_arena_reset(emitter: &mut Emitter) {
    emitter.comment("reset the PHP heap arena to pure-bump allocation for the next request");
    abi::emit_store_zero_to_symbol(emitter, "_heap_discard_frees", 0);
    emit_heap_arena_poison(emitter);
    emit_object_handle_index_reset(emitter);
    crate::codegen_support::runtime::emit_heap_live_bases_reset(emitter);
    abi::emit_store_zero_to_symbol(emitter, "_heap_off", 0);
    // `_gc_live` is the LIVE-BYTE counter, and dropping the arena leaves nothing live. The
    // boundary frees no block individually, so nothing decrements it: without this store the
    // counter keeps request 1's total, request 2 adds its own on top, and it climbs forever.
    //
    // That was invisible while only `--heap-debug` read it. `memory_get_usage()` reads it too,
    // and the drift is visible in one measurement: on a `--web` worker, request 1 answered
    // `memory_get_usage(true) >= memory_get_usage(false)` and request 2 answered the opposite,
    // because the real figure (`_heap_off`) had correctly restarted at zero while the live
    // figure had not. Reporting bytes that no longer exist is exactly the fabricated number
    // these builtins must not produce.
    //
    // `_gc_peak` is deliberately NOT cleared: it is a process high-water mark, which is what
    // php's `memory_get_peak_usage()` is too, and it is only meaningful once `_gc_live` is
    // honest. `_gc_allocs` stays cumulative; `_gc_frees` is set equal to it below so the
    // leak summary's `live_blocks` (their difference) does not count the retired arena.
    // (`_gc_live` is cleared below, after `_gc_frees` is reconciled with `_gc_allocs`.)
    abi::emit_store_zero_to_symbol(emitter, "_heap_free_list", 0);
    // Every size class is emptied too: the blocks parked in them are gone with the arena.
    abi::emit_call_label(emitter, "__rt_heap_bins_clear");
    // Bulk reclamation also retires every block that remained after typed cleanup.
    // Keep cumulative allocation totals, but do not carry their live footprint into a new arena.
    let value = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, value, "_gc_allocs", 0);
    abi::emit_store_reg_to_symbol(emitter, value, "_gc_frees", 0);
    abi::emit_store_zero_to_symbol(emitter, "_gc_live", 0);
    // `memory_get_peak_usage()` is per request in PHP, so its watermark starts over with the arena.
    abi::emit_store_zero_to_symbol(emitter, "_gc_peak", 0);
}

#[cfg(test)]
mod handler_reset_tests {
    use super::*;
    use crate::codegen::platform::{AppleVariant, Platform, Target};

    /// Full arena reclamation accounts for outstanding blocks only after retiring the old heap.
    #[test]
    fn web_arena_reset_reconciles_allocation_counters_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_heap_arena_reset(&mut emitter);
            let asm = emitter.output();
            let heap_reset = asm.rfind("__rt_heap_bins_clear").unwrap();
            let bases = asm.find("_heap_live_bases").unwrap();
            let extent_reset = asm.rfind("_heap_off").unwrap();
            let allocs = asm.find("_gc_allocs").unwrap();
            let frees = asm.find("_gc_frees").unwrap();
            let live = asm.find("_gc_live").unwrap();
            let peak = asm.find("_gc_peak").expect("the live-byte watermark is reset per request");
            assert!(heap_reset < allocs && allocs < frees && frees < live && live < peak, "{name}");
            assert!(bases < extent_reset, "{name}: live bases must retire before the extent resets");
            assert!(!asm.contains("__rt_heap_alloc"), "{name}");
        }
    }

    /// Saved C callbacks are released after the static and global owners, whose destructors may
    /// still reach C code that calls one, and before the heap arena is reset.
    #[test]
    fn web_reset_releases_extern_callbacks_after_global_destructors() {
        let target = Target::new(Platform::MacOS, Arch::AArch64);
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
        emit_web_reset(&mut emitter, &module, &DataSection::new());
        let asm = emitter.output();
        let release = asm.find(crate::codegen::frame::EXTERN_CALLBACK_RELEASE_LABEL).unwrap();
        let global = asm.rfind(&ir_global_symbol("probe")).unwrap();
        let superglobal = asm.rfind(&ir_global_symbol("_SERVER")).unwrap();
        let heap = asm.find("__rt_heap_bins_clear").unwrap();
        assert!(global < release && superglobal < release && release < heap, "{asm}");
    }

    /// Every target releases handlers before inventory cleanup and before resetting the heap.
    #[test]
    fn web_handler_reset_precedes_heap_reset_on_every_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_web_reset(&mut emitter, &Module::new(target), &DataSection::new());
            let asm = emitter.output();
            let borrowed_refs = asm.find("_rt_unmanaged_ref_borrow_top").unwrap();
            let magic_set_guards = asm.find("_magic_set_guard_head").unwrap();
            let saved_magic_set_guards = asm.find("_fiber_main_saved_magic_set_guard").unwrap();
            let output_level = asm.find("_ob_level").unwrap();
            let print_r_mode = asm.find("_print_r_mode").unwrap();
            let print_r_base = asm.find("_print_r_base").unwrap();
            let print_r_cap = asm.find("_print_r_cap").unwrap();
            let stream_context = asm.find("_stream_context_options").unwrap();
            let final_output_level = asm.rfind("_ob_level").unwrap();
            let final_print_r_mode = asm.rfind("_print_r_mode").unwrap();
            let final_print_r_base = asm.rfind("_print_r_base").unwrap();
            let final_print_r_cap = asm.rfind("_print_r_cap").unwrap();
            let final_stream_context = asm.rfind("_stream_context_options").unwrap();
            let inventory = asm.find("__rt_resource_inventory_reset").unwrap();
            let wrapper_table = asm.find("_user_wrappers").unwrap();
            let heap = asm.find("_heap_off").unwrap();
            for symbol in ["__rt_core_error_handler_pop", "__rt_core_exception_handler_pop", "_php_error_reporting"] {
                assert!(asm.find(symbol).unwrap() < inventory, "{target:?}: {symbol}");
            }
            assert!(borrowed_refs < inventory, "{target:?}: stale borrow state must clear first");
            assert!(
                magic_set_guards < inventory,
                "{target:?}: stale magic-set guard state must clear first"
            );
            assert!(
                saved_magic_set_guards < inventory,
                "{target:?}: saved main magic-set guard state must clear first"
            );
            for (label, position) in [
                ("_ob_level", output_level),
                ("_print_r_mode", print_r_mode),
                ("_print_r_base", print_r_base),
                ("_print_r_cap", print_r_cap),
                ("_stream_context_options", stream_context),
            ] {
                assert!(
                    position < inventory,
                    "{target:?}: {label} must clear before cleanup can run destructors"
                );
            }
            assert!(
                inventory < wrapper_table && wrapper_table < heap,
                "{target:?}: fd-keyed tables must clear after inventory cleanup and before the heap reset"
            );
            for (label, position) in [
                ("_ob_level", final_output_level),
                ("_print_r_mode", final_print_r_mode),
                ("_print_r_base", final_print_r_base),
                ("_print_r_cap", final_print_r_cap),
                ("_stream_context_options", final_stream_context),
            ] {
                assert!(
                    inventory < position && position < heap,
                    "{target:?}: {label} must clear again after cleanup and before the heap reset"
                );
            }
            assert!(
                asm.matches("memset").count() >= 15,
                "{target:?}: request-scoped tables must be cleared with memset"
            );
            assert!(inventory < heap, "{target:?}: handlers need the live request heap");
        }
    }
}

/// Clears the runtime flags whose PHP-visible lifetime is one REQUEST, not one process.
///
/// `.comm` storage starts at zero, so a CLI build never notices that nothing puts it back; a
/// prefork worker does. `_headers_sent` is the one that showed: `__rt_stdout_write` raises it the
/// first time bytes escape the buffering stack, so from request 2 onward `headers_sent()` answered
/// true before the handler had emitted anything, Symfony's `Response::sendHeaders()` returned
/// early, and every response after the first lost its `Content-Type` and `Cache-Control` while
/// still carrying a byte-identical body -- a wrong response that no status code reports.
///
/// The rest are cleared for the same reason. A request that ends through a fatal or an uncaught
/// throw leaves its depth counters, its capture-mode flags and its exception chain set, and the
/// chain in particular points into the arena `emit_heap_arena_reset` is about to wipe.
fn emit_request_scoped_flag_resets(emitter: &mut Emitter) {
    emitter.comment("reset the runtime flags whose lifetime is one request");
    for symbol in REQUEST_SCOPED_RUNTIME_FLAGS {
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
}

/// The request-scoped runtime flags `__rt_web_reset` clears, grouped by owning subsystem.
///
/// Every entry is 8-byte `.comm` storage declared unconditionally by the fixed runtime data, so
/// the symbol always resolves and zero is always its process-start value. Tables whose slots hold
/// OS resources (stream, directory and process handles) are deliberately absent: those have to be
/// CLOSED at the request boundary, not blanked, which is a separate change.
///
/// `_obj_handle_next` is absent for a sharper reason, and putting it back would be a bug.
/// `_obj_handle_index` binds a handle to a heap GRANULE and is not cleared by the arena wipe, so
/// a granule that held an object last request still reads back that object's handle. Today the
/// cursor only climbs, so such a stale handle is always below it and re-minting it is harmless.
/// Restart the cursor at 1 and it stops being harmless: the next block to land on that granule
/// pushes the stale handle onto the free stack when it is freed, and a handle this request has
/// already handed to a live object gets handed out a second time. Restarting the numbering means
/// clearing `_obj_handle_index` too -- one `u32` per 16 heap bytes, so 128MB of stores for a
/// 512MB heap, every request -- which is why `spl_object_id()` still climbs across requests here.
const REQUEST_SCOPED_RUNTIME_FLAGS: &[&str] = &[
    // Raised by `__rt_stdout_write` once bytes reach the response sink.
    "_headers_sent",
    // Output buffering: stack depth, both re-entry guards, and the implicit-flush setting.
    "_ob_level",
    "_ob_in_handler",
    "_ob_flushing",
    "_ob_implicit_flush",
    // `print_r($value, true)` capture mode and its accumulated write offset.
    "_print_r_mode",
    "_print_r_off",
    // var_dump indentation and its recursion guard depth.
    "_vd_indent",
    "_vd_seen_n",
    // The handler chain and the pending throw, both of which point into the request arena.
    "_exc_handler_top",
    "_exc_call_frame_top",
    "_exc_value",
    // Fiber scheduling state and the saved main-fiber context.
    "_fiber_current",
    "_fiber_main_saved_sp",
    "_fiber_main_saved_exc",
    "_fiber_main_saved_call_frame",
    // A `new`'s reserved object handle; a throw between its arguments and its allocation
    // can leave it set.
    "_obj_handle_pending",
    // The configured default timezone. Zero makes the next date operation apply php's default
    // (`TZ=UTC`) again; left set, a request's `date_default_timezone_set('Europe/Paris')`
    // carried into every later request, where php starts each one from the ini default.
    "_php_default_tz_len",
    // serialize()/unserialize() back-reference counters and policy fields.
    "_ser_value_counter",
    "_ser_obj_count",
    "_unser_count",
    "_unser_depth",
    "_unser_active",
    "_unser_context",
    "_unser_allowed_mode",
    // `json_last_error()` plus the encoder/decoder state its message is rendered from. Every
    // json call site rewrites the active fields before use, so zero is a safe boundary value.
    "_json_last_error",
    "_json_active_flags",
    "_json_active_depth",
    "_json_indent_depth",
    "_json_depth_limit",
    "_json_validate_idx",
    "_json_validate_ptr",
    "_json_validate_len",
    "_json_decode_assoc",
    "_json_error_source_ptr",
    "_json_error_location_active",
    "_json_error_line",
    "_json_error_column",
    // The `@` suppression depth, which a fatal inside a suppressed call leaves raised.
    "_rt_diag_suppression",
    // The two retained dynamic C-string scratch buffers. Each holds a raw pointer into the arena
    // (`__rt_cstr`/`__rt_cstr2` keep the buffer of the last long conversion so the next one can
    // free it), and the arena wipe below makes that pointer dangle: the next request's first long
    // conversion -- a regex pattern over 4 KiB -- freed whatever LIVE block began at that address
    // in the new request. Laravel's `mb_ucfirst()` title-case pattern is 6 KiB, which is how the
    // free surfaced as an intermittent "heap debug detected double free" worker crash.
    "_cstr_buf_dynamic",
    "_cstr_buf2_dynamic",
    // The error-handler dispatch guard, which a fatal inside a `set_error_handler()` callback
    // leaves raised; a stuck guard sends every later request's diagnostics past its handler.
    "_rt_diag_dispatching",
    // The cycle collector's re-entry guard, safepoint counter and release suppression.
    "_gc_collecting",
    "_gc_safepoint_count",
    "_gc_release_suppressed",
];

/// Resets one function static local: skips uninitialized slots, releases any
/// owned refcounted value, then zeroes the 16-byte value and the init marker so
/// the static's initializer re-runs on the next request.
fn emit_static_local_reset(emitter: &mut Emitter, record: &StaticLocalRecord, labels: &mut LabelGen) {
    let ty = record.php_type.codegen_repr();
    let skip_label = labels.next("skip_static");
    emitter.comment(&format!("reset static local {}", record.symbol));
    // Guard on the init marker: a zero marker means the initializer never ran
    // this process, so the value slot is still all-zero `.comm` storage and there
    // is nothing to release or zero. This also keeps us from releasing garbage.
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), &record.init_symbol, 0);
    abi::emit_branch_if_int_result_zero(emitter, &skip_label);

    emit_release_symbol_value(emitter, &record.symbol, &ty);
    // Zero the 16-byte value slot and the init marker so the initializer re-runs.
    abi::emit_store_zero_to_symbol(emitter, &record.symbol, 0);
    abi::emit_store_zero_to_symbol(emitter, &record.symbol, 8);
    abi::emit_store_zero_to_symbol(emitter, &record.init_symbol, 0);

    emitter.label(&skip_label);
}

/// Releases the previous value of one refcounted static class property AND zeroes the slot.
///
/// Leaving the pointer behind looks safe -- the handler body's initializer overwrites it next
/// request -- and it is not. That store releases whatever it finds in the slot first, and the
/// arena is deterministic: the next request replays the same allocation sequence and the
/// initializer gets back the SAME address this request used. The store then releases the block
/// it is about to write, taking a brand-new value's refcount from 1 to 0, and the property is
/// left pointing at freed storage for the rest of the request.
///
/// `Request::$trustedProxies = []` in the Symfony `--web` build is exactly that: allocated at
/// arena offset 117104, read a moment later by `isFromTrustedProxy()` with `refcount=0, kind=0`
/// -- the footprint `__rt_heap_free` leaves. Zero releases as a no-op, which is the same reason
/// `emit_static_property_sentinel` clears the value word for properties without a default.
fn emit_static_property_release(
    emitter: &mut Emitter,
    symbol: &str,
    php_type: &PhpType,
    labels: &mut LabelGen,
) {
    let ty = php_type.codegen_repr();
    let skip_label = labels.next("skip_prop");
    emitter.comment(&format!("release previous static property value {}", symbol));
    // Typed static properties carry an uninitialized sentinel in the high word
    // until first written. If it is still the sentinel, the value word holds no
    // owned heap pointer, so skip the release entirely.
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), symbol, 8);
    emit_branch_if_equals_sentinel(emitter, &skip_label);
    emit_release_symbol_value(emitter, symbol, &ty);
    abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    emitter.label(&skip_label);
}

/// Releases and zeroes one referenced request superglobal's ref-cell owner.
///
/// The `_eir_global_*` symbol stores a shared cell pointer rather than the hash directly, so a
/// returned array containing `&$_SESSION` can retain the cell safely. Marker owners may keep the
/// payload alive until their containing arrays are released; this routine drops only the global
/// slot's share and then clears the symbol before the next prelude assignment.
fn emit_shared_superglobal_reset(emitter: &mut Emitter, symbol: &str, labels: &mut LabelGen) {
    let skip_label = labels.next("skip_superglobal");
    emitter.comment(&format!("reset request superglobal {}", symbol));
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), symbol, 0);
    abi::emit_branch_if_int_result_zero(emitter, &skip_label);
    abi::emit_call_label(emitter, "__rt_global_ref_cell_decref");
    abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    emitter.label(&skip_label);
}

/// Releases and zeroes one ordinary request superglobal stored as a direct hash pointer.
fn emit_superglobal_reset(emitter: &mut Emitter, symbol: &str, labels: &mut LabelGen) {
    let ty = superglobals::superglobal_type().codegen_repr();
    let skip_label = labels.next("skip_superglobal");
    emitter.comment(&format!("reset request superglobal {}", symbol));
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), symbol, 0);
    abi::emit_branch_if_int_result_zero(emitter, &skip_label);
    emit_release_symbol_value(emitter, symbol, &ty);
    abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    emitter.label(&skip_label);
}

/// Releases and zeroes one ordinary PHP global, whose storage is a boxed Mixed cell.
fn emit_ordinary_global_reset(emitter: &mut Emitter, symbol: &str, labels: &mut LabelGen) {
    let ty = PhpType::Mixed;
    let skip_label = labels.next("skip_global");
    emitter.comment(&format!("reset ordinary global {}", symbol));
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), symbol, 0);
    abi::emit_branch_if_int_result_zero(emitter, &skip_label);
    emit_release_symbol_value(emitter, symbol, &ty);
    abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    emitter.label(&skip_label);
}

/// Releases the owned refcounted value currently stored at `symbol` (offset 0).
///
/// Mirrors the function-epilogue cleanup shapes: strings free their payload via
/// the validating heap-free helper, callables release their descriptor, and
/// other refcounted kinds decref through the type-specific helper. Non-refcounted
/// types (int/bool/float/tagged scalar) own no heap value and are a no-op here.
fn emit_release_symbol_value(emitter: &mut Emitter, symbol: &str, ty: &PhpType) {
    match ty {
        PhpType::Str => {
            // Load the string pointer into the result register and free it. The
            // validating free safely ignores null and non-heap pointers.
            abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), symbol, 0);
            abi::emit_call_label(emitter, "__rt_heap_free_safe");
        }
        PhpType::Callable => {
            abi::emit_load_symbol_to_result(emitter, symbol, ty);
            abi::emit_decref_if_refcounted(emitter, ty);
        }
        other if other.is_refcounted() => {
            abi::emit_load_symbol_to_result(emitter, symbol, other);
            abi::emit_decref_if_refcounted(emitter, other);
        }
        _ => {}
    }
}

/// Branches to `label` when the integer result register equals the uninitialized
/// typed-property sentinel, so an unwritten typed property is skipped.
fn emit_branch_if_equals_sentinel(emitter: &mut Emitter, label: &str) {
    let scratch = abi::temp_int_reg(emitter.target);
    abi::emit_load_int_immediate(emitter, scratch, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(                                                // compare the property marker with the uninitialized sentinel
                &format!("cmp {}, {}", abi::int_result_reg(emitter), scratch)
            );
            emitter.instruction(&format!("b.eq {}", label));                    // skip the release when the property was never written
        }
        Arch::X86_64 => {
            emitter.instruction(                                                // compare the property marker with the uninitialized sentinel
                &format!("cmp {}, {}", abi::int_result_reg(emitter), scratch)
            );
            emitter.instruction(&format!("je {}", label));                      // skip the release when the property was never written
        }
    }
}

/// Resets the concat-buffer write offset to its process-start base (zero), so the
/// 64KB `_concat_buf` does not exhaust across many requests. `_concat_off` is
/// `.comm`-zero-initialized and nothing sets it nonzero at startup, so zero is the
/// correct base; the handler then captures this fresh base for its frame.
fn emit_concat_offset_reset(emitter: &mut Emitter) {
    emitter.comment("reset the concat-buffer write offset for the next request");
    abi::emit_store_zero_to_symbol(emitter, "_concat_off", 0);
}
