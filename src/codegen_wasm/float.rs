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

/// Registers the wasm32-wasi float<->string runtime helpers on `wm`.
///
/// Currently emits `__rt_f64_decompose` (the float decoder) and `__rt_bignum_mul_u32`
/// (a big-integer primitive). Later stages append the remaining bignum primitives,
/// digit-extraction, `%.14G` formatting, and string-to-float parsing routines here.
/// Must be called before rendering any function that references these symbols.
// Not yet referenced by a non-test caller: PHP-visible float formatting wires this
// into the command/reactor runtime in stage S6. Exercised by the unit tests below.
#[allow(dead_code)]
pub(super) fn emit_float_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_F64_DECOMPOSE);
    wm.add_raw_func(RT_BIGNUM_MUL_U32);
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
}
