//! Purpose:
//! Emits the hand-authored WebAssembly (WAT) indexed-array runtime for the
//! wasm32-wasi backend: allocation, capacity growth, integer append, bounded
//! integer read, deep free, and the array branch of the refcount dispatcher.
//! Built on top of the linear-memory allocator (`heap`) and refcount layer
//! (`refcount`).
//!
//! Called from:
//! - `crate::codegen_wasm::generate()` for every module, after the refcount layer.
//!
//! Key details:
//! - An indexed-array value is a pointer `A` to a 24-byte in-payload header:
//!     A+0  i64 length, A+8 i64 capacity, A+16 i64 elem_size, then the slots at A+24.
//!   The block header (`A-16 size`, `A-12 refcount`, `A-8 kind`) precedes it. The
//!   kind word low byte is 2 (indexed array); bits 8..14 are the value_type tag;
//!   bit 15 is the COW flag. This is byte-identical to the native array layout.
//! - Scalar element slots are 8 bytes (one i64). String slots are 16 bytes; the
//!   pointer is a zero-extended i64 at slot+0 and the length an i64 at slot+8.
//! - `push`/`grow` may reallocate, so they RETURN the (possibly new) array
//!   pointer; the EIR `ArrayPush` lowering writes that back into the operand's
//!   value local and source slot, matching the native backend.
//! - `__rt_array_free_deep` releases string/container children via
//!   `__rt_decref_any` before freeing the struct; scalar arrays free directly.

use super::wat::WatModule;

/// Adds the indexed-array runtime routines to `wm`. Emitted after the heap and
/// refcount runtimes, whose `__rt_heap_alloc` / `__rt_heap_free` / `__rt_decref_any`
/// and heap globals these routines reference.
pub(super) fn emit_array_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_ARRAY_NEW);
    wm.add_raw_func(RT_ARRAY_GROW);
    wm.add_raw_func(RT_ARRAY_PUSH_INT);
    wm.add_raw_func(RT_ARRAY_PUSH_STR);
    wm.add_raw_func(RT_ARRAY_GET_INT);
    wm.add_raw_func(RT_ARRAY_GET_STR);
    wm.add_raw_func(RT_ARRAY_FREE_DEEP);
    wm.add_raw_func(RT_DECREF_ARRAY);
}

/// `__rt_array_new`: allocates an indexed array with `capacity` slots of
/// `elem_size` bytes, a zeroed length, and the indexed-array kind stamped.
const RT_ARRAY_NEW: &str = r#"(func $__rt_array_new (param $capacity i64) (param $elem_size i64) (result i32)
  (local $bytes i32)
  (local $arr i32)
  (local $kind i64)
  (local.set $bytes (i32.add (i32.const 24) (i32.wrap_i64 (i64.mul (local.get $capacity) (local.get $elem_size)))))  ;; 24B header + capacity*elem_size slots
  (local.set $arr (call $__rt_heap_alloc (local.get $bytes)))  ;; block: refcount=1, kind=0
  (local.set $kind (i64.const 2))                              ;; low byte = indexed-array kind
  (if (i64.eq (local.get $elem_size) (i64.const 16))
    (then (local.set $kind (i64.or (local.get $kind) (i64.const 256)))))  ;; 16B slots default to value_type 1 (string)
  (local.set $kind (i64.or (local.get $kind) (i64.const 32768)))  ;; COW flag (bit 15)
  (i64.store (i32.sub (local.get $arr) (i32.const 8)) (local.get $kind))  ;; stamp kind word at A-8
  (i64.store (local.get $arr) (i64.const 0))                   ;; length = 0
  (i64.store (i32.add (local.get $arr) (i32.const 8)) (local.get $capacity))    ;; capacity
  (i64.store (i32.add (local.get $arr) (i32.const 16)) (local.get $elem_size))  ;; elem_size
  (local.get $arr))
"#;

/// `__rt_array_grow`: allocates a double-capacity array (min 8), copies the live
/// payload bytes and metadata over, frees the old struct shallowly, and returns
/// the new array pointer.
const RT_ARRAY_GROW: &str = r#"(func $__rt_array_grow (param $array i32) (result i32)
  (local $len i64)
  (local $cap i64)
  (local $esz i64)
  (local $newcap i64)
  (local $new i32)
  (local $i i32)
  (local $nbytes i32)
  (local.set $len (i64.load (local.get $array)))             ;; length
  (local.set $cap (i64.load (i32.add (local.get $array) (i32.const 8))))   ;; capacity
  (local.set $esz (i64.load (i32.add (local.get $array) (i32.const 16))))  ;; elem_size
  (local.set $newcap (i64.shl (local.get $cap) (i64.const 1)))  ;; newcap = cap * 2
  (if (i64.lt_s (local.get $newcap) (i64.const 8))
    (then (local.set $newcap (i64.const 8))))                ;; minimum capacity 8
  (local.set $new (call $__rt_array_new (local.get $newcap) (local.get $esz)))  ;; fresh larger array
  (i64.store (i32.sub (local.get $new) (i32.const 8))
             (i64.and (i64.load (i32.sub (local.get $array) (i32.const 8))) (i64.const 65535)))  ;; preserve old value_type/COW (low 16 bits)
  (i64.store (local.get $new) (local.get $len))              ;; copy length (capacity/elem_size set by array_new)
  (local.set $nbytes (i32.wrap_i64 (i64.mul (local.get $len) (local.get $esz))))  ;; live payload bytes
  (local.set $i (i32.const 0))
  (block $end (loop $copy
    (br_if $end (i32.ge_u (local.get $i) (local.get $nbytes)))
    (i32.store8 (i32.add (i32.add (local.get $new) (i32.const 24)) (local.get $i))
                (i32.load8_u (i32.add (i32.add (local.get $array) (i32.const 24)) (local.get $i))))  ;; copy one byte
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $copy)))
  (call $__rt_heap_free (local.get $array))                  ;; free old struct shallowly (children were moved)
  (local.get $new))
"#;

/// `__rt_array_push_int`: appends an integer, shaping an empty array to 8-byte
/// scalar slots and growing capacity when full. Returns the (possibly new) array.
const RT_ARRAY_PUSH_INT: &str = r#"(func $__rt_array_push_int (param $array i32) (param $value i64) (result i32)
  (local $len i64)
  (local $cap i64)
  (local $slot i32)
  (if (i64.eqz (i64.load (local.get $array)))               ;; empty -> shape as a scalar array
    (then
      (i64.store (i32.add (local.get $array) (i32.const 16)) (i64.const 8))  ;; elem_size = 8
      (i64.store (i32.sub (local.get $array) (i32.const 8))
                 (i64.and (i64.load (i32.sub (local.get $array) (i32.const 8))) (i64.const -32513)))))  ;; clear value_type bits 8-14 (~0x7f00)
  (local.set $len (i64.load (local.get $array)))            ;; length
  (local.set $cap (i64.load (i32.add (local.get $array) (i32.const 8))))  ;; capacity
  (if (i64.ge_s (local.get $len) (local.get $cap))          ;; full -> grow
    (then (local.set $array (call $__rt_array_grow (local.get $array)))))
  (local.set $len (i64.load (local.get $array)))            ;; reload length (grow preserves it)
  (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $len) (i64.const 8)))))  ;; slot = A+24+len*8
  (i64.store (local.get $slot) (local.get $value))          ;; write element
  (i64.store (local.get $array) (i64.add (local.get $len) (i64.const 1)))  ;; length++
  (local.get $array))
"#;

/// `__rt_array_get_int`: reads the i64 element at `index`, returning the PHP null
/// sentinel (0x7fff_ffff_ffff_fffe) for a negative or out-of-bounds index. Used
/// for scalar (8-byte slot) arrays.
const RT_ARRAY_GET_INT: &str = r#"(func $__rt_array_get_int (param $array i32) (param $index i64) (result i64)
  (local $len i64)
  (if (i64.lt_s (local.get $index) (i64.const 0))           ;; negative index -> null
    (then (return (i64.const 9223372036854775806))))
  (local.set $len (i64.load (local.get $array)))            ;; length
  (if (i64.ge_s (local.get $index) (local.get $len))        ;; out of bounds -> null
    (then (return (i64.const 9223372036854775806))))
  (i64.load (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $index) (i64.const 8))))))  ;; slot[index]
"#;

/// `__rt_array_push_str`: appends a string element, shaping an empty array to
/// 16-byte string slots, persisting the (possibly transient) string into an owned
/// heap block, and growing capacity when full. Returns the (possibly new) array.
const RT_ARRAY_PUSH_STR: &str = r#"(func $__rt_array_push_str (param $array i32) (param $ptr i32) (param $len i64) (result i32)
  (local $alen i64)
  (local $cap i64)
  (local $slot i32)
  (local $newptr i32)
  (local $plen i64)
  (if (i64.eqz (i64.load (local.get $array)))             ;; empty -> shape as a string array
    (then
      (i64.store (i32.add (local.get $array) (i32.const 8))
                 (i64.div_u (i64.mul (i64.load (i32.add (local.get $array) (i32.const 8))) (i64.load (i32.add (local.get $array) (i32.const 16)))) (i64.const 16)))  ;; rescale capacity to 16-byte slots
      (i64.store (i32.add (local.get $array) (i32.const 16)) (i64.const 16))  ;; elem_size = 16
      (i64.store (i32.sub (local.get $array) (i32.const 8))
                 (i64.or (i64.and (i64.load (i32.sub (local.get $array) (i32.const 8))) (i64.const -32513)) (i64.const 256)))))  ;; value_type = 1 (string)
  (call $__rt_str_persist (local.get $ptr) (local.get $len))  ;; copy string into an owned heap block
  (local.set $plen)                                       ;; persisted length (top of stack)
  (local.set $newptr)                                     ;; persisted heap pointer
  (local.set $cap (i64.load (i32.add (local.get $array) (i32.const 8))))  ;; capacity
  (local.set $alen (i64.load (local.get $array)))         ;; length
  (if (i64.ge_u (local.get $alen) (local.get $cap))       ;; full -> grow
    (then (local.set $array (call $__rt_array_grow (local.get $array)))))
  (local.set $alen (i64.load (local.get $array)))         ;; reload length after grow
  (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $alen) (i64.const 16)))))  ;; slot = A+24+len*16
  (i64.store (local.get $slot) (i64.extend_i32_u (local.get $newptr)))     ;; pointer (zero-extended) at slot+0
  (i64.store (i32.add (local.get $slot) (i32.const 8)) (local.get $plen))  ;; length at slot+8
  (i64.store (local.get $array) (i64.add (local.get $alen) (i64.const 1))) ;; length++
  (local.get $array))
"#;

/// `__rt_array_get_str`: reads the (pointer, length) string element at `index`,
/// returning the null/empty pair (0, 0) for a negative or out-of-bounds index.
const RT_ARRAY_GET_STR: &str = r#"(func $__rt_array_get_str (param $array i32) (param $index i64) (result i32) (result i64)
  (local $len i64)
  (local $slot i32)
  (if (i64.lt_s (local.get $index) (i64.const 0))         ;; negative index -> null pair
    (then (return (i32.const 0) (i64.const 0))))
  (local.set $len (i64.load (local.get $array)))          ;; length
  (if (i64.ge_u (local.get $index) (local.get $len))      ;; out of bounds -> null pair
    (then (return (i32.const 0) (i64.const 0))))
  (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $index) (i64.const 16)))))  ;; slot = A+24+index*16
  (i32.wrap_i64 (i64.load (local.get $slot)))             ;; result 0: pointer (wrapped from i64)
  (i64.load (i32.add (local.get $slot) (i32.const 8))))   ;; result 1: length
"#;

/// `__rt_array_free_deep`: releases each string/container child (value_type 1 or
/// 4..7) via `__rt_decref_any`, then frees the array struct itself. Scalar arrays
/// skip the child loop.
const RT_ARRAY_FREE_DEEP: &str = r#"(func $__rt_array_free_deep (param $array i32)
  (local $vt i32)
  (local $len i64)
  (local $esz i64)
  (local $i i64)
  (local $slot i32)
  (if (i32.eqz (local.get $array))
    (then (return)))                                         ;; null check
  (local.set $vt (i32.and (i32.wrap_i64 (i64.shr_u (i64.load (i32.sub (local.get $array) (i32.const 8))) (i64.const 8))) (i32.const 127)))  ;; value_type
  (local.set $len (i64.load (local.get $array)))            ;; length
  (local.set $esz (i64.load (i32.add (local.get $array) (i32.const 16))))  ;; elem_size
  (if (i32.or (i32.eq (local.get $vt) (i32.const 1))
      (i32.or (i32.eq (local.get $vt) (i32.const 4))
      (i32.or (i32.eq (local.get $vt) (i32.const 5))
      (i32.or (i32.eq (local.get $vt) (i32.const 6))
              (i32.eq (local.get $vt) (i32.const 7))))))    ;; string or container elements own children
    (then
      (local.set $i (i64.const 0))
      (block $end (loop $rel
        (br_if $end (i64.ge_s (local.get $i) (local.get $len)))
        (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24))
                                  (i32.wrap_i64 (i64.mul (local.get $i) (local.get $esz)))))  ;; slot base
        (call $__rt_decref_any (i32.wrap_i64 (i64.load (local.get $slot))))  ;; release the child by kind
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $rel)))))
  (call $__rt_heap_free (local.get $array)))
"#;

/// `__rt_decref_array`: decrements an indexed array's refcount and deep-frees it
/// when the count reaches 0. No-ops on null or non-heap pointers. This is the
/// kind-2 branch of `__rt_decref_any`.
const RT_DECREF_ARRAY: &str = r#"(func $__rt_decref_array (param $array i32)
  (local $rc i32)
  (if (i32.eqz (local.get $array))
    (then (return)))                                         ;; null check
  (if (i32.lt_u (local.get $array) (i32.add (global.get $__heap_base) (i32.const 16)))
    (then (return)))                                         ;; below heap
  (if (i32.ge_u (local.get $array) (global.get $__heap_ptr))
    (then (return)))                                         ;; above heap
  (local.set $rc (i32.sub (i32.load (i32.sub (local.get $array) (i32.const 12))) (i32.const 1)))  ;; refcount - 1
  (i32.store (i32.sub (local.get $array) (i32.const 12)) (local.get $rc))  ;; store decremented refcount
  (if (i32.eqz (local.get $rc))
    (then (call $__rt_array_free_deep (local.get $array)))))  ;; last owner -> deep free
"#;

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the WAT indexed-array runtime, exercised end-to-end under
    //! `wasmer` via a hand-written driver function and `--invoke`.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Each test builds a reactor module with the heap + refcount + array
    //!   runtime and one exported driver, validates it with `wasmparser`, and runs
    //!   it under `wasmer`. Runs skip silently when `wasmer` is absent.

    use super::emit_array_runtime;
    use super::super::heap::emit_heap_runtime;
    use super::super::refcount::emit_refcount_runtime;
    use super::super::wat::WatModule;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique temp directory path so concurrent wasmer runs never collide.
    fn unique_tmp_dir() -> std::path::PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("elephc_wasm_arr_{}_{}", std::process::id(), n))
    }

    /// Returns whether the `wasmer` CLI is available.
    fn wasmer_available() -> bool {
        std::process::Command::new("wasmer")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Builds a 3-page reactor module with the heap + refcount + array runtime and
    /// `driver`, validates it, and runs `export` under `wasmer`, returning trimmed
    /// stdout. `None` if wasmer is absent; validation always runs.
    fn run_driver(driver: &str, export: &str) -> Option<String> {
        let mut wm = WatModule::new();
        wm.set_memory(3, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 3 * 65536);
        emit_refcount_runtime(&mut wm);
        emit_array_runtime(&mut wm);
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

    /// Building [10,20,30] then reading index 1 returns 20, and the length is 3.
    #[test]
    fn push_and_get_int_roundtrips() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 10)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 20)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 30)))
  (call $__rt_array_get_int (local.get $a) (i64.const 1)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "20");
        }
    }

    /// Pushing past the initial capacity triggers growth; the last element is
    /// still readable (validates `__rt_array_grow` + the realloc'd pointer).
    #[test]
    fn push_beyond_capacity_grows() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local $i i64)
  (local.set $a (call $__rt_array_new (i64.const 2) (i64.const 16)))
  (local.set $i (i64.const 0))
  (block $end (loop $push
    (br_if $end (i64.ge_s (local.get $i) (i64.const 5)))
    (local.set $a (call $__rt_array_push_int (local.get $a) (i64.add (i64.const 100) (local.get $i))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $push)))
  (call $__rt_array_get_int (local.get $a) (i64.const 4)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "104");
        }
    }

    /// Array length is the i64 at A+0; after three pushes it is 3.
    #[test]
    fn length_reflects_pushes() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 7)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 8)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 9)))
  (i64.load (local.get $a)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "3");
        }
    }

    /// A read past the end returns the PHP null sentinel (0x7fff_ffff_ffff_fffe).
    #[test]
    fn out_of_bounds_get_returns_null_sentinel() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 1)))
  (call $__rt_array_get_int (local.get $a) (i64.const 9)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "9223372036854775806");
        }
    }

    /// `__rt_decref_array` on a sole owner deep-frees the array, restoring
    /// `_gc_live` to 0 (scalar array: no children, struct freed).
    #[test]
    fn decref_array_frees_and_balances_live() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 42)))
  (call $__rt_decref_array (local.get $a))
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }

    /// Pushing the bytes "abc" then reading element 0 returns a heap copy whose
    /// three bytes pack to `97<<16 | 98<<8 | 99 = 6382179`, proving `push_str`
    /// persists and `get_str` returns the right pointer.
    #[test]
    fn push_str_get_str_copies_bytes() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $a i32) (local $p i32) (local $l i64)
  (i32.store8 (i32.const 200) (i32.const 97))
  (i32.store8 (i32.const 201) (i32.const 98))
  (i32.store8 (i32.const 202) (i32.const 99))
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_str (local.get $a) (i32.const 200) (i64.const 3)))
  (call $__rt_array_get_str (local.get $a) (i64.const 0))
  (local.set $l)
  (local.set $p)
  (i32.add
    (i32.add
      (i32.mul (i32.load8_u (local.get $p)) (i32.const 65536))
      (i32.mul (i32.load8_u (i32.add (local.get $p) (i32.const 1))) (i32.const 256)))
    (i32.load8_u (i32.add (local.get $p) (i32.const 2)))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "6382179");
        }
    }

    /// `get_str` returns the stored length (3 for "abc").
    #[test]
    fn get_str_returns_length() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $p i32) (local $l i64)
  (i32.store8 (i32.const 200) (i32.const 97))
  (i32.store8 (i32.const 201) (i32.const 98))
  (i32.store8 (i32.const 202) (i32.const 99))
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_str (local.get $a) (i32.const 200) (i64.const 3)))
  (call $__rt_array_get_str (local.get $a) (i64.const 0))
  (local.set $l)
  (drop)
  (local.get $l))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "3");
        }
    }
}
