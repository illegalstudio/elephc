//! Purpose:
//! Emits the hand-authored WebAssembly (WAT) refcounting primitives for the
//! wasm32-wasi backend: `__rt_incref`, the `__rt_decref_any` free dispatcher,
//! the dedicated ref-cell retain/release pair, and `__rt_str_persist` (copy a
//! transient string into an owned heap block). These sit on top of the
//! linear-memory allocator (`heap`) and back the EIR ownership ops.
//!
//! Called from:
//! - `crate::codegen_wasm::generate()` for every module, right after the heap.
//!
//! Key details:
//! - Refcount lives at the i32 word `user_ptr - 12` (header + 4). `incref`/`decref`
//!   range-check the pointer against `[__heap_base + 16, __heap_ptr)` and no-op on
//!   borrowed / data-segment / concat-scratch pointers, so the native runtime's
//!   "non-heap pointers are skipped" contract holds without a sentinel refcount.
//! - `__rt_decref_any` dispatches on the header kind low-byte. Kind 1 (string) is
//!   freed directly via `__rt_heap_free_safe` (strings use copy-on-acquire, so a
//!   release is a free). Kinds 2/3/5 (array/hash/mixed) deep-free at zero through
//!   `__rt_decref_array`/`__rt_decref_hash`/`__rt_decref_mixed`; kind 4 (object)
//!   releases through `__rt_decref_object` (P6a: scalar-only, no property walk); kind
//!   6 (callable descriptor) releases through `__rt_callable_descriptor_release`
//!   (P7a0, emitted by `closures`); any other kind is a no-op today.
//! - Ref cells use dedicated heap kind 7 and a stricter runtime validator before
//!   touching their header. Their release takes a compile-time payload tag, so
//!   scalar bits are never interpreted as a possible heap pointer.
//! - `__rt_str_persist` always copies into a fresh heap block (PHP string value
//!   semantics). The native runtime may incref an already-heap string instead; the
//!   observable string content and lifetime are identical.

use super::wat::WatModule;

/// Adds the refcounting runtime routines to `wm`. Must be emitted alongside the
/// heap runtime (`heap::emit_heap_runtime`), whose globals and `__rt_heap_alloc` /
/// `__rt_heap_free_safe` these routines reference.
pub(super) fn emit_refcount_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_INCREF);
    wm.add_raw_func(RT_DECREF_ANY);
    wm.add_raw_func(RT_REF_CELL_IS_LIVE);
    wm.add_raw_func(RT_REF_CELL_INCREF);
    wm.add_raw_func(RT_REF_CELL_RELEASE);
    wm.add_raw_func(RT_STR_PERSIST);
}

/// `__rt_incref`: increments the refcount of a live heap block; no-ops on a null,
/// below-heap (data-segment / concat-scratch), or past-the-cursor pointer.
const RT_INCREF: &str = r#"(func $__rt_incref (param $ptr i32)
  (if (i32.eqz (local.get $ptr))                  ;; guard: null pointer
    (then (return)))
  (if (i32.lt_u (local.get $ptr) (i32.add (global.get $__heap_base) (i32.const 16)))
    (then (return)))                              ;; guard: below first payload (borrowed/literal)
  (if (i32.ge_u (local.get $ptr) (global.get $__heap_ptr))
    (then (return)))                              ;; guard: at/after bump cursor (not live)
  (i32.store                                      ;; refcount[ptr-12] += 1
    (i32.sub (local.get $ptr) (i32.const 12))
    (i32.add (i32.load (i32.sub (local.get $ptr) (i32.const 12))) (i32.const 1))))
"#;

/// `__rt_decref_any`: the kind-dispatched release entry. Frees a string (kind 1)
/// directly (copy-on-acquire model); decrefs an indexed array (kind 2) via
/// `__rt_decref_array`, an associative hash (kind 3) via `__rt_decref_hash`, a
/// boxed Mixed cell (kind 5) via `__rt_decref_mixed`, an object (kind 4) via
/// `__rt_decref_object` (P6a scalar-only release), and a callable descriptor
/// (kind 6) via `__rt_callable_descriptor_release` (P7a0). Any other kind is a
/// no-op. No-ops on non-heap pointers.
const RT_DECREF_ANY: &str = r#"(func $__rt_decref_any (param $ptr i32)
  (local $kind i32)
  (if (i32.eqz (local.get $ptr))                  ;; guard: null pointer
    (then (return)))
  (if (i32.lt_u (local.get $ptr) (i32.add (global.get $__heap_base) (i32.const 16)))
    (then (return)))                              ;; guard: below first payload
  (if (i32.ge_u (local.get $ptr) (global.get $__heap_ptr))
    (then (return)))                              ;; guard: at/after bump cursor
  (local.set $kind                                ;; kind = low byte of the kind word
    (i32.and (i32.wrap_i64 (i64.load (i32.sub (local.get $ptr) (i32.const 8)))) (i32.const 255)))
  (if (i32.eq (local.get $kind) (i32.const 1))    ;; kind 1 = string
    (then
      (call $__rt_heap_free_safe (local.get $ptr)) ;; copy-on-acquire string: release == free
      (return)))
  (if (i32.eq (local.get $kind) (i32.const 2))    ;; kind 2 = indexed array
    (then
      (call $__rt_decref_array (local.get $ptr))   ;; decrement; deep-free at zero
      (return)))
  (if (i32.eq (local.get $kind) (i32.const 3))    ;; kind 3 = associative hash
    (then
      (call $__rt_decref_hash (local.get $ptr))    ;; decrement; deep-free at zero
      (return)))
  (if (i32.eq (local.get $kind) (i32.const 5))    ;; kind 5 = boxed mixed cell
    (then
      (call $__rt_decref_mixed (local.get $ptr))   ;; decrement; deep-free at zero
      (return)))
  (if (i32.eq (local.get $kind) (i32.const 4))    ;; kind 4 = object instance
    (then
      (call $__rt_decref_object (local.get $ptr))  ;; P6a: scalar-only release, frees at zero
      (return)))
  (if (i32.eq (local.get $kind) (i32.const 6))    ;; kind 6 = callable descriptor
    (then
      (call $__rt_callable_descriptor_release (local.get $ptr)) ;; P7a0: decref + capture walk + free
      (return)))
  (return))
"#;

/// `__rt_ref_cell_is_live`: validates an owned ref-cell payload pointer before
/// either dedicated ref-cell helper may read or mutate its refcount.
///
/// The range and alignment guards run before header loads. The allocator payload
/// size must cover the 16-byte cell, the complete block must remain below the bump
/// cursor, the full kind word must be the dedicated value 7, and the refcount must
/// be non-zero. Borrowed foreach element addresses and ordinary heap values therefore
/// fail closed instead of being mistaken for owned ref cells.
const RT_REF_CELL_IS_LIVE: &str =
    r#"(func $__rt_ref_cell_is_live (param $ptr i32) (result i32)
  (local $size i32)
  (if (i32.eqz (local.get $ptr)) (then (return (i32.const 0))))    ;; reject null
  (if (i32.ne (i32.and (local.get $ptr) (i32.const 7)) (i32.const 0))
    (then (return (i32.const 0))))                                 ;; reject misaligned/interior pointers
  (if (i32.lt_u (local.get $ptr) (i32.add (global.get $__heap_base) (i32.const 16)))
    (then (return (i32.const 0))))                                 ;; reject below first payload
  (if (i32.ge_u (local.get $ptr) (global.get $__heap_ptr))
    (then (return (i32.const 0))))                                 ;; reject at/after bump cursor
  (local.set $size (i32.load (i32.sub (local.get $ptr) (i32.const 16))))  ;; allocator payload size
  (if (i32.lt_u (local.get $size) (i32.const 16))
    (then (return (i32.const 0))))                                 ;; ref-cell payload needs two words
  (if (i64.gt_u
        (i64.add (i64.extend_i32_u (local.get $ptr)) (i64.extend_i32_u (local.get $size)))
        (i64.extend_i32_u (global.get $__heap_ptr)))
    (then (return (i32.const 0))))                                 ;; reject incomplete/out-of-range block
  (if (i64.ne (i64.load (i32.sub (local.get $ptr) (i32.const 8))) (i64.const 7))
    (then (return (i32.const 0))))                                 ;; reject non-ref-cell heap kinds
  (if (i32.eqz (i32.load (i32.sub (local.get $ptr) (i32.const 12))))
    (then (return (i32.const 0))))                                 ;; reject freed/re-entrant blocks
  (return (i32.const 1)))
"#;

/// `__rt_ref_cell_incref`: retains one proven live kind-7 ref cell.
///
/// Unlike generic `__rt_incref`, this helper refuses any pointer that does not
/// satisfy the dedicated ref-cell header contract, so an interior array-element
/// address cannot become descriptor-owned accidentally.
const RT_REF_CELL_INCREF: &str = r#"(func $__rt_ref_cell_incref (param $ptr i32)
  (if (i32.eqz (call $__rt_ref_cell_is_live (local.get $ptr)))
    (then (return)))                                               ;; invalid/non-owned ref binding
  (i32.store
    (i32.sub (local.get $ptr) (i32.const 12))
    (i32.add (i32.load (i32.sub (local.get $ptr) (i32.const 12))) (i32.const 1))))
"#;

/// `__rt_ref_cell_release`: releases one kind-7 ref-cell owner and destroys the
/// payload plus cell exactly when the last owner disappears.
///
/// `payload_tag` uses the WASM capture-tag table (without the by-ref sentinel).
/// Only tags known to carry an owned heap pointer load `[cell+0]`; scalar payload
/// bits are never sent to `__rt_decref_any`.
const RT_REF_CELL_RELEASE: &str =
    r#"(func $__rt_ref_cell_release (param $ptr i32) (param $payload_tag i32)
  (local $rc i32) (local $payload i32)
  (if (i32.eqz (call $__rt_ref_cell_is_live (local.get $ptr)))
    (then (return)))                                               ;; invalid/non-owned ref binding
  (local.set $rc
    (i32.add (i32.load (i32.sub (local.get $ptr) (i32.const 12))) (i32.const -1)))  ;; rc--
  (if (i32.ne (local.get $rc) (i32.const 0))
    (then
      (i32.store (i32.sub (local.get $ptr) (i32.const 12)) (local.get $rc))
      (return)))                                                   ;; another owner keeps the cell alive
  (i32.store (i32.sub (local.get $ptr) (i32.const 12)) (i32.const 0))  ;; re-entrancy guard
  (if
    (i32.or
      (i32.or
        (i32.or
          (i32.eq (local.get $payload_tag) (i32.const 1))
          (i32.and
            (i32.ge_u (local.get $payload_tag) (i32.const 4))
            (i32.le_u (local.get $payload_tag) (i32.const 7))))
        (i32.eq (local.get $payload_tag) (i32.const 10)))
      (i32.eq (local.get $payload_tag) (i32.const 12)))
    (then
      (local.set $payload (i32.load offset=0 (local.get $ptr)))    ;; typed heap payload only
      (call $__rt_decref_any (local.get $payload))))               ;; release string/container/callable
  (call $__rt_heap_free (local.get $ptr)))                         ;; free cell after payload
"#;

/// `__rt_str_persist`: copies a string (data-segment literal or transient concat
/// buffer) into a fresh kind-1 heap block so it can be independently owned, and
/// returns the new `(ptr, len)`.
const RT_STR_PERSIST: &str = r#"(func $__rt_str_persist (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $n i32)
  (local $new i32)
  (local $i i32)
  (local.set $n
    (call $__rt_checked_layout
      (local.get $len)
      (i64.const 1)
      (i64.const 0)))                                       ;; checked byte length; rejects negative/wasm32 overflow
  (local.set $new (call $__rt_heap_alloc (local.get $n)))   ;; fresh heap block (8-byte minimum)
  (i64.store (i32.sub (local.get $new) (i32.const 8)) (i64.const 1)) ;; stamp header kind = 1 (string)
  (local.set $i (i32.const 0))                              ;; i = 0 (copy cursor)
  (block $end (loop $copy
    (br_if $end (i32.ge_u (local.get $i) (local.get $n)))   ;; stop after n bytes
    (i32.store8
      (i32.add (local.get $new) (local.get $i))
      (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))  ;; dst[i] = src[i]
    (local.set $i (i32.add (local.get $i) (i32.const 1)))   ;; i++
    (br $copy)))                                            ;; next byte
  (return (local.get $new) (local.get $len)))               ;; new pointer + original length
"#;

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the WAT refcounting primitives, exercised end-to-end under
    //! `wasmer` via a hand-written driver function and `--invoke`.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Each test builds a reactor module containing the heap runtime + the
    //!   refcount runtime + one exported driver, validates it with `wasmparser`,
    //!   and runs the driver under `wasmer`. Runs skip silently when `wasmer` is
    //!   absent (validation always runs).

    use super::emit_refcount_runtime;
    use super::super::arrays::emit_array_runtime;
    use super::super::classes::{emit_class_metadata_stub, emit_class_runtime};
    use super::super::closures::emit_closure_runtime;
    use super::super::heap::emit_heap_runtime;
    use super::super::mixed::emit_mixed_runtime;
    use super::super::objects::{emit_destructor_dispatch_stub, emit_gc_desc_stub, emit_object_runtime};
    use super::super::wat::{DataSegment, Global, ValType, WatModule};
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique temp directory path so concurrent wasmer runs never collide.
    fn unique_tmp_dir() -> std::path::PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("elephc_wasm_rc_{}_{}", std::process::id(), n))
    }

    /// Returns whether the `wasmer` CLI is available.
    fn wasmer_available() -> bool {
        std::process::Command::new("wasmer")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Builds a 3-page reactor module with the heap + refcount runtime and `driver`,
    /// validates it, and runs `export` under `wasmer`, returning trimmed stdout.
    /// `None` if wasmer is absent; validation always runs.
    fn run_driver(driver: &str, export: &str) -> Option<String> {
        let mut wm = WatModule::new();
        wm.set_memory(3, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 3 * 65536);
        emit_refcount_runtime(&mut wm);
        // `__rt_decref_any` kind-6 dispatches to `__rt_callable_descriptor_release`, so
        // the closure runtime must be present to validate (generate() emits it alongside
        // refcount).
        emit_closure_runtime(&mut wm);
        // `__rt_decref_any` dispatches to `__rt_decref_array` / `__rt_decref_hash` /
        // `__rt_decref_mixed`, so the array, hash, and mixed runtimes must be present
        // to validate (generate() emits all of them).
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm, false);
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
        super::super::hashes::emit_hash_runtime(&mut wm);
        // `__rt_decref_any` kind-4 dispatches to `__rt_decref_object`, so the object
        // runtime must be present to validate (generate() emits it alongside refcount).
        emit_object_runtime(&mut wm);
        // `__rt_decref_object` references the `$__gc_desc_*` globals; the stub declares
        // empty-table globals so the walk is skipped for harness blocks (no classes).
        emit_gc_desc_stub(&mut wm);
        // `__rt_decref_object` also calls `__rt_call_object_destructor`; the 0-arm stub
        // resolves the call as a no-op for harness blocks that hold no destructors.
        emit_destructor_dispatch_stub(&mut wm);
        emit_class_metadata_stub(&mut wm);
        emit_class_runtime(&mut wm);
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

    /// Two increfs on a freshly-allocated block (refcount 1) leave refcount 3.
    #[test]
    fn incref_bumps_refcount() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $a i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 16)))
  (call $__rt_incref (local.get $a))
  (call $__rt_incref (local.get $a))
  (i32.load (i32.sub (local.get $a) (i32.const 12))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "3");
        }
    }

    /// `__rt_decref_any` on a kind-1 (string) block frees it, restoring `_gc_live`
    /// to 0 (the kind-1 dispatch routes to `__rt_heap_free_safe`).
    #[test]
    fn decref_any_frees_string_kind() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 16)))
  (i64.store (i32.sub (local.get $a) (i32.const 8)) (i64.const 1))
  (call $__rt_decref_any (local.get $a))
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }

    /// A non-string kind (e.g. raw, kind 0) is a no-op for `__rt_decref_any` today:
    /// the block stays live, so `_gc_live` is unchanged (32 = payload 16 + header 16).
    #[test]
    fn decref_any_noops_on_other_kinds() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 16)))
  (call $__rt_decref_any (local.get $a))
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "32");
        }
    }

    /// Retaining a kind-7 ref cell creates a real second owner: the first release
    /// leaves refcount 1 and the final release frees the cell, restoring `_gc_live`.
    #[test]
    fn ref_cell_retain_and_last_release_balance_heap() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $cell i32) (local $first_ok i32)
  (local.set $cell (call $__rt_heap_alloc (i32.const 16)))
  (i64.store (i32.sub (local.get $cell) (i32.const 8)) (i64.const 7))
  (i64.store (local.get $cell) (i64.const 42))
  (call $__rt_ref_cell_incref (local.get $cell))
  (call $__rt_ref_cell_release (local.get $cell) (i32.const 0))
  (local.set $first_ok
    (i32.and
      (call $__rt_ref_cell_is_live (local.get $cell))
      (i32.eq
        (i32.load (i32.sub (local.get $cell) (i32.const 12)))
        (i32.const 1))))
  (call $__rt_ref_cell_release (local.get $cell) (i32.const 0))
  (i32.and
    (local.get $first_ok)
    (i64.eq (global.get $_gc_live) (i64.const 0))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1");
        }
    }

    /// A scalar cell payload may numerically equal another live heap pointer.
    /// Releasing it with tag 0 must never load/decref those integer bits.
    #[test]
    fn scalar_ref_cell_payload_never_decrefs_pointer_shaped_bits() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $victim i32) (local $cell i32) (local $ok i32)
  (local.set $victim (call $__rt_heap_alloc (i32.const 16)))
  (i64.store (i32.sub (local.get $victim) (i32.const 8)) (i64.const 1))
  (local.set $cell (call $__rt_heap_alloc (i32.const 16)))
  (i64.store (i32.sub (local.get $cell) (i32.const 8)) (i64.const 7))
  (i64.store (local.get $cell) (i64.extend_i32_u (local.get $victim)))
  (call $__rt_ref_cell_release (local.get $cell) (i32.const 0))
  (local.set $ok
    (i32.and
      (i32.eq
        (i32.load (i32.sub (local.get $victim) (i32.const 12)))
        (i32.const 1))
      (i64.eq (global.get $_gc_live) (i64.const 32))))
  (call $__rt_decref_any (local.get $victim))
  (local.get $ok))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1");
        }
    }

    /// Dedicated ref-cell helpers reject an aligned interior address before
    /// mutating a header; the owning cell keeps refcount 1 and is safely releasable.
    #[test]
    fn ref_cell_helpers_reject_interior_pointer() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $cell i32) (local $interior i32) (local $ok i32)
  (local.set $cell (call $__rt_heap_alloc (i32.const 16)))
  (i64.store (i32.sub (local.get $cell) (i32.const 8)) (i64.const 7))
  (local.set $interior (i32.add (local.get $cell) (i32.const 8)))
  (call $__rt_ref_cell_incref (local.get $interior))
  (call $__rt_ref_cell_release (local.get $interior) (i32.const 0))
  (local.set $ok
    (i32.and
      (call $__rt_ref_cell_is_live (local.get $cell))
      (i32.eq
        (i32.load (i32.sub (local.get $cell) (i32.const 12)))
        (i32.const 1))))
  (call $__rt_ref_cell_release (local.get $cell) (i32.const 0))
  (i32.and
    (local.get $ok)
    (i64.eq (global.get $_gc_live) (i64.const 0))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1");
        }
    }

    /// `__rt_str_persist` copies the source bytes into a fresh heap block: the
    /// driver writes "ABC" below the heap, persists it, and reads the 3 bytes back
    /// from the returned pointer (packed as b0<<16 | b1<<8 | b2 = 4276803).
    #[test]
    fn str_persist_copies_bytes_to_heap() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $new i32)
  (i32.store8 (i32.const 200) (i32.const 65))
  (i32.store8 (i32.const 201) (i32.const 66))
  (i32.store8 (i32.const 202) (i32.const 67))
  (call $__rt_str_persist (i32.const 200) (i64.const 3))
  drop
  local.set $new
  (i32.add
    (i32.add
      (i32.mul (i32.load8_u (local.get $new)) (i32.const 65536))
      (i32.mul (i32.load8_u (i32.add (local.get $new) (i32.const 1))) (i32.const 256)))
    (i32.load8_u (i32.add (local.get $new) (i32.const 2)))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "4276803");
        }
    }

    // ----- P6a: kind-4 (object) release through __rt_decref_object -----

    /// Allocates a kind-4 block, releases it via `__rt_decref_object` (refcount 1 -> 0
    /// -> `__rt_heap_free_safe`), and reads the live-byte counter `_gc_live`. A freed
    /// block contributes zero live bytes, so the result is "0" — deterministically
    /// proving the kind-4 release frees the block (mirrors `decref_any_frees_string_kind`).
    #[test]
    fn decref_object_frees_block() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 40)))            ;; 40 = 8 + 2*16 payload
  (i64.store (i32.sub (local.get $a) (i32.const 8)) (i64.const 4)) ;; stamp kind 4 (object)
  (call $__rt_decref_object (local.get $a))                       ;; rc 1 -> 0 -> free
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }

    /// Allocates a kind-4 block, increfs it to refcount 2, then releases once via
    /// `__rt_decref_object` (rc 2 -> 1, NOT freed). The block stays live, so `_gc_live`
    /// is nonzero — proving the above-zero path keeps the block alive (the negative
    /// scoping guarantee for P6a's no-walk release: a shared object is never freed early).
    #[test]
    fn decref_object_above_zero_keeps_block() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 40)))            ;; 40 = 8 + 2*16 payload
  (i64.store (i32.sub (local.get $a) (i32.const 8)) (i64.const 4)) ;; stamp kind 4 (object)
  (call $__rt_incref (local.get $a))                              ;; rc 1 -> 2
  (call $__rt_decref_object (local.get $a))                       ;; rc 2 -> 1, not freed
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_ne!(o, "0");
        }
    }

    /// `__rt_decref_object` on an object holding a kind-1 (string) property value walks the
    /// real gc_desc, releases the string child via `__rt_decref_any`, then frees the object,
    /// so `_gc_live` returns to 0. Unlike the stub-based P6a tests, this registers a real
    /// one-class descriptor (class 0 = one property of tag 1 / string) so the property walk
    /// actually executes and the child is released — exercising the full P6b deep-free path.
    #[test]
    fn decref_object_walks_and_releases_string_property() {
        let mut wm = WatModule::new();
        wm.set_memory(3, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 3 * 65536);
        emit_refcount_runtime(&mut wm);
        // `__rt_decref_any` kind-6 dispatches to `__rt_callable_descriptor_release`.
        emit_closure_runtime(&mut wm);
        // `__rt_decref_any` dispatches to the array/hash/mixed object runtimes.
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm, false);
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
        super::super::hashes::emit_hash_runtime(&mut wm);
        emit_object_runtime(&mut wm);
        // `__rt_decref_object` calls `__rt_call_object_destructor`; this harness registers no
        // class with a destructor, so the 0-arm stub resolves the call as a no-op.
        emit_destructor_dispatch_stub(&mut wm);
        emit_class_metadata_stub(&mut wm);
        emit_class_runtime(&mut wm);
        // Real gc_desc for class 0: one property of tag 1 (string). The desc byte and the
        // 4-aligned pointer table sit in the free [0, 64) region below CONCAT_BASE so they
        // never collide with the concat scratch (never written here) or the heap (>= 1024).
        wm.add_data(DataSegment { offset: 8, bytes: vec![1u8] });
        wm.add_data(DataSegment { offset: 12, bytes: 8u32.to_le_bytes().to_vec() });
        // Class 0's layout metadata: one declared property, no dynamic-property tail. The
        // release walk reads its count from HERE rather than from the block size.
        wm.add_data(DataSegment { offset: 16, bytes: 1u32.to_le_bytes().to_vec() });
        wm.add_global(Global {
            name: "__gc_desc_ptrs".to_string(),
            ty: ValType::I32,
            mutable: false,
            init: 12,
        });
        wm.add_global(Global {
            name: "__gc_desc_count".to_string(),
            ty: ValType::I32,
            mutable: false,
            init: 1,
        });
        wm.add_global(Global {
            name: "__gc_desc_meta".to_string(),
            ty: ValType::I32,
            mutable: false,
            init: 16,
        });
        // Static "hi" source for __rt_str_persist, also in the free [0, 64) region.
        wm.add_data(DataSegment { offset: 32, bytes: b"hi".to_vec() });
        let driver = r#"(func $t (export "t") (result i64)
  (local $o i32) (local $s i32)
  (local.set $o (call $__rt_heap_alloc (i32.const 24)))            ;; object: 8 + 1*16 payload
  (i64.store (i32.sub (local.get $o) (i32.const 8)) (i64.const 4)) ;; stamp kind 4 (object)
  (i64.store (local.get $o) (i64.const 0))                         ;; class_id = 0
  (i32.const 32) (i64.const 2) (call $__rt_str_persist)            ;; persist "hi" -> (ptr i32, len i64)
  (drop)                                                           ;; discard the length
  (local.set $s)                                                   ;; $s = owned string ptr (rc 1)
  (i32.store (i32.add (local.get $o) (i32.const 8)) (local.get $s)) ;; store string in slot 0 lo
  (call $__rt_decref_object (local.get $o))                        ;; walk releases string, then frees object
  (global.get $_gc_live))"#;
        wm.add_raw_func(driver);
        let wat = wm.render();
        let bytes = ::wat::parse_str(&wat)
            .unwrap_or_else(|e| panic!("WAT did not assemble: {e}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|e| panic!("wasm did not validate: {e}\n{wat}"));
        if !wasmer_available() {
            return;
        }
        let dir = unique_tmp_dir();
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("m.wasm");
        std::fs::write(&path, &bytes).expect("write wasm");
        let out = std::process::Command::new("wasmer")
            .arg("run")
            .arg("--invoke")
            .arg("t")
            .arg(&path)
            .output()
            .expect("run wasmer");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            out.status.success(),
            "wasmer --invoke t failed: {}\n{}",
            String::from_utf8_lossy(&out.stderr),
            wat
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
    }

    // ----- P6f: box_call_result_into_mixed source-release contract -----

    /// Regression guard for the P6f `box_call_result_into_mixed` ownership fix. The
    /// wasm lowerer boxes a callee's owned string return by calling
    /// `__rt_mixed_from_value` (which persists a fresh COPY and leaves the source
    /// owned — it does NOT consume it) and then `__rt_decref_any` on the source (the
    /// fix). Because the source is a WAT-stack callee return, the EIR ownership pass
    /// cannot see it, so the lowerer must release it itself.
    ///
    /// This driver replays that exact sequence against the real runtime: persist a
    /// 2-byte string into an owned heap block (rc 1), box it via `from_value` (the
    /// cell persists its own copy; the source rc is untouched), release the source via
    /// `__rt_decref_any` (rc 1 -> 0 -> freed), then release the cell via
    /// `__rt_decref_any` (kind 5 -> free_deep -> frees the cell + its persisted copy),
    /// and return `_gc_live`. With the source release the cycle is balanced -> "0";
    /// omitting the source release (the bug) leaks the source block -> nonzero.
    #[test]
    fn mixed_from_value_string_then_source_release_is_balanced() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $s i32)
  (local $cell i32)
  i32.const 32                            ;; source address (below heap, zero-filled)
  i64.const 2                             ;; 2-byte string
  call $__rt_str_persist                   ;; persist -> (ptr i32, len i64)
  drop                                     ;; discard the length
  local.set $s                             ;; $s = owned heap string (rc 1)
  i64.const 1                              ;; tag = string
  local.get $s                             ;; lo = source pointer
  i64.extend_i32_u
  i64.const 2                             ;; hi = length
  call $__rt_mixed_from_value              ;; cell persists its own copy; source rc unchanged
  local.set $cell
  local.get $s                             ;; release the callee's owned source (P6f fix)
  call $__rt_decref_any                    ;; rc 1 -> 0 -> freed
  local.get $cell                          ;; release the cell
  call $__rt_decref_any                    ;; kind 5 -> free_deep -> frees cell + its copy
  global.get $_gc_live)                    ;; balanced -> 0
"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }
}
