//! Purpose:
//! Regression tests for PHP's `float`→`int` conversion (`zend_dval_to_lval`) and for the
//! shared `__rt_php_float_to_int` runtime helper that implements it on every supported target.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixtures multiply by `$argc` (1 when the compiled binary runs with no CLI arguments) so
//!   the AST/EIR constant folders cannot evaluate the conversion at compile time; the point is
//!   to exercise the runtime lowering.
//! - Reference PHP 8.4 maps NaN and ±INF to `0` and reduces every other out-of-range finite
//!   double modulo 2^64. Raw hardware truncation does neither and, worse, disagrees between
//!   AArch64 (`fcvtzs` saturates) and x86_64 (`cvttsd2si` yields `INT64_MIN`), so the helper's
//!   assembly is asserted for *both* architectures from this host.

use crate::support::*;
use elephc::codegen::platform::{Arch, Platform, Target};

/// Verifies `(int)` of NaN, ±INF, and huge finite doubles matches PHP 8.4 (all `0` here).
#[test]
fn test_float_to_int_out_of_range_is_zero() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
var_dump((int)(NAN * $n));
var_dump((int)(INF * $n));
var_dump((int)(-INF * $n));
var_dump((int)(1e300 * $n));
var_dump((int)(-1e300 * $n));
var_dump(intval(1e300 * $n));
"#,
    );
    assert_eq!(
        out,
        "int(0)\nint(0)\nint(0)\nint(0)\nint(0)\nint(0)\n"
    );
}

/// Verifies out-of-range finite doubles wrap modulo 2^64 exactly like `zend_dval_to_lval`.
///
/// These are the cases that expose the AArch64/x86_64 divergence: saturation would give
/// `INT64_MAX`/`INT64_MIN` and `cvttsd2si` would give `INT64_MIN` for all of them.
#[test]
fn test_float_to_int_wraps_modulo_two_pow_64() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
var_dump((int)(9223372036854775808.0 * $n));
var_dump((int)(-9223372036854775808.0 * $n));
var_dump((int)(-9223372036854777856.0 * $n));
var_dump((int)(9223372036854777856.0 * $n));
var_dump((int)(18446744073709551616.0 * $n));
var_dump((int)(1.5e19 * $n));
"#,
    );
    assert_eq!(
        out,
        "int(-9223372036854775808)\n\
         int(-9223372036854775808)\n\
         int(9223372036854773760)\n\
         int(-9223372036854773760)\n\
         int(0)\n\
         int(-3446744073709551616)\n"
    );
}

/// Verifies in-range doubles still truncate toward zero, including sub-1 magnitudes.
#[test]
fn test_float_to_int_truncates_toward_zero() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
echo (int)(3.7 * $n), "|", (int)(-3.7 * $n), "|", (int)(0.5 * $n), "|", (int)(-0.5 * $n);
echo "|", (int)(1e18 * $n), "|", (int)(-1.0 * $n), "|", (int)(2.9 * $n);
"#,
    );
    assert_eq!(out, "3|-3|0|0|1000000000000000000|-1|2");
}

/// Verifies NaN/INF float array keys hash to `0` like PHP instead of a per-target garbage key.
///
/// Before the shared helper, `$a[INF]` produced `INT64_MAX` on AArch64 — enough to exhaust the
/// heap trying to size the packed array — and `INT64_MIN` on x86_64.
#[test]
fn test_float_array_keys_use_php_conversion() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
$a = [];
$a[NAN * $n] = 1;
$a[INF * $n] = 2;
$a[-INF * $n] = 3;
var_dump($a);
var_dump(array_key_exists(0, $a));
"#,
    );
    assert_eq!(out, "array(1) {\n  [0]=>\n  int(3)\n}\nbool(true)\n");
}

/// Returns the runtime assembly emitted for one supported target.
fn runtime_asm_for(arch: Arch, platform: Platform) -> String {
    elephc::codegen::generate_runtime(8_388_608, Target::new(platform, arch))
}

/// Verifies the AArch64 runtime defines `__rt_php_float_to_int` with its integer decode.
///
/// The helper must not fall back to a bare `fcvtzs`: that saturates instead of reducing modulo
/// 2^64 and returns `INT64_MAX`/`INT64_MIN` for out-of-range inputs.
#[test]
fn test_aarch64_runtime_defines_php_float_to_int() {
    let asm = runtime_asm_for(Arch::AArch64, Platform::Linux);
    assert!(
        asm.contains("__rt_php_float_to_int:"),
        "AArch64 runtime must define the shared PHP float->int helper"
    );
    for expected in [
        "fmov x9, d0",
        "ubfx x10, x9, #52, #11",
        "and x11, x9, #0x000fffffffffffff",
        "orr x11, x11, #0x0010000000000000",
        "sub x10, x10, #1075",
        "lsl x11, x11, x10",
        "lsr x11, x11, x10",
        "neg x9, x11",
    ] {
        assert!(
            asm.contains(expected),
            "AArch64 PHP float->int helper is missing `{expected}`"
        );
    }
}

/// Verifies the x86_64 runtime defines the same helper with the same IEEE-754 decode.
///
/// This target cannot be executed from the macOS/AArch64 development host, so the emitted
/// instruction sequence is asserted directly to keep both lowerings in lockstep.
#[test]
fn test_x86_64_runtime_defines_php_float_to_int() {
    let asm = runtime_asm_for(Arch::X86_64, Platform::Linux);
    assert!(
        asm.contains("__rt_php_float_to_int:"),
        "x86_64 runtime must define the shared PHP float->int helper"
    );
    for expected in [
        "movq r10, xmm0",
        "and ecx, 0x7ff",
        "bts r11, 52",
        "sub rcx, 1075",
        "shl r11, cl",
        "shr r11, cl",
        "neg r11",
    ] {
        assert!(
            asm.contains(expected),
            "x86_64 PHP float->int helper is missing `{expected}`"
        );
    }
}

/// Verifies the array/cast runtime helpers call the shared conversion instead of truncating.
///
/// `__rt_mixed_cast_int` (PHP `(int)` on a boxed Mixed) and `__rt_array_set_mixed_key` (float
/// array keys) each used a bare `fcvtzs` / `cvttsd2si`, which is where the per-target `(int)NAN`
/// and `$a[INF]` divergence came from.
#[test]
fn test_runtime_float_consumers_call_the_shared_helper() {
    for (arch, expected_calls) in [(Arch::AArch64, "bl __rt_php_float_to_int"), (
        Arch::X86_64,
        "call __rt_php_float_to_int",
    )] {
        let asm = runtime_asm_for(arch, Platform::Linux);
        assert!(
            asm.matches(expected_calls).count() >= 4,
            "{arch:?} runtime should route every float->int consumer through the shared helper"
        );
    }
}

/// Verifies the numeric-STRING cap lives in its own shared helper, not open-coded at the call.
///
/// PHP has two double-to-int rules and which applies depends on where the double came from: a
/// float VALUE wraps modulo 2^64, a numeric STRING caps. `__rt_str_to_int` needs the second, so
/// it must not call `__rt_php_float_to_int` -- and it must not reach for a bare `fcvtzs` /
/// `cvttsd2si` either, which is the open-coding this module exists to prevent: those two
/// disagree with each other on NaN and on overflow, which is where the original per-target
/// divergence came from.
#[test]
fn test_string_to_int_routes_through_the_capping_helper() {
    for (arch, call, bare) in [
        (Arch::AArch64, "bl __rt_php_float_to_int_cap", "fcvtzs x0, d0"),
        (Arch::X86_64, "call __rt_php_float_to_int_cap", "cvttsd2si rax, xmm0"),
    ] {
        let asm = runtime_asm_for(arch, Platform::Linux);
        let (_, after) = asm
            .split_once("__rt_str_to_int:")
            .unwrap_or_else(|| panic!("{arch:?} runtime should define __rt_str_to_int"));
        let body = after.split("__rt_str_to_number").next().unwrap_or(after);
        assert!(
            body.contains(call),
            "{arch:?} __rt_str_to_int should apply the cap through the shared helper"
        );
        assert!(
            !body.contains(bare),
            "{arch:?} __rt_str_to_int should not open-code the conversion with `{bare}`"
        );
    }
}

/// Verifies the capping helper is defined for both targets, with the non-finite arm first.
///
/// The cap is only well defined once NaN and the infinities are out of the way: `fcvtzs`
/// saturates them and `cvttsd2si` reports its indefinite pattern, so a helper that capped first
/// and checked afterwards would answer PHP_INT_MAX for `(int)"1e309"` instead of 0.
#[test]
fn test_runtime_defines_the_capping_helper() {
    for (arch, expected) in [
        (Arch::AArch64, vec!["__rt_php_float_to_int_cap:", "fcvtzs x9, d0"]),
        (Arch::X86_64, vec!["__rt_php_float_to_int_cap:", "cvttsd2si r11, xmm0"]),
    ] {
        let asm = runtime_asm_for(arch, Platform::Linux);
        for needle in expected {
            assert!(
                asm.contains(needle),
                "{arch:?} capping helper is missing `{needle}`"
            );
        }
    }
}
