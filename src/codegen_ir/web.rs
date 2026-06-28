//! Purpose:
//! Generates the per-program `__rt_web_reset` routine for `--web` builds. The
//! routine resets all process-persistent state between requests so the prefork
//! worker can serve request N+1 with the same clean state request N saw: it
//! releases and zeroes function static locals (and their init markers, so their
//! initializers re-run), releases the previous value of refcounted static class
//! properties (their initializers re-run in the handler body and restore the
//! defaults), releases and zeroes the request superglobals ($_SERVER/$_GET/
//! $_POST) that the web prelude reassigns each request, and resets the
//! concat-buffer write offset.
//!
//! Called from:
//! - `crate::codegen_ir::block_emit::emit_module()`, after every function and the
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
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::ir::Module;
use crate::names::{ir_global_symbol, static_property_symbol};
use crate::superglobals;
use crate::types::PhpType;

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

    let mut labels = LabelGen::new();
    for record in data.static_locals() {
        emit_static_local_reset(emitter, record, &mut labels);
    }
    for (symbol, php_type) in refcounted_static_properties(module) {
        emit_static_property_release(emitter, &symbol, &php_type, &mut labels);
    }
    // Request superglobals ($_SERVER/$_GET/$_POST) live in `_eir_global_*` symbol
    // storage and are reassigned (`$_SERVER = []`) by the web prelude every
    // request. `StoreGlobal` does not release the previous value, so without this
    // each request would leak the prior request's assoc-array hash.
    for name in superglobals::SUPERGLOBALS {
        emit_superglobal_reset(emitter, &ir_global_symbol(name), &mut labels);
    }

    emit_concat_offset_reset(emitter);

    abi::emit_frame_restore(emitter, RESET_FRAME_SIZE);
    abi::emit_return(emitter);
}

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
