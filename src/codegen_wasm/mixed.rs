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
    wm.add_raw_func(RT_MIXED_CAST_INT);
    wm.add_raw_func(RT_MIXED_CAST_FLOAT);
    wm.add_raw_func(RT_MIXED_CAST_STRING);
    wm.add_raw_func(RT_MIXED_CAST_STRING_REF);
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

/// `__rt_mixed_cast_int`: casts a boxed Mixed cell to a PHP int, mirroring the
/// native `__rt_mixed_cast_int` tag dispatch. Unboxes via `__rt_mixed_unbox` (which
/// unwraps nested tag-7 cells and maps null to tag 8), then per tag: an int forwards its
/// payload; a string casts through `__rt_str_to_int` (PHP string->int: saturating
/// integer-form, float-form finite truncates toward zero, +/-INF or NaN -> 0); a float
/// applies the same rule as the PHP string path (+/-INF/NaN -> 0, finite ->
/// `i64.trunc_sat_f64_s`), which is more PHP-correct than the native `fcvtzs` (PHP gives
/// `(int)(float)INF = 0`); a bool forwards its 0/1; an array/hash returns its element
/// count (or 0 for a null container); a resource returns its 1-based display id
/// (payload + 1); null/object/callable/other return 0. Borrows the cell (never frees).
const RT_MIXED_CAST_INT: &str = r#"(func $__rt_mixed_cast_int (param $ptr i32) (result i64) (local $tag i64) (local $lo i64) (local $hi i64) (local $cp i32) ;; cast a boxed Mixed cell to a PHP int, mirroring native tag dispatch
  (call $__rt_mixed_unbox (local.get $ptr))                             ;; unbox -> stack: tag, lo, hi
  (local.set $hi)                                                       ;; pop value high word
  (local.set $lo)                                                       ;; pop value low word
  (local.set $tag)                                                      ;; pop runtime tag
  (if (i64.eqz (local.get $tag))                                        ;; tag 0 = int
    (then (return (local.get $lo))))                                    ;; forward the stored integer payload
  (if (i64.eq (local.get $tag) (i64.const 1))                           ;; tag 1 = string
    (then (return (call $__rt_str_to_int (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi)) (global.get $__float_scratch))))) ;; PHP string -> int (saturate, INF/NaN -> 0)
  (if (i64.eq (local.get $tag) (i64.const 2))                           ;; tag 2 = float
    (then                                                               ;; PHP (int)float: ±INF/NaN -> 0, finite -> truncate toward zero
      (if (i32.eq (i32.and (i32.wrap_i64 (i64.shr_u (local.get $lo) (i64.const 52))) (i32.const 2047)) (i32.const 2047)) ;; exponent field all ones?
        (then (return (i64.const 0)))                                   ;; INF or NaN -> 0 (PHP (int)INF/NAN = 0, unlike saturating trunc)
        (else (return (i64.trunc_sat_f64_s (f64.reinterpret_i64 (local.get $lo)))))))) ;; finite -> truncate toward zero, saturate out-of-range
  (if (i64.eq (local.get $tag) (i64.const 3))                           ;; tag 3 = bool
    (then (return (local.get $lo))))                                    ;; already normalized 0/1
  (if (i32.or (i64.eq (local.get $tag) (i64.const 4)) (i64.eq (local.get $tag) (i64.const 5))) ;; tag 4/5 = array/hash
    (then
      (local.set $cp (i32.wrap_i64 (local.get $lo)))                    ;; container pointer
      (if (i32.eqz (local.get $cp))                                     ;; null container pointer?
        (then (return (i64.const 0)))                                   ;; null -> 0 (matches an empty container)
        (else (return (i64.load (local.get $cp)))))))                   ;; element count from the container header [cp+0]
  (if (i64.eq (local.get $tag) (i64.const 9))                           ;; tag 9 = resource
    (then (return (i64.add (local.get $lo) (i64.const 1)))))            ;; 1-based display id (payload + 1)
  (i64.const 0))                                                        ;; null(8)/object(6)/callable(10)/other -> 0
"#;

/// `__rt_mixed_cast_float`: casts a boxed Mixed cell to a PHP float, returning the raw
/// f64 bits (the wasm backend keeps floats as bits across helper boundaries). Unboxes via
/// `__rt_mixed_unbox`, then per tag: an int or bool widens to f64 via
/// `f64.convert_i64_s`; a string parses through `__rt_str_to_f64` (PHP string->float); a
/// float forwards its stored bits; arrays/hashes/objects/resources/null/other return 0.0.
/// Borrows the cell (never frees).
const RT_MIXED_CAST_FLOAT: &str = r#"(func $__rt_mixed_cast_float (param $ptr i32) (result i64) (local $tag i64) (local $lo i64) (local $hi i64) ;; cast a boxed Mixed cell to a PHP float, returning raw f64 bits
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
  (i64.const 0))                                                        ;; array/hash/object/resource/null/other -> 0.0
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
const RT_MIXED_CAST_STRING: &str = r#"(func $__rt_mixed_cast_string (param $ptr i32) (result i32) (result i32) (local $tag i64) (local $lo i64) (local $hi i64) (local $iptr i32) (local $ilen i32) (local $pptr i32) (local $plen i64) ;; cast a boxed Mixed cell to a PHP string (ptr,len), always persisting so callers own the result
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
      (call $__rt_ftoa (local.get $lo) (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80) (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 768) (i32.add (global.get $__float_scratch) (i32.const 4096))) ;; format into scratch+4096, returns (ptr,len)
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
  (i32.const 0) (i32.const 0))                                          ;; array/hash/object/resource/null/other -> empty string
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
      (call $__rt_ftoa (local.get $lo) (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80) (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 768) (i32.add (global.get $__float_scratch) (i32.const 4096))) ;; format into scratch+4096, returns (ptr,len)
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
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
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

    /// `__rt_mixed_cast_int` on float cells: finite in-range truncates toward zero,
    /// +/-INF and NaN -> 0 (PHP `(int)(float)INF = 0`, unlike the native saturating
    /// `fcvtzs` which yields INT64_MAX). Out-of-range finite saturates to INT64_MAX,
    /// matching the native ARM64 `fcvtzs` and the string-cast saturating path.
    #[test]
    fn cast_int_float_php_parity() {
        // (f64 value, expected i64 decimal)
        let cases: &[(f64, &str)] = &[
            (1.9, "1"),
            (-1.9, "-1"),
            (1e18, "1000000000000000000"),
            (1e20, "9223372036854775807"),
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
        // null cell (tag 8) -> ""
        if let Some(o) = run_driver(&cast_string_scalar_driver(8, 0, 0), "t") {
            assert_eq!(o, str_hash("").to_string());
        }
        // object cell (tag 6) -> ""
        if let Some(o) = run_driver(&cast_string_scalar_driver(6, 0, 0), "t") {
            assert_eq!(o, str_hash("").to_string());
        }
    }
}
