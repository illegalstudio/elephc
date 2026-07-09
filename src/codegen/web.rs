//! Purpose:
//! Generates the per-program `__rt_web_reset` routine for `--web` builds. The
//! routine resets all process-persistent state between requests so the prefork
//! worker can serve request N+1 with the same clean state request N saw: it
//! releases and zeroes function static locals (and their init markers, so their
//! initializers re-run), releases the previous value of refcounted static class
//! properties (their initializers re-run in the handler body and restore the
//! defaults), releases and zeroes ordinary globals plus request superglobals
//! ($_SERVER/$_GET/$_POST) that survive between requests, and resets the
//! concat-buffer write offset.
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

use crate::codegen::abi;
use crate::codegen::data_section::{DataSection, StaticLocalRecord};
use crate::codegen::emit::Emitter;
use crate::codegen::platform::{Arch, Platform};
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::ir::Module;
use crate::names::{ir_global_symbol, static_property_symbol};
use crate::superglobals;
use crate::types::PhpType;

/// Minimal frame: just the x29/x30 footer (AArch64) or `push rbp` (x86_64),
/// which keeps the stack 16-byte aligned across the runtime helper calls.
const RESET_FRAME_SIZE: usize = 16;

/// Monotonic per-routine label counter so the reset's internal skip labels never
/// collide with each other across the many static slots it touches. `base`
/// disambiguates labels between routines emitted in the same module: a classic
/// `--web` build now emits both `emit_web_reset` and
/// `emit_web_worker_request_reset`, and each must own a disjoint label namespace
/// so their internal skip labels (e.g. `skip_superglobal_0`) never collide.
struct LabelGen {
    base: &'static str,
    next: usize,
}

impl LabelGen {
    /// Creates a fresh label generator with the given base namespace, starting at zero.
    fn new(base: &'static str) -> Self {
        Self { base, next: 0 }
    }

    /// Returns a unique label scoped to this generator's `base` namespace with the
    /// given prefix, so labels from different reset routines never collide.
    fn next(&mut self, prefix: &str) -> String {
        let label = format!("{}_{}_{}", self.base, prefix, self.next);
        self.next += 1;
        label
    }
}

/// Declares the `.comm` storage for every request superglobal so both reset
/// routines can reference each `_eir_global_*` symbol even when the program (or
/// the usage-gated prelude, B1) never loads or stores that superglobal.
///
/// The reset routines iterate `superglobals::SUPERGLOBALS` unconditionally and
/// emit a load of each symbol (guarded so a null slot is skipped). Without a
/// storage declaration for an unreferenced superglobal, that load is a dangling
/// reference and the link fails. `add_comm` is idempotent per symbol (deduped),
/// so this is a no-op for superglobals the body already stored. All slots are
/// pointer-sized (`AssocArray` codegen repr), matching the store path's sizing.
pub(super) fn declare_superglobal_storage(data: &mut DataSection) {
    let size = superglobals::superglobal_type().codegen_repr().stack_size().max(8);
    for name in superglobals::SUPERGLOBALS {
        data.add_comm(ir_global_symbol(name), size);
    }
}

/// Emits the `__rt_web_reset` routine for the module.
///
/// Always emitted in `--web` builds (even with zero statics) so the handler's
/// `bl/call __rt_web_reset` resolves; in that case it only resets `_concat_off`.
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

    let mut labels = LabelGen::new("__rt_web_reset");
    for record in data.static_locals() {
        emit_static_local_reset(emitter, record, &mut labels);
    }
    for (symbol, php_type) in refcounted_static_properties(module) {
        emit_static_property_release(emitter, &symbol, &php_type, &mut labels);
    }
    for name in &module.data.global_names {
        if !superglobals::is_superglobal(name) && !module.extern_globals.contains_key(name) {
            emit_ordinary_global_reset(emitter, &ir_global_symbol(name), &mut labels);
        }
    }
    // Request superglobals ($_SERVER/$_GET/$_POST) live in `_eir_global_*` symbol
    // storage and are reassigned by the web prelude every request. Reset them here
    // so stale request arrays are gone before the next prelude builds replacements.
    for name in superglobals::SUPERGLOBALS {
        emit_superglobal_reset(emitter, &ir_global_symbol(name), &mut labels);
    }
    // User `global` variables live in the same `_eir_global_*` storage but are
    // NOT reassigned by the prelude, so `StoreGlobalReleasing` only releases the
    // previous value when the re-running body writes them again. Reset them here
    // so classic `--web` keeps PHP-FPM per-request isolation instead of leaking a
    // `global` written only inside a function or conditionally. Worker/script
    // modes never emit this routine, so their globals persist.
    for record in data.user_globals() {
        emit_user_global_reset(emitter, &record.symbol, &record.php_type, &mut labels);
    }

    emit_concat_offset_reset(emitter);
    emit_exception_state_reset(emitter);

    abi::emit_frame_restore(emitter, RESET_FRAME_SIZE);
    abi::emit_return(emitter);
}

/// Emits the `__rt_web_worker_request_reset` routine for `--web-worker` builds.
///
/// The worker reset is the request-scoped counterpart of `emit_web_reset`: it
/// only releases and zeroes the request superglobals and resets the concat
/// offset, so each request sees clean per-request state. Function static locals
/// and static class properties are intentionally NOT reset — they persist for
/// the worker's lifetime (the boot runs once and the handler closure captures
/// long-lived state), so releasing them per request would destroy state the
/// next request still needs.
///
/// `env_persistent` (true only for the trampoline `--web-worker` mode) makes the
/// routine skip `$_ENV`: in that mode `$_ENV` is built once at boot and must
/// survive every request, so releasing/zeroing it here would destroy it. In
/// `--web-worker=script` and classic `--web` (which also emits this routine only
/// so the shared Rust worker-loop symbol links) `env_persistent` is false and
/// `$_ENV` is reset like every other request superglobal, because those modes
/// re-fill it per request and would otherwise leak the previous request's hash.
///
/// Called from:
/// - `crate::codegen_ir::block_emit::emit_module()`, after every function is
///   emitted, when `web_worker` is true.
/// - The trampoline `elephc_worker_handle_request` calls this label via
///   `bl/call __rt_web_worker_request_reset`.
pub(super) fn emit_web_worker_request_reset(
    emitter: &mut Emitter,
    _module: &Module,
    _data: &DataSection,
    env_persistent: bool,
) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.raw(".align 2");
    }
    emitter.blank();
    emitter.comment("--- runtime: web worker per-request state reset ---");
    emitter.label_global("__rt_web_worker_request_reset");
    abi::emit_frame_prologue(emitter, RESET_FRAME_SIZE);

    let mut labels = LabelGen::new("__rt_web_worker_reset");
    // Only the request superglobals and concat offset are reset per request.
    // Function static locals and static class properties persist for the
    // worker lifetime and are intentionally left untouched here. `$_ENV` is
    // skipped when it is a persistent boot-time snapshot (trampoline mode).
    for name in superglobals::SUPERGLOBALS {
        if env_persistent && *name == "_ENV" {
            continue;
        }
        emit_superglobal_reset(emitter, &ir_global_symbol(name), &mut labels);
    }
    emit_concat_offset_reset(emitter);
    emit_exception_state_reset(emitter);

    abi::emit_frame_restore(emitter, RESET_FRAME_SIZE);
    abi::emit_return(emitter);

    // -- macOS C-ABI alias: ___rt_web_worker_request_reset -> __rt_web_worker_request_reset --
    // The Rust bridge (worker_mode.rs) calls this routine via `extern "C"`, which
    // on Mach-O resolves to `___rt_web_worker_request_reset` (Rust prepends the
    // leading `_` that the C ABI mandates). The body above is labeled literally
    // `__rt_web_worker_request_reset` (assembly `bl` calls use that exact name),
    // so emit a one-instruction tail-call stub under the Mach-O name that Rust
    // expects. Linux defines no leading underscore, so no alias is needed there.
    if emitter.platform == Platform::MacOS {
        emitter.blank();
        emitter.raw(".align 2");
        emitter.comment("-- macOS C-ABI alias: ___rt_web_worker_request_reset -> __rt_web_worker_request_reset --");
        emitter.label_global("___rt_web_worker_request_reset");
        emitter.instruction("b __rt_web_worker_request_reset");                 // tail-call the real reset routine
    }
}

/// Resets one function static local: skips uninitialized slots, releases any
/// owned refcounted value, then zeroes the 16-byte value and the init marker so
/// the static's initializer re-runs on the next request.
fn emit_static_local_reset(emitter: &mut Emitter, record: &StaticLocalRecord, labels: &mut LabelGen) {
    let ty = record.php_type.codegen_repr();
    let skip_label = labels.next("skip_static");
    emitter.comment(&format!("reset static local {}", record.symbol));
    // Guard on the init marker and release the owned value. The zeroing below
    // then clears the slot so the initializer re-runs on the next request.
    emit_static_local_release_only(emitter, record, &ty, &skip_label);
    // Zero the 16-byte value slot and the init marker so the initializer re-runs.
    abi::emit_store_zero_to_symbol(emitter, &record.symbol, 0);
    abi::emit_store_zero_to_symbol(emitter, &record.symbol, 8);
    abi::emit_store_zero_to_symbol(emitter, &record.init_symbol, 0);

    emitter.label(&skip_label);
}

/// Releases one function static local's owned refcounted value without zeroing
/// the slot. Guarded on the init marker: a zero marker means the initializer
/// never ran this process, so the value slot is still all-zero `.comm` storage
/// and there is nothing to release. The caller emits `skip_label` after any
/// follow-up work (zeroing for the web reset, nothing for the CLI exit release).
///
/// Shared by `emit_static_local_reset` (web per-request reset, followed by
/// zeroing) and `emit_function_static_locals_release_at_exit` (CLI program-exit
/// release, no zeroing) so the per-type release switch lives in exactly one
/// place via `emit_release_symbol_value`.
fn emit_static_local_release_only(
    emitter: &mut Emitter,
    record: &StaticLocalRecord,
    ty: &PhpType,
    skip_label: &str,
) {
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), &record.init_symbol, 0);
    abi::emit_branch_if_int_result_zero(emitter, skip_label);
    emit_release_symbol_value(emitter, &record.symbol, ty);
}

/// Releases every recorded function static local's owned refcounted value at CLI
/// program exit, so the final static value (which persists across calls and is
/// never freed by any function epilogue) does not leak at process exit.
///
/// Release-only: slots are NOT zeroed (the process is dying; no initializer will
/// re-run). Iterates `data.static_locals()` — the same registry the `--web`
/// reset uses — and reuses `emit_static_local_release_only` so the per-type
/// release shapes stay identical to the web path. Static class properties,
/// superglobals, and `_concat_off` are intentionally NOT touched here: those are
/// web/request-scoped and out of scope for CLI exit.
///
/// Called from:
/// - `crate::codegen_ir::frame::emit_main_epilogue`, after owned-local cleanup
///   and before gc_stats/heap_debug/exit so the freed statics are counted in the
///   allocator summary. The CLI epilogue is only emitted for non-`--web` builds
///   (the `--web` path uses `emit_web_handler_epilogue`), so this cleanup is
///   naturally gated to CLI builds and never runs in web mode.
pub(super) fn emit_function_static_locals_release_at_exit(emitter: &mut Emitter, data: &DataSection) {
    let statics = data.static_locals();
    if statics.is_empty() {
        return;
    }
    emitter.blank();
    emitter.comment("--- release function-static locals at exit ---");
    let mut labels = LabelGen::new("__rt_exit_static");
    for record in statics {
        let ty = record.php_type.codegen_repr();
        let skip_label = labels.next("skip_static_exit");
        emitter.comment(&format!("release static local {} at exit", record.symbol));
        emit_static_local_release_only(emitter, record, &ty, &skip_label);
        emitter.label(&skip_label);
    }
}

/// Releases the previous value of one refcounted static class property without
/// zeroing it: the handler body's re-run initializer overwrites both the value
/// and the typed-property sentinel after this reset, so only the old owner needs
/// releasing to avoid a per-request leak. Skips the uninitialized sentinel so a
/// sentinel is never released as if it were a heap pointer.
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
    emitter.label(&skip_label);
}

/// Releases and zeroes one request superglobal ($_SERVER/$_GET/$_POST), whose
/// assoc-array hash lives in `_eir_global_*` storage. Guarded against a null
/// symbol (the very first request, before the prelude's first assignment) so a
/// null is never released. Zeroing is safe because the prelude reassigns the
/// symbol right after the reset, and `StoreGlobal` does not read the old value.
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

/// Releases (if refcounted) and zero-initializes one user `global` variable in
/// `_eir_global_*` storage, so classic `--web` gives each request a fresh,
/// PHP-FPM-isolated global.
///
/// Refcounted globals (string/array/hash/object/mixed/callable) release their
/// heap payload before zeroing; scalar globals (int/bool/float/tagged scalar)
/// are just zeroed — a stale scalar would otherwise leak into the next request.
/// Two-word storage (`Str`, `TaggedScalar`) also zeroes the second word. The
/// refcounted path guards against a null pointer so an unwritten global is never
/// released. Safe because the reset runs before the re-running body reassigns the
/// global, and no live frame aliases the persistent slot at the request boundary
/// (the previous request's handler has already returned). Every primitive is an
/// `abi` helper that dispatches on `emitter.target.arch`, so the routine is
/// target-aware for both ARM64 and x86_64.
fn emit_user_global_reset(
    emitter: &mut Emitter,
    symbol: &str,
    php_type: &PhpType,
    labels: &mut LabelGen,
) {
    let repr = php_type.codegen_repr();
    let size = repr.stack_size().max(8);
    let refcounted = matches!(repr, PhpType::Str | PhpType::Callable) || repr.is_refcounted();
    emitter.comment(&format!("reset user global {}", symbol));
    if refcounted {
        let skip_label = labels.next("skip_global");
        abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), symbol, 0);
        abi::emit_branch_if_int_result_zero(emitter, &skip_label);
        emit_release_symbol_value(emitter, symbol, &repr);
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
        if size > 8 {
            abi::emit_store_zero_to_symbol(emitter, symbol, 8);
        }
        emitter.label(&skip_label);
    } else {
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
        if size > 8 {
            abi::emit_store_zero_to_symbol(emitter, symbol, 8);
        }
    }
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
            emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(emitter), scratch)); // compare the property marker against the uninitialized sentinel
            emitter.instruction(&format!("b.eq {}", label));                    // skip the release when the property was never written
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(emitter), scratch)); // compare the property marker against the uninitialized sentinel
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

/// Zeroes the exception-unwinder state (`_exc_handler_top`, the activation-record
/// cleanup chain `_exc_call_frame_top`, and the `@`-suppression depth
/// `_rt_diag_suppression`) at the start of each request, so the request body always
/// begins with a clean handler chain and empty cleanup chain regardless of how the
/// previous request ended.
///
/// All three are 0 at handler entry on every well-behaved path — balanced try
/// push/pop restores `_exc_handler_top`, each function pops its own activation record
/// so `_exc_call_frame_top` returns to its baseline, and the `exit()`/`die()` bailout
/// landing zeroes the handler state explicitly — so this is defense-in-depth: it
/// turns "clean exception state at body start" into an enforced invariant rather than
/// one that relies on every termination path restoring it. A stale non-zero value
/// would otherwise silently corrupt the next request (a dangling handler pointer
/// dereferenced by a throw, a stale cleanup record whose stack frame is gone, or
/// mis-suppressed warnings).
fn emit_exception_state_reset(emitter: &mut Emitter) {
    emitter.comment("reset the exception-unwinder state for the next request");
    abi::emit_store_zero_to_symbol(emitter, "_exc_handler_top", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_call_frame_top", 0);
    abi::emit_store_zero_to_symbol(emitter, "_rt_diag_suppression", 0);
}

/// Returns `(storage_symbol, php_type)` for every refcounted static class
/// property that the handler body initializes, enumerated exactly like
/// `emit_static_property_initializers` so the reset stays in lockstep with what
/// gets re-initialized each request. Non-refcounted properties are excluded:
/// their re-run initializer simply overwrites the scalar, with nothing to free.
fn refcounted_static_properties(module: &Module) -> Vec<(String, PhpType)> {
    let mut class_names = super::runtime_referenced_class_names(module)
        .into_iter()
        .collect::<Vec<_>>();
    class_names.sort();
    let mut props = Vec::new();
    for class_name in class_names {
        let Some(class_info) = module.class_infos.get(&class_name) else {
            continue;
        };
        for (property, php_type) in &class_info.static_properties {
            let declaring_class = class_info
                .static_property_declaring_classes
                .get(property)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            if declaring_class != class_name {
                continue;
            }
            let ty = php_type.codegen_repr();
            if !(matches!(ty, PhpType::Str | PhpType::Callable) || ty.is_refcounted()) {
                continue;
            }
            props.push((static_property_symbol(&class_name, property), php_type.clone()));
        }
    }
    props
}
