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
pub(super) const CLASS_VALUE_OFFSET: i32 = 10496;

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
    wm.add_raw_func(RT_THREEWAY);
    wm.add_raw_func(RT_MIXED_TRUTHY_PARTS);
    wm.add_raw_func(&rt_mixed_inc_dec());
    wm.add_raw_func(&rt_str_inc_dec_alpha());
    wm.add_raw_func(&rt_mixed_cmp_mixed());
    wm.add_raw_func(&rt_mixed_cmp_i64());
    wm.add_raw_func(RT_THREEWAY_PREDICATE);
    wm.add_raw_func(RT_MIXED_REL_UNORDERED);
    wm.add_raw_func(RT_MIXED_REL_MIXED);
    wm.add_raw_func(RT_MIXED_REL_I64);
}

/// `__rt_threeway_predicate`: turns php's -1/0/1 ordering into one relational answer.
///
/// The predicate word is the EIR `CmpPredicate` reduced to its four signed relational forms:
/// 0 = `<`, 1 = `<=`, 2 = `>`, 3 = `>=`. Nothing else reaches here — `==`, `!=` and `<=>` have
/// their own opcodes and never lower through `php_rel_cmp`.
const RT_THREEWAY_PREDICATE: &str =
    r#"(func $__rt_threeway_predicate (param $c i64) (param $pred i32) (result i64)
  (if (i32.eqz (local.get $pred))
    (then (return (i64.extend_i32_u (i64.lt_s (local.get $c) (i64.const 0))))))
  (if (i32.eq (local.get $pred) (i32.const 1))
    (then (return (i64.extend_i32_u (i64.le_s (local.get $c) (i64.const 0))))))
  (if (i32.eq (local.get $pred) (i32.const 2))
    (then (return (i64.extend_i32_u (i64.gt_s (local.get $c) (i64.const 0))))))
  (i64.extend_i32_u (i64.ge_s (local.get $c) (i64.const 0))))
"#;

/// `__rt_mixed_rel_unordered`: whether php's ordering of this pair is IEEE-unordered.
///
/// This is the one place relational comparison parts company with `<=>`. php-src answers 1 for
/// `NAN <=> $x` in both directions, but every relational operator on that pair is FALSE —
/// `NAN < 1`, `NAN > 1`, `NAN <= 1` and `NAN >= 1` alike. So the ordering answer cannot be
/// tested against zero without first asking whether it came from a NaN.
///
/// The test has to reproduce `__rt_mixed_cmp_mixed`'s rule ORDER, not just look for a NaN: a
/// bool or null on either side makes BOTH sides booleans BEFORE the NaN rule is reached, and
/// NaN is truthy, so `NAN < true` is an ordinary bool comparison and is not unordered. A tag of
/// 8 (null) is caught by the same clause, which also subsumes the null-against-string rule that
/// sits ahead of it.
const RT_MIXED_REL_UNORDERED: &str =
    r#"(func $__rt_mixed_rel_unordered (param $a i32) (param $b i32) (result i32)
  (local $ta i64) (local $la i64) (local $ha i64)
  (local $tb i64) (local $lb i64) (local $hb i64)
  (call $__rt_mixed_unbox (local.get $a))
  (local.set $ha)
  (local.set $la)
  (local.set $ta)
  (call $__rt_mixed_unbox (local.get $b))
  (local.set $hb)
  (local.set $lb)
  (local.set $tb)
  (if (i32.or
        (i32.or (i64.eq (local.get $ta) (i64.const 3)) (i64.eq (local.get $ta) (i64.const 8)))
        (i32.or (i64.eq (local.get $tb) (i64.const 3)) (i64.eq (local.get $tb) (i64.const 8))))
    (then (return (i32.const 0))))                                ;; both became booleans first
  (if (i32.and (i64.eq (local.get $ta) (i64.const 2))
        (f64.ne (f64.reinterpret_i64 (local.get $la)) (f64.reinterpret_i64 (local.get $la))))
    (then (return (i32.const 1))))
  (i32.and (i64.eq (local.get $tb) (i64.const 2))
    (f64.ne (f64.reinterpret_i64 (local.get $lb)) (f64.reinterpret_i64 (local.get $lb)))))
"#;

/// `__rt_mixed_rel_mixed`: one relational operator over two boxed cells.
const RT_MIXED_REL_MIXED: &str =
    r#"(func $__rt_mixed_rel_mixed (param $a i32) (param $b i32) (param $pred i32) (result i64)
  (if (call $__rt_mixed_rel_unordered (local.get $a) (local.get $b))
    (then (return (i64.const 0))))                                ;; every NaN relation is false
  (call $__rt_threeway_predicate
    (call $__rt_mixed_cmp_mixed (local.get $a) (local.get $b)) (local.get $pred)))
"#;

/// `__rt_mixed_rel_i64`: one relational operator between a boxed cell and a plain integer.
///
/// A long is never a bool, a null or a NaN, so the unordered question reduces to the cell: the
/// pair is unordered exactly when the cell holds a NaN float. The `pred` the caller passes has
/// already been mirrored if the integer was the LEFT operand, since `cmp(int, cell)` is
/// `-cmp(cell, int)` and mirroring the predicate is the same answer without the negation.
const RT_MIXED_REL_I64: &str =
    r#"(func $__rt_mixed_rel_i64 (param $cell i32) (param $r i64) (param $pred i32) (result i64)
  (local $tag i64) (local $lo i64) (local $hi i64)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i32.and (i64.eq (local.get $tag) (i64.const 2))
        (f64.ne (f64.reinterpret_i64 (local.get $lo)) (f64.reinterpret_i64 (local.get $lo))))
    (then (return (i64.const 0))))                                ;; every NaN relation is false
  (call $__rt_threeway_predicate
    (call $__rt_mixed_cmp_i64 (local.get $cell) (local.get $r)) (local.get $pred)))
"#;

/// Two sign helpers matching php-src's `ZEND_THREEWAY_COMPARE`.
///
/// The float one is not `f64.lt`/`f64.gt` folded together by accident: a NaN on EITHER side
/// answers **1**, which is php-src's own result and not what an ordered comparison would give.
const RT_THREEWAY: &str = r#"(func $__rt_i64_threeway (param $a i64) (param $b i64) (result i64)
  (i64.sub
    (i64.extend_i32_u (i64.gt_s (local.get $a) (local.get $b)))
    (i64.extend_i32_u (i64.lt_s (local.get $a) (local.get $b)))))
(func $__rt_f64_threeway (param $a f64) (param $b f64) (result i64)
  (if (i32.or (f64.ne (local.get $a) (local.get $a)) (f64.ne (local.get $b) (local.get $b)))
    (then (return (i64.const 1))))                                ;; php-src answers 1 for a NaN
  (i64.sub
    (i64.extend_i32_u (f64.gt (local.get $a) (local.get $b)))
    (i64.extend_i32_u (f64.lt (local.get $a) (local.get $b)))))
"#;

/// `__rt_mixed_cmp_i64`: php-src's `zend_compare(mixed, long)`, answering -1, 0 or 1.
///
/// This is NOT "cast the box to an int, then compare" — the two disagree on values PHP programs
/// actually hold. Measured on php-src 8.5.6 and validated on 1200 random pairs including the
/// i64 boundary: `"abc" <= 1` is FALSE where the cast answers true, because PHP renders the
/// LONG as a string and compares bytes when the string is not numeric. A boolean or null makes
/// BOTH sides booleans; an array outranks any scalar; and a NaN answers 1 whichever way it is
/// compared.
fn rt_mixed_cmp_i64() -> String {
    format!(
        r#"(func $__rt_mixed_cmp_i64 (param $cell i32) (param $r i64) (result i64)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $cls i32)
  (local $sp i32) (local $sl i32) (local $bp i32) (local $bl i32) (local $lb i64)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eqz (local.get $tag))                                  ;; tag 0 = int: longs compare AS LONGS
    (then (return (call $__rt_i64_threeway (local.get $lo) (local.get $r)))))
  (if (i32.or (i64.eq (local.get $tag) (i64.const 3)) (i64.eq (local.get $tag) (i64.const 8)))
    (then                                                         ;; bool or null: BOTH sides become booleans
      (local.set $lb (i64.const 0))
      (if (i64.eq (local.get $tag) (i64.const 3))
        (then (local.set $lb (i64.extend_i32_u (i64.ne (local.get $lo) (i64.const 0))))))
      (return (call $__rt_i64_threeway (local.get $lb)
        (i64.extend_i32_u (i64.ne (local.get $r) (i64.const 0)))))))
  (if (i64.eq (local.get $tag) (i64.const 2))                     ;; tag 2 = float
    (then (return (call $__rt_f64_threeway
      (f64.reinterpret_i64 (local.get $lo)) (f64.convert_i64_s (local.get $r))))))
  (if (i64.eq (local.get $tag) (i64.const 1))                     ;; tag 1 = string
    (then
      (local.set $sp (i32.wrap_i64 (local.get $lo)))
      (local.set $sl (i32.wrap_i64 (local.get $hi)))
      (local.set $cls (call $__rt_str_numeric_class (local.get $sp) (local.get $sl)))
      (if (i32.eq (local.get $cls) (i32.const 1))                 ;; wholly integral and inside i64
        (then (return (call $__rt_i64_threeway
          (i64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset})))
          (local.get $r)))))
      (if (i32.eq (local.get $cls) (i32.const 2))                 ;; wholly float-shaped
        (then (return (call $__rt_f64_threeway
          (f64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset})))
          (f64.convert_i64_s (local.get $r))))))
      (call $__rt_itoa (local.get $r) (i32.add (global.get $__float_scratch) (i32.const 9216)))
      (local.set $bl)                                             ;; itoa returns (ptr, len)
      (local.set $bp)
      (return (call $__rt_i64_threeway                            ;; normalize the raw byte distance
        (call $__rt_str_cmp (local.get $sp) (i64.extend_i32_u (local.get $sl))
          (local.get $bp) (i64.extend_i32_u (local.get $bl)) (i32.const 0))
        (i64.const 0)))))
  (if (i32.or (i64.eq (local.get $tag) (i64.const 4)) (i64.eq (local.get $tag) (i64.const 5)))
    (then (return (i64.const 1))))                                ;; an array outranks any scalar
  (call $__rt_fail (i32.const 9))                                 ;; object/resource/callable: not modelled
  unreachable)                                                    ;; elephc-trap:post-noreturn:mixed-compare
"#,
        value_offset = CLASS_VALUE_OFFSET
    )
}

/// `__rt_mixed_truthy_parts`: PHP truthiness from an already-unboxed `(tag, lo, hi)`.
///
/// Separate from `__rt_mixed_truthy`, which takes a cell and WARNS on a NaN: a comparison
/// converts silently, and NaN is truthy — `f64.ne(v, 0)` is already 1 for it, which is the
/// behaviour `NAN <=> true` being 0 depends on.
const RT_MIXED_TRUTHY_PARTS: &str = r#"(func $__rt_mixed_truthy_parts (param $tag i64) (param $lo i64) (param $hi i64) (result i64)
  (if (i64.eq (local.get $tag) (i64.const 8))                     ;; null
    (then (return (i64.const 0))))
  (if (i64.eq (local.get $tag) (i64.const 2))                     ;; float: NaN is TRUTHY
    (then (return (i64.extend_i32_u
      (f64.ne (f64.reinterpret_i64 (local.get $lo)) (f64.const 0))))))
  (if (i64.eq (local.get $tag) (i64.const 1))                     ;; string: "" and "0" are false
    (then
      (if (i64.eqz (local.get $hi))
        (then (return (i64.const 0))))
      (if (i32.and (i64.eq (local.get $hi) (i64.const 1))
            (i32.eq (i32.load8_u (i32.wrap_i64 (local.get $lo))) (i32.const 48)))
        (then (return (i64.const 0))))
      (return (i64.const 1))))
  (if (i32.or (i64.eq (local.get $tag) (i64.const 4)) (i64.eq (local.get $tag) (i64.const 5)))
    (then (return (i64.extend_i32_u                               ;; empty container is false
      (i64.ne (i64.load (i32.wrap_i64 (local.get $lo))) (i64.const 0))))))
  (i64.extend_i32_u (i64.ne (local.get $lo) (i64.const 0))))      ;; int and bool
"#;

/// `__rt_mixed_inc_dec`: PHP's `++`/`--` on a BORROWED boxed operand, answering an OWNED cell.
///
/// The whole case matrix was measured on php-src 8.5.6 (`cli/incdec_oracle.php`) rather than
/// assumed, because three of its rules are not guessable:
///
///   * `null++` is int 1, but `null--` WARNS and stays null — the two directions differ;
///   * a bool keeps its value in both directions, warning each time;
///   * the empty string is its own case in each direction: `""++` is the STRING "1" while
///     `""--` is the INT -1, with two different deprecations.
///
/// Ints, floats and FULLY-numeric strings delegate to `__rt_mixed_numeric_add` with the delta
/// boxed, which is where integer overflow already promotes to float exactly as php does. A
/// LEADING-numeric string ("10abc") must NOT take that path — arithmetic would warn and use
/// the prefix where `++` perl-increments the text — so the classifier separates them first.
///
/// An array or object operand is php-src's `TypeError: Unsupported operand types`, reported
/// through the same failure exit every arithmetic helper uses.
fn rt_mixed_inc_dec() -> String {
    r#"(func $__rt_mixed_inc_dec (param $cell i32) (param $delta i64) (result i32)
  (local $tag i64) (local $lo i64) (local $hi i64)
  (local $cls i32) (local $dbox i32) (local $res i32)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 8))                     ;; null: the directions differ
    (then
      (if (i64.gt_s (local.get $delta) (i64.const 0))
        (then (return (call $__rt_mixed_from_value (i64.const 0) (i64.const 1) (i64.const 0)))))
      (call $__rt_warn_dec_null)
      (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
  (if (i64.eq (local.get $tag) (i64.const 3))                     ;; bool: warn, keep the value
    (then
      (if (i64.gt_s (local.get $delta) (i64.const 0))
        (then (call $__rt_warn_inc_bool))
        (else (call $__rt_warn_dec_bool)))
      (return (call $__rt_mixed_from_value (i64.const 3) (local.get $lo) (i64.const 0)))))
  (if (i64.eq (local.get $tag) (i64.const 1))                     ;; string: classify FIRST
    (then
      (local.set $cls (call $__rt_str_numeric_class
        (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi))))
      (if (i32.eqz (i32.or (i32.eq (local.get $cls) (i32.const 1))
                           (i32.eq (local.get $cls) (i32.const 2))))
        (then (return (call $__rt_str_inc_dec_alpha
          (i32.wrap_i64 (local.get $lo)) (local.get $hi) (local.get $delta)))))))
  (if (i32.eqz (i32.or                                            ;; beyond scalars: TypeError
        (i32.or (i64.eqz (local.get $tag)) (i64.eq (local.get $tag) (i64.const 2)))
        (i64.eq (local.get $tag) (i64.const 1))))
    (then
      (call $__rt_fail (i32.const 9))                             ;; array/object: not modelled here
      (unreachable) ;; elephc-trap:post-noreturn:inc-dec-heap-operand
      ))
  ;; int, float, or fully-numeric string: the numeric add carries php's overflow promotion.
  (local.set $dbox (call $__rt_mixed_from_value (i64.const 0) (local.get $delta) (i64.const 0)))
  (local.set $res (call $__rt_mixed_numeric_add (local.get $cell) (local.get $dbox)))
  (call $__rt_decref_any (local.get $dbox))
  (local.get $res))
"#
    .to_string()
}

/// `__rt_str_inc_dec_alpha`: `++`/`--` on a NON-numeric string, php's perl-style rules.
///
/// Measured rules, each of which the obvious implementation gets wrong:
///
///   * the walk runs from the END and stops at the first non-alphanumeric byte, KEEPING the
///     wraps already made and DROPPING the carry: `"a!z"++` is `"a!a"`, never `"a!aa"` — and a
///     string whose LAST byte is non-alphanumeric is simply unchanged (`"ab!"++`);
///   * a carry that survives past the start PREPENDS by the class of the last wrapped byte:
///     `"Zz9"++` is `"AAa0"` ('A' because the leftmost wrapped byte was 'Z');
///   * `--` never edits a non-numeric string — `"a"--` stays `"a"` — but still deprecates;
///   * the EMPTY string: `""++` is the STRING "1", `""--` is the INT -1.
///
/// The result bytes are built in a scratch heap block, PERSISTED to an owned copy, and the
/// scratch freed: boxing the scratch directly would hand the cell a pointer one byte past its
/// block start, which `__rt_heap_free` cannot take back.
fn rt_str_inc_dec_alpha() -> String {
    r#"(func $__rt_str_inc_dec_alpha (param $ptr i32) (param $len i64) (param $delta i64) (result i32)
  (local $buf i32) (local $i i64) (local $c i32) (local $carry i32) (local $class i32)
  (local $out_ptr i32) (local $out_len i64) (local $np i32) (local $nl i64)
  (if (i64.le_s (local.get $delta) (i64.const 0))                 ;; -- : deprecate, never edit
    (then
      (if (i64.eqz (local.get $len))
        (then
          (call $__rt_depr_dec_empty)
          (return (call $__rt_mixed_from_value (i64.const 0) (i64.const -1) (i64.const 0)))))
      (call $__rt_depr_dec_str)
      (call $__rt_str_persist (local.get $ptr) (local.get $len))
      (local.set $nl)
      (local.set $np)
      (return (call $__rt_mixed_from_value (i64.const 1)
        (i64.extend_i32_u (local.get $np)) (local.get $nl)))))
  (call $__rt_depr_inc_str)
  (if (i64.eqz (local.get $len))                                  ;; ""++ is the STRING "1"
    (then
      (i32.store8 (global.get $__float_scratch) (i32.const 49))
      (call $__rt_str_persist (global.get $__float_scratch) (i64.const 1))
      (local.set $nl)
      (local.set $np)
      (return (call $__rt_mixed_from_value (i64.const 1)
        (i64.extend_i32_u (local.get $np)) (local.get $nl)))))
  ;; Working copy at buf+1: byte 0 is reserved for the possible prepend.
  (local.set $buf (call $__rt_heap_alloc (i32.add (i32.wrap_i64 (local.get $len)) (i32.const 1))))
  (memory.copy (i32.add (local.get $buf) (i32.const 1)) (local.get $ptr) (i32.wrap_i64 (local.get $len)))
  (local.set $i (i64.sub (local.get $len) (i64.const 1)))
  (local.set $carry (i32.const 1))
  (local.set $class (i32.const 0))
  (block $done (loop $walk
    (br_if $done (i64.lt_s (local.get $i) (i64.const 0)))
    (local.set $c (i32.load8_u (i32.add (i32.add (local.get $buf) (i32.const 1)) (i32.wrap_i64 (local.get $i)))))
    ;; wrap positions carry on: z->a, Z->A, 9->0
    (if (i32.eq (local.get $c) (i32.const 122))                   ;; 'z'
      (then
        (i32.store8 (i32.add (i32.add (local.get $buf) (i32.const 1)) (i32.wrap_i64 (local.get $i))) (i32.const 97))
        (local.set $class (i32.const 97))
        (local.set $i (i64.sub (local.get $i) (i64.const 1)))
        (br $walk)))
    (if (i32.eq (local.get $c) (i32.const 90))                    ;; 'Z'
      (then
        (i32.store8 (i32.add (i32.add (local.get $buf) (i32.const 1)) (i32.wrap_i64 (local.get $i))) (i32.const 65))
        (local.set $class (i32.const 65))
        (local.set $i (i64.sub (local.get $i) (i64.const 1)))
        (br $walk)))
    (if (i32.eq (local.get $c) (i32.const 57))                    ;; '9'
      (then
        (i32.store8 (i32.add (i32.add (local.get $buf) (i32.const 1)) (i32.wrap_i64 (local.get $i))) (i32.const 48))
        (local.set $class (i32.const 49))
        (local.set $i (i64.sub (local.get $i) (i64.const 1)))
        (br $walk)))
    ;; any other alphanumeric byte absorbs the carry
    (if (i32.or
          (i32.or
            (i32.and (i32.ge_u (local.get $c) (i32.const 97)) (i32.lt_u (local.get $c) (i32.const 122)))
            (i32.and (i32.ge_u (local.get $c) (i32.const 65)) (i32.lt_u (local.get $c) (i32.const 90))))
          (i32.and (i32.ge_u (local.get $c) (i32.const 48)) (i32.lt_u (local.get $c) (i32.const 57))))
      (then
        (i32.store8 (i32.add (i32.add (local.get $buf) (i32.const 1)) (i32.wrap_i64 (local.get $i)))
          (i32.add (local.get $c) (i32.const 1)))
        (local.set $carry (i32.const 0))
        (br $done)))
    ;; non-alphanumeric: the carry is DROPPED, wraps made so far stay
    (local.set $carry (i32.const 0))
    (br $done)))
  (if (result i32 i64) (i32.and (local.get $carry) (i32.ne (local.get $class) (i32.const 0)))
    (then                                                         ;; carry past the start: prepend
      (i32.store8 (local.get $buf) (local.get $class))
      (local.get $buf)
      (i64.add (local.get $len) (i64.const 1)))
    (else
      (i32.add (local.get $buf) (i32.const 1))
      (local.get $len)))
  (local.set $out_len)
  (local.set $out_ptr)
  (call $__rt_str_persist (local.get $out_ptr) (local.get $out_len))
  (local.set $nl)
  (local.set $np)
  (call $__rt_heap_free (local.get $buf))
  (call $__rt_mixed_from_value (i64.const 1)
    (i64.extend_i32_u (local.get $np)) (local.get $nl)))
"#
    .to_string()
}

/// `__rt_mixed_cmp_mixed`: php-src's `zend_compare` between two boxed cells.
///
/// Written against `scripts/php_compare_model.py`, which is validated on 7844 ordered pairs of
/// `php -n` output. The rule ORDER below is load-bearing and was measured, not assumed:
///
///   * a null against a STRING is a string comparison against `""`, not a boolean one;
///   * bool/null on either side makes BOTH sides booleans — and this outranks the NaN rule,
///     because NaN is TRUTHY: `NAN <=> true` is 0 while `NAN <=> 0` is 1;
///   * two ints compare exactly, so `PHP_INT_MAX` against `"9223372036854775807"` is 0, while
///     mixing an int with a float converts the int and makes
///     `9007199254740993 <=> 9007199254740992.0` equal;
///   * a number against a NON-numeric string renders the number as a string and compares bytes;
///   * two numeric strings that tie as doubles fall back to bytes only when one overflowed.
///
/// An array outranks any scalar, as in `__rt_mixed_cmp_i64`. Two arrays, and any object,
/// resource or callable, are not modelled and reach the same shared failure this file's other
/// comparison helper uses for them.
fn rt_mixed_cmp_mixed() -> String {
    format!(
        r#"(func $__rt_mixed_cmp_mixed (param $a i32) (param $b i32) (result i64)
  (local $ta i64) (local $la i64) (local $ha i64)
  (local $tb i64) (local $lb i64) (local $hb i64)
  (local $ba i64) (local $bb i64) (local $ca i32) (local $cb i32)
  (local $na i64) (local $nb i64) (local $fa f64) (local $fb f64)
  (local $ip i32) (local $il i32) (local $tie i64) (local $oa i32) (local $ob i32)
  (call $__rt_mixed_unbox (local.get $a))
  (local.set $ha)
  (local.set $la)
  (local.set $ta)
  (call $__rt_mixed_unbox (local.get $b))
  (local.set $hb)
  (local.set $lb)
  (local.set $tb)
  ;; null vs STRING is a string comparison against "", NOT the boolean rule below.
  (if (i32.and (i64.eq (local.get $ta) (i64.const 8)) (i64.eq (local.get $tb) (i64.const 1)))
    (then (return (call $__rt_i64_threeway
      (call $__rt_str_cmp (i32.const 0) (i64.const 0)
        (i32.wrap_i64 (local.get $lb)) (local.get $hb) (i32.const 0))
      (i64.const 0)))))
  (if (i32.and (i64.eq (local.get $tb) (i64.const 8)) (i64.eq (local.get $ta) (i64.const 1)))
    (then (return (call $__rt_i64_threeway
      (call $__rt_str_cmp (i32.wrap_i64 (local.get $la)) (local.get $ha)
        (i32.const 0) (i64.const 0) (i32.const 0))
      (i64.const 0)))))
  ;; bool or null on EITHER side: both become booleans. Ahead of the NaN rule on purpose.
  (if (i32.or
        (i32.or (i64.eq (local.get $ta) (i64.const 3)) (i64.eq (local.get $ta) (i64.const 8)))
        (i32.or (i64.eq (local.get $tb) (i64.const 3)) (i64.eq (local.get $tb) (i64.const 8))))
    (then
      (return (call $__rt_i64_threeway
        (call $__rt_mixed_truthy_parts (local.get $ta) (local.get $la) (local.get $ha))
        (call $__rt_mixed_truthy_parts (local.get $tb) (local.get $lb) (local.get $hb))))))
  ;; NaN on either side answers 1 in BOTH directions, and must be settled here rather than
  ;; left to the paths below: the string-against-number path negates its result when the
  ;; string sat on the right, which would turn php's 1 into -1.
  (if (i32.and (i64.eq (local.get $ta) (i64.const 2))
        (f64.ne (f64.reinterpret_i64 (local.get $la)) (f64.reinterpret_i64 (local.get $la))))
    (then (return (i64.const 1))))
  (if (i32.and (i64.eq (local.get $tb) (i64.const 2))
        (f64.ne (f64.reinterpret_i64 (local.get $lb)) (f64.reinterpret_i64 (local.get $lb))))
    (then (return (i64.const 1))))
  ;; Two strings.
  (if (i32.and (i64.eq (local.get $ta) (i64.const 1)) (i64.eq (local.get $tb) (i64.const 1)))
    (then
      (local.set $ca (call $__rt_str_numeric_class
        (i32.wrap_i64 (local.get $la)) (i32.wrap_i64 (local.get $ha))))
      (local.set $na (i64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))
      (local.set $oa (i32.load (i32.add (global.get $__float_scratch) (i32.const {oflow_offset}))))
      (local.set $cb (call $__rt_str_numeric_class
        (i32.wrap_i64 (local.get $lb)) (i32.wrap_i64 (local.get $hb))))
      (local.set $nb (i64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))
      (local.set $ob (i32.load (i32.add (global.get $__float_scratch) (i32.const {oflow_offset}))))
      (if (i32.and
            (i32.or (i32.eq (local.get $ca) (i32.const 1)) (i32.eq (local.get $ca) (i32.const 2)))
            (i32.or (i32.eq (local.get $cb) (i32.const 1)) (i32.eq (local.get $cb) (i32.const 2))))
        (then
          (if (i32.and (i32.eq (local.get $ca) (i32.const 1)) (i32.eq (local.get $cb) (i32.const 1)))
            (then (return (call $__rt_i64_threeway (local.get $na) (local.get $nb)))))
          (local.set $fa (if (result f64) (i32.eq (local.get $ca) (i32.const 1))
            (then (f64.convert_i64_s (local.get $na)))
            (else (f64.reinterpret_i64 (local.get $na)))))
          (local.set $fb (if (result f64) (i32.eq (local.get $cb) (i32.const 1))
            (then (f64.convert_i64_s (local.get $nb)))
            (else (f64.reinterpret_i64 (local.get $nb)))))
          (local.set $tie (call $__rt_f64_threeway (local.get $fa) (local.get $fb)))
          ;; A numeric tie between an OVERFLOWED integer string and another number falls back
          ;; to the bytes; an ordinary decimal tie such as "1.5"/"1.50" does not.
          ;; ...and equal INFINITIES do the same, though neither set `oflow`: php-src never
          ;; sets it for a float form, however large. `"1e400" <=> "1e500"` is -1 on the bytes,
          ;; and `"-1e400" <=> "-1e500"` is -1 too, so the fallback is a RAW byte compare
          ;; rather than a sign-aware one.
          (if (i32.and (i64.eqz (local.get $tie))
                (i32.or (i32.or (local.get $oa) (local.get $ob))
                  (i32.and
                    (f64.eq (f64.abs (local.get $fa)) (f64.const inf))
                    (f64.eq (f64.abs (local.get $fb)) (f64.const inf)))))
            (then (return (call $__rt_i64_threeway
              (call $__rt_str_cmp (i32.wrap_i64 (local.get $la)) (local.get $ha)
                (i32.wrap_i64 (local.get $lb)) (local.get $hb) (i32.const 0))
              (i64.const 0)))))
          (return (local.get $tie))))
      (return (call $__rt_i64_threeway
        (call $__rt_str_cmp (i32.wrap_i64 (local.get $la)) (local.get $ha)
          (i32.wrap_i64 (local.get $lb)) (local.get $hb) (i32.const 0))
        (i64.const 0)))))
  ;; An array outranks any scalar, whichever side it is on.
  (if (i32.and
        (i32.or (i64.eq (local.get $ta) (i64.const 4)) (i64.eq (local.get $ta) (i64.const 5)))
        (i32.eqz (i32.or (i64.eq (local.get $tb) (i64.const 4)) (i64.eq (local.get $tb) (i64.const 5)))))
    (then (return (i64.const 1))))
  (if (i32.and
        (i32.or (i64.eq (local.get $tb) (i64.const 4)) (i64.eq (local.get $tb) (i64.const 5)))
        (i32.eqz (i32.or (i64.eq (local.get $ta) (i64.const 4)) (i64.eq (local.get $ta) (i64.const 5)))))
    (then (return (i64.const -1))))
  ;; One string against one number: numeric when the string is, else the NUMBER is rendered
  ;; and the two compare as bytes.
  (if (i64.eq (local.get $ta) (i64.const 1))
    (then (return (call $__rt_mixed_cmp_str_num
      (i32.wrap_i64 (local.get $la)) (local.get $ha)
      (local.get $tb) (local.get $lb) (i32.const 0)))))
  (if (i64.eq (local.get $tb) (i64.const 1))
    (then (return (call $__rt_mixed_cmp_str_num
      (i32.wrap_i64 (local.get $lb)) (local.get $hb)
      (local.get $ta) (local.get $la) (i32.const 1)))))
  ;; Two numbers: exact when both are ints, otherwise both become doubles.
  (if (i32.and (i64.eqz (local.get $ta)) (i64.eqz (local.get $tb)))
    (then (return (call $__rt_i64_threeway (local.get $la) (local.get $lb)))))
  (if (i32.and
        (i32.or (i64.eqz (local.get $ta)) (i64.eq (local.get $ta) (i64.const 2)))
        (i32.or (i64.eqz (local.get $tb)) (i64.eq (local.get $tb) (i64.const 2))))
    (then
      (local.set $fa (if (result f64) (i64.eqz (local.get $ta))
        (then (f64.convert_i64_s (local.get $la)))
        (else (f64.reinterpret_i64 (local.get $la)))))
      (local.set $fb (if (result f64) (i64.eqz (local.get $tb))
        (then (f64.convert_i64_s (local.get $lb)))
        (else (f64.reinterpret_i64 (local.get $lb)))))
      (return (call $__rt_f64_threeway (local.get $fa) (local.get $fb)))))
  (call $__rt_fail (i32.const 9))                                 ;; two arrays, or an object
  unreachable)                                                    ;; elephc-trap:post-noreturn:mixed-compare

;; One STRING against one NUMBER, with `$flip` naming which side the string was on.
(func $__rt_mixed_cmp_str_num (param $sp i32) (param $sl i64) (param $ntag i64) (param $nlo i64) (param $flip i32) (result i64)
  (local $cls i32) (local $nv i64) (local $r i64) (local $bp i32) (local $bl i32)
  (local.set $cls (call $__rt_str_numeric_class (local.get $sp) (i32.wrap_i64 (local.get $sl))))
  (local.set $nv (i64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))
  (if (i32.eq (local.get $cls) (i32.const 1))                     ;; integral string inside i64
    (then
      (local.set $r (if (result i64) (i64.eqz (local.get $ntag))
        (then (call $__rt_i64_threeway (local.get $nv) (local.get $nlo)))
        (else (call $__rt_f64_threeway
          (f64.convert_i64_s (local.get $nv)) (f64.reinterpret_i64 (local.get $nlo))))))
      (return (if (result i64) (local.get $flip)
        (then (i64.sub (i64.const 0) (local.get $r))) (else (local.get $r))))))
  (if (i32.eq (local.get $cls) (i32.const 2))                     ;; float-shaped string
    (then
      (local.set $r (call $__rt_f64_threeway
        (f64.reinterpret_i64 (local.get $nv))
        (if (result f64) (i64.eqz (local.get $ntag))
          (then (f64.convert_i64_s (local.get $nlo)))
          (else (f64.reinterpret_i64 (local.get $nlo))))))
      (return (if (result i64) (local.get $flip)
        (then (i64.sub (i64.const 0) (local.get $r))) (else (local.get $r))))))
  ;; NOT numeric: php renders the NUMBER as a string and compares bytes.
  (if (i64.eqz (local.get $ntag))
    (then
      (call $__rt_itoa (local.get $nlo) (i32.add (global.get $__float_scratch) (i32.const 9216)))
      (local.set $bl)
      (local.set $bp))
    (else
      ;; The same call shape `Op::FToStr` uses, so a float renders here exactly as `echo`
      ;; would print it — `1.0E+300`, which is what the byte comparison then sees.
      (call $__rt_ftoa (local.get $nlo)
        (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80)
        (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 792)
        (i32.add (global.get $__float_scratch) (i32.const 4096)))
      (local.set $bl)
      (local.set $bp)))
  (local.set $r (call $__rt_i64_threeway
    (call $__rt_str_cmp (local.get $sp) (local.get $sl)
      (local.get $bp) (i64.extend_i32_u (local.get $bl)) (i32.const 0))
    (i64.const 0)))
  (if (result i64) (local.get $flip)
    (then (i64.sub (i64.const 0) (local.get $r))) (else (local.get $r))))
"#,
        value_offset = CLASS_VALUE_OFFSET,
        oflow_offset = CLASS_OFLOW_OFFSET
    )
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
          ;; -1 * INT_MIN is tested on its own branch BEFORE dividing: the check below would
          ;; compute `INT_MIN / -1`, which traps, and `i32.or` evaluates both of its operands.
          (if (i32.and (i64.eq (local.get $lv) (i64.const -1))
                       (i64.eq (local.get $rv) (i64.const -9223372036854775808)))
            (then (local.set $ovf (i32.const 1)))                 ;; -1 * INT_MIN overflows
            (else
              (if (i64.ne (local.get $lv) (i64.const 0))          ;; a zero factor never overflows
                (then
                  (if (i64.ne (i64.div_s (local.get $res) (local.get $lv)) (local.get $rv)) ;; division disagrees
                    (then (local.set $ovf (i32.const 1)))))))))) ;; product does not fit i64
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
