//! Purpose:
//! Emits the hand-authored WebAssembly (WAT) indexed-array runtime for the
//! wasm32-wasi backend: allocation, capacity growth, integer/string append,
//! bounded read, element assignment (`$a[i]=v`) with copy-on-write, the indexed
//! `+` union operator (`__rt_array_union`), deep free, and the array branch of
//! the refcount dispatcher. Built on top of the linear-memory allocator (`heap`)
//! and refcount layer (`refcount`).
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
//!   `value_type` 7 marks a 16-byte-slot Mixed-cell array (P7a1 closure-call arg
//!   buffer): each slot holds a kind-5 cell pointer at slot+0, and `free_deep`
//!   releases every cell through the kind-dispatched `__rt_decref_any`.

use super::wat::WatModule;

/// Adds the indexed-array runtime routines to `wm`. Emitted after the heap and
/// refcount runtimes, whose `__rt_heap_alloc` / `__rt_heap_free` / `__rt_decref_any`
/// and heap globals these routines reference.
pub(super) fn emit_array_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_ARRAY_NEW);
    wm.add_raw_func(RT_ARRAY_GROW);
    wm.add_raw_func(RT_ARRAY_PUSH_INT);
    wm.add_raw_func(RT_ARRAY_PUSH_STR);
    wm.add_raw_func(RT_ARRAY_PUSH_FLOAT);
    wm.add_raw_func(RT_ARRAY_PUSH_PTR);
    wm.add_raw_func(RT_ARRAY_GET_OBJECT);
    wm.add_raw_func(RT_ARRAY_GET_FLOAT);
    wm.add_raw_func(RT_ARRAY_PUSH_MIXED);
    wm.add_raw_func(RT_ARRAY_WIDEN_TO_MIXED);
    wm.add_raw_func(RT_ARRAY_SLICE);
    wm.add_raw_func(RT_SORT_SCALAR);
    wm.add_raw_func(RT_RANGE_INT);
    wm.add_raw_func(RT_ARRAY_FIND_INT);
    wm.add_raw_func(RT_ARRAY_FIND_FLOAT);
    wm.add_raw_func(RT_ARRAY_GET_INT);
    wm.add_raw_func(RT_ARRAY_GET_TAGGED_INT);
    wm.add_raw_func(RT_ARRAY_GET_MIXED_BOOL);
    wm.add_raw_func(RT_ARRAY_GET_STR);
    wm.add_raw_func(RT_ARRAY_GET_MIXED_STR);
    wm.add_raw_func(RT_ARRAY_ENSURE_UNIQUE);
    wm.add_raw_func(RT_ARRAY_CLONE_SHALLOW);
    wm.add_raw_func(RT_ARRAY_PREFLIGHT_SET);
    wm.add_raw_func(RT_ARRAY_SET_INT);
    wm.add_raw_func(RT_ARRAY_SET_STR);
    wm.add_raw_func(RT_ARRAY_FREE_DEEP);
    wm.add_raw_func(RT_DECREF_ARRAY);
    wm.add_raw_func(RT_ARRAY_APPEND_FROM);
    wm.add_raw_func(RT_ARRAY_UNION);
    wm.add_raw_func(RT_ARRAY_MERGE);
    wm.add_raw_func(RT_ARRAY_INDEX_KEYS);
    wm.add_raw_func(RT_ARRAY_REVERSE_INT);
    wm.add_raw_func(RT_ARRAY_SUM_INT);
    wm.add_raw_func(RT_ARRAY_PRODUCT_INT);
    wm.add_raw_func(RT_ARRAY_FILL_INT);
}

/// `__rt_array_new`: allocates an indexed array with `capacity` slots of
/// `elem_size` bytes, a zeroed length, and the indexed-array kind stamped.
const RT_ARRAY_NEW: &str = r#"(func $__rt_array_new (param $capacity i64) (param $elem_size i64) (result i32)
  (local $bytes i32)
  (local $arr i32)
  (local $kind i64)
  (local.set $bytes
    (call $__rt_checked_layout
      (local.get $capacity)
      (local.get $elem_size)
      (i64.const 24)))                                      ;; checked 24B header + capacity*elem_size slots
  (local.set $arr (call $__rt_heap_alloc (local.get $bytes)))  ;; block: refcount=1, kind=0
  (local.set $kind (i64.const 2))                              ;; low byte = indexed-array kind
  (if (i64.eq (local.get $elem_size) (i64.const 16))
    (then (local.set $kind (i64.or (local.get $kind) (i64.const 256)))))  ;; 16B slots default to value_type 1 (string)
  (local.set $kind (i64.or (local.get $kind) (i64.const 32768)))  ;; COW flag (bit 15)
  (i64.store (i32.sub (local.get $arr) (i32.const 8)) (local.get $kind))  ;; stamp kind word at A-8
  (i64.store (local.get $arr) (i64.const 0))                   ;; length = 0
  (i64.store (i32.add (local.get $arr) (i32.const 8)) (local.get $capacity))    ;; capacity
  (i64.store (i32.add (local.get $arr) (i32.const 16)) (local.get $elem_size))  ;; elem_size
  (local.get $arr))                                                              ;; return the new array pointer
"#;

/// `__rt_array_index_keys`: builds `[0, 1, ..., n-1]` for an indexed array of length `n`.
///
/// This is `array_keys()` over a list: its keys ARE the positions, so the result depends only on
/// the source length and never reads a payload slot. The result is a fresh owned array of raw
/// i64 slots, which is what `array<int>` uses.
const RT_ARRAY_INDEX_KEYS: &str = r#"(func $__rt_array_index_keys (param $array i32) (result i32)
  (local $len i64)
  (local $new i32)
  (local $i i64)
  (local.set $len (i64.load (local.get $array)))                 ;; length @ A+0
  (local.set $new (call $__rt_array_new (local.get $len) (i64.const 8)))  ;; exact capacity, raw i64 slots
  (i64.store (local.get $new) (local.get $len))                  ;; the result holds one key per element
  (local.set $i (i64.const 0))                                   ;; i = 0
  (block $end (loop $fill
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))      ;; stop after the last position
    (i64.store
      (i32.add (i32.add (local.get $new) (i32.const 24))
               (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8))))
      (local.get $i))                                            ;; slot i holds the key i
    (local.set $i (i64.add (local.get $i) (i64.const 1)))        ;; i++
    (br $fill)))
  (local.get $new))                                              ;; owned result
"#;

/// `__rt_array_reverse_int`: builds the reverse of an indexed array of raw i64 slots.
///
/// `array_reverse()` without `preserve_keys` re-indexes from zero, which over a list is exactly
/// the reversed sequence. The result is a fresh owned array; the source is only read.
const RT_ARRAY_REVERSE_INT: &str = r#"(func $__rt_array_reverse_int (param $array i32) (result i32)
  (local $len i64)
  (local $new i32)
  (local $i i64)
  (local.set $len (i64.load (local.get $array)))                 ;; length @ A+0
  (local.set $new (call $__rt_array_new (local.get $len) (i64.const 8)))  ;; exact capacity, raw i64 slots
  (i64.store (local.get $new) (local.get $len))                  ;; same element count
  (local.set $i (i64.const 0))                                   ;; i = 0
  (block $end (loop $copy
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))      ;; every element placed
    (i64.store
      (i32.add (i32.add (local.get $new) (i32.const 24))
               (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8))))
      (i64.load
        (i32.add (i32.add (local.get $array) (i32.const 24))
                 (i32.wrap_i64 (i64.mul (i64.sub (i64.sub (local.get $len) (i64.const 1)) (local.get $i))
                                        (i64.const 8))))))       ;; destination i takes source len-1-i
    (local.set $i (i64.add (local.get $i) (i64.const 1)))        ;; i++
    (br $copy)))
  (local.get $new))                                              ;; owned result
"#;

/// `__rt_array_sum_int`: adds every raw i64 slot of an indexed array, starting from zero.
///
/// The empty array sums to 0, which is what PHP answers. Addition WRAPS on overflow; see
/// `builtins::lower_array_fold` for why that divergence is not representable here.
const RT_ARRAY_SUM_INT: &str = r#"(func $__rt_array_sum_int (param $array i32) (result i64)
  (local $len i64)
  (local $i i64)
  (local $acc i64)
  (local.set $len (i64.load (local.get $array)))                 ;; length @ A+0
  (local.set $acc (i64.const 0))                                 ;; PHP sums an empty array to 0
  (local.set $i (i64.const 0))                                   ;; i = 0
  (block $end (loop $add
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))      ;; every element added
    (local.set $acc (i64.add (local.get $acc)
      (i64.load (i32.add (i32.add (local.get $array) (i32.const 24))
                         (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8)))))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))        ;; i++
    (br $add)))
  (local.get $acc))
"#;

/// `__rt_array_product_int`: multiplies every raw i64 slot of an indexed array.
///
/// The empty array's product is 1, which is what PHP answers. Multiplication WRAPS on overflow;
/// see `builtins::lower_array_fold`.
const RT_ARRAY_PRODUCT_INT: &str = r#"(func $__rt_array_product_int (param $array i32) (result i64)
  (local $len i64)
  (local $i i64)
  (local $acc i64)
  (local.set $len (i64.load (local.get $array)))                 ;; length @ A+0
  (local.set $acc (i64.const 1))                                 ;; PHP's empty product is 1
  (local.set $i (i64.const 0))                                   ;; i = 0
  (block $end (loop $mul
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))      ;; every element multiplied
    (local.set $acc (i64.mul (local.get $acc)
      (i64.load (i32.add (i32.add (local.get $array) (i32.const 24))
                         (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8)))))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))        ;; i++
    (br $mul)))
  (local.get $acc))
"#;

/// `__rt_array_fill_int`: builds `[value, value, ...]` with `count` raw i64 slots.
///
/// Serves `array_fill(0, $count, $value)` only: a non-zero start index produces the keys
/// `start..start+count-1`, which is not a list and therefore not this representation. A count of
/// zero yields the empty array, matching PHP.
const RT_ARRAY_FILL_INT: &str = r#"(func $__rt_array_fill_int (param $count i64) (param $value i64) (result i32)
  (local $new i32)
  (local $i i64)
  (local.set $new (call $__rt_array_new (local.get $count) (i64.const 8)))  ;; exact capacity, raw i64 slots
  (i64.store (local.get $new) (local.get $count))                 ;; every slot is live
  (local.set $i (i64.const 0))                                    ;; i = 0
  (block $end (loop $fill
    (br_if $end (i64.ge_s (local.get $i) (local.get $count)))     ;; all slots written
    (i64.store
      (i32.add (i32.add (local.get $new) (i32.const 24))
               (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8))))
      (local.get $value))                                         ;; the same value in every slot
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $fill)))
  (local.get $new))                                               ;; owned result
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
  (drop (call $__rt_checked_layout (local.get $cap) (local.get $esz) (i64.const 24)))  ;; validate the source layout before reading/copying slots
  (if (i64.gt_u (local.get $len) (local.get $cap))
    (then (call $__rt_oom) unreachable))                     ;; elephc-trap:deterministic-oom:array-grow-malformed-length malformed length cannot exceed capacity
  (local.set $newcap (i64.add (local.get $cap) (local.get $cap)))  ;; newcap = cap * 2 (cap is bounded above)
  (if (i64.lt_s (local.get $newcap) (i64.const 8))
    (then (local.set $newcap (i64.const 8))))                ;; minimum capacity 8
  (drop (call $__rt_checked_layout (local.get $newcap) (local.get $esz) (i64.const 24)))  ;; reject an unrepresentable doubled layout before allocation
  (local.set $new (call $__rt_array_new (local.get $newcap) (local.get $esz)))  ;; fresh larger array
  (i64.store (i32.sub (local.get $new) (i32.const 8))
             (i64.and (i64.load (i32.sub (local.get $array) (i32.const 8))) (i64.const 65535)))  ;; preserve old value_type/COW (low 16 bits)
  (i64.store (local.get $new) (local.get $len))              ;; copy length (capacity/elem_size set by array_new)
  (local.set $nbytes (call $__rt_checked_layout (local.get $len) (local.get $esz) (i64.const 0)))  ;; checked live payload bytes
  (local.set $i (i32.const 0))                                                   ;; i = 0 (copy cursor)
  (block $end (loop $copy
    (br_if $end (i32.ge_u (local.get $i) (local.get $nbytes)))                   ;; stop when all bytes copied
    (i32.store8 (i32.add (i32.add (local.get $new) (i32.const 24)) (local.get $i))
                (i32.load8_u (i32.add (i32.add (local.get $array) (i32.const 24)) (local.get $i))))  ;; copy one byte
    (local.set $i (i32.add (local.get $i) (i32.const 1)))                        ;; i++
    (br $copy)))                                                                 ;; next byte
  (call $__rt_heap_free (local.get $array))                  ;; free old struct shallowly (children were moved)
  (local.get $new))                                                              ;; return the grown array pointer
"#;

/// `__rt_array_push_int`: appends an integer, shaping an empty array to 8-byte
/// scalar slots and growing capacity when full. Returns the (possibly new) array.
const RT_ARRAY_PUSH_INT: &str = r#"(func $__rt_array_push_int (param $array i32) (param $value i64) (result i32)
  (local $len i64)
  (local $cap i64)
  (local $slot i32)
  ;; Copy-on-write split, exactly as `__rt_array_set_*` does: a shared array must be
  ;; cloned before it is appended to, or the append is visible through every other
  ;; reference — and `__rt_array_grow` frees the old block under them.
  (local.set $array (call $__rt_array_ensure_unique (local.get $array)))
  (if (i64.eqz (i64.load (local.get $array)))               ;; empty -> shape as a scalar array
    (then
      (i64.store (i32.add (local.get $array) (i32.const 16)) (i64.const 8))  ;; elem_size = 8
      (i64.store (i32.sub (local.get $array) (i32.const 8))
                 (i64.and (i64.load (i32.sub (local.get $array) (i32.const 8))) (i64.const -32513)))))  ;; clear value_type bits 8-14 (~0x7f00)
  (local.set $len (i64.load (local.get $array)))            ;; length
  (local.set $cap (i64.load (i32.add (local.get $array) (i32.const 8))))  ;; capacity
  (if (i64.ge_s (local.get $len) (local.get $cap))          ;; full -> grow
    (then (local.set $array (call $__rt_array_grow (local.get $array)))))        ;; update array pointer after grow
  (local.set $len (i64.load (local.get $array)))            ;; reload length (grow preserves it)
  (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $len) (i64.const 8)))))  ;; slot = A+24+len*8
  (i64.store (local.get $slot) (local.get $value))          ;; write element
  (i64.store (local.get $array) (i64.add (local.get $len) (i64.const 1)))  ;; length++
  (local.get $array))                                                            ;; return the (possibly new) array
"#;

/// `__rt_array_push_float`: appends a float, shaping an empty array to 8-byte slots stamped
/// `value_type` 2, and growing capacity when full. Returns the (possibly new) array.
///
/// A float shares the int slot width — the payload is the f64's bits — so this mirrors
/// `__rt_array_push_int` byte for byte apart from the stamp. The stamp is what tells a runtime
/// tag observer (and the native layout this is byte-identical to) that the payload is a float
/// rather than an integer; `__rt_array_free_deep` treats both as scalars and frees directly.
const RT_ARRAY_PUSH_FLOAT: &str = r#"(func $__rt_array_push_float (param $array i32) (param $value f64) (result i32)
  (local $len i64)
  (local $cap i64)
  (local $slot i32)
  ;; Copy-on-write split, exactly as `__rt_array_set_*` does: a shared array must be
  ;; cloned before it is appended to, or the append is visible through every other
  ;; reference — and `__rt_array_grow` frees the old block under them.
  (local.set $array (call $__rt_array_ensure_unique (local.get $array)))
  (if (i64.eqz (i64.load (local.get $array)))               ;; empty -> shape as a float array
    (then
      (i64.store (i32.add (local.get $array) (i32.const 16)) (i64.const 8))  ;; elem_size = 8
      (i64.store (i32.sub (local.get $array) (i32.const 8))
                 (i64.or (i64.and (i64.load (i32.sub (local.get $array) (i32.const 8))) (i64.const -32513)) (i64.const 512)))))  ;; value_type = 2 (float; 2 << 8 = 512)
  (local.set $len (i64.load (local.get $array)))            ;; length
  (local.set $cap (i64.load (i32.add (local.get $array) (i32.const 8))))  ;; capacity
  (if (i64.ge_s (local.get $len) (local.get $cap))          ;; full -> grow
    (then (local.set $array (call $__rt_array_grow (local.get $array)))))        ;; update array pointer after grow
  (local.set $len (i64.load (local.get $array)))            ;; reload length (grow preserves it)
  (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $len) (i64.const 8)))))  ;; slot = A+24+len*8
  (f64.store (local.get $slot) (local.get $value))          ;; write element
  (i64.store (local.get $array) (i64.add (local.get $len) (i64.const 1)))  ;; length++
  (local.get $array))                                                            ;; return the (possibly new) array
"#;

/// `__rt_array_push_ptr`: appends a refcounted container pointer, shaping an empty array to
/// 8-byte slots stamped with the caller's `value_type`, and growing capacity when full.
///
/// `vt` is 4 for an object element and 5 for a nested indexed array. (Note this target's
/// `value_type` 4 is OBJECT, where the native's is array-of-arrays; the tag is internal to
/// each backend, and both agree that 4..7 mean "the slot holds a refcounted pointer".)
///
/// The array owns a SHARE of the child — the caller increfs before handing it over, matching
/// what the EIR emits: `array_push` followed by a `release` of the operand.
/// `__rt_array_free_deep` and `__rt_array_clone_shallow` already treat `value_type` 4..7 as
/// refcounted children, so the stamp is what makes the array's own release and its
/// copy-on-write split reach them rather than dropping or aliasing them.
const RT_ARRAY_PUSH_PTR: &str = r#"(func $__rt_array_push_ptr (param $array i32) (param $obj i32) (param $vt i64) (result i32)
  (local $len i64)
  (local $cap i64)
  (local $slot i32)
  ;; Copy-on-write split, exactly as `__rt_array_set_*` does: a shared array must be
  ;; cloned before it is appended to, or the append is visible through every other
  ;; reference — and `__rt_array_grow` frees the old block under them.
  (local.set $array (call $__rt_array_ensure_unique (local.get $array)))
  (if (i64.eqz (i64.load (local.get $array)))               ;; empty -> shape as an object array
    (then
      (i64.store (i32.add (local.get $array) (i32.const 16)) (i64.const 8))  ;; elem_size = 8
      (i64.store (i32.sub (local.get $array) (i32.const 8))
                 (i64.or (i64.and (i64.load (i32.sub (local.get $array) (i32.const 8))) (i64.const -32513)) (i64.shl (local.get $vt) (i64.const 8))))))  ;; value_type = vt (object 4, nested array 5)
  (local.set $len (i64.load (local.get $array)))            ;; length
  (local.set $cap (i64.load (i32.add (local.get $array) (i32.const 8))))  ;; capacity
  (if (i64.ge_s (local.get $len) (local.get $cap))          ;; full -> grow
    (then (local.set $array (call $__rt_array_grow (local.get $array)))))
  (local.set $len (i64.load (local.get $array)))            ;; reload length (grow preserves it)
  (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $len) (i64.const 8)))))  ;; slot = A+24+len*8
  (i64.store (local.get $slot) (i64.extend_i32_u (local.get $obj)))  ;; object pointer (zero-extended)
  (i64.store (local.get $array) (i64.add (local.get $len) (i64.const 1)))  ;; length++
  (local.get $array))                                       ;; return the (possibly new) array
"#;

/// `__rt_array_get_object`: reads the container pointer at `index`, BORROWED.
///
/// Serves object elements (`value_type` 4) and nested indexed arrays (5) alike: both store
/// the child pointer at slot+0 with an 8-byte stride, so the read does not consult the tag.
///
/// A miss answers 0, which every release path treats as a no-op — unlike the int accessor's null
/// sentinel, which as a pointer would be garbage. The caller decides ownership: `foreach` binds an
/// OWNED value (`iter_current_value` is `own=owned` and the EIR releases it), so it increfs; a
/// boxing caller hands the pointer to `__rt_mixed_from_value`, which increfs refcounted children
/// itself.
const RT_ARRAY_GET_OBJECT: &str = r#"(func $__rt_array_get_object (param $array i32) (param $index i64) (result i32)
  (local $len i64)
  (if (i32.eqz (local.get $array))
    (then (return (i32.const 0))))
  (if (i64.lt_s (local.get $index) (i64.const 0))
    (then (return (i32.const 0))))
  (local.set $len (i64.load (local.get $array)))
  (if (i64.ge_s (local.get $index) (local.get $len))
    (then (return (i32.const 0))))
  (i32.wrap_i64 (i64.load (i32.add (i32.add (local.get $array) (i32.const 24))
                                   (i32.wrap_i64 (i64.mul (local.get $index) (i64.const 8)))))))
"#;

/// `__rt_array_get_float`: reads the f64 element at `index`.
///
/// A miss answers NaN rather than the int path's null sentinel: every bit pattern is a valid
/// float, so there is no spare value to reserve, and the callers that need to tell "missing" from
/// "present" read through the boxed accessor instead.
const RT_ARRAY_GET_FLOAT: &str = r#"(func $__rt_array_get_float (param $array i32) (param $index i64) (result f64)
  (local $len i64)
  (if (i64.lt_s (local.get $index) (i64.const 0))           ;; negative index -> NaN
    (then (return (f64.const nan))))
  (local.set $len (i64.load (local.get $array)))            ;; length
  (if (i64.ge_s (local.get $index) (local.get $len))        ;; out of bounds -> NaN
    (then (return (f64.const nan))))
  (f64.load (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $index) (i64.const 8))))))  ;; slot[index]
"#;

/// `__rt_array_get_int`: reads the i64 element at `index`, returning the PHP null
/// sentinel (0x7fff_ffff_ffff_fffe) for a negative or out-of-bounds index. Used
/// for scalar (8-byte slot) arrays.
const RT_ARRAY_GET_INT: &str = r#"(func $__rt_array_get_int (param $array i32) (param $index i64) (result i64)
  (local $len i64)
  (if (i64.lt_s (local.get $index) (i64.const 0))           ;; negative index -> null
    (then (return (i64.const 9223372036854775806))))                             ;; negative index -> null sentinel
  (local.set $len (i64.load (local.get $array)))            ;; length
  (if (i64.ge_s (local.get $index) (local.get $len))        ;; out of bounds -> null
    (then (return (i64.const 9223372036854775806))))                             ;; out of bounds -> null sentinel
  (i64.load (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $index) (i64.const 8))))))  ;; slot[index]
"#;

/// `__rt_array_get_tagged_int`: reads an integer as a `(payload, tag)` pair.
///
/// A miss returns tag 8 (null) without reserving an i64 payload value, so the
/// former null sentinel remains a valid, distinguishable PHP integer in-bounds.
const RT_ARRAY_GET_TAGGED_INT: &str = r#"(func $__rt_array_get_tagged_int (param $array i32) (param $index i64) (result i64) (result i32)
  (local $len i64)
  (if (i64.lt_s (local.get $index) (i64.const 0))           ;; negative index -> tagged null
    (then (return (i64.const 0) (i32.const 8))))             ;; payload 0, null tag
  (local.set $len (i64.load (local.get $array)))            ;; length
  (if (i64.ge_u (local.get $index) (local.get $len))        ;; out of bounds -> tagged null
    (then (return (i64.const 0) (i32.const 8))))             ;; payload 0, null tag
  (i64.load (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $index) (i64.const 8)))))  ;; payload = slot[index]
  (i32.const 0))                                             ;; integer tag
"#;

/// `__rt_array_get_mixed_bool`: reads a boolean into a fresh Mixed cell.
///
/// In-range values use tag 3 and a 0/1 payload; misses use tag 8 so truthiness,
/// `is_null`, casts, and coalescing cannot observe the old integer sentinel.
const RT_ARRAY_GET_MIXED_BOOL: &str = r#"(func $__rt_array_get_mixed_bool (param $array i32) (param $index i64) (result i32)
  (local $len i64)
  (if (i64.lt_s (local.get $index) (i64.const 0))           ;; negative index -> boxed null
    (then (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
  (local.set $len (i64.load (local.get $array)))            ;; length
  (if (i64.ge_u (local.get $index) (local.get $len))        ;; out of bounds -> boxed null
    (then (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
  (call $__rt_mixed_from_value
    (i64.const 3)                                            ;; bool tag
    (i64.load (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $index) (i64.const 8)))))  ;; 0/1 payload
    (i64.const 0)))                                          ;; unused high payload
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
  ;; Copy-on-write split, exactly as `__rt_array_set_*` does: a shared array must be
  ;; cloned before it is appended to, or the append is visible through every other
  ;; reference — and `__rt_array_grow` frees the old block under them.
  (local.set $array (call $__rt_array_ensure_unique (local.get $array)))
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
    (then (local.set $array (call $__rt_array_grow (local.get $array)))))        ;; update array pointer after grow
  (local.set $alen (i64.load (local.get $array)))         ;; reload length after grow
  (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $alen) (i64.const 16)))))  ;; slot = A+24+len*16
  (i64.store (local.get $slot) (i64.extend_i32_u (local.get $newptr)))     ;; pointer (zero-extended) at slot+0
  (i64.store (i32.add (local.get $slot) (i32.const 8)) (local.get $plen))  ;; length at slot+8
  (i64.store (local.get $array) (i64.add (local.get $alen) (i64.const 1))) ;; length++
  (local.get $array))                                                            ;; return the (possibly new) array
"#;

/// `__rt_array_get_str`: reads the (pointer, length) string element at `index`,
/// returning the null/empty pair (0, 0) for a negative or out-of-bounds index.
const RT_ARRAY_GET_STR: &str = r#"(func $__rt_array_get_str (param $array i32) (param $index i64) (result i32) (result i64)
  (local $len i64)
  (local $slot i32)
  (if (i64.lt_s (local.get $index) (i64.const 0))         ;; negative index -> null pair
    (then (return (i32.const 0) (i64.const 0))))                                 ;; negative index -> null pair
  (local.set $len (i64.load (local.get $array)))          ;; length
  (if (i64.ge_u (local.get $index) (local.get $len))      ;; out of bounds -> null pair
    (then (return (i32.const 0) (i64.const 0))))                                 ;; out of bounds -> null pair
  (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $index) (i64.const 16)))))  ;; slot = A+24+index*16
  (i32.wrap_i64 (i64.load (local.get $slot)))             ;; result 0: pointer (wrapped from i64)
  (i64.load (i32.add (local.get $slot) (i32.const 8))))   ;; result 1: length
"#;

/// `__rt_array_get_mixed_str`: reads a string into a fresh Mixed cell.
///
/// Boxing preserves the distinction between an in-range empty string (tag 1)
/// and a missing element (tag 8); `__rt_mixed_from_value` persists string bytes.
const RT_ARRAY_GET_MIXED_STR: &str = r#"(func $__rt_array_get_mixed_str (param $array i32) (param $index i64) (result i32)
  (local $len i64)
  (local $slot i32)
  (if (i64.lt_s (local.get $index) (i64.const 0))           ;; negative index -> boxed null
    (then (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
  (local.set $len (i64.load (local.get $array)))            ;; length
  (if (i64.ge_u (local.get $index) (local.get $len))        ;; out of bounds -> boxed null
    (then (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
  (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $index) (i64.const 16)))))  ;; slot = A+24+index*16
  (call $__rt_mixed_from_value
    (i64.const 1)                                            ;; string tag
    (i64.load (local.get $slot))                             ;; pointer payload
    (i64.load (i32.add (local.get $slot) (i32.const 8)))))   ;; length payload
"#;

/// `__rt_array_push_mixed`: appends a kind-5 Mixed-cell pointer, shaping an empty
/// array to 16-byte slots with `value_type` 7 (mixed-cell), and growing capacity
/// when full. The cell is stored BORROWED (no incref here): the caller — the P7a1
/// `ClosureCall` arg-buffer builder — owns the cell it just boxed via
/// `__rt_mixed_from_value` and transfers that ownership to the array, whose
/// `__rt_array_free_deep` (reached through `__rt_decref_any` kind-2) releases every
/// cell. Returns the (possibly new) array pointer.
const RT_ARRAY_PUSH_MIXED: &str = r#"(func $__rt_array_push_mixed (param $array i32) (param $cell i32) (result i32)
  (local $alen i64)
  (local $cap i64)
  (local $slot i32)
  ;; Copy-on-write split, exactly as `__rt_array_set_*` does: a shared array must be
  ;; cloned before it is appended to, or the append is visible through every other
  ;; reference — and `__rt_array_grow` frees the old block under them.
  (local.set $array (call $__rt_array_ensure_unique (local.get $array)))
  (if (i64.eqz (i64.load (local.get $array)))             ;; empty -> shape as a mixed-cell array (16B slots, value_type 7)
    (then
      (i64.store (i32.add (local.get $array) (i32.const 8))
                 (i64.div_u (i64.mul (i64.load (i32.add (local.get $array) (i32.const 8))) (i64.load (i32.add (local.get $array) (i32.const 16)))) (i64.const 16)))  ;; rescale capacity to 16-byte slots
      (i64.store (i32.add (local.get $array) (i32.const 16)) (i64.const 16))  ;; elem_size = 16
      (i64.store (i32.sub (local.get $array) (i32.const 8))
                 (i64.or (i64.and (i64.load (i32.sub (local.get $array) (i32.const 8))) (i64.const -32513)) (i64.const 1792)))))  ;; value_type = 7 (mixed cell; 7 << 8 = 1792)
  (local.set $cap (i64.load (i32.add (local.get $array) (i32.const 8))))  ;; capacity
  (local.set $alen (i64.load (local.get $array)))         ;; length
  (if (i64.ge_u (local.get $alen) (local.get $cap))       ;; full -> grow
    (then (local.set $array (call $__rt_array_grow (local.get $array)))))        ;; update array pointer after grow
  (local.set $alen (i64.load (local.get $array)))         ;; reload length after grow
  (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $alen) (i64.const 16)))))  ;; slot = A+24+len*16
  (i64.store (local.get $slot) (i64.extend_i32_u (local.get $cell)))  ;; cell pointer (zero-extended) at slot+0
  (i64.store (i32.add (local.get $slot) (i32.const 8)) (i64.const 0))  ;; slot+8 unused (0)
  (i64.store (local.get $array) (i64.add (local.get $alen) (i64.const 1)))  ;; length++
  (local.get $array))                                       ;; return the (possibly new) array
"#;

/// `__rt_array_widen_to_mixed`: copies a concrete-element array into a fresh Mixed-cell one.
///
/// A `mixed` destination is a `value_type`-7 array of boxed cells, which is a DIFFERENT layout
/// from the int (8-byte slots), bool (8-byte) and string (16-byte) arrays this target
/// specializes — so handing one over where the other is expected is a real conversion, not a
/// pointer copy. `$tag` is the cell tag every element gets (0 int, 1 string, 3 bool) and `$esz`
/// the source slot stride; a 16-byte source slot carries a (pointer, length) pair, so its second
/// word becomes the cell's `hi` payload.
///
/// The result is a FRESH array with one reference. `__rt_mixed_from_value` persists string bytes
/// and `__rt_array_push_mixed` stores each cell borrowed, so the new array owns every cell and
/// `__rt_array_free_deep` releases them. Copying also gives PHP's by-value array parameter
/// semantics for free: mutating the callee's copy cannot reach the caller's array.
const RT_ARRAY_WIDEN_TO_MIXED: &str = r#"(func $__rt_array_widen_to_mixed (param $src i32) (param $tag i64) (param $esz i64) (result i32)
  (local $len i64)
  (local $i i64)
  (local $slot i32)
  (local $out i32)
  (local $lo i64)
  (local $hi i64)
  (if (i32.eqz (local.get $src))
    (then (return (call $__rt_array_new (i64.const 0) (i64.const 16)))))  ;; null source -> empty
  (local.set $len (i64.load (local.get $src)))              ;; source length
  (local.set $out (call $__rt_array_new (local.get $len) (i64.const 16)))  ;; 16B slots for cells
  (local.set $i (i64.const 0))                              ;; i = 0
  (block $end (loop $next
    (br_if $end (i64.ge_u (local.get $i) (local.get $len)))  ;; stop past the last element
    (local.set $slot (i32.add (i32.add (local.get $src) (i32.const 24))
                              (i32.wrap_i64 (i64.mul (local.get $i) (local.get $esz)))))  ;; slot = S+24+i*esz
    (local.set $lo (i64.load (local.get $slot)))            ;; payload lo (scalar, or string pointer)
    (local.set $hi (i64.const 0))                           ;; scalars have no high payload
    (if (i64.eq (local.get $esz) (i64.const 16))
      (then (local.set $hi (i64.load (i32.add (local.get $slot) (i32.const 8))))))  ;; string length
    (local.set $out (call $__rt_array_push_mixed (local.get $out)
      (call $__rt_mixed_from_value (local.get $tag) (local.get $lo) (local.get $hi))))  ;; box + append
    (local.set $i (i64.add (local.get $i) (i64.const 1)))   ;; i++
    (br $next)))
  (local.get $out))                                          ;; the fresh mixed-cell array
"#;

/// `__rt_array_slice`: PHP's `array_slice` over a LIST, answering a fresh `array<mixed>`.
///
/// The offset/length rules are byte-for-byte `substr`'s, verified on 52 offset/length pairs
/// against php-src: a negative offset counts from the end and floors at 0, an offset at or past
/// the end gives an empty result, a negative length drops that many from the end, and a length is
/// clamped so the window never runs past the end or backwards.
///
/// Both bounds are CLAMPED into `[-n, n]` before any arithmetic, so `PHP_INT_MIN` as a length
/// cannot wrap an i64 — negating it would.
///
/// `$tag` selects how a source slot becomes a cell: 0/1/2/3 box the payload with that tag (an
/// `$esz` of 16 means the slot is a (pointer, length) pair), and a NEGATIVE tag means the slot
/// already holds a cell, which is shared with an incref rather than copied. Sharing is sound only
/// while cells are immutable once stored, which is true as long as no lowered setter rewrites one
/// in place.
const RT_ARRAY_SLICE: &str = r#"(func $__rt_array_slice (param $src i32) (param $off i64) (param $len i64) (param $has_len i32) (param $tag i64) (param $esz i64) (result i32)
  (local $n i64)
  (local $start i64)
  (local $end i64)
  (local $i i64)
  (local $slot i32)
  (local $out i32)
  (local $cell i32)
  (if (i32.eqz (local.get $src))
    (then (return (call $__rt_array_new (i64.const 0) (i64.const 16)))))  ;; null source -> empty
  (local.set $n (i64.load (local.get $src)))                ;; source length
  ;; clamp the offset into [-n, n] so the arithmetic below cannot wrap
  (if (i64.lt_s (local.get $off) (i64.sub (i64.const 0) (local.get $n)))
    (then (local.set $off (i64.sub (i64.const 0) (local.get $n)))))
  (if (i64.gt_s (local.get $off) (local.get $n))
    (then (local.set $off (local.get $n))))
  (local.set $start (local.get $off))
  (if (i64.lt_s (local.get $start) (i64.const 0))           ;; negative offset counts from the end
    (then (local.set $start (i64.add (local.get $n) (local.get $start)))))
  (if (i64.lt_s (local.get $start) (i64.const 0))
    (then (local.set $start (i64.const 0))))                ;; and floors at the first element
  (local.set $end (local.get $n))                           ;; no length -> to the end
  (if (local.get $has_len)
    (then
      ;; clamp the length into [-n, n] for the same reason
      (if (i64.lt_s (local.get $len) (i64.sub (i64.const 0) (local.get $n)))
        (then (local.set $len (i64.sub (i64.const 0) (local.get $n)))))
      (if (i64.gt_s (local.get $len) (local.get $n))
        (then (local.set $len (local.get $n))))
      (if (i64.lt_s (local.get $len) (i64.const 0))
        (then (local.set $end (i64.add (local.get $n) (local.get $len))))   ;; drop |len| from the end
        (else (local.set $end (i64.add (local.get $start) (local.get $len)))))
      (if (i64.gt_s (local.get $end) (local.get $n))
        (then (local.set $end (local.get $n))))))
  (if (i64.lt_s (local.get $end) (local.get $start))        ;; never run backwards
    (then (local.set $end (local.get $start))))
  (local.set $out (call $__rt_array_new (i64.sub (local.get $end) (local.get $start)) (i64.const 16)))
  (local.set $i (local.get $start))
  (block $done (loop $next
    (br_if $done (i64.ge_s (local.get $i) (local.get $end)))
    (local.set $slot (i32.add (i32.add (local.get $src) (i32.const 24))
                              (i32.wrap_i64 (i64.mul (local.get $i) (local.get $esz)))))
    (if (i64.lt_s (local.get $tag) (i64.const 0))
      (then                                                 ;; the slot already holds a cell
        (local.set $cell (i32.wrap_i64 (i64.load (local.get $slot))))
        (call $__rt_incref (local.get $cell))               ;; both arrays own it now
        (local.set $out (call $__rt_array_push_mixed (local.get $out) (local.get $cell))))
      (else                                                 ;; box the payload with its tag
        (local.set $out (call $__rt_array_push_mixed (local.get $out)
          (call $__rt_mixed_from_value (local.get $tag)
            (i64.load (local.get $slot))
            (select (i64.load (i32.add (local.get $slot) (i32.const 8))) (i64.const 0)
                    (i64.eq (local.get $esz) (i64.const 16))))))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $next)))
  (local.get $out))
"#;

/// `__rt_range_int`: PHP's two-argument `range` over integers.
///
/// The step form does not exist here — the front-end rejects any arity but two — so the step is
/// always 1 and the direction comes from the operands: `range(5, 1)` counts DOWN. A single-element
/// range is `range(n, n)`, which is why the count is the span PLUS one.
///
/// The span is computed with wrapping arithmetic and then checked: a range spanning more than
/// `i64::MAX` elements cannot have its count represented at all, so it asks for a layout
/// `__rt_checked_layout` is guaranteed to reject, which raises the same deterministic
/// out-of-memory PHP raises for a range that large.
const RT_RANGE_INT: &str = r#"(func $__rt_range_int (param $start i64) (param $end i64) (result i32)
  (local $span i64)
  (local $count i64)
  (local $step i64)
  (local $out i32)
  (local $v i64)
  (local.set $step (i64.const 1))
  (local.set $span (i64.sub (local.get $end) (local.get $start)))
  (if (i64.lt_s (local.get $end) (local.get $start))         ;; descending range
    (then
      (local.set $step (i64.const -1))
      (local.set $span (i64.sub (local.get $start) (local.get $end)))))
  (if (i64.ge_u (local.get $span) (i64.const 9223372036854775807))  ;; the count would not fit
    (then (drop (call $__rt_checked_layout (i64.const -1) (i64.const 8) (i64.const 24)))))  ;; raises OOM
  (local.set $count (i64.add (local.get $span) (i64.const 1)))
  (local.set $out (call $__rt_array_new (local.get $count) (i64.const 8)))
  (local.set $v (local.get $start))
  (block $done (loop $next
    (br_if $done (i64.eqz (local.get $count)))
    (local.set $out (call $__rt_array_push_int (local.get $out) (local.get $v)))
    (local.set $v (i64.add (local.get $v) (local.get $step)))
    (local.set $count (i64.sub (local.get $count) (i64.const 1)))
    (br $next)))
  (local.get $out))
"#;

/// `__rt_array_find_int`: first index whose 8-byte scalar slot equals the needle, or -1.
///
/// Covers an int needle in an int haystack and a bool needle in a bool haystack, where PHP's loose
/// and strict answers coincide — same type on both sides, so `==` and `===` agree.
///
/// Answering the INDEX rather than a flag lets `in_array` and `array_search` share one scan: the
/// first tests it against zero, the second boxes it.
///
/// `$blocks` short-circuits to "not found": it is set when the CALLER knows a strict request
/// cannot match, because the needle and the elements are different types. `===` compares types
/// first, so no element can ever match and the scan is skipped entirely.
const RT_ARRAY_FIND_INT: &str = r#"(func $__rt_array_find_int (param $needle i64) (param $array i32) (param $blocks i32) (result i64)
  (local $i i64) (local $len i64)
  (if (local.get $blocks)
    (then (return (i64.const -1))))                         ;; strict across types: nothing can match
  (if (i32.eqz (local.get $array))
    (then (return (i64.const -1))))
  (local.set $len (i64.load (local.get $array)))
  (block $done (loop $scan
    (br_if $done (i64.ge_s (local.get $i) (local.get $len)))
    (if (i64.eq (local.get $needle)
                (i64.load (i32.add (i32.add (local.get $array) (i32.const 24))
                                   (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8))))))
      (then (return (local.get $i))))                       ;; first match wins, as PHP scans forward
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $scan)))
  (i64.const -1))
"#;

/// `__rt_array_find_float`: first index whose 8-byte scalar slot equals the needle as a double,
/// or -1.
///
/// `$widen` says the slots hold INTEGERS to convert rather than raw f64 bits, which is the
/// float-needle-in-an-int-haystack case. The opposite mix — an int needle in a float haystack —
/// widens the needle at the call site instead, so only one direction needs handling here.
///
/// The conversion is PHP's own: it widens and compares as doubles, precision loss included.
/// `f64.eq` answers false for NaN, which is what PHP does too.
const RT_ARRAY_FIND_FLOAT: &str = r#"(func $__rt_array_find_float (param $needle f64) (param $array i32) (param $blocks i32) (param $widen i32) (result i64)
  (local $i i64) (local $len i64) (local $slot i32) (local $v f64)
  (if (local.get $blocks)
    (then (return (i64.const -1))))                         ;; strict across types: nothing can match
  (if (i32.eqz (local.get $array))
    (then (return (i64.const -1))))
  (local.set $len (i64.load (local.get $array)))
  (block $done (loop $scan
    (br_if $done (i64.ge_s (local.get $i) (local.get $len)))
    (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24))
                              (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8)))))
    (local.set $v (f64.load (local.get $slot)))
    (if (local.get $widen)
      (then (local.set $v (f64.convert_i64_s (i64.load (local.get $slot))))))
    (if (f64.eq (local.get $needle) (local.get $v))
      (then (return (local.get $i))))                       ;; first match wins, as PHP scans forward
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $scan)))
  (i64.const -1))
"#;


/// `__rt_sort_scalar`: PHP's `sort`/`rsort` over 8-byte scalar slots, in place.
///
/// Copy-on-write-uniques the array first, then STABLE bubble sorts — PHP's sorts have been stable
/// since 8.0, so equal elements must keep their order, which is why the swap test is STRICT.
/// `$desc` reverses it for `rsort`; `$is_float` reads the slots as doubles rather than integers.
///
/// Returns the (possibly cloned) array pointer, which the call site writes back into the by-
/// reference argument's local — `sort($a)` rebinds `$a`.
/// `__rt_sort_string`: `sort`/`rsort` over an array of STRINGS, ordered by php-src's own
/// string comparison — which compares two NUMERIC strings numerically.
///
/// The same stable bubble walk as `__rt_sort_scalar`, over 16-byte `(ptr, len)` slots, with
/// `__rt_str_smart_cmp` deciding each pair. Both words of a slot move together on a swap.
pub(super) const RT_SORT_STRING: &str = r#"(func $__rt_sort_string (param $arr i32) (param $desc i32) (result i32)
  (local $len i64)
  (local $i i64)
  (local $j i64)
  (local $pa i32)
  (local $pb i32)
  (local $ap i64)
  (local $al i64)
  (local $bp i64)
  (local $bl i64)
  (local $ord i64)
  (local.set $arr (call $__rt_array_ensure_unique (local.get $arr)))
  (local.set $len (i64.load (local.get $arr)))
  (if (i64.lt_s (local.get $len) (i64.const 2))
    (then (return (local.get $arr))))
  (local.set $i (i64.const 0))
  (block $outer_end (loop $outer
    (br_if $outer_end (i64.ge_s (local.get $i) (local.get $len)))
    (local.set $j (i64.const 0))
    (block $inner_end (loop $inner
      (br_if $inner_end (i64.ge_s (local.get $j)
        (i64.sub (i64.sub (local.get $len) (i64.const 1)) (local.get $i))))
      (local.set $pa (i32.add (i32.add (local.get $arr) (i32.const 24))
                              (i32.wrap_i64 (i64.mul (local.get $j) (i64.const 16)))))
      (local.set $pb (i32.add (local.get $pa) (i32.const 16)))
      (local.set $ap (i64.load (local.get $pa)))
      (local.set $al (i64.load (i32.add (local.get $pa) (i32.const 8))))
      (local.set $bp (i64.load (local.get $pb)))
      (local.set $bl (i64.load (i32.add (local.get $pb) (i32.const 8))))
      (local.set $ord (call $__rt_str_smart_cmp
        (i32.wrap_i64 (local.get $ap)) (local.get $al)
        (i32.wrap_i64 (local.get $bp)) (local.get $bl)))
      (if (select
            (i64.lt_s (local.get $ord) (i64.const 0))
            (i64.gt_s (local.get $ord) (i64.const 0))
            (local.get $desc))
        (then
          (i64.store (local.get $pa) (local.get $bp))
          (i64.store (i32.add (local.get $pa) (i32.const 8)) (local.get $bl))
          (i64.store (local.get $pb) (local.get $ap))
          (i64.store (i32.add (local.get $pb) (i32.const 8)) (local.get $al))))
      (local.set $j (i64.add (local.get $j) (i64.const 1)))
      (br $inner)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $outer)))
  (local.get $arr))
"#;

const RT_SORT_SCALAR: &str = r#"(func $__rt_sort_scalar (param $arr i32) (param $desc i32) (param $is_float i32) (result i32)
  (local $len i64)
  (local $i i64)
  (local $j i64)
  (local $pa i32)
  (local $pb i32)
  (local $a i64)
  (local $b i64)
  (local $swap i32)
  (local.set $arr (call $__rt_array_ensure_unique (local.get $arr)))
  (local.set $len (i64.load (local.get $arr)))
  (if (i64.lt_s (local.get $len) (i64.const 2))
    (then (return (local.get $arr))))
  (local.set $i (i64.const 0))
  (block $outer_end (loop $outer
    (br_if $outer_end (i64.ge_s (local.get $i) (local.get $len)))
    (local.set $j (i64.const 0))
    (block $inner_end (loop $inner
      (br_if $inner_end (i64.ge_s (local.get $j)
        (i64.sub (i64.sub (local.get $len) (i64.const 1)) (local.get $i))))
      (local.set $pa (i32.add (i32.add (local.get $arr) (i32.const 24))
                              (i32.wrap_i64 (i64.mul (local.get $j) (i64.const 8)))))
      (local.set $pb (i32.add (local.get $pa) (i32.const 8)))
      (local.set $a (i64.load (local.get $pa)))
      (local.set $b (i64.load (local.get $pb)))
      (if (local.get $is_float)
        (then (local.set $swap (select
                (f64.lt (f64.reinterpret_i64 (local.get $a)) (f64.reinterpret_i64 (local.get $b)))
                (f64.gt (f64.reinterpret_i64 (local.get $a)) (f64.reinterpret_i64 (local.get $b)))
                (local.get $desc))))
        (else (local.set $swap (select
                (i64.lt_s (local.get $a) (local.get $b))
                (i64.gt_s (local.get $a) (local.get $b))
                (local.get $desc)))))
      (if (local.get $swap)                                     ;; strict: equal keeps its order
        (then
          (i64.store (local.get $pa) (local.get $b))
          (i64.store (local.get $pb) (local.get $a))))
      (local.set $j (i64.add (local.get $j) (i64.const 1)))
      (br $inner)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $outer)))
  (local.get $arr))
"#;

/// `__rt_array_ensure_unique`: the copy-on-write split point. Returns the array
/// unchanged when it has at most one owner (refcount <= 1); otherwise clones it
/// shallowly, decrements the original's refcount (this caller's reference now
/// points at the clone), and returns the clone. COW is refcount-driven — the COW
/// bit in the kind word is only an "is a container" marker, never inspected here.
const RT_ARRAY_ENSURE_UNIQUE: &str = r#"(func $__rt_array_ensure_unique (param $array i32) (result i32)
  (local $refcount i32)
  (local $clone i32)
  (if (i32.eqz (local.get $array))
    (then (return (i32.const 0))))                          ;; null array -> trivially unique
  (local.set $refcount (i32.load (i32.sub (local.get $array) (i32.const 12))))  ;; refcount @ A-12
  (if (i32.le_s (local.get $refcount) (i32.const 1))
    (then (return (local.get $array))))                     ;; sole owner -> already unique
  (local.set $clone (call $__rt_array_clone_shallow (local.get $array)))  ;; duplicate before mutation
  (i32.store (i32.sub (local.get $array) (i32.const 12)) (i32.sub (local.get $refcount) (i32.const 1)))  ;; original loses this reference
  (local.get $clone))                                       ;; caller now owns the clone
"#;

/// `__rt_array_clone_shallow`: allocates a fresh array with the source's capacity
/// and elem_size, byte-copies its live payload, then gives the clone independent
/// ownership of children: string elements are re-persisted (the clone owns fresh
/// copies; the source keeps its own), and refcounted container elements are
/// increfed. Scalar/float arrays need no fixup beyond the byte copy.
const RT_ARRAY_CLONE_SHALLOW: &str = r#"(func $__rt_array_clone_shallow (param $array i32) (result i32)
  (local $len i64)
  (local $cap i64)
  (local $esz i64)
  (local $kindw i64)
  (local $new i32)
  (local $vt i32)
  (local $i i64)
  (local $slot i32)
  (local $oldptr i32)
  (local $slen i64)
  (local $np i32)
  (local $nl i64)
  (local $src i32)
  (local $dst i32)
  (local $j i64)
  (local.set $len (i64.load (local.get $array)))            ;; length @ A+0
  (local.set $cap (i64.load (i32.add (local.get $array) (i32.const 8))))   ;; capacity @ A+8
  (local.set $esz (i64.load (i32.add (local.get $array) (i32.const 16))))  ;; elem_size @ A+16
  (local.set $kindw (i64.load (i32.sub (local.get $array) (i32.const 8)))) ;; kind word @ A-8
  (local.set $new (call $__rt_array_new (local.get $cap) (local.get $esz)))  ;; fresh array, refcount=1
  (i64.store (i32.sub (local.get $new) (i32.const 8)) (i64.and (local.get $kindw) (i64.const 65535)))  ;; preserve kind/value_type/COW (low 16 bits)
  (i64.store (local.get $new) (local.get $len))             ;; restore length (array_new zeroed it)
  (local.set $src (i32.add (local.get $array) (i32.const 24)))  ;; source payload start
  (local.set $dst (i32.add (local.get $new) (i32.const 24)))    ;; dest payload start
  (local.set $j (i64.mul (local.get $len) (local.get $esz)))    ;; live payload byte count
  (block $bend (loop $bcopy
    (br_if $bend (i64.le_s (local.get $j) (i64.const 0)))   ;; copied all live bytes
    (i32.store8 (local.get $dst) (i32.load8_u (local.get $src)))  ;; copy one byte
    (local.set $src (i32.add (local.get $src) (i32.const 1)))     ;; advance source
    (local.set $dst (i32.add (local.get $dst) (i32.const 1)))     ;; advance dest
    (local.set $j (i64.sub (local.get $j) (i64.const 1)))        ;; bytes remaining--
    (br $bcopy)))                                                                ;; next byte
  (local.set $vt (i32.and (i32.wrap_i64 (i64.shr_u (local.get $kindw) (i64.const 8))) (i32.const 127)))  ;; value_type = bits 8..14
  (if (i32.eq (local.get $vt) (i32.const 1))                ;; string elements -> own fresh copies
    (then
      (local.set $i (i64.const 0))                                               ;; i = 0 (string element cursor)
      (block $send (loop $sclone
        (br_if $send (i64.ge_s (local.get $i) (local.get $len)))                 ;; stop when all string elements cloned
        (local.set $slot (i32.add (i32.add (local.get $new) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))  ;; slot = new+24+i*16
        (local.set $oldptr (i32.wrap_i64 (i64.load (local.get $slot))))           ;; shared ptr @ slot+0
        (local.set $slen (i64.load (i32.add (local.get $slot) (i32.const 8))))    ;; len @ slot+8
        (call $__rt_str_persist (local.get $oldptr) (local.get $slen))            ;; deep-copy the string
        (local.set $nl)                                     ;; persisted length (top of stack)
        (local.set $np)                                     ;; persisted pointer
        (i64.store (local.get $slot) (i64.extend_i32_u (local.get $np)))          ;; install owned ptr @ slot+0
        (i64.store (i32.add (local.get $slot) (i32.const 8)) (local.get $nl))     ;; install owned len @ slot+8
        (local.set $i (i64.add (local.get $i) (i64.const 1)))                    ;; i++
        (br $sclone))))                                                          ;; next string element
    (else
      (if (i32.or (i32.eq (local.get $vt) (i32.const 4))
          (i32.or (i32.eq (local.get $vt) (i32.const 5))
          (i32.or (i32.eq (local.get $vt) (i32.const 6))
          (i32.or (i32.eq (local.get $vt) (i32.const 7))
                  (i32.eq (local.get $vt) (i32.const 10))))))  ;; refcounted container children
        (then
          (local.set $i (i64.const 0))                                           ;; i = 0 (container element cursor)
          (block $rend (loop $rclone
            (br_if $rend (i64.ge_s (local.get $i) (local.get $len)))             ;; stop when all container elements increfed
            (call $__rt_incref (i32.wrap_i64 (i64.load (i32.add (i32.add (local.get $new) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $i) (local.get $esz)))))))  ;; share the child, bump its refcount
            (local.set $i (i64.add (local.get $i) (i64.const 1)))                ;; i++
            (br $rclone)))))))                                                   ;; next container element
  (local.get $new))                                         ;; return the independent clone
"#;

/// `__rt_array_preflight_set`: proves that an indexed assignment and every
/// doubling step needed to reach it fit the wasm32 payload ceiling.
///
/// This helper runs before copy-on-write or string persistence. It also validates
/// the source layout invariant (`0 <= length <= capacity`) and mirrors the
/// empty-string-array capacity rescaling performed by `__rt_array_set_str`.
const RT_ARRAY_PREFLIGHT_SET: &str = r#"(func $__rt_array_preflight_set (param $array i32) (param $index i64) (param $stride i64)
  (local $len i64)
  (local $cap i64)
  (local $esz i64)
  (local $newcap i64)
  (if (i64.lt_s (local.get $index) (i64.const 0))
    (then (call $__rt_oom) unreachable))                    ;; elephc-trap:deterministic-oom:array-set-negative-index callers reject negative indexes before preflight
  (if (i64.eq (local.get $index) (i64.const 9223372036854775807))
    (then (call $__rt_oom) unreachable))                    ;; elephc-trap:deterministic-oom:array-set-index-overflow index+1 would overflow i64
  (drop (call $__rt_checked_layout
    (i64.add (local.get $index) (i64.const 1))
    (local.get $stride)
    (i64.const 24)))                                        ;; exact target slot bound: header + (index+1)*stride
  (local.set $len (i64.load (local.get $array)))
  (local.set $cap (i64.load (i32.add (local.get $array) (i32.const 8))))
  (local.set $esz (i64.load (i32.add (local.get $array) (i32.const 16))))
  (drop (call $__rt_checked_layout (local.get $cap) (local.get $esz) (i64.const 24)))  ;; validate current allocation metadata
  (if (i64.gt_u (local.get $len) (local.get $cap))
    (then (call $__rt_oom) unreachable))                    ;; elephc-trap:deterministic-oom:array-set-malformed-length reject malformed source metadata before COW
  (if
    (i32.and
      (i64.eqz (local.get $len))
      (i64.eq (local.get $stride) (i64.const 16)))
    (then
      (local.set $cap
        (i64.div_u
          (i64.mul (local.get $cap) (local.get $esz))
          (i64.const 16)))))                                ;; mirror empty string-array shaping without mutation
  (block $done
    (loop $grow
      (br_if $done (i64.lt_u (local.get $index) (local.get $cap)))  ;; planned capacity reaches target index
      (local.set $newcap (i64.add (local.get $cap) (local.get $cap)))  ;; safe: current layout already bounded
      (if (i64.lt_u (local.get $newcap) (i64.const 8))
        (then (local.set $newcap (i64.const 8))))            ;; runtime minimum capacity
      (drop (call $__rt_checked_layout (local.get $newcap) (local.get $stride) (i64.const 24)))  ;; validate overshooting power-of-two growth too
      (local.set $cap (local.get $newcap))
      (br $grow))))
"#;

/// `__rt_array_set_int`: assigns a scalar (int/bool, or float bits as i64) at
/// `index`. Splits a shared array (COW), shapes an empty array to 8-byte slots,
/// grows to fit, zero-fills any gap between the old length and `index`, then
/// stores. Returns the (possibly cloned/reallocated) array pointer.
const RT_ARRAY_SET_INT: &str = r#"(func $__rt_array_set_int (param $array i32) (param $index i64) (param $value i64) (result i32)
  (local $oldlen i64)
  (local $j i64)
  (if (i64.lt_s (local.get $index) (i64.const 0))
    (then (return (local.get $array))))                     ;; reject negative index
  (call $__rt_array_preflight_set (local.get $array) (local.get $index) (i64.const 8))  ;; prove index+1 and all growth before COW
  (local.set $array (call $__rt_array_ensure_unique (local.get $array)))  ;; copy-on-write split
  (if (i64.eqz (i64.load (local.get $array)))               ;; empty -> shape as a scalar array
    (then
      (i64.store (i32.add (local.get $array) (i32.const 16)) (i64.const 8))  ;; elem_size = 8
      (i64.store (i32.sub (local.get $array) (i32.const 8))
                 (i64.and (i64.load (i32.sub (local.get $array) (i32.const 8))) (i64.const -32513)))))  ;; clear value_type bits 8-14
  (block $gend (loop $grow
    (br_if $gend (i64.lt_s (local.get $index) (i64.load (i32.add (local.get $array) (i32.const 8)))))  ;; index < capacity -> fits
    (local.set $array (call $__rt_array_grow (local.get $array)))  ;; double capacity
    (br $grow)))                                                                 ;; keep growing until index fits
  (local.set $oldlen (i64.load (local.get $array)))         ;; length after grow (grow preserves it)
  (if (i64.ge_s (local.get $index) (local.get $oldlen))     ;; writing at/after the end extends
    (then
      (local.set $j (local.get $oldlen))                                         ;; start fill cursor at current length
      (block $fend (loop $fill
        (br_if $fend (i64.ge_s (local.get $j) (local.get $index)))  ;; fill [oldlen, index)
        (i64.store (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $j) (i64.const 8)))) (i64.const 0))  ;; gap slot = 0
        (local.set $j (i64.add (local.get $j) (i64.const 1)))                    ;; j++
        (br $fill)))                                                             ;; next gap slot
      (i64.store (local.get $array) (i64.add (local.get $index) (i64.const 1)))))  ;; length = index+1
  (i64.store (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $index) (i64.const 8)))) (local.get $value))  ;; store element
  (local.get $array))                                                            ;; return the (possibly new) array
"#;

/// `__rt_array_set_str`: assigns a string at `index`. Splits a shared array (COW),
/// shapes an empty array to 16-byte string slots, persists the incoming string,
/// grows to fit, releases the previous string when overwriting, zero-fills any gap,
/// then stores. Returns the (possibly cloned/reallocated) array pointer.
const RT_ARRAY_SET_STR: &str = r#"(func $__rt_array_set_str (param $array i32) (param $index i64) (param $ptr i32) (param $len i64) (result i32)
  (local $oldlen i64)
  (local $np i32)
  (local $nl i64)
  (local $oldp i32)
  (local $slot i32)
  (local $j i64)
  (if (i64.lt_s (local.get $index) (i64.const 0))
    (then (return (local.get $array))))                     ;; reject negative index
  (call $__rt_array_preflight_set (local.get $array) (local.get $index) (i64.const 16))  ;; prove index+1 and all growth before COW
  (drop (call $__rt_checked_layout (local.get $len) (i64.const 1) (i64.const 0)))  ;; reject invalid/oversized string length before COW
  (local.set $array (call $__rt_array_ensure_unique (local.get $array)))  ;; copy-on-write split
  (if (i64.eqz (i64.load (local.get $array)))               ;; empty -> shape as a string array
    (then
      (i64.store (i32.add (local.get $array) (i32.const 16)) (i64.const 16))  ;; elem_size = 16
      (i64.store (i32.sub (local.get $array) (i32.const 8))
                 (i64.or (i64.and (i64.load (i32.sub (local.get $array) (i32.const 8))) (i64.const -32513)) (i64.const 256)))))  ;; value_type = 1 (string)
  (call $__rt_str_persist (local.get $ptr) (local.get $len))  ;; own a copy of the incoming string
  (local.set $nl)                                           ;; persisted length (top of stack)
  (local.set $np)                                           ;; persisted pointer
  (block $gend (loop $grow
    (br_if $gend (i64.lt_s (local.get $index) (i64.load (i32.add (local.get $array) (i32.const 8)))))  ;; index < capacity -> fits
    (local.set $array (call $__rt_array_grow (local.get $array)))  ;; double capacity
    (br $grow)))                                                                 ;; keep growing until index fits
  (local.set $oldlen (i64.load (local.get $array)))         ;; length after grow
  (if (i64.lt_s (local.get $index) (local.get $oldlen))     ;; overwriting an existing element
    (then
      (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $index) (i64.const 16)))))  ;; slot = A+24+index*16
      (local.set $oldp (i32.wrap_i64 (i64.load (local.get $slot))))  ;; previous string ptr
      (call $__rt_heap_free_safe (local.get $oldp)))        ;; release the overwritten string
    (else
      (local.set $j (local.get $oldlen))                                         ;; start fill cursor at current length
      (block $fend (loop $fill
        (br_if $fend (i64.ge_s (local.get $j) (local.get $index)))  ;; fill [oldlen, index)
        (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $j) (i64.const 16)))))  ;; gap slot
        (i64.store (local.get $slot) (i64.const 0))              ;; ptr = 0
        (i64.store (i32.add (local.get $slot) (i32.const 8)) (i64.const 0))  ;; len = 0
        (local.set $j (i64.add (local.get $j) (i64.const 1)))                    ;; j++
        (br $fill)))                                                             ;; next gap slot
      (i64.store (local.get $array) (i64.add (local.get $index) (i64.const 1)))))  ;; length = index+1
  (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $index) (i64.const 16)))))  ;; target slot
  (i64.store (local.get $slot) (i64.extend_i32_u (local.get $np)))           ;; store ptr @ slot+0
  (i64.store (i32.add (local.get $slot) (i32.const 8)) (local.get $nl))      ;; store len @ slot+8
  (local.get $array))                                                            ;; return the (possibly new) array
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
      (local.set $i (i64.const 0))                                               ;; i = 0 (child element cursor)
      (block $end (loop $rel
        (br_if $end (i64.ge_s (local.get $i) (local.get $len)))                  ;; stop when all children released
        (local.set $slot (i32.add (i32.add (local.get $array) (i32.const 24))
                                  (i32.wrap_i64 (i64.mul (local.get $i) (local.get $esz)))))  ;; slot base
        (call $__rt_decref_any (i32.wrap_i64 (i64.load (local.get $slot))))  ;; release the child by kind
        (local.set $i (i64.add (local.get $i) (i64.const 1)))                    ;; i++
        (br $rel)))))                                                            ;; next child element
  (call $__rt_heap_free (local.get $array)))                                     ;; free the array struct itself
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

/// `__rt_array_union`: the PHP `+` operator on two DENSE INDEXED arrays. The left
/// operand owns the lower integer keys `0..a.len-1`, so the result is a deep clone of
/// the left, then the right operand's TAIL (entries at indices `a.len..b.len-1`) is
/// appended — LEFT wins on key collision. Borrows `$a` and `$b` and returns a fresh
/// OWNED array. String elements go through `__rt_array_push_str` (which persists their
/// own copy); refcounted container/mixed elements (value_type 4..7) are increfed before
/// `__rt_array_push_int` retains the borrowed child; scalars are copied bits-as-is. The
/// value_type range 4..7 mirrors the native `__rt_array_union` dispatch.
const RT_ARRAY_UNION: &str = r#"(func $__rt_array_union (param $a i32) (param $b i32) (result i32)
  (local $i i64)
  (local.set $i (i64.load (local.get $a)))                              ;; i = a.len (first missing right index)
  (if (i64.eqz (local.get $i))                                          ;; left has no keys?
    (then (return (call $__rt_array_clone_shallow (local.get $b)))))   ;; result is just a copy of b
  (call $__rt_array_append_from
    (call $__rt_array_clone_shallow (local.get $a))                     ;; own a copy of the left operand
    (local.get $b)
    (local.get $i)))                                                    ;; append the right's TAIL only
"#;

/// `__rt_array_merge`: PHP's two-operand `array_merge` over LISTS.
///
/// Unlike `+`, which keeps the left's keys and takes only the right's surplus tail, `array_merge`
/// APPENDS every element of the right and reindexes — so this is the same walk starting at 0.
///
/// Both operands reach here with the same element type: when they differ, EIR widens each with
/// `Op::ArrayToMixed` before the call, so the result and both inputs agree on slot layout.
/// Borrows both and returns a fresh OWNED array; the element-copy discipline lives in
/// `__rt_array_append_from`.
const RT_ARRAY_MERGE: &str = r#"(func $__rt_array_merge (param $a i32) (param $b i32) (result i32)
  (if (i64.eqz (i64.load (local.get $a)))                               ;; left empty -> just a copy of b
    (then (return (call $__rt_array_clone_shallow (local.get $b)))))
  (call $__rt_array_append_from
    (call $__rt_array_clone_shallow (local.get $a))                     ;; own a copy of the left operand
    (local.get $b)
    (i64.const 0)))                                                     ;; append ALL of the right
"#;

/// `__rt_array_append_from`: appends `$b`'s elements from index `$start` onto an OWNED `$result`.
///
/// The element-copy discipline is the whole point of sharing this: a string element goes through
/// `__rt_array_push_str`, which persists a copy the result owns independently of `$b`; a
/// refcounted container (`value_type` 4..6) is increfed before the borrowed pointer is retained;
/// a Mixed cell (`value_type` 7) is increfed too but lives in a 16-BYTE slot, so it needs both a
/// different read stride and `__rt_array_push_mixed` — appending it as an 8-byte scalar writes
/// into the middle of the previous slot; every other slot is copied bits-as-is. Getting that wrong double-frees rather than
/// leaking, so `+` and `array_merge` both go through here.
///
/// Consumes `$result` (it may reallocate) and returns the live pointer. Borrows `$b`.
const RT_ARRAY_APPEND_FROM: &str = r#"(func $__rt_array_append_from (param $result i32) (param $b i32) (param $start i64) (result i32)
  (local $i i64) (local $blen i64) (local $vt i64)
  (local $slot i32) (local $ptr i32) (local $slen i64) (local $val i64)
  (local.set $i (local.get $start))
  (local.set $blen (i64.load (local.get $b)))                           ;; right length
  (local.set $vt (i64.and (i64.shr_u (i64.load (i32.sub (local.get $b) (i32.const 8))) (i64.const 8)) (i64.const 127)))  ;; right value_type tag
  (block $done (loop $walk
    (br_if $done (i64.ge_s (local.get $i) (local.get $blen)))           ;; appended everything asked for
    (if (i64.eq (local.get $vt) (i64.const 1))                          ;; string elements?
      (then
        (local.set $slot (i32.add (i32.add (local.get $b) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))  ;; &b string slot
        (local.set $ptr (i32.wrap_i64 (i64.load (local.get $slot))))    ;; borrowed string pointer
        (local.set $slen (i64.load (i32.add (local.get $slot) (i32.const 8))))  ;; string length
        (local.set $result (call $__rt_array_push_str (local.get $result) (local.get $ptr) (local.get $slen))))  ;; persist + append
      (else (if (i64.eq (local.get $vt) (i64.const 7))                  ;; Mixed cells: 16-BYTE slots
      (then
        ;; both the read stride and the append differ here — reading at 8 and appending with
        ;; __rt_array_push_int writes into the middle of the previous slot and corrupts it
        (local.set $ptr (i32.wrap_i64 (i64.load (i32.add (i32.add (local.get $b) (i32.const 24))
                                                         (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 16)))))))  ;; borrowed cell
        (call $__rt_incref (local.get $ptr))                            ;; the result shares the cell
        (local.set $result (call $__rt_array_push_mixed (local.get $result) (local.get $ptr))))
      (else
        (local.set $val (i64.load (i32.add (i32.add (local.get $b) (i32.const 24)) (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 8))))))  ;; scalar/container payload
        (if (i32.and (i64.ge_u (local.get $vt) (i64.const 4)) (i64.le_u (local.get $vt) (i64.const 7)))  ;; refcounted container range 4..7?
          (then
            (call $__rt_incref (i32.wrap_i64 (local.get $val)))         ;; retain the borrowed child
            (local.set $result (call $__rt_array_push_int (local.get $result) (local.get $val))))  ;; append container pointer
          (else
            (local.set $result (call $__rt_array_push_int (local.get $result) (local.get $val)))))))))  ;; append scalar bits
    (local.set $i (i64.add (local.get $i) (i64.const 1)))               ;; next right index
    (br $walk)))
  (local.get $result))                                                  ;; the (possibly reallocated) result
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
    use super::super::mixed::emit_mixed_runtime;
    use super::super::classes::{emit_class_metadata_stub, emit_class_runtime};
    use super::super::objects::{emit_destructor_dispatch_stub, emit_gc_desc_stub, emit_object_runtime};
    use super::super::refcount::emit_refcount_runtime;
    use super::super::closures::emit_closure_runtime;
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
        emit_closure_runtime(&mut wm);
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm, false);
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
        super::super::hashes::emit_hash_runtime(&mut wm);
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

    /// Builds and validates an indexed-array reactor whose exported driver must
    /// terminate through the deterministic reactor OOM trap.
    fn driver_traps(driver: &str, export: &str) {
        let mut wm = WatModule::new();
        wm.set_memory(3, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 3 * 65536);
        emit_refcount_runtime(&mut wm);
        emit_closure_runtime(&mut wm);
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm, false);
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
        super::super::hashes::emit_hash_runtime(&mut wm);
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
            "overflowing array driver unexpectedly succeeded\n{wat}"
        );
    }

    /// Maximum i64 capacities/indexes and invalid string lengths must terminate
    /// immediately instead of wrapping wasm32 or entering an unbounded loop.
    #[test]
    fn oversized_array_and_string_layouts_trap_without_large_allocation() {
        driver_traps(
            r#"(func $t (export "t")
  (drop (call $__rt_array_new (i64.const 9223372036854775807) (i64.const 8))))"#,
            "t",
        );
        driver_traps(
            r#"(func $t (export "t")
  (local $a i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 8)))
  (drop (call $__rt_array_set_int
    (local.get $a)
    (i64.const 9223372036854775807)
    (i64.const 1))))"#,
            "t",
        );
        driver_traps(
            r#"(func $t (export "t")
  (call $__rt_str_persist (i32.const 0) (i64.const -1))
  (drop)
  (drop))"#,
            "t",
        );
        driver_traps(
            r#"(func $t (export "t")
  (call $__rt_str_persist (i32.const 0) (i64.const 9223372036854775807))
  (drop)
  (drop))"#,
            "t",
        );
    }

    /// Assignment preflights target growth and inbound string length before the
    /// copy-on-write split or persistence can allocate or alter refcounts.
    #[test]
    fn array_set_preflight_precedes_every_observable_mutation() {
        let int_preflight = super::RT_ARRAY_SET_INT
            .find("call $__rt_array_preflight_set")
            .expect("integer assignment preflight");
        let int_cow = super::RT_ARRAY_SET_INT
            .find("call $__rt_array_ensure_unique")
            .expect("integer assignment COW");
        assert!(int_preflight < int_cow);

        let string_preflight = super::RT_ARRAY_SET_STR
            .find("call $__rt_array_preflight_set")
            .expect("string assignment preflight");
        let string_length = super::RT_ARRAY_SET_STR
            .find("call $__rt_checked_layout")
            .expect("string length preflight");
        let string_cow = super::RT_ARRAY_SET_STR
            .find("call $__rt_array_ensure_unique")
            .expect("string assignment COW");
        let string_persist = super::RT_ARRAY_SET_STR
            .find("call $__rt_str_persist")
            .expect("string persistence");
        assert!(string_preflight < string_cow);
        assert!(string_length < string_cow);
        assert!(string_cow < string_persist);
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

    /// Tagged integer reads report null for negative/OOB indexes while preserving
    /// the former sentinel bit pattern as a legitimate in-range integer payload.
    #[test]
    fn tagged_int_get_distinguishes_bounds_from_every_payload() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local $payload i64)
  (local $tag i32)
  (local $negative_tag i32)
  (local $oob_tag i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 9223372036854775806)))
  (call $__rt_array_get_tagged_int (local.get $a) (i64.const 0))
  (local.set $tag)
  (local.set $payload)
  (call $__rt_array_get_tagged_int (local.get $a) (i64.const -1))
  (local.set $negative_tag)
  (drop)
  (call $__rt_array_get_tagged_int (local.get $a) (i64.const 1))
  (local.set $oob_tag)
  (drop)
  (i64.extend_i32_u
    (i32.and
      (i32.and
        (i64.eq (local.get $payload) (i64.const 9223372036854775806))
        (i32.eq (local.get $tag) (i32.const 0)))
      (i32.and
        (i32.eq (local.get $negative_tag) (i32.const 8))
        (i32.eq (local.get $oob_tag) (i32.const 8))))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1");
        }
    }

    /// Boxed boolean reads keep the bool tag in-bounds and use the null tag for
    /// both negative and positive out-of-bounds indexes.
    #[test]
    fn mixed_bool_get_preserves_bool_and_null_tags() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local $cell i32)
  (local $in_tag i64)
  (local $negative_tag i64)
  (local $oob_tag i64)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 1)))
  (local.set $cell (call $__rt_array_get_mixed_bool (local.get $a) (i64.const 0)))
  (local.set $in_tag (i64.load (local.get $cell)))
  (call $__rt_decref_mixed (local.get $cell))
  (local.set $cell (call $__rt_array_get_mixed_bool (local.get $a) (i64.const -1)))
  (local.set $negative_tag (i64.load (local.get $cell)))
  (call $__rt_decref_mixed (local.get $cell))
  (local.set $cell (call $__rt_array_get_mixed_bool (local.get $a) (i64.const 1)))
  (local.set $oob_tag (i64.load (local.get $cell)))
  (call $__rt_decref_mixed (local.get $cell))
  (call $__rt_decref_array (local.get $a))
  (i64.add
    (i64.add
      (i64.add (i64.mul (local.get $in_tag) (i64.const 100)) (i64.mul (local.get $negative_tag) (i64.const 10)))
      (local.get $oob_tag))
    (global.get $_gc_live)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "388");
        }
    }

    /// Boxed string reads distinguish an in-range empty string (tag 1) from a
    /// missing element (tag 8), despite both raw payloads being `(0, 0)`.
    #[test]
    fn mixed_string_get_distinguishes_empty_string_from_null() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local $cell i32)
  (local $empty_tag i64)
  (local $missing_tag i64)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_str (local.get $a) (i32.const 0) (i64.const 0)))
  (local.set $cell (call $__rt_array_get_mixed_str (local.get $a) (i64.const 0)))
  (local.set $empty_tag (i64.load (local.get $cell)))
  (call $__rt_decref_mixed (local.get $cell))
  (local.set $cell (call $__rt_array_get_mixed_str (local.get $a) (i64.const 1)))
  (local.set $missing_tag (i64.load (local.get $cell)))
  (call $__rt_decref_mixed (local.get $cell))
  (call $__rt_decref_array (local.get $a))
  (i64.add
    (i64.add (i64.mul (local.get $empty_tag) (i64.const 10)) (local.get $missing_tag))
    (global.get $_gc_live)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "18");
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

    /// Overwriting an in-bounds index on a sole-owner array mutates in place: the
    /// setter returns the SAME pointer (no clone) and the element changes while the
    /// length is unchanged. Returns `(same_ptr)*1000 + a[1]` = 1099.
    #[test]
    fn set_int_unique_mutates_in_place() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 10)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 20)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 30)))
  (local.set $b (call $__rt_array_set_int (local.get $a) (i64.const 1) (i64.const 99)))
  (i64.add
    (i64.mul (i64.extend_i32_u (i32.eq (local.get $a) (local.get $b))) (i64.const 1000))
    (call $__rt_array_get_int (local.get $b) (i64.const 1))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "1099");
        }
    }

    /// Setting an index past the current length extends the array, zero-filling the
    /// gap. Returns `length*1000 + a[3] + a[1]` = 4*1000 + 77 + 0 = 4077.
    #[test]
    fn set_int_extends_with_gap_fill() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local.set $a (call $__rt_array_new (i64.const 2) (i64.const 16)))
  (local.set $a (call $__rt_array_set_int (local.get $a) (i64.const 3) (i64.const 77)))
  (i64.add (i64.add
    (i64.mul (i64.load (local.get $a)) (i64.const 1000))
    (call $__rt_array_get_int (local.get $a) (i64.const 3)))
    (call $__rt_array_get_int (local.get $a) (i64.const 1))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "4077");
        }
    }

    /// Copy-on-write: when an array is shared (refcount > 1), the setter clones it,
    /// leaving the original untouched. Returns `b[0]*100 + a[0]` = 99*100 + 10 =
    /// 9910 (clone has the new value, original keeps the old).
    #[test]
    fn set_int_cow_clones_shared_array() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 10)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 20)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 30)))
  (call $__rt_incref (local.get $a))
  (local.set $b (call $__rt_array_set_int (local.get $a) (i64.const 0) (i64.const 99)))
  (i64.add
    (i64.mul (call $__rt_array_get_int (local.get $b) (i64.const 0)) (i64.const 100))
    (call $__rt_array_get_int (local.get $a) (i64.const 0))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "9910");
        }
    }

    /// COW returns a DISTINCT pointer for a shared array and decrements the
    /// original's refcount back to 1. Returns `(distinct)*10 + original_refcount`
    /// = 1*10 + 1 = 11.
    #[test]
    fn set_int_cow_distinct_pointer_and_refcount() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 1)))
  (call $__rt_incref (local.get $a))
  (local.set $b (call $__rt_array_set_int (local.get $a) (i64.const 0) (i64.const 5)))
  (i64.add
    (i64.mul (i64.extend_i32_u (i32.ne (local.get $a) (local.get $b))) (i64.const 10))
    (i64.extend_i32_s (i32.load (i32.sub (local.get $a) (i32.const 12))))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "11");
        }
    }

    /// Setting a string at index 0 of an empty array persists it; reading it back
    /// returns the three bytes of "abc" packed as `97<<16|98<<8|99 = 6382179`.
    #[test]
    fn set_str_extends_and_reads_bytes() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $a i32) (local $p i32) (local $l i64)
  (i32.store8 (i32.const 200) (i32.const 97))
  (i32.store8 (i32.const 201) (i32.const 98))
  (i32.store8 (i32.const 202) (i32.const 99))
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_set_str (local.get $a) (i64.const 0) (i32.const 200) (i64.const 3)))
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

    /// Overwriting a string element frees the previous string, so deep-freeing the
    /// array afterwards balances `_gc_live` back to 0 (no leak, no double-free).
    #[test]
    fn set_str_overwrite_frees_old_and_balances_live() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (i32.store8 (i32.const 200) (i32.const 65))
  (i32.store8 (i32.const 201) (i32.const 66))
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_set_str (local.get $a) (i64.const 0) (i32.const 200) (i64.const 1)))
  (local.set $a (call $__rt_array_set_str (local.get $a) (i64.const 0) (i32.const 201) (i64.const 1)))
  (call $__rt_decref_array (local.get $a))
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }

    /// `__rt_array_union` on `[10,20] + [99,88,77]` keeps the left elements and appends
    /// only the right tail at index >= a.len, yielding `[10,20,77]` of length 3. Encoded
    /// as `len*1000000 + u0*10000 + u1*100 + u2` = 3*1000000 + 10*10000 + 20*100 + 77.
    #[test]
    fn array_union_int_left_wins_appends_tail() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32) (local $u i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 10)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 20)))
  (local.set $b (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $b (call $__rt_array_push_int (local.get $b) (i64.const 99)))
  (local.set $b (call $__rt_array_push_int (local.get $b) (i64.const 88)))
  (local.set $b (call $__rt_array_push_int (local.get $b) (i64.const 77)))
  (local.set $u (call $__rt_array_union (local.get $a) (local.get $b)))
  (i64.add (i64.add (i64.add
    (i64.mul (i64.load (local.get $u)) (i64.const 1000000))
    (i64.mul (call $__rt_array_get_int (local.get $u) (i64.const 0)) (i64.const 10000)))
    (i64.mul (call $__rt_array_get_int (local.get $u) (i64.const 1)) (i64.const 100)))
    (call $__rt_array_get_int (local.get $u) (i64.const 2))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "3102077");
        }
    }

    /// `__rt_array_union` BORROWS both operands: after `[10,20] + [99,88,77]` the left
    /// array still has length 2 and element 0 == 10 (it was cloned, never mutated).
    #[test]
    fn array_union_borrows_left_operand() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 10)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 20)))
  (local.set $b (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $b (call $__rt_array_push_int (local.get $b) (i64.const 99)))
  (local.set $b (call $__rt_array_push_int (local.get $b) (i64.const 77)))
  (drop (call $__rt_array_union (local.get $a) (local.get $b)))
  (i64.add (i64.mul (i64.load (local.get $a)) (i64.const 100))
           (call $__rt_array_get_int (local.get $a) (i64.const 0))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "210");
        }
    }

    /// `__rt_array_union` with an EMPTY left operand returns a clone of the right operand:
    /// `[] + [5,6]` yields `[5,6]` (length 2, element 1 == 6). Encoded as `len*100 + u1`.
    #[test]
    fn array_union_empty_left_copies_right() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32) (local $u i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $b (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $b (call $__rt_array_push_int (local.get $b) (i64.const 5)))
  (local.set $b (call $__rt_array_push_int (local.get $b) (i64.const 6)))
  (local.set $u (call $__rt_array_union (local.get $a) (local.get $b)))
  (i64.add (i64.mul (i64.load (local.get $u)) (i64.const 100))
           (call $__rt_array_get_int (local.get $u) (i64.const 1))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "206");
        }
    }

    /// `__rt_array_union` where the left is at least as long as the right appends no tail:
    /// `[1,2,3] + [9]` yields `[1,2,3]` (length 3, element 2 == 3). Encoded `len*100 + u2`.
    #[test]
    fn array_union_left_longer_appends_nothing() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32) (local $u i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 1)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 2)))
  (local.set $a (call $__rt_array_push_int (local.get $a) (i64.const 3)))
  (local.set $b (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $b (call $__rt_array_push_int (local.get $b) (i64.const 9)))
  (local.set $u (call $__rt_array_union (local.get $a) (local.get $b)))
  (i64.add (i64.mul (i64.load (local.get $u)) (i64.const 100))
           (call $__rt_array_get_int (local.get $u) (i64.const 2))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "303");
        }
    }

    /// `__rt_array_union` on STRING arrays persists and appends the right tail: with the
    /// bytes "a","b" on the left and "x","y","z" on the right, the result `["a","b","z"]`
    /// has length 3 and `u[2]` == "z" (first byte 122, length 1). Encoded `len*1000000 +
    /// u2_byte*1000 + u2_len`.
    #[test]
    fn array_union_string_appends_persisted_tail() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32) (local $u i32) (local $p i32) (local $l i64)
  (i32.store8 (i32.const 200) (i32.const 97))
  (i32.store8 (i32.const 201) (i32.const 98))
  (i32.store8 (i32.const 202) (i32.const 120))
  (i32.store8 (i32.const 203) (i32.const 121))
  (i32.store8 (i32.const 204) (i32.const 122))
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $a (call $__rt_array_push_str (local.get $a) (i32.const 200) (i64.const 1)))
  (local.set $a (call $__rt_array_push_str (local.get $a) (i32.const 201) (i64.const 1)))
  (local.set $b (call $__rt_array_new (i64.const 4) (i64.const 16)))
  (local.set $b (call $__rt_array_push_str (local.get $b) (i32.const 202) (i64.const 1)))
  (local.set $b (call $__rt_array_push_str (local.get $b) (i32.const 203) (i64.const 1)))
  (local.set $b (call $__rt_array_push_str (local.get $b) (i32.const 204) (i64.const 1)))
  (local.set $u (call $__rt_array_union (local.get $a) (local.get $b)))
  (call $__rt_array_get_str (local.get $u) (i64.const 2))
  (local.set $l)
  (local.set $p)
  (i64.add (i64.add
    (i64.mul (i64.load (local.get $u)) (i64.const 1000000))
    (i64.mul (i64.extend_i32_u (i32.load8_u (local.get $p))) (i64.const 1000)))
    (local.get $l)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "3122001");
        }
    }
}
