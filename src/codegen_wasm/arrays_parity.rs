//! Purpose:
//! The array-parity builtins that read or combine whole ASSOCIATIVE arrays without a callback:
//! `array_is_list`, `array_key_first`/`array_key_last`, `array_replace` and the two
//! `_assoc` set operations.
//!
//! Called from:
//! - `builtins::emit_builtin_runtime` — `emit_array_parity_runtime`.
//! - `builtins` — the shape checks and the lowerings.
//!
//! Key details:
//! - Every rule here was measured on php-src 8.5.6 before it was written, because several of
//!   them are not what they look like:
//!   * `array_is_list` reads the ORDER, not the key set — `[1 => "b", 0 => "a"]` is NOT a list;
//!   * `array_diff_assoc` compares values as STRINGS, so `1` and `"1"` are equal while `0` and
//!     `"0.0"` are not;
//!   * `array_replace` keeps the LEFT operand's key order and appends only genuinely new keys.
//! - The walks go through the insertion-order chain, which is what a hash's iteration is, and
//!   build their results with `__rt_hash_set`, which owns whatever it stores.

use super::wat::WatModule;

/// Adds the callback-free array-parity runtime.
///
/// Gated on the command runtime for the same reason the generic walk is: these compare through
/// `__rt_mixed_cast_string`, whose numeric-string classifier a reactor module does not carry.
pub(super) fn emit_array_parity_runtime(wm: &mut WatModule, has_main: bool) {
    if !has_main {
        return;
    }
    wm.add_raw_func(RT_HASH_IS_LIST);
    wm.add_raw_func(RT_HASH_EDGE_KEY);
    wm.add_raw_func(RT_HASH_REPLACE);
    wm.add_raw_func(RT_HASH_ASSOC_SET_OP);
}

/// `__rt_hash_is_list`: whether a hash's keys are exactly 0, 1, ... n-1 IN THAT ORDER.
///
/// php reads the insertion order, not the key set: `[1 => "b", 0 => "a"]` holds both keys of a
/// two-element list and is still not one, measured. An empty hash IS a list.
const RT_HASH_IS_LIST: &str = r#"(func $__rt_hash_is_list (param $hash i32) (result i64)
  (local $cur i64) (local $entry i32) (local $expected i64)
  (if (i32.eqz (local.get $hash))
    (then (return (i64.const 1))))                                ;; nothing to disagree with
  (local.set $cur (i64.load offset=24 (local.get $hash)))         ;; head of the insertion order
  (local.set $expected (i64.const 0))
  (block $done (loop $entry_loop
    (br_if $done (i64.eq (local.get $cur) (i64.const -1)))
    (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))
    (if (i64.ne (i64.load offset=16 (local.get $entry)) (i64.const -1))
      (then (return (i64.const 0))))                              ;; a string key is never a list key
    (if (i64.ne (i64.load offset=8 (local.get $entry)) (local.get $expected))
      (then (return (i64.const 0))))                              ;; out of order, or a gap
    (local.set $expected (i64.add (local.get $expected) (i64.const 1)))
    (local.set $cur (i64.load offset=56 (local.get $entry)))
    (br $entry_loop)))
  (i64.const 1))
"#;

/// `__rt_hash_edge_key`: the first (`$last` = 0) or last key of a hash, boxed, or a boxed null.
///
/// php answers `null` for an empty array rather than `false`, measured, and the key comes back
/// as its own kind — an integer key stays an integer.
const RT_HASH_EDGE_KEY: &str = r#"(func $__rt_hash_edge_key (param $hash i32) (param $last i32) (result i32)
  (local $cur i64) (local $entry i32) (local $khi i64) (local $kp i32) (local $kl i64) (local $cell i32)
  (if (i32.eqz (local.get $hash))
    (then (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
  (local.set $cur (select
    (i64.load offset=32 (local.get $hash))
    (i64.load offset=24 (local.get $hash))
    (local.get $last)))
  (if (i64.eq (local.get $cur) (i64.const -1))
    (then (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
  (local.set $entry (i32.add (i32.add (local.get $hash) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))
  (local.set $khi (i64.load offset=16 (local.get $entry)))
  (if (i64.eq (local.get $khi) (i64.const -1))
    (then (return (call $__rt_mixed_from_value (i64.const 0) (i64.load offset=8 (local.get $entry)) (i64.const 0)))))
  (call $__rt_str_persist (i32.wrap_i64 (i64.load offset=8 (local.get $entry))) (local.get $khi))
  (local.set $kl)
  (local.set $kp)
  ;; The cell persists its own copy, so the one made above is ours to give back.
  (local.set $cell (call $__rt_mixed_from_value (i64.const 1) (i64.extend_i32_u (local.get $kp)) (local.get $kl)))
  (call $__rt_heap_free (local.get $kp))
  (local.get $cell))
"#;

/// `__rt_hash_replace`: php's `array_replace($left, $right)`.
///
/// The result starts as the LEFT operand — which is what keeps its key ORDER — and every right
/// entry then overwrites in place or appends. `__rt_hash_set` does exactly that, so the ordering
/// falls out rather than being arranged.
const RT_HASH_REPLACE: &str = r#"(func $__rt_hash_replace (param $left i32) (param $right i32) (result i32)
  (local $result i32) (local $cur i64) (local $entry i32)
  (local.set $result (call $__rt_hash_clone_shallow (local.get $left)))
  (if (i32.eqz (local.get $right))
    (then (return (local.get $result))))
  (local.set $cur (i64.load offset=24 (local.get $right)))
  (block $done (loop $entry_loop
    (br_if $done (i64.eq (local.get $cur) (i64.const -1)))
    (local.set $entry (i32.add (i32.add (local.get $right) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))
    (local.set $result (call $__rt_hash_set (local.get $result)
      (i64.load offset=8 (local.get $entry)) (i64.load offset=16 (local.get $entry))
      (i64.load offset=24 (local.get $entry)) (i64.load offset=32 (local.get $entry))
      (i64.load offset=40 (local.get $entry))))
    (local.set $cur (i64.load offset=56 (local.get $entry)))
    (br $entry_loop)))
  (local.get $result))
"#;

/// `__rt_hash_assoc_set_op`: `array_diff_assoc` (`$keep_missing` = 1) and `array_intersect_assoc`.
///
/// Both compare the KEY and the VALUE, and php compares those values AS STRINGS — measured:
/// `array_diff_assoc(["a" => 1], ["a" => "1"])` is empty because `1` and `"1"` render alike,
/// while `0` against `"0.0"` survives because `"0"` and `"0.0"` do not. That is why this casts
/// rather than calling the numeric comparison.
///
/// `__rt_hash_get` answers `(found, value_lo, value_hi, value_tag)` — the flag is the FIRST
/// result, so it comes off the stack LAST — and
/// `__rt_mixed_cast_string` answers `(ptr, len)` with the length as an i32 and the bytes already
/// persisted — so both renderings are released once compared.
const RT_HASH_ASSOC_SET_OP: &str = r#"(func $__rt_hash_assoc_set_op (param $left i32) (param $right i32) (param $keep_missing i32) (result i32)
  (local $result i32) (local $cur i64) (local $entry i32) (local $found i32) (local $same i32)
  (local $lcell i32) (local $rcell i32) (local $lp i32) (local $ll i32) (local $rp i32) (local $rl i32)
  (local $vlo i64) (local $vhi i64) (local $vtag i64)
  (local $rlo i64) (local $rhi i64) (local $rtag i64)
  (if (i32.eqz (local.get $left))                                 ;; read the value_type only AFTER this
    (then (return (call $__rt_hash_new (i64.const 4) (i64.const 7)))))
  (local.set $result (call $__rt_hash_new (i64.const 4) (i64.load offset=16 (local.get $left))))
  (local.set $cur (i64.load offset=24 (local.get $left)))
  (block $done (loop $entry_loop
    (br_if $done (i64.eq (local.get $cur) (i64.const -1)))
    (local.set $entry (i32.add (i32.add (local.get $left) (i32.const 40)) (i32.wrap_i64 (i64.mul (local.get $cur) (i64.const 72)))))
    (local.set $vlo (i64.load offset=24 (local.get $entry)))
    (local.set $vhi (i64.load offset=32 (local.get $entry)))
    (local.set $vtag (i64.load offset=40 (local.get $entry)))
    (local.set $same (i32.const 0))
    (call $__rt_hash_get (local.get $right)
      (i64.load offset=8 (local.get $entry)) (i64.load offset=16 (local.get $entry)))
    (local.set $rtag)                                             ;; results are (found, lo, hi, tag);
    (local.set $rhi)                                              ;; the flag is FIRST, so it pops LAST
    (local.set $rlo)
    (local.set $found)
    (if (local.get $found)
      (then
        ;; The key exists on the right, so the values decide — compared as php renders them.
        (local.set $lcell (call $__rt_mixed_from_value (local.get $vtag) (local.get $vlo) (local.get $vhi)))
        (local.set $rcell (call $__rt_mixed_from_value (local.get $rtag) (local.get $rlo) (local.get $rhi)))
        (call $__rt_mixed_cast_string (local.get $lcell))
        (local.set $ll)
        (local.set $lp)
        (call $__rt_mixed_cast_string (local.get $rcell))
        (local.set $rl)
        (local.set $rp)
        (local.set $same (call $__rt_strict_str_eq
          (local.get $lp) (i64.extend_i32_u (local.get $ll))
          (local.get $rp) (i64.extend_i32_u (local.get $rl))))
        (call $__rt_decref_any (local.get $lp))                   ;; the cast persisted both
        (call $__rt_decref_any (local.get $rp))
        (call $__rt_decref_any (local.get $lcell))
        (call $__rt_decref_any (local.get $rcell))))
    ;; diff keeps what did NOT match, intersect keeps what did.
    (if (i32.eq (local.get $same) (i32.eqz (local.get $keep_missing)))
      (then
        (local.set $result (call $__rt_hash_set (local.get $result)
          (i64.load offset=8 (local.get $entry)) (i64.load offset=16 (local.get $entry))
          (local.get $vlo) (local.get $vhi) (local.get $vtag)))))
    (local.set $cur (i64.load offset=56 (local.get $entry)))
    (br $entry_loop)))
  (local.get $result))
"#;
