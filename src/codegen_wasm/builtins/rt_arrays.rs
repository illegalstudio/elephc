//! Purpose:
//! WAT source for the array walkers that build strings or search by string: implode, explode and `in_array` on strings.
//!
//! Called from:
//! - `super::emit_builtin_runtime` registers every constant; `arrays` and `array_search` lower the calls.

/// `__rt_implode`: owns the elements of an indexed STRING array joined by a glue string.
///
/// Two passes rather than a worst-case reservation: the total is the sum of the element lengths
/// plus `(count - 1)` glues, which is exactly knowable before writing, so the result block is
/// sized precisely. An empty array joins to the empty string with no glue at all, and a
/// single-element array to that element — the glue count is one FEWER than the elements, which is
/// the off-by-one this shape makes hard to get wrong.
pub(super) const RT_IMPLODE: &str = r#"(func $__rt_implode (param $array i32) (param $gptr i32) (param $glen i64) (result i32) (result i64)
  (local $n i64)                                                  ;; element count
  (local $i i64)                                                  ;; element cursor
  (local $j i64)                                                  ;; byte cursor within a piece
  (local $total i64)                                              ;; exact output length
  (local $eptr i32)                                               ;; current element pointer
  (local $elen i64)                                               ;; current element length
  (local $out i32)                                                ;; owned result block
  (local $w i32)                                                  ;; destination cursor
  (local.set $n (i64.load (local.get $array)))                    ;; the length lives at [array+0]
  (local.set $total (i64.const 0))
  (local.set $i (i64.const 0))
  (block $mend (loop $measure
    (br_if $mend (i64.ge_s (local.get $i) (local.get $n)))
    (call $__rt_array_get_str (local.get $array) (local.get $i))
    (local.set $elen)                                             ;; pop the length
    (local.set $eptr)                                             ;; pop the pointer
    (local.set $total (i64.add (local.get $total) (local.get $elen)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $measure)))
  (if (i64.gt_s (local.get $n) (i64.const 0))
    (then (local.set $total (i64.add (local.get $total)
      (i64.mul (i64.sub (local.get $n) (i64.const 1)) (local.get $glen))))))  ;; one fewer glue than elements
  (local.set $out (call $__rt_str_alloc (local.get $total)))
  (local.set $w (i32.const 0))
  (local.set $i (i64.const 0))
  (block $wend (loop $write
    (br_if $wend (i64.ge_s (local.get $i) (local.get $n)))
    (if (i64.gt_s (local.get $i) (i64.const 0))
      (then                                                       ;; glue goes BETWEEN, not after
        (local.set $j (i64.const 0))
        (block $gend (loop $glue
          (br_if $gend (i64.ge_s (local.get $j) (local.get $glen)))
          (i32.store8 (i32.add (local.get $out) (local.get $w))
            (i32.load8_u (i32.add (local.get $gptr) (i32.wrap_i64 (local.get $j)))))
          (local.set $w (i32.add (local.get $w) (i32.const 1)))
          (local.set $j (i64.add (local.get $j) (i64.const 1)))
          (br $glue)))))
    (call $__rt_array_get_str (local.get $array) (local.get $i))
    (local.set $elen)                                             ;; pop the length
    (local.set $eptr)                                             ;; pop the pointer
    (local.set $j (i64.const 0))
    (block $eend (loop $elem
      (br_if $eend (i64.ge_s (local.get $j) (local.get $elen)))
      (i32.store8 (i32.add (local.get $out) (local.get $w))
        (i32.load8_u (i32.add (local.get $eptr) (i32.wrap_i64 (local.get $j)))))
      (local.set $w (i32.add (local.get $w) (i32.const 1)))
      (local.set $j (i64.add (local.get $j) (i64.const 1)))
      (br $elem)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $write)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))             ;; owned result
"#;

/// `__rt_explode`: splits a string on a separator into a freshly built indexed string array.
///
/// Scanning is left to right and non-overlapping. Every separator produces a boundary, so a
/// leading or trailing one yields an EMPTY element rather than being trimmed away, and the tail
/// after the last separator is always pushed — which is why `explode(",", "")` is `[""]`, one
/// empty element, and never the empty array. The caller has already refused an empty separator,
/// which is what would otherwise make this loop forever.
///
/// `__rt_array_push_str` may reallocate, so its result is threaded back rather than discarded.
pub(super) const RT_EXPLODE: &str = r#"(func $__rt_explode (param $sptr i32) (param $slen i64) (param $pptr i32) (param $plen i64) (result i32)
  (local $arr i32)                                                ;; the array being built
  (local $i i64)                                                  ;; scan cursor
  (local $start i64)                                              ;; start of the current piece
  (local.set $arr (call $__rt_array_new (i64.const 4) (i64.const 16)))  ;; specialized on first push
  (local.set $i (i64.const 0))
  (local.set $start (i64.const 0))
  (block $end (loop $scan
    (br_if $end (i64.gt_s (i64.add (local.get $i) (local.get $plen)) (local.get $slen)))
    (if (i32.wrap_i64 (call $__rt_str_region_eq
          (local.get $sptr) (local.get $pptr) (local.get $plen) (local.get $i)))
      (then
        (local.set $arr (call $__rt_array_push_str (local.get $arr)
          (i32.add (local.get $sptr) (i32.wrap_i64 (local.get $start)))
          (i64.sub (local.get $i) (local.get $start))))           ;; the piece before this separator
        (local.set $i (i64.add (local.get $i) (local.get $plen))) ;; skip it, never rescan
        (local.set $start (local.get $i)))
      (else (local.set $i (i64.add (local.get $i) (i64.const 1)))))
    (br $scan)))
  (call $__rt_array_push_str (local.get $arr)
    (i32.add (local.get $sptr) (i32.wrap_i64 (local.get $start)))
    (i64.sub (local.get $slen) (local.get $start))))              ;; the tail is always an element
"#;

/// `__rt_array_find_str`: first index whose 16-byte (pointer, length) slot equals the needle,
/// or -1.
///
/// Lives here rather than beside the other two scans because it compares through
/// `__rt_strict_str_eq` and `__rt_str_loose_eq`, which the array runtime is emitted before.
///
/// `$strict` picks the comparison: `===` is byte equality, while `==` is php-src's
/// `zendi_smart_strcmp` — so `in_array("1e1", ["10"])` is TRUE loosely and FALSE strictly.
pub(super) const RT_ARRAY_FIND_STR: &str = r#"(func $__rt_array_find_str (param $nptr i32) (param $nlen i64) (param $array i32) (param $strict i32) (result i64)
  (local $i i64) (local $len i64) (local $slot i32) (local $ep i32) (local $el i64)
  (if (i32.eqz (local.get $array))
    (then (return (i64.const -1))))
  (local.set $len (i64.load (local.get $array)))
  (block $done (loop $scan
    (br_if $done (i64.ge_s (local.get $i) (local.get $len)))
    (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24))
                              (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))
    (local.set $ep (i32.wrap_i64 (i64.load (local.get $slot))))
    (local.set $el (i64.load (i32.add (local.get $slot) (i32.const 8))))
    (if (local.get $strict)
      (then
        (if (call $__rt_strict_str_eq (local.get $nptr) (local.get $nlen) (local.get $ep) (local.get $el))
          (then (return (local.get $i)))))
      (else
        (if (i64.ne (call $__rt_str_loose_eq (local.get $nptr) (local.get $nlen) (local.get $ep) (local.get $el)) (i64.const 0))
          (then (return (local.get $i))))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $scan)))
  (i64.const -1))
"#;

/// `__rt_implode_boxed`: `implode` over a boxed PHP array of any layout, list or hash.
///
/// A declared `array` is a boxed value, so the cell may hold an indexed list of ints, strings,
/// floats or cells, or a hash — none of which the slot-reading helpers can tell apart. The
/// generic `foreach` walk answers every element as an OWNED cell in php's iteration order, so
/// the cells are gathered into a scratch list and joined by `__rt_implode_owned`'s cell arm,
/// which converts each the way an explicit `(string)` cast does. The list owns the cells and
/// freeing it releases them.
pub(super) const RT_IMPLODE_BOXED: &str = r#"(func $__rt_implode_boxed (param $cell i32) (param $gptr i32) (param $glen i64) (result i32) (result i64)
  (local $src i32) (local $cursor i64) (local $kind i32) (local $more i64)
  (local $list i32) (local $ptr i32) (local $len i64)
  (call $__rt_mixed_iter_start (local.get $cell))
  (local.set $kind)
  (local.set $cursor)
  (local.set $src)
  (local.set $list (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (block $done (loop $entry
    (call $__rt_mixed_iter_next (local.get $src) (local.get $cursor) (local.get $kind))
    (local.set $more)
    (local.set $cursor)
    (br_if $done (i64.eqz (local.get $more)))
    (local.set $list (call $__rt_array_push_mixed (local.get $list)
      (call $__rt_mixed_iter_value (local.get $src) (local.get $cursor) (local.get $kind))))
    (br $entry)))
  (call $__rt_implode_owned (local.get $list) (local.get $gptr) (local.get $glen) (i32.const 1))
  (local.set $len)
  (local.set $ptr)
  (call $__rt_decref_any (local.get $list))
  (local.get $ptr) (local.get $len))
"#;

/// `__rt_implode_owned`: joins an array whose elements must be CONVERTED to strings first.
///
/// `$kind` selects the conversion: 0 renders an integer slot, 1 casts a boxed `Mixed` cell. PHP
/// converts each element with the same rule as an explicit `(string)` cast — measured element by
/// element — so the Mixed arm reuses `__rt_mixed_cast_string` rather than inventing one.
///
/// Both conversions PRODUCE an owned block, so each piece is released once copied; skipping that
/// would leak one string per element. The string-element case has no conversion and no ownership
/// to manage, which is why it keeps its own simpler helper.
///
/// Two passes over a scratch table rather than repeated concatenation: converting once and
/// remembering `(pointer, length)` keeps this linear, where pairwise appending would copy the
/// accumulated prefix again for every element.
pub(super) const RT_IMPLODE_OWNED: &str = r#"(func $__rt_implode_owned (param $array i32) (param $gptr i32) (param $glen i64) (param $kind i32) (result i32) (result i64)
  (local $cell i32)
  (local $n i64) (local $i i64) (local $j i64) (local $total i64)
  (local $table i32) (local $eptr i32) (local $elen i64) (local $out i32) (local $w i32)
  (local.set $n (i64.load (local.get $array)))
  (local.set $table (call $__rt_heap_alloc
    (i32.add (i32.const 8) (i32.wrap_i64 (i64.mul (local.get $n) (i64.const 16))))))
  (local.set $total (i64.const 0))
  (local.set $i (i64.const 0))
  (block $me (loop $ml                                            ;; cast once, remember the piece
    (br_if $me (i64.ge_s (local.get $i) (local.get $n)))
    (if (i32.eq (local.get $kind) (i32.const 1))
      (then
        ;; a Mixed-cell array has 16-byte slots (value_type 7) with the cell pointer at
        ;; slot+0 — NOT the 8-byte stride __rt_array_get_int walks
        (call $__rt_mixed_cast_string
          (i32.wrap_i64 (i64.load (i32.add (i32.add (local.get $array) (i32.const 24))
                                           (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))))
        (local.set $elen (i64.extend_i32_u))                       ;; the cast answers an i32 length
        (local.set $eptr))
      (else (if (i32.eq (local.get $kind) (i32.const 2))
        (then
          ;; a float renders exactly as the (string) cast does, and only the cast knows that
          ;; rule — so box the slot's bits into a throwaway tag-2 cell and reuse it
          (local.set $cell (call $__rt_mixed_from_value (i64.const 2)
            (i64.load (i32.add (i32.add (local.get $array) (i32.const 24))
                               (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8)))))
            (i64.const 0)))
          (call $__rt_mixed_cast_string (local.get $cell))
          (local.set $elen (i64.extend_i32_u))
          (local.set $eptr)
          (call $__rt_decref_any (local.get $cell)))               ;; the box was ours alone
        (else
          (call $__rt_fmt_int (call $__rt_array_get_int (local.get $array) (local.get $i))
                (i32.const 0) (i64.const 0) (i32.const 32) (i32.const 0))  ;; plain decimal, no field
          (local.set $elen)
          (local.set $eptr)))))
    (i32.store (i32.add (local.get $table) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16))))
               (local.get $eptr))
    (i64.store offset=8 (i32.add (local.get $table)
                                 (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16))))
               (local.get $elen))
    (local.set $total (i64.add (local.get $total) (local.get $elen)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $ml)))
  (if (i64.gt_s (local.get $n) (i64.const 0))
    (then (local.set $total (i64.add (local.get $total)
      (i64.mul (i64.sub (local.get $n) (i64.const 1)) (local.get $glen))))))
  (local.set $out (call $__rt_str_alloc (local.get $total)))
  (local.set $w (i32.const 0))
  (local.set $i (i64.const 0))
  (block $we (loop $wl
    (br_if $we (i64.ge_s (local.get $i) (local.get $n)))
    (if (i64.gt_s (local.get $i) (i64.const 0))
      (then                                                       ;; glue goes BETWEEN, not after
        (local.set $j (i64.const 0))
        (block $ge (loop $gl
          (br_if $ge (i64.ge_s (local.get $j) (local.get $glen)))
          (i32.store8 (i32.add (local.get $out) (local.get $w))
            (i32.load8_u (i32.add (local.get $gptr) (i32.wrap_i64 (local.get $j)))))
          (local.set $w (i32.add (local.get $w) (i32.const 1)))
          (local.set $j (i64.add (local.get $j) (i64.const 1)))
          (br $gl)))))
    (local.set $eptr (i32.load (i32.add (local.get $table)
      (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16))))))
    (local.set $elen (i64.load offset=8 (i32.add (local.get $table)
      (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16))))))
    (local.set $j (i64.const 0))
    (block $ee (loop $el
      (br_if $ee (i64.ge_s (local.get $j) (local.get $elen)))
      (i32.store8 (i32.add (local.get $out) (local.get $w))
        (i32.load8_u (i32.add (local.get $eptr) (i32.wrap_i64 (local.get $j)))))
      (local.set $w (i32.add (local.get $w) (i32.const 1)))
      (local.set $j (i64.add (local.get $j) (i64.const 1)))
      (br $el)))
    (if (local.get $eptr)
      (then (call $__rt_heap_free (local.get $eptr))))            ;; the cast owned it; release it
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $wl)))
  (call $__rt_heap_free (local.get $table))
  (local.get $out) (i64.extend_i32_u (local.get $w)))
"#;
