//! Purpose:
//! Emits the hand-authored WebAssembly (WAT) associative-array (hash) runtime for
//! the wasm32-wasi backend. PHP arrays are *ordered* maps: this layer owns the
//! hash-table allocation, the key hashing/equality primitives, and the teardown
//! (deep free + refcount dispatch). The element operations (get/set), copy-on-write,
//! iteration, and append/unset are layered on top in later sub-phases.
//!
//! Called from:
//! - `crate::codegen_wasm::generate()` for every module, after the indexed-array
//!   runtime (this layer's `__rt_hash_free_deep` calls `__rt_decref_any`, and
//!   `__rt_decref_any` routes hash kinds back here via `__rt_decref_hash`).
//!
//! Key details:
//! - A hash value is a pointer `P`. The 16-byte block header precedes it
//!   (`P-16 size`, `P-12 refcount`, `P-8 kind`); the kind word low byte is 3 and
//!   bit 15 is the COW flag (so a fresh hash kind word is `3 | 0x8000 = 32771`).
//! - The 40-byte hash header at `P` is five i64s: count, capacity, value_type,
//!   head, tail. `head`/`tail` are insertion-order slot indices (-1 when empty).
//! - Entries are 64 bytes each, slot `i` at `P + 40 + i*64`:
//!     +0 occupied (0 empty / 1 live / 2 tombstone), +8 key_lo, +16 key_hi
//!     (-1 = int-key sentinel, else string length), +24 value_lo, +32 value_hi,
//!     +40 value_tag, +48 prev, +56 next (prev/next are the insertion-order list).
//! - Int keys hash with a Knuth multiplicative mix; string keys with FNV-1a. This
//!   layout and hashing are byte-identical to the native hash runtime.

use super::wat::WatModule;

/// Adds the hash-table helper/teardown runtime to `wm`: hashing, key equality,
/// allocation, deep free, and the refcount-dispatcher's hash branch. Emitted after
/// the heap, refcount, array, and mixed runtimes.
pub(super) fn emit_hash_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_HASH_FNV1A);
    wm.add_raw_func(RT_HASH_KEY_HASH);
    wm.add_raw_func(RT_HASH_KEY_EQ);
    wm.add_raw_func(RT_HASH_NEW);
    wm.add_raw_func(RT_HASH_FREE_DEEP);
    wm.add_raw_func(RT_DECREF_HASH);
}

/// `__rt_hash_fnv1a`: FNV-1a 64-bit hash of the `len` bytes at `ptr`. Each byte is
/// XORed into the accumulator then the accumulator is multiplied by the FNV prime
/// (wrapping). Empty input returns the offset basis.
const RT_HASH_FNV1A: &str = r#"(func $__rt_hash_fnv1a (param $ptr i32) (param $len i64) (result i64)
  (local $hash i64)
  (local $i i64)
  (local.set $hash (i64.const 0xcbf29ce484222325))          ;; FNV offset basis
  (local.set $i (i64.const 0))
  (block $end (loop $byte
    (br_if $end (i64.ge_u (local.get $i) (local.get $len)))  ;; consumed all bytes
    (local.set $hash (i64.xor (local.get $hash)
      (i64.extend_i32_u (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))))  ;; hash ^= byte
    (local.set $hash (i64.mul (local.get $hash) (i64.const 0x100000001b3)))  ;; hash *= FNV prime (wraps)
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $byte)))
  (local.get $hash))
"#;

/// `__rt_hash_key_hash`: hashes a materialized key `(key_lo, key_hi)`. An integer
/// key (`key_hi == -1`) gets a Knuth multiplicative mix of `key_lo`; a string key
/// (`key_lo` = pointer, `key_hi` = length) is hashed with FNV-1a.
const RT_HASH_KEY_HASH: &str = r#"(func $__rt_hash_key_hash (param $key_lo i64) (param $key_hi i64) (result i64)
  (local $h i64)
  (if (i64.eq (local.get $key_hi) (i64.const -1))           ;; integer key?
    (then
      (local.set $h (local.get $key_lo))                    ;; h = key_lo
      (local.set $h (i64.xor (local.get $h) (i64.shr_u (local.get $h) (i64.const 33))))  ;; h ^= h >> 33 (logical)
      (return (i64.mul (local.get $h) (i64.const 0x9e3779b97f4a7c15)))))  ;; h *= Knuth constant
  (call $__rt_hash_fnv1a (i32.wrap_i64 (local.get $key_lo)) (local.get $key_hi)))  ;; string key -> FNV-1a
"#;

/// `__rt_hash_key_eq`: returns 1 if two materialized keys are equal, else 0. Two
/// integer keys compare by value; an integer key never equals a string key; two
/// string keys compare length then bytes.
const RT_HASH_KEY_EQ: &str = r#"(func $__rt_hash_key_eq (param $l_lo i64) (param $l_hi i64) (param $r_lo i64) (param $r_hi i64) (result i32)
  (local $i i64)
  (if (i64.eq (local.get $l_hi) (i64.const -1))             ;; left is an int key?
    (then
      (if (i64.eq (local.get $r_hi) (i64.const -1))         ;; right is an int key?
        (then (return (i64.eq (local.get $l_lo) (local.get $r_lo))))  ;; both int -> compare values
        (else (return (i32.const 0))))))                    ;; int vs string -> unequal
  (if (i64.eq (local.get $r_hi) (i64.const -1))             ;; left string, right int?
    (then (return (i32.const 0))))                          ;; string vs int -> unequal
  (if (i64.ne (local.get $l_hi) (local.get $r_hi))          ;; both string: different lengths?
    (then (return (i32.const 0))))
  (local.set $i (i64.const 0))
  (block $end (loop $byte
    (br_if $end (i64.ge_u (local.get $i) (local.get $l_hi)))  ;; compared all bytes -> equal
    (if (i32.ne
          (i32.load8_u (i32.add (i32.wrap_i64 (local.get $l_lo)) (i32.wrap_i64 (local.get $i))))
          (i32.load8_u (i32.add (i32.wrap_i64 (local.get $r_lo)) (i32.wrap_i64 (local.get $i)))))  ;; bytes differ?
      (then (return (i32.const 0))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $byte)))
  (i32.const 1))
"#;

/// `__rt_hash_new`: allocates an empty hash with `capacity` entry slots and a
/// default `value_tag`. Stamps the hash kind word, initializes the header (empty
/// insertion-order list), and zeroes every slot's `occupied` field (heap memory
/// may be dirty from reuse).
const RT_HASH_NEW: &str = r#"(func $__rt_hash_new (param $capacity i64) (param $value_tag i64) (result i32)
  (local $bytes i32)
  (local $p i32)
  (local $i i64)
  (local.set $bytes (i32.add (i32.const 40) (i32.wrap_i64 (i64.mul (local.get $capacity) (i64.const 64)))))  ;; 40B header + capacity*64 slots
  (local.set $p (call $__rt_heap_alloc (local.get $bytes)))  ;; block: refcount=1
  (i64.store (i32.sub (local.get $p) (i32.const 8)) (i64.const 32771))  ;; kind = hash(3) | COW(0x8000)
  (i64.store (local.get $p) (i64.const 0))                   ;; count = 0
  (i64.store (i32.add (local.get $p) (i32.const 8)) (local.get $capacity))   ;; capacity
  (i64.store (i32.add (local.get $p) (i32.const 16)) (local.get $value_tag)) ;; value_type
  (i64.store (i32.add (local.get $p) (i32.const 24)) (i64.const -1))  ;; head = -1 (empty)
  (i64.store (i32.add (local.get $p) (i32.const 32)) (i64.const -1))  ;; tail = -1 (empty)
  (local.set $i (i64.const 0))
  (block $end (loop $slot
    (br_if $end (i64.ge_u (local.get $i) (local.get $capacity)))  ;; zeroed every slot
    (i64.store
      (i32.add (i32.add (local.get $p) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 64))))  ;; &slot[i].occupied = P+40+i*64
      (i64.const 0))                                         ;; occupied = empty
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $slot)))
  (local.get $p))
"#;

/// `__rt_hash_free_deep`: releases the children of every live entry (string keys,
/// string values, and refcounted container values), then frees the block. Walks
/// all slots; tombstones and empty slots own nothing.
const RT_HASH_FREE_DEEP: &str = r#"(func $__rt_hash_free_deep (param $hash i32)
  (local $capacity i64)
  (local $i i64)
  (local $entry i32)
  (local $vtag i64)
  (if (i32.eqz (local.get $hash))
    (then (return)))                                         ;; null check
  (local.set $capacity (i64.load (i32.add (local.get $hash) (i32.const 8))))  ;; capacity
  (local.set $i (i64.const 0))
  (block $end (loop $slot
    (br_if $end (i64.ge_u (local.get $i) (local.get $capacity)))  ;; visited every slot
    (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 64)))))  ;; &slot[i]
    (if (i64.eq (i64.load (local.get $entry)) (i64.const 1)) ;; live entry?
      (then
        (if (i64.ge_s (i64.load (i32.add (local.get $entry) (i32.const 16))) (i64.const 0))  ;; key_hi >= 0 -> string key
          (then (call $__rt_heap_free_safe (i32.wrap_i64 (i64.load (i32.add (local.get $entry) (i32.const 8)))))))  ;; free key string
        (local.set $vtag (i64.load (i32.add (local.get $entry) (i32.const 40))))  ;; value_tag
        (if (i64.eq (local.get $vtag) (i64.const 1))         ;; string value
          (then (call $__rt_heap_free_safe (i32.wrap_i64 (i64.load (i32.add (local.get $entry) (i32.const 24))))))  ;; free value string
          (else
            (if (i32.or (i32.or (i64.eq (local.get $vtag) (i64.const 4)) (i64.eq (local.get $vtag) (i64.const 5)))
                (i32.or (i64.eq (local.get $vtag) (i64.const 6)) (i64.eq (local.get $vtag) (i64.const 7))))  ;; array/hash/object/mixed value (i64.eq yields i32 -> combine with i32.or)
              (then (call $__rt_decref_any (i32.wrap_i64 (i64.load (i32.add (local.get $entry) (i32.const 24)))))))))))  ;; release child
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $slot)))
  (call $__rt_heap_free (local.get $hash)))                  ;; free the struct
"#;

/// `__rt_decref_hash`: decrements a hash's refcount and deep-frees it at zero.
/// No-ops on null or non-heap pointers. This is the kind-3 branch of
/// `__rt_decref_any`.
const RT_DECREF_HASH: &str = r#"(func $__rt_decref_hash (param $hash i32)
  (local $rc i32)
  (if (i32.eqz (local.get $hash))
    (then (return)))                                         ;; null check
  (if (i32.lt_u (local.get $hash) (i32.add (global.get $__heap_base) (i32.const 16)))
    (then (return)))                                         ;; below heap
  (if (i32.ge_u (local.get $hash) (global.get $__heap_ptr))
    (then (return)))                                         ;; above heap
  (local.set $rc (i32.sub (i32.load (i32.sub (local.get $hash) (i32.const 12))) (i32.const 1)))  ;; refcount - 1
  (i32.store (i32.sub (local.get $hash) (i32.const 12)) (local.get $rc))  ;; store decremented refcount
  (if (i32.eqz (local.get $rc))
    (then (call $__rt_hash_free_deep (local.get $hash)))))   ;; last owner -> deep free
"#;

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the WAT hash helper/teardown runtime, exercised end-to-end
    //! under `wasmer` via a hand-written driver function and `--invoke`.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Each test builds a reactor module with the heap + refcount + array + mixed
    //!   + hash runtimes and one exported driver, validates it with `wasmparser`,
    //!   and runs it under `wasmer`. Runs skip silently when `wasmer` is absent.

    use super::emit_hash_runtime;
    use super::super::arrays::emit_array_runtime;
    use super::super::heap::emit_heap_runtime;
    use super::super::mixed::emit_mixed_runtime;
    use super::super::refcount::emit_refcount_runtime;
    use super::super::wat::WatModule;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique temp directory path so concurrent wasmer runs never collide.
    fn unique_tmp_dir() -> std::path::PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("elephc_wasm_hash_{}_{}", std::process::id(), n))
    }

    /// Returns whether the `wasmer` CLI is available.
    fn wasmer_available() -> bool {
        std::process::Command::new("wasmer")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Builds a 4-page reactor module with the heap + refcount + array + mixed +
    /// hash runtimes and `driver`, validates it, and runs `export` under `wasmer`,
    /// returning trimmed stdout. `None` if wasmer is absent; validation always runs.
    fn run_driver(driver: &str, export: &str) -> Option<String> {
        let mut wm = WatModule::new();
        wm.set_memory(4, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 4 * 65536);
        emit_refcount_runtime(&mut wm);
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm);
        emit_hash_runtime(&mut wm);
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

    /// A fresh hash has count 0, the requested capacity, an empty insertion-order
    /// list (head/tail = -1), and a zeroed first `occupied` slot. Returns
    /// `count + capacity*10 + (head==-1)*100 + (occupied0==0)*1000` = 0+80+100+1000.
    #[test]
    fn hash_new_initializes_header_and_slots() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 7)))
  (i64.add (i64.add (i64.add
    (i64.load (local.get $h))
    (i64.mul (i64.load (i32.add (local.get $h) (i32.const 8))) (i64.const 10)))
    (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $h) (i32.const 24))) (i64.const -1))) (i64.const 100)))
    (i64.mul (i64.extend_i32_u (i64.eqz (i64.load (i32.add (local.get $h) (i32.const 40))))) (i64.const 1000))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1180");
        }
    }

    /// FNV-1a is deterministic and content-sensitive: `hash("ab")==hash("ab")` and
    /// `hash("ab")!=hash("ac")`. Returns `same*10 + differ` = 11.
    #[test]
    fn fnv1a_deterministic_and_sensitive() {
        let driver = r#"(func $t (export "t") (result i32)
  (i32.store8 (i32.const 300) (i32.const 97))
  (i32.store8 (i32.const 301) (i32.const 98))
  (i32.store8 (i32.const 302) (i32.const 99))
  (i32.add
    (i32.mul (i64.eq
      (call $__rt_hash_fnv1a (i32.const 300) (i64.const 2))
      (call $__rt_hash_fnv1a (i32.const 300) (i64.const 2))) (i32.const 10))
    (i64.ne
      (call $__rt_hash_fnv1a (i32.const 300) (i64.const 2))
      (call $__rt_hash_fnv1a (i32.const 301) (i64.const 2)))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "11");
        }
    }

    /// `__rt_hash_key_eq`: int 5 == int 5 (1), int 5 != int 6 (0), int vs string (0),
    /// "abc" == "abc" from distinct buffers (1), "abc" != "abd" (0). Packs the five
    /// results as a base-2 bitfield = 1*16 + 0*8 + 0*4 + 1*2 + 0 = 18.
    #[test]
    fn key_eq_int_and_string_cases() {
        let driver = r#"(func $t (export "t") (result i32)
  (i32.store8 (i32.const 300) (i32.const 97))
  (i32.store8 (i32.const 301) (i32.const 98))
  (i32.store8 (i32.const 302) (i32.const 99))
  (i32.store8 (i32.const 310) (i32.const 97))
  (i32.store8 (i32.const 311) (i32.const 98))
  (i32.store8 (i32.const 312) (i32.const 99))
  (i32.store8 (i32.const 320) (i32.const 97))
  (i32.store8 (i32.const 321) (i32.const 98))
  (i32.store8 (i32.const 322) (i32.const 100))
  (i32.add (i32.add (i32.add (i32.add
    (i32.mul (call $__rt_hash_key_eq (i64.const 5) (i64.const -1) (i64.const 5) (i64.const -1)) (i32.const 16))
    (i32.mul (call $__rt_hash_key_eq (i64.const 5) (i64.const -1) (i64.const 6) (i64.const -1)) (i32.const 8)))
    (i32.mul (call $__rt_hash_key_eq (i64.const 5) (i64.const -1) (i64.const 300) (i64.const 3)) (i32.const 4)))
    (i32.mul (call $__rt_hash_key_eq (i64.const 300) (i64.const 3) (i64.const 310) (i64.const 3)) (i32.const 2)))
    (call $__rt_hash_key_eq (i64.const 300) (i64.const 3) (i64.const 320) (i64.const 3))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "18");
        }
    }

    /// `__rt_decref_hash` on a sole owner deep-frees the (empty) hash, restoring
    /// `_gc_live` to 0.
    #[test]
    fn decref_hash_frees_and_balances_live() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 7)))
  (call $__rt_decref_hash (local.get $h))
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }
}
