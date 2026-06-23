//! Purpose:
//! Emits the hand-authored WebAssembly (WAT) associative-array (hash) runtime for
//! the wasm32-wasi backend. PHP arrays are *ordered* maps: this layer owns the
//! hash-table allocation, the key hashing/equality primitives, and the teardown
//! (deep free + refcount dispatch). The element operations (get/set), copy-on-write,
//! iteration, auto-indexed append (`$h[] = v`), and the array-union operator (`$a + $b`)
//! are layered on top. Element `unset($h[$k])` is a separate frontend gap (the EIR
//! lowering never emits a hash-unset op for non-object receivers) and is not handled here.
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
    wm.add_raw_func(RT_HASH_NORMALIZE_KEY);
    wm.add_raw_func(RT_HASH_KEY_FROM_MIXED);
    wm.add_raw_func(RT_HASH_ITER_NEXT);
    wm.add_raw_func(RT_HASH_NEW);
    wm.add_raw_func(RT_HASH_GET);
    wm.add_raw_func(RT_HASH_INSERT_OWNED);
    wm.add_raw_func(RT_HASH_RESIZE);
    wm.add_raw_func(RT_HASH_CLONE_SHALLOW);
    wm.add_raw_func(RT_HASH_ENSURE_UNIQUE);
    wm.add_raw_func(RT_HASH_SET);
    wm.add_raw_func(RT_HASH_APPEND);
    wm.add_raw_func(RT_HASH_UNION);
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

/// `__rt_hash_normalize_key`: classifies a string array key. Returns `(key_lo, key_hi)`
/// = `(int_value, -1)` when the string is a canonical PHP integer (round-trips through
/// `(string)(int)$s === $s`: optional leading `-`, no leading zeros except a lone "0",
/// no "-0", magnitude within i64), and the string fallback `(extend_i32_u(ptr), len)`
/// otherwise. Mirrors PHP's `_zend_handle_numeric_str_ex`. Overflow is detected per
/// digit against a sign-dependent limit (positive cap `i64::MAX`, negative magnitude
/// cap `i64::MIN`).
const RT_HASH_NORMALIZE_KEY: &str = r#"(func $__rt_hash_normalize_key (param $ptr i32) (param $len i64) (result i64 i64)
  (local $i i64)
  (local $neg i32)
  (local $c i32)
  (local $d i64)
  (local $acc i64)
  (local $limit_div i64)
  (local $limit_mod i64)
  (if (i64.eqz (local.get $len))                                ;; empty string is not an int key
    (then (return (i64.extend_i32_u (local.get $ptr)) (local.get $len))))  ;; keep it a string key
  (local.set $neg (i32.const 0))                                ;; assume non-negative
  (local.set $i (i64.const 0))                                  ;; scan from byte 0
  (if (i32.eq (i32.load8_u (local.get $ptr)) (i32.const 45))    ;; leading '-'?
    (then
      (local.set $neg (i32.const 1))                            ;; mark negative
      (local.set $i (i64.const 1))                              ;; skip the sign byte
      (if (i64.eq (local.get $len) (i64.const 1))               ;; "-" alone is not an int
        (then (return (i64.extend_i32_u (local.get $ptr)) (local.get $len))))))  ;; string fallback
  (local.set $c (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))  ;; first digit byte
  (if (i32.eq (local.get $c) (i32.const 48))                    ;; leading '0'?
    (then
      (if (i32.and (i32.eqz (local.get $neg)) (i64.eq (local.get $len) (i64.const 1)))  ;; only a lone unsigned "0" is canonical
        (then (return (i64.const 0) (i64.const -1)))            ;; "0" -> int key 0
        (else (return (i64.extend_i32_u (local.get $ptr)) (local.get $len))))))  ;; "00"/"01"/"-0" stay strings
  (if (i32.or (i32.lt_u (local.get $c) (i32.const 49)) (i32.gt_u (local.get $c) (i32.const 57)))  ;; first digit must be '1'..'9'
    (then (return (i64.extend_i32_u (local.get $ptr)) (local.get $len))))  ;; non-digit start -> string
  (local.set $limit_div (i64.const 922337203685477580))         ;; floor(i64 cap / 10), same for both signs
  (if (i32.eq (local.get $neg) (i32.const 1))                   ;; negative magnitude cap is i64::MIN
    (then (local.set $limit_mod (i64.const 8)))                 ;; cap % 10 = 8 for 9223372036854775808
    (else (local.set $limit_mod (i64.const 7))))                ;; cap % 10 = 7 for 9223372036854775807
  (local.set $acc (i64.const 0))                                ;; accumulated magnitude
  (block $end
    (loop $scan
      (br_if $end (i64.ge_u (local.get $i) (local.get $len)))   ;; consumed every byte
      (local.set $c (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))  ;; next byte
      (if (i32.or (i32.lt_u (local.get $c) (i32.const 48)) (i32.gt_u (local.get $c) (i32.const 57)))  ;; any non-digit?
        (then (return (i64.extend_i32_u (local.get $ptr)) (local.get $len))))  ;; -> string fallback
      (local.set $d (i64.extend_i32_u (i32.sub (local.get $c) (i32.const 48))))  ;; digit value 0..9
      (if (i32.or (i64.gt_u (local.get $acc) (local.get $limit_div))            ;; acc*10 would overflow, or
                  (i32.and (i64.eq (local.get $acc) (local.get $limit_div))     ;; acc*10 is exactly at the cap and
                           (i64.gt_u (local.get $d) (local.get $limit_mod))))   ;; the last digit exceeds it
        (then (return (i64.extend_i32_u (local.get $ptr)) (local.get $len))))  ;; out of i64 range -> string
      (local.set $acc (i64.add (i64.mul (local.get $acc) (i64.const 10)) (local.get $d)))  ;; acc = acc*10 + d
      (local.set $i (i64.add (local.get $i) (i64.const 1)))     ;; advance
      (br $scan)))                                              ;; loop back-edge
  (if (result i64 i64) (i32.eqz (local.get $neg))               ;; apply the sign to the magnitude
    (then (local.get $acc) (i64.const -1))                      ;; positive: (acc, -1)
    (else (i64.sub (i64.const 0) (local.get $acc)) (i64.const -1))))  ;; negative: (-acc, -1); -acc of i64::MIN magnitude is i64::MIN
"#;

/// `__rt_hash_key_from_mixed`: turns a boxed `Mixed` array key into the two-word hash
/// key `(key_lo, key_hi)` the `__rt_hash_*` runtime expects. Unboxes the cell (tags:
/// 0=int, 1=string, 2=float, 3=bool, 8=null) and classifies: int/bool pass through as
/// `(value, -1)`; a float truncates toward zero (PHP key coercion); a string is routed
/// through `__rt_hash_normalize_key` so integer-like strings collapse to int keys; null
/// becomes PHP's empty-string key `(0, 0)`. Any other (illegal) offset type falls back
/// to its payload word as an int key — PHP would fatal, but the wasm backend has no
/// exceptions yet. `key_hi == -1` marks an int key; `key_hi >= 0` marks a string key.
const RT_HASH_KEY_FROM_MIXED: &str = r#"(func $__rt_hash_key_from_mixed (param $ptr i32) (result i64 i64)
  (local $tag i64)
  (local $lo i64)
  (local $hi i64)
  (call $__rt_mixed_unbox (local.get $ptr))                    ;; unbox -> (tag, lo, hi); hi on top
  (local.set $hi)                                              ;; payload high word (string length / unused)
  (local.set $lo)                                              ;; payload low word (value / string ptr)
  (local.set $tag)                                             ;; runtime type tag
  (if (i32.or (i64.eq (local.get $tag) (i64.const 0))          ;; int, or
              (i64.eq (local.get $tag) (i64.const 3)))         ;; bool: low word is already the integer key
    (then (return (local.get $lo) (i64.const -1))))            ;; (value, -1) marks an int key
  (if (i64.eq (local.get $tag) (i64.const 2))                  ;; float key
    (then (return (i64.trunc_sat_f64_s (f64.reinterpret_i64 (local.get $lo))) (i64.const -1))))  ;; truncate toward zero
  (if (i64.eq (local.get $tag) (i64.const 1))                  ;; string key
    (then (return (call $__rt_hash_normalize_key (i32.wrap_i64 (local.get $lo)) (local.get $hi)))))  ;; int-like -> int, else string
  (if (i64.eq (local.get $tag) (i64.const 8))                  ;; null key
    (then (return (i64.const 0) (i64.const 0))))               ;; PHP $a[null] is the "" (zero-length string) key
  (local.get $lo)                                              ;; illegal offset type: use the payload word
  (i64.const -1))                                              ;; as an int key (no exceptions yet)
"#;

/// `__rt_hash_iter_next`: advances a foreach cursor over a hash in INSERTION ORDER and
/// returns `(new_cursor, has_more)`. `cursor` is a slot index, with the sentinel `-2`
/// meaning "before the first entry": on that first call the next slot is the list head
/// (`hash+24`); on later calls it is the current entry's `next` link (`entry+56`). Both
/// head and next store slot indices with `-1` as the end sentinel, and the insertion
/// list holds only live entries (unset splices removed ones out), so walking never
/// lands on an empty/tombstone slot. `has_more` is `1` while `new_cursor != -1`.
const RT_HASH_ITER_NEXT: &str = r#"(func $__rt_hash_iter_next (param $hash i32) (param $cursor i64) (result i64 i64)
  (local $next i64)                                            ;; next slot index to return
  (local $entry i32)                                           ;; address of the current entry
  (if (i64.eq (local.get $cursor) (i64.const -2))              ;; first call (before-first sentinel)?
    (then
      (local.set $next (i64.load (i32.add (local.get $hash) (i32.const 24)))))  ;; next = list head
    (else
      (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40))     ;; entry = hash+40+cursor*64
                                 (i32.wrap_i64 (i64.mul (local.get $cursor) (i64.const 64)))))
      (local.set $next (i64.load (i32.add (local.get $entry) (i32.const 56))))))  ;; next = entry.next
  (local.get $next)                                            ;; result 0: new cursor (slot index, -1 at end)
  (i64.extend_i32_u (i64.ne (local.get $next) (i64.const -1))))  ;; result 1: has_more (1 unless -1)
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

/// `__rt_hash_get`: linear-probe lookup of `(key_lo, key_hi)`. Returns the 4-tuple
/// `(found, value_lo, value_hi, value_tag)`; a miss yields `(0, 0, 0, 8)` (PHP null
/// tag). Probing stops at the first empty slot (definitive absence) or after
/// `capacity` probes; tombstones (occupied == 2) are skipped, and equality is only
/// tested on live slots so a freed tombstone key is never dereferenced.
const RT_HASH_GET: &str = r#"(func $__rt_hash_get (param $hash i32) (param $key_lo i64) (param $key_hi i64) (result i32 i64 i64 i64)
  (local $cap i64)
  (local $slot i64)
  (local $probes i64)
  (local $entry i32)
  (local $occ i64)
  (if (i32.eqz (local.get $hash))
    (then (return (i32.const 0) (i64.const 0) (i64.const 0) (i64.const 8))))  ;; null hash -> miss
  (local.set $cap (i64.load (i32.add (local.get $hash) (i32.const 8))))  ;; capacity
  (if (i64.eqz (local.get $cap))
    (then (return (i32.const 0) (i64.const 0) (i64.const 0) (i64.const 8))))  ;; empty table -> miss
  (local.set $slot (i64.rem_u (call $__rt_hash_key_hash (local.get $key_lo) (local.get $key_hi)) (local.get $cap)))  ;; initial bucket
  (local.set $probes (i64.const 0))
  (block $done (loop $probe
    (br_if $done (i64.ge_u (local.get $probes) (local.get $cap)))  ;; probed every slot -> miss
    (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $slot) (i64.const 64)))))  ;; &slot[slot]
    (local.set $occ (i64.load (local.get $entry)))           ;; occupied flag
    (if (i64.eqz (local.get $occ))
      (then (return (i32.const 0) (i64.const 0) (i64.const 0) (i64.const 8))))  ;; empty slot -> definitively absent
    (if (i64.eq (local.get $occ) (i64.const 1))              ;; only compare live entries (skip tombstones)
      (then
        (if (call $__rt_hash_key_eq (local.get $key_lo) (local.get $key_hi)
              (i64.load (i32.add (local.get $entry) (i32.const 8)))
              (i64.load (i32.add (local.get $entry) (i32.const 16))))  ;; keys equal?
          (then (return (i32.const 1)
            (i64.load (i32.add (local.get $entry) (i32.const 24)))   ;; value_lo
            (i64.load (i32.add (local.get $entry) (i32.const 32)))   ;; value_hi
            (i64.load (i32.add (local.get $entry) (i32.const 40))))))))  ;; value_tag
    (local.set $slot (i64.rem_u (i64.add (local.get $slot) (i64.const 1)) (local.get $cap)))  ;; next bucket (wrap)
    (local.set $probes (i64.add (local.get $probes) (i64.const 1)))
    (br $probe)))
  (i32.const 0) (i64.const 0) (i64.const 0) (i64.const 8))    ;; saturated -> miss
"#;

/// `__rt_hash_insert_owned`: places an ALREADY-OWNED key/value into the first empty
/// slot and appends it to the insertion-order list. Assumes the key is absent and
/// the table has room — it neither persists strings, resizes, nor updates an
/// existing key (used by resize/clone). Returns the same `hash` pointer.
const RT_HASH_INSERT_OWNED: &str = r#"(func $__rt_hash_insert_owned (param $hash i32) (param $key_lo i64) (param $key_hi i64) (param $val_lo i64) (param $val_hi i64) (param $val_tag i64) (result i32)
  (local $cap i64)
  (local $slot i64)
  (local $entry i32)
  (local $tail i64)
  (local $tail_entry i32)
  (local.set $cap (i64.load (i32.add (local.get $hash) (i32.const 8))))  ;; capacity
  (local.set $slot (i64.rem_u (call $__rt_hash_key_hash (local.get $key_lo) (local.get $key_hi)) (local.get $cap)))  ;; initial bucket
  (block $empty (loop $probe
    (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $slot) (i64.const 64)))))  ;; &slot[slot]
    (br_if $empty (i64.eqz (i64.load (local.get $entry))))   ;; found an empty slot
    (local.set $slot (i64.rem_u (i64.add (local.get $slot) (i64.const 1)) (local.get $cap)))  ;; next bucket (wrap)
    (br $probe)))
  (i64.store (local.get $entry) (i64.const 1))               ;; occupied = live
  (i64.store (i32.add (local.get $entry) (i32.const 8)) (local.get $key_lo))    ;; key_lo
  (i64.store (i32.add (local.get $entry) (i32.const 16)) (local.get $key_hi))   ;; key_hi
  (i64.store (i32.add (local.get $entry) (i32.const 24)) (local.get $val_lo))   ;; value_lo
  (i64.store (i32.add (local.get $entry) (i32.const 32)) (local.get $val_hi))   ;; value_hi
  (i64.store (i32.add (local.get $entry) (i32.const 40)) (local.get $val_tag))  ;; value_tag
  (local.set $tail (i64.load (i32.add (local.get $hash) (i32.const 32))))       ;; current tail slot
  (i64.store (i32.add (local.get $entry) (i32.const 48)) (local.get $tail))     ;; prev = old tail
  (i64.store (i32.add (local.get $entry) (i32.const 56)) (i64.const -1))        ;; next = none
  (if (i64.ne (local.get $tail) (i64.const -1))
    (then
      (local.set $tail_entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $tail) (i64.const 64)))))  ;; &old tail
      (i64.store (i32.add (local.get $tail_entry) (i32.const 56)) (local.get $slot)))  ;; old tail's next = this slot
    (else
      (i64.store (i32.add (local.get $hash) (i32.const 24)) (local.get $slot))))  ;; first entry -> head = this slot
  (i64.store (i32.add (local.get $hash) (i32.const 32)) (local.get $slot))      ;; tail = this slot
  (i64.store (local.get $hash) (i64.add (i64.load (local.get $hash)) (i64.const 1)))  ;; count++
  (local.get $hash))
"#;

/// `__rt_hash_resize`: doubles the capacity and rehashes every live entry in
/// insertion order, MOVING (not copying) each key/value into a fresh table, then
/// frees the old table shallowly. Returns the new hash pointer.
const RT_HASH_RESIZE: &str = r#"(func $__rt_hash_resize (param $hash i32) (result i32)
  (local $newcap i64)
  (local $new i32)
  (local $cur i64)
  (local $oe i32)
  (local.set $newcap (i64.shl (i64.load (i32.add (local.get $hash) (i32.const 8))) (i64.const 1)))  ;; newcap = capacity * 2
  (if (i64.lt_u (local.get $newcap) (i64.const 8))
    (then (local.set $newcap (i64.const 8))))                ;; minimum capacity 8
  (local.set $new (call $__rt_hash_new (local.get $newcap) (i64.load (i32.add (local.get $hash) (i32.const 16)))))  ;; fresh larger table, same value_type
  (local.set $cur (i64.load (i32.add (local.get $hash) (i32.const 24))))  ;; cur = old head
  (block $done (loop $walk
    (br_if $done (i64.eq (local.get $cur) (i64.const -1)))   ;; reached end of insertion order
    (local.set $oe (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 64)))))  ;; &old entry
    (drop (call $__rt_hash_insert_owned (local.get $new)
      (i64.load (i32.add (local.get $oe) (i32.const 8)))     ;; key_lo
      (i64.load (i32.add (local.get $oe) (i32.const 16)))    ;; key_hi
      (i64.load (i32.add (local.get $oe) (i32.const 24)))    ;; value_lo
      (i64.load (i32.add (local.get $oe) (i32.const 32)))    ;; value_hi
      (i64.load (i32.add (local.get $oe) (i32.const 40)))))  ;; value_tag (moved, not persisted)
    (local.set $cur (i64.load (i32.add (local.get $oe) (i32.const 56))))  ;; cur = next
    (br $walk)))
  (call $__rt_heap_free (local.get $hash))                   ;; shallow free: children moved to `new`
  (local.get $new))
"#;

/// `__rt_hash_clone_shallow`: allocates a fresh hash with the same capacity and
/// value_type, then copies every live entry in insertion order, giving the clone
/// independent ownership: string keys and string values are re-persisted, refcounted
/// container values are increfed (shared child / own reference), and scalar values
/// are copied verbatim.
const RT_HASH_CLONE_SHALLOW: &str = r#"(func $__rt_hash_clone_shallow (param $hash i32) (result i32)
  (local $new i32)
  (local $cur i64)
  (local $oe i32)
  (local $klo i64)
  (local $khi i64)
  (local $vlo i64)
  (local $vhi i64)
  (local $vtag i64)
  (local $np i32)
  (local $nl i64)
  (local.set $new (call $__rt_hash_new
    (i64.load (i32.add (local.get $hash) (i32.const 8)))
    (i64.load (i32.add (local.get $hash) (i32.const 16)))))  ;; fresh hash, same capacity + value_type
  (local.set $cur (i64.load (i32.add (local.get $hash) (i32.const 24))))  ;; cur = old head
  (block $done (loop $walk
    (br_if $done (i64.eq (local.get $cur) (i64.const -1)))   ;; reached end of insertion order
    (local.set $oe (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 64)))))  ;; &old entry
    (local.set $klo (i64.load (i32.add (local.get $oe) (i32.const 8))))   ;; key_lo
    (local.set $khi (i64.load (i32.add (local.get $oe) (i32.const 16))))  ;; key_hi
    (if (i64.ge_s (local.get $khi) (i64.const 0))            ;; string key -> own a fresh copy
      (then
        (call $__rt_str_persist (i32.wrap_i64 (local.get $klo)) (local.get $khi))  ;; persist key string
        (local.set $nl)                                      ;; persisted length (unused; same length)
        (local.set $np)                                      ;; persisted pointer
        (local.set $klo (i64.extend_i32_u (local.get $np)))))  ;; clone's key_lo
    (local.set $vlo (i64.load (i32.add (local.get $oe) (i32.const 24))))  ;; value_lo
    (local.set $vhi (i64.load (i32.add (local.get $oe) (i32.const 32))))  ;; value_hi
    (local.set $vtag (i64.load (i32.add (local.get $oe) (i32.const 40)))) ;; value_tag
    (if (i64.eq (local.get $vtag) (i64.const 1))             ;; string value -> own a fresh copy
      (then
        (call $__rt_str_persist (i32.wrap_i64 (local.get $vlo)) (local.get $vhi))  ;; persist value string
        (local.set $nl)                                      ;; persisted length
        (local.set $np)                                      ;; persisted pointer
        (local.set $vlo (i64.extend_i32_u (local.get $np)))  ;; clone's value_lo
        (local.set $vhi (local.get $nl)))                    ;; clone's value_hi
      (else
        (if (i32.or (i32.or (i64.eq (local.get $vtag) (i64.const 4)) (i64.eq (local.get $vtag) (i64.const 5)))
            (i32.or (i32.or (i64.eq (local.get $vtag) (i64.const 6)) (i64.eq (local.get $vtag) (i64.const 7)))
                    (i64.eq (local.get $vtag) (i64.const 10))))  ;; refcounted container value
          (then (call $__rt_incref (i32.wrap_i64 (local.get $vlo)))))))  ;; share, bump refcount
    (drop (call $__rt_hash_insert_owned (local.get $new) (local.get $klo) (local.get $khi) (local.get $vlo) (local.get $vhi) (local.get $vtag)))  ;; place into clone
    (local.set $cur (i64.load (i32.add (local.get $oe) (i32.const 56))))  ;; cur = next
    (br $walk)))
  (local.get $new))
"#;

/// `__rt_hash_ensure_unique`: the copy-on-write split point. Returns the hash
/// unchanged when it has at most one owner (refcount <= 1); otherwise clones it
/// shallowly, decrements the original's refcount, and returns the clone. COW is
/// refcount-driven — the kind word's COW bit is only a marker.
const RT_HASH_ENSURE_UNIQUE: &str = r#"(func $__rt_hash_ensure_unique (param $hash i32) (result i32)
  (local $rc i32)
  (local $clone i32)
  (if (i32.eqz (local.get $hash))
    (then (return (i32.const 0))))                           ;; null -> trivially unique
  (local.set $rc (i32.load (i32.sub (local.get $hash) (i32.const 12))))  ;; refcount @ hash-12
  (if (i32.le_s (local.get $rc) (i32.const 1))
    (then (return (local.get $hash))))                       ;; sole owner -> mutate in place
  (local.set $clone (call $__rt_hash_clone_shallow (local.get $hash)))  ;; duplicate before mutation
  (i32.store (i32.sub (local.get $hash) (i32.const 12)) (i32.sub (local.get $rc) (i32.const 1)))  ;; original loses this reference
  (local.get $clone))                                        ;; caller now owns the clone
"#;

/// `__rt_hash_set`: the user-facing `$h[k] = v`. Splits a shared hash (COW), grows
/// when the load factor would exceed 75%, probes for the key, and either UPDATES the
/// value in place (releasing the old heap child) or INSERTS a new entry. String keys
/// and string values are persisted into owned copies; refcounted container values are
/// increfed. Returns the (possibly cloned/reallocated) hash pointer.
const RT_HASH_SET: &str = r#"(func $__rt_hash_set (param $hash i32) (param $key_lo i64) (param $key_hi i64) (param $val_lo i64) (param $val_hi i64) (param $val_tag i64) (result i32)
  (local $cap i64)
  (local $slot i64)
  (local $probes i64)
  (local $entry i32)
  (local $occ i64)
  (local $matched i32)
  (local $mentry i32)
  (local $oldtag i64)
  (local $np i32)
  (local $nl i64)
  (local.set $hash (call $__rt_hash_ensure_unique (local.get $hash)))  ;; copy-on-write split
  (if (i64.ge_u (i64.mul (i64.load (local.get $hash)) (i64.const 4))
                (i64.mul (i64.load (i32.add (local.get $hash) (i32.const 8))) (i64.const 3)))  ;; count*4 >= capacity*3
    (then (local.set $hash (call $__rt_hash_resize (local.get $hash)))))  ;; grow past 75% load
  (local.set $cap (i64.load (i32.add (local.get $hash) (i32.const 8))))  ;; capacity (post-resize)
  (local.set $slot (i64.rem_u (call $__rt_hash_key_hash (local.get $key_lo) (local.get $key_hi)) (local.get $cap)))  ;; initial bucket
  (local.set $probes (i64.const 0))
  (local.set $matched (i32.const 0))
  (block $stop (loop $probe
    (br_if $stop (i64.ge_u (local.get $probes) (local.get $cap)))  ;; probed every slot
    (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $slot) (i64.const 64)))))  ;; &slot[slot]
    (local.set $occ (i64.load (local.get $entry)))           ;; occupied flag
    (br_if $stop (i64.eqz (local.get $occ)))                 ;; empty slot -> key absent
    (if (i64.eq (local.get $occ) (i64.const 1))              ;; live entry?
      (then (if (call $__rt_hash_key_eq (local.get $key_lo) (local.get $key_hi)
                  (i64.load (i32.add (local.get $entry) (i32.const 8)))
                  (i64.load (i32.add (local.get $entry) (i32.const 16))))  ;; keys equal?
              (then
                (local.set $matched (i32.const 1))
                (local.set $mentry (local.get $entry))))))
    (br_if $stop (i32.eq (local.get $matched) (i32.const 1)))  ;; found existing key
    (local.set $slot (i64.rem_u (i64.add (local.get $slot) (i64.const 1)) (local.get $cap)))  ;; next bucket (wrap)
    (local.set $probes (i64.add (local.get $probes) (i64.const 1)))
    (br $probe)))
  (if (i32.eq (local.get $matched) (i32.const 1))             ;; UPDATE existing entry
    (then
      (local.set $oldtag (i64.load (i32.add (local.get $mentry) (i32.const 40))))  ;; old value tag
      (if (i64.eq (local.get $oldtag) (i64.const 1))          ;; old string -> free it
        (then (call $__rt_heap_free_safe (i32.wrap_i64 (i64.load (i32.add (local.get $mentry) (i32.const 24)))))))
      (if (i32.or (i32.or (i64.eq (local.get $oldtag) (i64.const 4)) (i64.eq (local.get $oldtag) (i64.const 5)))
          (i32.or (i32.or (i64.eq (local.get $oldtag) (i64.const 6)) (i64.eq (local.get $oldtag) (i64.const 7)))
                  (i64.eq (local.get $oldtag) (i64.const 10))))  ;; old container -> decref
        (then (call $__rt_decref_any (i32.wrap_i64 (i64.load (i32.add (local.get $mentry) (i32.const 24)))))))
      (if (i64.eq (local.get $val_tag) (i64.const 1))          ;; new string -> own a copy
        (then
          (call $__rt_str_persist (i32.wrap_i64 (local.get $val_lo)) (local.get $val_hi))
          (local.set $nl)
          (local.set $np)
          (local.set $val_lo (i64.extend_i32_u (local.get $np)))
          (local.set $val_hi (local.get $nl)))
        (else
          (if (i32.or (i32.or (i64.eq (local.get $val_tag) (i64.const 4)) (i64.eq (local.get $val_tag) (i64.const 5)))
              (i32.or (i32.or (i64.eq (local.get $val_tag) (i64.const 6)) (i64.eq (local.get $val_tag) (i64.const 7)))
                      (i64.eq (local.get $val_tag) (i64.const 10))))  ;; new container -> incref
            (then (call $__rt_incref (i32.wrap_i64 (local.get $val_lo)))))))
      (i64.store (i32.add (local.get $mentry) (i32.const 24)) (local.get $val_lo))  ;; value_lo
      (i64.store (i32.add (local.get $mentry) (i32.const 32)) (local.get $val_hi))  ;; value_hi
      (i64.store (i32.add (local.get $mentry) (i32.const 40)) (local.get $val_tag)) ;; value_tag
      (return (local.get $hash))))
  (if (i64.ge_s (local.get $key_hi) (i64.const 0))            ;; INSERT: own the string key
    (then
      (call $__rt_str_persist (i32.wrap_i64 (local.get $key_lo)) (local.get $key_hi))
      (local.set $nl)
      (local.set $np)
      (local.set $key_lo (i64.extend_i32_u (local.get $np)))))  ;; key_hi unchanged
  (if (i64.eq (local.get $val_tag) (i64.const 1))              ;; own the string value
    (then
      (call $__rt_str_persist (i32.wrap_i64 (local.get $val_lo)) (local.get $val_hi))
      (local.set $nl)
      (local.set $np)
      (local.set $val_lo (i64.extend_i32_u (local.get $np)))
      (local.set $val_hi (local.get $nl)))
    (else
      (if (i32.or (i32.or (i64.eq (local.get $val_tag) (i64.const 4)) (i64.eq (local.get $val_tag) (i64.const 5)))
          (i32.or (i32.or (i64.eq (local.get $val_tag) (i64.const 6)) (i64.eq (local.get $val_tag) (i64.const 7)))
                  (i64.eq (local.get $val_tag) (i64.const 10))))  ;; own the container value
        (then (call $__rt_incref (i32.wrap_i64 (local.get $val_lo)))))))
  (local.set $hash (call $__rt_hash_insert_owned (local.get $hash) (local.get $key_lo) (local.get $key_hi) (local.get $val_lo) (local.get $val_hi) (local.get $val_tag)))  ;; place the new entry
  (local.get $hash))
"#;

/// `__rt_hash_append`: the user-facing `$h[] = v`. Computes the next PHP integer append
/// key by scanning every live entry for the largest integer key and adding one (or 0 when
/// the hash has no integer keys — matching the native append-key scan, which likewise does
/// not track a monotonic next-free index), then delegates to `__rt_hash_set` with that key
/// and `key_hi = -1`. `__rt_hash_set` owns the copy-on-write split, resize, and value
/// persist/incref, so this routine only derives the key. Returns the resulting hash pointer.
const RT_HASH_APPEND: &str = r#"(func $__rt_hash_append (param $hash i32) (param $val_lo i64) (param $val_hi i64) (param $val_tag i64) (result i32)
  (local $cap i64)
  (local $i i64)
  (local $key i64)
  (local $seen i64)
  (local $entry i32)
  (local $cand i64)
  (local.set $cap (i64.load (i32.add (local.get $hash) (i32.const 8))))   ;; capacity from header
  (local.set $i (i64.const 0))                                            ;; scan from slot 0
  (local.set $key (i64.const 0))                                          ;; default append key 0 (no int keys)
  (local.set $seen (i64.const 0))                                         ;; no integer key seen yet
  (block $end (loop $scan
    (br_if $end (i64.ge_u (local.get $i) (local.get $cap)))               ;; scanned every slot -> stop
    (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 64)))))  ;; &slot[i] = hash+40+i*64
    (if (i64.eq (i64.load (local.get $entry)) (i64.const 1))              ;; live entry?
      (then
        (if (i64.eq (i64.load (i32.add (local.get $entry) (i32.const 16))) (i64.const -1))  ;; integer key (key_hi == -1)?
          (then
            (local.set $cand (i64.add (i64.load (i32.add (local.get $entry) (i32.const 8))) (i64.const 1)))  ;; candidate = int key + 1
            (if (i64.eqz (local.get $seen))                               ;; first integer key seen?
              (then
                (local.set $key (local.get $cand))                       ;; seed the append key
                (local.set $seen (i64.const 1)))                         ;; remember we saw one
              (else
                (if (i64.gt_s (local.get $cand) (local.get $key))        ;; larger than current best?
                  (then (local.set $key (local.get $cand))))))))))       ;; keep the maximum int key + 1
    (local.set $i (i64.add (local.get $i) (i64.const 1)))                 ;; advance to next slot
    (br $scan)))                                                          ;; loop back-edge
  (call $__rt_hash_set (local.get $hash) (local.get $key) (i64.const -1) (local.get $val_lo) (local.get $val_hi) (local.get $val_tag)))
"#;

/// `__rt_hash_union`: the PHP array-union operator `$a + $b` on two ordered-map hashes.
/// Starts from a deep clone of `$a` (so the left operand's entries and order win), then
/// walks `$b` in insertion order and appends every entry whose key is absent from the
/// clone via `__rt_hash_set` (which persists/increfs the borrowed value). Borrows `$a`
/// and `$b` (never frees/decrefs them) and returns a fresh OWNED hash. No Mixed-promotion
/// pass is needed: unlike the native runtime's `__rt_hash_to_mixed`, the wasm hash always
/// stores entries concretely with a per-entry tag and boxes on read, so a heterogeneous
/// union result already has the representation a Mixed-valued read expects.
const RT_HASH_UNION: &str = r#"(func $__rt_hash_union (param $a i32) (param $b i32) (result i32)
  (local $result i32)
  (local $cur i64)
  (local $be i32)
  (local $klo i64)
  (local $khi i64)
  (local $found i32)
  (local $vlo i64)
  (local $vhi i64)
  (local $vtag i64)
  (if (i32.eqz (local.get $a))                                          ;; null left operand?
    (then (return (call $__rt_hash_clone_shallow (local.get $b)))))     ;; result is just a copy of b
  (local.set $result (call $__rt_hash_clone_shallow (local.get $a)))    ;; start from an owned copy of a
  (local.set $cur (i64.load (i32.add (local.get $b) (i32.const 24))))   ;; cur = b.head (insertion-order start)
  (block $done (loop $walk
    (br_if $done (i64.eq (local.get $cur) (i64.const -1)))              ;; end of b's insertion order
    (local.set $be (i32.add (i32.add (local.get $b) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 64)))))  ;; &b entry
    (local.set $klo (i64.load (i32.add (local.get $be) (i32.const 8))))   ;; b key_lo
    (local.set $khi (i64.load (i32.add (local.get $be) (i32.const 16))))  ;; b key_hi
    (call $__rt_hash_get (local.get $result) (local.get $klo) (local.get $khi))  ;; probe result for this key
    (drop)                                                              ;; discard value_tag
    (drop)                                                              ;; discard value_hi
    (drop)                                                              ;; discard value_lo
    (local.set $found)                                                  ;; keep the found flag
    (if (i32.eqz (local.get $found))                                    ;; key absent in a -> take b's entry
      (then
        (local.set $vlo (i64.load (i32.add (local.get $be) (i32.const 24))))   ;; b value_lo
        (local.set $vhi (i64.load (i32.add (local.get $be) (i32.const 32))))   ;; b value_hi
        (local.set $vtag (i64.load (i32.add (local.get $be) (i32.const 40))))  ;; b value_tag
        (local.set $result (call $__rt_hash_set (local.get $result) (local.get $klo) (local.get $khi) (local.get $vlo) (local.get $vhi) (local.get $vtag)))))  ;; append (set owns its copy)
    (local.set $cur (i64.load (i32.add (local.get $be) (i32.const 56))))  ;; cur = b entry.next
    (br $walk)))
  (local.get $result))
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

    /// A lookup that misses returns found = 0 (and the PHP null tuple).
    #[test]
    fn get_misses_on_empty_hash() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const 7) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.extend_i32_u (local.get $found)) (local.get $vtag)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "8"); // found 0 + null tag 8
        }
    }

    /// Insert an owned int key/value then read it back: `found*1000 + value_lo` =
    /// 1*1000 + 100 = 1100.
    #[test]
    fn insert_owned_then_get_roundtrips() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 42) (i64.const -1) (i64.const 100) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const 42) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.mul (i64.extend_i32_u (local.get $found)) (i64.const 1000)) (local.get $vlo)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1100");
        }
    }

    /// Three distinct int keys insert and resolve independently: looking up key 2
    /// returns `found*1000 + value_lo` = 1*1000 + 20 = 1020.
    #[test]
    fn insert_owned_multiple_keys_resolve() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 2) (i64.const -1) (i64.const 20) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 3) (i64.const -1) (i64.const 30) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const 2) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.mul (i64.extend_i32_u (local.get $found)) (i64.const 1000)) (local.get $vlo)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1020");
        }
    }

    /// Resize doubles capacity, rehashes in insertion order, and preserves entries:
    /// after resizing a 3-of-4 table, the head entry's key is still 1 (first
    /// inserted), count is 3, and key 3 still resolves to 30. Returns
    /// `head_key*1000 + count*100 + get(3).value_lo` = 1000 + 300 + 30 = 1330.
    #[test]
    fn resize_preserves_entries_and_insertion_order() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $head i64) (local $headkey i64) (local $count i64)
  (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 4) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 2) (i64.const -1) (i64.const 20) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 3) (i64.const -1) (i64.const 30) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_resize (local.get $h)))
  (local.set $head (i64.load (i32.add (local.get $h) (i32.const 24))))
  (local.set $headkey (i64.load (i32.add (i32.add (i32.add (local.get $h) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $head) (i64.const 64)))) (i32.const 8))))
  (local.set $count (i64.load (local.get $h)))
  (call $__rt_hash_get (local.get $h) (i64.const 3) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.add
    (i64.mul (local.get $headkey) (i64.const 1000))
    (i64.mul (local.get $count) (i64.const 100)))
    (local.get $vlo)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1330");
        }
    }

    /// `__rt_hash_clone_shallow` copies int entries into a distinct table: the clone
    /// is a different pointer, has the same count, and resolves key 2 to 20. Returns
    /// `(distinct)*1000 + count*100 + get(clone,2).value_lo` = 1000 + 200 + 20 = 1220.
    #[test]
    fn clone_shallow_copies_int_entries() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $c i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 2) (i64.const -1) (i64.const 20) (i64.const 0) (i64.const 0)))
  (local.set $c (call $__rt_hash_clone_shallow (local.get $h)))
  (call $__rt_hash_get (local.get $c) (i64.const 2) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.add
    (i64.mul (i64.extend_i32_u (i32.ne (local.get $h) (local.get $c))) (i64.const 1000))
    (i64.mul (i64.load (local.get $c)) (i64.const 100)))
    (local.get $vlo)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1220");
        }
    }

    /// `__rt_hash_clone_shallow` deep-copies string values: the clone's value pointer
    /// differs from the source's (re-persisted) yet holds the same first byte 'H'(72).
    /// Returns `(distinct_ptr)*1000 + byte0` = 1000 + 72 = 1072.
    #[test]
    fn clone_shallow_deep_copies_string_value() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $c i32) (local $sp i32) (local $sl i64)
  (local $origptr i64) (local $cloneptr i64)
  (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (i32.store8 (i32.const 400) (i32.const 72))
  (i32.store8 (i32.const 401) (i32.const 105))
  (call $__rt_str_persist (i32.const 400) (i64.const 2))
  (local.set $sl) (local.set $sp)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 1)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 1) (i64.const -1) (i64.extend_i32_u (local.get $sp)) (local.get $sl) (i64.const 1)))
  (call $__rt_hash_get (local.get $h) (i64.const 1) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $origptr) (local.set $found)
  (local.set $c (call $__rt_hash_clone_shallow (local.get $h)))
  (call $__rt_hash_get (local.get $c) (i64.const 1) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $cloneptr) (local.set $found)
  (i64.add
    (i64.mul (i64.extend_i32_u (i64.ne (local.get $origptr) (local.get $cloneptr))) (i64.const 1000))
    (i64.extend_i32_u (i32.load8_u (i32.wrap_i64 (local.get $cloneptr))))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1072");
        }
    }

    /// `__rt_hash_ensure_unique` on a sole-owner hash returns the SAME pointer.
    #[test]
    fn ensure_unique_returns_same_when_unshared() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (i64.extend_i32_u (i32.eq (local.get $h) (call $__rt_hash_ensure_unique (local.get $h)))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1");
        }
    }

    /// `__rt_hash_ensure_unique` on a shared hash (refcount 2) returns a distinct
    /// clone and decrements the original's refcount back to 1. Returns
    /// `(distinct)*10 + original_refcount` = 1*10 + 1 = 11.
    #[test]
    fn ensure_unique_clones_when_shared() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $u i32)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 1) (i64.const -1) (i64.const 9) (i64.const 0) (i64.const 0)))
  (call $__rt_incref (local.get $h))
  (local.set $u (call $__rt_hash_ensure_unique (local.get $h)))
  (i64.add
    (i64.mul (i64.extend_i32_u (i32.ne (local.get $h) (local.get $u))) (i64.const 10))
    (i64.extend_i32_s (i32.load (i32.sub (local.get $h) (i32.const 12))))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "11");
        }
    }

    /// `__rt_hash_set` inserts an int key/value then `__rt_hash_get` reads it back:
    /// `found*1000 + value_lo` = 1000 + 50 = 1050.
    #[test]
    fn set_insert_int_then_get() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 5) (i64.const -1) (i64.const 50) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const 5) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.mul (i64.extend_i32_u (local.get $found)) (i64.const 1000)) (local.get $vlo)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1050");
        }
    }

    /// Setting the same key twice updates in place (no new entry): count stays 1 and
    /// the value becomes 99. Returns `count*1000 + get.value_lo` = 1000 + 99 = 1099.
    #[test]
    fn set_update_existing_key() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 5) (i64.const -1) (i64.const 50) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 5) (i64.const -1) (i64.const 99) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const 5) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.mul (i64.load (local.get $h)) (i64.const 1000)) (local.get $vlo)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1099");
        }
    }

    /// A string key and string value round-trip through set + get: the persisted
    /// value's first byte is 'Y'(89), and the lookup (by an independent copy of the
    /// key bytes) hits. Returns `found*1000 + value_byte0` = 1000 + 89 = 1089.
    #[test]
    fn set_string_key_and_value() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (i32.store8 (i32.const 500) (i32.const 111)) (i32.store8 (i32.const 501) (i32.const 107))
  (i32.store8 (i32.const 510) (i32.const 89)) (i32.store8 (i32.const 511) (i32.const 111))
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 1)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 500) (i64.const 2) (i64.const 510) (i64.const 2) (i64.const 1)))
  (call $__rt_hash_get (local.get $h) (i64.const 500) (i64.const 2))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.mul (i64.extend_i32_u (local.get $found)) (i64.const 1000))
           (i64.extend_i32_u (i32.load8_u (i32.wrap_i64 (local.get $vlo))))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1089");
        }
    }

    /// Inserting past the 75% load factor on a capacity-4 table triggers a resize;
    /// all four keys still resolve and count is 4. Returns `count*1000 + get(4).vlo`
    /// = 4000 + 40 = 4040.
    #[test]
    fn set_triggers_resize_keeps_keys() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 4) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 2) (i64.const -1) (i64.const 20) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 3) (i64.const -1) (i64.const 30) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 4) (i64.const -1) (i64.const 40) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const 4) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.mul (i64.load (local.get $h)) (i64.const 1000)) (local.get $vlo)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "4040");
        }
    }

    /// Setting a key on a shared hash (refcount 2) copies-on-write: the original is
    /// untouched (key 1 stays 10) while the returned clone sees the new value (99).
    /// Returns `original_value + clone_value*1000` = 10 + 99000 = 99010.
    #[test]
    fn set_cow_clones_shared_hash() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $b i32) (local $ov i64) (local $cv i64)
  (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (call $__rt_incref (local.get $h))
  (local.set $b (call $__rt_hash_set (local.get $h) (i64.const 1) (i64.const -1) (i64.const 99) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const 1) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $ov) (local.set $found)
  (call $__rt_hash_get (local.get $b) (i64.const 1) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $cv) (local.set $found)
  (i64.add (local.get $ov) (i64.mul (local.get $cv) (i64.const 1000))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "99010");
        }
    }

    /// Builds a driver that writes `s` to memory at offset 300, normalizes it as a hash
    /// key, and returns the materialized int value when it is a canonical integer key
    /// (`key_hi == -1`) or the returned `key_hi` (the byte length for a string key)
    /// otherwise — letting a test distinguish an int key from a string key by sign.
    fn normalize_driver(s: &str) -> String {
        let mut body = String::from(
            "(func $t (export \"t\") (result i64)\n  (local $lo i64) (local $hi i64)\n",
        );
        for (i, b) in s.bytes().enumerate() {
            body.push_str(&format!(
                "  (i32.store8 (i32.const {}) (i32.const {}))\n",
                300 + i,
                b
            ));
        }
        body.push_str(&format!(
            "  (call $__rt_hash_normalize_key (i32.const 300) (i64.const {}))\n",
            s.len()
        ));
        body.push_str("  (local.set $hi) (local.set $lo)\n");
        body.push_str(
            "  (if (result i64) (i64.eq (local.get $hi) (i64.const -1))\n    (then (local.get $lo))\n    (else (local.get $hi))))",
        );
        body
    }

    /// Canonical decimal integers normalize to int keys: "123" -> 123, "-45" -> -45,
    /// "0" -> 0. The driver returns the int value when `key_hi == -1`.
    #[test]
    fn normalize_key_canonical_integers() {
        if let Some(o) = run_driver(&normalize_driver("123"), "t") {
            assert_eq!(o, "123");
        }
        if let Some(o) = run_driver(&normalize_driver("-45"), "t") {
            assert_eq!(o, "-45");
        }
        if let Some(o) = run_driver(&normalize_driver("0"), "t") {
            assert_eq!(o, "0");
        }
    }

    /// Non-canonical numeric strings stay string keys (the driver returns `key_hi`,
    /// which is the byte length, not -1): "01" (leading zero, len 2), "-0" (len 2),
    /// "+1" (leading plus, len 2), "1 " (trailing space, len 2).
    #[test]
    fn normalize_key_non_canonical_stays_string() {
        for (s, len) in [("01", "2"), ("-0", "2"), ("+1", "2"), ("1 ", "2")] {
            if let Some(o) = run_driver(&normalize_driver(s), "t") {
                assert_eq!(o, len, "input {:?} should stay a string key", s);
            }
        }
    }

    /// Non-numeric strings stay string keys: "abc" (len 3) and "-" alone (len 1)
    /// return `key_hi` = the length rather than -1.
    #[test]
    fn normalize_key_non_numeric_stays_string() {
        if let Some(o) = run_driver(&normalize_driver("abc"), "t") {
            assert_eq!(o, "3");
        }
        if let Some(o) = run_driver(&normalize_driver("-"), "t") {
            assert_eq!(o, "1");
        }
    }

    /// The i64 boundaries normalize exactly: "9223372036854775807" -> i64::MAX and
    /// "-9223372036854775808" -> i64::MIN (the negative-magnitude cap).
    #[test]
    fn normalize_key_i64_bounds() {
        if let Some(o) = run_driver(&normalize_driver("9223372036854775807"), "t") {
            assert_eq!(o, "9223372036854775807");
        }
        if let Some(o) = run_driver(&normalize_driver("-9223372036854775808"), "t") {
            assert_eq!(o, "-9223372036854775808");
        }
    }

    /// Out-of-range magnitudes stay string keys: "9223372036854775808" (i64::MAX + 1,
    /// len 19) and "-9223372036854775809" (i64::MIN - 1, len 20) overflow the per-digit
    /// cap and return `key_hi` = the byte length instead of an int value.
    #[test]
    fn normalize_key_overflow_stays_string() {
        if let Some(o) = run_driver(&normalize_driver("9223372036854775808"), "t") {
            assert_eq!(o, "19");
        }
        if let Some(o) = run_driver(&normalize_driver("-9223372036854775809"), "t") {
            assert_eq!(o, "20");
        }
    }

    /// `__rt_hash_iter_next` walks three int-keyed entries in INSERTION ORDER. Inserting
    /// keys 10, 20, 30 then walking from the `-2` sentinel and folding each visited
    /// entry's `key_lo` as `acc = acc*100 + key` yields 102030 — proving both the
    /// head→next traversal order and that the loop terminates (has_more flips to 0 after
    /// the third entry, never re-reading).
    #[test]
    fn iter_next_walks_insertion_order() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $c i64) (local $m i64) (local $acc i64) (local $entry i32)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 10) (i64.const -1) (i64.const 0) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 20) (i64.const -1) (i64.const 0) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 30) (i64.const -1) (i64.const 0) (i64.const 0) (i64.const 0)))
  (local.set $c (i64.const -2))
  (local.set $acc (i64.const 0))
  (block $end (loop $L
    (call $__rt_hash_iter_next (local.get $h) (local.get $c))
    (local.set $m)
    (local.set $c)
    (br_if $end (i64.eqz (local.get $m)))
    (local.set $entry (i32.add (i32.add (local.get $h) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $c) (i64.const 64)))))
    (local.set $acc (i64.add (i64.mul (local.get $acc) (i64.const 100)) (i64.load (i32.add (local.get $entry) (i32.const 8)))))
    (br $L)))
  (local.get $acc))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "102030");
        }
    }

    /// `__rt_hash_iter_next` on an empty hash returns immediately: the first call with
    /// the `-2` sentinel reads `head == -1`, so `new_cursor == -1` and `has_more == 0`.
    /// Returns `has_more*10 + (new_cursor == -1)` = 0*10 + 1 = 1.
    #[test]
    fn iter_next_empty_hash_has_no_more() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $c i64) (local $m i64)
  (local.set $h (call $__rt_hash_new (i64.const 4) (i64.const 0)))
  (call $__rt_hash_iter_next (local.get $h) (i64.const -2))
  (local.set $m)
  (local.set $c)
  (i64.add
    (i64.mul (local.get $m) (i64.const 10))
    (i64.extend_i32_u (i64.eq (local.get $c) (i64.const -1)))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1");
        }
    }

    /// `__rt_hash_append` into a fresh hash uses integer key 0: appending value 42 then
    /// reading key 0 hits with `found*100 + value_lo` = 1*100 + 42 = 142.
    #[test]
    fn append_into_empty_uses_key_zero() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_append (local.get $h) (i64.const 42) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const 0) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.mul (i64.extend_i32_u (local.get $found)) (i64.const 100)) (local.get $vlo)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "142");
        }
    }

    /// `__rt_hash_append`'s next-key scan tracks the largest integer key, not the entry
    /// count: appending 100 (key 0) then 200 (key 1), explicitly setting key 5 = 500, then
    /// appending 999 places it at key 6 (max int key 5 + 1). Reading key 6 returns
    /// `found*1000 + value_lo` = 1*1000 + 999 = 1999.
    #[test]
    fn append_next_key_follows_max_int_key() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_append (local.get $h) (i64.const 100) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_append (local.get $h) (i64.const 200) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 5) (i64.const -1) (i64.const 500) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_append (local.get $h) (i64.const 999) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const 6) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.mul (i64.extend_i32_u (local.get $found)) (i64.const 1000)) (local.get $vlo)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1999");
        }
    }

    /// `__rt_hash_union` keeps the LEFT operand's value on a key collision and appends the
    /// right operand's new keys. With a = {1:10, 2:20} and b = {2:99, 3:30}, the union is
    /// {1:10, 2:20, 3:30}: key 2 stays 20 (left wins), key 3 is 30 (from b), count is 3.
    /// Returns `get(2)*10000 + get(3)*100 + count` = 20*10000 + 30*100 + 3 = 203003.
    #[test]
    fn union_left_wins_and_merges() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32) (local $u i32)
  (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local $g2 i64) (local $g3 i64) (local $cnt i64)
  (local.set $a (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $a (call $__rt_hash_set (local.get $a) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (local.set $a (call $__rt_hash_set (local.get $a) (i64.const 2) (i64.const -1) (i64.const 20) (i64.const 0) (i64.const 0)))
  (local.set $b (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $b (call $__rt_hash_set (local.get $b) (i64.const 2) (i64.const -1) (i64.const 99) (i64.const 0) (i64.const 0)))
  (local.set $b (call $__rt_hash_set (local.get $b) (i64.const 3) (i64.const -1) (i64.const 30) (i64.const 0) (i64.const 0)))
  (local.set $u (call $__rt_hash_union (local.get $a) (local.get $b)))
  (call $__rt_hash_get (local.get $u) (i64.const 2) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $g2) (local.set $found)
  (call $__rt_hash_get (local.get $u) (i64.const 3) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $g3) (local.set $found)
  (local.set $cnt (i64.load (local.get $u)))
  (i64.add (i64.add (i64.mul (local.get $g2) (i64.const 10000)) (i64.mul (local.get $g3) (i64.const 100))) (local.get $cnt)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "203003");
        }
    }

    /// `__rt_hash_union` BORROWS its left operand: after unioning a = {1:10, 2:20} with
    /// b = {3:30}, the original `a` is unchanged — key 3 still misses in `a` and `a`'s
    /// count is still 2. Returns `a_count*10 + get(a,3).found` = 2*10 + 0 = 20.
    #[test]
    fn union_borrows_left_operand() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32) (local $u i32)
  (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $a (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $a (call $__rt_hash_set (local.get $a) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (local.set $a (call $__rt_hash_set (local.get $a) (i64.const 2) (i64.const -1) (i64.const 20) (i64.const 0) (i64.const 0)))
  (local.set $b (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $b (call $__rt_hash_set (local.get $b) (i64.const 3) (i64.const -1) (i64.const 30) (i64.const 0) (i64.const 0)))
  (local.set $u (call $__rt_hash_union (local.get $a) (local.get $b)))
  (call $__rt_hash_get (local.get $a) (i64.const 3) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.mul (i64.load (local.get $a)) (i64.const 10)) (i64.extend_i32_u (local.get $found))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "20");
        }
    }
}
