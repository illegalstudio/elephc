//! Purpose:
//! Emits the hand-authored WebAssembly (WAT) boxed-`Mixed` runtime for the
//! wasm32-wasi backend: `__rt_mixed_from_value` (box), `__rt_mixed_unbox`,
//! `__rt_mixed_free_deep`, and `__rt_decref_mixed`. A PHP `Mixed` value is a
//! refcounted heap cell that carries a runtime type tag plus a two-word payload.
//!
//! Called from:
//! - `crate::codegen_wasm::generate()` for every module, after the array runtime.
//!
//! Key details:
//! - A Mixed cell is a 24-byte heap block with kind low-byte 5. Relative to its
//!   pointer P: `[P+0 i64 tag][P+8 i64 lo][P+16 i64 hi]`. Tags: 0=int, 1=string
//!   (lo=zero-extended ptr, hi=len), 2=float (lo=f64 bits), 3=bool, 4=array,
//!   5=hash, 6=object, 7=nested-mixed, 8=null, 9=resource, 10=callable — matching
//!   the native runtime byte-for-byte.
//! - `from_value` normalizes ownership: a string is persisted into an owned heap
//!   copy; an array/hash/object/nested-mixed/callable child is increfed. `unbox`
//!   transparently unwraps nested (tag 7) cells. `free_deep` releases the owned
//!   child (if any) then frees the cell; `decref_mixed` is the kind-5 branch of
//!   `__rt_decref_any`.

use super::wat::WatModule;

/// Adds the boxed-Mixed runtime routines to `wm`. Emitted after the heap, refcount,
/// and array runtimes, whose `__rt_heap_alloc`/`__rt_heap_free`/`__rt_heap_free_safe`
/// /`__rt_incref`/`__rt_decref_any`/`__rt_str_persist` and heap globals it references.
pub(super) fn emit_mixed_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_MIXED_FROM_VALUE);
    wm.add_raw_func(RT_MIXED_UNBOX);
    wm.add_raw_func(RT_MIXED_FREE_DEEP);
    wm.add_raw_func(RT_DECREF_MIXED);
}

/// `__rt_mixed_from_value`: boxes a `(tag, lo, hi)` triple into a fresh 24-byte
/// Mixed cell, persisting a string payload and increfing a refcounted child so the
/// cell owns its contents.
const RT_MIXED_FROM_VALUE: &str = r#"(func $__rt_mixed_from_value (param $tag i64) (param $lo i64) (param $hi i64) (result i32)
  (local $cell i32)
  (local $sp i32)
  (local $sl i64)
  (local.set $cell (call $__rt_heap_alloc (i32.const 24)))              ;; 24-byte cell
  (i64.store (i32.sub (local.get $cell) (i32.const 8)) (i64.const 5))   ;; stamp kind = 5 (mixed)
  (if (i64.eq (local.get $tag) (i64.const 1))                           ;; string: own a persisted copy
    (then
      (call $__rt_str_persist (i32.wrap_i64 (local.get $lo)) (local.get $hi))
      (local.set $sl)                                                   ;; persisted length
      (local.set $sp)                                                   ;; persisted pointer
      (local.set $lo (i64.extend_i32_u (local.get $sp)))
      (local.set $hi (local.get $sl)))
    (else
      (if (i32.or (i32.or (i32.or (i32.or
            (i64.eq (local.get $tag) (i64.const 4))
            (i64.eq (local.get $tag) (i64.const 5)))
            (i64.eq (local.get $tag) (i64.const 6)))
            (i64.eq (local.get $tag) (i64.const 7)))
            (i64.eq (local.get $tag) (i64.const 10)))                   ;; refcounted child: share ownership
        (then (call $__rt_incref (i32.wrap_i64 (local.get $lo)))))))
  (i64.store (local.get $cell) (local.get $tag))                        ;; tag @ +0
  (i64.store (i32.add (local.get $cell) (i32.const 8)) (local.get $lo)) ;; lo @ +8
  (i64.store (i32.add (local.get $cell) (i32.const 16)) (local.get $hi)) ;; hi @ +16
  (local.get $cell))
"#;

/// `__rt_mixed_unbox`: returns the `(tag, lo, hi)` of a Mixed cell, transparently
/// unwrapping nested (tag 7) cells; a null pointer unboxes to the null tag (8).
const RT_MIXED_UNBOX: &str = r#"(func $__rt_mixed_unbox (param $ptr i32) (result i64) (result i64) (result i64)
  (local $tag i64)
  (if (i32.eqz (local.get $ptr))
    (then (return (i64.const 8) (i64.const 0) (i64.const 0))))          ;; null -> (null, 0, 0)
  (block $done
    (loop $L
      (local.set $tag (i64.load (local.get $ptr)))                      ;; current tag
      (br_if $done (i64.ne (local.get $tag) (i64.const 7)))             ;; not nested -> done
      (local.set $ptr (i32.wrap_i64 (i64.load (i32.add (local.get $ptr) (i32.const 8)))))  ;; follow nested pointer
      (br_if $done (i32.eqz (local.get $ptr)))                          ;; nested null -> done
      (br $L)))
  (i64.load (local.get $ptr))                                           ;; result 0: tag
  (i64.load (i32.add (local.get $ptr) (i32.const 8)))                   ;; result 1: lo
  (i64.load (i32.add (local.get $ptr) (i32.const 16))))                 ;; result 2: hi
"#;

/// `__rt_mixed_free_deep`: releases the cell's owned child (a persisted string via
/// `__rt_heap_free_safe`, or a refcounted container via `__rt_decref_any`) then
/// frees the cell itself.
const RT_MIXED_FREE_DEEP: &str = r#"(func $__rt_mixed_free_deep (param $ptr i32)
  (local $tag i64)
  (if (i32.eqz (local.get $ptr))
    (then (return)))
  (local.set $tag (i64.load (local.get $ptr)))                          ;; tag @ +0
  (if (i64.eq (local.get $tag) (i64.const 1))                           ;; string: free the owned copy
    (then
      (call $__rt_heap_free_safe (i32.wrap_i64 (i64.load (i32.add (local.get $ptr) (i32.const 8))))))
    (else
      (if (i32.or (i32.or (i32.or (i32.or
            (i64.eq (local.get $tag) (i64.const 4))
            (i64.eq (local.get $tag) (i64.const 5)))
            (i64.eq (local.get $tag) (i64.const 6)))
            (i64.eq (local.get $tag) (i64.const 7)))
            (i64.eq (local.get $tag) (i64.const 10)))                   ;; refcounted child: release it
        (then (call $__rt_decref_any (i32.wrap_i64 (i64.load (i32.add (local.get $ptr) (i32.const 8)))))))))
  (call $__rt_heap_free (local.get $ptr)))
"#;

/// `__rt_decref_mixed`: decrements a Mixed cell's refcount and deep-frees it when
/// the count reaches 0. The kind-5 branch of `__rt_decref_any`. No-ops on null or
/// non-heap pointers.
const RT_DECREF_MIXED: &str = r#"(func $__rt_decref_mixed (param $ptr i32)
  (local $rc i32)
  (if (i32.eqz (local.get $ptr))
    (then (return)))                                                    ;; null check
  (if (i32.lt_u (local.get $ptr) (i32.add (global.get $__heap_base) (i32.const 16)))
    (then (return)))                                                    ;; below heap
  (if (i32.ge_u (local.get $ptr) (global.get $__heap_ptr))
    (then (return)))                                                    ;; above heap
  (local.set $rc (i32.sub (i32.load (i32.sub (local.get $ptr) (i32.const 12))) (i32.const 1)))  ;; refcount - 1
  (i32.store (i32.sub (local.get $ptr) (i32.const 12)) (local.get $rc))
  (if (i32.eqz (local.get $rc))
    (then (call $__rt_mixed_free_deep (local.get $ptr)))))              ;; last owner -> deep free
"#;

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the WAT boxed-Mixed runtime, exercised end-to-end under
    //! `wasmer` via a hand-written driver and `--invoke`.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Each test builds a reactor module with the heap + refcount + array + mixed
    //!   runtime and a driver, validates it with `wasmparser`, and runs it under
    //!   `wasmer`. Runs skip silently when `wasmer` is absent.

    use super::emit_mixed_runtime;
    use super::super::arrays::emit_array_runtime;
    use super::super::heap::emit_heap_runtime;
    use super::super::refcount::emit_refcount_runtime;
    use super::super::wat::WatModule;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique temp directory path so concurrent wasmer runs never collide.
    fn unique_tmp_dir() -> std::path::PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("elephc_wasm_mixed_{}_{}", std::process::id(), n))
    }

    /// Returns whether the `wasmer` CLI is available.
    fn wasmer_available() -> bool {
        std::process::Command::new("wasmer")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Builds a 3-page reactor module with the heap + refcount + array + mixed
    /// runtime and `driver`, validates it, and runs `export` under `wasmer`.
    fn run_driver(driver: &str, export: &str) -> Option<String> {
        let mut wm = WatModule::new();
        wm.set_memory(3, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 3 * 65536);
        emit_refcount_runtime(&mut wm);
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm);
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

    /// Boxing an int (tag 0, value 42) then unboxing returns tag 0 and value 42.
    /// Driver returns `tag*1000 + lo`.
    #[test]
    fn box_int_unbox_roundtrips() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $c i32) (local $tag i64) (local $lo i64) (local $hi i64)
  (local.set $c (call $__rt_mixed_from_value (i64.const 0) (i64.const 42) (i64.const 0)))
  (call $__rt_mixed_unbox (local.get $c))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (i64.add (i64.mul (local.get $tag) (i64.const 1000)) (local.get $lo)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "42");
        }
    }

    /// Boxing a string (tag 1) persists its bytes; unboxing yields tag 1, a heap
    /// pointer, and the length. Driver writes "Hi" at 200, boxes it, unboxes, and
    /// returns `tag*1000000 + byte0*1000 + len` = 1*1000000 + 72*1000 + 2 = 1072002.
    #[test]
    fn box_string_persists_and_unboxes() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $c i32) (local $tag i64) (local $lo i64) (local $hi i64)
  (i32.store8 (i32.const 200) (i32.const 72))
  (i32.store8 (i32.const 201) (i32.const 105))
  (local.set $c (call $__rt_mixed_from_value (i64.const 1) (i64.extend_i32_u (i32.const 200)) (i64.const 2)))
  (call $__rt_mixed_unbox (local.get $c))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (i64.add
    (i64.add
      (i64.mul (local.get $tag) (i64.const 1000000))
      (i64.mul (i64.load8_u (i32.wrap_i64 (local.get $lo))) (i64.const 1000)))
    (local.get $hi)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1072002");
        }
    }

    /// Boxing then deep-freeing an int Mixed cell restores `_gc_live` to 0.
    #[test]
    fn decref_mixed_frees_int_cell() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $c i32)
  (local.set $c (call $__rt_mixed_from_value (i64.const 0) (i64.const 42) (i64.const 0)))
  (call $__rt_decref_mixed (local.get $c))
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }
}
