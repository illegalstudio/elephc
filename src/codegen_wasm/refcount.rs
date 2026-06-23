//! Purpose:
//! Emits the hand-authored WebAssembly (WAT) refcounting primitives for the
//! wasm32-wasi backend: `__rt_incref`, the `__rt_decref_any` free dispatcher, and
//! `__rt_str_persist` (copy a transient string into an owned heap block). These
//! sit on top of the linear-memory allocator (`heap`) and back the EIR ownership
//! ops (`Acquire` / `Release` / `Move` / `Borrow`).
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
//!   release is a free). Kinds 2..5 (array/hash/object/mixed) gain their per-kind
//!   deep-free branches in later phases; any other kind is a no-op today.
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
/// directly; kinds 2..5 (array/hash/object/mixed) get their deep-free branches in
/// later phases, and any other kind is a no-op. No-ops on non-heap pointers.
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
  (return))
"#;

/// `__rt_str_persist`: copies a string (data-segment literal or transient concat
/// buffer) into a fresh kind-1 heap block so it can be independently owned, and
/// returns the new `(ptr, len)`.
const RT_STR_PERSIST: &str = r#"(func $__rt_str_persist (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $n i32)
  (local $new i32)
  (local $i i32)
  (local.set $n (i32.wrap_i64 (local.get $len)))            ;; byte length
  (local.set $new (call $__rt_heap_alloc (local.get $n)))   ;; fresh heap block (8-byte minimum)
  (i64.store (i32.sub (local.get $new) (i32.const 8)) (i64.const 1)) ;; stamp header kind = 1 (string)
  (local.set $i (i32.const 0))
  (block $end (loop $copy
    (br_if $end (i32.ge_u (local.get $i) (local.get $n)))   ;; stop after n bytes
    (i32.store8
      (i32.add (local.get $new) (local.get $i))
      (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $copy)))
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
    use super::super::heap::emit_heap_runtime;
    use super::super::wat::WatModule;
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
}
