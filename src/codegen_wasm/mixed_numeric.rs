//! Purpose:
//! Emits `__rt_mixed_numeric_add/sub/mul`, PHP arithmetic over boxed Mixed operands.
//! Carries PHP's integer-overflow promotion and its numeric-string classification.
//!
//! Called from:
//! - `crate::codegen_wasm::runtime::emit_command_runtime()`, after the mixed and float
//!   runtimes whose `__rt_mixed_unbox`, `__rt_mixed_from_value`, `__rt_str_to_int`, and
//!   `__rt_str_to_f64` helpers it calls.
//!
//! Key details:
//! - Operand classification follows php-src, not the operand's static type: `"7" + 5` is
//!   an integer while `"7.0" + 5` is a double.
//! - A non-numeric operand is a PHP `TypeError`. WebAssembly has no exception machinery
//!   yet, so it is reported as an uncaught fatal and exits 255; catching it needs W2.

use super::wat::WatModule;

/// Byte offsets into the numeric-string classifier's shared scratch region.
///
/// The classifier reports a class plus the parsed value; the value lands here rather
/// than in extra multi-value results so the arithmetic helpers can read whichever of
/// the two representations the class selects.
const CLASS_VALUE_OFFSET: i32 = 10496;

/// Scratch offset where `__rt_str_numeric_class` publishes php-src's `oflow` alongside the parsed
/// value: 0 when the text fits i64, 1 past `i64::MAX`, -1 below `i64::MIN`.
const CLASS_OFLOW_OFFSET: i32 = 10512;

/// `__rt_int_text_overflows`: whether an INTEGRAL numeric string names a value outside i64.
///
/// The obvious round-trip test — parse as i64, convert back to f64, compare with the f64 parse —
/// is BLIND exactly at the boundary: `__rt_str_to_int` saturates `"9223372036854775808"` to
/// `i64::MAX`, and `(f64)i64::MAX` rounds up to 2^63, which is what the text parses to as a float.
/// So the two agree and the overflow goes unnoticed, which made
/// `"9223372036854775807" == "9223372036854775808"` answer true.
///
/// This accumulates the digits instead, checking before each multiply, in UNSIGNED arithmetic so
/// the negative limit (2^63, one past `i64::MAX`) is representable. `$vlen` is the mantissa
/// length the classifier already computed, so the scan never runs past the number.
///
/// Answers php-src's `oflow`: `0` when the text fits, `1` when it is past `i64::MAX`, `-1` when it
/// is below `i64::MIN`. The DIRECTION matters — `zendi_smart_strcmp` uses it to settle a
/// comparison outright rather than risk the accuracy a double conversion would lose.
const RT_INT_TEXT_OVERFLOWS: &str = r#"(func $__rt_int_text_overflows (param $ptr i32) (param $vlen i32) (result i32)
  (local $i i32) (local $c i32) (local $neg i32) (local $acc i64) (local $limit i64) (local $d i64)
  (block $ws (loop $wl                                            ;; PHP's leading whitespace
    (br_if $ws (i32.ge_u (local.get $i) (local.get $vlen)))
    (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
    (br_if $ws (i32.eqz (i32.or (i32.or
      (i32.or (i32.eq (local.get $c) (i32.const 32)) (i32.eq (local.get $c) (i32.const 9)))
      (i32.or (i32.eq (local.get $c) (i32.const 10)) (i32.eq (local.get $c) (i32.const 13))))
      (i32.or (i32.eq (local.get $c) (i32.const 11)) (i32.eq (local.get $c) (i32.const 12))))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $wl)))
  (if (i32.lt_u (local.get $i) (local.get $vlen))
    (then
      (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
      (if (i32.eq (local.get $c) (i32.const 45))                  ;; '-'
        (then (local.set $neg (i32.const 1)) (local.set $i (i32.add (local.get $i) (i32.const 1)))))
      (if (i32.eq (local.get $c) (i32.const 43))                  ;; '+'
        (then (local.set $i (i32.add (local.get $i) (i32.const 1)))))))
  (local.set $limit (i64.const 9223372036854775807))              ;; i64::MAX
  (if (local.get $neg)
    (then (local.set $limit (i64.const -9223372036854775808))))   ;; 2^63 read as unsigned
  (block $done (loop $scan
    (br_if $done (i32.ge_u (local.get $i) (local.get $vlen)))
    (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
    (br_if $done (i32.or (i32.lt_u (local.get $c) (i32.const 48))
                         (i32.gt_u (local.get $c) (i32.const 57))))  ;; mantissa digits only
    (local.set $d (i64.extend_i32_u (i32.sub (local.get $c) (i32.const 48))))
    ;; acc*10 + d would pass the limit?  check before multiplying, unsigned throughout
    (if (i64.gt_u (local.get $acc)
                  (i64.div_u (i64.sub (local.get $limit) (local.get $d)) (i64.const 10)))
      (then (return (select (i32.const -1) (i32.const 1) (local.get $neg)))))
    (local.set $acc (i64.add (i64.mul (local.get $acc) (i64.const 10)) (local.get $d)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $scan)))
  (i32.const 0))
"#;

/// `__rt_str_loose_eq`: PHP 8's `==` between two strings — php-src's `zendi_smart_strcmp`.
///
/// Transcribed from php-src and validated on 3000 pairs against 8.5.6: 1600 from a systematic
/// 40-string matrix and 1400 randomly generated. The naive rule — "both numeric, so compare the
/// numbers" — passes a 625-pair sample and is still WRONG, which is why the sweep was widened.
///
/// Two strings compare numerically only when BOTH are fully numeric; a leading-numeric string like
/// `"10abc"` does not qualify and falls back to bytes. On top of that php-src tracks `oflow`, set
/// only for an INTEGRAL-form string whose magnitude escapes i64, and uses it to settle the
/// comparison WITHOUT converting:
///
/// - both overflowed the same way and agree as doubles -> compare the BYTES, since the double
///   comparison has already lost the accuracy that would separate them;
/// - one side is an integer and the other overflowed -> they cannot be equal, whatever the
///   doubles say;
/// - two equal INFINITIES -> compare the bytes, for the same reason.
///
/// That is what makes `"9223372036854775807" == "9223372036854775808"` false while
/// `"9223372036854775807" == "9.2233720368547758e18"` is TRUE: the second is in float form, so it
/// never sets `oflow` and both sides go through the ordinary double conversion.
///
/// The classifier publishes its parsed value and flag into shared scratch, so the first operand's
/// class, value and flag are copied out before the second call overwrites them.
fn rt_str_loose_eq() -> String {
    format!(
        r#"(func $__rt_str_loose_eq (param $ap i32) (param $al i64) (param $bp i32) (param $bl i64) (result i64)
  (local $ca i32) (local $cb i32) (local $oa i32) (local $ob i32)
  (local $ia i64) (local $ib i64) (local $fa f64) (local $fb f64)
  (local.set $ca (call $__rt_str_numeric_class (local.get $ap) (i32.wrap_i64 (local.get $al))))
  (local.set $oa (i32.load (i32.add (global.get $__float_scratch) (i32.const {oflow_offset}))))
  (if (i32.eq (local.get $ca) (i32.const 1))
    (then (local.set $ia (i64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))))
  (if (i32.eq (local.get $ca) (i32.const 2))
    (then (local.set $fa (f64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))))
  (local.set $cb (call $__rt_str_numeric_class (local.get $bp) (i32.wrap_i64 (local.get $bl))))
  (local.set $ob (i32.load (i32.add (global.get $__float_scratch) (i32.const {oflow_offset}))))
  (if (i32.eq (local.get $cb) (i32.const 1))
    (then (local.set $ib (i64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))))
  (if (i32.eq (local.get $cb) (i32.const 2))
    (then (local.set $fb (f64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))))
  ;; classes 3 and 4 are LEADING-numeric, which php-src does not treat as numeric here
  (if (i32.and (i32.or (i32.eq (local.get $ca) (i32.const 1)) (i32.eq (local.get $ca) (i32.const 2)))
               (i32.or (i32.eq (local.get $cb) (i32.const 1)) (i32.eq (local.get $cb) (i32.const 2))))
    (then
      ;; both overflowed the same way and agree as doubles: the doubles cannot separate them
      (if (i32.and (i32.and (i32.ne (local.get $oa) (i32.const 0))
                            (i32.eq (local.get $oa) (local.get $ob)))
                   (f64.eq (local.get $fa) (local.get $fb)))
        (then (return (i64.extend_i32_u (call $__rt_strict_str_eq
                (local.get $ap) (local.get $al) (local.get $bp) (local.get $bl))))))
      (if (i32.or (i32.eq (local.get $ca) (i32.const 2)) (i32.eq (local.get $cb) (i32.const 2)))
        (then
          (if (i32.ne (local.get $ca) (i32.const 2))
            (then                                            ;; integer on the left, double on the right
              (if (local.get $ob) (then (return (i64.const 0))))   ;; the overflowed side is strictly further out
              (local.set $fa (f64.convert_i64_s (local.get $ia))))
            (else (if (i32.ne (local.get $cb) (i32.const 2))
              (then                                          ;; double on the left, integer on the right
                (if (local.get $oa) (then (return (i64.const 0))))
                (local.set $fb (f64.convert_i64_s (local.get $ib))))
              (else                                          ;; two doubles: equal infinities go to bytes
                (if (i32.and (f64.eq (local.get $fa) (local.get $fb))
                             (f64.eq (f64.abs (local.get $fa)) (f64.const inf)))
                  (then (return (i64.extend_i32_u (call $__rt_strict_str_eq
                          (local.get $ap) (local.get $al) (local.get $bp) (local.get $bl))))))))))
          (return (i64.extend_i32_u (f64.eq (local.get $fa) (local.get $fb))))))
      (return (i64.extend_i32_u (i64.eq (local.get $ia) (local.get $ib))))))
  (i64.extend_i32_u (call $__rt_strict_str_eq                    ;; not both numeric: byte for byte
    (local.get $ap) (local.get $al) (local.get $bp) (local.get $bl))))
"#,
        value_offset = CLASS_VALUE_OFFSET,
        oflow_offset = CLASS_OFLOW_OFFSET
    )
}


/// `__rt_str_smart_cmp`: php-src's ORDERING of two strings — what `sort()` and `<=>` use.
///
/// Two numeric strings compare NUMERICALLY, which is why `sort(["10", "9"])` answers `9, 10`
/// and not `10, 9`; anything else compares byte for byte, normalized to -1/0/1 as PHP 8 does.
///
/// Three escapes, all MEASURED against php-src 8.5.6 rather than assumed, and each one a case
/// where the double values cannot separate the operands:
/// - both texts overflow `i64` the SAME way and agree as doubles -> compare the bytes, so
///   `"18446744073709551616" < "…617"` even though both round to the same double;
/// - one text overflowed and the other is a genuine `i64` -> the overflowed side wins outright,
///   so `"9223372036854775808" > "9223372036854775807"`. Note this does NOT apply against a
///   real float literal: `"9223372036854775808" == "9.223372036854775808e18"`;
/// - two equal INFINITIES -> compare the bytes, so `"1e400" < "1e401"`.
///
/// Validated on php-src's own answers for a 23x23 systematic table and 1500 random pairs.
fn rt_str_smart_cmp() -> String {
    format!(
        r#"(func $__rt_str_smart_cmp (param $ap i32) (param $al i64) (param $bp i32) (param $bl i64) (result i64)
  (local $ca i32) (local $cb i32) (local $oa i32) (local $ob i32)
  (local $ia i64) (local $ib i64) (local $fa f64) (local $fb f64) (local $raw i64)
  (local.set $ca (call $__rt_str_numeric_class (local.get $ap) (i32.wrap_i64 (local.get $al))))
  (local.set $oa (i32.load (i32.add (global.get $__float_scratch) (i32.const {oflow_offset}))))
  (if (i32.eq (local.get $ca) (i32.const 1))
    (then (local.set $ia (i64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))))
  (if (i32.eq (local.get $ca) (i32.const 2))
    (then (local.set $fa (f64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))))
  (local.set $cb (call $__rt_str_numeric_class (local.get $bp) (i32.wrap_i64 (local.get $bl))))
  (local.set $ob (i32.load (i32.add (global.get $__float_scratch) (i32.const {oflow_offset}))))
  (if (i32.eq (local.get $cb) (i32.const 1))
    (then (local.set $ib (i64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))))
  (if (i32.eq (local.get $cb) (i32.const 2))
    (then (local.set $fb (f64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))))
  ;; classes 3 and 4 are LEADING-numeric, which php-src does not treat as numeric here
  (if (i32.and (i32.or (i32.eq (local.get $ca) (i32.const 1)) (i32.eq (local.get $ca) (i32.const 2)))
               (i32.or (i32.eq (local.get $cb) (i32.const 1)) (i32.eq (local.get $cb) (i32.const 2))))
    (then
      (block $bytes
        ;; both overflowed the same way and agree as doubles: the doubles cannot separate them
        (br_if $bytes (i32.and (i32.and (i32.ne (local.get $oa) (i32.const 0))
                                        (i32.eq (local.get $oa) (local.get $ob)))
                               (f64.eq (local.get $fa) (local.get $fb))))
        ;; an overflowed integer text keeps its true magnitude against a genuine i64
        (if (i32.and (i32.ne (local.get $oa) (i32.const 0)) (i32.eq (local.get $cb) (i32.const 1)))
          (then (return (i64.extend_i32_s (local.get $oa)))))
        (if (i32.and (i32.ne (local.get $ob) (i32.const 0)) (i32.eq (local.get $ca) (i32.const 1)))
          (then (return (i64.sub (i64.const 0) (i64.extend_i32_s (local.get $ob))))))
        (if (i32.or (i32.eq (local.get $ca) (i32.const 2)) (i32.eq (local.get $cb) (i32.const 2)))
          (then
            (if (i32.ne (local.get $ca) (i32.const 2))
              (then (local.set $fa (f64.convert_i64_s (local.get $ia)))))
            (if (i32.ne (local.get $cb) (i32.const 2))
              (then (local.set $fb (f64.convert_i64_s (local.get $ib)))))
            ;; equal infinities carry no ordering information: fall through to the bytes
            (br_if $bytes (i32.and (f64.eq (local.get $fa) (local.get $fb))
                                   (f64.eq (f64.abs (local.get $fa)) (f64.const inf))))
            (return (i64.sub
              (i64.extend_i32_u (f64.gt (local.get $fa) (local.get $fb)))
              (i64.extend_i32_u (f64.lt (local.get $fa) (local.get $fb)))))))
        (return (i64.sub
          (i64.extend_i32_u (i64.gt_s (local.get $ia) (local.get $ib)))
          (i64.extend_i32_u (i64.lt_s (local.get $ia) (local.get $ib))))))))
  ;; byte for byte, normalized to -1/0/1 the way PHP 8 reports it
  (local.set $raw (call $__rt_str_cmp
    (local.get $ap) (local.get $al) (local.get $bp) (local.get $bl) (i32.const 0)))
  (i64.sub
    (i64.extend_i32_u (i64.gt_s (local.get $raw) (i64.const 0)))
    (i64.extend_i32_u (i64.lt_s (local.get $raw) (i64.const 0)))))
"#,
        value_offset = CLASS_VALUE_OFFSET,
        oflow_offset = CLASS_OFLOW_OFFSET
    )
}

/// Adds the boxed-Mixed arithmetic runtime to `wm`.
pub(super) fn emit_mixed_numeric_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_INT_TEXT_OVERFLOWS);
    wm.add_raw_func(&rt_str_numeric_class());
    wm.add_raw_func(&rt_str_loose_eq());
    wm.add_raw_func(&rt_str_smart_cmp());
    wm.add_raw_func(&rt_mixed_numeric_operand());
    wm.add_raw_func(RT_MIXED_NUMERIC_COMMON);
    wm.add_raw_func(RT_MIXED_NUMERIC_ADD);
    wm.add_raw_func(RT_MIXED_NUMERIC_SUB);
    wm.add_raw_func(RT_MIXED_NUMERIC_MUL);
}

/// Classifies a PHP string for arithmetic, following php-src's `is_numeric_string_ex`.
///
/// PHP accepts `WS* [+-]? (DIGITS (. DIGITS?)? | . DIGITS) ([eE][+-]? DIGITS)? WS*`. A
/// string matching it entirely is numeric; one matching only a prefix is "leading
/// numeric", which arithmetic accepts while emitting a warning; anything else is not a
/// number at all and arithmetic rejects it. Integer versus float is decided by form, not
/// by magnitude: a string carrying a decimal point or an exponent is always a float, and
/// so is an integral string too large for `i64` (`"9223372036854775808"`).
///
/// Returns 0 for non-numeric, 1 for an integer, 2 for a float, 3 for a leading-numeric
/// integer, and 4 for a leading-numeric float. Classes 1 and 3 leave the parsed `i64` at
/// the scratch offset; classes 2 and 4 leave raw `f64` bits there.
fn rt_str_numeric_class() -> String {
    format!(
        r#"(func $__rt_str_numeric_class (param $ptr i32) (param $len i32) (result i32)
  (local $i i32) (local $c i32) (local $digits i32) (local $isfloat i32)
  (local $end i32) (local $numend i32) (local $vlen i32) (local $iv i64)
  (local.set $i (i32.const 0))                                    ;; cursor
  (block $ws                                                      ;; skip PHP's leading whitespace
    (loop $wl
      (br_if $ws (i32.ge_u (local.get $i) (local.get $len)))      ;; end of string
      (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))  ;; current byte
      (br_if $ws (i32.eqz (i32.or (i32.or (i32.or
        (i32.eq (local.get $c) (i32.const 32))                    ;; space
        (i32.eq (local.get $c) (i32.const 9)))                    ;; tab
        (i32.or (i32.eq (local.get $c) (i32.const 10))            ;; newline
                (i32.eq (local.get $c) (i32.const 13))))          ;; carriage return
        (i32.or (i32.eq (local.get $c) (i32.const 11))            ;; vertical tab
                (i32.eq (local.get $c) (i32.const 12))))))        ;; form feed
      (local.set $i (i32.add (local.get $i) (i32.const 1)))       ;; consume whitespace
      (br $wl)))
  (if (i32.lt_u (local.get $i) (local.get $len))                  ;; optional sign
    (then
      (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))  ;; sign candidate
      (if (i32.or (i32.eq (local.get $c) (i32.const 43))          ;; '+'
                  (i32.eq (local.get $c) (i32.const 45)))         ;; '-'
        (then (local.set $i (i32.add (local.get $i) (i32.const 1)))))))       ;; consume sign
  (local.set $digits (i32.const 0))                               ;; integral digit count
  (block $id                                                      ;; integral digits
    (loop $il
      (br_if $id (i32.ge_u (local.get $i) (local.get $len)))      ;; end of string
      (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))  ;; current byte
      (br_if $id (i32.or (i32.lt_u (local.get $c) (i32.const 48)) ;; not a digit
                         (i32.gt_u (local.get $c) (i32.const 57))))
      (local.set $digits (i32.add (local.get $digits) (i32.const 1)))         ;; count it
      (local.set $i (i32.add (local.get $i) (i32.const 1)))       ;; consume digit
      (br $il)))
  (local.set $isfloat (i32.const 0))                              ;; integer until proven otherwise
  (i32.store (i32.add (global.get $__float_scratch) (i32.const {oflow_offset})) (i32.const 0))  ;; only an INTEGRAL form can overflow
  (if (i32.lt_u (local.get $i) (local.get $len))                  ;; optional fraction
    (then
      (if (i32.eq (i32.load8_u (i32.add (local.get $ptr) (local.get $i))) (i32.const 46))  ;; '.'
        (then
          (local.set $isfloat (i32.const 1))                      ;; a decimal point forces float
          (local.set $i (i32.add (local.get $i) (i32.const 1)))   ;; consume '.'
          (block $fd                                              ;; fractional digits
            (loop $fl
              (br_if $fd (i32.ge_u (local.get $i) (local.get $len)))          ;; end of string
              (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))  ;; current byte
              (br_if $fd (i32.or (i32.lt_u (local.get $c) (i32.const 48))     ;; not a digit
                                 (i32.gt_u (local.get $c) (i32.const 57))))
              (local.set $digits (i32.add (local.get $digits) (i32.const 1))) ;; count it
              (local.set $i (i32.add (local.get $i) (i32.const 1)))           ;; consume digit
              (br $fl)))))))
  (if (i32.eqz (local.get $digits))                               ;; no mantissa digit at all
    (then (return (i32.const 0))))                                ;; not a number
  (local.set $numend (local.get $i))                              ;; end of the mantissa
  (if (i32.lt_u (local.get $i) (local.get $len))                  ;; optional exponent
    (then
      (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))  ;; 'e' candidate
      (if (i32.or (i32.eq (local.get $c) (i32.const 101))         ;; 'e'
                  (i32.eq (local.get $c) (i32.const 69)))         ;; 'E'
        (then
          (local.set $end (i32.add (local.get $i) (i32.const 1))) ;; provisional cursor past 'e'
          (if (i32.lt_u (local.get $end) (local.get $len))        ;; optional exponent sign
            (then
              (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $end))))  ;; sign candidate
              (if (i32.or (i32.eq (local.get $c) (i32.const 43))  ;; '+'
                          (i32.eq (local.get $c) (i32.const 45))) ;; '-'
                (then (local.set $end (i32.add (local.get $end) (i32.const 1)))))))     ;; consume sign
          (local.set $c (i32.const 0))                            ;; exponent digit count
          (block $ed                                              ;; exponent digits
            (loop $el
              (br_if $ed (i32.ge_u (local.get $end) (local.get $len)))        ;; end of string
              (br_if $ed (i32.or
                (i32.lt_u (i32.load8_u (i32.add (local.get $ptr) (local.get $end))) (i32.const 48))
                (i32.gt_u (i32.load8_u (i32.add (local.get $ptr) (local.get $end))) (i32.const 57))))  ;; not a digit
              (local.set $c (i32.add (local.get $c) (i32.const 1)))           ;; count it
              (local.set $end (i32.add (local.get $end) (i32.const 1)))       ;; consume digit
              (br $el)))
          ;; an exponent needs at least one digit, otherwise 'e' is trailing garbage
          (if (local.get $c)
            (then
              (local.set $isfloat (i32.const 1))                  ;; an exponent forces float
              (local.set $numend (local.get $end))                ;; mantissa+exponent consumed
              (local.set $i (local.get $end))))))))               ;; advance past the exponent
  (local.set $vlen (local.get $numend))                           ;; bytes the number occupies
  (block $tw                                                      ;; skip PHP's trailing whitespace
    (loop $tl
      (br_if $tw (i32.ge_u (local.get $i) (local.get $len)))      ;; end of string
      (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))  ;; current byte
      (br_if $tw (i32.eqz (i32.or (i32.or (i32.or
        (i32.eq (local.get $c) (i32.const 32))                    ;; space
        (i32.eq (local.get $c) (i32.const 9)))                    ;; tab
        (i32.or (i32.eq (local.get $c) (i32.const 10))            ;; newline
                (i32.eq (local.get $c) (i32.const 13))))          ;; carriage return
        (i32.or (i32.eq (local.get $c) (i32.const 11))            ;; vertical tab
                (i32.eq (local.get $c) (i32.const 12))))))        ;; form feed
      (local.set $i (i32.add (local.get $i) (i32.const 1)))       ;; consume whitespace
      (br $tl)))
  (if (i32.eqz (local.get $isfloat))                              ;; integral form: does it fit i64?
    (then
      (local.set $iv (call $__rt_str_to_int (local.get $ptr) (local.get $vlen) (global.get $__float_scratch)))  ;; parse the integral text
      (i64.store (i32.add (global.get $__float_scratch) (i32.const {value_offset})) (local.get $iv))            ;; publish the i64
      (i32.store (i32.add (global.get $__float_scratch) (i32.const {oflow_offset}))
                 (call $__rt_int_text_overflows (local.get $ptr) (local.get $vlen)))  ;; publish php-src's oflow
      (if (i32.load (i32.add (global.get $__float_scratch) (i32.const {oflow_offset})))
        (then (local.set $isfloat (i32.const 1))))))              ;; magnitude exceeds i64: PHP calls it a float
  (if (local.get $isfloat)                                        ;; float form: publish raw f64 bits
    (then
      (call $__rt_str_to_f64 (local.get $ptr) (local.get $vlen) (i32.add (global.get $__float_scratch) (i32.const {value_offset})) (global.get $__float_scratch))))  ;; parse into the value slot
  (if (i32.eq (local.get $i) (local.get $len))                    ;; the whole string was consumed
    (then (return (i32.add (i32.const 1) (local.get $isfloat))))) ;; 1 = int, 2 = float
  (i32.add (i32.const 3) (local.get $isfloat)))                   ;; 3 = leading int, 4 = leading float
"#,
        value_offset = CLASS_VALUE_OFFSET,
        oflow_offset = CLASS_OFLOW_OFFSET
    )
}

/// Builds `__rt_mixed_numeric_operand`, which reduces one boxed operand to a number.
///
/// Returns `(is_float, value)` where `value` carries either the `i64` or raw `f64` bits.
/// Integers and booleans are integers, floats are floats, and null is integer zero —
/// matching PHP, where `null + 1` is `1`. A string is classified: a leading-numeric one
/// contributes its numeric prefix after warning, and a wholly non-numeric one is a
/// `TypeError` that exits 255 because catching it needs exception support this target
/// does not have. Arrays, objects, and resources never reach here: the capability audit
/// rejects them before emission.
fn rt_mixed_numeric_operand() -> String {
    format!(
        r#"(func $__rt_mixed_numeric_operand (param $ptr i32) (result i32) (result i64)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $class i32)
  (call $__rt_mixed_unbox (local.get $ptr))                       ;; unbox -> stack: tag, lo, hi
  (local.set $hi)                                                 ;; pop value high word
  (local.set $lo)                                                 ;; pop value low word
  (local.set $tag)                                                ;; pop runtime tag
  (if (i64.eqz (local.get $tag))                                  ;; tag 0 = int
    (then (return (i32.const 0) (local.get $lo))))                ;; integer operand
  (if (i64.eq (local.get $tag) (i64.const 3))                     ;; tag 3 = bool
    (then (return (i32.const 0) (local.get $lo))))                ;; PHP treats a bool as 0/1
  (if (i64.eq (local.get $tag) (i64.const 8))                     ;; tag 8 = null
    (then (return (i32.const 0) (i64.const 0))))                  ;; PHP treats null as 0
  (if (i64.eq (local.get $tag) (i64.const 2))                     ;; tag 2 = float
    (then (return (i32.const 1) (local.get $lo))))                ;; forward the stored f64 bits
  (if (i64.eq (local.get $tag) (i64.const 1))                     ;; tag 1 = string
    (then
      (local.set $class (call $__rt_str_numeric_class (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi))))  ;; classify the text
      (if (i32.eqz (local.get $class))                            ;; no numeric prefix at all
        (then (call $__rt_fatal_unsupported_operand)))            ;; PHP TypeError, uncatchable here
      (if (i32.gt_u (local.get $class) (i32.const 2))             ;; leading numeric: value plus a warning
        (then (call $__rt_warn_non_numeric_value)))               ;; "A non-numeric value encountered"
      (return
        (i32.eqz (i32.and (local.get $class) (i32.const 1)))      ;; even classes (2 and 4) are the floats
        (i64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset})))))) ;; parsed value
  (call $__rt_fatal_unsupported_operand)                          ;; any other tag is not a number
  (i32.const 0) (i64.const 0))                                    ;; unreachable, keeps the signature
"#,
        value_offset = CLASS_VALUE_OFFSET
    )
}

/// Shared body of the three arithmetic helpers, selected by `$op` (0 add, 1 sub, 2 mul).
///
/// Both operands are reduced first, so a `TypeError` or warning fires in PHP's own
/// left-to-right order. When both are integers the operation runs in `i64` and its
/// overflow is detected exactly, promoting to a double the way php-src does; otherwise
/// both widen to `f64`. The result is boxed so the caller observes either tag.
const RT_MIXED_NUMERIC_COMMON: &str =
    r#"(func $__rt_mixed_numeric_common (param $l i32) (param $r i32) (param $op i32) (result i32)
  (local $lf i32) (local $lv i64) (local $rf i32) (local $rv i64)
  (local $res i64) (local $ovf i32) (local $x f64) (local $y f64)
  (call $__rt_mixed_numeric_operand (local.get $l))               ;; reduce the left operand first
  (local.set $lv)                                                 ;; pop its value
  (local.set $lf)                                                 ;; pop its float flag
  (call $__rt_mixed_numeric_operand (local.get $r))               ;; then the right operand
  (local.set $rv)                                                 ;; pop its value
  (local.set $rf)                                                 ;; pop its float flag
  (if (i32.eqz (i32.or (local.get $lf) (local.get $rf)))          ;; both integers: try i64 arithmetic
    (then
      (if (i32.eqz (local.get $op))                               ;; add
        (then
          (local.set $res (i64.add (local.get $lv) (local.get $rv)))                    ;; wrapped sum
          (local.set $ovf (i64.lt_s
            (i64.and (i64.xor (local.get $lv) (local.get $res))
                     (i64.xor (local.get $rv) (local.get $res)))
            (i64.const 0)))))                                     ;; signed-add overflow
      (if (i32.eq (local.get $op) (i32.const 1))                  ;; sub
        (then
          (local.set $res (i64.sub (local.get $lv) (local.get $rv)))                    ;; wrapped difference
          (local.set $ovf (i64.lt_s
            (i64.and (i64.xor (local.get $lv) (local.get $rv))
                     (i64.xor (local.get $lv) (local.get $res)))
            (i64.const 0)))))                                     ;; signed-sub overflow
      (if (i32.eq (local.get $op) (i32.const 2))                  ;; mul
        (then
          (local.set $res (i64.mul (local.get $lv) (local.get $rv)))                    ;; wrapped product
          (local.set $ovf (i32.const 0))                          ;; assume it fits
          (if (i64.ne (local.get $lv) (i64.const 0))              ;; a zero factor never overflows
            (then
              (if (i32.or
                    (i64.ne (i64.div_s (local.get $res) (local.get $lv)) (local.get $rv))  ;; division disagrees
                    (i32.and (i64.eq (local.get $lv) (i64.const -1))
                             (i64.eq (local.get $rv) (i64.const -9223372036854775808))))   ;; -1 * INT_MIN
                (then (local.set $ovf (i32.const 1))))))))        ;; product does not fit i64
      (if (i32.eqz (local.get $ovf))                              ;; it fits: an integer result
        (then (return (call $__rt_mixed_from_value (i64.const 0) (local.get $res) (i64.const 0)))))))
  (local.set $x (if (result f64) (local.get $lf)                  ;; widen the left operand
    (then (f64.reinterpret_i64 (local.get $lv)))
    (else (f64.convert_i64_s (local.get $lv)))))
  (local.set $y (if (result f64) (local.get $rf)                  ;; widen the right operand
    (then (f64.reinterpret_i64 (local.get $rv)))
    (else (f64.convert_i64_s (local.get $rv)))))
  (if (i32.eqz (local.get $op))                                   ;; add
    (then (local.set $x (f64.add (local.get $x) (local.get $y)))))
  (if (i32.eq (local.get $op) (i32.const 1))                      ;; sub
    (then (local.set $x (f64.sub (local.get $x) (local.get $y)))))
  (if (i32.eq (local.get $op) (i32.const 2))                      ;; mul
    (then (local.set $x (f64.mul (local.get $x) (local.get $y)))))
  (call $__rt_mixed_from_value (i64.const 2) (i64.reinterpret_f64 (local.get $x)) (i64.const 0)))
"#;

/// PHP `+` over boxed Mixed operands.
const RT_MIXED_NUMERIC_ADD: &str =
    r#"(func $__rt_mixed_numeric_add (param $l i32) (param $r i32) (result i32)
  (call $__rt_mixed_numeric_common (local.get $l) (local.get $r) (i32.const 0)))  ;; op 0 = add
"#;

/// PHP `-` over boxed Mixed operands.
const RT_MIXED_NUMERIC_SUB: &str =
    r#"(func $__rt_mixed_numeric_sub (param $l i32) (param $r i32) (result i32)
  (call $__rt_mixed_numeric_common (local.get $l) (local.get $r) (i32.const 1)))  ;; op 1 = sub
"#;

/// PHP `*` over boxed Mixed operands.
const RT_MIXED_NUMERIC_MUL: &str =
    r#"(func $__rt_mixed_numeric_mul (param $l i32) (param $r i32) (result i32)
  (call $__rt_mixed_numeric_common (local.get $l) (local.get $r) (i32.const 2)))  ;; op 2 = mul
"#;
