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

use super::wat::WatModule;

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

/// Registers the wasm32-wasi float<->string runtime helpers on `wm`.
///
/// Currently emits `__rt_f64_decompose` (the float decoder) plus the big-integer
/// primitives `__rt_bignum_mul_u32` and `__rt_bignum_divmod_u32`. Later stages append
/// the remaining primitives, digit-extraction, `%.14G` formatting, and string-to-float
/// parsing routines here. Must be called before rendering any function that references
/// these symbols.
// Not yet referenced by a non-test caller: PHP-visible float formatting wires this
// into the command/reactor runtime in stage S6. Exercised by the unit tests below.
#[allow(dead_code)]
pub(super) fn emit_float_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_F64_DECOMPOSE);
    wm.add_raw_func(RT_BIGNUM_MUL_U32);
    wm.add_raw_func(RT_BIGNUM_DIVMOD_U32);
    wm.add_raw_func(RT_BIGNUM_MUL_SMALL_N_TIMES);
    wm.add_raw_func(RT_BIGNUM_IS_ZERO);
    wm.add_raw_func(RT_U32_TO_9DIGITS);
    wm.add_raw_func(RT_F64_DIGITS);
    wm.add_raw_func(RT_ROUND_DIGITS);
    wm.add_raw_func(RT_U32_TO_DEC);
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
        emit_float_runtime(&mut wm);
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
}
