//! Purpose:
//! The array builtins that read or rebuild a whole array: `array_reverse` over any element
//! representation, `array_pad`, `array_unique` and `array_column`.
//!
//! Called from:
//! - `builtins::emit_builtin_runtime` — `emit_array_ext_runtime`.
//! - `builtins` — the shape checks and the lowerings.
//!
//! Key details:
//! - Where an operation only MOVES elements, it is generic: `__rt_array_clone_shallow` already
//!   copies the payload bytes and increfs every refcounted child, so reversing is a swap of
//!   slots on the clone and the element representation is preserved untouched. Nothing here has
//!   to know whether a slot holds an int, a string pair or a Mixed cell.
//! - Where an operation has to WRITE a new element, it cannot be generic: the value's width and
//!   its refcount obligation both follow the array's `value_type`. Those are served for the raw
//!   i64 representation and refused otherwise, which is the same line `__rt_array_reverse_int`
//!   and `__rt_array_sum_int` already draw.
//! - `array_column` is the exception that is naturally heterogeneous: its rows are hashes and
//!   its result is `array<mixed>`, so every value is boxed and the result carries cells.

use super::wat::WatModule;

/// Adds the array-rebuilding runtime. References only helpers the array, hash and mixed
/// runtimes already emit.
pub(super) fn emit_array_ext_runtime(wm: &mut WatModule, has_main: bool) {
    wm.add_raw_func(RT_ARRAY_REVERSE_ANY);
    wm.add_raw_func(RT_ARRAY_PAD_INT);
    wm.add_raw_func(RT_ARRAY_UNIQUE_INT);
    wm.add_raw_func(RT_ARRAY_CLONE_RANGE);
    wm.add_raw_func(RT_ARRAY_SPLICE_ANY);
    wm.add_raw_func(RT_ARRAY_UNSHIFT_INT);
    wm.add_raw_func(RT_ARRAY_CELLS_TO);
    // This one reads its elements through the GENERIC walk, which belongs to the command
    // runtime because its `foreach` dispatch warns through WASI. A reactor module has neither.
    if !has_main {
        return;
    }
    wm.add_raw_func(RT_ARRAY_COLUMN);
    wm.add_raw_func(RT_BOXED_ARRAY_VALUES);
    wm.add_raw_func(RT_BOXED_ARRAY_KEYS);
    wm.add_raw_func(RT_BOXED_ARRAY_REVERSE);
    wm.add_raw_func(RT_BOXED_FIND_VALUE);
}

/// `__rt_boxed_array_values`: `array_values` of a boxed array, list or hash, of any layout.
///
/// The generic `foreach` walk answers each value as an OWNED cell in php's order, and the result
/// list keeps them, so it is an `array<mixed>` of cells whatever the source's layout was.
const RT_BOXED_ARRAY_VALUES: &str = r#"(func $__rt_boxed_array_values (param $cell i32) (result i32)
  (local $src i32) (local $cursor i64) (local $kind i32) (local $more i64) (local $out i32)
  (call $__rt_mixed_iter_start (local.get $cell))
  (local.set $kind)
  (local.set $cursor)
  (local.set $src)
  (local.set $out (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (block $done (loop $entry
    (call $__rt_mixed_iter_next (local.get $src) (local.get $cursor) (local.get $kind))
    (local.set $more)
    (local.set $cursor)
    (br_if $done (i64.eqz (local.get $more)))
    (local.set $out (call $__rt_array_push_mixed (local.get $out)
      (call $__rt_mixed_iter_value (local.get $src) (local.get $cursor) (local.get $kind))))
    (br $entry)))
  (local.get $out))
"#;

/// `__rt_boxed_array_keys`: `array_keys` of a boxed array — positions of a list, keys of a hash.
const RT_BOXED_ARRAY_KEYS: &str = r#"(func $__rt_boxed_array_keys (param $cell i32) (result i32)
  (local $src i32) (local $cursor i64) (local $kind i32) (local $more i64) (local $out i32)
  (call $__rt_mixed_iter_start (local.get $cell))
  (local.set $kind)
  (local.set $cursor)
  (local.set $src)
  (local.set $out (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (block $done (loop $entry
    (call $__rt_mixed_iter_next (local.get $src) (local.get $cursor) (local.get $kind))
    (local.set $more)
    (local.set $cursor)
    (br_if $done (i64.eqz (local.get $more)))
    (local.set $out (call $__rt_array_push_mixed (local.get $out)
      (call $__rt_mixed_iter_key (local.get $src) (local.get $cursor) (local.get $kind))))
    (br $entry)))
  (local.get $out))
"#;

/// `__rt_boxed_array_reverse`: `array_reverse` of a boxed array, answered as an owned cell.
///
/// A list is reversed by the generic slot swap, whatever its layout. A hash is rebuilt from its
/// last entry to its first: php keeps a string key and renumbers an integer one, which is what
/// appending an integer-keyed value to a fresh hash does.
const RT_BOXED_ARRAY_REVERSE: &str = r#"(func $__rt_boxed_array_reverse (param $cell i32) (result i32)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $out i32) (local $result i32)
  (local $keys i32) (local $values i32) (local $i i64) (local $k i32) (local $v i32)
  (local $ktag i64) (local $klo i64) (local $khi i64) (local $vtag i64) (local $vlo i64) (local $vhi i64)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 4))
    (then
      (local.set $out (call $__rt_array_reverse_any (i32.wrap_i64 (local.get $lo))))
      (local.set $result (call $__rt_mixed_from_value (i64.const 4)
        (i64.extend_i32_u (local.get $out)) (i64.const 0)))
      (call $__rt_decref_any (local.get $out))                            ;; the cell holds it now
      (return (local.get $result))))
  (local.set $keys (call $__rt_boxed_array_keys (local.get $cell)))
  (local.set $values (call $__rt_boxed_array_values (local.get $cell)))
  (local.set $out (call $__rt_hash_new (i64.const 8) (i64.const 7)))
  (local.set $i (i64.load (local.get $keys)))
  (block $done (loop $entry
    (br_if $done (i64.eqz (local.get $i)))
    (local.set $i (i64.sub (local.get $i) (i64.const 1)))
    (local.set $k (i32.wrap_i64 (i64.load (i32.add (i32.add (local.get $keys) (i32.const 24))
      (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))))
    (local.set $v (i32.wrap_i64 (i64.load (i32.add (i32.add (local.get $values) (i32.const 24))
      (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))))
    (call $__rt_mixed_unbox (local.get $k))
    (local.set $khi)
    (local.set $klo)
    (local.set $ktag)
    (call $__rt_mixed_unbox (local.get $v))
    (local.set $vhi)
    (local.set $vlo)
    (local.set $vtag)
    (if (i64.eq (local.get $ktag) (i64.const 1))
      (then (local.set $out (call $__rt_hash_set (local.get $out) (local.get $klo) (local.get $khi)
        (local.get $vlo) (local.get $vhi) (local.get $vtag))))
      (else (local.set $out (call $__rt_hash_append (local.get $out)
        (local.get $vlo) (local.get $vhi) (local.get $vtag)))))
    (br $entry)))
  (call $__rt_decref_any (local.get $keys))
  (call $__rt_decref_any (local.get $values))
  (local.set $result (call $__rt_mixed_from_value (i64.const 5)
    (i64.extend_i32_u (local.get $out)) (i64.const 0)))
  (call $__rt_decref_any (local.get $out))
  (local.get $result))
"#;

/// `__rt_boxed_find_value`: `in_array`/`array_search` over a boxed array, list or hash.
///
/// The twin of `__rt_hash_find_value` through the generic walk: the same two comparisons, and the
/// same OWNED key cell for a hit (0 for a miss).
const RT_BOXED_FIND_VALUE: &str = r#"(func $__rt_boxed_find_value (param $cell i32) (param $needle i32) (param $strict i32) (result i32)
  (local $src i32) (local $cursor i64) (local $kind i32) (local $more i64) (local $v i32) (local $hit i32)
  (call $__rt_mixed_iter_start (local.get $cell))
  (local.set $kind)
  (local.set $cursor)
  (local.set $src)
  (block $done (loop $entry
    (call $__rt_mixed_iter_next (local.get $src) (local.get $cursor) (local.get $kind))
    (local.set $more)
    (local.set $cursor)
    (br_if $done (i64.eqz (local.get $more)))
    (local.set $v (call $__rt_mixed_iter_value (local.get $src) (local.get $cursor) (local.get $kind)))
    (if (local.get $strict)
      (then (local.set $hit (call $__rt_strict_mixed_mixed (local.get $needle) (local.get $v))))
      (else (local.set $hit (i64.eqz (call $__rt_mixed_cmp_mixed (local.get $needle) (local.get $v))))))
    (call $__rt_decref_any (local.get $v))
    (if (local.get $hit)
      (then (return (call $__rt_mixed_iter_key (local.get $src) (local.get $cursor) (local.get $kind)))))
    (br $entry)))
  (i32.const 0))
"#;

/// `__rt_array_cells_to`: a list of Mixed cells rebuilt in one concrete layout, consuming it.
///
/// `$kind` 0 stores each cell's payload as an int slot, 1 as a string slot, 2 as a float slot.
/// The caller vouches every cell carries that type — `array_map` with a callback declared to
/// return it — so the payload is read as is. Strings are persisted by the push and the cells
/// released with the list.
const RT_ARRAY_CELLS_TO: &str = r#"(func $__rt_array_cells_to (param $list i32) (param $kind i32) (result i32)
  (local $len i64) (local $i i64) (local $out i32)
  (local $tag i64) (local $lo i64) (local $hi i64)
  (local.set $len (i64.load (local.get $list)))
  (local.set $out (call $__rt_array_new (local.get $len)
    (select (i64.const 16) (i64.const 8) (i32.eq (local.get $kind) (i32.const 1)))))
  (block $done (loop $each
    (br_if $done (i64.ge_s (local.get $i) (local.get $len)))
    (call $__rt_mixed_unbox (i32.wrap_i64 (i64.load (i32.add (i32.add (local.get $list) (i32.const 24))
      (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))))
    (local.set $hi)
    (local.set $lo)
    (local.set $tag)
    (if (i32.eq (local.get $kind) (i32.const 1))
      (then (local.set $out (call $__rt_array_push_str (local.get $out)
        (i32.wrap_i64 (local.get $lo)) (local.get $hi))))
      (else (if (i32.eq (local.get $kind) (i32.const 2))
        (then (local.set $out (call $__rt_array_push_float (local.get $out)
          (f64.reinterpret_i64 (local.get $lo)))))
        (else (local.set $out (call $__rt_array_push_int (local.get $out) (local.get $lo)))))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $each)))
  (call $__rt_decref_any (local.get $list))
  (local.get $out))
"#;

/// `__rt_array_reverse_any`: `array_reverse` for any element representation.
///
/// The clone carries the source's `value_type`, `elem_size` and one reference per refcounted
/// child, so reversing is a byte swap of whole slots: nothing is boxed, nothing is re-tagged,
/// and the refcounts are already right because the elements are the same objects.
const RT_ARRAY_REVERSE_ANY: &str = r#"(func $__rt_array_reverse_any (param $array i32) (result i32)
  (local $out i32) (local $len i64) (local $esz i32) (local $i i64) (local $j i64)
  (local $a i32) (local $b i32) (local $k i32) (local $tmp i32)
  (local.set $out (call $__rt_array_clone_shallow (local.get $array)))
  (local.set $len (i64.load (local.get $out)))
  (local.set $esz (i32.wrap_i64 (i64.load offset=16 (local.get $out))))
  (local.set $i (i64.const 0))
  (local.set $j (i64.sub (local.get $len) (i64.const 1)))
  (block $done (loop $swap
    (br_if $done (i64.ge_s (local.get $i) (local.get $j)))
    (local.set $a (i32.add (i32.add (local.get $out) (i32.const 24))
                           (i32.mul (i32.wrap_i64 (local.get $i)) (local.get $esz))))
    (local.set $b (i32.add (i32.add (local.get $out) (i32.const 24))
                           (i32.mul (i32.wrap_i64 (local.get $j)) (local.get $esz))))
    (local.set $k (i32.const 0))
    (block $swapped (loop $byte
      (br_if $swapped (i32.ge_u (local.get $k) (local.get $esz)))
      (local.set $tmp (i32.load8_u (i32.add (local.get $a) (local.get $k))))
      (i32.store8 (i32.add (local.get $a) (local.get $k))
                  (i32.load8_u (i32.add (local.get $b) (local.get $k))))
      (i32.store8 (i32.add (local.get $b) (local.get $k)) (local.get $tmp))
      (local.set $k (i32.add (local.get $k) (i32.const 1)))
      (br $byte)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (local.set $j (i64.sub (local.get $j) (i64.const 1)))
    (br $swap)))
  (local.get $out))
"#;

/// `__rt_array_pad_int`: `array_pad` over raw i64 slots.
///
/// php pads on the RIGHT for a positive size and on the LEFT for a negative one, and answers a
/// copy untouched when the array is already that long — the size is a target, not an amount.
const RT_ARRAY_PAD_INT: &str = r#"(func $__rt_array_pad_int (param $array i32) (param $size i64) (param $value i64) (result i32)
  (local $len i64) (local $want i64) (local $out i32) (local $i i64)
  (local.set $len (i64.load (local.get $array)))
  (local.set $want (select (i64.sub (i64.const 0) (local.get $size)) (local.get $size)
                           (i64.lt_s (local.get $size) (i64.const 0))))
  (if (i64.le_s (local.get $want) (local.get $len))
    (then (return (call $__rt_array_clone_shallow (local.get $array)))))
  (local.set $out (call $__rt_array_new (local.get $want) (i64.const 8)))
  (if (i64.lt_s (local.get $size) (i64.const 0))
    (then
      ;; A negative size pads on the left, so the fill comes first.
      (local.set $i (local.get $len))
      (block $filled (loop $pad
        (br_if $filled (i64.ge_s (local.get $i) (local.get $want)))
        (local.set $out (call $__rt_array_push_int (local.get $out) (local.get $value)))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $pad)))))
  (local.set $i (i64.const 0))
  (block $copied (loop $element
    (br_if $copied (i64.ge_s (local.get $i) (local.get $len)))
    (local.set $out (call $__rt_array_push_int (local.get $out)
      (i64.load (i32.add (i32.add (local.get $array) (i32.const 24))
                         (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8)))))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $element)))
  (if (i64.ge_s (local.get $size) (i64.const 0))
    (then
      (local.set $i (local.get $len))
      (block $filled (loop $pad
        (br_if $filled (i64.ge_s (local.get $i) (local.get $want)))
        (local.set $out (call $__rt_array_push_int (local.get $out) (local.get $value)))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $pad)))))
  (local.get $out))
"#;

/// `__rt_array_unique_int`: `array_unique` over raw i64 slots, answering php's own shape.
///
/// php keeps the FIRST occurrence of each value and PRESERVES the original keys, so the result
/// of `[1, 2, 2, 3, 3, 4]` is `[0 => 1, 1 => 2, 3 => 3, 5 => 4]` — measured, and deliberately
/// not a list: `array_is_list()` on it is false. An indexed result cannot hold those holes, so
/// the answer is a HASH keyed by the source position, which is what the checker now types it.
///
/// Uniqueness is php's default `SORT_STRING`, and for an integer-valued source that is exactly
/// i64 equality: two distinct longs never render the same decimal string. A source whose slots
/// are not raw longs reaches a different admission and never arrives here.
const RT_ARRAY_UNIQUE_INT: &str = r#"(func $__rt_array_unique_int (param $array i32) (result i32)
  (local $len i64) (local $out i32) (local $i i64) (local $value i64) (local $seen i32)
  (local $set i32) (local $lo i64) (local $hi i64) (local $tag i64)
  (local.set $len (i64.load (local.get $array)))
  (local.set $out (call $__rt_hash_new (local.get $len) (i64.const 0)))  ;; value_type 0 = int
  (local.set $set (call $__rt_hash_new (local.get $len) (i64.const 0)))  ;; the values already kept
  (local.set $i (i64.const 0))
  (block $done (loop $element
    (br_if $done (i64.ge_s (local.get $i) (local.get $len)))
    (local.set $value (i64.load (i32.add (i32.add (local.get $array) (i32.const 24))
                                         (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8))))))
    ;; membership is a hash keyed by the VALUE: one probe per element instead of a scan of
    ;; every earlier slot, which made `array_unique(range(1, 100000))` ~5e9 comparisons
    (call $__rt_hash_get (local.get $set) (local.get $value) (i64.const -1))
    (local.set $tag)
    (local.set $hi)
    (local.set $lo)
    (local.set $seen)
    (if (i32.eqz (local.get $seen))
      (then
        (local.set $set (call $__rt_hash_set (local.get $set)
          (local.get $value) (i64.const -1) (i64.const 1) (i64.const 0) (i64.const 0)))
        (local.set $out (call $__rt_hash_set (local.get $out)
          (local.get $i) (i64.const -1)                            ;; the SOURCE position, as an int key
          (local.get $value) (i64.const 0) (i64.const 0)))))       ;; tag 0 = int
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $element)))
  (call $__rt_decref_hash (local.get $set))
  (local.get $out))
"#;

/// `__rt_array_column`: the named value of every row, as an `array<mixed>`.
///
/// The rows are hashes, so each lookup goes through the hash getter and the found `(lo, hi, tag)`
/// triple is boxed — the result is heterogeneous by construction, which is what the checker types
/// it. A row missing the key contributes nothing, which is php's rule.
const RT_ARRAY_COLUMN: &str = r#"(func $__rt_array_column (param $array i32) (param $key i32) (param $key_len i64) (result i32)
  (local $out i32) (local $cursor i64) (local $more i64) (local $next i64)
  (local $row i32) (local $found i32) (local $lo i64) (local $hi i64) (local $tag i64)
  (local $cell i32)
  (local.set $out (call $__rt_array_new (i64.load (local.get $array)) (i64.const 16)))
  (local.set $cursor (i64.const -1))
  (block $done (loop $entry
    (call $__rt_mixed_iter_next (local.get $array) (local.get $cursor) (i32.const 0))
    (local.set $more)
    (local.set $next)
    (br_if $done (i64.eqz (local.get $more)))
    (local.set $cursor (local.get $next))
    (local.set $cell (call $__rt_mixed_iter_value (local.get $array) (local.get $cursor) (i32.const 0)))
    (call $__rt_mixed_unbox (local.get $cell))
    (local.set $hi)
    (local.set $lo)
    (local.set $tag)
    (local.set $row (i32.wrap_i64 (local.get $lo)))
    (call $__rt_decref_any (local.get $cell))
    (if (i64.ne (local.get $tag) (i64.const 5))                    ;; only a hash row has columns
      (then (br $entry)))
    (call $__rt_hash_get (local.get $row)
      (i64.extend_i32_u (local.get $key)) (local.get $key_len))
    (local.set $tag)
    (local.set $hi)
    (local.set $lo)
    (local.set $found)
    (if (i32.eqz (local.get $found))
      (then (br $entry)))
    (local.set $cell (call $__rt_mixed_from_value (local.get $tag) (local.get $lo) (local.get $hi)))
    (local.set $out (call $__rt_array_push_mixed (local.get $out) (local.get $cell)))
    (br $entry)))
  (local.get $out))
"#;

/// `__rt_array_clone_range`: an independent array holding `count` elements from `start`.
///
/// The ownership fixup is `__rt_array_clone_shallow`'s, bounded to the range: a string element
/// is persisted into its own copy and a refcounted container child is increfed, because the copy
/// is a second owner. Anything else is bytes, which the copy already carries.
const RT_ARRAY_CLONE_RANGE: &str = r#"(func $__rt_array_clone_range (param $array i32) (param $start i64) (param $count i64) (result i32)
  (local $esz i64) (local $kindw i64) (local $new i32) (local $vt i32) (local $i i64)
  (local $slot i32) (local $oldptr i32) (local $slen i64) (local $np i32) (local $nl i64)
  (local $src i32) (local $dst i32) (local $j i64)
  (local.set $esz (i64.load offset=16 (local.get $array)))
  (local.set $kindw (i64.load (i32.sub (local.get $array) (i32.const 8))))
  (if (i64.lt_s (local.get $count) (i64.const 0))
    (then (local.set $count (i64.const 0))))
  (local.set $new (call $__rt_array_new
    (select (local.get $count) (i64.const 1) (i64.gt_s (local.get $count) (i64.const 0)))
    (local.get $esz)))
  (i64.store (i32.sub (local.get $new) (i32.const 8))
             (i64.and (local.get $kindw) (i64.const 65535)))       ;; same kind/value_type/COW
  (i64.store (local.get $new) (local.get $count))
  (local.set $src (i32.add (i32.add (local.get $array) (i32.const 24))
                           (i32.wrap_i64 (i64.mul (local.get $start) (local.get $esz)))))
  (local.set $dst (i32.add (local.get $new) (i32.const 24)))
  (local.set $j (i64.mul (local.get $count) (local.get $esz)))
  (memory.copy (local.get $dst) (local.get $src) (i32.wrap_i64 (local.get $j)))  ;; the slice's live bytes
  (local.set $vt (i32.and (i32.wrap_i64 (i64.shr_u (local.get $kindw) (i64.const 8))) (i32.const 127)))
  (if (i32.eq (local.get $vt) (i32.const 1))                       ;; string elements own their bytes
    (then
      (local.set $i (i64.const 0))
      (block $send (loop $sclone
        (br_if $send (i64.ge_s (local.get $i) (local.get $count)))
        (local.set $slot (i32.add (i32.add (local.get $new) (i32.const 24))
                                  (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))
        (local.set $oldptr (i32.wrap_i64 (i64.load (local.get $slot))))
        (local.set $slen (i64.load offset=8 (local.get $slot)))
        (call $__rt_str_persist (local.get $oldptr) (local.get $slen))
        (local.set $nl)
        (local.set $np)
        (i64.store (local.get $slot) (i64.extend_i32_u (local.get $np)))
        (i64.store offset=8 (local.get $slot) (local.get $nl))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $sclone))))
    (else
      (if (i32.or (i32.eq (local.get $vt) (i32.const 4))
          (i32.or (i32.eq (local.get $vt) (i32.const 5))
          (i32.or (i32.eq (local.get $vt) (i32.const 6))
          (i32.or (i32.eq (local.get $vt) (i32.const 7))
                  (i32.eq (local.get $vt) (i32.const 10))))))      ;; refcounted children
        (then
          (local.set $i (i64.const 0))
          (block $rend (loop $rclone
            (br_if $rend (i64.ge_s (local.get $i) (local.get $count)))
            (call $__rt_incref (i32.wrap_i64 (i64.load
              (i32.add (i32.add (local.get $new) (i32.const 24))
                       (i32.wrap_i64 (i64.mul (local.get $i) (local.get $esz)))))))
            (local.set $i (i64.add (local.get $i) (i64.const 1)))
            (br $rclone)))))))
  (local.get $new))
"#;

/// `__rt_array_splice_any`: removes a window and inserts a replacement, answering both halves.
///
/// Returns `(out, removed)`. The caller writes `out` back through the array's storage and owns
/// `removed`; the SOURCE keeps its own reference, which the write-back then drops.
///
/// php's window normalisation is the fiddly part and is measured, not guessed: a negative offset
/// counts from the end, a negative length leaves that many elements at the end untouched, and an
/// offset past the end appends.
const RT_ARRAY_SPLICE_ANY: &str = r#"(func $__rt_array_splice_any (param $array i32) (param $off i64) (param $len i64) (param $has_len i32) (param $repl i32) (result i32 i32)
  (local $n i64) (local $start i64) (local $count i64) (local $out i32) (local $removed i32)
  (local.set $n (i64.load (local.get $array)))
  (local.set $start (local.get $off))
  (if (i64.lt_s (local.get $start) (i64.const 0))
    (then
      (local.set $start (i64.add (local.get $n) (local.get $start)))
      (if (i64.lt_s (local.get $start) (i64.const 0))
        (then (local.set $start (i64.const 0))))))
  (if (i64.gt_s (local.get $start) (local.get $n))
    (then (local.set $start (local.get $n))))
  (if (i32.eqz (local.get $has_len))
    (then (local.set $count (i64.sub (local.get $n) (local.get $start))))
    (else
      (local.set $count (local.get $len))
      (if (i64.lt_s (local.get $count) (i64.const 0))
        (then
          (local.set $count (i64.add (i64.sub (local.get $n) (local.get $start)) (local.get $count)))
          (if (i64.lt_s (local.get $count) (i64.const 0))
            (then (local.set $count (i64.const 0))))))))
  ;; compare against the room left, not start+count: PHP_INT_MAX as a length overflowed the sum
  (if (i64.gt_s (local.get $count) (i64.sub (local.get $n) (local.get $start)))
    (then (local.set $count (i64.sub (local.get $n) (local.get $start)))))
  (local.set $removed (call $__rt_array_clone_range (local.get $array) (local.get $start) (local.get $count)))
  (local.set $out (call $__rt_array_clone_range (local.get $array) (i64.const 0) (local.get $start)))
  (if (local.get $repl)
    (then (local.set $out (call $__rt_array_append_from (local.get $out) (local.get $repl) (i64.const 0)))))
  (local.set $out (call $__rt_array_append_from (local.get $out) (local.get $array)
    (i64.add (local.get $start) (local.get $count))))
  (local.get $out)
  (local.get $removed))
"#;

/// `__rt_array_unshift_int`: prepends a list of raw i64 values, answering the new count.
///
/// Only the raw i64 representation, for the reason `array_pad` is: the prepended elements are
/// WRITTEN, and a value's width and refcount obligation both follow the array's `value_type`.
const RT_ARRAY_UNSHIFT_INT: &str = r#"(func $__rt_array_unshift_int (param $array i32) (param $values i32) (result i32 i64)
  (local $out i32)
  (local.set $out (call $__rt_array_clone_range (local.get $values) (i64.const 0)
                                                (i64.load (local.get $values))))
  (local.set $out (call $__rt_array_append_from (local.get $out) (local.get $array) (i64.const 0)))
  (local.get $out)
  (i64.load (local.get $out)))
"#;
