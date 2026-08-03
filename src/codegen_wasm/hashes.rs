//! Purpose:
//! Emits the hand-authored WebAssembly (WAT) associative-array (hash) runtime for
//! the wasm32-wasi backend. PHP arrays are *ordered* maps: this layer owns the
//! hash-table allocation, the key hashing/equality primitives, and the teardown
//! (deep free + refcount dispatch). The element operations (get/set), copy-on-write,
//! iteration, auto-indexed append (`$h[] = v`), element removal (`unset($h[$k])` via
//! `__rt_hash_unset`), and the `$a + $b` array-union operator — both same-shape
//! (`__rt_hash_union`) and the cross-representation forms that promote an indexed
//! operand to integer keys (`__rt_array_hash_union`, `__rt_hash_array_union`) — are
//! layered on top.
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
//! - Entries are 72 bytes each, slot `i` at `P + 40 + i*72`:
//!     +0 occupied (0 empty / 1 live / 2 tombstone), +8 key_lo, +16 key_hi
//!     (-1 = int-key sentinel, else string length), +24 value_lo, +32 value_hi,
//!     +40 value_tag, +48 prev, +56 next (prev/next are the insertion-order list),
//!     and +64 the logical Zend bucket ordinal used to preserve `nNumUsed` holes.
//! - A 32-byte Zend-semantics trailer follows the slots at `P + 40 + capacity*72`:
//!   next-free integer key, logical table mode (uninitialized/packed/mixed),
//!   logical `nNumUsed`, and logical table size. Physical probe capacity is independent
//!   of this logical state; physical resize preserves the trailer and ordinals exactly.
//! - Int keys hash with a Knuth multiplicative mix; string keys with FNV-1a. The
//!   split probe/logical representation preserves php-src semantics, not its bytes.

use super::wat::WatModule;
use crate::web_prelude::PhpVersion;

/// Adds the hash-table helper/teardown runtime to `wm`: hashing, key equality,
/// allocation, deep free, and the refcount-dispatcher's hash branch. Emitted after
/// the heap, refcount, array, and mixed runtimes.
pub(super) fn emit_hash_runtime(wm: &mut WatModule) {
    emit_hash_runtime_for_version(wm, crate::codegen_support::compile_php_version());
}

/// Emits the hash runtime for one PHP compatibility profile.
///
/// Mutable HashTables start with `ZEND_LONG_MIN` on every supported PHP profile.
/// PHP 8.2's zero-valued immutable `zend_empty_array` is represented initially as
/// an indexed empty array; `Op::ArrayToHash` preserves that origin separately.
fn emit_hash_runtime_for_version(wm: &mut WatModule, _php_version: PhpVersion) {
    wm.add_raw_func(RT_HASH_FNV1A);
    wm.add_raw_func(RT_HASH_KEY_HASH);
    wm.add_raw_func(RT_HASH_KEY_EQ);
    wm.add_raw_func(RT_HASH_NORMALIZE_KEY);
    wm.add_raw_func(RT_HASH_KEY_FROM_MIXED);
    wm.add_raw_func(RT_HASH_ITER_NEXT);
    wm.add_raw_func(RT_HASH_META_ADDR);
    wm.add_raw_func(RT_HASH_ZEND_TABLE_SIZE);
    wm.add_raw_func(RT_HASH_NEW);
    wm.add_raw_func(RT_HASH_VALIDATE_LAYOUT);
    wm.add_raw_func(RT_HASH_PREFLIGHT_UNION);
    wm.add_raw_func(RT_HASH_GET);
    wm.add_raw_func(RT_HASH_INSERT_OWNED);
    wm.add_raw_func(RT_HASH_RESIZE);
    wm.add_raw_func(RT_HASH_CLONE_SHALLOW);
    wm.add_raw_func(RT_HASH_ENSURE_UNIQUE);
    wm.add_raw_func(RT_HASH_COMPACT_LOGICAL);
    wm.add_raw_func(RT_HASH_RESERVE_NEW_KEY);
    wm.add_raw_func(RT_HASH_SET);
    wm.add_raw_func(RT_HASH_UNSET);
    wm.add_raw_func(RT_HASH_APPEND);
    wm.add_raw_func(RT_HASH_UNION);
    wm.add_raw_func(RT_ARRAY_HASH_UNION);
    wm.add_raw_func(RT_HASH_ARRAY_UNION);
    wm.add_raw_func(RT_ARRAY_TO_HASH);
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
  (local.set $i (i64.const 0))                              ;; i = 0 (byte cursor)
  (block $end (loop $byte
    (br_if $end (i64.ge_u (local.get $i) (local.get $len)))  ;; consumed all bytes
    (local.set $hash (i64.xor (local.get $hash)
      (i64.extend_i32_u (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))))))  ;; hash ^= byte
    (local.set $hash (i64.mul (local.get $hash) (i64.const 0x100000001b3)))  ;; hash *= FNV prime (wraps)
    (local.set $i (i64.add (local.get $i) (i64.const 1)))    ;; i++
    (br $byte)))                                             ;; next byte
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
    (then (return (i32.const 0))))                          ;; different lengths -> unequal
  (local.set $i (i64.const 0))                              ;; i = 0 (byte cursor)
  (block $end (loop $byte
    (br_if $end (i64.ge_u (local.get $i) (local.get $l_hi)))  ;; compared all bytes -> equal
    (if (i32.ne
          (i32.load8_u (i32.add (i32.wrap_i64 (local.get $l_lo)) (i32.wrap_i64 (local.get $i))))
          (i32.load8_u (i32.add (i32.wrap_i64 (local.get $r_lo)) (i32.wrap_i64 (local.get $i)))))  ;; bytes differ?
      (then (return (i32.const 0))))                        ;; bytes differ -> unequal
    (local.set $i (i64.add (local.get $i) (i64.const 1)))   ;; i++
    (br $byte)))                                            ;; next byte
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
/// `(value, -1)`; a float truncates toward zero and wraps finite out-of-range values
/// modulo 2^64 (PHP 64-bit key coercion result); a string is routed through
/// `__rt_hash_normalize_key` so integer-like strings collapse to int keys; null becomes
/// PHP's empty-string key `(0, 0)`. Any other (illegal) offset type falls back to its
/// payload word as an int key — PHP would fatal, but the wasm backend has no exceptions
/// yet. `key_hi == -1` marks an int key; `key_hi >= 0` marks a string key.
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
    (then (return (call $__rt_float_to_int (local.get $lo)) (i64.const -1))))  ;; PHP float key coercion
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
      (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40))     ;; entry = hash+40+cursor*72
                                 (i32.wrap_i64 (i64.mul (local.get $cursor) (i64.const 72)))))
      (local.set $next (i64.load (i32.add (local.get $entry) (i32.const 56))))))  ;; next = entry.next
  (local.get $next)                                            ;; result 0: new cursor (slot index, -1 at end)
  (i64.extend_i32_u (i64.ne (local.get $next) (i64.const -1))))  ;; result 1: has_more (1 unless -1)
"#;

/// `__rt_hash_meta_addr`: returns the address of the Zend-semantics trailer that
/// immediately follows the capacity-sized 72-byte entry array.
const RT_HASH_META_ADDR: &str = r#"(func $__rt_hash_meta_addr (param $hash i32) (result i32)
  (i32.add
    (i32.add (local.get $hash) (i32.const 40))
    (i32.wrap_i64
      (i64.mul
        (i64.load (i32.add (local.get $hash) (i32.const 8)))
        (i64.const 72)))))
"#;

/// `__rt_hash_zend_table_size`: rounds a capacity hint to php-src's minimum-eight,
/// power-of-two logical table size used by the uninitialized-to-packed decision.
const RT_HASH_ZEND_TABLE_SIZE: &str = r#"(func $__rt_hash_zend_table_size (param $capacity i64) (result i64)
  (local $size i64)
  (local.set $size (i64.const 8))                            ;; Zend's minimum table size
  (block $done (loop $grow
    (br_if $done (i64.ge_u (local.get $size) (local.get $capacity)))  ;; rounded up far enough
    (if (i64.ge_u (local.get $size) (i64.const 4294967296))
      (then unreachable))                                   ;; elephc-trap:proven-invariant:hash-capacity-limit impossible wasm32 hash capacity
    (local.set $size (i64.shl (local.get $size) (i64.const 1)))  ;; next power of two
    (br $grow)))
  (local.get $size))
"#;

/// `__rt_hash_new`: allocates an empty hash with `capacity` entry slots and a
/// default `value_tag`. Stamps the hash kind word, initializes the header (empty
/// insertion-order list), zeroes every slot's `occupied` field (heap memory may
/// be dirty from reuse), and initializes the trailing Zend-visible append state.
const RT_HASH_NEW: &str = r#"(func $__rt_hash_new (param $capacity i64) (param $value_tag i64) (result i32)
  (local $bytes i32)
  (local $p i32)
  (local $i i64)
  (local $meta i32)
  (local.set $bytes
    (call $__rt_checked_layout
      (local.get $capacity)
      (i64.const 72)
      (i64.const 72)))                                      ;; checked 40B header + capacity*72 slots + 32B trailer
  (local.set $p (call $__rt_heap_alloc (local.get $bytes)))  ;; block: refcount=1
  (i64.store (i32.sub (local.get $p) (i32.const 8)) (i64.const 32771))  ;; kind = hash(3) | COW(0x8000)
  (i64.store (local.get $p) (i64.const 0))                   ;; count = 0
  (i64.store (i32.add (local.get $p) (i32.const 8)) (local.get $capacity))   ;; capacity
  (i64.store (i32.add (local.get $p) (i32.const 16)) (local.get $value_tag)) ;; value_type
  (i64.store (i32.add (local.get $p) (i32.const 24)) (i64.const -1))  ;; head = -1 (empty)
  (i64.store (i32.add (local.get $p) (i32.const 32)) (i64.const -1))  ;; tail = -1 (empty)
  (local.set $i (i64.const 0))                               ;; slot cursor = 0
  (block $end (loop $slot
    (br_if $end (i64.ge_u (local.get $i) (local.get $capacity)))  ;; zeroed every slot
    (i64.store
      (i32.add (i32.add (local.get $p) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 72))))  ;; &slot[i].occupied = P+40+i*72
      (i64.const 0))                                         ;; occupied = empty
    (local.set $i (i64.add (local.get $i) (i64.const 1)))    ;; next slot
    (br $slot)))                                             ;; next slot
  (local.set $meta (call $__rt_hash_meta_addr (local.get $p)))  ;; trailer after all entry slots
  (i64.store (local.get $meta) (i64.const -9223372036854775808))  ;; mutable HashTable nNextFreeElement = ZEND_LONG_MIN
  (i64.store (i32.add (local.get $meta) (i32.const 8)) (i64.const 0))   ;; Zend mode = UNINITIALIZED
  (i64.store (i32.add (local.get $meta) (i32.const 16)) (i64.const 0))  ;; Zend nNumUsed = 0
  (i64.store (i32.add (local.get $meta) (i32.const 24))
    (call $__rt_hash_zend_table_size (local.get $capacity)))  ;; logical Zend nTableSize
  (local.get $p))
"#;

/// `__rt_hash_validate_layout`: proves that a hash's physical slot/trailer layout
/// is representable and that its live entry count does not exceed capacity.
const RT_HASH_VALIDATE_LAYOUT: &str = r#"(func $__rt_hash_validate_layout (param $hash i32)
  (local $count i64)
  (local $capacity i64)
  (local.set $count (i64.load (local.get $hash)))
  (local.set $capacity (i64.load (i32.add (local.get $hash) (i32.const 8))))
  (drop (call $__rt_checked_layout
    (local.get $capacity)
    (i64.const 72)
    (i64.const 72)))                                        ;; header + slots + semantic trailer
  (if (i64.gt_u (local.get $count) (local.get $capacity))
    (then (call $__rt_oom) unreachable)))                   ;; elephc-trap:deterministic-oom:hash-malformed-live-count malformed live count
"#;

/// `__rt_hash_preflight_union`: validates the worst-case capacity needed when
/// every right-hand entry has a distinct key. Two slots per possible entry keep
/// the result below the resize threshold; this runs before cloning or insertion.
const RT_HASH_PREFLIGHT_UNION: &str = r#"(func $__rt_hash_preflight_union (param $left_count i64) (param $right_count i64)
  (local $total i64)
  (if (i64.gt_u
        (local.get $left_count)
        (i64.sub (i64.const 9223372036854775807) (local.get $right_count)))
    (then (call $__rt_oom) unreachable))                   ;; elephc-trap:deterministic-oom:hash-union-count-overflow signed count sum would overflow
  (local.set $total (i64.add (local.get $left_count) (local.get $right_count)))
  (drop (call $__rt_checked_layout
    (local.get $total)
    (i64.const 144)
    (i64.const 72)))                                        ;; capacity=2*total, 72 bytes per slot
)
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
  (local.set $probes (i64.const 0))                          ;; probe counter = 0
  (block $done (loop $probe
    (br_if $done (i64.ge_u (local.get $probes) (local.get $cap)))  ;; probed every slot -> miss
    (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $slot) (i64.const 72)))))  ;; &slot[slot]
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
    (local.set $probes (i64.add (local.get $probes) (i64.const 1)))    ;; probe counter++
    (br $probe)))                                            ;; next probe
  (i32.const 0) (i64.const 0) (i64.const 0) (i64.const 8))    ;; saturated -> miss
"#;

/// `__rt_hash_insert_owned`: places an ALREADY-OWNED key/value into the first
/// tombstone in its probe chain, or the first empty slot when no tombstone was
/// encountered, and appends it to the insertion-order list. The bounded full-table
/// fallback reuses a remembered tombstone when no empty slot exists. Assumes the key
/// is absent and the table has room — it neither persists strings, resizes, nor
/// updates an existing key (used by set/resize/clone). `zend_idx` is the logical
/// php-src bucket ordinal, independent of the physical probe slot. Returns `hash`.
const RT_HASH_INSERT_OWNED: &str = r#"(func $__rt_hash_insert_owned (param $hash i32) (param $key_lo i64) (param $key_hi i64) (param $val_lo i64) (param $val_hi i64) (param $val_tag i64) (param $zend_idx i64) (result i32)
  (local $cap i64)
  (local $slot i64)
  (local $probes i64)
  (local $first_tomb i64)
  (local $entry i32)
  (local $occ i64)
  (local $tail i64)
  (local $tail_entry i32)
  (local.set $cap (i64.load (i32.add (local.get $hash) (i32.const 8))))  ;; capacity
  (local.set $slot (i64.rem_u (call $__rt_hash_key_hash (local.get $key_lo) (local.get $key_hi)) (local.get $cap)))  ;; initial bucket
  (local.set $probes (i64.const 0))                          ;; probe counter = 0
  (local.set $first_tomb (i64.const -1))                     ;; no reusable tombstone seen yet
  (block $place (loop $probe
    (if (i64.ge_u (local.get $probes) (local.get $cap))      ;; completed a full table tour?
      (then
        (if (i64.eq (local.get $first_tomb) (i64.const -1))  ;; no empty and no tombstone violates the room contract
          (then unreachable))                              ;; elephc-trap:proven-invariant:hash-insert-room-contract preflight guarantees an empty slot or tombstone
        (local.set $slot (local.get $first_tomb))            ;; full table with a tombstone -> reuse it
        (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $slot) (i64.const 72)))))  ;; &remembered tombstone
        (br $place)))
    (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $slot) (i64.const 72)))))  ;; &slot[slot]
    (local.set $occ (i64.load (local.get $entry)))           ;; occupied flag
    (if (i32.and (i64.eq (local.get $occ) (i64.const 2))
                 (i64.eq (local.get $first_tomb) (i64.const -1)))  ;; first tombstone in this probe chain?
      (then (local.set $first_tomb (local.get $slot))))      ;; remember it, but keep probing
    (if (i64.eqz (local.get $occ))                           ;; empty slot proves the absent key can be placed
      (then
        (if (i64.ne (local.get $first_tomb) (i64.const -1))  ;; prefer the first earlier tombstone
          (then
            (local.set $slot (local.get $first_tomb))
            (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $slot) (i64.const 72)))))))  ;; &remembered tombstone
        (br $place)))
    (local.set $slot (i64.rem_u (i64.add (local.get $slot) (i64.const 1)) (local.get $cap)))  ;; next bucket (wrap)
    (local.set $probes (i64.add (local.get $probes) (i64.const 1)))    ;; probe counter++
    (br $probe)))                                            ;; next probe
  (i64.store (local.get $entry) (i64.const 1))               ;; occupied = live
  (i64.store (i32.add (local.get $entry) (i32.const 8)) (local.get $key_lo))    ;; key_lo
  (i64.store (i32.add (local.get $entry) (i32.const 16)) (local.get $key_hi))   ;; key_hi
  (i64.store (i32.add (local.get $entry) (i32.const 24)) (local.get $val_lo))   ;; value_lo
  (i64.store (i32.add (local.get $entry) (i32.const 32)) (local.get $val_hi))   ;; value_hi
  (i64.store (i32.add (local.get $entry) (i32.const 40)) (local.get $val_tag))  ;; value_tag
  (i64.store (i32.add (local.get $entry) (i32.const 64)) (local.get $zend_idx)) ;; logical Zend bucket ordinal
  (local.set $tail (i64.load (i32.add (local.get $hash) (i32.const 32))))       ;; current tail slot
  (i64.store (i32.add (local.get $entry) (i32.const 48)) (local.get $tail))     ;; prev = old tail
  (i64.store (i32.add (local.get $entry) (i32.const 56)) (i64.const -1))        ;; next = none
  (if (i64.ne (local.get $tail) (i64.const -1))
    (then
      (local.set $tail_entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $tail) (i64.const 72)))))  ;; &old tail
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
  (local $oldcap i64)
  (local $newcap i64)
  (local $new i32)
  (local $cur i64)
  (local $oe i32)
  (local $oldmeta i32)
  (local $newmeta i32)
  (call $__rt_hash_validate_layout (local.get $hash))       ;; prove current slot/trailer addresses before reading metadata
  (local.set $oldcap (i64.load (i32.add (local.get $hash) (i32.const 8))))
  (local.set $newcap (i64.add (local.get $oldcap) (local.get $oldcap)))  ;; double without a wrapping shift
  (if (i64.lt_u (local.get $newcap) (i64.const 8))
    (then (local.set $newcap (i64.const 8))))                ;; minimum capacity 8
  (drop (call $__rt_checked_layout (local.get $newcap) (i64.const 72) (i64.const 72)))  ;; reject overflow before any allocation/move
  (local.set $oldmeta (call $__rt_hash_meta_addr (local.get $hash)))  ;; preserve semantic trailer across physical rehash
  (local.set $new (call $__rt_hash_new (local.get $newcap) (i64.load (i32.add (local.get $hash) (i32.const 16)))))  ;; fresh larger table, same value_type
  (local.set $cur (i64.load (i32.add (local.get $hash) (i32.const 24))))  ;; cur = old head
  (block $done (loop $walk
    (br_if $done (i64.eq (local.get $cur) (i64.const -1)))   ;; reached end of insertion order
    (local.set $oe (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))  ;; &old entry
    (drop (call $__rt_hash_insert_owned (local.get $new)
      (i64.load (i32.add (local.get $oe) (i32.const 8)))     ;; key_lo
      (i64.load (i32.add (local.get $oe) (i32.const 16)))    ;; key_hi
      (i64.load (i32.add (local.get $oe) (i32.const 24)))    ;; value_lo
      (i64.load (i32.add (local.get $oe) (i32.const 32)))    ;; value_hi
      (i64.load (i32.add (local.get $oe) (i32.const 40)))    ;; value_tag (moved, not persisted)
      (i64.load (i32.add (local.get $oe) (i32.const 64)))))  ;; preserve logical bucket ordinal exactly
    (local.set $cur (i64.load (i32.add (local.get $oe) (i32.const 56))))  ;; cur = next
    (br $walk)))                                             ;; next entry
  (local.set $newmeta (call $__rt_hash_meta_addr (local.get $new)))
  (i64.store (local.get $newmeta) (i64.load (local.get $oldmeta)))  ;; exact next-free
  (i64.store (i32.add (local.get $newmeta) (i32.const 8)) (i64.load (i32.add (local.get $oldmeta) (i32.const 8))))   ;; exact logical mode
  (i64.store (i32.add (local.get $newmeta) (i32.const 16)) (i64.load (i32.add (local.get $oldmeta) (i32.const 16)))) ;; exact logical nNumUsed
  (i64.store (i32.add (local.get $newmeta) (i32.const 24)) (i64.load (i32.add (local.get $oldmeta) (i32.const 24)))) ;; exact logical nTableSize
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
  (local $oldmeta i32)
  (local $newmeta i32)
  (local $mode i64)
  (local $zend_idx i64)
  (local $dense_idx i64)
  (local.set $oldmeta (call $__rt_hash_meta_addr (local.get $hash)))  ;; source semantic trailer
  (local.set $mode (i64.load (i32.add (local.get $oldmeta) (i32.const 8))))  ;; source logical mode
  (local.set $dense_idx (i64.const 0))                       ;; next compact MIXED ordinal
  (local.set $new (call $__rt_hash_new
    (i64.load (i32.add (local.get $hash) (i32.const 8)))
    (i64.load (i32.add (local.get $hash) (i32.const 16)))))  ;; fresh hash, same capacity + value_type
  (local.set $cur (i64.load (i32.add (local.get $hash) (i32.const 24))))  ;; cur = old head
  (block $done (loop $walk
    (br_if $done (i64.eq (local.get $cur) (i64.const -1)))   ;; reached end of insertion order
    (local.set $oe (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))  ;; &old entry
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
    (if (i64.eq (local.get $mode) (i64.const 2))             ;; ordinary MIXED duplication compacts holes
      (then
        (local.set $zend_idx (local.get $dense_idx))
        (local.set $dense_idx (i64.add (local.get $dense_idx) (i64.const 1))))
      (else
        (local.set $zend_idx (i64.load (i32.add (local.get $oe) (i32.const 64))))))  ;; PACKED preserves implicit holes
    (drop (call $__rt_hash_insert_owned (local.get $new) (local.get $klo) (local.get $khi) (local.get $vlo) (local.get $vhi) (local.get $vtag) (local.get $zend_idx)))  ;; place into clone
    (local.set $cur (i64.load (i32.add (local.get $oe) (i32.const 56))))  ;; cur = next
    (br $walk)))                                             ;; next entry
  (local.set $newmeta (call $__rt_hash_meta_addr (local.get $new)))
  (i64.store (local.get $newmeta) (i64.load (local.get $oldmeta)))  ;; clone preserves next-free exactly
  (i64.store (i32.add (local.get $newmeta) (i32.const 24)) (i64.load (i32.add (local.get $oldmeta) (i32.const 24))))  ;; clone preserves logical table size
  (if (i64.eqz (i64.load (local.get $hash)))                 ;; php-src duplicates an empty source as UNINITIALIZED
    (then
      (i64.store (i32.add (local.get $newmeta) (i32.const 8)) (i64.const 0))
      (i64.store (i32.add (local.get $newmeta) (i32.const 16)) (i64.const 0))
      (i64.store (i32.add (local.get $newmeta) (i32.const 24)) (i64.const 8)))  ;; empty duplicate resets to HT_MIN_SIZE
    (else
      (i64.store (i32.add (local.get $newmeta) (i32.const 8)) (i64.load (i32.add (local.get $oldmeta) (i32.const 8))))
      (if (i64.eq (local.get $mode) (i64.const 2))
        (then
          (i64.store (i32.add (local.get $newmeta) (i32.const 16)) (i64.load (local.get $new))))  ;; MIXED clone nNumUsed = live count
        (else
          (i64.store (i32.add (local.get $newmeta) (i32.const 16)) (i64.load (i32.add (local.get $oldmeta) (i32.const 16))))))))  ;; PACKED preserves holes
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

/// `__rt_hash_compact_logical`: renumbers every live logical bucket densely in
/// insertion order and returns the resulting `nNumUsed` (the live count).
///
/// Physical probe slots and links are untouched. This is php-src's semantic
/// `zend_hash_rehash()` compaction projected onto Elephc's split representation.
const RT_HASH_COMPACT_LOGICAL: &str = r#"(func $__rt_hash_compact_logical (param $hash i32) (result i64)
  (local $cur i64)
  (local $idx i64)
  (local $entry i32)
  (local.set $cur (i64.load (i32.add (local.get $hash) (i32.const 24))))  ;; insertion-order head
  (local.set $idx (i64.const 0))                               ;; next dense logical bucket
  (block $done (loop $walk
    (br_if $done (i64.eq (local.get $cur) (i64.const -1)))     ;; all live entries renumbered
    (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40))
      (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))  ;; current physical entry
    (i64.store (i32.add (local.get $entry) (i32.const 64)) (local.get $idx))  ;; dense Zend ordinal
    (local.set $idx (i64.add (local.get $idx) (i64.const 1)))  ;; next ordinal
    (local.set $cur (i64.load (i32.add (local.get $entry) (i32.const 56))))  ;; insertion-order next
    (br $walk)))
  (local.get $idx))
"#;

/// `__rt_hash_reserve_new_key`: updates the trailing php-src-compatible logical
/// `HashTable` state for a key known to be absent and returns its logical bucket
/// ordinal. The caller stores that ordinal in the new physical entry.
///
/// Modes are 0 = UNINITIALIZED, 1 = PACKED, and 2 = MIXED. The PACKED branches
/// reproduce `_zend_hash_index_add_or_update_i`: inserting below `nNumUsed` into
/// a hole converts to MIXED, in-range extension resets next-free to `h + 1`, and
/// sparse/string/negative keys use the monotonic MIXED rule.
const RT_HASH_RESERVE_NEW_KEY: &str = r#"(func $__rt_hash_reserve_new_key (param $hash i32) (param $key_lo i64) (param $key_hi i64) (result i64)
  (local $meta i32)
  (local $mode i64)
  (local $used i64)
  (local $size i64)
  (local $next i64)
  (local $old_count i64)
  (local $mixed i32)
  (local $idx i64)
  (local.set $meta (call $__rt_hash_meta_addr (local.get $hash)))  ;; Zend-state trailer
  (local.set $mode (i64.load (i32.add (local.get $meta) (i32.const 8))))  ;; logical flags
  (local.set $used (i64.load (i32.add (local.get $meta) (i32.const 16)))) ;; logical nNumUsed
  (local.set $size (i64.load (i32.add (local.get $meta) (i32.const 24)))) ;; logical nTableSize
  (local.set $next (i64.load (local.get $meta)))             ;; nNextFreeElement
  (local.set $old_count (i64.load (local.get $hash)))        ;; live count before this insertion
  (local.set $mixed (i32.const 0))                           ;; no mixed insertion selected yet

  local.get $key_hi                                         ;; string key?
  i64.const 0
  i64.ge_s
  if
    local.get $mode                                         ;; first string initializes MIXED directly
    i64.eqz
    if
      local.get $meta
      i32.const 8
      i32.add
      i64.const 2
      i64.store                                             ;; mode = MIXED
      local.get $meta
      i32.const 16
      i32.add
      i64.const 1
      i64.store                                             ;; nNumUsed = 1
      i64.const 0                                           ;; first mixed bucket ordinal
      return
    end
    i32.const 1
    local.set $mixed                                        ;; existing PACKED/MIXED table
  end

  local.get $key_hi                                         ;; integer into UNINITIALIZED table?
  i64.const -1
  i64.eq
  local.get $mode
  i64.eqz
  i32.and
  if
    local.get $key_lo
    i64.const 0
    i64.ge_s
    local.get $key_lo
    local.get $size
    i64.lt_u
    i32.and
    if
      local.get $meta
      local.get $key_lo
      i64.const 1
      i64.add
      i64.store                                             ;; add_to_packed resets next to h + 1
      local.get $meta
      i32.const 8
      i32.add
      i64.const 1
      i64.store                                             ;; mode = PACKED
      local.get $meta
      i32.const 16
      i32.add
      local.get $key_lo
      i64.const 1
      i64.add
      i64.store                                             ;; nNumUsed = h + 1
      local.get $key_lo                                     ;; packed bucket ordinal equals key
      return
    end
    i32.const 1
    local.set $mixed                                        ;; negative/out-of-range initializes MIXED
  end

  local.get $key_hi                                         ;; integer into PACKED table?
  i64.const -1
  i64.eq
  local.get $mode
  i64.const 1
  i64.eq
  i32.and
  if
    local.get $key_lo
    i64.const 0
    i64.lt_s
    if
      i32.const 1
      local.set $mixed                                      ;; negative integer converts to MIXED
    end
    local.get $mixed
    i32.eqz
    if
      local.get $key_lo
      local.get $used
      i64.lt_u
      if
        i32.const 1
        local.set $mixed                                    ;; absent lower key is a packed hole
      end
    end
    local.get $mixed
    i32.eqz
    if
      local.get $key_lo
      local.get $size
      i64.lt_u
      if
        local.get $meta
        local.get $key_lo
        i64.const 1
        i64.add
        i64.store                                           ;; packed extension resets next
        local.get $meta
        i32.const 16
        i32.add
        local.get $key_lo
        i64.const 1
        i64.add
        i64.store                                           ;; nNumUsed = h + 1
        local.get $key_lo                                   ;; packed bucket ordinal equals key
        return
      end
    end
    local.get $mixed
    i32.eqz
    if
      local.get $key_lo
      i64.const 1
      i64.shr_u
      local.get $size
      i64.lt_u
      local.get $size
      i64.const 1
      i64.shr_u
      local.get $old_count
      i64.lt_u
      i32.and
      if
        local.get $size
        i64.const 1
        i64.shl
        local.set $size
        local.get $meta
        i32.const 24
        i32.add
        local.get $size
        i64.store                                           ;; packed table doubles
        local.get $meta
        local.get $key_lo
        i64.const 1
        i64.add
        i64.store                                           ;; next = h + 1
        local.get $meta
        i32.const 16
        i32.add
        local.get $key_lo
        i64.const 1
        i64.add
        i64.store                                           ;; nNumUsed = h + 1
        local.get $key_lo                                   ;; packed bucket ordinal equals key
        return
      end
    end
    local.get $mixed
    i32.eqz
    if
      local.get $used
      local.get $size
      i64.ge_u
      if
        local.get $size
        i64.const 1
        i64.shl
        local.set $size
        local.get $meta
        i32.const 24
        i32.add
        local.get $size
        i64.store                                           ;; make room before PACKED -> MIXED
      end
      i32.const 1
      local.set $mixed                                      ;; sparse packed key converts to MIXED
    end
  end

  local.get $key_hi                                         ;; integer into existing MIXED table?
  i64.const -1
  i64.eq
  local.get $mode
  i64.const 2
  i64.eq
  i32.and
  if
    i32.const 1
    local.set $mixed
  end

  local.get $mixed
  if
    local.get $used
    local.get $size
    i64.ge_u
    if                                                     ;; ZEND_HASH_IF_FULL_DO_RESIZE
      local.get $used
      local.get $old_count
      local.get $old_count
      i64.const 5
      i64.shr_u
      i64.add
      i64.gt_u
      if
        local.get $old_count
        local.set $used                                    ;; compact tombstone buckets
      else
        local.get $size
        i64.const 1
        i64.shl
        local.set $size
        local.get $meta
        i32.const 24
        i32.add
        local.get $size
        i64.store                                          ;; grow mixed table
      end
      local.get $hash
      call $__rt_hash_compact_logical
      local.set $used                                      ;; every logical resize compacts holes
    end
    local.get $used
    local.set $idx                                         ;; reserve current logical tail
    local.get $used
    i64.const 1
    i64.add
    local.set $used                                        ;; append a mixed bucket
    local.get $meta
    i32.const 8
    i32.add
    i64.const 2
    i64.store                                              ;; mode = MIXED
    local.get $meta
    i32.const 16
    i32.add
    local.get $used
    i64.store                                              ;; persist nNumUsed
    local.get $key_hi
    i64.const -1
    i64.eq
    local.get $key_lo
    local.get $next
    i64.ge_s
    i32.and
    if
      local.get $key_lo
      i64.const 9223372036854775807
      i64.eq
      if
        local.get $meta
        i64.const 9223372036854775807
        i64.store                                          ;; saturate at PHP_INT_MAX
      else
        local.get $meta
        local.get $key_lo
        i64.const 1
        i64.add
        i64.store                                          ;; monotonically advance next
      end
    end
  end
  local.get $idx)
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
  (local $count i64)
  (local $need_resize i32)
  (local $newcap i64)
  (local $zend_idx i64)
  (call $__rt_hash_validate_layout (local.get $hash))        ;; validate physical metadata before COW
  (if (i64.ge_s (local.get $key_hi) (i64.const 0))
    (then
      (drop (call $__rt_checked_layout (local.get $key_hi) (i64.const 1) (i64.const 0)))))  ;; validate string-key length before COW
  (if (i64.eq (local.get $val_tag) (i64.const 1))
    (then
      (drop (call $__rt_checked_layout (local.get $val_hi) (i64.const 1) (i64.const 0)))))  ;; validate string-value length before COW/release
  (local.set $cap (i64.load (i32.add (local.get $hash) (i32.const 8))))
  (local.set $count (i64.load (local.get $hash)))
  (local.set $need_resize
    (i64.ge_u
      (i64.mul (local.get $count) (i64.const 4))
      (i64.mul (local.get $cap) (i64.const 3))))            ;; products are safe after the bounded-layout proof
  (if (local.get $need_resize)
    (then
      (local.set $newcap (i64.add (local.get $cap) (local.get $cap)))
      (if (i64.lt_u (local.get $newcap) (i64.const 8))
        (then (local.set $newcap (i64.const 8))))
      (drop (call $__rt_checked_layout (local.get $newcap) (i64.const 72) (i64.const 72)))))  ;; prove resize before COW
  (local.set $hash (call $__rt_hash_ensure_unique (local.get $hash)))  ;; copy-on-write split
  (if (local.get $need_resize)
    (then (local.set $hash (call $__rt_hash_resize (local.get $hash)))))  ;; grow past 75% load
  (local.set $cap (i64.load (i32.add (local.get $hash) (i32.const 8))))  ;; capacity (post-resize)
  (local.set $slot (i64.rem_u (call $__rt_hash_key_hash (local.get $key_lo) (local.get $key_hi)) (local.get $cap)))  ;; initial bucket
  (local.set $probes (i64.const 0))                          ;; probe counter = 0
  (local.set $matched (i32.const 0))                         ;; no match yet
  (block $stop (loop $probe
    (br_if $stop (i64.ge_u (local.get $probes) (local.get $cap)))  ;; probed every slot
    (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $slot) (i64.const 72)))))  ;; &slot[slot]
    (local.set $occ (i64.load (local.get $entry)))           ;; occupied flag
    (br_if $stop (i64.eqz (local.get $occ)))                 ;; empty slot -> key absent
    (if (i64.eq (local.get $occ) (i64.const 1))              ;; live entry?
      (then (if (call $__rt_hash_key_eq (local.get $key_lo) (local.get $key_hi)
                  (i64.load (i32.add (local.get $entry) (i32.const 8)))
                  (i64.load (i32.add (local.get $entry) (i32.const 16))))  ;; keys equal?
              (then
                (local.set $matched (i32.const 1))           ;; record the hit
                (local.set $mentry (local.get $entry))))))   ;; save matched entry address
    (br_if $stop (i32.eq (local.get $matched) (i32.const 1)))  ;; found existing key
    (local.set $slot (i64.rem_u (i64.add (local.get $slot) (i64.const 1)) (local.get $cap)))  ;; next bucket (wrap)
    (local.set $probes (i64.add (local.get $probes) (i64.const 1)))    ;; probe counter++
    (br $probe)))                                            ;; next probe
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
          (local.set $nl)                                    ;; persisted length
          (local.set $np)                                    ;; persisted pointer
          (local.set $val_lo (i64.extend_i32_u (local.get $np)))       ;; val_lo = persisted ptr
          (local.set $val_hi (local.get $nl)))               ;; val_hi = persisted length
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
      (local.set $nl)                                        ;; persisted length
      (local.set $np)                                        ;; persisted pointer
      (local.set $key_lo (i64.extend_i32_u (local.get $np)))))  ;; key_hi unchanged
  (if (i64.eq (local.get $val_tag) (i64.const 1))              ;; own the string value
    (then
      (call $__rt_str_persist (i32.wrap_i64 (local.get $val_lo)) (local.get $val_hi))
      (local.set $nl)                                        ;; persisted length
      (local.set $np)                                        ;; persisted pointer
      (local.set $val_lo (i64.extend_i32_u (local.get $np)))    ;; val_lo = persisted ptr
      (local.set $val_hi (local.get $nl)))                   ;; val_hi = persisted length
    (else
      (if (i32.or (i32.or (i64.eq (local.get $val_tag) (i64.const 4)) (i64.eq (local.get $val_tag) (i64.const 5)))
          (i32.or (i32.or (i64.eq (local.get $val_tag) (i64.const 6)) (i64.eq (local.get $val_tag) (i64.const 7)))
                  (i64.eq (local.get $val_tag) (i64.const 10))))  ;; own the container value
        (then (call $__rt_incref (i32.wrap_i64 (local.get $val_lo)))))))
  (local.set $zend_idx (call $__rt_hash_reserve_new_key (local.get $hash) (local.get $key_lo) (local.get $key_hi)))  ;; reserve logical Zend bucket
  (local.set $hash (call $__rt_hash_insert_owned (local.get $hash) (local.get $key_lo) (local.get $key_hi) (local.get $val_lo) (local.get $val_hi) (local.get $val_tag) (local.get $zend_idx)))  ;; place the new entry
  (local.get $hash))
"#;

/// `__rt_hash_unset`: the user-facing `unset($h[$k])`. Copy-on-write splits the table, then
/// linear-probes for the key exactly like `__rt_hash_set`/`__rt_hash_get` (a `$matched` flag
/// breaks the loop on the hit). The flat post-loop removal releases the owned string key and
/// value payloads using the same rules as `__rt_hash_set`'s update branch and
/// `__rt_hash_free_deep` (string -> `__rt_heap_free_safe`, container tags 4/5/6/7/10 ->
/// `__rt_decref_any`), splices the entry out of the insertion-order doubly-linked list,
/// tombstones the slot (occupied = 2, preserving probe chains), and decrements the live count.
/// A missing key or null/empty table is a no-op. Returns the unique (possibly cloned) hash ptr.
const RT_HASH_UNSET: &str = r#"(func $__rt_hash_unset (param $hash i32) (param $key_lo i64) (param $key_hi i64) (result i32)
  (local $cap i64)
  (local $slot i64)
  (local $probes i64)
  (local $entry i32)
  (local $occ i64)
  (local $matched i32)
  (local $mentry i32)
  (local $vtag i64)
  (local $prev i64)
  (local $next i64)
  (local $pe i32)
  (local $ne i32)
  (local $meta i32)
  (local $used i64)
  (local $removed_idx i64)
  (local $new_used i64)
  (local.set $hash (call $__rt_hash_ensure_unique (local.get $hash)))   ;; copy-on-write split
  (if (i32.eqz (local.get $hash)) (then (return (local.get $hash))))    ;; null hash -> nothing to remove
  (local.set $cap (i64.load (i32.add (local.get $hash) (i32.const 8)))) ;; capacity
  (if (i64.eqz (local.get $cap)) (then (return (local.get $hash))))     ;; empty table -> nothing to remove
  (local.set $slot (i64.rem_u (call $__rt_hash_key_hash (local.get $key_lo) (local.get $key_hi)) (local.get $cap)))  ;; initial bucket
  (local.set $probes (i64.const 0))                                    ;; probe counter
  (local.set $matched (i32.const 0))                                   ;; no match yet
  (block $stop (loop $probe
    (br_if $stop (i64.ge_u (local.get $probes) (local.get $cap)))      ;; probed every slot -> stop (absent)
    (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $slot) (i64.const 72)))))  ;; &slot[slot]
    (local.set $occ (i64.load (local.get $entry)))                     ;; occupied flag
    (br_if $stop (i64.eqz (local.get $occ)))                           ;; empty slot -> key absent, stop
    (if (i64.eq (local.get $occ) (i64.const 1))                        ;; live entry?
      (then (if (call $__rt_hash_key_eq (local.get $key_lo) (local.get $key_hi)
                  (i64.load (i32.add (local.get $entry) (i32.const 8)))
                  (i64.load (i32.add (local.get $entry) (i32.const 16))))  ;; keys equal?
              (then
                (local.set $matched (i32.const 1))                     ;; record the hit
                (local.set $mentry (local.get $entry))))))             ;; save matched entry address
    (br_if $stop (i32.eq (local.get $matched) (i32.const 1)))          ;; found -> leave loop
    (local.set $slot (i64.rem_u (i64.add (local.get $slot) (i64.const 1)) (local.get $cap)))  ;; next bucket (wrap)
    (local.set $probes (i64.add (local.get $probes) (i64.const 1)))    ;; bump probe count
    (br $probe)))                                                      ;; loop back-edge
  (if (i32.eq (local.get $matched) (i32.const 1))                      ;; REMOVE the matched entry
    (then
      (if (i64.ge_s (i64.load (i32.add (local.get $mentry) (i32.const 16))) (i64.const 0))  ;; string key?
        (then (call $__rt_heap_free_safe (i32.wrap_i64 (i64.load (i32.add (local.get $mentry) (i32.const 8)))))))  ;; free key string
      (local.set $vtag (i64.load (i32.add (local.get $mentry) (i32.const 40))))  ;; value tag
      (if (i64.eq (local.get $vtag) (i64.const 1))                     ;; string value?
        (then (call $__rt_heap_free_safe (i32.wrap_i64 (i64.load (i32.add (local.get $mentry) (i32.const 24))))))  ;; free value string
        (else
          (if (i32.or (i32.or (i64.eq (local.get $vtag) (i64.const 4)) (i64.eq (local.get $vtag) (i64.const 5)))
                      (i32.or (i32.or (i64.eq (local.get $vtag) (i64.const 6)) (i64.eq (local.get $vtag) (i64.const 7)))
                              (i64.eq (local.get $vtag) (i64.const 10))))  ;; container value?
            (then (call $__rt_decref_any (i32.wrap_i64 (i64.load (i32.add (local.get $mentry) (i32.const 24)))))))))  ;; release container child
      (local.set $prev (i64.load (i32.add (local.get $mentry) (i32.const 48))))  ;; prev slot index
      (local.set $next (i64.load (i32.add (local.get $mentry) (i32.const 56))))  ;; next slot index
      (local.set $removed_idx (i64.load (i32.add (local.get $mentry) (i32.const 64))))  ;; logical Zend bucket
      (if (i64.ne (local.get $prev) (i64.const -1))                    ;; has a predecessor?
        (then
          (local.set $pe (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $prev) (i64.const 72)))))  ;; &slot[prev]
          (i64.store (i32.add (local.get $pe) (i32.const 56)) (local.get $next)))  ;; predecessor.next = next
        (else (i64.store (i32.add (local.get $hash) (i32.const 24)) (local.get $next))))  ;; head = next
      (if (i64.ne (local.get $next) (i64.const -1))                    ;; has a successor?
        (then
          (local.set $ne (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $next) (i64.const 72)))))  ;; &slot[next]
          (i64.store (i32.add (local.get $ne) (i32.const 48)) (local.get $prev)))  ;; successor.prev = prev
        (else (i64.store (i32.add (local.get $hash) (i32.const 32)) (local.get $prev))))  ;; tail = prev
      (i64.store (local.get $mentry) (i64.const 2))                    ;; tombstone the slot (NOT 0 - keeps probe chains)
      (i64.store (local.get $hash) (i64.sub (i64.load (local.get $hash)) (i64.const 1)))  ;; count -= 1
      (local.set $meta (call $__rt_hash_meta_addr (local.get $hash)))
      (local.set $used (i64.load (i32.add (local.get $meta) (i32.const 16))))
      (if (i64.eq (i64.add (local.get $removed_idx) (i64.const 1)) (local.get $used))
        (then
          (local.set $new_used (i64.const 0))                        ;; no preceding live bucket
          (if (i64.ne (local.get $prev) (i64.const -1))
            (then
              (local.set $new_used
                (i64.add (i64.load (i32.add (local.get $pe) (i32.const 64))) (i64.const 1)))))  ;; predecessor ordinal + 1
          (i64.store (i32.add (local.get $meta) (i32.const 16)) (local.get $new_used))))  ;; collapse every trailing logical hole
    ))
  (local.get $hash))
"#;

/// `__rt_hash_append`: the user-facing `$h[] = v`. Loads the persisted
/// `nNextFreeElement` in O(1), remaps the fresh `ZEND_LONG_MIN` sentinel
/// to zero, rejects an occupied saturated key without mutation, and otherwise
/// delegates ownership/COW/resize work to `__rt_hash_set`.
///
/// A zero return is an internal non-null-pointer sentinel consumed by
/// `lower_hash_append`, which reports php-src's fatal `Error` in command modules
/// and traps on the same exceptional edge in import-free reactors.
const RT_HASH_APPEND: &str = r#"(func $__rt_hash_append (param $hash i32) (param $val_lo i64) (param $val_hi i64) (param $val_tag i64) (result i32)
  (local $key i64)
  (local $found i32)
  (local.set $key (i64.load (call $__rt_hash_meta_addr (local.get $hash))))  ;; persisted nNextFreeElement
  (if (i64.eq (local.get $key) (i64.const -9223372036854775808))
    (then (local.set $key (i64.const 0))))                 ;; HASH_ADD_NEXT remaps fresh LONG_MIN to key 0
  (call $__rt_hash_get (local.get $hash) (local.get $key) (i64.const -1))
  (drop)                                                    ;; discard value_tag
  (drop)                                                    ;; discard value_hi
  (drop)                                                    ;; discard value_lo
  (local.set $found)                                        ;; append key already occupied?
  (if (local.get $found)
    (then (return (i32.const 0))))                          ;; saturated PHP_INT_MAX collision: caller raises Error
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
  (call $__rt_hash_validate_layout (local.get $b))                    ;; validate right slots/trailer before clone or iteration
  (call $__rt_hash_preflight_union
    (i64.const 0)
    (i64.load (local.get $b)))                                        ;; null-left clone still preflights the complete right result
  (if (i32.eqz (local.get $a))                                          ;; null left operand?
    (then (return (call $__rt_hash_clone_shallow (local.get $b)))))     ;; result is just a copy of b
  (call $__rt_hash_validate_layout (local.get $a))                    ;; validate left slots/trailer before clone
  (call $__rt_hash_preflight_union
    (i64.load (local.get $a))
    (i64.load (local.get $b)))                                        ;; worst-case distinct-key result before clone
  (local.set $result (call $__rt_hash_clone_shallow (local.get $a)))    ;; start from an owned copy of a
  (local.set $cur (i64.load (i32.add (local.get $b) (i32.const 24))))   ;; cur = b.head (insertion-order start)
  (block $done (loop $walk
    (br_if $done (i64.eq (local.get $cur) (i64.const -1)))              ;; end of b's insertion order
    (local.set $be (i32.add (i32.add (local.get $b) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))  ;; &b entry
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
    (br $walk)))                                                        ;; next entry
  (local.get $result))
"#;

/// `__rt_array_hash_union`: the PHP `+` operator with a DENSE INDEXED left operand and
/// an ASSOCIATIVE HASH right operand; the result is a fresh OWNED hash. The left indexed
/// entries are promoted to integer-keyed hash entries (`key 0,1,2,...`), then each right
/// hash entry whose key is not already present is appended in the right's insertion order
/// (LEFT wins on collision). Borrows both operands. Unlike native — which pre-persists
/// strings / pre-increfs containers before insertion because its `__rt_hash_set` does not
/// take ownership — the wasm `__rt_hash_set` OWNS its value (persists tag-1 strings,
/// increfs tag-4..7/10 containers), so the borrowed value words are passed straight
/// through. The result hash carries header value_type 7 (mixed), since a cross-
/// representation union may merge heterogeneous values.
const RT_ARRAY_HASH_UNION: &str = r#"(func $__rt_array_hash_union (param $a i32) (param $b i32) (result i32)
  (local $result i32)
  (local $cap i64)
  (local $avt i64)
  (local $alen i64)
  (local $i i64)
  (local $slot i32)
  (local $vlo i64)
  (local $vhi i64)
  (local $vtag i64)
  (local $cur i64)
  (local $be i32)
  (local $klo i64)
  (local $khi i64)
  (local $found i32)
  (local $acap i64)
  (local $aesz i64)
  (local $total i64)
  (local.set $alen (i64.load (local.get $a)))
  (local.set $acap (i64.load (i32.add (local.get $a) (i32.const 8))))
  (local.set $aesz (i64.load (i32.add (local.get $a) (i32.const 16))))
  (drop (call $__rt_checked_layout (local.get $acap) (local.get $aesz) (i64.const 24)))  ;; validate indexed source layout
  (if (i64.gt_u (local.get $alen) (local.get $acap))
    (then (call $__rt_oom) unreachable))                               ;; elephc-trap:deterministic-oom:array-hash-union-malformed-left malformed indexed length
  (call $__rt_hash_validate_layout (local.get $b))                    ;; validate associative source layout
  (call $__rt_hash_preflight_union (local.get $alen) (i64.load (local.get $b)))  ;; validate worst-case result before allocation
  (local.set $total (i64.add (local.get $alen) (i64.load (local.get $b))))
  (local.set $cap (i64.add (local.get $total) (local.get $total)))     ;; safe after checked worst-case union layout
  (if (i64.lt_s (local.get $cap) (i64.const 16))                        ;; below the minimum capacity?
    (then (local.set $cap (i64.const 16))))                            ;; clamp to 16
  (drop (call $__rt_checked_layout (local.get $cap) (i64.const 72) (i64.const 72)))  ;; validate minimum-capacity clamp too
  (local.set $result (call $__rt_hash_new (local.get $cap) (i64.const 7)))  ;; fresh mixed-valued result hash
  (local.set $avt (i64.and (i64.shr_u (i64.load (i32.sub (local.get $a) (i32.const 8))) (i64.const 8)) (i64.const 127)))  ;; left value_type tag
  (local.set $i (i64.const 0))                                         ;; left position cursor
  (block $lend (loop $lwalk                                            ;; promote each left indexed entry
    (br_if $lend (i64.ge_s (local.get $i) (local.get $alen)))          ;; promoted all left entries
    (if (i64.eq (local.get $avt) (i64.const 1))                        ;; string element?
      (then                                                           ;; 16-byte string slot
        (local.set $slot (i32.add (i32.add (local.get $a) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))  ;; &a string slot
        (local.set $vlo (i64.load (local.get $slot)))                  ;; zero-extended string pointer
        (local.set $vhi (i64.load (i32.add (local.get $slot) (i32.const 8))))  ;; string length
        (local.set $vtag (i64.const 1)))                              ;; value_tag = string
      (else                                                           ;; 8-byte scalar/container slot
        (local.set $slot (i32.add (i32.add (local.get $a) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8)))))  ;; &a scalar/container slot
        (local.set $vlo (i64.load (local.get $slot)))                  ;; payload bits / container pointer
        (local.set $vhi (i64.const 0))                                ;; no high word
        (local.set $vtag (local.get $avt))))                          ;; value_tag = left value_type
    (local.set $result (call $__rt_hash_set (local.get $result) (local.get $i) (i64.const -1) (local.get $vlo) (local.get $vhi) (local.get $vtag)))  ;; insert under integer key (set owns the value)
    (local.set $i (i64.add (local.get $i) (i64.const 1)))              ;; next left position
    (br $lwalk)))                                                      ;; next left entry
  (local.set $cur (i64.load (i32.add (local.get $b) (i32.const 24))))  ;; cur = b.head (insertion-order start)
  (block $rend (loop $rwalk                                           ;; merge each right hash entry
    (br_if $rend (i64.eq (local.get $cur) (i64.const -1)))             ;; end of b's insertion order
    (local.set $be (i32.add (i32.add (local.get $b) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))  ;; &b entry
    (local.set $klo (i64.load (i32.add (local.get $be) (i32.const 8))))   ;; b key_lo
    (local.set $khi (i64.load (i32.add (local.get $be) (i32.const 16))))  ;; b key_hi
    (call $__rt_hash_get (local.get $result) (local.get $klo) (local.get $khi))  ;; already present in result?
    (drop)                                                            ;; discard value_tag
    (drop)                                                            ;; discard value_hi
    (drop)                                                            ;; discard value_lo
    (local.set $found)                                                ;; keep the found flag
    (if (i32.eqz (local.get $found))                                  ;; key absent -> take b's entry
      (then
        (local.set $vlo (i64.load (i32.add (local.get $be) (i32.const 24))))   ;; b value_lo
        (local.set $vhi (i64.load (i32.add (local.get $be) (i32.const 32))))   ;; b value_hi
        (local.set $vtag (i64.load (i32.add (local.get $be) (i32.const 40))))  ;; b value_tag
        (local.set $result (call $__rt_hash_set (local.get $result) (local.get $klo) (local.get $khi) (local.get $vlo) (local.get $vhi) (local.get $vtag)))))  ;; append b's entry (set owns the value)
    (local.set $cur (i64.load (i32.add (local.get $be) (i32.const 56))))  ;; cur = b entry.next
    (br $rwalk)))                                                      ;; next right entry
  (local.get $result))
"#;

/// `__rt_hash_array_union`: the PHP `+` operator with an ASSOCIATIVE HASH left operand
/// and a DENSE INDEXED right operand; the result is a fresh OWNED hash. It starts as a
/// deep clone of the left hash (left/order wins), then each right indexed entry whose
/// integer position is not already a key of the left is appended under that integer key.
/// Borrows both operands. As with `__rt_array_hash_union`, the borrowed value words are
/// passed straight to the value-owning `__rt_hash_set` (no pre-persist / pre-incref).
const RT_HASH_ARRAY_UNION: &str = r#"(func $__rt_hash_array_union (param $a i32) (param $b i32) (result i32)
  (local $result i32)
  (local $bvt i64)
  (local $blen i64)
  (local $i i64)
  (local $found i32)
  (local $slot i32)
  (local $vlo i64)
  (local $vhi i64)
  (local $vtag i64)
  (local $bcap i64)
  (local $besz i64)
  (call $__rt_hash_validate_layout (local.get $a))                    ;; validate left slots/trailer before clone
  (local.set $blen (i64.load (local.get $b)))
  (local.set $bcap (i64.load (i32.add (local.get $b) (i32.const 8))))
  (local.set $besz (i64.load (i32.add (local.get $b) (i32.const 16))))
  (drop (call $__rt_checked_layout (local.get $bcap) (local.get $besz) (i64.const 24)))  ;; validate indexed source layout
  (if (i64.gt_u (local.get $blen) (local.get $bcap))
    (then (call $__rt_oom) unreachable))                               ;; elephc-trap:deterministic-oom:hash-array-union-malformed-right malformed indexed length
  (call $__rt_hash_preflight_union (i64.load (local.get $a)) (local.get $blen))  ;; worst-case distinct-key result before clone
  (local.set $result (call $__rt_hash_clone_shallow (local.get $a)))   ;; own a copy of the left hash
  (local.set $bvt (i64.and (i64.shr_u (i64.load (i32.sub (local.get $b) (i32.const 8))) (i64.const 8)) (i64.const 127)))  ;; right value_type tag
  (local.set $i (i64.const 0))                                         ;; right position cursor
  (block $done (loop $walk                                             ;; consider each right index
    (br_if $done (i64.ge_s (local.get $i) (local.get $blen)))          ;; considered every right index
    (call $__rt_hash_get (local.get $result) (local.get $i) (i64.const -1))  ;; integer key already present?
    (drop)                                                            ;; discard value_tag
    (drop)                                                            ;; discard value_hi
    (drop)                                                            ;; discard value_lo
    (local.set $found)                                                ;; keep the found flag
    (if (i32.eqz (local.get $found))                                  ;; key absent on the left -> append b's entry
      (then
        (if (i64.eq (local.get $bvt) (i64.const 1))                   ;; string element?
          (then                                                       ;; 16-byte string slot
            (local.set $slot (i32.add (i32.add (local.get $b) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))  ;; &b string slot
            (local.set $vlo (i64.load (local.get $slot)))             ;; zero-extended string pointer
            (local.set $vhi (i64.load (i32.add (local.get $slot) (i32.const 8))))  ;; string length
            (local.set $vtag (i64.const 1)))                          ;; value_tag = string
          (else                                                       ;; 8-byte scalar/container slot
            (local.set $slot (i32.add (i32.add (local.get $b) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8)))))  ;; &b scalar/container slot
            (local.set $vlo (i64.load (local.get $slot)))             ;; payload bits / container pointer
            (local.set $vhi (i64.const 0))                            ;; no high word
            (local.set $vtag (local.get $bvt))))                      ;; value_tag = right value_type
        (local.set $result (call $__rt_hash_set (local.get $result) (local.get $i) (i64.const -1) (local.get $vlo) (local.get $vhi) (local.get $vtag)))))  ;; append under integer key (set owns the value)
    (local.set $i (i64.add (local.get $i) (i64.const 1)))             ;; next right index
    (br $walk)))                                                      ;; next right entry
  (local.get $result))
"#;

/// `__rt_array_to_hash`: consumes an indexed-array value and returns associative
/// hash storage with the same integer-keyed entries. An already-promoted kind-3
/// value is forwarded unchanged. The `empty_next_zero` flag carries the sole
/// version-sensitive provenance: an empty literal promoted under PHP 8.2 keeps
/// immutable `zend_empty_array.nNextFreeElement == 0`; every mutable `HashNew`
/// and every PHP 8.3+ empty-literal promotion starts from `ZEND_LONG_MIN`.
///
/// Indexed mutators that can empty a previously non-empty array (`array_pop`,
/// `array_shift`, and related runtime calls) remain capability-rejected on WASM.
/// Before admitting one, the indexed representation must carry its runtime
/// next-key history instead of reusing this compile-profile literal provenance.
const RT_ARRAY_TO_HASH: &str = r#"(func $__rt_array_to_hash (param $array i32) (param $empty_next_zero i32) (result i32)
  (local $kind i32)
  (local $empty i32)
  (local $result i32)
  (local.set $kind
    (i32.and
      (i32.wrap_i64 (i64.load (i32.sub (local.get $array) (i32.const 8))))
      (i32.const 255)))                                          ;; low byte of heap kind
  (if (i32.eq (local.get $kind) (i32.const 3))
    (then (return (local.get $array))))                           ;; already promoted
  (if (i32.ne (local.get $kind) (i32.const 2))
    (then (call $__rt_oom) unreachable))                         ;; elephc-trap:deterministic-oom:array-to-hash-malformed-source malformed ArrayToHash source
  (local.set $empty (call $__rt_hash_new (i64.const 0) (i64.const 7)))  ;; empty borrowed right operand
  (local.set $result (call $__rt_array_hash_union (local.get $array) (local.get $empty)))  ;; copy indexed entries
  (call $__rt_decref_hash (local.get $empty))                     ;; release temporary empty hash
  (call $__rt_decref_array (local.get $array))                    ;; conversion consumes indexed source
  (if (i32.and (local.get $empty_next_zero) (i64.eqz (i64.load (local.get $result))))
    (then
      (i64.store (call $__rt_hash_meta_addr (local.get $result)) (i64.const 0))))  ;; PHP 8.2 immutable-empty origin
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
  (local.set $i (i64.const 0))                               ;; slot cursor = 0
  (block $end (loop $slot
    (br_if $end (i64.ge_u (local.get $i) (local.get $capacity)))  ;; visited every slot
    (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 72)))))  ;; &slot[i]
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
    (local.set $i (i64.add (local.get $i) (i64.const 1)))    ;; next slot
    (br $slot)))                                             ;; next slot
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

    use super::emit_hash_runtime_for_version;
    use super::super::arrays::emit_array_runtime;
    use super::super::heap::emit_heap_runtime;
    use super::super::classes::{emit_class_metadata_stub, emit_class_runtime};
    use super::super::mixed::emit_mixed_runtime;
    use super::super::objects::{emit_destructor_dispatch_stub, emit_gc_desc_stub, emit_object_runtime};
    use super::super::refcount::emit_refcount_runtime;
    use super::super::closures::emit_closure_runtime;
    use super::super::wat::WatModule;
    use crate::web_prelude::PhpVersion;
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
        run_driver_for_version(driver, export, PhpVersion::Php85)
    }

    /// Builds and runs a hash-runtime driver for the selected PHP compatibility
    /// profile so version-dependent Zend state can be checked explicitly.
    fn run_driver_for_version(
        driver: &str,
        export: &str,
        php_version: PhpVersion,
    ) -> Option<String> {
        let mut wm = WatModule::new();
        wm.set_memory(4, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 4 * 65536);
        emit_refcount_runtime(&mut wm);
        emit_closure_runtime(&mut wm);
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm, false);
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
        emit_hash_runtime_for_version(&mut wm, php_version);
        emit_object_runtime(&mut wm);
        emit_gc_desc_stub(&mut wm);
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

    /// Builds and validates a hash-runtime reactor whose driver must terminate
    /// through the deterministic reactor OOM trap.
    fn driver_traps(driver: &str, export: &str) {
        let mut wm = WatModule::new();
        wm.set_memory(4, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 4 * 65536);
        emit_refcount_runtime(&mut wm);
        emit_closure_runtime(&mut wm);
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm, false);
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
        emit_hash_runtime_for_version(&mut wm, PhpVersion::Php85);
        emit_object_runtime(&mut wm);
        emit_gc_desc_stub(&mut wm);
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
            return;
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
            !out.status.success(),
            "overflowing hash driver unexpectedly succeeded\n{wat}"
        );
    }

    /// Hash allocation, resize, and union bounds reject maximal inputs without
    /// allocating a multi-gigabyte linear-memory region or looping on a shift.
    #[test]
    fn oversized_hash_layouts_trap_without_large_allocation() {
        driver_traps(
            r#"(func $t (export "t")
  (drop (call $__rt_hash_new
    (i64.const 9223372036854775807)
    (i64.const 0))))"#,
            "t",
        );
        driver_traps(
            r#"(func $t (export "t")
  (local $h i32)
  (local.set $h (call $__rt_hash_new (i64.const 4) (i64.const 0)))
  (i64.store (i32.add (local.get $h) (i32.const 8)) (i64.const 9223372036854775807))
  (drop (call $__rt_hash_resize (local.get $h))))"#,
            "t",
        );
        driver_traps(
            r#"(func $t (export "t")
  (call $__rt_hash_preflight_union
    (i64.const 9223372036854775807)
    (i64.const 1)))"#,
            "t",
        );
    }

    /// All mutating hash paths validate layout/string lengths before COW, and all
    /// three union forms preflight their worst-case result before clone/allocation.
    #[test]
    fn hash_mutation_and_union_preflights_precede_observable_work() {
        let set_validate = super::RT_HASH_SET
            .find("call $__rt_hash_validate_layout")
            .expect("hash layout validation");
        let set_string = super::RT_HASH_SET
            .find("call $__rt_checked_layout")
            .expect("hash string validation");
        let set_cow = super::RT_HASH_SET
            .find("call $__rt_hash_ensure_unique")
            .expect("hash COW");
        assert!(set_validate < set_cow);
        assert!(set_string < set_cow);

        let hash_preflight = super::RT_HASH_UNION
            .find("call $__rt_hash_preflight_union")
            .expect("hash+hash preflight");
        let hash_clone = super::RT_HASH_UNION
            .find("call $__rt_hash_clone_shallow")
            .expect("hash+hash clone");
        assert!(hash_preflight < hash_clone);

        let array_hash_preflight = super::RT_ARRAY_HASH_UNION
            .find("call $__rt_hash_preflight_union")
            .expect("array+hash preflight");
        let array_hash_new = super::RT_ARRAY_HASH_UNION
            .find("call $__rt_hash_new")
            .expect("array+hash allocation");
        assert!(array_hash_preflight < array_hash_new);

        let hash_array_preflight = super::RT_HASH_ARRAY_UNION
            .find("call $__rt_hash_preflight_union")
            .expect("hash+array preflight");
        let hash_array_clone = super::RT_HASH_ARRAY_UNION
            .find("call $__rt_hash_clone_shallow")
            .expect("hash+array clone");
        assert!(hash_array_preflight < hash_array_clone);
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

    /// A boxed 1e20 float key uses PHP's modulo-2^64 integer result rather than
    /// WebAssembly's saturating conversion; the second result remains the -1
    /// integer-key marker.
    #[test]
    fn mixed_float_key_wraps_out_of_range_like_php() {
        let bits = 1.0e20f64.to_bits() as i64;
        let driver = format!(
            r#"(func $t (export "t") (result i64)
  (local $key i64) (local $marker i64)
  (call $__rt_hash_key_from_mixed
    (call $__rt_mixed_from_value (i64.const 2) (i64.const {bits}) (i64.const 0)))
  (local.set $marker)
  (local.set $key)
  (if (i64.eq (local.get $marker) (i64.const -1))
    (then (return (local.get $key))))
  (i64.const 0))"#
        );
        if let Some(output) = run_driver(&driver, "t") {
            assert_eq!(output, "7766279631452241920");
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
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 42) (i64.const -1) (i64.const 100) (i64.const 0) (i64.const 0) (i64.const 0)))
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
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 2) (i64.const -1) (i64.const 20) (i64.const 0) (i64.const 0) (i64.const 1)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 3) (i64.const -1) (i64.const 30) (i64.const 0) (i64.const 0) (i64.const 2)))
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
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 2) (i64.const -1) (i64.const 20) (i64.const 0) (i64.const 0) (i64.const 1)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 3) (i64.const -1) (i64.const 30) (i64.const 0) (i64.const 0) (i64.const 2)))
  (local.set $h (call $__rt_hash_resize (local.get $h)))
  (local.set $head (i64.load (i32.add (local.get $h) (i32.const 24))))
  (local.set $headkey (i64.load (i32.add (i32.add (i32.add (local.get $h) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $head) (i64.const 72)))) (i32.const 8))))
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
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 2) (i64.const -1) (i64.const 20) (i64.const 0) (i64.const 0) (i64.const 1)))
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
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 1) (i64.const -1) (i64.extend_i32_u (local.get $sp)) (local.get $sl) (i64.const 1) (i64.const 0)))
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
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 1) (i64.const -1) (i64.const 9) (i64.const 0) (i64.const 0) (i64.const 0)))
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
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 10) (i64.const -1) (i64.const 0) (i64.const 0) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 20) (i64.const -1) (i64.const 0) (i64.const 0) (i64.const 0) (i64.const 1)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 30) (i64.const -1) (i64.const 0) (i64.const 0) (i64.const 0) (i64.const 2)))
  (local.set $c (i64.const -2))
  (local.set $acc (i64.const 0))
  (block $end (loop $L
    (call $__rt_hash_iter_next (local.get $h) (local.get $c))
    (local.set $m)
    (local.set $c)
    (br_if $end (i64.eqz (local.get $m)))
    (local.set $entry (i32.add (i32.add (local.get $h) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $c) (i64.const 72)))))
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

    /// `__rt_hash_append` uses the persisted next-free key, not the live entry count:
    /// appending 100 (key 0) then 200 (key 1), explicitly setting key 5 = 500, then
    /// appending 999 places it at key 6. Reading key 6 returns
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

    /// A directly allocated mutable HashTable starts at `ZEND_LONG_MIN` in every
    /// supported php-src profile. Therefore `[-3 => 1]; $a[] = 99` appends at -2
    /// under PHP 8.2, 8.3, 8.4, and 8.5. The bitfield verifies both the fresh
    /// trailer sentinel and the appended key/value.
    #[test]
    fn mutable_hash_negative_first_key_uses_long_min_in_every_profile() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local $sentinel i32)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $sentinel
    (i64.eq
      (i64.load (call $__rt_hash_meta_addr (local.get $h)))
      (i64.const -9223372036854775808)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -3) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_append (local.get $h) (i64.const 99) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const -2) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add
    (i64.add
      (i64.mul (i64.extend_i32_u (local.get $sentinel)) (i64.const 200))
      (i64.mul (i64.extend_i32_u (local.get $found)) (i64.const 100)))
    (local.get $vlo)))"#;
        for version in [
            PhpVersion::Php82,
            PhpVersion::Php83,
            PhpVersion::Php84,
            PhpVersion::Php85,
        ] {
            if let Some(output) = run_driver_for_version(driver, "t", version) {
                assert_eq!(output, "399", "profile {version:?}");
            }
        }
    }

    /// `ArrayToHash` carries the PHP 8.2 immutable-empty origin explicitly:
    /// promoting `[]`, setting -3, and appending uses key 0 only when the flag is
    /// set; the PHP 8.3+ origin appends at -2. A direct mutable HashNew remains
    /// covered independently by `mutable_hash_negative_first_key_uses_long_min_in_every_profile`.
    #[test]
    fn empty_indexed_promotion_preserves_php82_next_zero_origin() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $h i32) (local $php82_value i64) (local $newer_value i64)
  (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $a (call $__rt_array_new (i64.const 0) (i64.const 16)))
  (local.set $h (call $__rt_array_to_hash (local.get $a) (i32.const 1)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -3) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_append (local.get $h) (i64.const 99) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const 0) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (local.set $php82_value
    (i64.add (i64.mul (i64.extend_i32_u (local.get $found)) (i64.const 100)) (local.get $vlo)))
  (local.set $a (call $__rt_array_new (i64.const 0) (i64.const 16)))
  (local.set $h (call $__rt_array_to_hash (local.get $a) (i32.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -3) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_append (local.get $h) (i64.const 99) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const -2) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (local.set $newer_value
    (i64.add (i64.mul (i64.extend_i32_u (local.get $found)) (i64.const 100)) (local.get $vlo)))
  (i64.add (i64.mul (local.get $php82_value) (i64.const 1000)) (local.get $newer_value)))"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "199199");
        }
    }

    /// Unsetting or overwriting key 5 never rolls back its persisted history:
    /// each independent hash appends at key 6. The two appended values fold to
    /// `11*1000 + 22` = 11022.
    #[test]
    fn append_history_survives_unset_and_overwrite() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32) (local $found i32)
  (local $vlo i64) (local $vhi i64) (local $vtag i64) (local $av i64) (local $bv i64)
  (local.set $a (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $a (call $__rt_hash_set (local.get $a) (i64.const 5) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $a (call $__rt_hash_unset (local.get $a) (i64.const 5) (i64.const -1)))
  (local.set $a (call $__rt_hash_append (local.get $a) (i64.const 11) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $a) (i64.const 6) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $av) (local.set $found)
  (local.set $b (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $b (call $__rt_hash_set (local.get $b) (i64.const 5) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $b (call $__rt_hash_set (local.get $b) (i64.const 5) (i64.const -1) (i64.const 2) (i64.const 0) (i64.const 0)))
  (local.set $b (call $__rt_hash_append (local.get $b) (i64.const 22) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $b) (i64.const 6) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $bv) (local.set $found)
  (i64.add (i64.mul (local.get $av) (i64.const 1000)) (local.get $bv)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "11022");
        }
    }

    /// Reusing an emptied PACKED hash with key 2 resets append to 3, whereas an
    /// emptied MIXED hash retains its former high-water mark 6. Appended values
    /// at those keys fold to `70*1000 + 80` = 70080.
    #[test]
    fn packed_hole_reuse_and_mixed_hole_history_match_php() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $p i32) (local $m i32) (local $found i32)
  (local $vlo i64) (local $vhi i64) (local $vtag i64) (local $pv i64) (local $mv i64)
  (i32.store8 (i32.const 300) (i32.const 120))
  (local.set $p (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $p (call $__rt_hash_set (local.get $p) (i64.const 5) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $p (call $__rt_hash_unset (local.get $p) (i64.const 5) (i64.const -1)))
  (local.set $p (call $__rt_hash_set (local.get $p) (i64.const 2) (i64.const -1) (i64.const 2) (i64.const 0) (i64.const 0)))
  (local.set $p (call $__rt_hash_append (local.get $p) (i64.const 70) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $p) (i64.const 3) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $pv) (local.set $found)
  (local.set $m (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $m (call $__rt_hash_set (local.get $m) (i64.const 300) (i64.const 1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $m (call $__rt_hash_set (local.get $m) (i64.const 5) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $m (call $__rt_hash_unset (local.get $m) (i64.const 300) (i64.const 1)))
  (local.set $m (call $__rt_hash_unset (local.get $m) (i64.const 5) (i64.const -1)))
  (local.set $m (call $__rt_hash_set (local.get $m) (i64.const 2) (i64.const -1) (i64.const 2) (i64.const 0) (i64.const 0)))
  (local.set $m (call $__rt_hash_append (local.get $m) (i64.const 80) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $m) (i64.const 6) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $mv) (local.set $found)
  (i64.add (i64.mul (local.get $pv) (i64.const 1000)) (local.get $mv)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "70080");
        }
    }

    /// Resizing and a refcount-triggered empty COW split both preserve next-free 6.
    /// The resized table's append and the COW clone's append therefore produce
    /// values 61 and 62 at key 6, folded as `61*1000 + 62` = 61062.
    #[test]
    fn append_history_survives_resize_and_empty_cow() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $r i32) (local $a i32) (local $b i32) (local $found i32)
  (local $vlo i64) (local $vhi i64) (local $vtag i64) (local $rv i64) (local $bv i64)
  (local.set $r (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $r (call $__rt_hash_set (local.get $r) (i64.const 0) (i64.const -1) (i64.const 0) (i64.const 0) (i64.const 0)))
  (local.set $r (call $__rt_hash_set (local.get $r) (i64.const 1) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $r (call $__rt_hash_set (local.get $r) (i64.const 2) (i64.const -1) (i64.const 2) (i64.const 0) (i64.const 0)))
  (local.set $r (call $__rt_hash_set (local.get $r) (i64.const 3) (i64.const -1) (i64.const 3) (i64.const 0) (i64.const 0)))
  (local.set $r (call $__rt_hash_set (local.get $r) (i64.const 4) (i64.const -1) (i64.const 4) (i64.const 0) (i64.const 0)))
  (local.set $r (call $__rt_hash_set (local.get $r) (i64.const 5) (i64.const -1) (i64.const 5) (i64.const 0) (i64.const 0)))
  (local.set $r (call $__rt_hash_append (local.get $r) (i64.const 61) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $r) (i64.const 6) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $rv) (local.set $found)
  (local.set $a (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $a (call $__rt_hash_set (local.get $a) (i64.const 5) (i64.const -1) (i64.const 5) (i64.const 0) (i64.const 0)))
  (local.set $a (call $__rt_hash_unset (local.get $a) (i64.const 5) (i64.const -1)))
  (call $__rt_incref (local.get $a))
  (local.set $b (call $__rt_hash_append (local.get $a) (i64.const 62) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $b) (i64.const 6) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $bv) (local.set $found)
  (i64.add (i64.mul (local.get $rv) (i64.const 1000)) (local.get $bv)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "61062");
        }
    }

    /// An empty clone resets logical table size to `HT_MIN_SIZE=8` but copies
    /// next-free 16. Reusing key 8 therefore initializes MIXED and preserves 16,
    /// so the following append is found at key 16 with value 88.
    #[test]
    fn empty_clone_resets_table_size_but_preserves_high_history() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $c i32) (local $i i64)
  (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $i (i64.const 0))
  (block $inserted (loop $insert
    (br_if $inserted (i64.ge_u (local.get $i) (i64.const 16)))
    (local.set $h (call $__rt_hash_set (local.get $h) (local.get $i) (i64.const -1) (local.get $i) (i64.const 0) (i64.const 0)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $insert)))
  (local.set $i (i64.const 0))
  (block $removed (loop $remove
    (br_if $removed (i64.ge_u (local.get $i) (i64.const 16)))
    (local.set $h (call $__rt_hash_unset (local.get $h) (local.get $i) (i64.const -1)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $remove)))
  (local.set $c (call $__rt_hash_clone_shallow (local.get $h)))
  (local.set $c (call $__rt_hash_set (local.get $c) (i64.const 8) (i64.const -1) (i64.const 8) (i64.const 0) (i64.const 0)))
  (local.set $c (call $__rt_hash_append (local.get $c) (i64.const 88) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $c) (i64.const 16) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.mul (i64.extend_i32_u (local.get $found)) (i64.const 100)) (local.get $vlo)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "188");
        }
    }

    /// Unioning into an empty high-history left array follows php-src's clone
    /// reinitialization: a first positive key 2 resets append to 3, while a first
    /// string key keeps next-free 6. Values at those append keys fold to 33044.
    #[test]
    fn union_reinitialization_depends_on_first_right_key_kind() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $left i32) (local $ri i32) (local $rs i32) (local $ui i32) (local $us i32)
  (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local $iv i64) (local $sv i64)
  (i32.store8 (i32.const 300) (i32.const 120))
  (local.set $left (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $left (call $__rt_hash_set (local.get $left) (i64.const 5) (i64.const -1) (i64.const 5) (i64.const 0) (i64.const 0)))
  (local.set $left (call $__rt_hash_unset (local.get $left) (i64.const 5) (i64.const -1)))
  (local.set $ri (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $ri (call $__rt_hash_set (local.get $ri) (i64.const 2) (i64.const -1) (i64.const 2) (i64.const 0) (i64.const 0)))
  (local.set $ui (call $__rt_hash_union (local.get $left) (local.get $ri)))
  (local.set $ui (call $__rt_hash_append (local.get $ui) (i64.const 33) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $ui) (i64.const 3) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $iv) (local.set $found)
  (local.set $rs (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $rs (call $__rt_hash_set (local.get $rs) (i64.const 300) (i64.const 1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $us (call $__rt_hash_union (local.get $left) (local.get $rs)))
  (local.set $us (call $__rt_hash_append (local.get $us) (i64.const 44) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $us) (i64.const 6) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $sv) (local.set $found)
  (i64.add (i64.mul (local.get $iv) (i64.const 1000)) (local.get $sv)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "33044");
        }
    }

    /// A canonical numeric string key `"5"` advances integer append to 6, while
    /// non-canonical `"05"` remains a string and leaves append at 0. Appended
    /// values at those keys fold to `55*1000 + 66` = 55066.
    #[test]
    fn normalized_numeric_string_keys_affect_append_like_php() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32) (local $lo i64) (local $hi i64)
  (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local $av i64) (local $bv i64)
  (i32.store8 (i32.const 300) (i32.const 53))
  (i32.store8 (i32.const 310) (i32.const 48))
  (i32.store8 (i32.const 311) (i32.const 53))
  (local.set $a (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (call $__rt_hash_normalize_key (i32.const 300) (i64.const 1))
  (local.set $hi) (local.set $lo)
  (local.set $a (call $__rt_hash_set (local.get $a) (local.get $lo) (local.get $hi) (i64.const 5) (i64.const 0) (i64.const 0)))
  (local.set $a (call $__rt_hash_append (local.get $a) (i64.const 55) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $a) (i64.const 6) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $av) (local.set $found)
  (local.set $b (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (call $__rt_hash_normalize_key (i32.const 310) (i64.const 2))
  (local.set $hi) (local.set $lo)
  (local.set $b (call $__rt_hash_set (local.get $b) (local.get $lo) (local.get $hi) (i64.const 5) (i64.const 0) (i64.const 0)))
  (local.set $b (call $__rt_hash_append (local.get $b) (i64.const 66) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $b) (i64.const 0) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $bv) (local.set $found)
  (i64.add (i64.mul (local.get $av) (i64.const 1000)) (local.get $bv)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "55066");
        }
    }

    /// Inserting `PHP_INT_MAX` saturates next-free without wrapping. Append returns
    /// the null failure sentinel while MAX is occupied; after unset it reuses MAX
    /// once, and the following append fails again. The result bitfield is 7.
    #[test]
    fn append_at_php_int_max_saturates_and_never_wraps() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $h i32) (local $first i32) (local $reuse i32) (local $second i32)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 9223372036854775807) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $first (call $__rt_hash_append (local.get $h) (i64.const 2) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 9223372036854775807) (i64.const -1)))
  (local.set $reuse (call $__rt_hash_append (local.get $h) (i64.const 3) (i64.const 0) (i64.const 0)))
  (local.set $second (call $__rt_hash_append (local.get $reuse) (i64.const 4) (i64.const 0) (i64.const 0)))
  (i32.add
    (i32.add
      (i32.mul (i32.eqz (local.get $first)) (i32.const 4))
      (i32.mul (i32.ne (local.get $reuse) (i32.const 0)) (i32.const 2)))
    (i32.eqz (local.get $second))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "7");
        }
    }

    /// Mixed-table deletion follows php-src's trailing-hole rule. Removing an
    /// interior bucket keeps `nNumUsed=4`; deleting the live tail collapses over
    /// that hole to 3, the next tail deletion collapses to 1, and deleting the
    /// final bucket resets it to 0. The decimal fold is therefore 4310.
    #[test]
    fn mixed_unset_collapses_only_trailing_logical_holes() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $meta i32)
  (local $u1 i64) (local $u2 i64) (local $u3 i64) (local $u4 i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -10) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -9) (i64.const -1) (i64.const 2) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -8) (i64.const -1) (i64.const 3) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -7) (i64.const -1) (i64.const 4) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const -9) (i64.const -1)))
  (local.set $meta (call $__rt_hash_meta_addr (local.get $h)))
  (local.set $u1 (i64.load (i32.add (local.get $meta) (i32.const 16))))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const -7) (i64.const -1)))
  (local.set $u2 (i64.load (i32.add (local.get $meta) (i32.const 16))))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const -8) (i64.const -1)))
  (local.set $u3 (i64.load (i32.add (local.get $meta) (i32.const 16))))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const -10) (i64.const -1)))
  (local.set $u4 (i64.load (i32.add (local.get $meta) (i32.const 16))))
  (i64.add
    (i64.add (i64.mul (local.get $u1) (i64.const 1000)) (i64.mul (local.get $u2) (i64.const 100)))
    (i64.add (i64.mul (local.get $u3) (i64.const 10)) (local.get $u4))))"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "4310");
        }
    }

    /// A full logical table with size 8, used 8, and live count 7 satisfies
    /// `used > count + (count >> 5)`, so php-src compacts without growing. The
    /// new entry gets dense ordinal 7 and the table remains MIXED size 8.
    #[test]
    fn mixed_logical_resize_compacts_size_eight_without_growth() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $i i64) (local $meta i32) (local $tail_entry i32) (local $ok i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $i (i64.const 0))
  (block $done (loop $fill
    (br_if $done (i64.ge_u (local.get $i) (i64.const 8)))
    (local.set $h (call $__rt_hash_set (local.get $h)
      (i64.add (i64.const -100) (local.get $i)) (i64.const -1)
      (local.get $i) (i64.const 0) (i64.const 0)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $fill)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const -99) (i64.const -1)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -92) (i64.const -1) (i64.const 99) (i64.const 0) (i64.const 0)))
  (local.set $meta (call $__rt_hash_meta_addr (local.get $h)))
  (local.set $tail_entry (i32.add (i32.add (local.get $h) (i32.const 40))
    (i32.wrap_i64 (i64.mul (i64.load (i32.add (local.get $h) (i32.const 32))) (i64.const 72)))))
  (local.set $ok (i64.mul (i64.extend_i32_u (i64.eq (i64.load (local.get $h)) (i64.const 8))) (i64.const 16)))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 8))) (i64.const 2))) (i64.const 8))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 16))) (i64.const 8))) (i64.const 4))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 24))) (i64.const 8))) (i64.const 2))))
  (i64.add (local.get $ok) (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $tail_entry) (i32.const 64))) (i64.const 7)))))"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "31");
        }
    }

    /// A full logical size-64 table with 63 live buckets does not satisfy the
    /// compaction threshold (`64 > 63 + 1` is false). php-src doubles to 128,
    /// compacts the hole, and reserves dense ordinal 63 for the new key.
    #[test]
    fn mixed_logical_resize_grows_size_sixty_four_at_threshold() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $i i64) (local $meta i32) (local $tail_entry i32) (local $ok i64)
  (local.set $h (call $__rt_hash_new (i64.const 64) (i64.const 0)))
  (local.set $i (i64.const 0))
  (block $done (loop $fill
    (br_if $done (i64.ge_u (local.get $i) (i64.const 64)))
    (local.set $h (call $__rt_hash_set (local.get $h)
      (i64.add (i64.const -1000) (local.get $i)) (i64.const -1)
      (local.get $i) (i64.const 0) (i64.const 0)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $fill)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const -999) (i64.const -1)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -936) (i64.const -1) (i64.const 99) (i64.const 0) (i64.const 0)))
  (local.set $meta (call $__rt_hash_meta_addr (local.get $h)))
  (local.set $tail_entry (i32.add (i32.add (local.get $h) (i32.const 40))
    (i32.wrap_i64 (i64.mul (i64.load (i32.add (local.get $h) (i32.const 32))) (i64.const 72)))))
  (local.set $ok (i64.mul (i64.extend_i32_u (i64.eq (i64.load (local.get $h)) (i64.const 64))) (i64.const 16)))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 8))) (i64.const 2))) (i64.const 8))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 16))) (i64.const 64))) (i64.const 4))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 24))) (i64.const 128))) (i64.const 2))))
  (i64.add (local.get $ok) (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $tail_entry) (i32.const 64))) (i64.const 63)))))"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "31");
        }
    }

    /// Physical load-factor resize is transparent to Zend state. A MIXED table
    /// with ordinal hole 1 keeps next=-6, mode=2, used=4, size=8, and live
    /// ordinals 0,2,3 after its probe capacity doubles from 8 to 16.
    #[test]
    fn physical_resize_preserves_zend_trailer_and_ordinals() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $meta i32) (local $cur i64) (local $entry i32) (local $ok i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -10) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -9) (i64.const -1) (i64.const 2) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -8) (i64.const -1) (i64.const 3) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -7) (i64.const -1) (i64.const 4) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const -9) (i64.const -1)))
  (local.set $h (call $__rt_hash_resize (local.get $h)))
  (local.set $meta (call $__rt_hash_meta_addr (local.get $h)))
  (local.set $ok (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $h) (i32.const 8))) (i64.const 16))) (i64.const 64)))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (local.get $meta)) (i64.const -6))) (i64.const 32))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 8))) (i64.const 2))) (i64.const 16))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 16))) (i64.const 4))) (i64.const 8))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 24))) (i64.const 8))) (i64.const 4))))
  (local.set $cur (i64.load (i32.add (local.get $h) (i32.const 24))))
  (local.set $entry (i32.add (i32.add (local.get $h) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eqz (i64.load (i32.add (local.get $entry) (i32.const 64))))) (i64.const 2))))
  (local.set $cur (i64.load (i32.add (local.get $entry) (i32.const 56))))
  (local.set $entry (i32.add (i32.add (local.get $h) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))
  (local.set $ok (i64.add (local.get $ok) (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $entry) (i32.const 64))) (i64.const 2)))))
  (local.get $ok))"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "127");
        }
    }

    /// COW duplication of MIXED storage compacts logical holes in insertion
    /// order. The source keeps used=4 and ordinals 0,2,3; the clone gets used=3
    /// and dense ordinals 0,1,2 while preserving next=-6 and size=8.
    #[test]
    fn mixed_cow_compacts_logical_holes() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $c i32) (local $meta i32) (local $cur i64) (local $entry i32) (local $ok i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -10) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -9) (i64.const -1) (i64.const 2) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -8) (i64.const -1) (i64.const 3) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const -7) (i64.const -1) (i64.const 4) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const -9) (i64.const -1)))
  (call $__rt_incref (local.get $h))
  (local.set $c (call $__rt_hash_ensure_unique (local.get $h)))
  (local.set $ok (i64.mul (i64.extend_i32_u (i32.ne (local.get $h) (local.get $c))) (i64.const 256)))
  (local.set $meta (call $__rt_hash_meta_addr (local.get $h)))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 16))) (i64.const 4))) (i64.const 128))))
  (local.set $meta (call $__rt_hash_meta_addr (local.get $c)))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 8))) (i64.const 2))) (i64.const 64))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 16))) (i64.const 3))) (i64.const 32))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 24))) (i64.const 8))) (i64.const 16))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (local.get $meta)) (i64.const -6))) (i64.const 8))))
  (local.set $cur (i64.load (i32.add (local.get $c) (i32.const 24))))
  (local.set $entry (i32.add (i32.add (local.get $c) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eqz (i64.load (i32.add (local.get $entry) (i32.const 64))))) (i64.const 4))))
  (local.set $cur (i64.load (i32.add (local.get $entry) (i32.const 56))))
  (local.set $entry (i32.add (i32.add (local.get $c) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $entry) (i32.const 64))) (i64.const 1))) (i64.const 2))))
  (local.set $cur (i64.load (i32.add (local.get $entry) (i32.const 56))))
  (local.set $entry (i32.add (i32.add (local.get $c) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))
  (local.set $ok (i64.add (local.get $ok) (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $entry) (i32.const 64))) (i64.const 2)))))
  (local.get $ok))"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "511");
        }
    }

    /// PACKED COW preserves implicit holes rather than compacting. After deleting
    /// key 1 from keys 0,1,2, the clone remains mode=PACKED, used=3, count=2,
    /// with its surviving head/tail ordinals still 0 and 2.
    #[test]
    fn packed_cow_preserves_implicit_holes() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $c i32) (local $meta i32) (local $entry i32) (local $ok i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 0) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 1) (i64.const -1) (i64.const 11) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 2) (i64.const -1) (i64.const 12) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 1) (i64.const -1)))
  (call $__rt_incref (local.get $h))
  (local.set $c (call $__rt_hash_ensure_unique (local.get $h)))
  (local.set $meta (call $__rt_hash_meta_addr (local.get $c)))
  (local.set $ok (i64.mul (i64.extend_i32_u (i32.ne (local.get $h) (local.get $c))) (i64.const 64)))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 8))) (i64.const 1))) (i64.const 32))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 16))) (i64.const 3))) (i64.const 16))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (local.get $c)) (i64.const 2))) (i64.const 8))))
  (local.set $entry (i32.add (i32.add (local.get $c) (i32.const 40))
    (i32.wrap_i64 (i64.mul (i64.load (i32.add (local.get $c) (i32.const 24))) (i64.const 72)))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eqz (i64.load (i32.add (local.get $entry) (i32.const 64))))) (i64.const 4))))
  (local.set $entry (i32.add (i32.add (local.get $c) (i32.const 40))
    (i32.wrap_i64 (i64.mul (i64.load (i32.add (local.get $c) (i32.const 32))) (i64.const 72)))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $entry) (i32.const 64))) (i64.const 2))) (i64.const 2))))
  (i64.add (local.get $ok) (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 24))) (i64.const 8)))))"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "127");
        }
    }

    /// COW of an empty formerly-PACKED table resets mode/used/table-size to
    /// UNINITIALIZED/0/8 while preserving append history. Setting then deleting
    /// key 5 leaves next=6; the empty clone keeps that next value.
    #[test]
    fn empty_cow_resets_layout_but_preserves_next_free() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $c i32) (local $meta i32) (local $source_meta i32) (local $ok i64)
  (local.set $h (call $__rt_hash_new (i64.const 32) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 5) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 5) (i64.const -1)))
  (local.set $source_meta (call $__rt_hash_meta_addr (local.get $h)))
  (call $__rt_incref (local.get $h))
  (local.set $c (call $__rt_hash_ensure_unique (local.get $h)))
  (local.set $meta (call $__rt_hash_meta_addr (local.get $c)))
  (local.set $ok (i64.mul (i64.extend_i32_u (i32.ne (local.get $h) (local.get $c))) (i64.const 64)))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $source_meta) (i32.const 8))) (i64.const 1))) (i64.const 32))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eqz (i64.load (i32.add (local.get $meta) (i32.const 8))))) (i64.const 16))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eqz (i64.load (i32.add (local.get $meta) (i32.const 16))))) (i64.const 8))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $meta) (i32.const 24))) (i64.const 8))) (i64.const 4))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (local.get $meta)) (i64.const 6))) (i64.const 2))))
  (i64.add (local.get $ok) (i64.extend_i32_u (i64.eqz (i64.load (local.get $c))))))"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "127");
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

    /// `__rt_hash_unset` removes the matching entry and decrements the live count while
    /// leaving the other entries intact. With {1:10, 2:20, 3:30}, unset(2) leaves count 2,
    /// get(2) misses (found 0), and get(1)/get(3) still hit. Returns
    /// `count*100000 + get(1).vlo*1000 + get(2).found*100 + get(3).vlo`
    /// = 2*100000 + 10*1000 + 0 + 30 = 210030.
    #[test]
    fn unset_removes_entry_and_decrements_count() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local $g1 i64) (local $f2 i32) (local $g3 i64) (local $cnt i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 2) (i64.const -1) (i64.const 20) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 3) (i64.const -1) (i64.const 30) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 2) (i64.const -1)))
  (call $__rt_hash_get (local.get $h) (i64.const 1) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $g1) (local.set $found)
  (call $__rt_hash_get (local.get $h) (i64.const 2) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $f2)
  (call $__rt_hash_get (local.get $h) (i64.const 3) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $g3) (local.set $found)
  (local.set $cnt (i64.load (local.get $h)))
  (i64.add (i64.add (i64.mul (local.get $cnt) (i64.const 100000)) (i64.mul (local.get $g1) (i64.const 1000)))
           (i64.add (i64.mul (i64.extend_i32_u (local.get $f2)) (i64.const 100)) (local.get $g3))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "210030");
        }
    }

    /// `__rt_hash_unset` on a key the table does not contain is a no-op: the count and the
    /// existing entry are unchanged. With {1:10}, unset(99) keeps count 1 and get(1)=10.
    /// Returns `count*100 + get(1).vlo` = 1*100 + 10 = 110.
    #[test]
    fn unset_missing_key_is_noop() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 99) (i64.const -1)))
  (call $__rt_hash_get (local.get $h) (i64.const 1) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.mul (i64.load (local.get $h)) (i64.const 100)) (local.get $vlo)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "110");
        }
    }

    /// `__rt_hash_unset` tombstones the removed slot (occupied = 2, not 0) so the probe
    /// chain stays intact; re-setting the same key inserts a fresh entry that resolves
    /// correctly. With {5:50}, unset(5) then set(5)=55 leaves count 1 and get(5)=55.
    /// Returns `count*1000 + get(5).vlo` = 1*1000 + 55 = 1055.
    #[test]
    fn unset_then_reinsert_same_key() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 5) (i64.const -1) (i64.const 50) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 5) (i64.const -1)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 5) (i64.const -1) (i64.const 55) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const 5) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add (i64.mul (i64.load (local.get $h)) (i64.const 1000)) (local.get $vlo)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1055");
        }
    }

    /// Colliding integer keys 1 and 9 start at bucket 5 in a capacity-8 table.
    /// After key 1 is removed, lookup of key 9 must skip tombstone slot 5 and find
    /// the surviving entry at slot 6 without disturbing count or insertion order.
    #[test]
    fn colliding_lookup_survives_an_earlier_tombstone() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 9) (i64.const -1) (i64.const 90) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 1) (i64.const -1)))
  (call $__rt_hash_get (local.get $h) (i64.const 9) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add
    (i64.add
      (i64.add
        (i64.add
          (i64.mul (i64.load (local.get $h)) (i64.const 1000000))
          (i64.mul (local.get $vlo) (i64.const 1000)))
        (i64.mul (i64.load (i32.add (local.get $h) (i32.const 400))) (i64.const 100)))
      (i64.mul (i64.load (i32.add (local.get $h) (i32.const 24))) (i64.const 10)))
    (i64.load (i32.add (local.get $h) (i32.const 32)))))"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "1090266");
        }
    }

    /// Reinserting colliding key 1 after its removal reuses tombstone slot 5 rather
    /// than the later empty slot 7. The reinserted entry is appended after key 9 in
    /// insertion order, while count remains the number of live entries.
    #[test]
    fn colliding_reinsert_reuses_tombstone_and_appends_order() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $e5 i32) (local $e6 i32) (local $ok i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 9) (i64.const -1) (i64.const 90) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 1) (i64.const -1)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 1) (i64.const -1) (i64.const 11) (i64.const 0) (i64.const 0)))
  (local.set $e5 (i32.add (local.get $h) (i32.const 400)))
  (local.set $e6 (i32.add (local.get $h) (i32.const 472)))
  (local.set $ok (i64.mul (i64.extend_i32_u (i64.eq (i64.load (local.get $h)) (i64.const 2))) (i64.const 128)))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $h) (i32.const 24))) (i64.const 6))) (i64.const 64))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $h) (i32.const 32))) (i64.const 5))) (i64.const 32))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (local.get $e5)) (i64.const 1))) (i64.const 16))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $e5) (i32.const 8))) (i64.const 1))) (i64.const 8))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $e5) (i32.const 24))) (i64.const 11))) (i64.const 4))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $e6) (i32.const 56))) (i64.const 5))) (i64.const 2))))
  (i64.add (local.get $ok) (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $e5) (i32.const 48))) (i64.const 6))))
)"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "255");
        }
    }

    /// A new colliding key 17 also prefers the first tombstone at slot 5 over the
    /// later empty slot 7, and is appended after surviving key 9 in insertion order.
    #[test]
    fn new_colliding_key_reuses_first_tombstone() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $e5 i32) (local $e6 i32) (local $ok i64)
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 1) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 9) (i64.const -1) (i64.const 90) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 1) (i64.const -1)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 17) (i64.const -1) (i64.const 170) (i64.const 0) (i64.const 0)))
  (local.set $e5 (i32.add (local.get $h) (i32.const 400)))
  (local.set $e6 (i32.add (local.get $h) (i32.const 472)))
  (local.set $ok (i64.mul (i64.extend_i32_u (i64.eq (i64.load (local.get $h)) (i64.const 2))) (i64.const 128)))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $h) (i32.const 24))) (i64.const 6))) (i64.const 64))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $h) (i32.const 32))) (i64.const 5))) (i64.const 32))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (local.get $e5)) (i64.const 1))) (i64.const 16))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $e5) (i32.const 8))) (i64.const 17))) (i64.const 8))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $e5) (i32.const 24))) (i64.const 170))) (i64.const 4))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $e6) (i32.const 56))) (i64.const 5))) (i64.const 2))))
  (i64.add (local.get $ok) (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $e5) (i32.const 48))) (i64.const 6))))
)"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "255");
        }
    }

    /// `__rt_hash_insert_owned` terminates and reuses slot 2 when a capacity-4
    /// table contains three live entries plus one tombstone and no empty bucket.
    /// The new key is appended after the surviving 0,1,3 insertion order.
    #[test]
    fn insert_owned_reuses_tombstone_when_table_has_no_empty_slot() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $e0 i32) (local $e1 i32) (local $e2 i32) (local $e3 i32)
  (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64) (local $ok i64)
  (local.set $h (call $__rt_hash_new (i64.const 4) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 0) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 1) (i64.const -1) (i64.const 11) (i64.const 0) (i64.const 0) (i64.const 1)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 2) (i64.const -1) (i64.const 12) (i64.const 0) (i64.const 0) (i64.const 2)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 3) (i64.const -1) (i64.const 13) (i64.const 0) (i64.const 0) (i64.const 3)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 2) (i64.const -1)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 6) (i64.const -1) (i64.const 60) (i64.const 0) (i64.const 0) (i64.const 4)))
  (local.set $e0 (i32.add (local.get $h) (i32.const 40)))
  (local.set $e1 (i32.add (local.get $h) (i32.const 112)))
  (local.set $e2 (i32.add (local.get $h) (i32.const 184)))
  (local.set $e3 (i32.add (local.get $h) (i32.const 256)))
  (call $__rt_hash_get (local.get $h) (i64.const 6) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (local.set $ok (i64.mul (i64.extend_i32_u (i64.eq (i64.load (local.get $h)) (i64.const 4))) (i64.const 512)))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (local.get $e2)) (i64.const 1))) (i64.const 256))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $e2) (i32.const 8))) (i64.const 6))) (i64.const 128))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $e2) (i32.const 24))) (i64.const 60))) (i64.const 64))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eqz (i64.load (i32.add (local.get $h) (i32.const 24))))) (i64.const 32))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $h) (i32.const 32))) (i64.const 2))) (i64.const 16))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $e0) (i32.const 56))) (i64.const 1))) (i64.const 8))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $e1) (i32.const 56))) (i64.const 3))) (i64.const 4))))
  (local.set $ok (i64.add (local.get $ok) (i64.mul (i64.extend_i32_u (i64.eq (i64.load (i32.add (local.get $e3) (i32.const 56))) (i64.const 2))) (i64.const 2))))
  (i64.add (local.get $ok) (i64.extend_i32_u (i32.and (local.get $found) (i64.eq (local.get $vlo) (i64.const 60)))))
)"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "1023");
        }
    }

    /// Four removals can leave a capacity-4 probe table with four tombstones and
    /// no empty slot while live count is zero. A normal `hash_set` must complete
    /// the bounded full-table probe, reuse a tombstone without resizing, and make
    /// the new value readable from the same physical table.
    #[test]
    fn hash_set_reuses_tombstone_when_every_physical_slot_is_deleted() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local.set $h (call $__rt_hash_new (i64.const 4) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 0) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0) (i64.const 0)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 1) (i64.const -1) (i64.const 11) (i64.const 0) (i64.const 0) (i64.const 1)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 2) (i64.const -1) (i64.const 12) (i64.const 0) (i64.const 0) (i64.const 2)))
  (local.set $h (call $__rt_hash_insert_owned (local.get $h) (i64.const 3) (i64.const -1) (i64.const 13) (i64.const 0) (i64.const 0) (i64.const 3)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 0) (i64.const -1)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 1) (i64.const -1)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 2) (i64.const -1)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 3) (i64.const -1)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 9) (i64.const -1) (i64.const 99) (i64.const 0) (i64.const 0)))
  (call $__rt_hash_get (local.get $h) (i64.const 9) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (i64.add
    (i64.add
      (i64.mul (i64.load (local.get $h)) (i64.const 1000))
      (i64.mul (i64.load (i32.add (local.get $h) (i32.const 8))) (i64.const 100)))
    (i64.add (i64.mul (i64.extend_i32_u (local.get $found)) (i64.const 10)) (local.get $vlo))))"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "1509");
        }
    }

    /// Lookup and unset skip a tombstone whose stale string-key pointer is outside
    /// linear memory. Any accidental equality check on occupied=2 would dereference
    /// address 300000 and trap; both operations must instead advance to the empty slot.
    #[test]
    fn lookup_and_unset_skip_poisoned_tombstone_key() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $slot i64) (local $entry i32)
  (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (i32.store8 (i32.const 300) (i32.const 120))
  (local.set $h (call $__rt_hash_new (i64.const 4) (i64.const 0)))
  (local.set $slot (i64.rem_u (call $__rt_hash_key_hash (i64.const 300) (i64.const 1)) (i64.const 4)))
  (local.set $entry (i32.add (i32.add (local.get $h) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $slot) (i64.const 72)))))
  (i64.store (local.get $entry) (i64.const 2))
  (i64.store (i32.add (local.get $entry) (i32.const 8)) (i64.const 300000))
  (i64.store (i32.add (local.get $entry) (i32.const 16)) (i64.const 1))
  (call $__rt_hash_get (local.get $h) (i64.const 300) (i64.const 1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $found)
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 300) (i64.const 1)))
  (i64.add
    (i64.add
      (i64.mul (i64.extend_i32_u (i32.eqz (local.get $found))) (i64.const 100))
      (i64.mul (i64.extend_i32_u (i64.eqz (i64.load (local.get $h)))) (i64.const 10)))
    (i64.extend_i32_u (i64.eq (i64.load (local.get $entry)) (i64.const 2))))
)"#;
        if let Some(output) = run_driver(driver, "t") {
            assert_eq!(output, "111");
        }
    }

    /// `__rt_hash_unset` frees an owned string key and string value while a sibling entry
    /// survives. With a string-valued hash {"ok":"AB", "hi":"CD"}, unset("ok") leaves count
    /// 1, get("ok") misses, and get("hi") returns the persisted "CD" (first byte 'C' = 67).
    /// Returns `count*1000 + get("ok").found*100 + firstByte(get("hi").vlo)`
    /// = 1*1000 + 0 + 67 = 1067.
    #[test]
    fn unset_string_key_frees_and_keeps_sibling() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $h i32) (local $found i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local $fok i32) (local $ghi i64)
  (i32.store8 (i32.const 500) (i32.const 111)) (i32.store8 (i32.const 501) (i32.const 107))
  (i32.store8 (i32.const 510) (i32.const 104)) (i32.store8 (i32.const 511) (i32.const 105))
  (i32.store8 (i32.const 520) (i32.const 65)) (i32.store8 (i32.const 521) (i32.const 66))
  (i32.store8 (i32.const 530) (i32.const 67)) (i32.store8 (i32.const 531) (i32.const 68))
  (local.set $h (call $__rt_hash_new (i64.const 8) (i64.const 1)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 500) (i64.const 2) (i64.const 520) (i64.const 2) (i64.const 1)))
  (local.set $h (call $__rt_hash_set (local.get $h) (i64.const 510) (i64.const 2) (i64.const 530) (i64.const 2) (i64.const 1)))
  (local.set $h (call $__rt_hash_unset (local.get $h) (i64.const 500) (i64.const 2)))
  (call $__rt_hash_get (local.get $h) (i64.const 500) (i64.const 2))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $fok)
  (call $__rt_hash_get (local.get $h) (i64.const 510) (i64.const 2))
  (local.set $vtag) (local.set $vhi) (local.set $ghi) (local.set $found)
  (i64.add (i64.add (i64.mul (i64.load (local.get $h)) (i64.const 1000))
                    (i64.mul (i64.extend_i32_u (local.get $fok)) (i64.const 100)))
           (i64.extend_i32_u (i32.load8_u (i32.wrap_i64 (local.get $ghi))))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1067");
        }
    }

    /// `__rt_array_hash_union` promotes the left indexed entries to integer keys, then
    /// merges the right hash's key-absent entries (LEFT wins). With a = [10,20] (→ keys
    /// 0:10, 1:20) and b = {1:99, 5:30}: key 1 stays 20 (left wins over b's 99), key 5 is
    /// 30 (from b), count is 3. Returns `count + get(0)*100 + get(1)*10000 + get(5)*1000000`
    /// = 3 + 10*100 + 20*10000 + 30*1000000 = 30201003.
    #[test]
    fn array_hash_union_promotes_left_and_merges() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32) (local $u i32)
  (local $f i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local $g0 i64) (local $g1 i64) (local $g5 i64)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 10)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 20)))
  (local.set $b (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $b (call $__rt_hash_set (local.get $b) (i64.const 1) (i64.const -1) (i64.const 99) (i64.const 0) (i64.const 0)))
  (local.set $b (call $__rt_hash_set (local.get $b) (i64.const 5) (i64.const -1) (i64.const 30) (i64.const 0) (i64.const 0)))
  (local.set $u (call $__rt_array_hash_union (local.get $a) (local.get $b)))
  (call $__rt_hash_get (local.get $u) (i64.const 0) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $g0) (local.set $f)
  (call $__rt_hash_get (local.get $u) (i64.const 1) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $g1) (local.set $f)
  (call $__rt_hash_get (local.get $u) (i64.const 5) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $g5) (local.set $f)
  (i64.add (i64.add (i64.add
    (i64.load (local.get $u))
    (i64.mul (local.get $g0) (i64.const 100)))
    (i64.mul (local.get $g1) (i64.const 10000)))
    (i64.mul (local.get $g5) (i64.const 1000000))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "30201003");
        }
    }

    /// `__rt_array_hash_union` routes a left STRING element through the value-owning
    /// `__rt_hash_set`, persisting it under its integer position. With a = ["AB"] (bytes
    /// 65,66) and an empty b, the result's key 0 is the string "AB": value_tag 1, and the
    /// first byte of the OWNED copy is 65. Returns `value_tag*1000 + firstByte` = 1065.
    #[test]
    fn array_hash_union_persists_left_string_value() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32) (local $u i32)
  (local $f i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (i32.store8 (i32.const 200) (i32.const 65))
  (i32.store8 (i32.const 201) (i32.const 66))
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_str (local.get $a) (i32.const 200) (i64.const 2)))
  (local.set $b (call $__rt_hash_new (i64.const 8) (i64.const 7)))
  (local.set $u (call $__rt_array_hash_union (local.get $a) (local.get $b)))
  (call $__rt_hash_get (local.get $u) (i64.const 0) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $vlo) (local.set $f)
  (i64.add (i64.mul (local.get $vtag) (i64.const 1000))
           (i64.extend_i32_u (i32.load8_u (i32.wrap_i64 (local.get $vlo))))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1065");
        }
    }

    /// `__rt_hash_array_union` clones the left hash, then appends right indexed entries
    /// whose integer position is absent from the left (LEFT wins). With a = {0:10, 5:50}
    /// and b = [99,88,77]: key 0 stays 10 (left wins over b's 99), keys 1:88 and 2:77 are
    /// added, count is 4. Returns `count + get(0)*100 + get(2)*10000 + get(1)*1000000`
    /// = 4 + 10*100 + 77*10000 + 88*1000000 = 88771004.
    #[test]
    fn hash_array_union_clones_left_and_appends_missing() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32) (local $u i32)
  (local $f i32) (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local $g0 i64) (local $g1 i64) (local $g2 i64)
  (local.set $a (call $__rt_hash_new (i64.const 8) (i64.const 0)))
  (local.set $a (call $__rt_hash_set (local.get $a) (i64.const 0) (i64.const -1) (i64.const 10) (i64.const 0) (i64.const 0)))
  (local.set $a (call $__rt_hash_set (local.get $a) (i64.const 5) (i64.const -1) (i64.const 50) (i64.const 0) (i64.const 0)))
  (local.set $b (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $b (call $__rt_array_push_int (local.get $b) (i64.const 99)))
  (local.set $b (call $__rt_array_push_int (local.get $b) (i64.const 88)))
  (local.set $b (call $__rt_array_push_int (local.get $b) (i64.const 77)))
  (local.set $u (call $__rt_hash_array_union (local.get $a) (local.get $b)))
  (call $__rt_hash_get (local.get $u) (i64.const 0) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $g0) (local.set $f)
  (call $__rt_hash_get (local.get $u) (i64.const 1) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $g1) (local.set $f)
  (call $__rt_hash_get (local.get $u) (i64.const 2) (i64.const -1))
  (local.set $vtag) (local.set $vhi) (local.set $g2) (local.set $f)
  (i64.add (i64.add (i64.add
    (i64.load (local.get $u))
    (i64.mul (local.get $g0) (i64.const 100)))
    (i64.mul (local.get $g2) (i64.const 10000)))
    (i64.mul (local.get $g1) (i64.const 1000000))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "88771004");
        }
    }
}
