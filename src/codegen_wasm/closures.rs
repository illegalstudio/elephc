//! Purpose:
//! Lowers EIR closure / callable instructions for the wasm32-wasi backend and emits
//! the kind-6 (callable descriptor) refcount runtime `__rt_callable_descriptor_release`
//! referenced by `__rt_decref_any`. P7a0 ships only the release runtime + the ownership
//! wiring; the create/call lowering (`ClosureNew` / `ClosureCall` / wrappers /
//! `__rt_closure_call`) lands in P7a1.
//!
//! Called from:
//! - `crate::codegen_wasm::generate()` emits `emit_closure_runtime` right after the
//!   refcount runtime, because `__rt_decref_any`'s kind-6 branch calls
//!   `__rt_callable_descriptor_release` and WAT requires every call target to be
//!   defined for the module to validate.
//! - Unit-test harnesses that emit `emit_refcount_runtime` must also emit
//!   `emit_closure_runtime` for the same reason (see the M5 ripple).
//!
//! Key details:
//! - A callable is a single heap pointer (carried as `WasmRepr::I64`, a zero-extended
//!   i32) to a callable descriptor: a generic heap block whose 16-byte header is
//!   stamped with heap-kind 6 at `[ptr-8]`. The descriptor payload (P7a0 layout) is:
//!   `[ptr+0]` i64 descriptor kind (Closure=1; reserved for FirstClass/Static/Instance
//!   variants later), `[ptr+8]` i32 entry_index (the if-ladder key), `[ptr+12]` i32
//!   capture_count, `[ptr+16]` i32 capture_tags_ptr (a static per-closure tag-byte
//!   array), `[ptr+20]` pad, and capture slots at `[ptr+32 + i*16]` (low 8 = value/ptr,
//!   high 8 = string length). Slot base 32 (not native's 64) because WASM needs no
//!   signature/environment/invocation symbol records.
//! - `__rt_callable_descriptor_release` mirrors `__rt_decref_object`: null / below-payload
//!   / at-cursor guards, a refcount==0 re-entrancy guard, mark-zero, then a capture walk
//!   that releases each refcounted slot (tag in {1,4,5,6,7,10} = str/array/assoc/object/
//!   mixed/callable) via the kind-dispatched `__rt_decref_any` (so a callable capture
//!   recurses through kind-6), and finally `__rt_heap_free` (unsafe; refcount already 0).
//!   By-ref captures use tag sentinel 0xFF and are skipped (the promoted cell outlives
//!   the closure). P7a0 descriptors have capture_count 0, so the walk is a no-op today;
//!   the full walk is emitted now so P7b only needs `ClosureNew` to populate slots.

use super::wat::WatModule;

/// Registers the callable-descriptor refcount runtime (`__rt_callable_descriptor_release`)
/// on `wm`.
///
/// Must be emitted alongside `refcount::emit_refcount_runtime`, whose `__rt_decref_any`
/// calls this from its kind-6 branch. The function references only `__rt_decref_any`
/// (for the capture walk) and `__rt_heap_free`, both always present alongside the
/// refcount runtime, so — unlike `emit_object_runtime` — no extra globals are required
/// and the same emitter serves production modules and unit-test harnesses.
pub(super) fn emit_closure_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_CALLABLE_DESCRIPTOR_RELEASE);
}

/// `__rt_callable_descriptor_release`: the kind-6 release entry. Decrements the
/// descriptor refcount; at zero, walks the capture slots (releasing each refcounted
/// child via the kind-dispatched `__rt_decref_any`, so callable captures recurse) and
/// frees the descriptor. No-ops on null or non-heap pointers. Mirrors
/// `__rt_decref_object` (objects.rs) in guard shape and walk structure, but reads the
/// slot count/tags from the descriptor payload (`[ptr+12]` / `[ptr+16]`) instead of a
/// class gc_desc, since a closure's capture layout is per-descriptor, not per-class.
const RT_CALLABLE_DESCRIPTOR_RELEASE: &str = r#"(func $__rt_callable_descriptor_release (param $ptr i32)
  (local $rc i32) (local $n i32) (local $tags i32) (local $i i32) (local $tag i32) (local $slot i32)
  (if (i32.eqz (local.get $ptr)) (then (return)))                    ;; guard: null pointer
  (if (i32.lt_u (local.get $ptr) (i32.add (global.get $__heap_base) (i32.const 16)))
    (then (return)))                                                  ;; guard: below first payload (borrowed/literal)
  (if (i32.ge_u (local.get $ptr) (global.get $__heap_ptr))
    (then (return)))                                                  ;; guard: at/after bump cursor (not live)
  (local.set $rc (i32.load (i32.sub (local.get $ptr) (i32.const 12))))  ;; refcount = [ptr-12]
  (if (i32.eqz (local.get $rc)) (then (return)))                    ;; guard: refcount == 0 (re-entrancy)
  (local.set $rc (i32.add (local.get $rc) (i32.const -1)))          ;; rc = rc - 1
  (if (i32.ne (local.get $rc) (i32.const 0)) (then                  ;; keep path: rc > 0, store and return
    (i32.store (i32.sub (local.get $ptr) (i32.const 12)) (local.get $rc))  ;; store decremented refcount
    (return)))                                                       ;; keep live and return
  (i32.store (i32.sub (local.get $ptr) (i32.const 12)) (i32.const 0))  ;; mark refcount 0 (re-entrancy guard)
  (local.set $n (i32.load offset=12 (local.get $ptr)))             ;; capture_count = [ptr+12]
  (local.set $tags (i32.load offset=16 (local.get $ptr)))          ;; capture_tags_ptr = [ptr+16]
  (local.set $i (i32.const 0))                                     ;; capture index = 0
  (block $walk_end
    (loop $walk
      (br_if $walk_end (i32.ge_u (local.get $i) (local.get $n)))   ;; i >= n -> end walk
      (local.set $tag (i32.load8_u (i32.add (local.get $tags) (local.get $i))))  ;; tag = tags[i]
      ;; refcounted tags: 1 (str), 4 (array), 5 (assoc), 6 (object), 7 (mixed), 10 (callable).
      ;; Scalars (0/2/3), null (8), and the by-ref sentinel (0xFF) own no heap storage.
      (if (i32.or (i32.or (i32.eq (local.get $tag) (i32.const 1)) (i32.and (i32.ge_u (local.get $tag) (i32.const 4)) (i32.le_u (local.get $tag) (i32.const 7)))) (i32.eq (local.get $tag) (i32.const 10))) (then  ;; tag in {1,4,5,6,7,10} -> release the slot
        (local.set $slot (i32.wrap_i64 (i64.load offset=32 (i32.add (local.get $ptr) (i32.mul (local.get $i) (i32.const 16))))))  ;; slot ptr = low 8 bytes of [ptr+32+i*16]
        (call $__rt_decref_any (local.get $slot))                  ;; release the child (kind-dispatched; callable recurses via kind 6)
      )                                                            ;; close then (tag check)
      )                                                            ;; close if (tag check)
      (local.set $i (i32.add (local.get $i) (i32.const 1)))        ;; i++
      (br $walk)                                                   ;; loop back
    )                                                              ;; close loop $walk
  )                                                                ;; close block $walk_end
  (call $__rt_heap_free (local.get $ptr))                          ;; free the descriptor (unsafe: refcount already 0)
  (return)                                                         ;; top-level return
)
"#;

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the P7a0 callable-descriptor release runtime, exercised
    //! end-to-end under `wasmer` via hand-written driver functions and `--invoke`.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Each test builds a reactor module containing the heap + refcount + closure
    //!   runtime (plus the array/mixed/hash/float/object runtimes that `__rt_decref_any`
    //!   may dispatch to), validates it with `wasmparser`, and runs the driver under
    //!   `wasmer`. Runs skip silently when `wasmer` is absent (validation always runs).
    //! - P7a0 covers only the release contract: kind-6 dispatch through
    //!   `__rt_decref_any`, the refcount keep/free paths, and a callable boxed in a
    //!   Mixed cell releasing through the tag-10 arm. Create/call lowering is P7a1.

    use super::emit_closure_runtime;
    use super::super::arrays::emit_array_runtime;
    use super::super::classes::{emit_class_metadata_stub, emit_class_runtime};
    use super::super::heap::emit_heap_runtime;
    use super::super::mixed::emit_mixed_runtime;
    use super::super::objects::{emit_destructor_dispatch_stub, emit_gc_desc_stub, emit_object_runtime};
    use super::super::refcount::emit_refcount_runtime;
    use super::super::wat::{WatModule};
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique temp directory path so concurrent wasmer runs never collide.
    fn unique_tmp_dir() -> std::path::PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("elephc_wasm_p7_{}_{}", std::process::id(), n))
    }

    /// Returns whether the `wasmer` CLI is available.
    fn wasmer_available() -> bool {
        std::process::Command::new("wasmer")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Builds a 3-page reactor module with the heap + refcount + closure runtime and the
    /// full `__rt_decref_any` dispatch surface, validates it, and runs `export` under
    /// `wasmer`, returning trimmed stdout. `None` if wasmer is absent; validation
    /// always runs.
    fn run_driver(driver: &str, export: &str) -> Option<String> {
        let mut wm = WatModule::new();
        wm.set_memory(3, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 3 * 65536);
        emit_refcount_runtime(&mut wm);
        // `__rt_decref_any` dispatches to the array/hash/mixed/object/closure runtimes,
        // so all of them must be present to validate (generate() emits all of them).
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm);
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
        super::super::hashes::emit_hash_runtime(&mut wm);
        emit_object_runtime(&mut wm);
        emit_gc_desc_stub(&mut wm);
        emit_destructor_dispatch_stub(&mut wm);
        emit_class_metadata_stub(&mut wm);
        emit_class_runtime(&mut wm);
        // `__rt_decref_any` kind-6 dispatches to `__rt_callable_descriptor_release`.
        emit_closure_runtime(&mut wm);
        wm.add_raw_func(driver);
        let wat = wm.render();
        let bytes = ::wat::parse_str(&wat)
            .unwrap_or_else(|e| panic!("WAT did not assemble: {e}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|e| panic!("wasm did not validate: {e}\n{wat}"));
        if !wasmer_available() {
            return None;
        }
        let dir = unique_tmp_dir();
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("m.wasm");
        std::fs::write(&path, &bytes).expect("write wasm");
        let out = std::process::Command::new("wasmer")
            .arg("run")
            .arg("--invoke")
            .arg(export)
            .arg(&path)
            .output()
            .expect("run wasmer");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            out.status.success(),
            "wasmer --invoke {export} failed: {}\n{}",
            String::from_utf8_lossy(&out.stderr),
            wat
        );
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// A kind-6 descriptor at refcount 1, released through `__rt_decref_any` (the
    /// kind-6 branch), frees the block, so `_gc_live` returns to "0". Proves the
    /// kind-6 dispatch routes to `__rt_callable_descriptor_release` and the rc 1 -> 0
    /// path frees (no captures, so the walk is empty).
    #[test]
    fn decref_any_kind6_frees_descriptor() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $d i32)
  (local.set $d (call $__rt_heap_alloc (i32.const 32)))            ;; 32-byte descriptor (no captures)
  (i64.store (i32.sub (local.get $d) (i32.const 8)) (i64.const 6)) ;; stamp heap-header kind = 6 (callable)
  (i64.store (local.get $d) (i64.const 1))                         ;; descriptor kind = 1 (Closure)
  (i32.store offset=8 (local.get $d) (i32.const 0))               ;; entry_index = 0
  (i32.store offset=12 (local.get $d) (i32.const 0))              ;; capture_count = 0
  (i32.store offset=16 (local.get $d) (i32.const 0))              ;; capture_tags_ptr = 0 (no walk)
  (call $__rt_decref_any (local.get $d))                          ;; kind 6 -> release -> rc 1 -> 0 -> free
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }

    /// A kind-6 descriptor incref'd to refcount 2, then released once via
    /// `__rt_decref_any` (rc 2 -> 1, NOT freed), stays live — proving the above-zero
    /// keep path holds the descriptor (a shared callable is never freed early).
    #[test]
    fn decref_any_kind6_above_zero_keeps_descriptor() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $d i32)
  (local.set $d (call $__rt_heap_alloc (i32.const 32)))
  (i64.store (i32.sub (local.get $d) (i32.const 8)) (i64.const 6)) ;; stamp kind 6
  (call $__rt_incref (local.get $d))                              ;; rc 1 -> 2
  (call $__rt_decref_any (local.get $d))                          ;; rc 2 -> 1, not freed
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_ne!(o, "0");
        }
    }

    /// A callable descriptor boxed in a Mixed cell (tag 10). `__rt_mixed_from_value`
    /// shares ownership (it increfs the refcounted child), so the balanced sequence is:
    /// the caller owns the descriptor (rc 1), boxes it (cell increfs -> rc 2), the
    /// caller releases its own ref (rc 1), then releasing the cell (kind 5 ->
    /// `__rt_decref_mixed` -> tag-10 arm -> `__rt_decref_any` on the child -> kind 6 ->
    /// descriptor release) drops the last ref and frees both the cell and the
    /// descriptor, so `_gc_live` returns to "0". Proves the Mixed-tag-10 path releases a
    /// callable correctly with NO change to `mixed.rs` (the tag-10 arm already calls
    /// `__rt_decref_any`, which now dispatches kind 6).
    #[test]
    fn mixed_tag10_releases_callable_descriptor() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $d i32) (local $c i32)
  (local.set $d (call $__rt_heap_alloc (i32.const 32)))            ;; descriptor
  (i64.store (i32.sub (local.get $d) (i32.const 8)) (i64.const 6)) ;; stamp heap-header kind = 6 (callable)
  (i32.store offset=12 (local.get $d) (i32.const 0))               ;; capture_count = 0
  (i64.const 10)                                                   ;; tag = 10 (callable)
  (i64.extend_i32_u (local.get $d))                                ;; lo = descriptor pointer
  (i64.const 0)                                                    ;; hi = 0
  (call $__rt_mixed_from_value)                                    ;; box: cell increfs the descriptor (rc 1 -> 2)
  (local.set $c)
  (call $__rt_decref_any (local.get $d))                           ;; caller releases its own ref (rc 2 -> 1)
  (call $__rt_decref_any (local.get $c))                           ;; cell kind 5 -> tag-10 -> child kind 6 -> rc 1 -> 0 -> free both
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }
}