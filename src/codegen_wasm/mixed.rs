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
//! - `cast_bool` applies PHP `(bool)` truthiness to a boxed value, mirroring the
//!   native `__rt_mixed_cast_bool` tag dispatch exactly. It needs no float
//!   formatting, so unlike the int/float/string casts (which depend on the
//!   deferred `strtod`/`%.14G` ftoa) it is fully PHP-correct on wasm32-wasi.

use super::wat::WatModule;

/// Adds the boxed-Mixed runtime routines to `wm`. Emitted after the heap, refcount,
/// and array runtimes, whose `__rt_heap_alloc`/`__rt_heap_free`/`__rt_heap_free_safe`
/// /`__rt_incref`/`__rt_decref_any`/`__rt_str_persist` and heap globals it references.
pub(super) fn emit_mixed_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_MIXED_FROM_VALUE);
    wm.add_raw_func(RT_MIXED_UNBOX);
    wm.add_raw_func(RT_MIXED_FREE_DEEP);
    wm.add_raw_func(RT_DECREF_MIXED);
    wm.add_raw_func(RT_MIXED_CAST_BOOL);
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

/// `__rt_mixed_cast_bool`: casts a boxed Mixed cell to a PHP boolean (0 or 1)
/// following PHP `(bool)` truthiness, mirroring the native `__rt_mixed_cast_bool`
/// tag dispatch byte-for-byte. Unboxes via `__rt_mixed_unbox` (which already
/// unwraps nested tag-7 cells and maps null to tag 8), then per tag: int/float are
/// truthy when non-zero (float via an integer bit test, so NaN is truthy and ±0.0
/// is falsy); a string is falsy only when "" or the single byte "0"; a bool returns
/// its stored 0/1; an array/hash is truthy when non-null and its count (i64 at
/// offset 0) is non-zero; an object or resource is always truthy; null, callable,
/// and any other tag are falsy. Borrows the cell (never frees/mutates it).
const RT_MIXED_CAST_BOOL: &str = r#"(func $__rt_mixed_cast_bool (param $ptr i32) (result i64)
  (local $tag i64)
  (local $lo i64)
  (local $hi i64)
  (local $cp i32)
  (call $__rt_mixed_unbox (local.get $ptr))                            ;; unbox -> stack: tag, lo, hi
  (local.set $hi)                                                      ;; pop value high word
  (local.set $lo)                                                      ;; pop value low word
  (local.set $tag)                                                     ;; pop runtime tag
  (if (i64.eqz (local.get $tag))                                       ;; tag 0 = int
    (then (return (i64.extend_i32_u (i64.ne (local.get $lo) (i64.const 0))))))  ;; truthy if non-zero
  (if (i64.eq (local.get $tag) (i64.const 1))                          ;; tag 1 = string
    (then (return (i64.extend_i32_u
      (i32.and
        (i64.ne (local.get $hi) (i64.const 0))                        ;; non-empty (len != 0)
        (i32.or
          (i64.ne (local.get $hi) (i64.const 1))                      ;; length != 1, OR
          (i32.ne (i32.load8_u (i32.wrap_i64 (local.get $lo))) (i32.const 48))))))))  ;; first byte != '0'
  (if (i64.eq (local.get $tag) (i64.const 2))                          ;; tag 2 = float (integer bit test)
    (then (return (i64.extend_i32_u
      (i32.and
        (i64.ne (local.get $lo) (i64.const 0))                        ;; bits != +0.0
        (i64.ne (local.get $lo) (i64.const 0x8000000000000000)))))))  ;; bits != -0.0
  (if (i64.eq (local.get $tag) (i64.const 3))                          ;; tag 3 = bool
    (then (return (local.get $lo))))                                   ;; already normalized 0/1
  (if (i32.or (i64.eq (local.get $tag) (i64.const 4)) (i64.eq (local.get $tag) (i64.const 5)))  ;; tag 4/5 = array/hash
    (then
      (local.set $cp (i32.wrap_i64 (local.get $lo)))                  ;; container pointer
      (return (i64.extend_i32_u
        (i32.and
          (i32.ne (local.get $cp) (i32.const 0))                      ;; non-null container
          (i64.ne (i64.load (local.get $cp)) (i64.const 0)))))))      ;; element count != 0
  (if (i32.or (i64.eq (local.get $tag) (i64.const 6)) (i64.eq (local.get $tag) (i64.const 9)))  ;; tag 6 object / 9 resource
    (then (return (i64.const 1))))                                    ;; always truthy
  (i64.const 0))                                                      ;; tag 8 null / 10 callable / other = false
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
        super::super::hashes::emit_hash_runtime(&mut wm);
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

    /// `__rt_mixed_cast_bool` on int cells: a non-zero int is truthy (1), zero is
    /// falsy (0). Driver returns `truthy*10 + falsy` = 10.
    #[test]
    fn cast_bool_int_zero_falsy_nonzero_truthy() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $tr i64) (local $fa i64)
  (local.set $tr (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 0) (i64.const 5) (i64.const 0))))
  (local.set $fa (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 0) (i64.const 0) (i64.const 0))))
  (i64.add (i64.mul (local.get $tr) (i64.const 10)) (local.get $fa)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "10");
        }
    }

    /// `__rt_mixed_cast_bool` on float cells (PHP truthiness via integer bit test):
    /// 1.5 truthy, +0.0 falsy, -0.0 falsy, NaN truthy. Driver returns
    /// `b(1.5)*1000 + b(+0.0)*100 + b(-0.0)*10 + b(NaN)` = 1001.
    #[test]
    fn cast_bool_float_zero_falsy_nan_truthy() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i64) (local $b i64) (local $c i64) (local $d i64)
  (local.set $a (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 2) (i64.const 0x3FF8000000000000) (i64.const 0))))
  (local.set $b (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 2) (i64.const 0) (i64.const 0))))
  (local.set $c (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 2) (i64.const 0x8000000000000000) (i64.const 0))))
  (local.set $d (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 2) (i64.const 0x7FF8000000000000) (i64.const 0))))
  (i64.add
    (i64.add (i64.mul (local.get $a) (i64.const 1000)) (i64.mul (local.get $b) (i64.const 100)))
    (i64.add (i64.mul (local.get $c) (i64.const 10)) (local.get $d))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1001");
        }
    }

    /// `__rt_mixed_cast_bool` on string cells: "00" truthy, the single byte "0"
    /// falsy, "" falsy — matching PHP's `(bool)` string rule. The bytes are written
    /// into scratch then boxed (persisted) as tag-1 cells. Driver returns
    /// `b("00")*100 + b("0")*10 + b("")` = 100.
    #[test]
    fn cast_bool_string_empty_and_zero_falsy() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i64) (local $b i64) (local $c i64)
  (i32.store8 (i32.const 300) (i32.const 48))
  (i32.store8 (i32.const 301) (i32.const 48))
  (local.set $a (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 1) (i64.extend_i32_u (i32.const 300)) (i64.const 2))))
  (local.set $b (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 1) (i64.extend_i32_u (i32.const 300)) (i64.const 1))))
  (local.set $c (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 1) (i64.extend_i32_u (i32.const 300)) (i64.const 0))))
  (i64.add (i64.add (i64.mul (local.get $a) (i64.const 100)) (i64.mul (local.get $b) (i64.const 10))) (local.get $c)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "100");
        }
    }

    /// `__rt_mixed_cast_bool` on container cells (tag 4 array / tag 5 hash): truthy
    /// when the element count (i64 at offset 0) is non-zero, falsy when empty. Two
    /// heap blocks stand in for containers with counts 3 and 0. Driver returns
    /// `truthy*10 + falsy` = 10.
    #[test]
    fn cast_bool_container_nonempty_truthy_empty_falsy() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a1 i32) (local $a2 i32) (local $tr i64) (local $fa i64)
  (local.set $a1 (call $__rt_heap_alloc (i32.const 8)))
  (i64.store (local.get $a1) (i64.const 3))
  (local.set $a2 (call $__rt_heap_alloc (i32.const 8)))
  (i64.store (local.get $a2) (i64.const 0))
  (local.set $tr (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 4) (i64.extend_i32_u (local.get $a1)) (i64.const 0))))
  (local.set $fa (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 5) (i64.extend_i32_u (local.get $a2)) (i64.const 0))))
  (i64.add (i64.mul (local.get $tr) (i64.const 10)) (local.get $fa)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "10");
        }
    }

    /// `__rt_mixed_cast_bool` on object (tag 6), resource (tag 9), and null (tag 8)
    /// cells: objects and resources are always truthy, null is falsy. Driver returns
    /// `b(object)*100 + b(resource)*10 + b(null)` = 110.
    #[test]
    fn cast_bool_object_resource_truthy_null_falsy() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $o i64) (local $r i64) (local $n i64)
  (local.set $o (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 6) (i64.const 0) (i64.const 0))))
  (local.set $r (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 9) (i64.const 7) (i64.const 0))))
  (local.set $n (call $__rt_mixed_cast_bool (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0))))
  (i64.add (i64.add (i64.mul (local.get $o) (i64.const 100)) (i64.mul (local.get $r) (i64.const 10))) (local.get $n)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "110");
        }
    }
}
