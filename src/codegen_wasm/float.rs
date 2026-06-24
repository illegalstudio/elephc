//! Purpose:
//! Hand-authored WebAssembly text (WAT) runtime for the wasm32-wasi backend's
//! float<->string bridge. Stage 1 provides `__rt_f64_decompose`, which splits an
//! IEEE-754 binary64 (raw i64 bits) into sign, integer mantissa, base-2 exponent,
//! and a class code (finite / inf / NaN / zero) for the exact decimal-conversion path.
//!
//! Called from:
//! - `crate::codegen_wasm` runtime emission via `emit_float_runtime` (wired into
//!   PHP-visible float formatting in a later stage; unit-tested here under `wasmer`).
//!
//! Key details:
//! - `__rt_f64_decompose` is pure bit manipulation: no allocation, no memory access,
//!   no calls. Magnitude = mantissa * 2^exp2; mantissa and exp2 are 0 for zero/inf/nan.
//! - Mantissa carries the implicit leading 1 for normals (`frac | 1<<52`) and the bare
//!   fraction for subnormals (`exp2 = -1074`).

use super::wat::{Global, ValType, WatModule};

/// Decomposes an IEEE-754 binary64 (raw i64 bits) into four results, in order:
/// sign (i32 0/1), integer mantissa (i64), signed base-2 exponent (i32), and a class
/// code (i32: 0 finite non-zero, 1 infinity, 2 NaN, 3 zero). Magnitude = mantissa *
/// 2^exp2. Pure bit manipulation: no allocation, no memory, no calls.
const RT_F64_DECOMPOSE: &str = r#"(func $__rt_f64_decompose (param $bits i64) (result i32 i64 i32 i32)
  (local $sign i32)
  (local $raw_exp i64)
  (local $frac i64)
  (local.set $sign (i32.wrap_i64 (i64.and (i64.shr_u (local.get $bits) (i64.const 63)) (i64.const 1))))  ;; sign bit (bit 63)
  (local.set $raw_exp (i64.and (i64.shr_u (local.get $bits) (i64.const 52)) (i64.const 2047)))  ;; biased exponent (bits 62..52)
  (local.set $frac (i64.and (local.get $bits) (i64.const 0xFFFFFFFFFFFFF)))  ;; 52-bit fraction field (bits 51..0)
  (if (i64.eq (local.get $raw_exp) (i64.const 2047))                  ;; all-ones exponent: infinity or NaN
    (then
      (if (i64.eqz (local.get $frac))                                 ;; zero fraction -> infinity
        (then (return (local.get $sign) (i64.const 0) (i32.const 0) (i32.const 1))))  ;; class 1 = infinity
      (return (local.get $sign) (i64.const 0) (i32.const 0) (i32.const 2))))  ;; non-zero fraction -> NaN (class 2)
  (if (i64.eqz (local.get $raw_exp))                                  ;; zero exponent: zero or subnormal
    (then
      (if (i64.eqz (local.get $frac))                                 ;; zero fraction -> +/- zero
        (then (return (local.get $sign) (i64.const 0) (i32.const 0) (i32.const 3))))  ;; class 3 = zero
      (return (local.get $sign) (local.get $frac) (i32.const -1074) (i32.const 0))))  ;; subnormal: mantissa=frac, exp2=-1074
  (return                                                             ;; normal number (class 0)
    (local.get $sign)
    (i64.or (local.get $frac) (i64.const 0x10000000000000))           ;; mantissa = frac | (1<<52), implicit leading 1
    (i32.wrap_i64 (i64.sub (local.get $raw_exp) (i64.const 1075)))    ;; exp2 = raw_exp - 1075 (= raw_exp - 1023 - 52)
    (i32.const 0)))                                                   ;; class 0 = finite non-zero
"#;

/// Multiplies a fixed-width big integer in place by a small unsigned 32-bit factor.
///
/// The big integer is `$n` little-endian limbs of base 2^32 (i32 limbs, limb[0] least
/// significant) starting at byte address `$ptr`; `$k` is the multiplier in [0, 2^32-1]
/// passed in an i64. Each limb becomes `(limb*k + carry) mod 2^32` with carry propagated
/// low-to-high. Returns the final carry (i64 in [0, 2^32-1]) the caller may store as an
/// (n+1)-th limb. Overflow-safe: `limb*k + carry < 2^64` fits one i64 accumulator.
const RT_BIGNUM_MUL_U32: &str = r#"(func $__rt_bignum_mul_u32 (param $ptr i32) (param $n i32) (param $k i64) (result i64)
  (local $i i32) (local $carry i64) (local $acc i64) (local $addr i32)
  (local.set $i (i32.const 0))                                       ;; limb index = 0
  (local.set $carry (i64.const 0))                                   ;; running carry = 0
  (block $end                                                        ;; loop exit target
    (loop $top                                                       ;; iterate over limbs low-to-high
      (br_if $end (i32.ge_u (local.get $i) (local.get $n)))          ;; stop once i >= n
      (local.set $addr (i32.add (local.get $ptr) (i32.shl (local.get $i) (i32.const 2))))  ;; &limb[i] = ptr + i*4
      (local.set $acc                                                ;; acc = limb[i]*k + carry
        (i64.add
          (i64.mul (i64.load32_u (local.get $addr)) (local.get $k))
          (local.get $carry)))
      (i64.store32 (local.get $addr) (local.get $acc))               ;; limb[i] = low 32 bits of acc
      (local.set $carry (i64.shr_u (local.get $acc) (i64.const 32))) ;; carry = high 32 bits of acc
      (local.set $i (i32.add (local.get $i) (i32.const 1)))          ;; i = i + 1
      (br $top)))                                                    ;; continue the loop
  (local.get $carry))                                                ;; return the final carry
"#;

/// Divides a fixed-width big integer in place by a small unsigned 32-bit divisor.
///
/// The big integer is `$n` little-endian limbs of base 2^32 at `$ptr`; `$d` is the
/// divisor in [1, 2^32-1] (caller guarantees non-zero) passed in an i64. Processes
/// limbs most-significant to least, carrying a running remainder: the array becomes
/// `floor(value / d)` and the function returns `value mod d` (i64 in [0, d-1]). This
/// is the decimal digit-extraction primitive (repeated divmod by 1e9). Overflow-safe:
/// `(rem << 32) | limb < d*2^32 < 2^64` fits one i64.
const RT_BIGNUM_DIVMOD_U32: &str = r#"(func $__rt_bignum_divmod_u32 (param $ptr i32) (param $n i32) (param $d i64) (result i64)
  (local $i i32) (local $rem i64) (local $cur i64) (local $addr i32)
  (local.set $i (local.get $n))                                     ;; i = n (one past the top limb)
  (local.set $rem (i64.const 0))                                    ;; running remainder = 0
  (block $end                                                       ;; loop exit target
    (loop $top                                                      ;; iterate limbs high-to-low
      (br_if $end (i32.eqz (local.get $i)))                         ;; stop once i == 0 (all limbs done)
      (local.set $i (i32.sub (local.get $i) (i32.const 1)))         ;; pre-decrement: now i indexes the current limb
      (local.set $addr (i32.add (local.get $ptr) (i32.shl (local.get $i) (i32.const 2))))  ;; &limb[i] = ptr + i*4
      (local.set $cur                                               ;; cur = (rem << 32) | limb[i]
        (i64.or
          (i64.shl (local.get $rem) (i64.const 32))
          (i64.load32_u (local.get $addr))))
      (i64.store32 (local.get $addr) (i64.div_u (local.get $cur) (local.get $d)))  ;; limb[i] = cur / d
      (local.set $rem (i64.rem_u (local.get $cur) (local.get $d)))  ;; rem = cur % d
      (br $top)))                                                   ;; continue the loop
  (local.get $rem))                                                 ;; return the final remainder
"#;

/// Multiplies a fixed-width big integer in place by a small factor, `$count` times.
///
/// Reuses `__rt_bignum_mul_u32`. The caller must pre-zero a buffer of `$n` limbs large
/// enough that the final product still fits (high limbs absorb every carry), so each
/// multiply's carry-out is 0 and is dropped — there is no limb-append. Used to build the
/// exact integer J = value * 10^P: J = mantissa, then `*2` exp2 times (exp2 >= 0) or `*5`
/// (-exp2) times (exp2 < 0). Returns nothing.
const RT_BIGNUM_MUL_SMALL_N_TIMES: &str = r#"(func $__rt_bignum_mul_small_n_times (param $ptr i32) (param $n i32) (param $factor i64) (param $count i32)
  (local $c i32)                                                    ;; iteration counter
  (local.set $c (i32.const 0))                                      ;; iteration counter = 0
  (block $end                                                       ;; loop exit target
    (loop $top                                                      ;; repeat the multiply $count times
      (br_if $end (i32.ge_u (local.get $c) (local.get $count)))     ;; stop once c >= count
      (drop (call $__rt_bignum_mul_u32 (local.get $ptr) (local.get $n) (local.get $factor)))  ;; multiply in place; carry is 0 (drop it)
      (local.set $c (i32.add (local.get $c) (i32.const 1)))         ;; c = c + 1
      (br $top))))                                                  ;; continue the loop
"#;

/// Tests whether a fixed-width big integer is zero (all `$n` limbs zero).
///
/// Scans the little-endian limbs at `$ptr` low-to-high and short-circuits: returns 0
/// (i32) the moment any limb is non-zero, or 1 if every limb is zero. Used as the
/// termination test for the divmod-by-1e9 digit-extraction loop.
const RT_BIGNUM_IS_ZERO: &str = r#"(func $__rt_bignum_is_zero (param $ptr i32) (param $n i32) (result i32)
  (local $i i32)
  (local.set $i (i32.const 0))                                      ;; limb index = 0
  (block $end                                                       ;; loop exit target
    (loop $top                                                      ;; scan limbs low-to-high
      (br_if $end (i32.ge_u (local.get $i) (local.get $n)))         ;; stop once i >= n (all scanned)
      (if (i32.load (i32.add (local.get $ptr) (i32.shl (local.get $i) (i32.const 2))))  ;; if limb[i] != 0
        (then (return (i32.const 0))))                              ;; a non-zero limb -> not zero
      (local.set $i (i32.add (local.get $i) (i32.const 1)))         ;; i = i + 1
      (br $top)))                                                   ;; continue the loop
  (i32.const 1))                                                    ;; every limb was zero -> 1
"#;

/// Writes a 32-bit value in [0, 999999999] as exactly 9 ASCII decimal digits.
///
/// Stores 9 bytes at `$ptr`, most-significant digit first, zero-padded: `ptr[0]` is
/// `value / 10^8 mod 10` and `ptr[8]` is `value mod 10`. This emits one divmod-by-1e9
/// chunk into the digit buffer; intermediate chunks keep their leading zeros (the most
/// significant chunk's leading zeros are stripped by the caller). Returns nothing.
const RT_U32_TO_9DIGITS: &str = r#"(func $__rt_u32_to_9digits (param $value i32) (param $ptr i32)
  (local $i i32) (local $v i32)
  (local.set $v (local.get $value))                                ;; working copy of the value
  (local.set $i (i32.const 9))                                     ;; one past the last byte index
  (block $end                                                      ;; loop exit target
    (loop $top                                                     ;; write 9 digits right-to-left
      (br_if $end (i32.eqz (local.get $i)))                        ;; stop after writing all 9
      (local.set $i (i32.sub (local.get $i) (i32.const 1)))        ;; pre-decrement: i is the current byte index
      (i32.store8                                                  ;; ptr[i] = ASCII digit (v mod 10)
        (i32.add (local.get $ptr) (local.get $i))
        (i32.add (i32.const 48) (i32.rem_u (local.get $v) (i32.const 10))))
      (local.set $v (i32.div_u (local.get $v) (i32.const 10)))     ;; drop the digit just written
      (br $top)))                                                  ;; continue the loop
)"#;

/// Produces the exact decimal digits of an IEEE-754 double — the heart of the float
/// formatter. Orchestrates `__rt_f64_decompose`, the bignum primitives, and
/// `__rt_u32_to_9digits`.
///
/// Builds the exact integer `J = magnitude * 10^p` (so `value == J / 10^p`): with
/// `exp2 >= 0`, `J = mantissa * 2^exp2` and `p = 0`; with `exp2 < 0`, `J = mantissa *
/// 5^(-exp2)` and `p = -exp2` (an exact integer, no rounding). It then peels 9-digit
/// chunks via repeated divmod-by-1e9 into `$dbuf`, strips leading zeros, and returns
/// `(sign, class, digptr, ndigits, p)` where `digptr`/`ndigits` are the significant
/// decimal digits most-significant-first and `p` is the fractional digit count. For
/// non-finite/zero inputs (`class != 0`) it returns no digits. `$big` must be a
/// pre-zeroed buffer of `$nlimbs` limbs large enough to hold `J` (80 limbs covers every
/// double), and `$dbuf` must be at least 768 bytes (the smallest subnormal has 751 digits).
const RT_F64_DIGITS: &str = r#"(func $__rt_f64_digits (param $bits i64) (param $big i32) (param $nlimbs i32) (param $dbuf i32) (param $dmax i32) (result i32 i32 i32 i32 i32)
  (local $sign i32) (local $mant i64) (local $exp2 i32) (local $class i32)
  (local $p i32) (local $wp i32) (local $rem i64) (local $start i32)
  (call $__rt_f64_decompose (local.get $bits))                     ;; -> sign, mantissa, exp2, class
  (local.set $class)                                               ;; pop class
  (local.set $exp2)                                                ;; pop exp2
  (local.set $mant)                                                ;; pop mantissa
  (local.set $sign)                                                ;; pop sign
  (if (i32.ne (local.get $class) (i32.const 0))                    ;; non-finite or zero: no digits
    (then (return (local.get $sign) (local.get $class) (local.get $dbuf) (i32.const 0) (i32.const 0))))
  (i64.store32 (local.get $big) (local.get $mant))                 ;; big[0] = mantissa low 32 bits
  (i64.store32 (i32.add (local.get $big) (i32.const 4)) (i64.shr_u (local.get $mant) (i64.const 32)))  ;; big[1] = mantissa high bits
  (local.set $p (i32.const 0))                                     ;; default p = 0 (exp2 >= 0 case)
  (if (i32.ge_s (local.get $exp2) (i32.const 0))                   ;; exp2 >= 0: J = mantissa * 2^exp2
    (then (call $__rt_bignum_mul_small_n_times (local.get $big) (local.get $nlimbs) (i64.const 2) (local.get $exp2))))
  (if (i32.lt_s (local.get $exp2) (i32.const 0))                   ;; exp2 < 0: J = mantissa * 5^(-exp2), p = -exp2
    (then
      (local.set $p (i32.sub (i32.const 0) (local.get $exp2)))     ;; p = -exp2
      (call $__rt_bignum_mul_small_n_times (local.get $big) (local.get $nlimbs) (i64.const 5) (local.get $p))))
  (local.set $wp (local.get $dmax))                               ;; write cursor starts past the buffer end
  (block $pend                                                    ;; chunk-loop exit target
    (loop $ptop                                                   ;; do-while: peel at least one 9-digit chunk
      (local.set $rem (call $__rt_bignum_divmod_u32 (local.get $big) (local.get $nlimbs) (i64.const 1000000000)))  ;; low 9 digits
      (local.set $wp (i32.sub (local.get $wp) (i32.const 9)))     ;; reserve 9 bytes to the left
      (call $__rt_u32_to_9digits (i32.wrap_i64 (local.get $rem)) (i32.add (local.get $dbuf) (local.get $wp)))  ;; write the chunk
      (br_if $pend (call $__rt_bignum_is_zero (local.get $big) (local.get $nlimbs)))  ;; stop when J reaches 0
      (br $ptop)))                                                ;; otherwise peel another chunk
  (local.set $start (local.get $wp))                             ;; first written byte (most significant chunk)
  (block $send                                                    ;; strip-loop exit target
    (loop $stop                                                   ;; advance start over leading '0' bytes
      (br_if $send (i32.ge_u (local.get $start) (i32.sub (local.get $dmax) (i32.const 1))))  ;; keep at least one digit
      (br_if $send (i32.ne (i32.load8_u (i32.add (local.get $dbuf) (local.get $start))) (i32.const 48)))  ;; stop at first non-'0'
      (local.set $start (i32.add (local.get $start) (i32.const 1)))  ;; skip this leading zero
      (br $stop)))                                                ;; keep stripping
  (return                                                        ;; sign, class(0), digptr, ndigits, p
    (local.get $sign)
    (local.get $class)
    (i32.add (local.get $dbuf) (local.get $start))               ;; digptr = dbuf + start
    (i32.sub (local.get $dmax) (local.get $start))               ;; ndigits = dmax - start
    (local.get $p)))                                             ;; p = fractional digit count
"#;

/// Rounds an ASCII decimal digit string (most-significant first) to `$prec` significant
/// digits in place, round-half-to-even.
///
/// Inspects the dropped tail (digits at index `$prec` and beyond): rounds up iff the
/// first dropped digit is `> 5`, or it is `5` with a nonzero digit after it, or it is `5`
/// exactly half-way and the last kept digit is odd. Otherwise truncates. On a carry that
/// overflows all `$prec` digits (the prefix was all '9's) the buffer becomes `'1'` then
/// `$prec-1` zeros and the routine returns 1 (the caller adds 1 to the decimal exponent);
/// every other case returns 0. If `$ndigits <= $prec` nothing is rounded and it returns 0.
const RT_ROUND_DIGITS: &str = r#"(func $__rt_round_digits (param $digptr i32) (param $ndigits i32) (param $prec i32) (result i32)
  (local $rd i32) (local $up i32) (local $i i32) (local $sticky i32)
  (if (i32.le_s (local.get $ndigits) (local.get $prec))            ;; already <= prec significant digits: nothing to round
    (then (return (i32.const 0))))
  (local.set $rd (i32.sub (i32.load8_u (i32.add (local.get $digptr) (local.get $prec))) (i32.const 48)))  ;; first dropped digit value
  (local.set $up (i32.const 0))                                    ;; round-up flag = false
  (if (i32.gt_s (local.get $rd) (i32.const 5))                     ;; dropped digit > 5 -> round up
    (then (local.set $up (i32.const 1))))
  (if (i32.eq (local.get $rd) (i32.const 5))                       ;; dropped digit exactly 5 -> half-way case
    (then
      (local.set $sticky (i32.const 0))                            ;; any nonzero digit after the round digit?
      (local.set $i (i32.add (local.get $prec) (i32.const 1)))     ;; start just past the round digit
      (block $se                                                   ;; sticky-scan exit target
        (loop $sl                                                  ;; scan the remaining dropped digits
          (br_if $se (i32.ge_s (local.get $i) (local.get $ndigits)))  ;; scanned all -> stop
          (if (i32.ne (i32.load8_u (i32.add (local.get $digptr) (local.get $i))) (i32.const 48))  ;; found a nonzero digit
            (then (local.set $sticky (i32.const 1)) (br $se)))     ;; sticky = true, stop scanning
          (local.set $i (i32.add (local.get $i) (i32.const 1)))    ;; next digit
          (br $sl)))                                               ;; continue scanning
      (if (local.get $sticky)                                      ;; nonzero tail -> definitely round up
        (then (local.set $up (i32.const 1)))
        (else
          (if (i32.and (i32.sub (i32.load8_u (i32.add (local.get $digptr) (i32.sub (local.get $prec) (i32.const 1)))) (i32.const 48)) (i32.const 1))  ;; last kept digit is odd
            (then (local.set $up (i32.const 1))))))))              ;; round half to even
  (if (i32.eqz (local.get $up))                                    ;; not rounding up -> truncation suffices
    (then (return (i32.const 0))))
  (local.set $i (local.get $prec))                                 ;; carry walk starts one past the last kept digit
  (block $ce                                                       ;; carry-loop exit target
    (loop $cl                                                      ;; propagate the +1 carry leftward
      (br_if $ce (i32.eqz (local.get $i)))                         ;; carried out of every digit -> overflow
      (local.set $i (i32.sub (local.get $i) (i32.const 1)))        ;; pre-decrement to the current digit
      (if (i32.lt_s (i32.load8_u (i32.add (local.get $digptr) (local.get $i))) (i32.const 57))  ;; digit < '9': increment and stop
        (then
          (i32.store8 (i32.add (local.get $digptr) (local.get $i)) (i32.add (i32.load8_u (i32.add (local.get $digptr) (local.get $i))) (i32.const 1)))  ;; digit += 1
          (return (i32.const 0))))                                 ;; done, no exponent shift
      (i32.store8 (i32.add (local.get $digptr) (local.get $i)) (i32.const 48))  ;; was '9' -> '0', carry continues
      (br $cl)))                                                   ;; keep carrying
  (i32.store8 (local.get $digptr) (i32.const 49))                  ;; overflow: prefix was all '9' -> leading '1'
  (i32.const 1))                                                   ;; signal exponent shifts up by 1
"#;

/// Writes a non-negative 32-bit value as its minimal decimal ASCII (no leading zeros,
/// no padding) and returns the digit count.
///
/// `value` 0 writes a single '0' (length 1). Used to format a float's decimal exponent
/// (e.g. 20 -> "20", 7 -> "7") in PHP-style scientific notation, which omits the leading
/// zero a C `%E` exponent would pad. Counts digits, then writes them right-to-left.
const RT_U32_TO_DEC: &str = r#"(func $__rt_u32_to_dec (param $value i32) (param $ptr i32) (result i32)
  (local $v i32) (local $len i32) (local $i i32)
  (if (i32.eqz (local.get $value))                                 ;; zero -> single '0'
    (then
      (i32.store8 (local.get $ptr) (i32.const 48))                 ;; write '0'
      (return (i32.const 1))))                                     ;; length 1
  (local.set $v (local.get $value))                               ;; copy for digit counting
  (local.set $len (i32.const 0))                                  ;; digit count = 0
  (block $ce                                                      ;; count-loop exit target
    (loop $cl                                                     ;; count digits
      (br_if $ce (i32.eqz (local.get $v)))                        ;; stop when the copy reaches 0
      (local.set $len (i32.add (local.get $len) (i32.const 1)))   ;; one more digit
      (local.set $v (i32.div_u (local.get $v) (i32.const 10)))    ;; drop the lowest digit
      (br $cl)))                                                  ;; continue counting
  (local.set $v (local.get $value))                              ;; reset copy for writing
  (local.set $i (local.get $len))                                ;; write cursor = len (one past the last byte)
  (block $we                                                      ;; write-loop exit target
    (loop $wl                                                     ;; write digits right-to-left
      (br_if $we (i32.eqz (local.get $i)))                        ;; stop after writing all digits
      (local.set $i (i32.sub (local.get $i) (i32.const 1)))       ;; pre-decrement to the current byte index
      (i32.store8                                                 ;; ptr[i] = ASCII digit (v mod 10)
        (i32.add (local.get $ptr) (local.get $i))
        (i32.add (i32.const 48) (i32.rem_u (local.get $v) (i32.const 10))))
      (local.set $v (i32.div_u (local.get $v) (i32.const 10)))    ;; drop the digit just written
      (br $wl)))                                                  ;; continue writing
  (local.get $len))                                              ;; return the digit count
"#;

/// Formats already-rounded significant digits in PHP-style scientific notation.
///
/// PHP scientific differs from C `%E`: the mantissa is always `D.D...` (a leading digit,
/// '.', then the remaining significant digits, or a single '0' when there are none), and
/// the exponent has no leading zero (`E-7`, not `E-07`). Writes `[-]d0.dddE±X` into `$out`
/// where `$x` is the leading digit's decimal exponent and `$sign` selects the leading '-';
/// the exponent magnitude is written via `__rt_u32_to_dec`. Returns the byte length.
const RT_FTOA_SCIENTIFIC: &str = r#"(func $__rt_ftoa_scientific (param $digptr i32) (param $nsig i32) (param $x i32) (param $sign i32) (param $out i32) (result i32)
  (local $w i32) (local $i i32) (local $ax i32)
  (local.set $w (i32.const 0))                                    ;; write cursor (bytes written) = 0
  (if (local.get $sign)                                           ;; negative -> leading '-'
    (then
      (i32.store8 (local.get $out) (i32.const 45))                ;; '-'
      (local.set $w (i32.const 1))))                              ;; advance cursor
  (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.load8_u (local.get $digptr)))  ;; leading digit
  (local.set $w (i32.add (local.get $w) (i32.const 1)))           ;; advance
  (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 46))  ;; '.'
  (local.set $w (i32.add (local.get $w) (i32.const 1)))           ;; advance
  (if (i32.gt_s (local.get $nsig) (i32.const 1))                  ;; more significant digits?
    (then
      (local.set $i (i32.const 1))                                ;; copy digit[1..nsig]
      (block $fe                                                  ;; copy-loop exit target
        (loop $fl                                                 ;; append the remaining significant digits
          (br_if $fe (i32.ge_s (local.get $i) (local.get $nsig))) ;; done copying
          (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.load8_u (i32.add (local.get $digptr) (local.get $i))))  ;; copy digit
          (local.set $w (i32.add (local.get $w) (i32.const 1)))   ;; advance out
          (local.set $i (i32.add (local.get $i) (i32.const 1)))   ;; advance source
          (br $fl))))                                             ;; continue copying
    (else
      (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 48))  ;; no more digits -> '0'
      (local.set $w (i32.add (local.get $w) (i32.const 1)))))     ;; advance
  (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 69))  ;; 'E'
  (local.set $w (i32.add (local.get $w) (i32.const 1)))           ;; advance
  (if (i32.lt_s (local.get $x) (i32.const 0))                     ;; negative exponent
    (then
      (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 45))  ;; '-'
      (local.set $ax (i32.sub (i32.const 0) (local.get $x))))     ;; ax = -x
    (else
      (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 43))  ;; '+'
      (local.set $ax (local.get $x))))                           ;; ax = x
  (local.set $w (i32.add (local.get $w) (i32.const 1)))           ;; advance past the sign
  (local.set $w (i32.add (local.get $w) (call $__rt_u32_to_dec (local.get $ax) (i32.add (local.get $out) (local.get $w)))))  ;; write exponent magnitude, advance by its length
  (local.get $w))                                                ;; total bytes written
"#;

/// Formats already-rounded significant digits in PHP fixed-point notation.
///
/// `$x` is the leading digit's decimal exponent (here -4 <= x <= 13). If x >= 0 the integer
/// part is x+1 digits (significant digits then zero padding), with the remaining significant
/// digits after a '.'; if x < 0 it writes "0.", `-x-1` leading zeros, then all significant
/// digits. `$sign` selects a leading '-'. Trailing zeros were already stripped by the caller.
/// Writes into `$out` and returns the byte length.
const RT_FTOA_FIXED: &str = r#"(func $__rt_ftoa_fixed (param $digptr i32) (param $nsig i32) (param $x i32) (param $sign i32) (param $out i32) (result i32)
  (local $w i32) (local $i i32) (local $intlen i32) (local $k i32)
  (local.set $w (i32.const 0))                                    ;; bytes written = 0
  (if (local.get $sign)                                           ;; negative -> leading '-'
    (then
      (i32.store8 (local.get $out) (i32.const 45))                ;; '-'
      (local.set $w (i32.const 1))))                              ;; advance
  (if (i32.ge_s (local.get $x) (i32.const 0))                     ;; X >= 0: integer part has X+1 digits
    (then
      (local.set $intlen (i32.add (local.get $x) (i32.const 1)))  ;; integer digit count
      (local.set $i (i32.const 0))                                ;; source index
      (block $ie                                                  ;; integer-copy exit
        (loop $il                                                 ;; copy up to min(intlen,nsig) significant digits
          (br_if $ie (i32.ge_s (local.get $i) (local.get $intlen)))  ;; filled the integer slots
          (br_if $ie (i32.ge_s (local.get $i) (local.get $nsig)))    ;; ran out of significant digits
          (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.load8_u (i32.add (local.get $digptr) (local.get $i))))  ;; copy digit
          (local.set $w (i32.add (local.get $w) (i32.const 1)))   ;; advance out
          (local.set $i (i32.add (local.get $i) (i32.const 1)))   ;; advance source
          (br $il)))                                              ;; continue
      (block $pe                                                  ;; zero-pad exit
        (loop $pl                                                 ;; pad integer part with zeros if nsig < intlen
          (br_if $pe (i32.ge_s (local.get $i) (local.get $intlen)))  ;; integer part complete
          (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 48))  ;; '0'
          (local.set $w (i32.add (local.get $w) (i32.const 1)))   ;; advance
          (local.set $i (i32.add (local.get $i) (i32.const 1)))   ;; advance count
          (br $pl)))                                              ;; continue
      (if (i32.gt_s (local.get $nsig) (local.get $intlen))        ;; remaining digits -> fractional part
        (then
          (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 46))  ;; '.'
          (local.set $w (i32.add (local.get $w) (i32.const 1)))   ;; advance
          (local.set $i (local.get $intlen))                      ;; fraction starts at index intlen
          (block $fe                                              ;; fraction-copy exit
            (loop $fl                                             ;; copy digit[intlen..nsig]
              (br_if $fe (i32.ge_s (local.get $i) (local.get $nsig)))  ;; done
              (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.load8_u (i32.add (local.get $digptr) (local.get $i))))  ;; copy digit
              (local.set $w (i32.add (local.get $w) (i32.const 1)))  ;; advance out
              (local.set $i (i32.add (local.get $i) (i32.const 1)))  ;; advance source
              (br $fl)))))                                        ;; continue; closes the fraction loop, block and if
      (return (local.get $w))))                                   ;; X >= 0 case done
  (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 48))  ;; X < 0: leading '0'
  (local.set $w (i32.add (local.get $w) (i32.const 1)))           ;; advance
  (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 46))  ;; '.'
  (local.set $w (i32.add (local.get $w) (i32.const 1)))           ;; advance
  (local.set $k (i32.sub (i32.sub (i32.const 0) (local.get $x)) (i32.const 1)))  ;; leading-zero count = -x-1
  (local.set $i (i32.const 0))                                    ;; zero counter
  (block $ze                                                      ;; leading-zero exit
    (loop $zl                                                     ;; write -x-1 leading zeros
      (br_if $ze (i32.ge_s (local.get $i) (local.get $k)))        ;; done padding
      (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 48))  ;; '0'
      (local.set $w (i32.add (local.get $w) (i32.const 1)))       ;; advance
      (local.set $i (i32.add (local.get $i) (i32.const 1)))       ;; advance count
      (br $zl)))                                                  ;; continue
  (local.set $i (i32.const 0))                                    ;; source index
  (block $se                                                      ;; significant-digit exit
    (loop $sl                                                     ;; append all nsig significant digits
      (br_if $se (i32.ge_s (local.get $i) (local.get $nsig)))     ;; done
      (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.load8_u (i32.add (local.get $digptr) (local.get $i))))  ;; copy digit
      (local.set $w (i32.add (local.get $w) (i32.const 1)))       ;; advance out
      (local.set $i (i32.add (local.get $i) (i32.const 1)))       ;; advance source
      (br $sl)))                                                  ;; continue
  (local.get $w))                                                ;; total bytes written
"#;

/// Top-level `__rt_ftoa` orchestrator: converts an IEEE-754 double (given by its raw
/// bit pattern in `$bits`) to its PHP-compatible decimal string. This is the single
/// entry point PHP-visible code calls; it owns the full pipeline.
///
/// Flow: decode the bits via `__rt_f64_digits` into (sign, class, digptr, ndigits, p),
/// short-circuit INF/NAN/zero, otherwise compute `X = ndigits-1-p` (the leading digit's
/// decimal exponent), round to 14 significant digits (bumping X on all-9s carry
/// overflow), clamp `nsig = min(ndigits, 14)`, strip trailing '0' digits (keeping at
/// least one), then dispatch to `__rt_ftoa_scientific` when `X < -4` or `X >= 14` and to
/// `__rt_ftoa_fixed` otherwise. Writes into `$out` and returns the pointer and byte
/// length. `$big`/`$nlimbs` and `$dbuf`/`$dmax` are scratch handed straight to
/// `__rt_f64_digits`.
const RT_FTOA: &str = r#"(func $__rt_ftoa (param $bits i64) (param $big i32) (param $nlimbs i32) (param $dbuf i32) (param $dmax i32) (param $out i32) (result i32 i32)
  (local $sign i32) (local $class i32) (local $digptr i32) (local $ndigits i32) (local $p i32)
  (local $x i32) (local $nsig i32) (local $w i32)
  (call $__rt_f64_digits (local.get $bits) (local.get $big) (local.get $nlimbs) (local.get $dbuf) (local.get $dmax))  ;; -> sign, class, digptr, ndigits, p
  (local.set $p)                                                   ;; pop p
  (local.set $ndigits)                                            ;; pop ndigits
  (local.set $digptr)                                            ;; pop digptr
  (local.set $class)                                            ;; pop class
  (local.set $sign)                                            ;; pop sign
  (if (i32.eq (local.get $class) (i32.const 1))                  ;; infinity
    (then
      (local.set $w (i32.const 0))                              ;; length cursor
      (if (local.get $sign)                                     ;; negative infinity
        (then (i32.store8 (local.get $out) (i32.const 45)) (local.set $w (i32.const 1))))  ;; '-'
      (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 73))  ;; 'I'
      (i32.store8 (i32.add (local.get $out) (i32.add (local.get $w) (i32.const 1))) (i32.const 78))  ;; 'N'
      (i32.store8 (i32.add (local.get $out) (i32.add (local.get $w) (i32.const 2))) (i32.const 70))  ;; 'F'
      (return (local.get $out) (i32.add (local.get $w) (i32.const 3)))))  ;; "INF"
  (if (i32.eq (local.get $class) (i32.const 2))                  ;; NaN
    (then
      (i32.store8 (local.get $out) (i32.const 78))              ;; 'N'
      (i32.store8 (i32.add (local.get $out) (i32.const 1)) (i32.const 65))  ;; 'A'
      (i32.store8 (i32.add (local.get $out) (i32.const 2)) (i32.const 78))  ;; 'N'
      (return (local.get $out) (i32.const 3))))                 ;; "NAN"
  (if (i32.eq (local.get $class) (i32.const 3))                  ;; zero
    (then
      (local.set $w (i32.const 0))                              ;; length cursor
      (if (local.get $sign)                                     ;; negative zero
        (then (i32.store8 (local.get $out) (i32.const 45)) (local.set $w (i32.const 1))))  ;; '-'
      (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 48))  ;; '0'
      (return (local.get $out) (i32.add (local.get $w) (i32.const 1)))))  ;; "0" or "-0"
  (local.set $x (i32.sub (i32.sub (local.get $ndigits) (i32.const 1)) (local.get $p)))  ;; X = ndigits-1-p
  (if (call $__rt_round_digits (local.get $digptr) (local.get $ndigits) (i32.const 14))  ;; round to 14 sig digits
    (then (local.set $x (i32.add (local.get $x) (i32.const 1)))))  ;; carry overflow shifts the exponent
  (local.set $nsig (local.get $ndigits))                         ;; nsig = min(ndigits, 14)
  (if (i32.gt_s (local.get $ndigits) (i32.const 14))             ;; clamp to 14 significant digits
    (then (local.set $nsig (i32.const 14))))
  (block $st                                                     ;; trailing-zero strip exit
    (loop $sl                                                    ;; drop trailing '0' digits, keep at least one
      (br_if $st (i32.le_s (local.get $nsig) (i32.const 1)))     ;; keep at least one digit
      (br_if $st (i32.ne (i32.load8_u (i32.add (local.get $digptr) (i32.sub (local.get $nsig) (i32.const 1)))) (i32.const 48)))  ;; last digit not '0'
      (local.set $nsig (i32.sub (local.get $nsig) (i32.const 1)))  ;; drop it
      (br $sl)))                                                 ;; continue
  (if (i32.or (i32.lt_s (local.get $x) (i32.const -4)) (i32.ge_s (local.get $x) (i32.const 14)))  ;; scientific range
    (then (return (local.get $out) (call $__rt_ftoa_scientific (local.get $digptr) (local.get $nsig) (local.get $x) (local.get $sign) (local.get $out)))))  ;; scientific notation
  (return (local.get $out) (call $__rt_ftoa_fixed (local.get $digptr) (local.get $nsig) (local.get $x) (local.get $sign) (local.get $out))))  ;; fixed notation
"#;

/// `__rt_parse_decimal` tokenizes a decimal numeric string into the pieces the
/// string->float orchestrator needs. Reads bytes `[$ptr, $ptr+$len)`, skips leading
/// whitespace, an optional sign, then either an `inf`/`nan` quick path or
/// `[integer][.fraction][eExponent]`.
///
/// Returns four i32s: `sign` (1 if a leading '-' was consumed), `ndig` (count of ASCII
/// digit bytes written to `$out`, no sign and no point), `K` (decimal exponent such
/// that value = int($out digits) × 10^K), and `class` (0 finite, 1 empty/no-digits ->
/// 0.0, 2 infinity, 3 NaN). The `inf`/`nan` paths consume their three letters and
/// short-circuit via `br $done` with `ndig`/`K` zeroed.
const RT_PARSE_DECIMAL: &str = r#"  (func $__rt_parse_decimal (param $ptr i32) (param $len i32) (param $out i32) (result i32 i32 i32 i32) (local $i i32) (local $sign i32) (local $ndig i32) (local $K i32) (local $class i32) (local $frac i32) (local $c i32) (local $es i32) (local $ev i32) (local $tmp i32)  ;; decimal-string parser: ptr,len,out -> sign,ndig,K,class (4 results)
    (block $wse  ;; A: skip leading whitespace
      (loop $wsl (br_if $wse (i32.ge_s (local.get $i) (local.get $len))) (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))  ;; A: whitespace loop
        (if (i32.or (i32.and (i32.ge_s (local.get $c) (i32.const 9)) (i32.le_s (local.get $c) (i32.const 13))) (i32.eq (local.get $c) (i32.const 32)))  ;; A: whitespace byte? (9..13 or 32)
          (then (local.set $i (i32.add (local.get $i) (i32.const 1))) (br $wsl)))))  ;; A: consume whitespace, continue
    (if (i32.lt_s (local.get $i) (local.get $len))  ;; bounds check: cursor < len before reading
      (then (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))  ;; load current byte ptr[i] into c
        (if (i32.eq (local.get $c) (i32.const 45))  ;; ...
          (then (local.set $sign (i32.const 1)) (local.set $i (i32.add (local.get $i) (i32.const 1))))  ;; consume one byte (cursor++)
          (else  ;; ...
            (if (i32.eq (local.get $c) (i32.const 43))  ;; B: '+' -> consume
              (then (local.set $i (i32.add (local.get $i) (i32.const 1)))))))))  ;; consume one byte (cursor++)
    (block $done  ;; C..G: parse body (inf/nan exits early via br $done)
      (if (i32.lt_s (local.get $i) (local.get $len))  ;; bounds check: cursor < len before reading
        (then (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))  ;; load current byte ptr[i] into c
          (if (i32.and (i32.and (i32.le_s (i32.add (local.get $i) (i32.const 3)) (local.get $len)) (i32.or (i32.eq (local.get $c) (i32.const 105)) (i32.eq (local.get $c) (i32.const 73)))) (i32.and (i32.or (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $i) (i32.const 1)))) (i32.const 110)) (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $i) (i32.const 1)))) (i32.const 78))) (i32.or (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $i) (i32.const 2)))) (i32.const 102)) (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $i) (i32.const 2)))) (i32.const 70)))))  ;; C: "inf"/"INF"? (i/I, n/N, f/F, 3 bytes)
            (then (local.set $class (i32.const 2)) (local.set $i (i32.add (local.get $i) (i32.const 3))) (br $done)))  ;; C: infinity -> class=2
          (if (i32.and (i32.and (i32.le_s (i32.add (local.get $i) (i32.const 3)) (local.get $len)) (i32.or (i32.eq (local.get $c) (i32.const 110)) (i32.eq (local.get $c) (i32.const 78)))) (i32.and (i32.or (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $i) (i32.const 1)))) (i32.const 97)) (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $i) (i32.const 1)))) (i32.const 65))) (i32.or (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $i) (i32.const 2)))) (i32.const 110)) (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $i) (i32.const 2)))) (i32.const 78)))))  ;; C: "nan"/"NAN"? (n/N, a/A, n/N, 3 bytes)
            (then (local.set $class (i32.const 3)) (local.set $i (i32.add (local.get $i) (i32.const 3))) (br $done)))))  ;; C: NaN -> class=3
      (block $ide  ;; D: integer digits
        (loop $idl (br_if $ide (i32.ge_s (local.get $i) (local.get $len))) (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i)))) (br_if $ide (i32.lt_s (local.get $c) (i32.const 48))) (br_if $ide (i32.gt_s (local.get $c) (i32.const 57))) (i32.store8 (i32.add (local.get $out) (local.get $ndig)) (local.get $c)) (local.set $ndig (i32.add (local.get $ndig) (i32.const 1))) (local.set $i (i32.add (local.get $i) (i32.const 1))) (br $idl)))  ;; D: integer-digit loop
      (if (i32.lt_s (local.get $i) (local.get $len))  ;; bounds check: cursor < len before reading
        (then (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))  ;; load current byte ptr[i] into c
          (if (i32.eq (local.get $c) (i32.const 46))  ;; E: '.' -> consume, parse fraction digits
            (then (local.set $i (i32.add (local.get $i) (i32.const 1)))  ;; consume one byte (cursor++)
              (block $fde  ;; E: fraction digits
                (loop $fdl (br_if $fde (i32.ge_s (local.get $i) (local.get $len))) (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i)))) (br_if $fde (i32.lt_s (local.get $c) (i32.const 48))) (br_if $fde (i32.gt_s (local.get $c) (i32.const 57))) (i32.store8 (i32.add (local.get $out) (local.get $ndig)) (local.get $c)) (local.set $ndig (i32.add (local.get $ndig) (i32.const 1))) (local.set $frac (i32.add (local.get $frac) (i32.const 1))) (local.set $i (i32.add (local.get $i) (i32.const 1))) (br $fdl)))))))  ;; E: fraction-digit loop
      (if (i32.lt_s (local.get $i) (local.get $len))  ;; bounds check: cursor < len before reading
        (then (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))  ;; load current byte ptr[i] into c
          (if (i32.or (i32.eq (local.get $c) (i32.const 101)) (i32.eq (local.get $c) (i32.const 69)))  ;; F: 'e' or 'E' -> parse exponent
            (then (local.set $i (i32.add (local.get $i) (i32.const 1))) (local.set $es (i32.const 1))  ;; F: default exponent sign +1
              (if (i32.lt_s (local.get $i) (local.get $len))  ;; bounds check: cursor < len before reading
                (then (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))  ;; load current byte ptr[i] into c
                  (if (i32.eq (local.get $c) (i32.const 45))  ;; ...
                    (then (local.set $es (i32.const -1)) (local.set $i (i32.add (local.get $i) (i32.const 1)))))  ;; F: '-' -> exponent sign -1
                  (if (i32.eq (local.get $c) (i32.const 43))  ;; B: '+' -> consume
                    (then (local.set $i (i32.add (local.get $i) (i32.const 1)))))))  ;; consume one byte (cursor++)
              (block $ede  ;; F: exponent digits
                (loop $edl (br_if $ede (i32.ge_s (local.get $i) (local.get $len))) (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i)))) (br_if $ede (i32.lt_s (local.get $c) (i32.const 48))) (br_if $ede (i32.gt_s (local.get $c) (i32.const 57))) (local.set $ev (i32.add (i32.mul (local.get $ev) (i32.const 10)) (i32.sub (local.get $c) (i32.const 48)))) (local.set $i (i32.add (local.get $i) (i32.const 1))) (br $edl))) (local.set $K (i32.add (local.get $K) (i32.mul (local.get $es) (local.get $ev))))))))  ;; F: exponent-digit loop
      (if (i32.eqz (local.get $ndig))  ;; G: no digits parsed?
        (then (local.set $class (i32.const 1)))) (local.set $K (i32.sub (local.get $K) (local.get $frac)))) (return (local.get $sign) (local.get $ndig) (local.get $K) (local.get $class)))  ;; G: K = es*ev - frac (decimal exponent)
"#;

/// Compares two little-endian base-2^32 big integers; returns i32 -1 / 0 / 1 for
/// a < b / a == b / a > b. Trims leading zero limbs (effective length) first, then
/// compares lengths; equal-length numbers are scanned most-significant limb first.
const RT_BIGNUM_CMP: &str = r#"  (func $__rt_bignum_cmp (param $a i32) (param $na i32) (param $b i32) (param $nb i32) (result i32) (local $ha i32) (local $hb i32) (local $i i32) (local $va i64) (local $vb i64) (local.set $ha (local.get $na)) (local.set $hb (local.get $nb))  ;; compare two big ints a,b (limb counts na,nb) -> i32 -1/0/1
    (block $ae  ;; trim leading zero limbs off a
      (loop $al (br_if $ae (i32.eqz (local.get $ha))) (br_if $ae (i64.ne (i64.load32_u (i32.add (local.get $a) (i32.shl (i32.sub (local.get $ha) (i32.const 1)) (i32.const 2)))) (i64.const 0))) (local.set $ha (i32.sub (local.get $ha) (i32.const 1))) (br $al)))  ;; a trim loop
    (block $be  ;; trim leading zero limbs off b
      (loop $bl (br_if $be (i32.eqz (local.get $hb))) (br_if $be (i64.ne (i64.load32_u (i32.add (local.get $b) (i32.shl (i32.sub (local.get $hb) (i32.const 1)) (i32.const 2)))) (i64.const 0))) (local.set $hb (i32.sub (local.get $hb) (i32.const 1))) (br $bl)))  ;; b trim loop
    (if (i32.gt_s (local.get $ha) (local.get $hb))  ;; a longer than b?
      (then (return (i32.const 1))))  ;; a > b -> return 1
    (if (i32.lt_s (local.get $ha) (local.get $hb))  ;; a shorter than b?
      (then (return (i32.const -1)))) (local.set $i (local.get $ha))  ;; i = ha (trimmed effective length, for equal-length scan)
    (block $ce  ;; equal length: scan limbs top-down
      (loop $cl (br_if $ce (i32.eqz (local.get $i))) (local.set $i (i32.sub (local.get $i) (i32.const 1))) (local.set $va (i64.load32_u (i32.add (local.get $a) (i32.shl (local.get $i) (i32.const 2))))) (local.set $vb (i64.load32_u (i32.add (local.get $b) (i32.shl (local.get $i) (i32.const 2))))) (br_if $cl (i64.eq (local.get $va) (local.get $vb)))  ;; equal-length scan loop
        (if (i64.gt_u (local.get $va) (local.get $vb))  ;; a limb > b limb?
          (then (return (i32.const 1)))  ;; a > b -> return 1
          (else (return (i32.const -1)))) (br $cl))) (return (i32.const 0)))  ;; a limb < b limb -> return -1
"#;

/// In-place big-integer subtraction a -= b over $n limbs (little-endian base-2^32).
/// Caller guarantees a >= b (check with `__rt_bignum_cmp`). Returns the final borrow
/// (i64, 0 when no underflow); limbs use two complement mod 2^32 on underflow.
const RT_BIGNUM_SUB: &str = r#"  (func $__rt_bignum_sub (param $a i32) (param $b i32) (param $n i32) (result i64) (local $i i32) (local $borrow i64) (local $va i64) (local $vb i64) (local $acc i64) (local $addr i32) (local.set $i (i32.const 0)) (local.set $borrow (i64.const 0))  ;; in-place a -= b over n limbs (caller guarantees a>=b); returns final borrow
    (block $end  ;; subtraction loop
      (loop $top (br_if $end (i32.ge_u (local.get $i) (local.get $n))) (local.set $addr (i32.add (local.get $a) (i32.shl (local.get $i) (i32.const 2)))) (local.set $va (i64.load32_u (local.get $addr))) (local.set $vb (i64.load32_u (i32.add (local.get $b) (i32.shl (local.get $i) (i32.const 2))))) (local.set $acc (i64.sub (i64.sub (local.get $va) (local.get $vb)) (local.get $borrow))) (i64.store32 (local.get $addr) (local.get $acc)) (local.set $borrow (select (i64.const 1) (i64.const 0) (i64.lt_s (local.get $acc) (i64.const 0)))) (local.set $i (i32.add (local.get $i) (i32.const 1))) (br $top))) (local.get $borrow))  ;; limb subtract loop
"#;

/// Adds a small unsigned 32-bit value to a fixed-width big integer in place.
///
/// `$ptr` is `$n` little-endian base-2^32 limbs; `$k` is the addend in [0, 2^32-1] passed
/// in an i64 (it becomes the initial carry). Each limb becomes `(limb + carry) mod 2^32`
/// with carry propagated low-to-high. Returns the final carry (i64), which the caller may
/// store as an `(n+1)`-th limb. Used by the S5c ratio rounder to build the decimal
/// mantissa `M` digit-by-digit (`M = M*10 + d` via `mul_u32(10)` then `add_u32(d)`).
const RT_BIGNUM_ADD_U32: &str = r#"  (func $__rt_bignum_add_u32 (param $ptr i32) (param $n i32) (param $k i64) (result i64) (local $i i32) (local $carry i64) (local $acc i64) (local $addr i32) ;; in-place a += k (k u32 in i64) over n limbs; returns final carry
    (local.set $i (i32.const 0))                                                 ;; limb index = 0
    (local.set $carry (local.get $k))                                           ;; running carry = k (first addend)
    (block $end                                                                 ;; add loop exit target
      (loop $top                                                                ;; iterate limbs low-to-high
        (br_if $end (i32.ge_u (local.get $i) (local.get $n)))                    ;; stop once i >= n
        (local.set $addr (i32.add (local.get $ptr) (i32.shl (local.get $i) (i32.const 2)))) ;; &limb[i] = ptr + i*4
        (local.set $acc (i64.add (i64.load32_u (local.get $addr)) (local.get $carry))) ;; acc = limb[i] + carry
        (i64.store32 (local.get $addr) (local.get $acc))                         ;; limb[i] = low 32 bits of acc
        (local.set $carry (i64.shr_u (local.get $acc) (i64.const 32)))           ;; carry = high 32 bits of acc
        (local.set $i (i32.add (local.get $i) (i32.const 1)))                    ;; i = i + 1
        (br $top))                                                              ;; continue the loop
    )                                                                           ;; end block $end
    (local.get $carry))                                                         ;; return the final carry
"#;

/// Copies `$n` little-endian limbs from `$src` to `$dst`.
///
/// A plain word-by-word copy (4 bytes per limb). Used by the S5c ratio rounder to take a
/// private copy of the numerator before the binary long-division loop mutates it as the
/// running remainder. Returns nothing.
const RT_BIGNUM_COPY: &str = r#"  (func $__rt_bignum_copy (param $dst i32) (param $src i32) (param $n i32) (local $i i32) (local $addr i32) ;; copy n limbs (4 bytes each) from src to dst
    (local.set $i (i32.const 0))                                                 ;; limb index = 0
    (block $end                                                                  ;; copy loop exit target
      (loop $top                                                                 ;; iterate limbs low-to-high
        (br_if $end (i32.ge_u (local.get $i) (local.get $n)))                    ;; stop once i >= n
        (local.set $addr (i32.add (local.get $src) (i32.shl (local.get $i) (i32.const 2)))) ;; &src[i] = src + i*4
        (i32.store (i32.add (local.get $dst) (i32.shl (local.get $i) (i32.const 2))) (i32.load (local.get $addr))) ;; dst[i] = src[i] (copy one limb)
        (local.set $i (i32.add (local.get $i) (i32.const 1)))                    ;; i = i + 1
        (br $top))                                                               ;; continue the loop
    )                                                                            ;; end block $end
  )                                                                              ;; end func
"#;

/// Returns the bit length of a fixed-width big integer (1-indexed count of bits).
///
/// Trims leading zero limbs top-down to find the effective length `ha`, then returns
/// `(ha-1)*32 + (32 - clz(top_limb))` (0 when the value is zero). Used by the S5c ratio
/// rounder to estimate `floor(log2(num/den))` so the numerator/denominator can be scaled
/// by powers of two into the `[2^52, 2^53)` mantissa window.
const RT_BIGNUM_BITLEN: &str = r#"  (func $__rt_bignum_bitlen (param $ptr i32) (param $n i32) (result i32) (local $ha i32) (local $v i32) ;; position of the highest set bit (1-indexed count), 0 if zero
    (local.set $ha (local.get $n))                                               ;; ha = n (start from the top limb)
    (block $ae                                                                   ;; trim leading zero limbs top-down
      (loop $al                                                                  ;; trim loop
        (br_if $ae (i32.eqz (local.get $ha)))                                    ;; ha==0 -> all limbs zero, stop
        (br_if $ae (i64.ne (i64.load32_u (i32.add (local.get $ptr) (i32.shl (i32.sub (local.get $ha) (i32.const 1)) (i32.const 2)))) (i64.const 0))) ;; top limb nonzero -> stop
        (local.set $ha (i32.sub (local.get $ha) (i32.const 1)))                  ;; drop zero top limb (ha--)
        (br $al))                                                                ;; continue the trim loop
    )                                                                            ;; end block $ae
    (if (i32.eqz (local.get $ha))                                                ;; all limbs zero?
      (then                                                                      ;; then branch: value is zero
        (return (i32.const 0))                                                   ;; zero value -> 0 bits
      )                                                                          ;; end then
    )                                                                            ;; end if
    (local.set $v (i32.wrap_i64 (i64.load32_u (i32.add (local.get $ptr) (i32.shl (i32.sub (local.get $ha) (i32.const 1)) (i32.const 2)))))) ;; v = top significant limb (i32, for clz)
    (return (i32.sub (i32.add (i32.shl (i32.sub (local.get $ha) (i32.const 1)) (i32.const 5)) (i32.const 32)) (i32.clz (local.get $v))))) ;; bitlen = (ha-1)*32 + (32 - clz(v))
"#;

/// Right-shifts a fixed-width big integer in place by exactly one bit.
///
/// Processes limbs low-to-high: each limb becomes `(limb[i] >> 1) | (lsb(limb[i+1]) << 31)`,
/// with the top limb taking no borrowed bit. Used by the S5c ratio rounder's binary
/// long-division loop, which starts from `den << 52` and halves the comparison target
/// each of the 53 iterations. Returns nothing.
const RT_BIGNUM_SHR1: &str = r#"  (func $__rt_bignum_shr1 (param $ptr i32) (param $n i32) (local $i i32) (local $cur i64) (local $nxt i64) (local $bit i64) (local $acc i64) (local $addr i32) ;; in-place right shift by 1 bit (low-to-high; borrows lsb of limb[i+1])
    (local.set $i (i32.const 0))                                                 ;; limb index = 0
    (block $end                                                                  ;; shift loop exit target
      (loop $top                                                                 ;; iterate limbs low-to-high
        (br_if $end (i32.ge_u (local.get $i) (local.get $n)))                    ;; stop once i >= n
        (local.set $addr (i32.add (local.get $ptr) (i32.shl (local.get $i) (i32.const 2)))) ;; &limb[i] = ptr + i*4
        (local.set $cur (i64.load32_u (local.get $addr)))                        ;; cur = limb[i]
        (local.set $nxt (select (i64.load32_u (i32.add (local.get $ptr) (i32.shl (i32.add (local.get $i) (i32.const 1)) (i32.const 2)))) (i64.const 0) (i32.lt_u (local.get $i) (i32.sub (local.get $n) (i32.const 1))))) ;; nxt = (i < n-1) ? limb[i+1] : 0
        (local.set $bit (i64.shl (i64.and (local.get $nxt) (i64.const 1)) (i64.const 31))) ;; bit = lsb of limb[i+1] shifted into bit 31
        (local.set $acc (i64.or (i64.shr_u (local.get $cur) (i64.const 1)) (local.get $bit))) ;; acc = (cur >> 1) | bit
        (i64.store32 (local.get $addr) (local.get $acc))                         ;; limb[i] = shifted value
        (local.set $i (i32.add (local.get $i) (i32.const 1)))                    ;; i = i + 1
        (br $top))                                                               ;; continue the loop
    )                                                                            ;; end block $end
  )                                                                              ;; end func
"#;

/// Zeroes `n` little-endian base-2^32 limbs at `ptr` (writes 0 to each). Used to clear
/// the fixed bignum buffers at the start of `__rt_digits_to_f64` so a repeated call in
/// the same program does not see stale limbs left by a prior parse (fresh linear memory
/// is zero-initialized, but only the first call can rely on that).
const RT_BIGNUM_ZERO: &str = r#"  (func $__rt_bignum_zero (param $ptr i32) (param $n i32) (local $i i32) (local $addr i32) ;; zero n limbs (write 0 to every limb)
    (local.set $i (i32.const 0))                                                 ;; limb index = 0
    (block $end                                                                  ;; zero loop exit target
      (loop $top                                                                 ;; iterate limbs low-to-high
        (br_if $end (i32.ge_u (local.get $i) (local.get $n)))                    ;; stop once i >= n
        (local.set $addr (i32.add (local.get $ptr) (i32.shl (local.get $i) (i32.const 2)))) ;; &limb[i] = ptr + i*4
        (i64.store32 (local.get $addr) (i64.const 0))                            ;; limb[i] = 0
        (local.set $i (i32.add (local.get $i) (i32.const 1)))                    ;; i = i + 1
        (br $top)                                                                ;; continue the loop
      )                                                                          ;; end loop $top
    )                                                                            ;; end block $end
  )                                                                              ;; end func
"#;

/// Correctly-rounded decimal string -> IEEE-754 double bits (the strtod core).
///
/// `(sign, ndig, K, digptr) -> i64` where the value is `M * 10^K` and `M` is the
/// `ndig` ASCII digit bytes at `digptr` (leading zeros preserved). Builds the exact
/// integer ratio `num/den` (absorbing `10^K` as `5^K * 2^K` into `num` for `K >= 0`
/// or into `den` for `K < 0`), normalizes it into the `[2^52, 2^53)` mantissa window by
/// powers of two, extracts the 53-bit significand via binary long division against
/// `den << 52`, rounds half-to-even, and assembles the bits. Trailing digits beyond
/// `KEEP = 400` are dropped (with `K` adjusted) since they fall below the double
/// subnormal ulp; magnitudes `>= 1e309` short-circuit to `+/-inf` and `< 1e-308` (the
/// deferred subnormal range) to `+/-0`. Bignums use 96-limb fixed buffers at
/// `0x4000`/`0x4400`/`0x4800`/`0x4C00` (zero-initialized).
const RT_DIGITS_TO_F64: &str = r#"  (func $__rt_digits_to_f64 (param $sign i32) (param $ndig i32) (param $K i32) (param $digptr i32) (param $scratch i32) (result i64) (local $i i32) (local $d i32) (local $a i32) (local $b i32) (local $dd i32) (local $cmp i32) (local $Q i64) (local $exp i32) (local $biased i32) (local $drop i32) (local $kc i32) ;; correctly-rounded decimal M*10^K -> f64 bits (sign,ndig,K,digptr)
    (call $__rt_bignum_zero (i32.add (local.get $scratch) (i32.const 0)) (i32.const 96)) ;; clear NUM (96 limbs) so a repeated call sees no stale mantissa
    (call $__rt_bignum_zero (i32.add (local.get $scratch) (i32.const 1024)) (i32.const 96)) ;; clear DEN (96 limbs) so a repeated call sees no stale denominator
    (if (i32.gt_s (local.get $ndig) (i32.const 400))                             ;; keep at most KEEP significant digits
      (then                                                                      ;; then: input longer than KEEP
        (local.set $drop (i32.sub (local.get $ndig) (i32.const 400)))            ;; drop = ndig - KEEP
        (local.set $K (i32.add (local.get $K) (local.get $drop)))                ;; K += drop (value preserved to ~2^-1074)
        (local.set $ndig (i32.const 400))                                        ;; ndig = KEEP
      )                                                                          ;; end then
    )                                                                            ;; end if
    (if (i32.eqz (local.get $ndig))                                              ;; no digits?
      (then                                                                      ;; then: zero magnitude
        (return (i64.shl (i64.extend_i32_u (local.get $sign)) (i64.const 63)))   ;; return +/-0 (sign << 63)
      )                                                                          ;; end then
    )                                                                            ;; end if
    (if (i32.ge_s (i32.add (i32.sub (local.get $ndig) (i32.const 1)) (local.get $K)) (i32.const 309)) ;; order of magnitude >= 1e309?
      (then                                                                      ;; then: overflow
        (return (i64.or (i64.shl (i64.extend_i32_u (local.get $sign)) (i64.const 63)) (i64.const 0x7FF0000000000000))) ;; return +/-inf
      )                                                                          ;; end then
    )                                                                            ;; end if
    (if (i32.le_s (i32.add (local.get $ndig) (local.get $K)) (i32.const -308))   ;; order of magnitude < 1e-308?
      (then                                                                      ;; then: underflow (subnormal -> 0)
        (return (i64.shl (i64.extend_i32_u (local.get $sign)) (i64.const 63)))   ;; return +/-0 (subnormals deferred)
      )                                                                          ;; end then
    )                                                                            ;; end if
    (local.set $i (i32.const 0))                                                 ;; digit index = 0
    (block $mb                                                                   ;; mantissa-build loop exit
      (loop $mt                                                                  ;; iterate digits low-to-high
        (br_if $mb (i32.ge_s (local.get $i) (local.get $ndig)))                  ;; stop once i >= ndig
        (local.set $d (i32.sub (i32.load8_u (i32.add (local.get $digptr) (local.get $i))) (i32.const 48))) ;; d = digit byte minus ASCII '0'
        (drop (call $__rt_bignum_mul_u32 (i32.add (local.get $scratch) (i32.const 0)) (i32.const 96) (i64.const 10))) ;; M = M * 10
        (drop (call $__rt_bignum_add_u32 (i32.add (local.get $scratch) (i32.const 0)) (i32.const 96) (i64.extend_i32_u (local.get $d)))) ;; M = M + d
        (local.set $i (i32.add (local.get $i) (i32.const 1)))                    ;; i = i + 1
        (br $mt)                                                                 ;; continue the mantissa-build loop
      )                                                                          ;; end loop $mt
    )                                                                            ;; end block $mb
    (if (call $__rt_bignum_is_zero (i32.add (local.get $scratch) (i32.const 0)) (i32.const 96)) ;; mantissa M == 0 ?
      (then (return (i64.shl (i64.extend_i32_u (local.get $sign)) (i64.const 63)))) ;; return +/-0
    )                                                                            ;; end if
    (if (i32.ge_s (local.get $K) (i32.const 0))                                  ;; K >= 0 ?
      (then                                                                      ;; then: K >= 0 -> num = M*5^K*2^K, den = 1
        (i32.store (i32.add (local.get $scratch) (i32.const 1024)) (i32.const 1)) ;; den = 1 (limb0 = 1, rest zero-init)
        (call $__rt_bignum_mul_small_n_times (i32.add (local.get $scratch) (i32.const 0)) (i32.const 96) (i64.const 5) (local.get $K)) ;; num *= 5^K
        (call $__rt_bignum_mul_small_n_times (i32.add (local.get $scratch) (i32.const 0)) (i32.const 96) (i64.const 2) (local.get $K)) ;; num *= 2^K (= *10^K total)
      )                                                                          ;; end then
      (else                                                                      ;; else: K < 0 -> den = 10^|K|, num = M
        (i32.store (i32.add (local.get $scratch) (i32.const 1024)) (i32.const 1)) ;; den = 1 (start)
        (local.set $kc (i32.sub (i32.const 0) (local.get $K)))                   ;; kc = |K| = -K
        (call $__rt_bignum_mul_small_n_times (i32.add (local.get $scratch) (i32.const 1024)) (i32.const 96) (i64.const 5) (local.get $kc)) ;; den *= 5^|K|
        (call $__rt_bignum_mul_small_n_times (i32.add (local.get $scratch) (i32.const 1024)) (i32.const 96) (i64.const 2) (local.get $kc)) ;; den *= 2^|K| (= 10^|K|)
      )                                                                          ;; end else
    )                                                                            ;; end if
    (local.set $a (i32.const 0))                                                 ;; num double-shifts a = 0
    (local.set $b (i32.const 0))                                                 ;; den double-shifts b = 0
    (local.set $dd (i32.sub (call $__rt_bignum_bitlen (i32.add (local.get $scratch) (i32.const 0)) (i32.const 96)) (call $__rt_bignum_bitlen (i32.add (local.get $scratch) (i32.const 1024)) (i32.const 96)))) ;; dd = bitlen(num) - bitlen(den) (~ floor log2 value)
    (if (i32.gt_s (local.get $dd) (i32.const 52))                                ;; dd > 52 ? (value too large)
      (then                                                                      ;; then: shrink quotient by scaling den up
        (local.set $b (i32.sub (local.get $dd) (i32.const 52)))                  ;; b = dd - 52
        (call $__rt_bignum_mul_small_n_times (i32.add (local.get $scratch) (i32.const 1024)) (i32.const 96) (i64.const 2) (local.get $b)) ;; den *= 2^b
      )                                                                          ;; end then
    )                                                                            ;; end if
    (if (i32.lt_s (local.get $dd) (i32.const 52))                                ;; dd < 52 ? (value too small)
      (then                                                                      ;; then: grow quotient by scaling num up
        (local.set $a (i32.sub (i32.const 52) (local.get $dd)))                  ;; a = 52 - dd
        (call $__rt_bignum_mul_small_n_times (i32.add (local.get $scratch) (i32.const 0)) (i32.const 96) (i64.const 2) (local.get $a)) ;; num *= 2^a
      )                                                                          ;; end then
    )                                                                            ;; end if
    (call $__rt_bignum_copy (i32.add (local.get $scratch) (i32.const 3072)) (i32.add (local.get $scratch) (i32.const 1024)) (i32.const 96)) ;; den_shl = copy(den)
    (call $__rt_bignum_mul_small_n_times (i32.add (local.get $scratch) (i32.const 3072)) (i32.const 96) (i64.const 2) (i32.const 52)) ;; den_shl <<= 52
    (if (i32.lt_s (call $__rt_bignum_cmp (i32.add (local.get $scratch) (i32.const 0)) (i32.const 96) (i32.add (local.get $scratch) (i32.const 3072)) (i32.const 96)) (i32.const 0)) ;; num < den_shl ? (Q would be < 2^52)
      (then                                                                      ;; then: scale num up once more
        (call $__rt_bignum_mul_small_n_times (i32.add (local.get $scratch) (i32.const 0)) (i32.const 96) (i64.const 2) (i32.const 1)) ;; num *= 2
        (local.set $a (i32.add (local.get $a) (i32.const 1)))                    ;; a += 1 (exp stays correct)
      )                                                                          ;; end then
    )                                                                            ;; end if
    (call $__rt_bignum_copy (i32.add (local.get $scratch) (i32.const 2048)) (i32.add (local.get $scratch) (i32.const 0)) (i32.const 96)) ;; work = copy(num) (running remainder)
    (local.set $Q (i64.const 0))                                                 ;; quotient Q = 0
    (local.set $i (i32.const 52))                                                ;; bit index = 52 (top of 53-bit mantissa)
    (block $ld                                                                   ;; long-division loop exit
      (loop $lt                                                                  ;; iterate bit index 52 down to 0
        (br_if $ld (i32.lt_s (local.get $i) (i32.const 0)))                      ;; stop once i < 0
        (if (i32.ge_s (call $__rt_bignum_cmp (i32.add (local.get $scratch) (i32.const 2048)) (i32.const 96) (i32.add (local.get $scratch) (i32.const 3072)) (i32.const 96)) (i32.const 0)) ;; work >= den_shl ?
          (then                                                                  ;; then: this bit of the quotient is 1
            (local.set $Q (i64.or (local.get $Q) (i64.shl (i64.const 1) (i64.extend_i32_u (local.get $i))))) ;; Q |= (1 << i)
            (drop (call $__rt_bignum_sub (i32.add (local.get $scratch) (i32.const 2048)) (i32.add (local.get $scratch) (i32.const 3072)) (i32.const 96))) ;; work -= den_shl
          )                                                                      ;; end then
        )                                                                        ;; end if
        (call $__rt_bignum_shr1 (i32.add (local.get $scratch) (i32.const 3072)) (i32.const 96)) ;; den_shl >>= 1 (next lower bit)
        (local.set $i (i32.sub (local.get $i) (i32.const 1)))                    ;; i = i - 1
        (br $lt)                                                                 ;; continue the long-division loop
      )                                                                          ;; end loop $lt
    )                                                                            ;; end block $ld
    (call $__rt_bignum_mul_small_n_times (i32.add (local.get $scratch) (i32.const 2048)) (i32.const 96) (i64.const 2) (i32.const 1)) ;; work = 2 * rem
    (local.set $cmp (call $__rt_bignum_cmp (i32.add (local.get $scratch) (i32.const 2048)) (i32.const 96) (i32.add (local.get $scratch) (i32.const 1024)) (i32.const 96))) ;; cmp = compare(2*rem, den)
    (if (i32.gt_s (local.get $cmp) (i32.const 0))                                ;; 2*rem > den ? (round up)
      (then (local.set $Q (i64.add (local.get $Q) (i64.const 1))))               ;; Q += 1 (round up)
    )                                                                            ;; end if
    (if (i32.and (i32.eq (local.get $cmp) (i32.const 0)) (i64.ne (i64.and (local.get $Q) (i64.const 1)) (i64.const 0))) ;; exact half and Q odd ? (round to even)
      (then (local.set $Q (i64.add (local.get $Q) (i64.const 1))))               ;; Q += 1 (round half to even)
    )                                                                            ;; end if
    (if (i64.eq (local.get $Q) (i64.const 0x20000000000000))                     ;; rounding carried Q to 2^53 ?
      (then                                                                      ;; then: normalize carry
        (local.set $Q (i64.const 0x10000000000000))                              ;; Q = 2^52
        (local.set $a (i32.sub (local.get $a) (i32.const 1)))                    ;; a -= 1 (exp += 1)
      )                                                                          ;; end then
    )                                                                            ;; end if
    (local.set $exp (i32.add (i32.const 52) (i32.sub (local.get $b) (local.get $a)))) ;; unbiased exp = 52 + (b - a)
    (local.set $biased (i32.add (local.get $exp) (i32.const 1023)))              ;; biased exp = exp + 1023
    (if (i32.ge_s (local.get $biased) (i32.const 2047))                          ;; biased >= 2047 ? (overflow to inf)
      (then (return (i64.or (i64.shl (i64.extend_i32_u (local.get $sign)) (i64.const 63)) (i64.const 0x7FF0000000000000)))) ;; return +/-inf
    )                                                                            ;; end if
    (if (i32.le_s (local.get $biased) (i32.const 0))                             ;; biased <= 0 ? (subnormal -> 0)
      (then (return (i64.shl (i64.extend_i32_u (local.get $sign)) (i64.const 63)))) ;; return +/-0 (subnormals deferred)
    )                                                                            ;; end if
    (i64.or (i64.or (i64.shl (i64.extend_i32_u (local.get $sign)) (i64.const 63)) (i64.shl (i64.extend_i32_u (local.get $biased)) (i64.const 52))) (i64.sub (local.get $Q) (i64.const 0x10000000000000))) ;; bits = (sign<<63) | (biased<<52) | (Q - 2^52)
  )                                                                              ;; end func
"#;

/// Parses a decimal string and stores its IEEE-754 double bits at `$out`.
///
/// `(ptr, len, out)` calls `__rt_parse_decimal` (digits written to the fixed `DIGBUF`
/// at `0x5000`) and dispatches on the class. PHP's string->float grammar only accepts
/// finite numeric syntax: the `inf`/`nan` tokens and empty/invalid strings (classes 1,
/// 2, 3) all yield `+0.0` (PHP returns `+0.0` even for `"-inf"`), so only the finite
/// branch (`__rt_digits_to_f64`) runs -- and it handles overflow to `+/-inf` internally.
/// The result is stored as an i64 at `*out` (the raw bit pattern; callers reinterpret it
/// as f64). This is the wasm32-wasi `strtod` equivalent used by `(float)`/`(int)`-of-
/// float-string casts.
const RT_STR_TO_F64: &str = r#"  (func $__rt_str_to_f64 (param $ptr i32) (param $len i32) (param $out i32) (param $scratch i32) (local $sign i32) (local $ndig i32) (local $K i32) (local $class i32) (local $bits i64) ;; parse a decimal string and store its f64 bits at out (scratch = bignum region base)
    (call $__rt_parse_decimal (local.get $ptr) (local.get $len) (i32.add (local.get $scratch) (i32.const 4096))) ;; parse_decimal -> sign,ndig,K,class (digits -> scratch+DIGBUF)
    (local.set $class)                                                           ;; pop class
    (local.set $K)                                                               ;; pop K
    (local.set $ndig)                                                            ;; pop ndig
    (local.set $sign)                                                            ;; pop sign
    (if (i32.eq (local.get $class) (i32.const 0))                                ;; class 0 (finite decimal)?
      (then (local.set $bits (call $__rt_digits_to_f64 (local.get $sign) (local.get $ndig) (local.get $K) (i32.add (local.get $scratch) (i32.const 4096)) (local.get $scratch)))) ;; bits = correctly-rounded finite M*10^K
      (else (local.set $bits (i64.const 0)))                                     ;; class 1/2/3 (invalid/inf/nan token) -> +0.0 (PHP non-numeric -> 0.0)
    )                                                                            ;; end if (finite vs non-numeric)
    (i64.store (local.get $out) (local.get $bits))                               ;; *(out) = bits
  )                                                                              ;; end func
"#;

/// `__rt_itoa`: convert a signed 64-bit integer to its PHP decimal string, written
/// into the caller-provided 21-byte scratch buffer `$out`, and return `(ptr, len)`
/// where `ptr` is the buffer start and `len` is the byte count (digits, plus one for a
/// '-' on negatives). The magnitude is taken as unsigned via `0 - value` (wrapping), so
/// `i64::MIN` becomes 2^63 and yields the correct 19 digits with a '-' prefix — matching
/// PHP's `(string)PHP_INT_MIN` = "-9223372036854775808". Zero is a single '0'. Unlike the
/// native `__rt_itoa` (which writes into the concat buffer), the wasm path writes into a
/// caller-provided `out` so the always-persist `cast_string` path can own the result.
const RT_ITOA: &str = r#"  (func $__rt_itoa (param $value i64) (param $out i32) (result i32 i32) (local $neg i32) (local $v i64) (local $tmp i64) (local $len i32) (local $i i32) (local $w i32) ;; signed i64 -> PHP decimal string at $out, returns (ptr,len)
    (if (i64.lt_s (local.get $value) (i64.const 0))                              ;; negative input?
      (then
        (local.set $neg (i32.const 1))                                           ;; neg flag = 1 (emit a leading '-')
        (local.set $v (i64.sub (i64.const 0) (local.get $value))))               ;; magnitude = -value (wrapping; INT64_MIN -> 2^63 unsigned)
      (else
        (local.set $neg (i32.const 0))                                           ;; non-negative: no sign
        (local.set $v (local.get $value))))                                      ;; magnitude = value
    (if (i64.eqz (local.get $v))                                                 ;; magnitude zero (input was 0)?
      (then
        (i32.store8 (local.get $out) (i32.const 48))                             ;; write '0'
        (return (local.get $out) (i32.const 1))))                                ;; return (out, 1) -> "0"
    (local.set $tmp (local.get $v))                                              ;; digit-counting copy of the magnitude
    (local.set $len (i32.const 0))                                               ;; digit count = 0
    (block $ce                                                                   ;; count-loop exit target
      (loop $cl                                                                  ;; count base-10 digits of the unsigned magnitude
        (br_if $ce (i64.eqz (local.get $tmp)))                                   ;; stop when the copy reaches 0
        (local.set $len (i32.add (local.get $len) (i32.const 1)))                ;; one more digit
        (local.set $tmp (i64.div_u (local.get $tmp) (i64.const 10)))             ;; drop the lowest digit
        (br $cl)))                                                               ;; continue counting
    (local.set $w (i32.const 0))                                                 ;; write offset (0, or 1 when a '-' is emitted)
    (if (local.get $neg)                                                         ;; negative?
      (then
        (i32.store8 (local.get $out) (i32.const 45))                             ;; write '-' at out+0
        (local.set $w (i32.const 1))))                                           ;; digits start at out+1
    (local.set $tmp (local.get $v))                                              ;; write copy of the magnitude
    (local.set $i (local.get $len))                                              ;; cursor = len (one past the last digit position)
    (block $we                                                                   ;; write-loop exit target
      (loop $wl                                                                  ;; write digits right-to-left into out+$w..out+$w+len-1
        (br_if $we (i32.eqz (local.get $i)))                                     ;; stop after writing all digits
        (local.set $i (i32.sub (local.get $i) (i32.const 1)))                    ;; pre-decrement to the current byte index
        (i32.store8                                                              ;; out+$w+$i = '0' + (magnitude mod 10)
          (i32.add (local.get $out) (i32.add (local.get $w) (local.get $i)))
          (i32.add (i32.const 48) (i32.wrap_i64 (i64.rem_u (local.get $tmp) (i64.const 10)))))
        (local.set $tmp (i64.div_u (local.get $tmp) (i64.const 10)))             ;; drop the digit just written
        (br $wl)))                                                               ;; continue writing
    (return (local.get $out) (i32.add (local.get $len) (local.get $neg))))       ;; return (out, digits + sign)
"#;

/// `__rt_str_to_int`: PHP `(int)$str` for a bounded string. Mirrors the native
/// `__rt_str_to_int` (strtoll + strtod) without libc: scan leading whitespace + an
/// optional sign, then accumulate integer digits with PHP saturating overflow to
/// `INT64_MAX`/`INT64_MIN` (the cap is 2^63 for negatives so `-INT64_MIN`'s magnitude
/// 2^63 is representable). A `.` or `e`/`E` right after the integer digits marks the
/// prefix float-form, in which case the string is parsed as a double via
/// `__rt_str_to_f64` and PHP's float->int rule is applied: finite -> `trunc_sat` toward
/// zero with saturation, but `±INF`/`NaN` -> `0` (matching `php -r`, where
/// `(int)"1e400"` / `(int)"INF"` / `(int)"NAN"` are all `0`, unlike `trunc_sat(±INF)`
/// which yields `±INT64_MAX`/`MIN`). Non-numeric / no-digit prefixes return `0`.
/// `scratch` is the float-scratch base (`$__float_scratch`); the parsed f64 bits land at
/// `scratch+10240` (clear of the strtod bignum buffers at 0..0x1400).
const RT_STR_TO_INT: &str = r#"  (func $__rt_str_to_int (param $ptr i32) (param $len i32) (param $scratch i32) (result i64) (local $i i32) (local $sign i32) (local $c i32) (local $val i64) (local $sat i32) (local $ndig i32) (local $float i32) (local $bits i64) (local $cap i64) (local $d i64) ;; parse a leading numeric string -> i64 (PHP (int)string)
    (block $wse                                                                  ;; whitespace-skip block
      (loop $wsl                                                                 ;; whitespace loop
        (br_if $wse (i32.ge_s (local.get $i) (local.get $len)))
        (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
        (if (i32.or (i32.and (i32.ge_s (local.get $c) (i32.const 9)) (i32.le_s (local.get $c) (i32.const 13))) (i32.eq (local.get $c) (i32.const 32)))
          (then (local.set $i (i32.add (local.get $i) (i32.const 1))) (br $wsl))
          (else (br $wse)))))
    (if (i32.lt_s (local.get $i) (local.get $len))                               ;; optional sign
      (then
        (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
        (if (i32.eq (local.get $c) (i32.const 45))
          (then (local.set $sign (i32.const 1)) (local.set $i (i32.add (local.get $i) (i32.const 1))))
          (else
            (if (i32.eq (local.get $c) (i32.const 43))
              (then (local.set $i (i32.add (local.get $i) (i32.const 1)))))))))
    (if (i32.eq (local.get $sign) (i32.const 1))                                 ;; saturation cap: 2^63 (neg) or 2^63-1 (pos)
      (then (local.set $cap (i64.const -9223372036854775808)))
      (else (local.set $cap (i64.const 9223372036854775807))))
    (block $ide                                                                  ;; integer-digit block
      (loop $idl                                                                 ;; integer-digit loop
        (br_if $ide (i32.ge_s (local.get $i) (local.get $len)))
        (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
        (br_if $ide (i32.or (i32.lt_s (local.get $c) (i32.const 48)) (i32.gt_s (local.get $c) (i32.const 57))))
        (local.set $ndig (i32.add (local.get $ndig) (i32.const 1)))
        (if (i32.eqz (local.get $sat))                                           ;; still within range?
          (then
            (local.set $d (i64.extend_i32_u (i32.sub (local.get $c) (i32.const 48))))
            (if (i64.gt_u (local.get $val) (i64.div_u (i64.sub (local.get $cap) (local.get $d)) (i64.const 10)))
              (then (local.set $sat (i32.const 1)) (local.set $val (local.get $cap))) ;; overflow -> clamp to the cap
              (else (local.set $val (i64.add (i64.mul (local.get $val) (i64.const 10)) (local.get $d)))))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $idl)))
    (if (i32.lt_s (local.get $i) (local.get $len))                               ;; float continuation? ('.' or 'e'/'E')
      (then
        (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
        (if (i32.eq (local.get $c) (i32.const 46))
          (then (local.set $float (i32.const 1)))
          (else
            (if (i32.or (i32.eq (local.get $c) (i32.const 101)) (i32.eq (local.get $c) (i32.const 69)))
              (then (local.set $float (i32.const 1))))))))
    (if (i32.eq (local.get $float) (i32.const 1))                                ;; float-form -> parse double, INF/NaN -> 0
      (then
        (call $__rt_str_to_f64 (local.get $ptr) (local.get $len) (i32.add (local.get $scratch) (i32.const 10240)) (local.get $scratch))
        (local.set $bits (i64.load (i32.add (local.get $scratch) (i32.const 10240))))
        (if (i32.eq (i32.and (i32.wrap_i64 (i64.shr_u (local.get $bits) (i64.const 52))) (i32.const 2047)) (i32.const 2047))
          (then (return (i64.const 0)))                                          ;; INF or NaN -> 0 (PHP (int)INF/NAN)
          (else (return (i64.trunc_sat_f64_s (f64.reinterpret_i64 (local.get $bits))))))) ;; finite -> truncate toward zero, saturate
      (else                                                                      ;; int-form: no digits -> 0, else apply sign
        (if (i32.eqz (local.get $ndig))
          (then (return (i64.const 0)))
          (else
            (if (i32.eq (local.get $sign) (i32.const 1))
              (then (return (i64.sub (i64.const 0) (local.get $val))))           ;; negate (wrapping for INT_MIN)
              (else (return (local.get $val))))))))
    (i64.const 0))
"#;

/// Registers the wasm32-wasi float<->string runtime helpers on `wm`.
///
/// Currently emits the full float<->string pipeline: the `__rt_f64_decompose` decoder,
/// the big-integer primitives, exact decimal digit extraction, round-to-14-significant,
/// the `__rt_ftoa_scientific`/`__rt_ftoa_fixed` formatters, the `__rt_ftoa` top-level
/// orchestrator, and the `__rt_str_to_f64` parser. The four strtod bignum buffers and
/// the digit buffer are no longer hardcoded: `__rt_digits_to_f64` and `__rt_str_to_f64`
/// take a `$scratch` base, and `base` is published as the immutable `$__float_scratch`
/// global so runtime callers (casts, echo, mixed stdout) pass
/// `(global.get $__float_scratch)`. Must be called before rendering any function that
/// references these symbols.
pub(super) fn emit_float_runtime(wm: &mut WatModule, base: i32) {
    wm.add_global(Global {
        name: "__float_scratch".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: base as i64,
    });
    wm.add_raw_func(RT_F64_DECOMPOSE);
    wm.add_raw_func(RT_BIGNUM_MUL_U32);
    wm.add_raw_func(RT_BIGNUM_DIVMOD_U32);
    wm.add_raw_func(RT_BIGNUM_MUL_SMALL_N_TIMES);
    wm.add_raw_func(RT_BIGNUM_IS_ZERO);
    wm.add_raw_func(RT_U32_TO_9DIGITS);
    wm.add_raw_func(RT_F64_DIGITS);
    wm.add_raw_func(RT_ROUND_DIGITS);
    wm.add_raw_func(RT_U32_TO_DEC);
    wm.add_raw_func(RT_FTOA_SCIENTIFIC);
    wm.add_raw_func(RT_FTOA_FIXED);
    wm.add_raw_func(RT_FTOA);
    wm.add_raw_func(RT_PARSE_DECIMAL);
    wm.add_raw_func(RT_BIGNUM_CMP);
    wm.add_raw_func(RT_BIGNUM_SUB);
    wm.add_raw_func(RT_BIGNUM_ADD_U32);
    wm.add_raw_func(RT_BIGNUM_COPY);
    wm.add_raw_func(RT_BIGNUM_BITLEN);
    wm.add_raw_func(RT_BIGNUM_SHR1);
    wm.add_raw_func(RT_BIGNUM_ZERO);
    wm.add_raw_func(RT_DIGITS_TO_F64);
    wm.add_raw_func(RT_STR_TO_F64);
    wm.add_raw_func(RT_ITOA);
    wm.add_raw_func(RT_STR_TO_INT);
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the wasm32-wasi float<->string runtime (`emit_float_runtime`).
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Each test builds a 1-page module with the float runtime plus a driver that
    //!   calls a routine and returns a single i64 witness, validates it with
    //!   `wasmparser`, and runs it under `wasmer`. Runs skip silently when `wasmer`
    //!   is absent. Inputs are raw f64 bit patterns; `__rt_f64_decompose` returns four
    //!   stacked results popped in reverse (class, exp2, mantissa, sign).

    use super::emit_float_runtime;
    use super::super::wat::WatModule;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique temp directory path so concurrent wasmer runs never collide.
    fn unique_tmp_dir() -> std::path::PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("elephc_wasm_float_{}_{}", std::process::id(), n))
    }

    /// Returns whether the `wasmer` CLI is available.
    fn wasmer_available() -> bool {
        std::process::Command::new("wasmer")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Builds a 1-page module with the float runtime and `driver`, validates it, and
    /// runs `export` under `wasmer`, returning trimmed stdout. `None` if wasmer is
    /// absent; WAT assembly and wasm validation always run.
    fn run_float_driver(driver: &str, export: &str) -> Option<String> {
        let mut wm = WatModule::new();
        wm.set_memory(1, Some("memory"));
        emit_float_runtime(&mut wm, 0x4000);
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

    /// Driver template: call `__rt_f64_decompose` on a raw bit pattern, pop the four
    /// results in reverse (class, exp2, mantissa, sign), and return `witness`.
    fn decompose_driver(bits_hex: &str, witness: &str) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
  (local $sign i32) (local $mant i64) (local $exp2 i32) (local $class i32)
  (call $__rt_f64_decompose (i64.const {bits_hex}))
  (local.set $class)
  (local.set $exp2)
  (local.set $mant)
  (local.set $sign)
  {witness})"#
        )
    }

    /// 2.0 decomposes to mantissa 2^52 = 4503599627370496 (frac 0 | implicit leading 1).
    #[test]
    fn normal_two_mantissa() {
        let d = decompose_driver("0x4000000000000000", "(local.get $mant)");
        if let Some(o) = run_float_driver(&d, "t") {
            assert_eq!(o, "4503599627370496");
        }
    }

    /// 2.0 = 2^52 * 2^-51, so exp2 = -51 (returned biased by +1000000 to stay positive).
    #[test]
    fn normal_two_exp2() {
        let d = decompose_driver(
            "0x4000000000000000",
            "(i64.add (i64.extend_i32_s (local.get $exp2)) (i64.const 1000000))",
        );
        if let Some(o) = run_float_driver(&d, "t") {
            assert_eq!(o, "999949");
        }
    }

    /// -1.0 has the sign bit set, so the reported sign is 1.
    #[test]
    fn negative_one_sign() {
        let d = decompose_driver("0xBFF0000000000000", "(i64.extend_i32_u (local.get $sign))");
        if let Some(o) = run_float_driver(&d, "t") {
            assert_eq!(o, "1");
        }
    }

    /// +0.0 is class 3 (zero).
    #[test]
    fn positive_zero_class() {
        let d = decompose_driver("0x0000000000000000", "(i64.extend_i32_u (local.get $class))");
        if let Some(o) = run_float_driver(&d, "t") {
            assert_eq!(o, "3");
        }
    }

    /// -0.0 reports sign 1 and class 3 (zero); witness is sign*10 + class = 13.
    #[test]
    fn negative_zero_sign_and_class() {
        let d = decompose_driver(
            "0x8000000000000000",
            "(i64.add (i64.mul (i64.extend_i32_u (local.get $sign)) (i64.const 10)) (i64.extend_i32_u (local.get $class)))",
        );
        if let Some(o) = run_float_driver(&d, "t") {
            assert_eq!(o, "13");
        }
    }

    /// +infinity is class 1.
    #[test]
    fn positive_infinity_class() {
        let d = decompose_driver("0x7FF0000000000000", "(i64.extend_i32_u (local.get $class))");
        if let Some(o) = run_float_driver(&d, "t") {
            assert_eq!(o, "1");
        }
    }

    /// A quiet NaN is class 2.
    #[test]
    fn nan_class() {
        let d = decompose_driver("0x7FF8000000000000", "(i64.extend_i32_u (local.get $class))");
        if let Some(o) = run_float_driver(&d, "t") {
            assert_eq!(o, "2");
        }
    }

    /// The smallest positive subnormal (bits 0x1) decomposes to mantissa 1 and
    /// exp2 -1074; witness = mantissa*1000000 + (exp2 + 1000000) = 1998926.
    #[test]
    fn min_subnormal_mantissa_and_exp2() {
        let d = decompose_driver(
            "0x0000000000000001",
            "(i64.add (i64.mul (local.get $mant) (i64.const 1000000)) (i64.add (i64.extend_i32_s (local.get $exp2)) (i64.const 1000000)))",
        );
        if let Some(o) = run_float_driver(&d, "t") {
            assert_eq!(o, "1998926");
        }
    }

    /// One limb [5] * 3 = 15 with no carry; witness = carry*1000 + limb0 = 15.
    #[test]
    fn mul_u32_basic() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $carry i64)
  (i64.store32 (i32.const 256) (i64.const 5))
  (local.set $carry (call $__rt_bignum_mul_u32 (i32.const 256) (i32.const 1) (i64.const 3)))
  (i64.add (i64.mul (local.get $carry) (i64.const 1000)) (i64.load32_u (i32.const 256))))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "15");
        }
    }

    /// One limb [0xFFFFFFFF] * 2 = 0x1FFFFFFFE: limb0 = 0xFFFFFFFE (4294967294), carry 1;
    /// witness = carry*10000000000 + limb0 = 14294967294.
    #[test]
    fn mul_u32_single_limb_carry() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $carry i64)
  (i64.store32 (i32.const 256) (i64.const 0xFFFFFFFF))
  (local.set $carry (call $__rt_bignum_mul_u32 (i32.const 256) (i32.const 1) (i64.const 2)))
  (i64.add (i64.mul (local.get $carry) (i64.const 10000000000)) (i64.load32_u (i32.const 256))))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "14294967294");
        }
    }

    /// Overflow safety: [0xFFFFFFFF] * 0xFFFFFFFF = (2^32-1)^2 = 0xFFFFFFFE00000001, so
    /// limb0 = 1 and carry = 0xFFFFFFFE (4294967294); witness = carry*10 + limb0 = 42949672941.
    #[test]
    fn mul_u32_max_factor() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $carry i64)
  (i64.store32 (i32.const 256) (i64.const 0xFFFFFFFF))
  (local.set $carry (call $__rt_bignum_mul_u32 (i32.const 256) (i32.const 1) (i64.const 0xFFFFFFFF)))
  (i64.add (i64.mul (local.get $carry) (i64.const 10)) (i64.load32_u (i32.const 256))))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "42949672941");
        }
    }

    /// Carry propagation across two limbs: [0xFFFFFFFF, 1] (= 2^33-1) * 2 = 2^34-2, so
    /// limb0 = 0xFFFFFFFE (4294967294), limb1 = 3, carry 0; witness = limb1*1e11 + limb0.
    #[test]
    fn mul_u32_two_limb_propagation() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $carry i64)
  (i64.store32 (i32.const 256) (i64.const 0xFFFFFFFF))
  (i64.store32 (i32.const 260) (i64.const 1))
  (local.set $carry (call $__rt_bignum_mul_u32 (i32.const 256) (i32.const 2) (i64.const 2)))
  (i64.add (i64.mul (i64.load32_u (i32.const 260)) (i64.const 100000000000)) (i64.load32_u (i32.const 256))))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "304294967294");
        }
    }

    /// Multiplying by 0 zeroes every limb and returns carry 0; witness = limb0+limb1+carry = 0.
    #[test]
    fn mul_u32_by_zero() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $carry i64)
  (i64.store32 (i32.const 256) (i64.const 123))
  (i64.store32 (i32.const 260) (i64.const 456))
  (local.set $carry (call $__rt_bignum_mul_u32 (i32.const 256) (i32.const 2) (i64.const 0)))
  (i64.add (i64.add (i64.load32_u (i32.const 256)) (i64.load32_u (i32.const 260))) (local.get $carry)))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }

    /// [15] / 4 = quotient 3 remainder 3; witness = rem*1000 + limb0 = 3003.
    #[test]
    fn divmod_u32_basic() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $rem i64)
  (i64.store32 (i32.const 256) (i64.const 15))
  (local.set $rem (call $__rt_bignum_divmod_u32 (i32.const 256) (i32.const 1) (i64.const 4)))
  (i64.add (i64.mul (local.get $rem) (i64.const 1000)) (i64.load32_u (i32.const 256))))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "3003");
        }
    }

    /// [100] / 10 = quotient 10 remainder 0; witness = rem*1000 + limb0 = 10.
    #[test]
    fn divmod_u32_exact() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $rem i64)
  (i64.store32 (i32.const 256) (i64.const 100))
  (local.set $rem (call $__rt_bignum_divmod_u32 (i32.const 256) (i32.const 1) (i64.const 10)))
  (i64.add (i64.mul (local.get $rem) (i64.const 1000)) (i64.load32_u (i32.const 256))))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "10");
        }
    }

    /// [12345] / 1 = quotient 12345 remainder 0; witness = rem*100000 + limb0 = 12345.
    #[test]
    fn divmod_u32_by_one() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $rem i64)
  (i64.store32 (i32.const 256) (i64.const 12345))
  (local.set $rem (call $__rt_bignum_divmod_u32 (i32.const 256) (i32.const 1) (i64.const 1)))
  (i64.add (i64.mul (local.get $rem) (i64.const 100000)) (i64.load32_u (i32.const 256))))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "12345");
        }
    }

    /// [5] / 10 = quotient 0 remainder 5; witness = rem*1000 + limb0 = 5000.
    #[test]
    fn divmod_u32_smaller_than_divisor() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $rem i64)
  (i64.store32 (i32.const 256) (i64.const 5))
  (local.set $rem (call $__rt_bignum_divmod_u32 (i32.const 256) (i32.const 1) (i64.const 10)))
  (i64.add (i64.mul (local.get $rem) (i64.const 1000)) (i64.load32_u (i32.const 256))))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "5000");
        }
    }

    /// Two-limb high->low remainder carry: [0, 1] (= 2^32 = 4294967296) / 7 = quotient
    /// 613566756 remainder 4 (the top limb divides to 0 with remainder 1, carried into the
    /// low limb). limb1 becomes 0; witness = rem*1000000000 + limb0 = 4613566756.
    #[test]
    fn divmod_u32_two_limb_remainder_carry() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $rem i64)
  (i64.store32 (i32.const 256) (i64.const 0))
  (i64.store32 (i32.const 260) (i64.const 1))
  (local.set $rem (call $__rt_bignum_divmod_u32 (i32.const 256) (i32.const 2) (i64.const 7)))
  (i64.add (i64.mul (local.get $rem) (i64.const 1000000000)) (i64.load32_u (i32.const 256))))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "4613566756");
        }
    }

    /// [1] multiplied by 10 five times = 100000 (linear memory is zero-initialized, so
    /// only limb0 is stored; the high limbs absorb carries). witness = limb0 = 100000.
    #[test]
    fn mul_ntimes_ten() {
        let driver = r#"(func $t (export "t") (result i64)
  (i64.store32 (i32.const 256) (i64.const 1))
  (call $__rt_bignum_mul_small_n_times (i32.const 256) (i32.const 4) (i64.const 10) (i32.const 5))
  (i64.load32_u (i32.const 256)))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "100000");
        }
    }

    /// [1] doubled 40 times = 2^40 = 1099511627776, spanning two limbs (limb0 = 0,
    /// limb1 = 256). witness reconstructs the i64 = (limb1 << 32) | limb0.
    #[test]
    fn mul_ntimes_two_pow40() {
        let driver = r#"(func $t (export "t") (result i64)
  (i64.store32 (i32.const 256) (i64.const 1))
  (call $__rt_bignum_mul_small_n_times (i32.const 256) (i32.const 3) (i64.const 2) (i32.const 40))
  (i64.or (i64.shl (i64.load32_u (i32.const 260)) (i64.const 32)) (i64.load32_u (i32.const 256))))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "1099511627776");
        }
    }

    /// count = 0 leaves the big integer unchanged. witness = limb0 = 7.
    #[test]
    fn mul_ntimes_zero_count() {
        let driver = r#"(func $t (export "t") (result i64)
  (i64.store32 (i32.const 256) (i64.const 7))
  (call $__rt_bignum_mul_small_n_times (i32.const 256) (i32.const 3) (i64.const 2) (i32.const 0))
  (i64.load32_u (i32.const 256)))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "7");
        }
    }

    /// [1] multiplied by 5 three times = 125 (the exp2<0 J-construction factor). witness = limb0.
    #[test]
    fn mul_ntimes_five() {
        let driver = r#"(func $t (export "t") (result i64)
  (i64.store32 (i32.const 256) (i64.const 1))
  (call $__rt_bignum_mul_small_n_times (i32.const 256) (i32.const 3) (i64.const 5) (i32.const 3))
  (i64.load32_u (i32.const 256)))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "125");
        }
    }

    /// An all-zero buffer (linear memory is zero-initialized) is zero; result = 1.
    #[test]
    fn is_zero_all_zero() {
        let driver = r#"(func $t (export "t") (result i64)
  (i64.extend_i32_u (call $__rt_bignum_is_zero (i32.const 256) (i32.const 4))))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "1");
        }
    }

    /// A non-zero low limb makes the integer non-zero; result = 0.
    #[test]
    fn is_zero_low_limb_set() {
        let driver = r#"(func $t (export "t") (result i64)
  (i64.store32 (i32.const 256) (i64.const 5))
  (i64.extend_i32_u (call $__rt_bignum_is_zero (i32.const 256) (i32.const 4))))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }

    /// A non-zero HIGH limb (limb 3 at byte 268) is detected by the full scan; result = 0.
    #[test]
    fn is_zero_high_limb_set() {
        let driver = r#"(func $t (export "t") (result i64)
  (i64.store32 (i32.const 268) (i64.const 1))
  (i64.extend_i32_u (call $__rt_bignum_is_zero (i32.const 256) (i32.const 4))))"#;
        if let Some(o) = run_float_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }

    /// Builds a WAT expression that reconstructs the 9-digit decimal number stored as
    /// ASCII bytes at `base..base+9` (most-significant first) via Horner's rule. Used to
    /// validate both digit ordering and zero-padding of `__rt_u32_to_9digits`.
    fn nine_digit_horner(base: u32) -> String {
        let digit = |off: u32| {
            format!(
                "(i64.extend_i32_u (i32.sub (i32.load8_u (i32.const {})) (i32.const 48)))",
                base + off
            )
        };
        let mut expr = digit(0);
        for off in 1..9 {
            expr = format!("(i64.add (i64.mul {expr} (i64.const 10)) {})", digit(off));
        }
        expr
    }

    /// 123456789 writes as the 9 digits "123456789" (MSB-first, no padding); the Horner
    /// reconstruction returns the original value.
    #[test]
    fn u32_to_9digits_full() {
        let driver = format!(
            r#"(func $t (export "t") (result i64)
  (call $__rt_u32_to_9digits (i32.const 123456789) (i32.const 256))
  {})"#,
            nine_digit_horner(256)
        );
        if let Some(o) = run_float_driver(&driver, "t") {
            assert_eq!(o, "123456789");
        }
    }

    /// 42 writes as "000000042" — leading zeros pad to 9 digits; Horner returns 42.
    #[test]
    fn u32_to_9digits_zero_padded() {
        let driver = format!(
            r#"(func $t (export "t") (result i64)
  (call $__rt_u32_to_9digits (i32.const 42) (i32.const 256))
  {})"#,
            nine_digit_horner(256)
        );
        if let Some(o) = run_float_driver(&driver, "t") {
            assert_eq!(o, "42");
        }
    }

    /// Builds a driver that runs `__rt_f64_digits` on a raw bit pattern (big scratch at
    /// 1024 with 80 limbs, digit buffer at 2048 with 768 bytes) and returns a witness
    /// `d0*1e8 + d1*1e6 + ndigits*1000 + p` pinning the leading two digits, the digit
    /// count, and the fractional digit count.
    fn f64_digits_driver(bits_hex: &str) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
  (local $sign i32) (local $class i32) (local $digptr i32) (local $ndig i32) (local $p i32)
  (call $__rt_f64_digits (i64.const {bits_hex}) (i32.const 1024) (i32.const 80) (i32.const 2048) (i32.const 768))
  (local.set $p)
  (local.set $ndig)
  (local.set $digptr)
  (local.set $class)
  (local.set $sign)
  (i64.add
    (i64.add
      (i64.mul (i64.extend_i32_u (i32.sub (i32.load8_u (local.get $digptr)) (i32.const 48))) (i64.const 100000000))
      (i64.mul (i64.extend_i32_u (i32.sub (i32.load8_u (i32.add (local.get $digptr) (i32.const 1))) (i32.const 48))) (i64.const 1000000)))
    (i64.add
      (i64.mul (i64.extend_i32_u (local.get $ndig)) (i64.const 1000))
      (i64.extend_i32_u (local.get $p)))))"#
        )
    }

    /// 2.0 -> J = 2*10^51 (digits "2" then 51 zeros, 52 digits), p = 51. Leading digit 2,
    /// second 0; witness = 2*1e8 + 0 + 52*1000 + 51 = 200052051.
    #[test]
    fn f64_digits_two() {
        if let Some(o) = run_float_driver(&f64_digits_driver("0x4000000000000000"), "t") {
            assert_eq!(o, "200052051");
        }
    }

    /// 0.5 -> J = 5*10^52 (53 digits), p = 53. Leading digit 5, second 0;
    /// witness = 5*1e8 + 0 + 53*1000 + 53 = 500053053.
    #[test]
    fn f64_digits_half() {
        if let Some(o) = run_float_driver(&f64_digits_driver("0x3FE0000000000000"), "t") {
            assert_eq!(o, "500053053");
        }
    }

    /// 1.5 -> J = 15*10^51 (digits "15" then 51 zeros, 53 digits), p = 52. Leading digits
    /// 1 and 5; witness = 1*1e8 + 5*1e6 + 53*1000 + 52 = 105053052.
    #[test]
    fn f64_digits_one_point_five() {
        if let Some(o) = run_float_driver(&f64_digits_driver("0x3FF8000000000000"), "t") {
            assert_eq!(o, "105053052");
        }
    }

    /// 100.0 -> J = 10^48 (digits "1" then 48 zeros, 49 digits), p = 46. Leading digit 1,
    /// second 0; witness = 1*1e8 + 0 + 49*1000 + 46 = 100049046.
    #[test]
    fn f64_digits_hundred() {
        if let Some(o) = run_float_driver(&f64_digits_driver("0x4059000000000000"), "t") {
            assert_eq!(o, "100049046");
        }
    }

    /// 2^60 exercises the exp2 >= 0 path: J = 2^60 = 1152921504606846976 (19 digits), p = 0.
    /// Leading digits 1 and 1; witness = 1*1e8 + 1*1e6 + 19*1000 + 0 = 101019000.
    #[test]
    fn f64_digits_two_pow60_integer_path() {
        if let Some(o) = run_float_driver(&f64_digits_driver("0x43B0000000000000"), "t") {
            assert_eq!(o, "101019000");
        }
    }

    /// Emits `i32.store8` instructions writing the ASCII bytes of `s` at `base..`.
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

    /// Builds a WAT expression reconstructing the `n`-digit decimal number stored as ASCII
    /// bytes at `base..base+n` (most-significant first) via Horner's rule.
    fn digits_value(base: u32, n: u32) -> String {
        let digit = |off: u32| {
            format!(
                "(i64.extend_i32_u (i32.sub (i32.load8_u (i32.const {})) (i32.const 48)))",
                base + off
            )
        };
        let mut expr = digit(0);
        for off in 1..n {
            expr = format!("(i64.add (i64.mul {expr} (i64.const 10)) {})", digit(off));
        }
        expr
    }

    /// Builds a driver that writes `input` digits at 256, rounds them to `prec` significant
    /// digits, and returns `flag*1000000 + value(first sig_len digits)` (flag is the
    /// overflow/exponent-shift result of `__rt_round_digits`).
    fn round_driver(input: &str, prec: u32, sig_len: u32) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
  (local $flag i32)
{stores}  (local.set $flag (call $__rt_round_digits (i32.const 256) (i32.const {ndig}) (i32.const {prec})))
  (i64.add (i64.mul (i64.extend_i32_u (local.get $flag)) (i64.const 1000000)) {horner}))"#,
            stores = store_ascii(256, input),
            ndig = input.len(),
            horner = digits_value(256, sig_len),
        )
    }

    /// ndigits <= prec: nothing is rounded, the digits are unchanged and the flag is 0.
    #[test]
    fn round_no_rounding_needed() {
        if let Some(o) = run_float_driver(&round_driver("125", 5, 3), "t") {
            assert_eq!(o, "125");
        }
    }

    /// First dropped digit < 5 truncates: "12342" -> "1234".
    #[test]
    fn round_truncate_down() {
        if let Some(o) = run_float_driver(&round_driver("12342", 4, 4), "t") {
            assert_eq!(o, "1234");
        }
    }

    /// First dropped digit > 5 rounds up: "12347" -> "1235".
    #[test]
    fn round_up_gt_five() {
        if let Some(o) = run_float_driver(&round_driver("12347", 4, 4), "t") {
            assert_eq!(o, "1235");
        }
    }

    /// Exactly half with even last kept digit rounds DOWN: "12345" -> "1234" ('4' is even).
    #[test]
    fn round_half_to_even_down() {
        if let Some(o) = run_float_driver(&round_driver("12345", 4, 4), "t") {
            assert_eq!(o, "1234");
        }
    }

    /// Exactly half with odd last kept digit rounds UP: "12355" -> "1236" ('5' is odd).
    #[test]
    fn round_half_to_even_up() {
        if let Some(o) = run_float_driver(&round_driver("12355", 4, 4), "t") {
            assert_eq!(o, "1236");
        }
    }

    /// A nonzero digit after the round digit forces round up regardless of evenness:
    /// "123451" -> "1235" (the trailing '1' makes it more than half).
    #[test]
    fn round_half_sticky_up() {
        if let Some(o) = run_float_driver(&round_driver("123451", 4, 4), "t") {
            assert_eq!(o, "1235");
        }
    }

    /// All-nines overflow: "9999" rounded to 3 sig digits becomes "100" and the flag is 1
    /// (exponent shifts up by one). witness = 1*1000000 + 100 = 1000100.
    #[test]
    fn round_overflow_all_nines() {
        if let Some(o) = run_float_driver(&round_driver("9999", 3, 3), "t") {
            assert_eq!(o, "1000100");
        }
    }

    /// Carry propagation across interior nines: "12995" -> "1300" (round up, 9s carry).
    #[test]
    fn round_carry_propagation() {
        if let Some(o) = run_float_driver(&round_driver("12995", 4, 4), "t") {
            assert_eq!(o, "1300");
        }
    }

    /// Builds a driver that writes the minimal decimal of `value` at 256, then returns
    /// `len*1000000 + value(first expect_len digits)` to check both the length and digits.
    fn u32_to_dec_driver(value: u32, expect_len: u32) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
  (local $len i32)
  (local.set $len (call $__rt_u32_to_dec (i32.const {value}) (i32.const 256)))
  (i64.add (i64.mul (i64.extend_i32_u (local.get $len)) (i64.const 1000000)) {horner}))"#,
            horner = digits_value(256, expect_len),
        )
    }

    /// 20 -> "20", length 2; witness = 2*1000000 + 20 = 2000020.
    #[test]
    fn u32_to_dec_two_digits() {
        if let Some(o) = run_float_driver(&u32_to_dec_driver(20, 2), "t") {
            assert_eq!(o, "2000020");
        }
    }

    /// 7 -> "7", length 1; witness = 1*1000000 + 7 = 1000007.
    #[test]
    fn u32_to_dec_single_digit() {
        if let Some(o) = run_float_driver(&u32_to_dec_driver(7, 1), "t") {
            assert_eq!(o, "1000007");
        }
    }

    /// 0 -> "0", length 1; witness = 1*1000000 + 0 = 1000000.
    #[test]
    fn u32_to_dec_zero() {
        if let Some(o) = run_float_driver(&u32_to_dec_driver(0, 1), "t") {
            assert_eq!(o, "1000000");
        }
    }

    /// 308 -> "308", length 3 (a typical large float exponent); witness = 3*1000000 + 308.
    #[test]
    fn u32_to_dec_three_digits() {
        if let Some(o) = run_float_driver(&u32_to_dec_driver(308, 3), "t") {
            assert_eq!(o, "3000308");
        }
    }

    /// 4294967295 (u32 max) -> "4294967295", length 10; witness = 10*1000000 + 4294967295.
    #[test]
    fn u32_to_dec_max() {
        if let Some(o) = run_float_driver(&u32_to_dec_driver(4294967295, 10), "t") {
            assert_eq!(o, "4304967295");
        }
    }

    /// Polynomial rolling hash of `s` (h = (h*257 + byte) mod 1e15), matching the WAT hash
    /// loop the formatter drivers run over the output bytes. Validates the exact output
    /// string (length and content) through a single i64 witness.
    fn str_hash(s: &str) -> u64 {
        let mut h: u64 = 0;
        for b in s.bytes() {
            h = (h.wrapping_mul(257).wrapping_add(b as u64)) % 1_000_000_000_000_000;
        }
        h
    }

    /// Builds a driver that writes `digits` at 256, formats them in scientific notation into
    /// a buffer at 512, and returns the rolling hash of the `len` output bytes (matching
    /// `str_hash`), validating the exact formatted string.
    fn sci_driver(digits: &str, nsig: u32, x: i32, sign: u32) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
  (local $len i32) (local $i i32) (local $h i64)
{stores}  (local.set $len (call $__rt_ftoa_scientific (i32.const 256) (i32.const {nsig}) (i32.const {x}) (i32.const {sign}) (i32.const 512)))
  (local.set $h (i64.const 0))
  (local.set $i (i32.const 0))
  (block $e
    (loop $l
      (br_if $e (i32.ge_s (local.get $i) (local.get $len)))
      (local.set $h (i64.rem_u (i64.add (i64.mul (local.get $h) (i64.const 257)) (i64.load8_u (i32.add (i32.const 512) (local.get $i)))) (i64.const 1000000000000000)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
  (local.get $h))"#,
            stores = store_ascii(256, digits),
        )
    }

    /// 1e20 -> "1.0E+20" (single significant digit, forced ".0", positive exponent "20").
    #[test]
    fn sci_one_e20() {
        if let Some(o) = run_float_driver(&sci_driver("1", 1, 20, 0), "t") {
            assert_eq!(o, str_hash("1.0E+20").to_string());
        }
    }

    /// 1.2345678901234E+14 -> all 14 significant digits with the fractional tail.
    #[test]
    fn sci_full_mantissa() {
        if let Some(o) = run_float_driver(&sci_driver("12345678901234", 14, 14, 0), "t") {
            assert_eq!(o, str_hash("1.2345678901234E+14").to_string());
        }
    }

    /// 1e-7 -> "1.0E-7": negative exponent with no leading zero (PHP, unlike C's "E-07").
    #[test]
    fn sci_negative_exponent() {
        if let Some(o) = run_float_driver(&sci_driver("1", 1, -7, 0), "t") {
            assert_eq!(o, str_hash("1.0E-7").to_string());
        }
    }

    /// -1.5E+20 -> negative number, two significant digits.
    #[test]
    fn sci_negative_sign() {
        if let Some(o) = run_float_driver(&sci_driver("15", 2, 20, 1), "t") {
            assert_eq!(o, str_hash("-1.5E+20").to_string());
        }
    }

    /// Like `sci_driver` but formats in fixed-point notation via `__rt_ftoa_fixed`.
    fn fixed_driver(digits: &str, nsig: u32, x: i32, sign: u32) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
  (local $len i32) (local $i i32) (local $h i64)
{stores}  (local.set $len (call $__rt_ftoa_fixed (i32.const 256) (i32.const {nsig}) (i32.const {x}) (i32.const {sign}) (i32.const 512)))
  (local.set $h (i64.const 0))
  (local.set $i (i32.const 0))
  (block $e
    (loop $l
      (br_if $e (i32.ge_s (local.get $i) (local.get $len)))
      (local.set $h (i64.rem_u (i64.add (i64.mul (local.get $h) (i64.const 257)) (i64.load8_u (i32.add (i32.const 512) (local.get $i)))) (i64.const 1000000000000000)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
  (local.get $h))"#,
            stores = store_ascii(256, digits),
        )
    }

    /// 100: one significant digit, exponent 2 -> "100" (significant digit then zero padding).
    #[test]
    fn fixed_integer_padded() {
        if let Some(o) = run_float_driver(&fixed_driver("1", 1, 2, 0), "t") {
            assert_eq!(o, str_hash("100").to_string());
        }
    }

    /// 12345.6789: nine significant digits, exponent 4 -> integer part "12345", fraction "6789".
    #[test]
    fn fixed_integer_and_fraction() {
        if let Some(o) = run_float_driver(&fixed_driver("123456789", 9, 4, 0), "t") {
            assert_eq!(o, str_hash("12345.6789").to_string());
        }
    }

    /// 2: single digit, exponent 0 -> "2" (no fraction, no padding).
    #[test]
    fn fixed_single_integer() {
        if let Some(o) = run_float_driver(&fixed_driver("2", 1, 0, 0), "t") {
            assert_eq!(o, str_hash("2").to_string());
        }
    }

    /// 0.1: exponent -1 -> "0.1" (no leading fractional zeros).
    #[test]
    fn fixed_leading_zero_point() {
        if let Some(o) = run_float_driver(&fixed_driver("1", 1, -1, 0), "t") {
            assert_eq!(o, str_hash("0.1").to_string());
        }
    }

    /// 0.0001: exponent -4 -> "0.0001" (three leading fractional zeros).
    #[test]
    fn fixed_many_leading_zeros() {
        if let Some(o) = run_float_driver(&fixed_driver("1", 1, -4, 0), "t") {
            assert_eq!(o, str_hash("0.0001").to_string());
        }
    }

    /// -0.5: negative fraction -> "-0.5".
    #[test]
    fn fixed_negative_fraction() {
        if let Some(o) = run_float_driver(&fixed_driver("5", 1, -1, 1), "t") {
            assert_eq!(o, str_hash("-0.5").to_string());
        }
    }

    /// 1000000: exponent 6 with one significant digit -> "1000000" (six zero pads).
    #[test]
    fn fixed_large_integer() {
        if let Some(o) = run_float_driver(&fixed_driver("1", 1, 6, 0), "t") {
            assert_eq!(o, str_hash("1000000").to_string());
        }
    }

    /// 12.5: three significant digits, exponent 1 -> integer "12", fraction "5".
    #[test]
    fn fixed_short_fraction() {
        if let Some(o) = run_float_driver(&fixed_driver("125", 3, 1, 0), "t") {
            assert_eq!(o, str_hash("12.5").to_string());
        }
    }

    /// Builds a driver that runs the full `__rt_ftoa` orchestrator on a raw f64 bit
    /// pattern (big scratch at 1024 with 80 limbs, digit buffer at 2048 with 768 bytes,
    /// output buffer at 4096) and returns the rolling hash of the `len` output bytes,
    /// matching `str_hash` for byte-exact validation of the formatted string.
    fn ftoa_driver(bits_hex: &str) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
  (local $ptr i32) (local $len i32) (local $i i32) (local $h i64)
  (call $__rt_ftoa (i64.const {bits_hex}) (i32.const 1024) (i32.const 80) (i32.const 2048) (i32.const 768) (i32.const 4096))
  (local.set $len)
  (local.set $ptr)
  (local.set $h (i64.const 0))
  (local.set $i (i32.const 0))
  (block $e
    (loop $l
      (br_if $e (i32.ge_s (local.get $i) (local.get $len)))
      (local.set $h (i64.rem_u (i64.add (i64.mul (local.get $h) (i64.const 257)) (i64.load8_u (i32.add (local.get $ptr) (local.get $i)))) (i64.const 1000000000000000)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
  (local.get $h))"#
        )
    }

    /// 2.0 -> "2".
    #[test]
    fn ftoa_two() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x4000000000000000"), "t") {
            assert_eq!(o, str_hash("2").to_string());
        }
    }

    /// 0.5 -> "0.5" (fixed notation with a single fractional digit).
    #[test]
    fn ftoa_half() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x3FE0000000000000"), "t") {
            assert_eq!(o, str_hash("0.5").to_string());
        }
    }

    /// 1.5 -> "1.5" (integer digit plus one fractional digit).
    #[test]
    fn ftoa_one_point_five() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x3FF8000000000000"), "t") {
            assert_eq!(o, str_hash("1.5").to_string());
        }
    }

    /// 100.0 -> "100" (integer with zero padding, fixed notation).
    #[test]
    fn ftoa_hundred() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x4059000000000000"), "t") {
            assert_eq!(o, str_hash("100").to_string());
        }
    }

    /// 0.1 -> "0.1" (exercises the exp2 < 0 exact-decimal path and rounding).
    #[test]
    fn ftoa_one_tenth() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x3FB999999999999A"), "t") {
            assert_eq!(o, str_hash("0.1").to_string());
        }
    }

    /// 1e20 -> "1.0E+20" (scientific: mantissa forced ".0", minimal exponent).
    #[test]
    fn ftoa_one_e20() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x4415AF1D78B58C40"), "t") {
            assert_eq!(o, str_hash("1.0E+20").to_string());
        }
    }

    /// 1e-7 -> "1.0E-7" (scientific with a negative, no-leading-zero exponent).
    #[test]
    fn ftoa_one_e_minus_seven() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x3E7AD7F29ABCAF48"), "t") {
            assert_eq!(o, str_hash("1.0E-7").to_string());
        }
    }

    /// 12345.6789 -> "12345.6789" (fixed notation with integer and fraction).
    #[test]
    fn ftoa_mixed_int_frac() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x40C81CD6E631F8A1"), "t") {
            assert_eq!(o, str_hash("12345.6789").to_string());
        }
    }

    /// 1e14 -> "1.0E+14" (boundary: X == 14 selects scientific notation).
    #[test]
    fn ftoa_one_e14_boundary() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x42D6BCC41E900000"), "t") {
            assert_eq!(o, str_hash("1.0E+14").to_string());
        }
    }

    /// 1e15 -> "1.0E+15" (X == 15, scientific).
    #[test]
    fn ftoa_one_e15() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x430C6BF526340000"), "t") {
            assert_eq!(o, str_hash("1.0E+15").to_string());
        }
    }

    /// 123456789012345.0 -> "1.2345678901234E+14" (14 significant digits, scientific).
    #[test]
    fn ftoa_fourteen_sig_digits() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x42DC12218377DE40"), "t") {
            assert_eq!(o, str_hash("1.2345678901234E+14").to_string());
        }
    }

    /// 0.0001 -> "0.0001" (fixed notation with three leading fractional zeros).
    #[test]
    fn ftoa_one_over_ten_thousand() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x3F1A36E2EB1C432D"), "t") {
            assert_eq!(o, str_hash("0.0001").to_string());
        }
    }

    /// +0.0 -> "0".
    #[test]
    fn ftoa_positive_zero() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x0000000000000000"), "t") {
            assert_eq!(o, str_hash("0").to_string());
        }
    }

    /// -0.0 -> "-0".
    #[test]
    fn ftoa_negative_zero() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x8000000000000000"), "t") {
            assert_eq!(o, str_hash("-0").to_string());
        }
    }

    /// +INF -> "INF".
    #[test]
    fn ftoa_positive_inf() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x7FF0000000000000"), "t") {
            assert_eq!(o, str_hash("INF").to_string());
        }
    }

    /// -INF -> "-INF".
    #[test]
    fn ftoa_negative_inf() {
        if let Some(o) = run_float_driver(&ftoa_driver("0xFFF0000000000000"), "t") {
            assert_eq!(o, str_hash("-INF").to_string());
        }
    }

    /// NaN -> "NAN" (never signed).
    #[test]
    fn ftoa_nan() {
        if let Some(o) = run_float_driver(&ftoa_driver("0x7FF8000000000000"), "t") {
            assert_eq!(o, str_hash("NAN").to_string());
        }
    }

    /// Builds a driver that calls `__rt_itoa(value, 512)` and returns the rolling hash of
    /// the `len` output bytes (matching `str_hash`), validating the exact decimal string.
    /// `value` is a decimal i64 literal (signed), so `i64::MIN` is written verbatim.
    fn itoa_driver(value: &str) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
  (local $ptr i32) (local $len i32) (local $i i32) (local $h i64)
  (call $__rt_itoa (i64.const {value}) (i32.const 512))
  (local.set $len)
  (local.set $ptr)
  (local.set $h (i64.const 0))
  (local.set $i (i32.const 0))
  (block $e
    (loop $l
      (br_if $e (i32.ge_s (local.get $i) (local.get $len)))
      (local.set $h (i64.rem_u (i64.add (i64.mul (local.get $h) (i64.const 257)) (i64.load8_u (i32.add (local.get $ptr) (local.get $i)))) (i64.const 1000000000000000)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
  (local.get $h))"#,
        )
    }

    /// 0 -> "0" (the zero special case writes a single '0').
    #[test]
    fn itoa_zero() {
        if let Some(o) = run_float_driver(&itoa_driver("0"), "t") {
            assert_eq!(o, str_hash("0").to_string());
        }
    }

    /// 1 -> "1" (single positive digit).
    #[test]
    fn itoa_one() {
        if let Some(o) = run_float_driver(&itoa_driver("1"), "t") {
            assert_eq!(o, str_hash("1").to_string());
        }
    }

    /// 42 -> "42" (multi-digit positive, written right-to-left into the buffer).
    #[test]
    fn itoa_positive_small() {
        if let Some(o) = run_float_driver(&itoa_driver("42"), "t") {
            assert_eq!(o, str_hash("42").to_string());
        }
    }

    /// 1000000 -> "1000000" (seven digits, exercises the count + write loops).
    #[test]
    fn itoa_positive_large() {
        if let Some(o) = run_float_driver(&itoa_driver("1000000"), "t") {
            assert_eq!(o, str_hash("1000000").to_string());
        }
    }

    /// -5 -> "-5" (negative: '-' prefix then magnitude digits).
    #[test]
    fn itoa_negative_small() {
        if let Some(o) = run_float_driver(&itoa_driver("-5"), "t") {
            assert_eq!(o, str_hash("-5").to_string());
        }
    }

    /// -1 -> "-1" (negative single digit).
    #[test]
    fn itoa_negative_one() {
        if let Some(o) = run_float_driver(&itoa_driver("-1"), "t") {
            assert_eq!(o, str_hash("-1").to_string());
        }
    }

    /// PHP_INT_MAX 9223372036854775807 -> "9223372036854775807" (19 digits, positive).
    #[test]
    fn itoa_int_max() {
        if let Some(o) = run_float_driver(&itoa_driver("9223372036854775807"), "t") {
            assert_eq!(o, str_hash("9223372036854775807").to_string());
        }
    }

    /// PHP_INT_MIN -9223372036854775808 -> "-9223372036854775808". The wrapping negate
    /// `0 - INT64_MIN` yields 2^63 as an unsigned magnitude, so `div_u`/`rem_u` produce
    /// the correct 19 digits and the '-' is prepended (verified vs `php -r`).
    #[test]
    fn itoa_int_min() {
        if let Some(o) = run_float_driver(&itoa_driver("-9223372036854775808"), "t") {
            assert_eq!(o, str_hash("-9223372036854775808").to_string());
        }
    }

    /// Builds a driver that stores `s` at 512 and calls
    /// `__rt_str_to_int(512, s.len(), $__float_scratch)`, returning the i64 PHP `(int)$s`
    /// (wasmer prints the signed decimal). Used by the table-driven parity test below.
    fn str_to_int_driver(s: &str) -> String {
        format!(
            r#"(func $t (export "t") (result i64)
{stores}  (call $__rt_str_to_int (i32.const 512) (i32.const {len}) (global.get $__float_scratch)))"#,
            stores = store_ascii(512, s),
            len = s.len(),
        )
    }

    /// `(int)$str` parity vs `php -r`: integer-form (saturating), float-form (truncate),
    /// `±INF`/`NaN`/overflow-to-`±INF` -> 0, whitespace, sign, leading zeros, hex-reject,
    /// trailing garbage. Each row is `(input, expected)`. Verified against PHP 8.5.
    #[test]
    fn str_to_int_parity() {
        const CASES: &[(&str, &str)] = &[
            ("10", "10"),
            ("-10", "-10"),
            ("+10", "10"),
            ("0", "0"),
            ("00010", "10"),
            ("123abc", "123"),
            ("123abc456", "123"),
            ("abc", "0"),
            ("0x1A", "0"),
            ("  123", "123"),
            ("  -45  ", "-45"),
            ("", "0"),
            ("  ", "0"),
            ("9223372036854775807", "9223372036854775807"),
            ("9223372036854775808", "9223372036854775807"),
            ("-9223372036854775808", "-9223372036854775808"),
            ("-9223372036854775809", "-9223372036854775808"),
            ("999999999999999999999", "9223372036854775807"),
            ("-999999999999999999999", "-9223372036854775808"),
            ("1e3", "1000"),
            ("1.9", "1"),
            ("1.9e2", "190"),
            ("1.5E2", "150"),
            ("123e2", "12300"),
            ("2e-3", "0"),
            (".5", "0"),
            ("-.5", "0"),
            ("1.", "1"),
            (".e1", "0"),
            ("1e20", "9223372036854775807"),
            ("1.5e308", "9223372036854775807"),
            ("1.7976931348623e308", "9223372036854775807"),
            ("9223372036854775807.0", "9223372036854775807"),
            ("1e400", "0"),
            ("-1e400", "0"),
            ("INF", "0"),
            ("+INF", "0"),
            ("-INF", "0"),
            ("NAN", "0"),
        ];
        let mut failed = String::new();
        for &(input, expected) in CASES {
            if let Some(o) = run_float_driver(&str_to_int_driver(input), "t") {
                if o != expected {
                    failed.push_str(&format!("  ({:?}, got {}, want {})\n", input, o, expected));
                }
            }
        }
        assert!(failed.is_empty(), "str_to_int parity failures vs php:\n{failed}");
    }

    /// Builds a driver that stores two little-endian base-2^32 big integers (`a` at 256,
    /// `b` at 4096) and calls `__rt_bignum_cmp(256, a.len, 4096, b.len)`, returning the
    /// signed i32 result (-1/0/1) extended to i64 so wasmer prints `-1`/`0`/`1`.
    fn bignum_cmp_driver(a: &[u32], b: &[u32]) -> String {
        let mut stores = String::new();
        for (i, &v) in a.iter().enumerate() {
            stores.push_str(&format!(
                "  (i64.store32 (i32.const {}) (i64.const {}))\n",
                256 + i * 4,
                v
            ));
        }
        for (i, &v) in b.iter().enumerate() {
            stores.push_str(&format!(
                "  (i64.store32 (i32.const {}) (i64.const {}))\n",
                4096 + i * 4,
                v
            ));
        }
        format!(
            r#"(func $t (export "t") (result i64)
{stores}  (i64.extend_i32_s (call $__rt_bignum_cmp (i32.const 256) (i32.const {na}) (i32.const 4096) (i32.const {nb}))))"#,
            na = a.len(),
            nb = b.len()
        )
    }

    /// `[5]` vs `[5]` -> equal -> 0.
    #[test]
    fn bignum_cmp_equal_single_limb() {
        if let Some(o) = run_float_driver(&bignum_cmp_driver(&[5], &[5]), "t") {
            assert_eq!(o, "0");
        }
    }

    /// `[6]` vs `[5]` -> greater -> 1.
    #[test]
    fn bignum_cmp_greater_single_limb() {
        if let Some(o) = run_float_driver(&bignum_cmp_driver(&[6], &[5]), "t") {
            assert_eq!(o, "1");
        }
    }

    /// `[5]` vs `[6]` -> less -> -1.
    #[test]
    fn bignum_cmp_less_single_limb() {
        if let Some(o) = run_float_driver(&bignum_cmp_driver(&[5], &[6]), "t") {
            assert_eq!(o, "-1");
        }
    }

    /// `[1,1]` (0x1_00000001) vs `[0xFFFFFFFF]` -> longer is bigger -> 1.
    #[test]
    fn bignum_cmp_longer_is_greater() {
        if let Some(o) = run_float_driver(&bignum_cmp_driver(&[1, 1], &[0xFFFFFFFF]), "t") {
            assert_eq!(o, "1");
        }
    }

    /// Leading zero limbs are trimmed: `[5,0,0]` and `[5]` are both `[5]` -> 0.
    #[test]
    fn bignum_cmp_trims_leading_zeros() {
        if let Some(o) = run_float_driver(&bignum_cmp_driver(&[5, 0, 0], &[5]), "t") {
            assert_eq!(o, "0");
        }
    }

    /// Both zero -> 0.
    #[test]
    fn bignum_cmp_both_zero() {
        if let Some(o) = run_float_driver(&bignum_cmp_driver(&[0], &[0]), "t") {
            assert_eq!(o, "0");
        }
    }

    /// `[0xFFFFFFFF,0]` trims to `[0xFFFFFFFF]` (1 limb) vs `[0,1]` (2 limbs) -> shorter -> -1.
    #[test]
    fn bignum_cmp_trimmed_shorter_is_less() {
        if let Some(o) = run_float_driver(&bignum_cmp_driver(&[0xFFFFFFFF, 0], &[0, 1]), "t") {
            assert_eq!(o, "-1");
        }
    }

    /// Equal length, top limb decides: `[0,1]` vs `[1,1]` -> 0<1 at top -> -1.
    #[test]
    fn bignum_cmp_top_limb_decides() {
        if let Some(o) = run_float_driver(&bignum_cmp_driver(&[0, 1], &[1, 1]), "t") {
            assert_eq!(o, "-1");
        }
    }

    /// Equal length, low limb decides: `[1,5]` vs `[1,4]` -> top equal, low 5>4 -> 1.
    #[test]
    fn bignum_cmp_low_limb_decides() {
        if let Some(o) = run_float_driver(&bignum_cmp_driver(&[1, 5], &[1, 4]), "t") {
            assert_eq!(o, "1");
        }
    }

    /// Builds a driver that stores two big integers, calls `__rt_bignum_sub(a, b, n)`
    /// in place, and returns `borrow * 10^9 + limb[watch]` (borrow is the final i64
    /// borrow, limb[watch] is the watched result limb).
    fn bignum_sub_driver(a: &[u32], b: &[u32], watch: usize) -> String {
        let n = a.len().max(b.len());
        let mut stores = String::new();
        for (i, &v) in a.iter().enumerate() {
            stores.push_str(&format!(
                "  (i64.store32 (i32.const {}) (i64.const {}))\n",
                256 + i * 4,
                v
            ));
        }
        for (i, &v) in b.iter().enumerate() {
            stores.push_str(&format!(
                "  (i64.store32 (i32.const {}) (i64.const {}))\n",
                4096 + i * 4,
                v
            ));
        }
        format!(
            r#"(func $t (export "t") (result i64)
  (local $borrow i64)
{stores}  (local.set $borrow (call $__rt_bignum_sub (i32.const 256) (i32.const 4096) (i32.const {n})))
  (i64.add (i64.mul (local.get $borrow) (i64.const 1000000000)) (i64.load32_u (i32.const {watch_addr}))))"#,
            n = n,
            watch_addr = 256 + watch * 4
        )
    }

    /// [10] - [3] = [7], no borrow -> 7.
    #[test]
    fn bignum_sub_simple_no_borrow() {
        if let Some(o) = run_float_driver(&bignum_sub_driver(&[10], &[3], 0), "t") {
            assert_eq!(o, "7");
        }
    }

    /// [3] - [10] underflows: limb0 = (3-10) mod 2^32 = 0xFFFFFFF9 = 4294967289, borrow 1.
    #[test]
    fn bignum_sub_underflow_single_limb() {
        if let Some(o) = run_float_driver(&bignum_sub_driver(&[3], &[10], 0), "t") {
            assert_eq!(o, "5294967289");
        }
    }

    /// [2^32] - [1] = [0xFFFFFFFF, 0]: low limb borrows, high limb clears -> 4294967295.
    #[test]
    fn bignum_sub_multi_limb_borrow() {
        if let Some(o) = run_float_driver(&bignum_sub_driver(&[0, 1], &[1, 0], 0), "t") {
            assert_eq!(o, "4294967295");
        }
    }

    /// [5,5] - [6,5]: low underflows (0xFFFFFFFF), borrow propagates, high = 5-5-1 = -1
    /// -> 0xFFFFFFFF with final borrow 1 -> 1*10^9 + 4294967295 = 5294967295.
    #[test]
    fn bignum_sub_borrow_propagation() {
        if let Some(o) = run_float_driver(&bignum_sub_driver(&[5, 5], &[6, 5], 0), "t") {
            assert_eq!(o, "5294967295");
        }
    }

    /// Builds a driver that stores `a` at 256, calls `__rt_bignum_add_u32(256, n, k)`, and
    /// returns `(carry << 32) | limb[watch]` so both the propagated carry and the resulting
    /// low limb are observable.
    fn bignum_add_u32_driver(a: &[u32], k: u32, watch: usize) -> String {
        let mut stores = String::new();
        for (i, &v) in a.iter().enumerate() {
            stores.push_str(&format!(
                "  (i64.store32 (i32.const {}) (i64.const {}))\n",
                256 + i * 4,
                v
            ));
        }
        format!(
            r#"(func $t (export "t") (result i64)
{stores}  (i64.or
    (i64.shl (call $__rt_bignum_add_u32 (i32.const 256) (i32.const {n}) (i64.const {k})) (i64.const 32))
    (i64.load32_u (i32.const {watch_addr}))))"#,
            n = a.len(),
            k = k,
            watch_addr = 256 + watch * 4
        )
    }

    /// `[5] + 0` -> carry 0, limb 5 -> `0<<32 | 5` = 5.
    #[test]
    fn bignum_add_u32_zero_addend() {
        if let Some(o) = run_float_driver(&bignum_add_u32_driver(&[5], 0, 0), "t") {
            assert_eq!(o, "5");
        }
    }

    /// `[5] + 3` -> 8, no carry -> 8.
    #[test]
    fn bignum_add_u32_single_limb() {
        if let Some(o) = run_float_driver(&bignum_add_u32_driver(&[5], 3, 0), "t") {
            assert_eq!(o, "8");
        }
    }

    /// `[0xFFFFFFFF] + 1` -> limb 0, carry 1 -> `1<<32 | 0` = 4294967296.
    #[test]
    fn bignum_add_u32_overflow_carry() {
        if let Some(o) = run_float_driver(&bignum_add_u32_driver(&[0xFFFFFFFF], 1, 0), "t") {
            assert_eq!(o, "4294967296");
        }
    }

    /// `[0xFFFFFFFF, 0xFFFFFFFF] + 1` -> limb0 0, limb1 0, carry 1 (propagated through
    /// both limbs) -> `1<<32 | 0` = 4294967296 (watching limb0).
    #[test]
    fn bignum_add_u32_multi_limb_carry() {
        if let Some(o) = run_float_driver(&bignum_add_u32_driver(&[0xFFFFFFFF, 0xFFFFFFFF], 1, 0), "t") {
            assert_eq!(o, "4294967296");
        }
    }

    /// Builds a driver that stores `src` at 256, copies it to 4096, and returns the copied
    /// limb at `4096 + watch*4`.
    fn bignum_copy_driver(src: &[u32], watch: usize) -> String {
        let mut stores = String::new();
        for (i, &v) in src.iter().enumerate() {
            stores.push_str(&format!(
                "  (i64.store32 (i32.const {}) (i64.const {}))\n",
                256 + i * 4,
                v
            ));
        }
        format!(
            r#"(func $t (export "t") (result i64)
{stores}  (call $__rt_bignum_copy (i32.const 4096) (i32.const 256) (i32.const {n}))
  (i64.load32_u (i32.const {watch_addr})))"#,
            n = src.len(),
            watch_addr = 4096 + watch * 4
        )
    }

    /// Copy `[10, 20, 30]`, watch limb 1 -> 20.
    #[test]
    fn bignum_copy_middle_limb() {
        if let Some(o) = run_float_driver(&bignum_copy_driver(&[10, 20, 30], 1), "t") {
            assert_eq!(o, "20");
        }
    }

    /// Copy a full 32-bit limb across the boundary.
    #[test]
    fn bignum_copy_full_limb() {
        if let Some(o) = run_float_driver(&bignum_copy_driver(&[0xDEADBEEF], 0), "t") {
            assert_eq!(o, "3735928559");
        }
    }

    /// Builds a driver that stores `a` at 256 and returns `__rt_bignum_bitlen(256, n)`.
    fn bignum_bitlen_driver(a: &[u32]) -> String {
        let mut stores = String::new();
        for (i, &v) in a.iter().enumerate() {
            stores.push_str(&format!(
                "  (i64.store32 (i32.const {}) (i64.const {}))\n",
                256 + i * 4,
                v
            ));
        }
        format!(
            r#"(func $t (export "t") (result i64)
{stores}  (i64.extend_i32_u (call $__rt_bignum_bitlen (i32.const 256) (i32.const {n}))))"#,
            n = a.len()
        )
    }

    /// Zero -> 0 bits.
    #[test]
    fn bignum_bitlen_zero() {
        if let Some(o) = run_float_driver(&bignum_bitlen_driver(&[0]), "t") {
            assert_eq!(o, "0");
        }
    }

    /// `[1]` -> 1 bit.
    #[test]
    fn bignum_bitlen_one() {
        if let Some(o) = run_float_driver(&bignum_bitlen_driver(&[1]), "t") {
            assert_eq!(o, "1");
        }
    }

    /// `[0xFFFFFFFF]` -> 32 bits.
    #[test]
    fn bignum_bitlen_full_limb() {
        if let Some(o) = run_float_driver(&bignum_bitlen_driver(&[0xFFFFFFFF]), "t") {
            assert_eq!(o, "32");
        }
    }

    /// `[0, 1]` = 2^32 -> 33 bits; leading-zero trim does not apply (top limb nonzero).
    #[test]
    fn bignum_bitlen_two_limbs() {
        if let Some(o) = run_float_driver(&bignum_bitlen_driver(&[0, 1]), "t") {
            assert_eq!(o, "33");
        }
    }

    /// `[0, 0, 0]` trims to zero -> 0 bits.
    #[test]
    fn bignum_bitlen_all_zero_trimmed() {
        if let Some(o) = run_float_driver(&bignum_bitlen_driver(&[0, 0, 0]), "t") {
            assert_eq!(o, "0");
        }
    }

    /// `[0xFFFFFFFF, 0xFFFFFFFF]` -> 64 bits.
    #[test]
    fn bignum_bitlen_two_full_limbs() {
        if let Some(o) = run_float_driver(&bignum_bitlen_driver(&[0xFFFFFFFF, 0xFFFFFFFF]), "t") {
            assert_eq!(o, "64");
        }
    }

    /// Builds a driver that stores `a` at 256, right-shifts it by 1, and returns
    /// `limb[watch]` after the shift.
    fn bignum_shr1_driver(a: &[u32], watch: usize) -> String {
        let mut stores = String::new();
        for (i, &v) in a.iter().enumerate() {
            stores.push_str(&format!(
                "  (i64.store32 (i32.const {}) (i64.const {}))\n",
                256 + i * 4,
                v
            ));
        }
        format!(
            r#"(func $t (export "t") (result i64)
{stores}  (call $__rt_bignum_shr1 (i32.const 256) (i32.const {n}))
  (i64.load32_u (i32.const {watch_addr})))"#,
            n = a.len(),
            watch_addr = 256 + watch * 4
        )
    }

    /// `[4]` >> 1 -> `[2]` -> 2.
    #[test]
    fn bignum_shr1_even_single() {
        if let Some(o) = run_float_driver(&bignum_shr1_driver(&[4], 0), "t") {
            assert_eq!(o, "2");
        }
    }

    /// `[3]` >> 1 -> `[1]` (truncate) -> 1.
    #[test]
    fn bignum_shr1_truncate_single() {
        if let Some(o) = run_float_driver(&bignum_shr1_driver(&[3], 0), "t") {
            assert_eq!(o, "1");
        }
    }

    /// `[0, 1]` (2^32) >> 1 -> 2^31: limb0 = 0x80000000 = 2147483648, limb1 = 0.
    #[test]
    fn bignum_shr1_borrows_into_low_limb() {
        if let Some(o) = run_float_driver(&bignum_shr1_driver(&[0, 1], 0), "t") {
            assert_eq!(o, "2147483648");
        }
    }

    /// `[0, 0xFFFFFFFF]` (0xFFFFFFFF_00000000) >> 1 -> 0x7FFFFFFF_80000000: limb1 = 0x7FFFFFFF.
    #[test]
    fn bignum_shr1_top_limb_shifts() {
        if let Some(o) = run_float_driver(&bignum_shr1_driver(&[0, 0xFFFFFFFF], 1), "t") {
            assert_eq!(o, "2147483647");
        }
    }

    /// Builds a driver that writes the ASCII bytes of `s` at 256, calls
    /// `__rt_parse_decimal` on it with the digit out-buffer at 512, and returns a witness
    /// `sign*10^15 + ndig*10^12 + (K+32768)*1000 + class`. K is biased by 32768 so it stays
    /// non-negative for any realistic decimal exponent; the parsed digits at 512 are
    /// validated separately by the digit-hash tests below.
    fn parse_driver(s: &str) -> String {
        let stores = store_ascii(256, s);
        format!(
            r#"(func $t (export "t") (result i64)
  (local $sign i32) (local $ndig i32) (local $K i32) (local $class i32)
{stores}  (call $__rt_parse_decimal (i32.const 256) (i32.const {len}) (i32.const 512))
  (local.set $class)
  (local.set $K)
  (local.set $ndig)
  (local.set $sign)
  (i64.add
    (i64.add
      (i64.mul (i64.extend_i32_u (local.get $sign)) (i64.const 1000000000000000))
      (i64.mul (i64.extend_i32_u (local.get $ndig)) (i64.const 1000000000000)))
    (i64.add
      (i64.mul (i64.extend_i32_u (i32.add (local.get $K) (i32.const 32768))) (i64.const 1000))
      (i64.extend_i32_u (local.get $class)))))"#,
            len = s.len(),
        )
    }

    /// Reconstructs the witness the driver returns for a given parse result.
    fn parse_witness(sign: u32, ndig: u32, k: i32, class: u32) -> u64 {
        (sign as u64) * 1_000_000_000_000_000
            + (ndig as u64) * 1_000_000_000_000
            + ((k + 32768) as u64) * 1000
            + class as u64
    }

    /// "1e3" -> sign 0, ndig 1, K 3, class 0 (value = 1 * 10^3 = 1000).
    #[test]
    fn parse_one_e3() {
        if let Some(o) = run_float_driver(&parse_driver("1e3"), "t") {
            assert_eq!(o, parse_witness(0, 1, 3, 0).to_string());
        }
    }

    /// "3.14" -> ndig 3, K -2, class 0 (value = 314 * 10^-2 = 3.14).
    #[test]
    fn parse_three_point_fourteen() {
        if let Some(o) = run_float_driver(&parse_driver("3.14"), "t") {
            assert_eq!(o, parse_witness(0, 3, -2, 0).to_string());
        }
    }

    /// ".5" -> ndig 1, K -1, class 0 (value = 5 * 10^-1 = 0.5).
    #[test]
    fn parse_dot_five() {
        if let Some(o) = run_float_driver(&parse_driver(".5"), "t") {
            assert_eq!(o, parse_witness(0, 1, -1, 0).to_string());
        }
    }

    /// "0.5e2" -> ndig 2 ("05"), K = 2 - 1 = 1, class 0 (value = 5 * 10^1 = 50).
    #[test]
    fn parse_half_times_e2() {
        if let Some(o) = run_float_driver(&parse_driver("0.5e2"), "t") {
            assert_eq!(o, parse_witness(0, 2, 1, 0).to_string());
        }
    }

    /// "-1.5" -> sign 1, ndig 2, K -1, class 0.
    #[test]
    fn parse_negative_one_point_five() {
        if let Some(o) = run_float_driver(&parse_driver("-1.5"), "t") {
            assert_eq!(o, parse_witness(1, 2, -1, 0).to_string());
        }
    }

    /// "  +100 " -> sign 0, ndig 3, K 0, class 0 (whitespace + plus handled).
    #[test]
    fn parse_ws_plus_hundred() {
        if let Some(o) = run_float_driver(&parse_driver("  +100 "), "t") {
            assert_eq!(o, parse_witness(0, 3, 0, 0).to_string());
        }
    }

    /// "INF" -> class 2, ndig 0, K 0.
    #[test]
    fn parse_inf() {
        if let Some(o) = run_float_driver(&parse_driver("INF"), "t") {
            assert_eq!(o, parse_witness(0, 0, 0, 2).to_string());
        }
    }

    /// "-inf" -> sign 1, class 2.
    #[test]
    fn parse_neg_inf() {
        if let Some(o) = run_float_driver(&parse_driver("-inf"), "t") {
            assert_eq!(o, parse_witness(1, 0, 0, 2).to_string());
        }
    }

    /// "NAN" -> class 3 (never signed).
    #[test]
    fn parse_nan() {
        if let Some(o) = run_float_driver(&parse_driver("NAN"), "t") {
            assert_eq!(o, parse_witness(0, 0, 0, 3).to_string());
        }
    }

    /// "abc" -> no digits, class 1 (empty/invalid -> 0.0).
    #[test]
    fn parse_no_digits() {
        if let Some(o) = run_float_driver(&parse_driver("abc"), "t") {
            assert_eq!(o, parse_witness(0, 0, 0, 1).to_string());
        }
    }

    /// "0" -> ndig 1, K 0, class 0.
    #[test]
    fn parse_zero() {
        if let Some(o) = run_float_driver(&parse_driver("0"), "t") {
            assert_eq!(o, parse_witness(0, 1, 0, 0).to_string());
        }
    }

    /// "1E-7" -> ndig 1, K -7, class 0 (uppercase E, negative exponent).
    #[test]
    fn parse_one_e_minus_seven() {
        if let Some(o) = run_float_driver(&parse_driver("1E-7"), "t") {
            assert_eq!(o, parse_witness(0, 1, -7, 0).to_string());
        }
    }

    /// Validates the parsed digit bytes at 512 via a rolling hash, ensuring the
    /// concatenated digits (no sign, no point) are written correctly for "12345.6789".
    fn parse_digits_driver(s: &str) -> String {
        let stores = store_ascii(256, s);
        format!(
            r#"(func $t (export "t") (result i64)
  (local $sign i32) (local $ndig i32) (local $K i32) (local $class i32)
  (local $i i32) (local $h i64)
{stores}  (call $__rt_parse_decimal (i32.const 256) (i32.const {len}) (i32.const 512))
  (local.set $class)
  (local.set $K)
  (local.set $ndig)
  (local.set $sign)
  (local.set $h (i64.const 0))
  (local.set $i (i32.const 0))
  (block $e
    (loop $l
      (br_if $e (i32.ge_s (local.get $i) (local.get $ndig)))
      (local.set $h (i64.rem_u (i64.add (i64.mul (local.get $h) (i64.const 257)) (i64.load8_u (i32.add (i32.const 512) (local.get $i)))) (i64.const 1000000000000000)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
  (local.get $h))"#,
            len = s.len(),
        )
    }

    /// "12345.6789" -> digit bytes "123456789" written at 512 (hash matches str_hash).
    #[test]
    fn parse_digits_stripped() {
        if let Some(o) = run_float_driver(&parse_digits_driver("12345.6789"), "t") {
            assert_eq!(o, str_hash("123456789").to_string());
        }
    }


    /// Formats an f64 raw bit pattern (a u64) the way `wasmer --invoke` prints the
    /// returned i64 -- as a signed decimal (a set sign bit yields a negative number).
    fn bits_str(bits: u64) -> String {
        format!("{}", bits as i64)
    }

    /// Stores ASCII `digs` at 512 and calls `__rt_digits_to_f64(sign, ndig, K, 512,
    /// scratch)`, returning the i64 bits. `ndig` must equal `digs.len()`. The scratch
    /// base comes from the `$__float_scratch` global (init 0x4000 in the 1-page test
    /// module), mirroring how real runtime callers pass the bignum region.
    fn digits_driver(sign: u32, ndig: u32, k: i32, digs: &str) -> String {
        let stores = store_ascii(512, digs);
        format!(
            r#"(func $t (export "t") (result i64)
{stores}  (call $__rt_digits_to_f64 (i32.const {sign}) (i32.const {ndig}) (i32.const {k}) (i32.const 512) (global.get $__float_scratch)))"#,
        )
    }

    /// Stores string `s` at 256, calls `__rt_str_to_f64(256, len, 600, scratch)`, and
    /// loads the i64 bits the routine stored at 600 (the full parse + dispatch +
    /// rounding path). The scratch base comes from the `$__float_scratch` global.
    fn strtod_driver(s: &str) -> String {
        let stores = store_ascii(256, s);
        format!(
            r#"(func $t (export "t") (result i64)
{stores}  (call $__rt_str_to_f64 (i32.const 256) (i32.const {len}) (i32.const 600) (global.get $__float_scratch))
  (i64.load (i32.const 600)))"#,
            len = s.len(),
        )
    }

    /// Calls `__rt_str_to_f64` twice in one module (first at 256, then 320), returning
    /// the second result. Guards the buffer zeroing: a prior large-magnitude parse must
    /// not leave stale bignum limbs for the following call.
    fn strtod_repeated_driver(first: &str, second: &str) -> String {
        let s1 = store_ascii(256, first);
        let s2 = store_ascii(320, second);
        format!(
            r#"(func $t (export "t") (result i64)
{s1}  (call $__rt_str_to_f64 (i32.const 256) (i32.const {len1}) (i32.const 600) (global.get $__float_scratch))
{s2}  (call $__rt_str_to_f64 (i32.const 320) (i32.const {len2}) (i32.const 600) (global.get $__float_scratch))
  (i64.load (i32.const 600)))"#,
            len1 = first.len(),
            len2 = second.len(),
        )
    }

    // -- __rt_digits_to_f64: orchestrator parity vs `php -r` (PHP 8.5 true IEEE bits) --

    /// 1e3 = 1000 -> 0x408f400000000000 (sign 0, 1 digit, K 3).
    #[test]
    fn digits_one_e3() {
        if let Some(o) = run_float_driver(&digits_driver(0, 1, 3, "1"), "t") {
            assert_eq!(o, bits_str(0x408f400000000000));
        }
    }

    /// 3.14 -> 0x40091eb851eb851f (digits "314", K -2).
    #[test]
    fn digits_three_point_fourteen() {
        if let Some(o) = run_float_driver(&digits_driver(0, 3, -2, "314"), "t") {
            assert_eq!(o, bits_str(0x40091eb851eb851f));
        }
    }

    /// 0.1 -> 0x3fb999999999999a (digits "01", K -1).
    #[test]
    fn digits_one_tenth() {
        if let Some(o) = run_float_driver(&digits_driver(0, 2, -1, "01"), "t") {
            assert_eq!(o, bits_str(0x3fb999999999999a));
        }
    }

    /// 1e-7 -> 0x3e7ad7f29abcaf48 (digits "1", K -7).
    #[test]
    fn digits_one_e_minus_seven() {
        if let Some(o) = run_float_driver(&digits_driver(0, 1, -7, "1"), "t") {
            assert_eq!(o, bits_str(0x3e7ad7f29abcaf48));
        }
    }

    /// 2.5 -> 0x4004000000000000 (digits "25", K -1).
    #[test]
    fn digits_two_point_five() {
        if let Some(o) = run_float_driver(&digits_driver(0, 2, -1, "25"), "t") {
            assert_eq!(o, bits_str(0x4004000000000000));
        }
    }

    /// -2.5 -> 0xc004000000000000 (sign 1, digits "25", K -1).
    #[test]
    fn digits_neg_two_point_five() {
        if let Some(o) = run_float_driver(&digits_driver(1, 2, -1, "25"), "t") {
            assert_eq!(o, bits_str(0xc004000000000000));
        }
    }

    /// 1e100 -> 0x54b249ad2594c37d (digits "1", K 100).
    #[test]
    fn digits_one_e_100() {
        if let Some(o) = run_float_driver(&digits_driver(0, 1, 100, "1"), "t") {
            assert_eq!(o, bits_str(0x54b249ad2594c37d));
        }
    }

    /// 1e-100 -> 0x2b2bff2ee48e0530 (digits "1", K -100).
    #[test]
    fn digits_one_e_minus_100() {
        if let Some(o) = run_float_driver(&digits_driver(0, 1, -100, "1"), "t") {
            assert_eq!(o, bits_str(0x2b2bff2ee48e0530));
        }
    }

    /// DBL_MAX = 1.7976931348623157e308 -> 0x7fefffffffffffff (17 digits, K 292).
    #[test]
    fn digits_max_finite() {
        if let Some(o) = run_float_driver(&digits_driver(0, 17, 292, "17976931348623157"), "t") {
            assert_eq!(o, bits_str(0x7fefffffffffffff));
        }
    }

    /// M == 0 with a sign -> -0.0 = 0x8000000000000000 (digits "0", K 0, sign 1).
    #[test]
    fn digits_negative_zero() {
        if let Some(o) = run_float_driver(&digits_driver(1, 1, 0, "0"), "t") {
            assert_eq!(o, bits_str(0x8000000000000000));
        }
    }

    /// Smallest positive normal = 2.2250738585072014e-308 -> 0x0010000000000000
    /// (digits "22250738585072014", K -324; just above the underflow short-circuit).
    #[test]
    fn digits_smallest_normal() {
        if let Some(o) = run_float_driver(&digits_driver(0, 17, -324, "22250738585072014"), "t") {
            assert_eq!(o, bits_str(0x0010000000000000));
        }
    }

    // -- __rt_str_to_f64: full parse + dispatch + rounding parity vs `php -r` --

    /// "1e3" -> 0x408f400000000000.
    #[test]
    fn strtod_one_e3() {
        if let Some(o) = run_float_driver(&strtod_driver("1e3"), "t") {
            assert_eq!(o, bits_str(0x408f400000000000));
        }
    }

    /// "3.14" -> 0x40091eb851eb851f.
    #[test]
    fn strtod_three_point_fourteen() {
        if let Some(o) = run_float_driver(&strtod_driver("3.14"), "t") {
            assert_eq!(o, bits_str(0x40091eb851eb851f));
        }
    }

    /// "0.1" -> 0x3fb999999999999a.
    #[test]
    fn strtod_one_tenth() {
        if let Some(o) = run_float_driver(&strtod_driver("0.1"), "t") {
            assert_eq!(o, bits_str(0x3fb999999999999a));
        }
    }

    /// "1E-7" (uppercase E) -> 0x3e7ad7f29abcaf48.
    #[test]
    fn strtod_one_e_minus_seven() {
        if let Some(o) = run_float_driver(&strtod_driver("1E-7"), "t") {
            assert_eq!(o, bits_str(0x3e7ad7f29abcaf48));
        }
    }

    /// "2.5" -> 0x4004000000000000.
    #[test]
    fn strtod_two_point_five() {
        if let Some(o) = run_float_driver(&strtod_driver("2.5"), "t") {
            assert_eq!(o, bits_str(0x4004000000000000));
        }
    }

    /// "-2.5" -> 0xc004000000000000.
    #[test]
    fn strtod_neg_two_point_five() {
        if let Some(o) = run_float_driver(&strtod_driver("-2.5"), "t") {
            assert_eq!(o, bits_str(0xc004000000000000));
        }
    }

    /// "1e100" -> 0x54b249ad2594c37d.
    #[test]
    fn strtod_one_e_100() {
        if let Some(o) = run_float_driver(&strtod_driver("1e100"), "t") {
            assert_eq!(o, bits_str(0x54b249ad2594c37d));
        }
    }

    /// "1e-100" -> 0x2b2bff2ee48e0530.
    #[test]
    fn strtod_one_e_minus_100() {
        if let Some(o) = run_float_driver(&strtod_driver("1e-100"), "t") {
            assert_eq!(o, bits_str(0x2b2bff2ee48e0530));
        }
    }

    /// "1e309" overflows -> +inf = 0x7ff0000000000000 (matches PHP `(float)"1e309"`).
    #[test]
    fn strtod_overflow_inf() {
        if let Some(o) = run_float_driver(&strtod_driver("1e309"), "t") {
            assert_eq!(o, bits_str(0x7ff0000000000000));
        }
    }

    /// "-1e309" overflows -> -inf = 0xfff0000000000000.
    #[test]
    fn strtod_overflow_neg_inf() {
        if let Some(o) = run_float_driver(&strtod_driver("-1e309"), "t") {
            assert_eq!(o, bits_str(0xfff0000000000000));
        }
    }

    /// "1.7976931348623157e308" -> DBL_MAX = 0x7fefffffffffffff.
    #[test]
    fn strtod_max_finite() {
        if let Some(o) = run_float_driver(&strtod_driver("1.7976931348623157e308"), "t") {
            assert_eq!(o, bits_str(0x7fefffffffffffff));
        }
    }

    /// "1e308" -> 0x7fe1ccf385ebc8a0 (just below the overflow threshold).
    #[test]
    fn strtod_one_e_308() {
        if let Some(o) = run_float_driver(&strtod_driver("1e308"), "t") {
            assert_eq!(o, bits_str(0x7fe1ccf385ebc8a0));
        }
    }

    /// "0.0" -> +0.0 = 0x0 (M == 0).
    #[test]
    fn strtod_zero() {
        if let Some(o) = run_float_driver(&strtod_driver("0.0"), "t") {
            assert_eq!(o, bits_str(0x0));
        }
    }

    /// "-0.0" -> -0.0 = 0x8000000000000000.
    #[test]
    fn strtod_neg_zero() {
        if let Some(o) = run_float_driver(&strtod_driver("-0.0"), "t") {
            assert_eq!(o, bits_str(0x8000000000000000));
        }
    }

    /// "1e-307" -> 0x0031fa182c40c60d (smallest tested non-underflowing negative exponent).
    #[test]
    fn strtod_one_e_minus_307() {
        if let Some(o) = run_float_driver(&strtod_driver("1e-307"), "t") {
            assert_eq!(o, bits_str(0x0031fa182c40c60d));
        }
    }

    /// "2.2250738585072014e-308" -> smallest normal 0x0010000000000000.
    #[test]
    fn strtod_smallest_normal() {
        if let Some(o) = run_float_driver(&strtod_driver("2.2250738585072014e-308"), "t") {
            assert_eq!(o, bits_str(0x0010000000000000));
        }
    }

    // -- non-numeric tokens: PHP string->float yields +0.0 for inf/nan/invalid --

    /// "inf" -> +0.0 (PHP grammar excludes the `inf` token from numeric strings).
    #[test]
    fn strtod_inf_token_is_zero() {
        if let Some(o) = run_float_driver(&strtod_driver("inf"), "t") {
            assert_eq!(o, bits_str(0x0));
        }
    }

    /// "-inf" -> +0.0 (PHP returns +0.0 even for the negated non-numeric token).
    #[test]
    fn strtod_neg_inf_token_is_zero() {
        if let Some(o) = run_float_driver(&strtod_driver("-inf"), "t") {
            assert_eq!(o, bits_str(0x0));
        }
    }

    /// "nan" -> +0.0 (PHP grammar excludes the `nan` token from numeric strings).
    #[test]
    fn strtod_nan_token_is_zero() {
        if let Some(o) = run_float_driver(&strtod_driver("nan"), "t") {
            assert_eq!(o, bits_str(0x0));
        }
    }

    /// "abc" -> +0.0 (no leading numeric prefix).
    #[test]
    fn strtod_invalid_is_zero() {
        if let Some(o) = run_float_driver(&strtod_driver("abc"), "t") {
            assert_eq!(o, bits_str(0x0));
        }
    }

    /// A large-magnitude parse followed by "1" must still yield 1.0 = 0x3ff0000000000000:
    /// the buffer zeroing prevents the first call's bignum limbs from corrupting the
    /// second call's mantissa build.
    #[test]
    fn strtod_repeated_call_clears_buffers() {
        let d = strtod_repeated_driver("1e100", "1");
        if let Some(o) = run_float_driver(&d, "t") {
            assert_eq!(o, bits_str(0x3ff0000000000000));
        }
    }

    /// Assembles RT_PARSE_DECIMAL alone and dumps numbered WAT on failure (diagnostic).
    #[test]
    fn probe_assemble_parse() {
        let mut wm = WatModule::new();
        wm.set_memory(1, Some("memory"));
        wm.add_raw_func(super::RT_PARSE_DECIMAL);
        let wat = wm.render();
        match ::wat::parse_str(&wat) {
            Ok(bytes) => { let _ = ::wasmparser::validate(&bytes); }
            Err(e) => {
                let numbered: String = wat.lines().enumerate()
                    .map(|(i,l)| format!("{:4}: {}", i+1, l)).collect::<Vec<_>>().join("\n");
                panic!("PARSE-ASSEMBLE failed: {e}\n==== WAT ====\n{numbered}");
            }
        }
    }

}


