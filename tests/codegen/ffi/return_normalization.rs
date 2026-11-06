//! Purpose:
//! Regression tests for extern return normalization against C callees that leave unspecified register bits dirty.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The C fixture returns full 64-bit words whose low bits encode the declared C return type, mirroring
//!   what the SysV x86_64 and AAPCS64 ABIs let a callee leave above a narrow return value.
//! - Narrow-argument observers take a full 32-bit register so they report exactly what the caller
//!   left there, which is how the de-facto extension contract for 8 and 16-bit arguments is checked.
//! - C `float` callees check that `f32` arguments are rounded to single precision before the call and
//!   that `f32` returns are widened from the low half of the floating-point register.
//! - The fixture is assembled into the test program itself, so no shared library or search path is needed.

use super::*;

/// C fixture whose functions are declared in PHP with narrower return types than the word they return.
const DIRTY_RETURN_FIXTURE: &str = r#"
#include <stdint.h>

// Declared as `bool` in PHP: bits 0..7 say false, bit 8 is garbage the ABI allows.
uint64_t elephc_test_dirty_false(void) { return 0x100; }

// Declared as `bool` in PHP: the low byte says true, every other bit is garbage.
uint64_t elephc_test_dirty_true(void) { return 0xffffffffffffff01ull; }

// Declared as `i32` or `u32`: the low half holds the value, the high half is garbage or clean,
// so only the declared width and signedness decide what PHP reads.
uint64_t elephc_test_dirty_32_positive(void) { return 0xffffffff00000005ull; }
uint64_t elephc_test_dirty_32_negative(void) { return 0x00000000fffffffbull; }

// Declared as `i16` or `u16`.
uint64_t elephc_test_dirty_16_positive(void) { return 0xffffffffffff0005ull; }
uint64_t elephc_test_dirty_16_negative(void) { return 0x000000000000fffbull; }

// Declared as `i8` or `u8`.
uint64_t elephc_test_dirty_8_positive(void) { return 0xffffffffffffff05ull; }
uint64_t elephc_test_dirty_8_negative(void) { return 0x00000000000000fbull; }

// Declared as `int`, `i64` and `u64`: every bit is the value.
int64_t elephc_test_wide_i64(void) { return -5000000000ll; }
uint64_t elephc_test_wide_u64(void) { return 0xffffffffffffffffull; }

// Narrow-argument observers: declared in PHP with a narrow parameter but reading the whole
// 32-bit argument register, so they report exactly what the caller left there.
uint32_t elephc_test_observe_u8(uint32_t raw) { return raw; }
int32_t elephc_test_observe_i8(int32_t raw) { return raw; }
uint32_t elephc_test_observe_u16(uint32_t raw) { return raw; }
int32_t elephc_test_observe_i16(int32_t raw) { return raw; }
uint32_t elephc_test_observe_u32(uint32_t raw) { return raw; }

// Declared as `f32` in PHP: C float parameters and returns live in the low half of the FP register.
float elephc_test_ret_f32(void) { return 1.5f; }
float elephc_test_echo_f32(float value) { return value; }
float elephc_test_sum_f32(float a, float b) { return a + b; }
double elephc_test_echo_f64(double value) { return value; }
// Mixed widths in one call: an int, a float, and a double each use their own ABI slot.
double elephc_test_mix(int32_t i, float f, double d) { return i + f + d; }
"#;

/// Verifies a C `bool` return keeps only its low byte: an ABI-valid false with dirty upper bits reads as false
/// through `var_dump`, truthiness, identity comparison, and a stored local.
#[test]
fn test_ffi_extern_bool_return_uses_only_the_low_byte() {
    let out = compile_and_run_with_c_fixture(
        r#"<?php
extern function elephc_test_dirty_false(): bool;
extern function elephc_test_dirty_true(): bool;
var_dump(elephc_test_dirty_false());
var_dump(elephc_test_dirty_true());
echo elephc_test_dirty_false() ? "truthy" : "falsy", "\n";
$ok = elephc_test_dirty_false();
var_dump($ok === false);
var_dump(elephc_test_dirty_true() === true);
"#,
        DIRTY_RETURN_FIXTURE,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(true)\nfalsy\nbool(true)\nbool(true)\n"
    );
}

/// Verifies integer returns widen from the declared C width: narrow signed returns sign-extend, narrow
/// unsigned returns zero-extend, and 64-bit returns pass through untouched (a `u64` above
/// `PHP_INT_MAX` reads as the same bits reinterpreted as a signed `int`).
#[test]
fn test_ffi_extern_int_returns_widen_from_the_declared_c_width() {
    let out = compile_and_run_with_c_fixture(
        r#"<?php
extern function elephc_test_dirty_32_positive(): i32;
extern function elephc_test_dirty_32_negative(): i32;
extern function elephc_test_dirty_16_positive(): i16;
extern function elephc_test_dirty_16_negative(): i16;
extern function elephc_test_dirty_8_positive(): i8;
extern function elephc_test_dirty_8_negative(): i8;
extern function elephc_test_wide_i64(): int;
extern function elephc_test_wide_u64(): u64;
echo elephc_test_dirty_32_positive(), " ", elephc_test_dirty_32_negative(), "\n";
echo elephc_test_dirty_16_positive(), " ", elephc_test_dirty_16_negative(), "\n";
echo elephc_test_dirty_8_positive(), " ", elephc_test_dirty_8_negative(), "\n";
echo elephc_test_wide_i64(), " ", elephc_test_wide_u64(), "\n";
"#,
        DIRTY_RETURN_FIXTURE,
    );
    assert_eq!(out, "5 -5\n5 -5\n5 -5\n-5000000000 -1\n");
}

/// Verifies unsigned narrow returns zero-extend: the negative words of the signed test read as their
/// unsigned values, the positive words with dirty upper bits read as 5 (without zero extension the
/// `u32` word would be a negative PHP int), and `i64` is an alias of `int`.
#[test]
fn test_ffi_extern_unsigned_int_returns_zero_extend() {
    let out = compile_and_run_with_c_fixture(
        r#"<?php
extern function elephc_test_dirty_32_positive(): u32;
extern function elephc_test_dirty_32_negative(): u32;
extern function elephc_test_dirty_16_positive(): u16;
extern function elephc_test_dirty_16_negative(): u16;
extern function elephc_test_dirty_8_positive(): u8;
extern function elephc_test_dirty_8_negative(): u8;
extern function elephc_test_wide_i64(): i64;
echo elephc_test_dirty_32_negative(), " ", elephc_test_dirty_16_negative(), " ", elephc_test_dirty_8_negative(), "\n";
echo elephc_test_dirty_32_positive(), " ", elephc_test_dirty_16_positive(), " ", elephc_test_dirty_8_positive(), "\n";
echo elephc_test_wide_i64(), "\n";
"#,
        DIRTY_RETURN_FIXTURE,
    );
    assert_eq!(out, "4294967291 65531 251\n5 5 5\n-5000000000\n");
}

/// Verifies 8 and 16-bit arguments are reduced modulo their width and extended to 32 bits by the
/// caller, while 32-bit arguments pass through and the callee reads only its declared bits.
#[test]
fn test_ffi_extern_narrow_int_args_are_truncated_and_extended_by_the_caller() {
    let out = compile_and_run_with_c_fixture(
        r#"<?php
extern function elephc_test_observe_u8(u8 $value): u32;
extern function elephc_test_observe_i8(i8 $value): i32;
extern function elephc_test_observe_u16(u16 $value): u32;
extern function elephc_test_observe_i16(i16 $value): i32;
extern function elephc_test_observe_u32(u32 $value): u32;
$big = 4294967297;
echo elephc_test_observe_u8(300), " ", elephc_test_observe_u8(-1), "\n";
echo elephc_test_observe_i8(200), " ", elephc_test_observe_i8(-1), "\n";
echo elephc_test_observe_u16(70000), " ", elephc_test_observe_i16(40000), "\n";
echo elephc_test_observe_u32($big), "\n";
"#,
        DIRTY_RETURN_FIXTURE,
    );
    assert_eq!(out, "44 255\n-56 -1\n4464 -25536\n1\n");
}

/// Verifies `f32` returns widen the C float to a PHP double and `f32` arguments round the PHP double to a
/// single before the call: 0.1 survives the round trip only at single precision while `f64` keeps it, an
/// int argument is converted, and mixed `i32`/`f32`/`double` parameters each land in their own ABI slot.
#[test]
fn test_ffi_extern_f32_arguments_and_returns_round_through_single_precision() {
    let out = compile_and_run_with_c_fixture(
        r#"<?php
extern function elephc_test_ret_f32(): f32;
extern function elephc_test_echo_f32(f32 $value): f32;
extern function elephc_test_sum_f32(f32 $a, f32 $b): f32;
extern function elephc_test_echo_f64(f64 $value): float;
extern function elephc_test_mix(i32 $i, f32 $f, double $d): float;
var_dump(elephc_test_ret_f32());
var_dump(elephc_test_echo_f32(2.5));
var_dump(elephc_test_echo_f32(0.1));
var_dump(elephc_test_sum_f32(1.25, 2));
var_dump(elephc_test_echo_f64(0.1));
var_dump(elephc_test_mix(1, 0.5, 0.25));
"#,
        DIRTY_RETURN_FIXTURE,
    );
    assert_eq!(
        out,
        "float(1.5)\nfloat(2.5)\nfloat(0.10000000149011612)\nfloat(3.25)\nfloat(0.1)\nfloat(1.75)\n"
    );
}
