//! Purpose:
//! Arbitrary-precision decimal arithmetic on `wasm32-wasi`: `bcscale`, `bcadd`, `bcmul` and
//! `bcdivmod`.
//!
//! Called from:
//! - `runtime` registers the helpers when the module does decimal arithmetic.
//! - `builtins` lowers the four `RuntimeFnId` identities onto them.
//! - `capability` gates each call against the operand shapes the helpers read.
//!
//! Key details:
//! - This is a REIMPLEMENTATION, not a port. The native backend links `elephc-bcmath`; a wasm
//!   module assembles one self-contained WAT text and links nothing, so the arithmetic is
//!   written here in digits.
//! - A number is a heap block: sign at +0 (0 or 1), digit count at +4, fraction-digit count at
//!   +8, then one BYTE per decimal digit, most significant first, value 0..9. One digit per
//!   byte rather than packed limbs because every operation below is schoolbook and the
//!   products fit an `i32` column accumulator without any carry analysis — the decimal
//!   arithmetic bc promises is what makes the slow representation the right one.
//! - Every result is TRUNCATED to the target scale, never rounded. Measured: at scale 2,
//!   `bcadd("0.005", "0.004")` is `"0.00"`, and `bcmul("149.99", "0.22")` is `"32.99"` where
//!   the exact product is `32.9978`.
//! - Every result is RENDERED at exactly the target scale, trailing zeros included:
//!   `bcadd("1", "2")` at scale 2 is `"3.00"`, not `"3"`.
//! - `bcdivmod` answers the quotient at scale 0 and the remainder at the target scale. The
//!   quotient truncates toward zero and the remainder takes the DIVIDEND's sign, measured:
//!   `bcdivmod("-7", "2")` is `["-3", "-1.00"]` and `bcdivmod("7", "-2")` is `["-3", "1.00"]`.

use super::wat::{Global, ValType, WatModule};

/// The `__rt_fail` code a malformed bcmath operand reports under.
///
/// php raises `ValueError: bcadd(): Argument #1 ($num1) is not well-formed`, naming the
/// function and the argument. The message table here holds ONE string per code, so the
/// wording is generic; the class and the catchability are php's.
pub(super) const MALFORMED_FAILURE_CODE: i32 = 18;

/// The `__rt_fail` code for an explicit scale argument outside `[0, INT_MAX]`.
pub(super) const SCALE_FAILURE_CODE: i32 = 24;

/// The `__rt_fail` code for `bcscale()` given a scale outside `[0, INT_MAX]`.
pub(super) const BCSCALE_FAILURE_CODE: i32 = 23;

/// Adds the decimal arithmetic runtime to `wm`. Requires the heap and string runtimes.
pub(super) fn emit_bcmath_runtime(wm: &mut WatModule) {
    wm.add_global(Global {
        name: "__bc_scale".to_string(),
        ty: ValType::I32,
        mutable: true,
        init: 0,
    });
    wm.add_raw_func(RT_BC_PARSE);
    wm.add_raw_func(RT_BC_FIT);
    wm.add_raw_func(RT_BC_UCMP);
    wm.add_raw_func(RT_BC_UADD);
    wm.add_raw_func(RT_BC_USUB);
    wm.add_raw_func(RT_BC_ADDSUB);
    wm.add_raw_func(RT_BC_UMUL);
    wm.add_raw_func(RT_BC_MUL);
    wm.add_raw_func(RT_BC_UDIV);
    wm.add_raw_func(RT_BC_RENDER);
    wm.add_raw_func(RT_BCADD);
    wm.add_raw_func(RT_BCMUL);
    wm.add_raw_func(RT_BCDIVMOD);
}

/// `__rt_bc_parse`: a decimal string to a number block, or 0 when it is not well-formed.
///
/// Accepts what php accepts: an optional `+`/`-`, then digits, an optional `.`, and more
/// digits, with at least one digit somewhere. `.5` and `5.` are both well-formed — measured —
/// and an empty string, a bare sign or any other byte is not.
///
/// The integer part always gets at least one digit, so `.5` parses as `0.5` and every later
/// step can assume a non-empty integer part.
const RT_BC_PARSE: &str = r#"(func $__rt_bc_parse (param $ptr i32) (param $len i32) (result i32)
  (local $i i32) (local $sign i32) (local $int i32) (local $frac i32) (local $dot i32)
  (local $c i32) (local $b i32) (local $w i32) (local $lead i32)
  (if (i32.eqz (local.get $len)) (then (return (i32.const 0))))
  (if (i32.eq (i32.load8_u (local.get $ptr)) (i32.const 45))
    (then (local.set $sign (i32.const 1)) (local.set $i (i32.const 1))))
  (if (i32.eq (i32.load8_u (local.get $ptr)) (i32.const 43))
    (then (local.set $i (i32.const 1))))                            ;; a leading plus is accepted
  (local.set $lead (local.get $i))
  ;; -- first pass: count the digits on each side and reject anything else --
  (block $scanned (loop $scan
    (br_if $scanned (i32.ge_u (local.get $i) (local.get $len)))
    (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
    (if (i32.eq (local.get $c) (i32.const 46)) (then                ;; '.'
      (if (local.get $dot) (then (return (i32.const 0))))           ;; a second point
      (local.set $dot (i32.const 1)))
    (else
      (if (i32.or (i32.lt_u (local.get $c) (i32.const 48))
                  (i32.gt_u (local.get $c) (i32.const 57)))
        (then (return (i32.const 0))))                              ;; not a digit
      (if (local.get $dot)
        (then (local.set $frac (i32.add (local.get $frac) (i32.const 1))))
        (else (local.set $int (i32.add (local.get $int) (i32.const 1)))))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $scan)))
  (if (i32.eqz (i32.add (local.get $int) (local.get $frac)))
    (then (return (i32.const 0))))                                  ;; no digits at all
  ;; -- allocate and fill; an empty integer part becomes a single zero --
  (local.set $b (call $__rt_heap_alloc
    (i32.add (i32.const 16) (i32.add (select (local.get $int) (i32.const 1) (local.get $int))
                                     (local.get $frac)))))
  (if (i32.eqz (local.get $b)) (then (return (i32.const 0))))
  (i32.store (local.get $b) (local.get $sign))
  (i32.store offset=8 (local.get $b) (local.get $frac))
  (local.set $w (i32.const 16))
  (if (i32.eqz (local.get $int)) (then
    (i32.store8 (i32.add (local.get $b) (i32.const 16)) (i32.const 0))
    (local.set $w (i32.const 17))))
  (local.set $i (local.get $lead))
  (block $copied (loop $copy
    (br_if $copied (i32.ge_u (local.get $i) (local.get $len)))
    (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
    (if (i32.ne (local.get $c) (i32.const 46)) (then
      (i32.store8 (i32.add (local.get $b) (local.get $w)) (i32.sub (local.get $c) (i32.const 48)))
      (local.set $w (i32.add (local.get $w) (i32.const 1)))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $copy)))
  (i32.store offset=4 (local.get $b) (i32.sub (local.get $w) (i32.const 16)))
  (local.get $b))
"#;

/// `__rt_bc_fit`: a copy of `$b` with exactly `$int` integer digits and `$scale` fraction ones.
///
/// Reshaping is what lets every operation below assume its two operands line up digit for
/// digit: the caller computes the common shape once and both sides are fitted to it. An
/// integer part too short is zero-extended on the left; a fraction too long is TRUNCATED on
/// the right, which is bc's rule rather than a shortcut.
///
/// A number that does not fit the requested integer width is not representable, and the caller
/// only ever asks for a width it computed as large enough, so the overflow digits are dropped
/// rather than reported.
const RT_BC_FIT: &str = r#"(func $__rt_bc_fit (param $b i32) (param $int i32) (param $scale i32) (result i32)
  (local $out i32) (local $bint i32) (local $bscale i32) (local $i i32) (local $src i32) (local $d i32)
  (local.set $bscale (i32.load offset=8 (local.get $b)))
  (local.set $bint (i32.sub (i32.load offset=4 (local.get $b)) (local.get $bscale)))
  (local.set $out (call $__rt_heap_alloc
    (i32.add (i32.const 16) (i32.add (local.get $int) (local.get $scale)))))
  (if (i32.eqz (local.get $out)) (then (return (i32.const 0))))
  (i32.store (local.get $out) (i32.load (local.get $b)))
  (i32.store offset=4 (local.get $out) (i32.add (local.get $int) (local.get $scale)))
  (i32.store offset=8 (local.get $out) (local.get $scale))
  (block $done (loop $fill
    (br_if $done (i32.ge_u (local.get $i) (i32.add (local.get $int) (local.get $scale))))
    ;; source index in b's own digit run, aligned at the decimal point
    (local.set $src (i32.sub (i32.add (local.get $i) (local.get $bint)) (local.get $int)))
    (local.set $d (i32.const 0))
    (if (i32.and (i32.ge_s (local.get $src) (i32.const 0))
                 (i32.lt_s (local.get $src) (i32.add (local.get $bint) (local.get $bscale))))
      (then (local.set $d (i32.load8_u (i32.add (i32.add (local.get $b) (i32.const 16))
                                                (local.get $src))))))
    (i32.store8 (i32.add (i32.add (local.get $out) (i32.const 16)) (local.get $i))
                (local.get $d))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $fill)))
  (local.get $out))
"#;

/// `__rt_bc_ucmp`: magnitude comparison of two blocks of the SAME shape. -1, 0 or 1.
const RT_BC_UCMP: &str = r#"(func $__rt_bc_ucmp (param $a i32) (param $b i32) (result i32)
  (local $i i32) (local $n i32) (local $x i32) (local $y i32)
  (local.set $n (i32.load offset=4 (local.get $a)))
  (block $done (loop $walk
    (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
    (local.set $x (i32.load8_u (i32.add (i32.add (local.get $a) (i32.const 16)) (local.get $i))))
    (local.set $y (i32.load8_u (i32.add (i32.add (local.get $b) (i32.const 16)) (local.get $i))))
    (if (i32.ne (local.get $x) (local.get $y))
      (then (return (select (i32.const 1) (i32.const -1) (i32.gt_u (local.get $x) (local.get $y))))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $walk)))
  (i32.const 0))
"#;

/// `__rt_bc_uadd`: magnitude sum of two blocks of the SAME shape, in place into `$a`.
///
/// The caller fits both operands with one spare integer digit, so the carry always has room
/// and no reallocation is needed.
const RT_BC_UADD: &str = r#"(func $__rt_bc_uadd (param $a i32) (param $b i32)
  (local $i i32) (local $carry i32) (local $sum i32)
  (local.set $i (i32.load offset=4 (local.get $a)))
  (block $done (loop $walk
    (br_if $done (i32.eqz (local.get $i)))
    (local.set $i (i32.sub (local.get $i) (i32.const 1)))
    (local.set $sum (i32.add (i32.add
      (i32.load8_u (i32.add (i32.add (local.get $a) (i32.const 16)) (local.get $i)))
      (i32.load8_u (i32.add (i32.add (local.get $b) (i32.const 16)) (local.get $i))))
      (local.get $carry)))
    (local.set $carry (i32.div_u (local.get $sum) (i32.const 10)))
    (i32.store8 (i32.add (i32.add (local.get $a) (i32.const 16)) (local.get $i))
                (i32.rem_u (local.get $sum) (i32.const 10)))
    (br $walk))))
"#;

/// `__rt_bc_usub`: magnitude difference `a - b` of two same-shaped blocks, in place into `$a`.
///
/// The caller has established `|a| >= |b|`, so the final borrow is always zero.
const RT_BC_USUB: &str = r#"(func $__rt_bc_usub (param $a i32) (param $b i32)
  (local $i i32) (local $borrow i32) (local $diff i32)
  (local.set $i (i32.load offset=4 (local.get $a)))
  (block $done (loop $walk
    (br_if $done (i32.eqz (local.get $i)))
    (local.set $i (i32.sub (local.get $i) (i32.const 1)))
    (local.set $diff (i32.sub (i32.sub
      (i32.load8_u (i32.add (i32.add (local.get $a) (i32.const 16)) (local.get $i)))
      (i32.load8_u (i32.add (i32.add (local.get $b) (i32.const 16)) (local.get $i))))
      (local.get $borrow)))
    (local.set $borrow (i32.lt_s (local.get $diff) (i32.const 0)))
    (if (local.get $borrow) (then (local.set $diff (i32.add (local.get $diff) (i32.const 10)))))
    (i32.store8 (i32.add (i32.add (local.get $a) (i32.const 16)) (local.get $i))
                (local.get $diff))
    (br $walk))))
"#;

/// `__rt_bc_addsub`: signed `a + b`, or `a - b` when `$negate` flips `b`'s sign.
///
/// Both operands are fitted to a common shape with one spare integer digit, so like signs add
/// and unlike signs subtract the smaller magnitude from the larger and take the larger's sign.
/// Frees neither input; answers a fresh block.
const RT_BC_ADDSUB: &str = r#"(func $__rt_bc_addsub (param $a i32) (param $b i32) (param $negate i32) (result i32)
  (local $ia i32) (local $ib i32) (local $sa i32) (local $sb i32)
  (local $int i32) (local $scale i32) (local $fa i32) (local $fb i32)
  (local $sgn_a i32) (local $sgn_b i32) (local $cmp i32) (local $out i32)
  (local.set $sa (i32.load offset=8 (local.get $a)))
  (local.set $sb (i32.load offset=8 (local.get $b)))
  (local.set $ia (i32.sub (i32.load offset=4 (local.get $a)) (local.get $sa)))
  (local.set $ib (i32.sub (i32.load offset=4 (local.get $b)) (local.get $sb)))
  (local.set $int (i32.add (i32.const 1)
    (select (local.get $ia) (local.get $ib) (i32.gt_u (local.get $ia) (local.get $ib)))))
  (local.set $scale
    (select (local.get $sa) (local.get $sb) (i32.gt_u (local.get $sa) (local.get $sb))))
  (local.set $fa (call $__rt_bc_fit (local.get $a) (local.get $int) (local.get $scale)))
  (local.set $fb (call $__rt_bc_fit (local.get $b) (local.get $int) (local.get $scale)))
  (if (i32.or (i32.eqz (local.get $fa)) (i32.eqz (local.get $fb)))
    (then (return (i32.const 0))))
  (local.set $sgn_a (i32.load (local.get $fa)))
  (local.set $sgn_b (i32.xor (i32.load (local.get $fb)) (local.get $negate)))
  (if (i32.eq (local.get $sgn_a) (local.get $sgn_b))
    (then
      (call $__rt_bc_uadd (local.get $fa) (local.get $fb))
      (local.set $out (local.get $fa))
      (call $__rt_heap_free (local.get $fb)))
    (else
      (local.set $cmp (call $__rt_bc_ucmp (local.get $fa) (local.get $fb)))
      (if (i32.ge_s (local.get $cmp) (i32.const 0))
        (then
          (call $__rt_bc_usub (local.get $fa) (local.get $fb))
          (local.set $out (local.get $fa))
          (call $__rt_heap_free (local.get $fb)))
        (else
          (call $__rt_bc_usub (local.get $fb) (local.get $fa))
          (i32.store (local.get $fb) (local.get $sgn_b))
          (local.set $out (local.get $fb))
          (call $__rt_heap_free (local.get $fa))))))
  (local.get $out))
"#;

/// `__rt_bc_umul`: the exact magnitude product; the result's scale is the sum of the operands'.
const RT_BC_UMUL: &str = r#"(func $__rt_bc_umul (param $a i32) (param $b i32) (result i32)
  (local $na i32) (local $nb i32) (local $out i32) (local $i i32) (local $j i32)
  (local $carry i32) (local $acc i32) (local $at i32)
  (local.set $na (i32.load offset=4 (local.get $a)))
  (local.set $nb (i32.load offset=4 (local.get $b)))
  (local.set $out (call $__rt_heap_alloc
    (i32.add (i32.const 16) (i32.add (local.get $na) (local.get $nb)))))
  (if (i32.eqz (local.get $out)) (then (return (i32.const 0))))
  (i32.store (local.get $out) (i32.const 0))
  (i32.store offset=4 (local.get $out) (i32.add (local.get $na) (local.get $nb)))
  (i32.store offset=8 (local.get $out)
    (i32.add (i32.load offset=8 (local.get $a)) (i32.load offset=8 (local.get $b))))
  (memory.fill (i32.add (local.get $out) (i32.const 16)) (i32.const 0)
               (i32.add (local.get $na) (local.get $nb)))
  (local.set $i (local.get $na))
  (block $rows (loop $row
    (br_if $rows (i32.eqz (local.get $i)))
    (local.set $i (i32.sub (local.get $i) (i32.const 1)))
    (local.set $carry (i32.const 0))
    (local.set $j (local.get $nb))
    (block $cols (loop $col
      (br_if $cols (i32.eqz (local.get $j)))
      (local.set $j (i32.sub (local.get $j) (i32.const 1)))
      ;; the column this product lands in: i + j + 1 in the (na + nb)-digit result
      (local.set $at (i32.add (i32.add (local.get $out) (i32.const 16))
                              (i32.add (i32.add (local.get $i) (local.get $j)) (i32.const 1))))
      (local.set $acc (i32.add (i32.add
        (i32.load8_u (local.get $at))
        (i32.mul (i32.load8_u (i32.add (i32.add (local.get $a) (i32.const 16)) (local.get $i)))
                 (i32.load8_u (i32.add (i32.add (local.get $b) (i32.const 16)) (local.get $j)))))
        (local.get $carry)))
      (i32.store8 (local.get $at) (i32.rem_u (local.get $acc) (i32.const 10)))
      (local.set $carry (i32.div_u (local.get $acc) (i32.const 10)))
      (br $col)))
    ;; the row's final carry lands one column left of the last one written
    (local.set $at (i32.add (i32.add (local.get $out) (i32.const 16)) (local.get $i)))
    (local.set $acc (i32.add (i32.load8_u (local.get $at)) (local.get $carry)))
    (i32.store8 (local.get $at) (i32.rem_u (local.get $acc) (i32.const 10)))
    (br $row)))
  (local.get $out))
"#;

/// `__rt_bc_mul`: the signed product. Zero is always positive, as bc prints it.
const RT_BC_MUL: &str = r#"(func $__rt_bc_mul (param $a i32) (param $b i32) (result i32)
  (local $out i32) (local $i i32) (local $n i32) (local $nonzero i32)
  (local.set $out (call $__rt_bc_umul (local.get $a) (local.get $b)))
  (if (i32.eqz (local.get $out)) (then (return (i32.const 0))))
  (local.set $n (i32.load offset=4 (local.get $out)))
  (block $done (loop $walk
    (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
    (if (i32.load8_u (i32.add (i32.add (local.get $out) (i32.const 16)) (local.get $i)))
      (then (local.set $nonzero (i32.const 1)) (br $done)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $walk)))
  (if (local.get $nonzero)
    (then (i32.store (local.get $out)
            (i32.xor (i32.load (local.get $a)) (i32.load (local.get $b))))))
  (local.get $out))
"#;

/// `__rt_bc_udiv`: schoolbook long division of two same-shaped magnitudes.
///
/// Writes the integer quotient's digits into `$q` (one per dividend digit, most significant
/// first) and leaves the remainder in `$rem`, which the caller reads at the shared scale. Each
/// quotient digit is found by repeated subtraction, which is at most nine steps and needs no
/// estimate-and-correct.
const RT_BC_UDIV: &str = r#"(func $__rt_bc_udiv (param $a i32) (param $b i32) (param $q i32) (param $rem i32)
  (local $n i32) (local $i i32) (local $digit i32)
  (local.set $n (i32.load offset=4 (local.get $a)))
  ;; the running remainder starts at zero and gains one dividend digit per step
  (memory.fill (i32.add (local.get $rem) (i32.const 16)) (i32.const 0) (local.get $n))
  (i32.store (local.get $rem) (i32.const 0))
  (i32.store offset=4 (local.get $rem) (local.get $n))
  (i32.store offset=8 (local.get $rem) (i32.const 0))
  (block $done (loop $step
    (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
    ;; shift the remainder left one digit and bring the next dividend digit down
    (memory.copy (i32.add (local.get $rem) (i32.const 16))
                 (i32.add (local.get $rem) (i32.const 17))
                 (i32.sub (local.get $n) (i32.const 1)))
    (i32.store8 (i32.add (i32.add (local.get $rem) (i32.const 16))
                         (i32.sub (local.get $n) (i32.const 1)))
                (i32.load8_u (i32.add (i32.add (local.get $a) (i32.const 16)) (local.get $i))))
    ;; subtract the divisor while it still fits; at most nine times
    (local.set $digit (i32.const 0))
    (block $fit (loop $try
      (br_if $fit (i32.lt_s (call $__rt_bc_ucmp (local.get $rem) (local.get $b)) (i32.const 0)))
      (call $__rt_bc_usub (local.get $rem) (local.get $b))
      (local.set $digit (i32.add (local.get $digit) (i32.const 1)))
      (br_if $fit (i32.ge_u (local.get $digit) (i32.const 10)))     ;; cannot happen; a guard, not a rule
      (br $try)))
    (i32.store8 (i32.add (i32.add (local.get $q) (i32.const 16)) (local.get $i))
                (local.get $digit))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $step))))
"#;

/// `__rt_bc_render`: a number block as bc prints it, at EXACTLY `$scale` fraction digits.
///
/// Leading zeros are dropped down to one, the fraction is padded or truncated to the scale,
/// and a negative zero prints as `0` — measured, `bcmul("1.5", "0")` is `"0.00"` and not
/// `"-0.00"`. The answer is a persisted string the caller owns.
const RT_BC_RENDER: &str = r#"(func $__rt_bc_render (param $b i32) (param $scale i32) (result i32 i64)
  (local $fit i32) (local $int i32) (local $i i32) (local $first i32) (local $n i32)
  (local $out i32) (local $w i32) (local $nonzero i32) (local $need i32)
  (local $pptr i32) (local $plen i64)
  (local.set $int (i32.sub (i32.load offset=4 (local.get $b)) (i32.load offset=8 (local.get $b))))
  (if (i32.lt_s (local.get $int) (i32.const 1)) (then (local.set $int (i32.const 1))))
  (local.set $fit (call $__rt_bc_fit (local.get $b) (local.get $int) (local.get $scale)))
  (if (i32.eqz (local.get $fit)) (then (return (call $__rt_str_persist (i32.const 0) (i64.const 0)))))
  ;; the first integer digit worth printing, and whether anything at all is non-zero
  (local.set $n (i32.add (local.get $int) (local.get $scale)))
  (block $scanned (loop $scan
    (br_if $scanned (i32.ge_u (local.get $i) (local.get $n)))
    (if (i32.load8_u (i32.add (i32.add (local.get $fit) (i32.const 16)) (local.get $i)))
      (then (local.set $nonzero (i32.const 1)) (br $scanned)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $scan)))
  (local.set $first (i32.const 0))
  (block $lead (loop $skip
    (br_if $lead (i32.ge_u (local.get $first) (i32.sub (local.get $int) (i32.const 1))))
    (br_if $lead (i32.load8_u (i32.add (i32.add (local.get $fit) (i32.const 16))
                                       (local.get $first))))
    (local.set $first (i32.add (local.get $first) (i32.const 1)))
    (br $skip)))
  ;; sign + integer digits + '.' + fraction digits
  (local.set $need (i32.add (i32.const 2)
    (i32.add (i32.sub (local.get $int) (local.get $first)) (local.get $scale))))
  (local.set $out (call $__rt_heap_alloc (local.get $need)))
  (if (i32.eqz (local.get $out)) (then
    (call $__rt_heap_free (local.get $fit))
    (return (call $__rt_str_persist (i32.const 0) (i64.const 0)))))
  (if (i32.and (i32.load (local.get $fit)) (local.get $nonzero)) (then
    (i32.store8 (local.get $out) (i32.const 45))                    ;; '-' only on a non-zero
    (local.set $w (i32.const 1))))
  (local.set $i (local.get $first))
  (block $ints (loop $int_digit
    (br_if $ints (i32.ge_u (local.get $i) (local.get $int)))
    (i32.store8 (i32.add (local.get $out) (local.get $w))
      (i32.add (i32.const 48)
               (i32.load8_u (i32.add (i32.add (local.get $fit) (i32.const 16)) (local.get $i)))))
    (local.set $w (i32.add (local.get $w) (i32.const 1)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $int_digit)))
  (if (local.get $scale) (then
    (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 46))  ;; '.'
    (local.set $w (i32.add (local.get $w) (i32.const 1)))
    (block $fracs (loop $frac_digit
      (br_if $fracs (i32.ge_u (local.get $i) (local.get $n)))
      (i32.store8 (i32.add (local.get $out) (local.get $w))
        (i32.add (i32.const 48)
                 (i32.load8_u (i32.add (i32.add (local.get $fit) (i32.const 16)) (local.get $i)))))
      (local.set $w (i32.add (local.get $w) (i32.const 1)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $frac_digit)))))
  (call $__rt_heap_free (local.get $fit))
  (call $__rt_str_persist (local.get $out) (i64.extend_i32_u (local.get $w)))
  (local.set $plen)                                                 ;; the persisted length (top)
  (local.set $pptr)                                                 ;; the persisted pointer
  (call $__rt_heap_free (local.get $out))                           ;; the scratch copy is done
  (local.get $pptr) (local.get $plen))
"#;


/// `__rt_bcadd`: `bcadd($a, $b, $scale)`, answering `(pointer, length, status)`.
///
/// Status 0 is a result, 1 is an operand php would reject as not well-formed. The caller
/// raises; a runtime helper cannot, because the raise site is what the module's `catch`
/// machinery is built around.
const RT_BCADD: &str = r#"(func $__rt_bcadd (param $ap i32) (param $al i32) (param $bp i32) (param $bl i32) (param $scale i32) (result i32 i64 i32)
  (local $a i32) (local $b i32) (local $sum i32) (local $ptr i32) (local $len i64)
  (local.set $a (call $__rt_bc_parse (local.get $ap) (local.get $al)))
  (local.set $b (call $__rt_bc_parse (local.get $bp) (local.get $bl)))
  (if (i32.or (i32.eqz (local.get $a)) (i32.eqz (local.get $b))) (then
    (call $__rt_heap_free (local.get $a))
    (call $__rt_heap_free (local.get $b))
    (return (i32.const 0) (i64.const 0) (i32.const 1))))
  (local.set $sum (call $__rt_bc_addsub (local.get $a) (local.get $b) (i32.const 0)))
  (call $__rt_heap_free (local.get $a))
  (call $__rt_heap_free (local.get $b))
  (call $__rt_bc_render (local.get $sum) (local.get $scale))
  (local.set $len)
  (local.set $ptr)
  (call $__rt_heap_free (local.get $sum))
  (local.get $ptr) (local.get $len) (i32.const 0))
"#;

/// `__rt_bcmul`: `bcmul($a, $b, $scale)`, answering `(pointer, length, status)`.
const RT_BCMUL: &str = r#"(func $__rt_bcmul (param $ap i32) (param $al i32) (param $bp i32) (param $bl i32) (param $scale i32) (result i32 i64 i32)
  (local $a i32) (local $b i32) (local $product i32) (local $ptr i32) (local $len i64)
  (local.set $a (call $__rt_bc_parse (local.get $ap) (local.get $al)))
  (local.set $b (call $__rt_bc_parse (local.get $bp) (local.get $bl)))
  (if (i32.or (i32.eqz (local.get $a)) (i32.eqz (local.get $b))) (then
    (call $__rt_heap_free (local.get $a))
    (call $__rt_heap_free (local.get $b))
    (return (i32.const 0) (i64.const 0) (i32.const 1))))
  (local.set $product (call $__rt_bc_mul (local.get $a) (local.get $b)))
  (call $__rt_heap_free (local.get $a))
  (call $__rt_heap_free (local.get $b))
  (call $__rt_bc_render (local.get $product) (local.get $scale))
  (local.set $len)
  (local.set $ptr)
  (call $__rt_heap_free (local.get $product))
  (local.get $ptr) (local.get $len) (i32.const 0))
"#;

/// `__rt_bcdivmod`: one half of `bcdivmod($a, $b, $scale)`. Status 2 is division by zero.
///
/// Both operands are scaled to a common fraction width, which turns the division into an
/// INTEGER one over the digit arrays: `182.98 / 10` becomes `18298 / 1000`, whose quotient is
/// 18 and whose remainder 298 reads back as `2.98` at that width. The quotient is therefore
/// always scale 0 and the remainder always carries the shared width, exactly as php answers.
///
/// The remainder is `a - b * q`, computed rather than read out of the division, because the
/// long division's running remainder is only correct for magnitudes and php's takes the
/// DIVIDEND's sign — `bcdivmod("7", "-2")` answers a positive `1.00`.
const RT_BCDIVMOD: &str = r#"(func $__rt_bcdivmod (param $ap i32) (param $al i32) (param $bp i32) (param $bl i32) (param $scale i32) (param $want_rem i32) (result i32 i64 i32)
  (local $a i32) (local $b i32) (local $shared i32) (local $width i32)
  (local $fa i32) (local $fb i32) (local $q i32) (local $rem i32)
  (local $zero i32) (local $i i32) (local $prod i32) (local $diff i32)
  (local $ptr i32) (local $len i64)
  (local.set $a (call $__rt_bc_parse (local.get $ap) (local.get $al)))
  (local.set $b (call $__rt_bc_parse (local.get $bp) (local.get $bl)))
  (if (i32.or (i32.eqz (local.get $a)) (i32.eqz (local.get $b))) (then
    (call $__rt_heap_free (local.get $a))
    (call $__rt_heap_free (local.get $b))
    (return (i32.const 0) (i64.const 0) (i32.const 1))))
  ;; a divisor of zero is php's DivisionByZeroError, whatever its scale or sign
  (local.set $zero (i32.const 1))
  (local.set $i (i32.const 0))
  (block $checked (loop $digits
    (br_if $checked (i32.ge_u (local.get $i) (i32.load offset=4 (local.get $b))))
    (if (i32.load8_u (i32.add (i32.add (local.get $b) (i32.const 16)) (local.get $i)))
      (then (local.set $zero (i32.const 0)) (br $checked)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $digits)))
  (if (local.get $zero) (then
    (call $__rt_heap_free (local.get $a))
    (call $__rt_heap_free (local.get $b))
    (return (i32.const 0) (i64.const 0) (i32.const 2))))
  ;; scale both to a common fraction width; the division is then an integer one
  (local.set $shared (i32.load offset=8 (local.get $a)))
  (if (i32.gt_u (i32.load offset=8 (local.get $b)) (local.get $shared))
    (then (local.set $shared (i32.load offset=8 (local.get $b)))))
  (local.set $width (i32.load offset=4 (local.get $a)))
  (if (i32.gt_u (i32.load offset=4 (local.get $b)) (local.get $width))
    (then (local.set $width (i32.load offset=4 (local.get $b)))))
  (local.set $width (i32.add (local.get $width) (local.get $shared)))
  (local.set $fa (call $__rt_bc_fit (local.get $a) (local.get $width) (local.get $shared)))
  (local.set $fb (call $__rt_bc_fit (local.get $b) (local.get $width) (local.get $shared)))
  (local.set $q (call $__rt_heap_alloc
    (i32.add (i32.const 16) (i32.add (local.get $width) (local.get $shared)))))
  (local.set $rem (call $__rt_heap_alloc
    (i32.add (i32.const 16) (i32.add (local.get $width) (local.get $shared)))))
  (if (i32.or (i32.or (i32.eqz (local.get $fa)) (i32.eqz (local.get $fb)))
              (i32.or (i32.eqz (local.get $q)) (i32.eqz (local.get $rem)))) (then
    (return (i32.const 0) (i64.const 0) (i32.const 1))))
  ;; the integer division ignores signs; the quotient's sign is the operands' xor
  (i32.store (local.get $fa) (i32.const 0))
  (i32.store (local.get $fb) (i32.const 0))
  (i32.store offset=8 (local.get $fa) (i32.const 0))
  (i32.store offset=8 (local.get $fb) (i32.const 0))
  (i32.store (local.get $q) (i32.const 0))
  (i32.store offset=4 (local.get $q) (i32.add (local.get $width) (local.get $shared)))
  (i32.store offset=8 (local.get $q) (i32.const 0))
  (call $__rt_bc_udiv (local.get $fa) (local.get $fb) (local.get $q) (local.get $rem))
  (i32.store (local.get $q)
    (i32.xor (i32.load (local.get $a)) (i32.load (local.get $b))))
  (if (i32.eqz (local.get $want_rem)) (then
    (call $__rt_bc_render (local.get $q) (i32.const 0))
    (local.set $len)
    (local.set $ptr))
  (else
    ;; r = a - b * q, which carries the dividend's sign by construction
    (local.set $prod (call $__rt_bc_mul (local.get $b) (local.get $q)))
    (local.set $diff (call $__rt_bc_addsub (local.get $a) (local.get $prod) (i32.const 1)))
    (call $__rt_bc_render (local.get $diff) (local.get $scale))
    (local.set $len)
    (local.set $ptr)
    (call $__rt_heap_free (local.get $prod))
    (call $__rt_heap_free (local.get $diff))))
  (call $__rt_heap_free (local.get $a))
  (call $__rt_heap_free (local.get $b))
  (call $__rt_heap_free (local.get $fa))
  (call $__rt_heap_free (local.get $fb))
  (call $__rt_heap_free (local.get $q))
  (call $__rt_heap_free (local.get $rem))
  (local.get $ptr) (local.get $len) (i32.const 0))
"#;

#[cfg(test)]
mod tests {
    use crate::ir::RuntimeFnId;

    /// The four identities this module serves are the four the example needs.
    ///
    /// bcmath is not a port here: the native backend links `elephc-bcmath`, and a wasm module
    /// links nothing, so what a program can do with decimal arithmetic on this target is
    /// exactly what is written in `bcmath.rs`. The list is therefore a capability statement
    /// rather than a formality — `bcsub`, `bcdiv`, `bcpow`, `bcsqrt` and `bccomp` are NOT
    /// served, and the audit refuses them rather than answering approximately.
    #[test]
    fn decimal_arithmetic_serves_exactly_what_is_written() {
        for target in [
            RuntimeFnId::BcAdd,
            RuntimeFnId::BcMul,
            RuntimeFnId::BcDivmod,
            RuntimeFnId::BcScale,
        ] {
            assert!(
                crate::codegen_wasm::capability::runtime_function_is_supported(target),
                "{target:?} must be supported on this target"
            );
        }
        for absent in [
            RuntimeFnId::BcSub,
            RuntimeFnId::BcDiv,
            RuntimeFnId::BcPow,
            RuntimeFnId::BcSqrt,
        ] {
            assert!(
                !crate::codegen_wasm::capability::runtime_function_is_supported(absent),
                "{absent:?} has no implementation here and must stay refused"
            );
        }
    }
}
