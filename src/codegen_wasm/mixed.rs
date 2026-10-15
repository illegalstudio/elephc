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
pub(super) fn emit_mixed_runtime(
    wm: &mut WatModule,
    has_main: bool,
    module: Option<&crate::ir::Module>,
) {
    // The string cast calls into this, so the two are emitted together rather than left to
    // every caller to pair up: a harness that emitted one without the other failed to assemble.
    super::objects::emit_object_to_string_dispatch(wm, module);
    wm.add_raw_func(RT_MIXED_FROM_VALUE);
    wm.add_raw_func(RT_MIXED_UNBOX);
    wm.add_raw_func(RT_MIXED_FREE_DEEP);
    wm.add_raw_func(RT_DECREF_MIXED);
    wm.add_raw_func(RT_MIXED_NARROW_INT);
    wm.add_raw_func(RT_MIXED_CAST_BOOL);
    wm.add_raw_func(&mixed_cast_int(has_main));
    wm.add_raw_func(&mixed_cast_float(has_main));
    wm.add_raw_func(&mixed_cast_string(has_main));
    wm.add_raw_func(RT_MIXED_CAST_STRING_REF);
    if has_main {
        // Truthiness warns for a NaN, so it needs WASI like the other diagnostic-producing
        // rules; a reactor keeps the silent cast.
        wm.add_raw_func(RT_MIXED_TRUTHY);
        wm.add_raw_func(RT_FLOAT_TRUTHY);
        // The `foreach` dispatch warns through WASI for a value PHP will not iterate, so it is
        // a command-module rule like every other diagnostic-producing one.
        // A boxed value can hold a GENERATOR (a second call of a closure that yields is typed
        // `mixed`), so a module with generators teaches the dynamic walk a fourth kind.
        let generators = module.is_some_and(|m| !super::generators::generator_functions(m).is_empty());
        let expand = |template: &str, parts: &[(&str, String)]| {
            let mut out = template.to_string();
            for (marker, generator_arm) in parts {
                out = out.replace(marker, if generators { generator_arm.as_str() } else { "" });
            }
            out
        };
        use crate::codegen_support::runtime::generators::frame as gf;
        wm.add_raw_func(&expand(
            RT_MIXED_ITER_START,
            &[(
                ";;GEN_START",
                r#"  (if (i64.eq (local.get $tag) (i64.const 6))                     ;; tag 6 = object
    (then
      (if (i64.eq (i64.and (i64.load (i32.sub (i32.wrap_i64 (local.get $lo)) (i32.const 8))) (i64.const 255)) (i64.const 7))
        (then (return (i32.wrap_i64 (local.get $lo)) (i64.const 0) (i32.const 3))))))   ;; a generator frame
"#
                .to_string(),
            )],
        ));
        wm.add_raw_func(RT_ITERABLE_ITER_START);
        wm.add_raw_func(&expand(
            RT_MIXED_ITER_NEXT,
            &[(
                ";;GEN_NEXT",
                r#"  (if (i32.eq (local.get $kind) (i32.const 3))
    (then (return (local.get $cursor) (i64.extend_i32_u (call $__rt_gen_foreach_step (local.get $src))))))
"#
                .to_string(),
            )],
        ));
        wm.add_raw_func(&expand(
            RT_MIXED_ITER_KEY,
            &[(
                ";;GEN_KEY",
                format!(
                    r#"  (if (i32.eq (local.get $kind) (i32.const 3))
    (then
      (local.set $cell (i32.wrap_i64 (i64.load offset={} (local.get $src))))
      (if (i32.eqz (local.get $cell))
        (then (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
      (call $__rt_incref (local.get $cell))
      (return (local.get $cell))))
"#,
                    gf::OFF_LAST_KEY
                ),
            )],
        ));
        wm.add_raw_func(&expand(
            RT_MIXED_ITER_VALUE,
            &[(
                ";;GEN_VALUE",
                format!(
                    r#"  (if (i32.eq (local.get $kind) (i32.const 3))
    (then
      (local.set $cell (i32.wrap_i64 (i64.load offset={} (local.get $src))))
      (if (i32.eqz (local.get $cell))
        (then (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
      (call $__rt_incref (local.get $cell))
      (return (local.get $cell))))
"#,
                    gf::OFF_LAST_VALUE
                ),
            )],
        ));
    }
    if has_main {
        wm.add_raw_func(RT_MIXED_ARRAY_GET_MISSING);
        wm.add_raw_func(RT_MIXED_ARRAY_GET);
        // The writes are command-only too: their failures are php fatals through `__rt_fail`.
        wm.add_raw_func(RT_MIXED_CELL_CONTAINER);
        wm.add_raw_func(RT_MIXED_CELL_OWN_LIST);
        wm.add_raw_func(RT_MIXED_CELL_APPEND);
        wm.add_raw_func(RT_MIXED_CELL_SET);
        wm.add_raw_func(RT_MIXED_CELL_UNSHIFT);
    }
}

/// `__rt_mixed_cell_unshift`: `array_unshift` through a boxed array, republished in its cell.
///
/// `$values` is a list of cells, in order; the result keeps its own references to them. A list
/// becomes the values followed by its elements. A hash is rebuilt the way php rebuilds it: the
/// values under keys 0.., then each entry, an integer key renumbered and a string key kept.
/// Answers the new element count, or -1 when the cell holds no array.
const RT_MIXED_CELL_UNSHIFT: &str = r#"(func $__rt_mixed_cell_unshift (param $cell i32) (param $values i32) (result i64)
  (local $c i32) (local $tag i64) (local $old i32) (local $out i32) (local $i i64) (local $n i64)
  (local $src i32) (local $cursor i64) (local $kind i32) (local $more i64)
  (local $k i32) (local $v i32) (local $ktag i64) (local $klo i64) (local $khi i64)
  (local $vtag i64) (local $vlo i64) (local $vhi i64) (local $value i32)
  (local.set $c (call $__rt_mixed_cell_container (local.get $cell)))
  (local.set $tag (i64.load (local.get $c)))
  (local.set $n (i64.load (local.get $values)))
  (if (i64.eq (local.get $tag) (i64.const 4))
    (then
      (local.set $old (call $__rt_mixed_cell_own_list (local.get $c)))
      (local.set $out (call $__rt_array_new
        (i64.add (local.get $n) (i64.load (local.get $old))) (i64.const 16)))
      (block $vdone (loop $vals
        (br_if $vdone (i64.ge_s (local.get $i) (local.get $n)))
        (local.set $value (i32.wrap_i64 (i64.load (i32.add (i32.add (local.get $values) (i32.const 24))
          (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))))
        (call $__rt_incref (local.get $value))                         ;; the result's own reference
        (local.set $out (call $__rt_array_push_mixed (local.get $out) (local.get $value)))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $vals)))
      (local.set $i (i64.const 0))
      (block $edone (loop $elements
        (br_if $edone (i64.ge_s (local.get $i) (i64.load (local.get $old))))
        (local.set $out (call $__rt_array_push_mixed (local.get $out)
          (call $__rt_array_elem_to_mixed (local.get $old) (local.get $i))))  ;; an owned copy
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $elements)))
      (call $__rt_decref_any (local.get $old))                          ;; the cell's old list
      (i64.store offset=8 (local.get $c) (i64.extend_i32_u (local.get $out)))
      (return (i64.load (local.get $out)))))
  (if (i64.ne (local.get $tag) (i64.const 5)) (then (return (i64.const -1))))
  (local.set $out (call $__rt_hash_new (i64.const 8) (i64.const 7)))
  (block $vdone (loop $vals
    (br_if $vdone (i64.ge_s (local.get $i) (local.get $n)))
    (call $__rt_mixed_unbox (i32.wrap_i64 (i64.load (i32.add (i32.add (local.get $values) (i32.const 24))
      (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))))
    (local.set $vhi)
    (local.set $vlo)
    (local.set $vtag)
    (local.set $out (call $__rt_hash_append (local.get $out) (local.get $vlo) (local.get $vhi) (local.get $vtag)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $vals)))
  (call $__rt_mixed_iter_start (local.get $c))
  (local.set $kind)
  (local.set $cursor)
  (local.set $src)
  (block $done (loop $entry
    (call $__rt_mixed_iter_next (local.get $src) (local.get $cursor) (local.get $kind))
    (local.set $more)
    (local.set $cursor)
    (br_if $done (i64.eqz (local.get $more)))
    (local.set $k (call $__rt_mixed_iter_key (local.get $src) (local.get $cursor) (local.get $kind)))
    (local.set $v (call $__rt_mixed_iter_value (local.get $src) (local.get $cursor) (local.get $kind)))
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
    (call $__rt_decref_any (local.get $k))
    (call $__rt_decref_any (local.get $v))
    (br $entry)))
  (call $__rt_decref_any (i32.wrap_i64 (i64.load offset=8 (local.get $c))))   ;; the cell's old hash
  (i64.store offset=8 (local.get $c) (i64.extend_i32_u (local.get $out)))
  (i64.load (local.get $out)))
"#;

/// `__rt_mixed_cell_container`: the cell that actually holds a value, past any nested cells.
const RT_MIXED_CELL_CONTAINER: &str = r#"(func $__rt_mixed_cell_container (param $cell i32) (result i32)
  (local $inner i32)
  (block $done (loop $walk
    (br_if $done (i64.ne (i64.load (local.get $cell)) (i64.const 7)))  ;; not nested -> this cell
    (local.set $inner (i32.wrap_i64 (i64.load offset=8 (local.get $cell))))
    (br_if $done (i32.eqz (local.get $inner)))                          ;; a nested null stays put
    (local.set $cell (local.get $inner))
    (br $walk)))
  (local.get $cell))
"#;

/// `__rt_mixed_cell_own_list`: makes the list a tag-4 cell holds a list of Mixed cells.
///
/// A boxed list can hold any element layout — `f([1, 2])` boxes raw int slots — while a write
/// stores a cell. A non-empty list of another layout is rebuilt as cells, every element read
/// through `__rt_array_elem_to_mixed`, and the cell's reference moves to the rebuilt list. An
/// empty list, or one already of cells, is left as is: the push and set helpers split a shared
/// one and shape an empty one themselves.
const RT_MIXED_CELL_OWN_LIST: &str = r#"(func $__rt_mixed_cell_own_list (param $cell i32) (result i32)
  (local $list i32) (local $len i64) (local $i i64) (local $out i32)
  (local.set $list (i32.wrap_i64 (i64.load offset=8 (local.get $cell))))
  (local.set $len (i64.load (local.get $list)))
  (if (i32.or (i64.eqz (local.get $len))
              (i64.eq (i64.and (i64.shr_u (i64.load (i32.sub (local.get $list) (i32.const 8)))
                                          (i64.const 8))
                               (i64.const 127))
                      (i64.const 7)))                                   ;; value_type 7: cells
    (then (return (local.get $list))))
  (local.set $out (call $__rt_array_new (local.get $len) (i64.const 16)))
  (block $copied (loop $element
    (br_if $copied (i64.ge_s (local.get $i) (local.get $len)))
    (local.set $out (call $__rt_array_push_mixed (local.get $out)
      (call $__rt_array_elem_to_mixed (local.get $list) (local.get $i))))  ;; owned cell, kept
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $element)))
  (call $__rt_decref_any (local.get $list))                            ;; the cell's old reference
  (i64.store offset=8 (local.get $cell) (i64.extend_i32_u (local.get $out)))
  (local.get $out))
"#;

/// `__rt_mixed_cell_append`: `$a[] = $v` where `$a` is a boxed array, republished in its cell.
///
/// `$v` is an OWNED cell. A list takes it as its new last element; a hash appends under its next
/// integer key, copying the value, so the cell is released. Answers 1 on success, 0 when php's
/// next key is already taken (`Cannot add element to the array as the next element is already
/// occupied`) and 2 when the cell holds no array. A null cell becomes an empty list first, as
/// php's `$null[] = $v` does.
const RT_MIXED_CELL_APPEND: &str = r#"(func $__rt_mixed_cell_append (param $cell i32) (param $v i32) (result i32)
  (local $c i32) (local $tag i64) (local $list i32) (local $hash i32)
  (local $vtag i64) (local $vlo i64) (local $vhi i64)
  (local.set $c (call $__rt_mixed_cell_container (local.get $cell)))
  (local.set $tag (i64.load (local.get $c)))
  (if (i64.eq (local.get $tag) (i64.const 8))
    (then
      (i64.store offset=8 (local.get $c)
        (i64.extend_i32_u (call $__rt_array_new (i64.const 0) (i64.const 16))))
      (i64.store offset=16 (local.get $c) (i64.const 0))
      (i64.store (local.get $c) (i64.const 4))
      (local.set $tag (i64.const 4))))
  (if (i64.eq (local.get $tag) (i64.const 4))
    (then
      (local.set $list (call $__rt_mixed_cell_own_list (local.get $c)))
      (local.set $list (call $__rt_array_push_mixed (local.get $list) (local.get $v)))
      (i64.store offset=8 (local.get $c) (i64.extend_i32_u (local.get $list)))
      (return (i32.const 1))))
  (if (i64.eq (local.get $tag) (i64.const 5))
    (then
      (call $__rt_mixed_unbox (local.get $v))
      (local.set $vhi)
      (local.set $vlo)
      (local.set $vtag)
      (local.set $hash (call $__rt_hash_append
        (i32.wrap_i64 (i64.load offset=8 (local.get $c)))
        (local.get $vlo) (local.get $vhi) (local.get $vtag)))
      (call $__rt_decref_any (local.get $v))                           ;; the hash copied it
      (if (i32.eqz (local.get $hash)) (then (return (i32.const 0))))
      (i64.store offset=8 (local.get $c) (i64.extend_i32_u (local.get $hash)))
      (return (i32.const 1))))
  (call $__rt_decref_any (local.get $v))
  (i32.const 2))
"#;

/// `__rt_mixed_cell_set`: `$a[$k] = $v` where `$a` is a boxed array, republished in its cell.
///
/// The key arrives normalized the way `__rt_hash_set` wants it — `(value, -1)` for an integer,
/// `(pointer, length)` for a string, with integer-like strings already collapsed. A list keeps
/// being a list while the key is an integer inside it or right at its end; any other key turns
/// it into a hash first, as php's packed-to-hash conversion does. `$v` is an OWNED cell. Answers
/// 1 on success and 2 when the cell holds no array.
const RT_MIXED_CELL_SET: &str = r#"(func $__rt_mixed_cell_set (param $cell i32) (param $klo i64) (param $khi i64) (param $v i32) (result i32)
  (local $c i32) (local $tag i64) (local $list i32) (local $hash i32) (local $len i64)
  (local $vtag i64) (local $vlo i64) (local $vhi i64)
  (local.set $c (call $__rt_mixed_cell_container (local.get $cell)))
  (local.set $tag (i64.load (local.get $c)))
  (if (i64.eq (local.get $tag) (i64.const 8))
    (then
      (i64.store offset=8 (local.get $c)
        (i64.extend_i32_u (call $__rt_array_new (i64.const 0) (i64.const 16))))
      (i64.store offset=16 (local.get $c) (i64.const 0))
      (i64.store (local.get $c) (i64.const 4))
      (local.set $tag (i64.const 4))))
  (if (i64.eq (local.get $tag) (i64.const 4))
    (then
      (local.set $list (i32.wrap_i64 (i64.load offset=8 (local.get $c))))
      (local.set $len (i64.load (local.get $list)))
      (if (i32.and (i64.eq (local.get $khi) (i64.const -1))
                   (i32.and (i64.ge_s (local.get $klo) (i64.const 0))
                            (i64.le_s (local.get $klo) (local.get $len))))
        (then
          (local.set $list (call $__rt_mixed_cell_own_list (local.get $c)))
          (if (i64.eq (local.get $klo) (local.get $len))
            (then (local.set $list (call $__rt_array_push_mixed (local.get $list) (local.get $v))))
            (else (local.set $list
              (call $__rt_array_set_mixed (local.get $list) (local.get $klo) (local.get $v)))))
          (i64.store offset=8 (local.get $c) (i64.extend_i32_u (local.get $list)))
          (return (i32.const 1))))
      ;; Any other key makes it a hash; split a shared list first, since the conversion consumes it.
      (local.set $list (call $__rt_array_ensure_unique (local.get $list)))
      (local.set $hash (call $__rt_array_to_hash (local.get $list) (i32.const 0)))
      (i64.store offset=8 (local.get $c) (i64.extend_i32_u (local.get $hash)))
      (i64.store (local.get $c) (i64.const 5))
      (local.set $tag (i64.const 5))))
  (if (i64.eq (local.get $tag) (i64.const 5))
    (then
      (call $__rt_mixed_unbox (local.get $v))
      (local.set $vhi)
      (local.set $vlo)
      (local.set $vtag)
      (local.set $hash (call $__rt_hash_set
        (i32.wrap_i64 (i64.load offset=8 (local.get $c)))
        (local.get $klo) (local.get $khi) (local.get $vlo) (local.get $vhi) (local.get $vtag)))
      (call $__rt_decref_any (local.get $v))                           ;; the hash copied it
      (i64.store offset=8 (local.get $c) (i64.extend_i32_u (local.get $hash)))
      (return (i32.const 1))))
  (call $__rt_decref_any (local.get $v))
  (i32.const 2))
"#;

/// `__rt_mixed_array_get`: reads `$mixed[$key]`, dispatching on the receiver's RUNTIME tag.
///
/// The receiver is genuinely unknown at compile time — `$deep["db"]["host"]` reaches this
/// because the inner read answers `mixed` — so nothing here can be decided by the checker. The
/// key arrives already normalized the way `__rt_hash_get` wants it: `(value, -1)` is an integer
/// key and `(pointer, length)` a string one, which is also what makes `$a["1"]` find the element
/// `$a[1]` holds. `warn` is zero for `isset()` and `??`, which read the same way but say nothing.
///
/// Every answer is an OWNED cell. Two arms go further than the native backend, which answers a
/// silent null for both and is wrong about it: a STRING receiver is indexed by byte, and a
/// scalar receiver warns before answering null. Two arms deliberately match native rather than
/// php-src, because the message php-src raises is a `TypeError`/`Error` this backend has no data
/// for: indexing a string with a string key, and indexing an object.
///
/// Emitted only into main-bearing modules — it calls the warning helpers, which are part of the
/// command runtime. The capability audit refuses the shape everywhere else.
const RT_MIXED_ARRAY_GET: &str = r#"(func $__rt_mixed_array_get (param $cell i32) (param $key_lo i64) (param $key_hi i64) (param $warn i32) (result i32)
  (local $tag i64)
  (local $lo i64)
  (local $hi i64)
  (local $elem i32)
  (local $found i32)
  (local $vlo i64)
  (local $vhi i64)
  (local $vtag i64)
  (local $index i64)
  (call $__rt_mixed_unbox (local.get $cell))                ;; -> (tag, lo, hi), hi on top
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 4))               ;; indexed array
    (then
      (if (i64.ne (local.get $key_hi) (i64.const -1))       ;; a string key never indexes a list
        (then (return (call $__rt_mixed_array_get_missing (local.get $key_lo) (local.get $key_hi) (local.get $warn)))))
      (local.set $elem (call $__rt_array_elem_to_mixed (i32.wrap_i64 (local.get $lo)) (local.get $key_lo)))
      (if (i32.eqz (local.get $elem))                       ;; absent index
        (then (return (call $__rt_mixed_array_get_missing (local.get $key_lo) (local.get $key_hi) (local.get $warn)))))
      (return (local.get $elem))))
  (if (i64.eq (local.get $tag) (i64.const 5))               ;; hash
    (then
      (call $__rt_hash_get (i32.wrap_i64 (local.get $lo)) (local.get $key_lo) (local.get $key_hi))
      (local.set $vtag)                                     ;; -> (found, value_lo, value_hi, value_tag)
      (local.set $vhi)
      (local.set $vlo)
      (local.set $found)
      (if (i32.eqz (local.get $found))
        (then (return (call $__rt_mixed_array_get_missing (local.get $key_lo) (local.get $key_hi) (local.get $warn)))))
      (return (call $__rt_mixed_from_value (local.get $vtag) (local.get $vlo) (local.get $vhi)))))
  (if (i64.eq (local.get $tag) (i64.const 1))               ;; string: one byte, by offset
    (then
      (if (i64.ne (local.get $key_hi) (i64.const -1))       ;; php-src raises a TypeError here; native answers null
        (then (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
      (local.set $index (local.get $key_lo))
      (if (i64.lt_s (local.get $index) (i64.const 0))       ;; a negative offset counts from the end
        (then (local.set $index (i64.add (local.get $index) (local.get $hi)))))
      (if (i32.or (i64.lt_s (local.get $index) (i64.const 0))
                  (i64.ge_s (local.get $index) (local.get $hi)))
        (then
          (if (local.get $warn)                             ;; the index is reported AS WRITTEN
            (then (call $__rt_warn_uninit_string_offset (local.get $key_lo))))
          (return (call $__rt_mixed_from_value (i64.const 1) (local.get $lo) (i64.const 0)))))
      (return (call $__rt_mixed_from_value (i64.const 1)
        (i64.add (local.get $lo) (local.get $index))
        (i64.const 1)))))
  (if (i64.eq (local.get $tag) (i64.const 8))               ;; null receiver
    (then
      (if (local.get $warn)
        (then (call $__rt_warn_array_offset_on_null)))
      (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
  (if (i32.or (i32.or
        (i64.eq (local.get $tag) (i64.const 0))             ;; int
        (i64.eq (local.get $tag) (i64.const 2)))            ;; float
        (i64.eq (local.get $tag) (i64.const 3)))            ;; bool
    (then
      (if (local.get $warn)
        (then (call $__rt_warn_array_offset_on_scalar (local.get $tag) (local.get $lo))))
      (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
  (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))
"#;

/// `__rt_mixed_array_get_missing`: the shared miss answer — warn about the key, then box null.
///
/// The two key shapes need different diagnostics, and PHP quotes a string key but not an
/// integer one. Split out because three arms of the read reach the same place.
const RT_MIXED_ARRAY_GET_MISSING: &str = r#"(func $__rt_mixed_array_get_missing (param $key_lo i64) (param $key_hi i64) (param $warn i32) (result i32)
  (if (local.get $warn)
    (then
      (if (i64.eq (local.get $key_hi) (i64.const -1))
        (then (call $__rt_warn_undefined_array_key_int (local.get $key_lo)))
        (else (call $__rt_warn_undefined_array_key_str
          (i32.wrap_i64 (local.get $key_lo))
          (i32.wrap_i64 (local.get $key_hi)))))))
  (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))
"#;

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
      (call $__rt_str_persist (i32.wrap_i64 (local.get $lo)) (local.get $hi))    ;; persist string into an owned heap copy
      (local.set $sl)                                                   ;; persisted length
      (local.set $sp)                                                   ;; persisted pointer
      (local.set $lo (i64.extend_i32_u (local.get $sp)))                         ;; update lo with persisted pointer
      (local.set $hi (local.get $sl)))                                           ;; update hi with persisted length
    (else
      (if (i32.or (i32.or (i32.or (i32.or
            (i64.eq (local.get $tag) (i64.const 4))
            (i64.eq (local.get $tag) (i64.const 5)))
            (i64.eq (local.get $tag) (i64.const 6)))
            (i64.eq (local.get $tag) (i64.const 7)))
            (i64.eq (local.get $tag) (i64.const 10)))                   ;; refcounted child: share ownership
        (then (call $__rt_incref (i32.wrap_i64 (local.get $lo)))))))             ;; incref the refcounted child
  (i64.store (local.get $cell) (local.get $tag))                        ;; tag @ +0
  (i64.store (i32.add (local.get $cell) (i32.const 8)) (local.get $lo)) ;; lo @ +8
  (i64.store (i32.add (local.get $cell) (i32.const 16)) (local.get $hi)) ;; hi @ +16
  (local.get $cell))                                                             ;; return the new Mixed cell
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
      (br $L)))                                                                  ;; back to top for next nested level
  (i64.load (local.get $ptr))                                           ;; result 0: tag
  (i64.load (i32.add (local.get $ptr) (i32.const 8)))                   ;; result 1: lo
  (i64.load (i32.add (local.get $ptr) (i32.const 16))))                 ;; result 2: hi
"#;

/// KNOWN GAP (unfixed on purpose): a resource cell (tag 9) releases nothing here, so a
/// `php://memory` / `php://temp` stream that is never `fclose()`d leaks its descriptor and buffer
/// when the last value naming it is dropped (`function w() { $h = fopen("php://memory", "w+");
/// ... }` in a loop). Freeing the stream from this function is NOT safe: the handle is shared
/// by value, not by cell. A resource stored in a hash or array element is kept inline as
/// `(tag 9, handle)` and re-boxed into a FRESH cell on every read, so dropping one of those
/// temporary cells would close a stream the container still holds. The release has to hang off
/// a per-stream refcount instead of the cell.
///
/// `__rt_mixed_free_deep`: releases the cell's owned child (a persisted string via
/// `__rt_heap_free_safe`, or a refcounted container via `__rt_decref_any`) then
/// frees the cell itself.
const RT_MIXED_FREE_DEEP: &str = r#"(func $__rt_mixed_free_deep (param $ptr i32)
  (local $tag i64)
  (if (i32.eqz (local.get $ptr))
    (then (return)))                                                             ;; null check
  (local.set $tag (i64.load (local.get $ptr)))                          ;; tag @ +0
  (if (i64.eq (local.get $tag) (i64.const 1))                           ;; string: free the owned copy
    (then
      (call $__rt_heap_free_safe (i32.wrap_i64 (i64.load (i32.add (local.get $ptr) (i32.const 8)))))) ;; free the owned string copy
    (else
      (if (i32.or (i32.or (i32.or (i32.or
            (i64.eq (local.get $tag) (i64.const 4))
            (i64.eq (local.get $tag) (i64.const 5)))
            (i64.eq (local.get $tag) (i64.const 6)))
            (i64.eq (local.get $tag) (i64.const 7)))
            (i64.eq (local.get $tag) (i64.const 10)))                   ;; refcounted child: release it
        (then (call $__rt_decref_any (i32.wrap_i64 (i64.load (i32.add (local.get $ptr) (i32.const 8))))))))) ;; release the refcounted child
  (call $__rt_heap_free (local.get $ptr)))                                       ;; free the cell itself
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
  (i32.store (i32.sub (local.get $ptr) (i32.const 12)) (local.get $rc))          ;; store decremented refcount
  (if (i32.eqz (local.get $rc))
    (then (call $__rt_mixed_free_deep (local.get $ptr)))))              ;; last owner -> deep free
"#;

/// `__rt_mixed_narrow_int`: the IMPLICIT Mixed-to-int coercion, silent.
///
/// Distinct from `__rt_mixed_cast_int`, which implements the EXPLICIT `(int)` cast: that one
/// warns when a float is out of range and wraps it, exactly as PHP does for `(int) $f`. An
/// implicit narrowing is a different operation — PHP does not perform one here at all, so
/// borrowing the cast's diagnostics would print a warning for a program PHP runs silently.
///
/// Where the value cannot be represented, this SATURATES and says nothing, which is what the
/// native backend does for the same coercion. The two targets therefore answer alike, and the
/// remaining divergence from PHP is the shared EIR one: a local widened to Mixed by checked
/// arithmetic is still read back as an `int`, so a genuine overflow loses its float-ness on both.
const RT_MIXED_NARROW_INT: &str = r#"(func $__rt_mixed_narrow_int (param $ptr i32) (result i64)
  (local $tag i64)
  (local $lo i64)
  (local $hi i64)
  (local $f f64)
  (call $__rt_mixed_unbox (local.get $ptr))                       ;; -> stack: tag, lo, hi
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 2))                     ;; float: saturate, do not warn
    (then
      (local.set $f (f64.reinterpret_i64 (local.get $lo)))
      (if (f64.ne (local.get $f) (local.get $f))
        (then (return (i64.const 0))))                            ;; NaN narrows to 0
      (if (f64.ge (local.get $f) (f64.const 9223372036854775808))
        (then (return (i64.const 9223372036854775807))))
      (if (f64.le (local.get $f) (f64.const -9223372036854775808))
        (then (return (i64.const -9223372036854775808))))
      (return (i64.trunc_f64_s (local.get $f)))))
  (if (i64.eq (local.get $tag) (i64.const 0))                     ;; int and bool carry their word
    (then (return (local.get $lo))))
  (if (i64.eq (local.get $tag) (i64.const 3))
    (then (return (local.get $lo))))
  (call $__rt_mixed_cast_int (local.get $ptr)))                   ;; anything else keeps the cast
"#;

/// `__rt_mixed_cast_bool`: casts a boxed Mixed cell to a PHP boolean (0 or 1)
/// following PHP `(bool)` truthiness, mirroring the native `__rt_mixed_cast_bool`
/// tag dispatch byte-for-byte. Unboxes via `__rt_mixed_unbox` (which already
/// unwraps nested tag-7 cells and maps null to tag 8), then per tag: int/float are
/// truthy when non-zero (float via an integer bit test, so NaN is truthy and ±0.0
/// is falsy); a string is falsy only when "" or the single byte "0"; a bool returns
/// its stored 0/1; an array/hash is truthy when non-null and its count (i64 at
/// offset 0) is non-zero; an object, resource or callable (tag 10) is always truthy; null
/// and any other tag are falsy. Borrows the cell (never frees/mutates it).

/// `__rt_mixed_truthy`: PHP's truthiness of a BOXED value, with the one diagnostic it carries.
///
/// Every per-tag ANSWER already lives in `__rt_mixed_cast_bool` and is exact — measured on
/// php-src 8.5.6 across seventeen arms, including the two that trip people up: `"0.0"` is TRUE
/// (only the single character `"0"` is false) and `-0.0` is FALSE like `+0.0`. What was missing
/// is that a NaN WARNS on the way, and is then TRUE. So this wraps rather than restates.
const RT_MIXED_TRUTHY: &str = r#"(func $__rt_mixed_truthy (param $cell i32) (result i64)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $f f64)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 2))                     ;; tag 2 = float
    (then
      (local.set $f (f64.reinterpret_i64 (local.get $lo)))
      (if (f64.ne (local.get $f) (local.get $f))                  ;; NaN is the only value unequal to itself
        (then (call $__rt_deprecate_nan_to_bool)))))
  (call $__rt_mixed_cast_bool (local.get $cell)))
"#;

/// `__rt_float_truthy`: the same for a CONCRETE float, which carries the same NaN warning.
///
/// `0.0` and `-0.0` are both false and everything else — the infinities and NaN included — is
/// true, so the answer is a bit test rather than a float comparison.
const RT_FLOAT_TRUTHY: &str = r#"(func $__rt_float_truthy (param $bits i64) (result i64)
  (local $f f64)
  (local.set $f (f64.reinterpret_i64 (local.get $bits)))
  (if (f64.ne (local.get $f) (local.get $f))
    (then (call $__rt_deprecate_nan_to_bool)))
  (i64.extend_i32_u
    (i32.and
      (i64.ne (local.get $bits) (i64.const 0))
      (i64.ne (local.get $bits) (i64.const 0x8000000000000000)))))
"#;

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
  (if (i32.or (i32.or (i64.eq (local.get $tag) (i64.const 6)) (i64.eq (local.get $tag) (i64.const 9)))
              (i64.eq (local.get $tag) (i64.const 10)))               ;; tag 6 object / 9 resource / 10 closure
    (then (return (i64.const 1))))                                    ;; always truthy (a closure is an object)
  (i64.const 0))                                                      ;; tag 8 null / other = false
"#;

/// `__rt_mixed_cast_int`: casts a boxed Mixed cell to a PHP int, mirroring the
/// native `__rt_mixed_cast_int` tag dispatch. Unboxes via `__rt_mixed_unbox` (which
/// unwraps nested tag-7 cells and maps null to tag 8), then per tag: an int forwards its
/// payload; a string casts through `__rt_str_to_int` (PHP string->int: saturating
/// integer-form, float-form finite truncates toward zero with saturation, +/-INF or
/// NaN -> 0); a float uses `__rt_float_to_int`, whose finite out-of-range results wrap
/// modulo 2^64 like `zend_dval_to_lval` while +/-INF/NaN become 0; a bool forwards its
/// 0/1; an array/hash returns its element count (or 0 for a null container); a resource
/// returns its 1-based display id (payload + 1); null/object/callable/other return 0.

/// Renders `__rt_mixed_cast_int` for this module.
///
/// Two things vary. The float arm routes through the diagnosing conversion under the PHP 8.5
/// profile, which is the only profile that reports an unrepresentable float. And the object arm
/// can only warn from a COMMAND module: `__rt_warn_object_to_int` writes to stderr through the
/// WASI imports a reactor does not carry, so an import-free module produces PHP's value without
/// its diagnostic rather than failing to link.
/// Renders `__rt_mixed_cast_string` for this module kind.
///
/// The two container arms carry PHP's only diagnostics in the cast family, and both write to
/// stderr — so a reactor module, which has no WASI to write through, gets the same ANSWERS
/// without them: "Array" for a container, and a bare trap for the object fatal.
fn mixed_cast_string(has_main: bool) -> String {
    let array_warning = if has_main {
        "(call $__rt_warn_array_to_string)"
    } else {
        ""
    };
    let object_fatal = if has_main {
        "(call $__rt_fail_object_to_string (i64.load (i32.wrap_i64 (local.get $lo))))\n      unreachable)) ;; elephc-trap:post-noreturn:object-to-string"
    } else {
        "unreachable)) ;; elephc-trap:non-public:reactor-object-to-string"
    };
    RT_MIXED_CAST_STRING_TEMPLATE
        .replace("{array_warning}", array_warning)
        .replace("{object_fatal}", object_fatal)
}

/// Builds `__rt_mixed_cast_int`: the `(int)` of a boxed value. `has_main` selects the float
/// conversion that warns under php 8.5 (which needs the command runtime) over the silent one.
fn mixed_cast_int(has_main: bool) -> String {
    let float_to_int = if has_main
        && matches!(
            crate::codegen_support::compile_php_version(),
            crate::web_prelude::PhpVersion::Php85
        ) {
        "$__rt_float_to_int_warn"
    } else {
        "$__rt_float_to_int"
    };
    let warning = if has_main {
        "(call $__rt_warn_object_to_int (i64.load (i32.wrap_i64 (local.get $lo))))  ;; class id lives at [obj+0]"
    } else {
        ";; reactor modules carry no WASI imports, so the diagnostic is omitted"
    };
    RT_MIXED_CAST_INT_TEMPLATE
        .replace("{FLOAT_TO_INT}", float_to_int)
        .replace("{OBJECT_INT_WARNING}", warning)
}

/// Renders `__rt_mixed_cast_float` for this module; see [`mixed_cast_int`] for why the object
/// arm's diagnostic is command-only.
fn mixed_cast_float(has_main: bool) -> String {
    let warning = if has_main {
        "(call $__rt_warn_object_to_float (i64.load (i32.wrap_i64 (local.get $lo))))  ;; class id lives at [obj+0]"
    } else {
        ";; reactor modules carry no WASI imports, so the diagnostic is omitted"
    };
    RT_MIXED_CAST_FLOAT_TEMPLATE.replace("{OBJECT_FLOAT_WARNING}", warning)
}

/// Borrows the cell (never frees).
const RT_MIXED_CAST_INT_TEMPLATE: &str = r#"(func $__rt_mixed_cast_int (param $ptr i32) (result i64) (local $tag i64) (local $lo i64) (local $hi i64) (local $cp i32) ;; cast a boxed Mixed cell to a PHP int, mirroring native tag dispatch
  (call $__rt_mixed_unbox (local.get $ptr))                             ;; unbox -> stack: tag, lo, hi
  (local.set $hi)                                                       ;; pop value high word
  (local.set $lo)                                                       ;; pop value low word
  (local.set $tag)                                                      ;; pop runtime tag
  (if (i64.eqz (local.get $tag))                                        ;; tag 0 = int
    (then (return (local.get $lo))))                                    ;; forward the stored integer payload
  (if (i64.eq (local.get $tag) (i64.const 1))                           ;; tag 1 = string
    (then (return (call $__rt_str_to_int (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi)) (global.get $__float_scratch))))) ;; PHP string -> int (saturate, INF/NaN -> 0)
  (if (i64.eq (local.get $tag) (i64.const 2))                           ;; tag 2 = float
    (then (return (call {FLOAT_TO_INT} (local.get $lo)))))              ;; PHP raw float cast: truncate, wrap finite OOR, non-finite -> 0
  (if (i64.eq (local.get $tag) (i64.const 3))                           ;; tag 3 = bool
    (then (return (local.get $lo))))                                    ;; already normalized 0/1
  (if (i32.or (i64.eq (local.get $tag) (i64.const 4)) (i64.eq (local.get $tag) (i64.const 5))) ;; tag 4/5 = array/hash
    (then
      (local.set $cp (i32.wrap_i64 (local.get $lo)))                    ;; container pointer
      (if (i32.eqz (local.get $cp))                                     ;; null container pointer?
        (then (return (i64.const 0)))                                   ;; null -> 0 (matches an empty container)
        (else (return (i64.extend_i32_u (i64.ne (i64.load (local.get $cp)) (i64.const 0)))))))) ;; PHP yields 1 for ANY non-empty array, never its length
  (if (i64.eq (local.get $tag) (i64.const 6))                           ;; tag 6 = object
    (then
      {OBJECT_INT_WARNING}
      (return (i64.const 1))))                                          ;; PHP still produces 1 after diagnosing
  (if (i64.eq (local.get $tag) (i64.const 9))                           ;; tag 9 = resource
    (then
      (if (i64.eq (i64.shr_u (local.get $lo) (i64.const 32)) (i64.const 1))   ;; a per-open record: its fd
        (then (return (i64.add (i64.extend_i32_u (i32.load (i32.wrap_i64 (local.get $lo)))) (i64.const 1)))))
      (return (i64.add (local.get $lo) (i64.const 1)))))                ;; 1-based display id (payload + 1)
  (i64.const 0))                                                        ;; null(8)/callable(10)/other -> 0
"#;

/// `__rt_mixed_cast_float`: casts a boxed Mixed cell to a PHP float, returning the raw
/// f64 bits (the wasm backend keeps floats as bits across helper boundaries). Unboxes via
/// `__rt_mixed_unbox`, then per tag: an int or bool widens to f64 via
/// `f64.convert_i64_s`; a string parses through `__rt_str_to_f64` (PHP string->float); a
/// float forwards its stored bits; arrays/hashes/objects/resources/null/other return 0.0.
/// Borrows the cell (never frees).
const RT_MIXED_CAST_FLOAT_TEMPLATE: &str = r#"(func $__rt_mixed_cast_float (param $ptr i32) (result i64) (local $tag i64) (local $lo i64) (local $hi i64) (local $cp i32) ;; cast a boxed Mixed cell to a PHP float, returning raw f64 bits
  (call $__rt_mixed_unbox (local.get $ptr))                             ;; unbox -> stack: tag, lo, hi
  (local.set $hi)                                                       ;; pop value high word
  (local.set $lo)                                                       ;; pop value low word
  (local.set $tag)                                                      ;; pop runtime tag
  (if (i64.eqz (local.get $tag))                                        ;; tag 0 = int
    (then (return (i64.reinterpret_f64 (f64.convert_i64_s (local.get $lo)))))) ;; widen int -> f64 bits
  (if (i64.eq (local.get $tag) (i64.const 1))                           ;; tag 1 = string
    (then                                                               ;; PHP string -> float via __rt_str_to_f64 (bits land at scratch+10240)
      (call $__rt_str_to_f64 (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi)) (i32.add (global.get $__float_scratch) (i32.const 10240)) (global.get $__float_scratch)) ;; parse the string into f64 bits
      (return (i64.load (i32.add (global.get $__float_scratch) (i32.const 10240)))))) ;; return the parsed f64 bits
  (if (i64.eq (local.get $tag) (i64.const 2))                           ;; tag 2 = float
    (then (return (local.get $lo))))                                    ;; forward the stored f64 bits
  (if (i64.eq (local.get $tag) (i64.const 3))                           ;; tag 3 = bool
    (then (return (i64.reinterpret_f64 (f64.convert_i64_s (local.get $lo)))))) ;; widen 0/1 -> f64 bits
  (if (i32.or (i64.eq (local.get $tag) (i64.const 4)) (i64.eq (local.get $tag) (i64.const 5))) ;; tag 4/5 = array/hash
    (then
      (local.set $cp (i32.wrap_i64 (local.get $lo)))                    ;; container pointer
      (if (i32.eqz (local.get $cp))                                     ;; null container pointer?
        (then (return (i64.const 0)))                                   ;; null -> 0.0
        (else (return (i64.reinterpret_f64 (f64.convert_i64_u (i64.extend_i32_u (i64.ne (i64.load (local.get $cp)) (i64.const 0)))))))))) ;; PHP yields 1.0 for ANY non-empty array
  (if (i64.eq (local.get $tag) (i64.const 6))                           ;; tag 6 = object
    (then
      {OBJECT_FLOAT_WARNING}
      (return (i64.reinterpret_f64 (f64.const 1)))))                    ;; PHP still produces 1.0 after diagnosing
  (if (i64.eq (local.get $tag) (i64.const 9))                           ;; tag 9 = resource
    (then
      (if (i64.eq (i64.shr_u (local.get $lo) (i64.const 32)) (i64.const 1))   ;; a per-open record: its fd
        (then (return (i64.reinterpret_f64 (f64.convert_i64_s
          (i64.add (i64.extend_i32_u (i32.load (i32.wrap_i64 (local.get $lo)))) (i64.const 1)))))))
      (return (i64.reinterpret_f64 (f64.convert_i64_s (i64.add (local.get $lo) (i64.const 1)))))))  ;; its 1-based display id, as in the int cast
  (i64.const 0))                                                        ;; null/other -> 0.0
"#;

/// `__rt_mixed_cast_string`: casts a boxed Mixed cell to a PHP string, returning
/// `(ptr, len)`. Always persists the result so the caller owns an independent copy: this
/// dodges the borrowed-aliasing bug where successive casts overwrite a shared scratch
/// buffer. Unboxes via `__rt_mixed_unbox`, then per tag: an int renders via `__rt_itoa`
/// into the float scratch and is persisted; a string is detached via `__rt_str_persist`;
/// a float renders via `__rt_ftoa` (PHP `%.14G` format) into scratch and is persisted; a
/// bool true renders "1" via `__rt_itoa` and is persisted, while false yields an empty
/// `(0, 0)`; arrays/hashes/objects/resources/null/other yield an empty `(0, 0)`. Borrows
/// the source cell (never frees it).
const RT_MIXED_CAST_STRING_TEMPLATE: &str = r#"(func $__rt_mixed_cast_string (param $ptr i32) (result i32) (result i32) (local $tag i64) (local $lo i64) (local $hi i64) (local $iptr i32) (local $ilen i32) (local $pptr i32) (local $plen i64) (local $slen i32) ;; cast a boxed Mixed cell to a PHP string (ptr,len), always persisting so callers own the result
  (call $__rt_mixed_unbox (local.get $ptr))                             ;; unbox -> stack: tag, lo, hi
  (local.set $hi)                                                       ;; pop value high word
  (local.set $lo)                                                       ;; pop value low word
  (local.set $tag)                                                      ;; pop runtime tag
  (if (i64.eqz (local.get $tag))                                        ;; tag 0 = int
    (then
      (call $__rt_itoa (local.get $lo) (global.get $__float_scratch))   ;; decimal text into scratch+0, returns (ptr,len)
      (local.set $ilen)                                                 ;; pop itoa length (result 1, on top)
      (local.set $iptr)                                                 ;; pop itoa pointer (result 0)
      (call $__rt_str_persist (local.get $iptr) (i64.extend_i32_u (local.get $ilen))) ;; own a persisted copy of the decimal text
      (local.set $plen)                                                 ;; pop persisted length (i64, result 1, on top)
      (local.set $pptr)                                                 ;; pop persisted pointer (result 0)
      (return (local.get $pptr) (i32.wrap_i64 (local.get $plen)))))     ;; return the owned decimal string
  (if (i64.eq (local.get $tag) (i64.const 1))                           ;; tag 1 = string
    (then
      (call $__rt_str_persist (i32.wrap_i64 (local.get $lo)) (local.get $hi)) ;; detach an owned copy from the source cell
      (local.set $plen)                                                 ;; pop persisted length (i64, result 1, on top)
      (local.set $pptr)                                                 ;; pop persisted pointer (result 0)
      (return (local.get $pptr) (i32.wrap_i64 (local.get $plen)))))     ;; return the owned string copy
  (if (i64.eq (local.get $tag) (i64.const 2))                           ;; tag 2 = float
    (then
      (call $__rt_ftoa (local.get $lo) (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80) (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 792) (i32.add (global.get $__float_scratch) (i32.const 4096))) ;; format into scratch+4096, returns (ptr,len)
      (local.set $ilen)                                                 ;; pop ftoa length (result 1, on top)
      (local.set $iptr)                                                 ;; pop ftoa pointer (result 0)
      (call $__rt_str_persist (local.get $iptr) (i64.extend_i32_u (local.get $ilen))) ;; own a persisted copy of the formatted text
      (local.set $plen)                                                 ;; pop persisted length (i64, result 1, on top)
      (local.set $pptr)                                                 ;; pop persisted pointer (result 0)
      (return (local.get $pptr) (i32.wrap_i64 (local.get $plen)))))     ;; return the owned float string
  (if (i64.eq (local.get $tag) (i64.const 3))                           ;; tag 3 = bool
    (then
      (if (i64.eqz (local.get $lo))                                     ;; false payload?
        (then (return (i32.const 0) (i32.const 0)))                     ;; false -> empty string (ptr=0, len=0)
        (else
          (call $__rt_itoa (i64.const 1) (global.get $__float_scratch)) ;; true -> "1" in scratch+0, returns (ptr,len)
          (local.set $ilen)                                             ;; pop itoa length (result 1, on top)
          (local.set $iptr)                                             ;; pop itoa pointer (result 0)
          (call $__rt_str_persist (local.get $iptr) (i64.extend_i32_u (local.get $ilen))) ;; own a persisted copy of "1"
          (local.set $plen)                                             ;; pop persisted length (i64, result 1, on top)
          (local.set $pptr)                                             ;; pop persisted pointer (result 0)
          (return (local.get $pptr) (i32.wrap_i64 (local.get $plen))))))) ;; return the owned "1"
  (if (i32.or (i64.eq (local.get $tag) (i64.const 4)) (i64.eq (local.get $tag) (i64.const 5)))  ;; tag 4/5 = array/hash
    (then
      ;; The ONE diagnostic in PHP's scalar-cast family: `(string)` of an array warns and
      ;; yields the literal text "Array". `(int)`, `(float)` and `(bool)` of one are silent.
      {array_warning}
      ;; "Array" is written into the float scratch rather than carried as a data segment, so
      ;; this helper stays independent of the module's static-data layout.
      (i32.store8 (global.get $__float_scratch) (i32.const 65))                        ;; 'A'
      (i32.store8 (i32.add (global.get $__float_scratch) (i32.const 1)) (i32.const 114)) ;; 'r'
      (i32.store8 (i32.add (global.get $__float_scratch) (i32.const 2)) (i32.const 114)) ;; 'r'
      (i32.store8 (i32.add (global.get $__float_scratch) (i32.const 3)) (i32.const 97))  ;; 'a'
      (i32.store8 (i32.add (global.get $__float_scratch) (i32.const 4)) (i32.const 121)) ;; 'y'
      (call $__rt_str_persist (global.get $__float_scratch) (i64.const 5))  ;; own a copy of "Array"
      (local.set $plen)                                                 ;; pop persisted length
      (local.set $pptr)                                                 ;; pop persisted pointer
      (return (local.get $pptr) (i32.wrap_i64 (local.get $plen)))))
  (if (i64.eq (local.get $tag) (i64.const 6))                           ;; tag 6 = object
    (then
      ;; An object whose class defines `__toString` CONVERTS through it — measured, php-src
      ;; prints `<em>` for a `(string)$tag` this arm used to raise on. The dispatch answers
      ;; whether the runtime class has one; only when it does not does PHP stop at a
      ;; diagnostic and TERMINATE, unlike the numeric casts which warn and answer 1.
      (call $__rt_object_to_string (i32.wrap_i64 (local.get $lo)))
      (local.set $slen)                                                 ;; pop the stringable flag
      (local.set $ilen)                                                 ;; pop the length
      (local.set $iptr)                                                 ;; pop the pointer
      (if (local.get $slen)
        (then
          (call $__rt_str_persist (local.get $iptr) (i64.extend_i32_u (local.get $ilen))) ;; own an independent copy
          (local.set $plen)
          (local.set $pptr)
          (call $__rt_decref_any (local.get $iptr))                     ;; a callee's Str return is OWNED (no-op on a literal)
          (return (local.get $pptr) (i32.wrap_i64 (local.get $plen)))))
      {object_fatal}
  (i32.const 0) (i32.const 0))                                          ;; resource/null/other -> empty string
"#;

/// `__rt_mixed_cast_string_ref`: like `__rt_mixed_cast_string` but returns a
/// BORROWED `(ptr, len)` -- the int/float text lands in the float scratch and the
/// string tag forwards the source cell's own pointer -- with NO `__rt_str_persist`.
/// This is the variant for callers that copy the bytes themselves (notably
/// `__rt_hash_set`, which persists inbound strings into its own storage): passing the
/// always-persisting `__rt_mixed_cast_string` result there would leak the cast's owned
/// copy, since `__rt_hash_set` re-persists. Unboxes via `__rt_mixed_unbox`, then per
/// tag: an int renders via `__rt_itoa` into scratch+0; a string forwards the cell's
/// `(ptr, len)`; a float renders via `__rt_ftoa` into scratch+4096; a bool true renders
/// "1" via `__rt_itoa`, false yields `(0, 0)`; arrays/hashes/objects/resources/null/
/// other yield `(0, 0)`. Borrows the source cell (never frees it). The borrowed scratch
/// pointer is only valid until the next float-scratch user, so callers must copy promptly.
const RT_MIXED_CAST_STRING_REF: &str = r#"(func $__rt_mixed_cast_string_ref (param $ptr i32) (result i32) (result i32) (local $tag i64) (local $lo i64) (local $hi i64) (local $iptr i32) (local $ilen i32) ;; cast a boxed Mixed cell to a BORROWED PHP string (ptr,len) in float-scratch (no persist; caller copies)
  (call $__rt_mixed_unbox (local.get $ptr))                             ;; unbox -> stack: tag, lo, hi
  (local.set $hi)                                                       ;; pop value high word
  (local.set $lo)                                                       ;; pop value low word
  (local.set $tag)                                                      ;; pop runtime tag
  (if (i64.eqz (local.get $tag))                                        ;; tag 0 = int
    (then
      (call $__rt_itoa (local.get $lo) (global.get $__float_scratch))   ;; decimal text into scratch+0, returns (ptr,len)
      (local.set $ilen)                                                 ;; pop itoa length (result 1, on top)
      (local.set $iptr)                                                 ;; pop itoa pointer (result 0)
      (return (local.get $iptr) (local.get $ilen))))                    ;; return borrowed scratch text
  (if (i64.eq (local.get $tag) (i64.const 1))                           ;; tag 1 = string
    (then
      (return (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi))))) ;; return borrowed source cell string (ptr,len)
  (if (i64.eq (local.get $tag) (i64.const 2))                           ;; tag 2 = float
    (then
      (call $__rt_ftoa (local.get $lo) (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80) (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 792) (i32.add (global.get $__float_scratch) (i32.const 4096))) ;; format into scratch+4096, returns (ptr,len)
      (local.set $ilen)                                                 ;; pop ftoa length (result 1, on top)
      (local.set $iptr)                                                 ;; pop ftoa pointer (result 0)
      (return (local.get $iptr) (local.get $ilen))))                    ;; return borrowed scratch text
  (if (i64.eq (local.get $tag) (i64.const 3))                           ;; tag 3 = bool
    (then
      (if (i64.eqz (local.get $lo))                                     ;; false payload?
        (then (return (i32.const 0) (i32.const 0)))                     ;; false -> empty (ptr=0, len=0)
        (else
          (call $__rt_itoa (i64.const 1) (global.get $__float_scratch)) ;; true -> "1" in scratch+0, returns (ptr,len)
          (local.set $ilen)                                             ;; pop itoa length (result 1, on top)
          (local.set $iptr)                                             ;; pop itoa pointer (result 0)
          (return (local.get $iptr) (local.get $ilen))))))              ;; return borrowed "1"
  (i32.const 0) (i32.const 0))                                          ;; array/hash/object/resource/null/other -> empty
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
    //!   `wasmer`. Runs skip with a SKIPPED line (fail under ELEPHC_TEST_REQUIRE_WASM_HOSTS) when `wasmer` is absent.

    use super::emit_mixed_runtime;
    use super::super::arrays::emit_array_runtime;
    use super::super::classes::{emit_class_metadata_stub, emit_class_runtime};
    use super::super::heap::emit_heap_runtime;
    use super::super::objects::{emit_destructor_dispatch_stub, emit_gc_desc_stub, emit_object_runtime};
    use super::super::refcount::emit_refcount_runtime;
    use super::super::closures::emit_closure_runtime;
    use super::super::wat::WatModule;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique temp directory path so concurrent wasmer runs never collide.
    fn unique_tmp_dir() -> std::path::PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("elephc_wasm_mixed_{}_{}", std::process::id(), n))
    }

    /// Returns whether the `wasmer` CLI is available; a missing host skips loudly or, under
/// `ELEPHC_TEST_REQUIRE_WASM_HOSTS`, fails (see `test_support`).
    fn wasmer_available() -> bool {
        crate::codegen_wasm::test_support::wasmer_available()
    }

    /// Builds a 3-page reactor module with the heap + refcount + array + mixed
    /// runtime and `driver`, validates it, and runs `export` under `wasmer`.
    fn run_driver(driver: &str, export: &str) -> Option<String> {
        let mut wm = WatModule::new();
        wm.set_memory(3, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 3 * 65536);
        emit_refcount_runtime(&mut wm);
        emit_closure_runtime(&mut wm);
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm, false, None);
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
        super::super::hashes::emit_hash_runtime(&mut wm, false);
        emit_object_runtime(&mut wm, false);
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

    /// Stores `s` as ASCII bytes at `base` (one `i32.store8` per byte), used to build
    /// string Mixed cells for the cast drivers below. Mirrors the float-suite helper.
    fn store_ascii(base: u32, s: &str) -> String {
        s.bytes()
            .enumerate()
            .map(|(i, b)| {
                format!(
                    "  (i32.store8 (i32.const {}) (i32.const {}))\n",
                    base + i as u32,
                    b
                )
            })
            .collect()
    }

    /// Rolling byte hash (`h = h*257 + b mod 1e15`) matching the float-suite helper, so
    /// `__rt_mixed_cast_string` results (a heap pointer + length) can be compared to the
    /// expected PHP string without parsing wasmer output as text.
    fn str_hash(s: &str) -> u64 {
        let mut h: u64 = 0;
        for b in s.bytes() {
            h = (h.wrapping_mul(257).wrapping_add(b as u64)) % 1_000_000_000_000_000;
        }
        h
    }

    /// Boxes the `(tag, lo, hi)` triple, calls `__rt_mixed_cast_int`, and returns the i64
    /// result (wasmer prints the signed decimal). For scalar payloads (int/bool/float).
    fn cast_int_scalar_driver(tag: i64, lo: i64, hi: i64) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
  (call $__rt_mixed_cast_int (call $__rt_mixed_from_value (i64.const {tag}) (i64.const {lo}) (i64.const {hi}))))"#,
        )
    }

    /// Stores `s` at 512, boxes it as a string cell, and casts to int. Exercises the
    /// `__rt_str_to_int` path of `__rt_mixed_cast_int` (tag 1).
    fn cast_int_string_driver(s: &str) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
{stores}  (call $__rt_mixed_cast_int (call $__rt_mixed_from_value (i64.const 1) (i64.extend_i32_u (i32.const 512)) (i64.const {len}))))"#,
            stores = store_ascii(512, s),
            len = s.len(),
        )
    }

    /// `__rt_mixed_cast_int` on int cells forwards the payload: 42 -> 42, -5 -> -5.
    /// Driver returns `cast(42)*10 + cast(-5)` = 420 + (-5) = 415.
    #[test]
    fn cast_int_forwards_int_payload() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i64) (local $b i64)
  (local.set $a (call $__rt_mixed_cast_int (call $__rt_mixed_from_value (i64.const 0) (i64.const 42) (i64.const 0))))
  (local.set $b (call $__rt_mixed_cast_int (call $__rt_mixed_from_value (i64.const 0) (i64.const -5) (i64.const 0))))
  (i64.add (i64.mul (local.get $a) (i64.const 10)) (local.get $b)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "415");
        }
    }

    /// `__rt_mixed_cast_int` on bool cells forwards the 0/1 payload: true -> 1, false -> 0.
    #[test]
    fn cast_int_bool_is_zero_or_one() {
        if let Some(o) = run_driver(&cast_int_scalar_driver(3, 1, 0), "t") {
            assert_eq!(o, "1");
        }
        if let Some(o) = run_driver(&cast_int_scalar_driver(3, 0, 0), "t") {
            assert_eq!(o, "0");
        }
    }

    /// `__rt_mixed_cast_int` on string cells follows PHP `(int)$str`: integer-form and
    /// finite float-form strings truncate/saturate, leading non-numeric -> 0, INF/NaN
    /// strings -> 0. Verified against `php -r`.
    #[test]
    fn cast_int_string_php_parity() {
        const CASES: &[(&str, &str)] = &[
            ("123", "123"),
            ("-45", "-45"),
            ("1e3", "1000"),
            ("1.9", "1"),
            ("-1.9", "-1"),
            ("1e18", "1000000000000000000"),
            ("1e20", "9223372036854775807"),
            ("1e400", "0"),
            ("-1e400", "0"),
            ("INF", "0"),
            ("NAN", "0"),
            ("abc", "0"),
            ("0x1A", "0"),
            ("  -45  ", "-45"),
        ];
        for (s, expected) in CASES {
            if let Some(o) = run_driver(&cast_int_string_driver(s), "t") {
                assert_eq!(o, *expected, "(int){s:?} mismatch");
            }
        }
    }

    /// `__rt_mixed_cast_int` on float cells truncates in-range values, maps +/-INF and
    /// NaN to 0, and wraps finite out-of-range values modulo 2^64 like PHP's
    /// `zend_dval_to_lval` path. This intentionally differs from numeric-string casts,
    /// whose finite out-of-range values saturate.
    #[test]
    fn cast_int_float_php_parity() {
        // (f64 value, expected i64 decimal)
        let cases: &[(f64, &str)] = &[
            (1.9, "1"),
            (-1.9, "-1"),
            (1e18, "1000000000000000000"),
            (1e20, "7766279631452241920"),
            (-1e20, "-7766279631452241920"),
            (9_223_372_036_854_774_784.0, "9223372036854774784"),
            (9_223_372_036_854_775_808.0, "-9223372036854775808"),
            (-9_223_372_036_854_775_808.0, "-9223372036854775808"),
            (-9_223_372_036_854_777_856.0, "9223372036854773760"),
            (f64::INFINITY, "0"),
            (f64::NEG_INFINITY, "0"),
            (f64::NAN, "0"),
            (0.0, "0"),
            (-0.0, "0"),
        ];
        for (v, expected) in cases {
            let bits = f64::to_bits(*v) as i64;
            if let Some(o) = run_driver(&cast_int_scalar_driver(2, bits, 0), "t") {
                assert_eq!(o, *expected, "(int)(float){v:?} (bits {bits}) mismatch");
            }
        }
    }

    /// Boxes `(tag, lo, hi)`, calls `__rt_mixed_cast_float`, and returns the raw f64 bits
    /// as an i64 (wasmer prints the signed decimal). For scalar payloads.
    fn cast_float_scalar_driver(tag: i64, lo: i64, hi: i64) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
  (call $__rt_mixed_cast_float (call $__rt_mixed_from_value (i64.const {tag}) (i64.const {lo}) (i64.const {hi}))))"#,
        )
    }

    /// Stores `s` at 512, boxes it as a string, and casts to float (returns f64 bits).
    /// Exercises the `__rt_str_to_f64` path of `__rt_mixed_cast_float` (tag 1).
    fn cast_float_string_driver(s: &str) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
{stores}  (call $__rt_mixed_cast_float (call $__rt_mixed_from_value (i64.const 1) (i64.extend_i32_u (i32.const 512)) (i64.const {len}))))"#,
            stores = store_ascii(512, s),
            len = s.len(),
        )
    }

    /// Helper: the f64 bits of `v` as a signed i64 decimal string (what wasmer prints).
    fn bits_str(v: f64) -> String {
        (f64::to_bits(v) as i64).to_string()
    }

    /// `__rt_mixed_cast_float` on int/bool cells widens to f64 (1000 -> 1000.0, true ->
    /// 1.0); a float cell forwards its bits. Compares the returned f64 bits.
    #[test]
    fn cast_float_widens_int_bool_forwards_float() {
        if let Some(o) = run_driver(&cast_float_scalar_driver(0, 1000, 0), "t") {
            assert_eq!(o, bits_str(1000.0));
        }
        if let Some(o) = run_driver(&cast_float_scalar_driver(3, 1, 0), "t") {
            assert_eq!(o, bits_str(1.0));
        }
        if let Some(o) = run_driver(&cast_float_scalar_driver(2, f64::to_bits(1.9) as i64, 0), "t") {
            assert_eq!(o, bits_str(1.9));
        }
    }

    /// `__rt_mixed_cast_float` on string cells follows PHP `(float)$str`: numeric strings
    /// parse, non-numeric -> 0.0. Verified against `php -r`.
    #[test]
    fn cast_float_string_php_parity() {
        const CASES: &[(&str, f64)] = &[
            ("1e3", 1000.0),
            ("1.9", 1.9),
            ("-45", -45.0),
            ("abc", 0.0),
            (".5", 0.5),
            ("0", 0.0),
        ];
        for (s, expected) in CASES {
            if let Some(o) = run_driver(&cast_float_string_driver(s), "t") {
                assert_eq!(o, bits_str(*expected), "(float){s:?} mismatch");
            }
        }
    }

    /// `__rt_mixed_cast_float` on array/hash/object/null cells returns 0.0 (bits 0).
    #[test]
    fn cast_float_non_numeric_is_zero() {
        // null cell (tag 8) -> 0.0
        if let Some(o) = run_driver(&cast_float_scalar_driver(8, 0, 0), "t") {
            assert_eq!(o, "0");
        }
    }

    /// Boxes `(tag, lo, hi)`, calls `__rt_mixed_cast_string`, and returns the rolling
    /// hash of the resulting (owned) string. For scalar payloads (int/bool/float/null).
    fn cast_string_scalar_driver(tag: i64, lo: i64, hi: i64) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
  (local $c i32) (local $sp i32) (local $len i32) (local $i i32) (local $h i64)
  (local.set $c (call $__rt_mixed_from_value (i64.const {tag}) (i64.const {lo}) (i64.const {hi})))
  (call $__rt_mixed_cast_string (local.get $c))
  (local.set $len)
  (local.set $sp)
  (local.set $h (i64.const 0))
  (local.set $i (i32.const 0))
  (block $e
    (loop $l
      (br_if $e (i32.ge_s (local.get $i) (local.get $len)))
      (local.set $h (i64.rem_u (i64.add (i64.mul (local.get $h) (i64.const 257)) (i64.load8_u (i32.add (local.get $sp) (local.get $i)))) (i64.const 1000000000000000)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
  (local.get $h))"#,
        )
    }

    /// Stores `s` at 512, boxes it as a string, and casts to string (returns the rolling
    /// hash of the persisted copy). Exercises the `__rt_str_persist` path (tag 1).
    fn cast_string_string_driver(s: &str) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
  (local $c i32) (local $sp i32) (local $len i32) (local $i i32) (local $h i64)
{stores}  (local.set $c (call $__rt_mixed_from_value (i64.const 1) (i64.extend_i32_u (i32.const 512)) (i64.const {len})))
  (call $__rt_mixed_cast_string (local.get $c))
  (local.set $len)
  (local.set $sp)
  (local.set $h (i64.const 0))
  (local.set $i (i32.const 0))
  (block $e
    (loop $l
      (br_if $e (i32.ge_s (local.get $i) (local.get $len)))
      (local.set $h (i64.rem_u (i64.add (i64.mul (local.get $h) (i64.const 257)) (i64.load8_u (i32.add (local.get $sp) (local.get $i)))) (i64.const 1000000000000000)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
  (local.get $h))"#,
            stores = store_ascii(512, s),
            len = s.len(),
        )
    }

    /// `__rt_mixed_cast_string` on int cells renders the PHP decimal text (42 -> "42").
    #[test]
    fn cast_string_int_renders_decimal() {
        if let Some(o) = run_driver(&cast_string_scalar_driver(0, 42, 0), "t") {
            assert_eq!(o, str_hash("42").to_string());
        }
        if let Some(o) = run_driver(&cast_string_scalar_driver(0, -5, 0), "t") {
            assert_eq!(o, str_hash("-5").to_string());
        }
        if let Some(o) = run_driver(&cast_string_scalar_driver(0, 0, 0), "t") {
            assert_eq!(o, str_hash("0").to_string());
        }
    }

    /// `__rt_mixed_cast_string` on bool cells: true -> "1", false -> "" (empty).
    #[test]
    fn cast_string_bool_is_one_or_empty() {
        if let Some(o) = run_driver(&cast_string_scalar_driver(3, 1, 0), "t") {
            assert_eq!(o, str_hash("1").to_string());
        }
        if let Some(o) = run_driver(&cast_string_scalar_driver(3, 0, 0), "t") {
            assert_eq!(o, str_hash("").to_string());
        }
    }

    /// `__rt_mixed_cast_string` on string cells detaches an owned copy preserving bytes.
    #[test]
    fn cast_string_string_detaches_copy() {
        if let Some(o) = run_driver(&cast_string_string_driver("hello"), "t") {
            assert_eq!(o, str_hash("hello").to_string());
        }
        if let Some(o) = run_driver(&cast_string_string_driver(""), "t") {
            assert_eq!(o, str_hash("").to_string());
        }
    }

    /// `__rt_mixed_cast_string` on float cells renders the PHP `%.14G` text. Verified
    /// against `php -r` (`(string)(float)1.9 = "1.9"`, `(string)INF = "INF"`).
    #[test]
    fn cast_string_float_renders_php_format() {
        const CASES: &[(f64, &str)] = &[
            (1.9, "1.9"),
            (100.0, "100"),
            (0.0, "0"),
            (-1.5, "-1.5"),
            (f64::INFINITY, "INF"),
            (f64::NAN, "NAN"),
        ];
        for (v, expected) in CASES {
            let bits = f64::to_bits(*v) as i64;
            if let Some(o) = run_driver(&cast_string_scalar_driver(2, bits, 0), "t") {
                assert_eq!(o, str_hash(expected).to_string(), "(string)(float){v:?} mismatch");
            }
        }
    }

    /// `__rt_mixed_cast_string` on null/object/array cells yields an empty string.
    #[test]
    fn cast_string_non_scalar_is_empty() {
        // null cell (tag 8) -> "", which is what PHP prints for null.
        if let Some(o) = run_driver(&cast_string_scalar_driver(8, 0, 0), "t") {
            assert_eq!(o, str_hash("").to_string());
        }
        // A resource cell (tag 9) has no lowered text either and stays empty.
        if let Some(o) = run_driver(&cast_string_scalar_driver(9, 0, 0), "t") {
            assert_eq!(o, str_hash("").to_string());
        }
        // An OBJECT cell no longer answers "": `(string)` of an object without `__toString`
        // is PHP's fatal, so the harness (a reactor, with no WASI to report through) traps
        // rather than producing a value. Asserting the old empty string here would be
        // asserting a wrong ANSWER.
    }
}

/// `__rt_mixed_iter_start`: opens a `foreach` over a BOXED value.
///
/// Returns `(source, cursor, kind)` where `kind` is 0 for an indexed array, 1 for a hash, and 2
/// for a value PHP will not iterate. Measured on php-src 8.5.6, that last case WARNS naming the
/// type and runs the body zero times rather than raising, which is why kind 2 exists at all
/// instead of a fatal: the loop still has to be entered and left cleanly.
///
/// The cursor seeds match the two concrete paths exactly — `-1` for an indexed array, which
/// pre-increments to 0, and `-2` for a hash, the before-first sentinel `__rt_hash_iter_next`
/// expects — so a dynamic iterator and a statically typed one advance identically.
const RT_MIXED_ITER_START: &str = r#"(func $__rt_mixed_iter_start (param $cell i32) (result i32) (result i64) (result i32)
  (local $tag i64) (local $lo i64) (local $hi i64)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 4))                     ;; tag 4 = indexed array
    (then (return (i32.wrap_i64 (local.get $lo)) (i64.const -1) (i32.const 0))))
  (if (i64.eq (local.get $tag) (i64.const 5))                     ;; tag 5 = hash
    (then (return (i32.wrap_i64 (local.get $lo)) (i64.const -2) (i32.const 1))))
;;GEN_START
  (call $__rt_warn_foreach_non_iterable (local.get $tag) (local.get $lo))
  (i32.const 0) (i64.const 0) (i32.const 2))                      ;; nothing to walk
"#;

/// `__rt_iterable_iter_start`: opens a `foreach` over an `iterable`-typed CONTAINER POINTER.
///
/// The pointer twin of `__rt_mixed_iter_start`, answering the same `(source, cursor, kind)`
/// triple so both feed the identical advance and read helpers. An `iterable` parameter is a
/// raw pointer rather than a boxed cell, so the storage question is settled by the heap
/// header's kind byte — 2 for an indexed array, 3 for a hash — instead of a cell tag.
///
/// A null pointer walks zero times rather than trapping: `iterable` is a declared type the
/// checker enforces, so an unset slot reaching here is a defensive case, not a php one.
const RT_ITERABLE_ITER_START: &str = r#"(func $__rt_iterable_iter_start (param $p i32) (result i32) (result i64) (result i32)
  (local $kind i64)
  (if (i32.eqz (local.get $p))
    (then (return (i32.const 0) (i64.const 0) (i32.const 2))))
  (local.set $kind (i64.and (i64.load (i32.sub (local.get $p) (i32.const 8))) (i64.const 255)))
  (if (i64.eq (local.get $kind) (i64.const 2))                    ;; heap kind 2 = indexed array
    (then (return (local.get $p) (i64.const -1) (i32.const 0))))
  (if (i64.eq (local.get $kind) (i64.const 3))                    ;; heap kind 3 = hash
    (then (return (local.get $p) (i64.const -2) (i32.const 1))))
  (local.get $p) (i64.const 0) (i32.const 2))                     ;; nothing to walk
"#;

/// `__rt_mixed_iter_next`: advances a dynamic `foreach` cursor, answering whether the body runs.
///
/// Each arm is the one the statically typed path emits inline: an indexed array pre-increments
/// and tests against the length at `+0`, a hash delegates to `__rt_hash_iter_next`, and kind 2
/// never runs the body at all.
const RT_MIXED_ITER_NEXT: &str = r#"(func $__rt_mixed_iter_next (param $src i32) (param $cursor i64) (param $kind i32) (result i64) (result i64)
  (local $next i64) (local $more i64)
;;GEN_NEXT
  (if (i32.eq (local.get $kind) (i32.const 2))
    (then (return (i64.const 0) (i64.const 0))))                  ;; not iterable: zero iterations
  (if (i32.eq (local.get $kind) (i32.const 1))
    (then
      (call $__rt_hash_iter_next (local.get $src) (local.get $cursor))
      (local.set $more)
      (local.set $next)
      (return (local.get $next) (local.get $more))))
  (local.set $next (i64.add (local.get $cursor) (i64.const 1)))
  (local.get $next)
  (i64.extend_i32_u (i64.lt_s (local.get $next) (i64.load (local.get $src)))))
"#;

/// `__rt_mixed_iter_key`: the current key of a dynamic `foreach`, boxed.
///
/// An indexed array's key is its cursor. A hash entry stores its key as `(key_lo, key_hi)` with
/// `key_hi = -1` marking an INTEGER key — the same sentinel `__rt_hash_get` reads — so the tag
/// is chosen from that rather than from the container's storage.
const RT_MIXED_ITER_KEY: &str = r#"(func $__rt_mixed_iter_key (param $src i32) (param $cursor i64) (param $kind i32) (result i32)
  (local $entry i32) (local $klo i64) (local $khi i64) (local $cell i32)
;;GEN_KEY
  (if (i32.eq (local.get $kind) (i32.const 1))
    (then
      (local.set $entry (i32.add (i32.add (local.get $src) (i32.const 40))
                                 (i32.wrap_i64 (i64.mul (local.get $cursor) (i64.const 72)))))
      (local.set $klo (i64.load (i32.add (local.get $entry) (i32.const 8))))
      (local.set $khi (i64.load (i32.add (local.get $entry) (i32.const 16))))
      (if (i64.eq (local.get $khi) (i64.const -1))
        (then (return (call $__rt_mixed_from_value (i64.const 0) (local.get $klo) (i64.const 0)))))
      (return (call $__rt_mixed_from_value (i64.const 1) (local.get $klo) (local.get $khi)))))
  (call $__rt_mixed_from_value (i64.const 0) (local.get $cursor) (i64.const 0)))
"#;

/// `__rt_mixed_iter_value`: the current value of a dynamic `foreach`, boxed and OWNED.
///
/// An indexed element goes through `__rt_array_elem_to_mixed`, which already maps every storage
/// `value_type` to its runtime tag and hands back an owned cell. A hash entry stores its value as
/// `(tag, lo, hi)` outright, so boxing it is one call. Both answers are owned because
/// `iter_current_value` is `own=owned` in the EIR and the release is scheduled for it.
const RT_MIXED_ITER_VALUE: &str = r#"(func $__rt_mixed_iter_value (param $src i32) (param $cursor i64) (param $kind i32) (result i32)
  (local $entry i32) (local $cell i32)
;;GEN_VALUE
  (if (i32.eq (local.get $kind) (i32.const 1))
    (then
      (local.set $entry (i32.add (i32.add (local.get $src) (i32.const 40))
                                 (i32.wrap_i64 (i64.mul (local.get $cursor) (i64.const 72)))))
      (return (call $__rt_mixed_from_value
        (i64.load (i32.add (local.get $entry) (i32.const 40)))
        (i64.load (i32.add (local.get $entry) (i32.const 24)))
        (i64.load (i32.add (local.get $entry) (i32.const 32)))))))
  (call $__rt_array_elem_to_mixed (local.get $src) (local.get $cursor)))
"#;
