//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of casts, constants, and introspection math builtins, including pow operator, pow operator float, and pow right associative.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies the `**` exponentiation operator with integer base 2 and exponent 10: expects `1024`.
#[test]
fn test_pow_operator() {
    let out = compile_and_run("<?php echo 2 ** 10;");
    assert_eq!(out, "1024");
}

/// Verifies the `**` exponentiation operator with float operands 2.0 and 0.5, which equals sqrt(2): expects `1.4142135623731`.
#[test]
fn test_pow_operator_float() {
    let out = compile_and_run("<?php echo 2.0 ** 0.5;");
    assert_eq!(out, "1.4142135623731");
}

/// Verifies exponentiation is right-associative: `2 ** 3 ** 2` means `2 ** (3 ** 2)` = `2 ** 9` = `512`.
#[test]
fn test_pow_right_associative() {
    let out = compile_and_run("<?php echo 2 ** 3 ** 2;");
    assert_eq!(out, "512");
}

/// Verifies exponentiation has higher precedence than unary minus: `-2 ** 2` = `-(2**2)` = `-4`.
#[test]
fn test_pow_higher_than_unary() {
    let out = compile_and_run("<?php echo -2 ** 2;");
    assert_eq!(out, "-4");
}

/// Verifies exponentiation has higher precedence than multiplication: `3 * 2 ** 3` = `3 * 8` = `24`.
#[test]
fn test_pow_higher_than_multiply() {
    let out = compile_and_run("<?php echo 3 * 2 ** 3;");
    assert_eq!(out, "24");
}

// --- fmod, fdiv ---

/// Verifies `fmod(10.5, 3.2)` returns the floating-point remainder: expects `0.9`.
#[test]
fn test_fmod() {
    let out = compile_and_run("<?php echo fmod(10.5, 3.2);");
    assert_eq!(out, "0.9");
}

/// Verifies `fdiv(10, 3)` performs floating-point division: expects `3.3333333333333`.
#[test]
fn test_fdiv() {
    let out = compile_and_run("<?php echo fdiv(10, 3);");
    assert_eq!(out, "3.3333333333333");
}

/// Verifies `fdiv(1, 0)` returns `INF` instead of crashing on division by zero.
#[test]
fn test_fdiv_by_zero() {
    let out = compile_and_run("<?php echo fdiv(1, 0);");
    assert_eq!(out, "INF");
}

// --- rand, mt_rand, random_int ---

/// Verifies `rand(1, 1)` returns the degenerate single-value range: expects `1`.
#[test]
fn test_rand_range() {
    let out = compile_and_run("<?php echo rand(1, 1);");
    assert_eq!(out, "1");
}

/// Verifies `mt_rand(5, 5)` returns the degenerate single-value range: expects `5`.
#[test]
fn test_mt_rand_range() {
    let out = compile_and_run("<?php echo mt_rand(5, 5);");
    assert_eq!(out, "5");
}

/// Verifies `random_int(42, 42)` returns the degenerate single-value range: expects `42`.
#[test]
fn test_random_int_range() {
    let out = compile_and_run("<?php echo random_int(42, 42);");
    assert_eq!(out, "42");
}

/// Verifies a range wider than 2^32 actually varies instead of always returning `$min`.
///
/// THE BOUNDARY IS THE POINT, and it is asserted as a PAIR. The range helper took its bound in a
/// 32-bit register, so a span that was a multiple of 2^32 arrived as zero and every draw came
/// back as `min`: `random_int(0, PHP_INT_MAX)` answered `0` every single time, and so did
/// `mt_rand()`. `4294967294` (span 2^32 - 1) worked while `4294967295` (span 2^32) did not,
/// which is what distinguishes a truncated bound from a broken generator — asserting only the
/// wide case would not have told the two apart.
///
/// Every test in this family used a range small enough to fit 32 bits, which is why the defect
/// survived. Eight draws are used per range: for a span this size, eight equal draws from a
/// working sampler is not a flake anyone will ever see.
#[test]
fn test_random_int_wide_span_is_not_stuck_at_min() {
    let out = compile_and_run(
        r#"<?php
function varies(callable $draw): string {
    $seen = [];
    for ($i = 0; $i < 8; $i++) { $seen[(string) $draw()] = true; }
    return count($seen) > 1 ? "v" : "!";
}
echo varies(fn() => random_int(0, PHP_INT_MAX));
echo varies(fn() => random_int(0, 4294967295));
echo varies(fn() => random_int(0, 4294967294));
echo varies(fn() => random_int(PHP_INT_MIN, PHP_INT_MAX));
echo varies(fn() => mt_rand(0, PHP_INT_MAX));
"#,
    );
    assert_eq!(out, "vvvvv");
}

/// Verifies wide-range draws land inside their bounds, both endpoints included.
///
/// A sampler that overshoots is worse than one that is stuck, because the result still looks
/// random. The full 64-bit span is the case that cannot be checked by eye: `max - min + 1`
/// overflows to zero there, so it exercises a different arm of the helper than the merely-wide
/// ranges above.
#[test]
fn test_random_int_wide_span_stays_in_range() {
    let out = compile_and_run(
        r#"<?php
$bad = 0;
for ($i = 0; $i < 100; $i++) {
    $v = random_int(0, PHP_INT_MAX);
    if ($v < 0) { $bad++; }
    $w = random_int(-10, 10);
    if ($w < -10 || $w > 10) { $bad++; }
}
echo $bad === 0 ? "in-range" : "out-of-range";
"#,
    );
    assert_eq!(out, "in-range");
}

/// Verifies small ranges still reach both endpoints after the 64-bit sampler was introduced.
///
/// Spans below 2^32 delegate to the pre-existing 32-bit sampler, so an off-by-one in that
/// delegation would silently drop `min` or `max` while every draw still passed a bounds check.
/// Enumerating the values actually produced is what catches that; counting them is not enough.
#[test]
fn test_random_int_small_range_reaches_both_endpoints() {
    let out = compile_and_run(
        r#"<?php
$seen = [];
for ($i = 0; $i < 400; $i++) { $seen[] = random_int(1, 6); }
$u = array_unique($seen);
sort($u);
echo implode(",", $u);
"#,
    );
    assert_eq!(out, "1,2,3,4,5,6");
}

/// Verifies `rand()` with no arguments does not crash and returns a non-negative integer.
#[test]
fn test_rand_no_args() {
    let out = compile_and_run("<?php $r = rand(); echo ($r >= 0 ? \"ok\" : \"bad\");");
    assert_eq!(out, "ok");
}

/// Verifies `random_int()` rejects an inverted range with PHP's catchable `ValueError`.
///
/// The lowering computed the sample width as `max - min + 1`; an inverted range made that
/// non-positive and `__rt_random_uniform` handed back an unbounded garbage integer instead of a
/// value inside the requested range.
#[test]
fn test_random_int_inverted_range_is_a_catchable_value_error() {
    let out = compile_and_run(
        r#"<?php
try {
    echo random_int(10, 5);
} catch (ValueError $e) {
    echo get_class($e), "|", $e->getMessage();
}
echo "|", random_int(7, 7);
"#,
    );
    assert_eq!(
        out,
        "ValueError|random_int(): Argument #1 ($min) must be less than or equal to argument #2 ($max)|7"
    );
}

/// Verifies an uncaught inverted `random_int()` range reports PHP's uncaught-`ValueError` fatal.
#[test]
fn test_random_int_inverted_range_uncaught_reports_value_error_fatal() {
    let err = compile_and_run_expect_failure("<?php echo random_int(10, 5);");
    assert!(err.contains(
        "Fatal error: Uncaught ValueError: random_int(): Argument #1 ($min) must be less than or equal to argument #2 ($max)"
    ));
}

/// Verifies `mt_rand()` throws on an inverted range while `rand()` swaps the bounds, like php-src.
#[test]
fn test_mt_rand_throws_and_rand_swaps_on_inverted_range() {
    let out = compile_and_run(
        r#"<?php
try {
    echo mt_rand(10, 5);
} catch (ValueError $e) {
    echo get_class($e), "|", $e->getMessage();
}
$r = rand(10, 5);
echo "|", ($r >= 5 && $r <= 10) ? "in-range" : "out";
"#,
    );
    assert_eq!(
        out,
        "ValueError|mt_rand(): Argument #2 ($max) must be greater than or equal to argument #1 ($min)|in-range"
    );
}

// --- number_format ---

/// Verifies `number_format(1234567)` formats with default 0 decimals, comma thousands separator: expects `1,234,567`.
#[test]
fn test_number_format_no_decimals() {
    let out = compile_and_run("<?php echo number_format(1234567);");
    assert_eq!(out, "1,234,567");
}

/// Verifies `number_format(1234.5678, 2)` rounds to 2 decimal places: expects `1,234.57`.
#[test]
fn test_number_format_with_decimals() {
    let out = compile_and_run("<?php echo number_format(1234.5678, 2);");
    assert_eq!(out, "1,234.57");
}

/// Verifies negative `$decimals` round to fewer significant digits instead of emitting garbage.
///
/// PHP does not reject a negative precision: it pre-rounds the magnitude to that power of ten
/// (half away from zero) and then formats with no decimals, so `-4.9` with `-1` decimals is
/// `"0"` and never `"-0"`. elephc used to build the format string as `'0' + $decimals`, which
/// turned `-1` into the literal `"%./f"` and printed `"/f"`.
#[test]
fn test_number_format_negative_decimals_round_to_significant_digits() {
    let out = compile_and_run(
        r#"<?php
echo number_format(1234.5678, -1), "|";
echo number_format(1234.5678, -2), "|";
echo number_format(1234.5678, -3), "|";
echo number_format(1234.5678, -4), "|";
echo number_format(-1234.5678, -1), "|";
echo number_format(1250.0, -2), "|";
echo number_format(2500.0, -3), "|";
echo number_format(-4.9, -1), "|";
echo number_format(-5.0, -1), "|";
echo number_format(1234.5678, -1, ",", "."), "|";
echo number_format(1234.5678, 2);
"#,
    );
    assert_eq!(
        out,
        "1,230|1,200|1,000|0|-1,230|1,300|3,000|0|-10|1.230|1,234.57"
    );
}

/// Verifies a two-digit `$decimals` renders real decimals instead of a malformed format string.
///
/// The single ASCII digit the format string used to carry turned `10` into `"%.:f"`.
#[test]
fn test_number_format_multi_digit_decimals() {
    let out = compile_and_run(
        r#"<?php
echo number_format(1.5, 10), "|", number_format(1.5, 12), "|", number_format(1234.5678, 9);
"#,
    );
    assert_eq!(out, "1.5000000000|1.500000000000|1,234.567800000");
}

/// Verifies wide magnitudes and precisions render in full instead of being truncated.
///
/// The raw `snprintf` buffer used to be 48 bytes while the helper trusted `snprintf`'s
/// *untruncated* return value, so a 79-digit result was assembled from bytes read past the end
/// of the buffer.
#[test]
fn test_number_format_wide_magnitude_and_precision_are_not_truncated() {
    let out = compile_and_run(
        r#"<?php
$z = (float)($argc - 1);
echo number_format(1e60 + $z, 0), "|";
echo number_format(1e30 + $z, 2), "|";
echo strlen(number_format(-1e15 + $z, 30));
"#,
    );
    assert_eq!(
        out,
        "999,999,999,999,999,949,387,135,297,074,018,866,963,645,011,013,410,073,083,904|\
         1,000,000,000,000,000,019,884,624,838,656.00|53"
    );
}

/// Verifies `number_format(42, 2)` pads small numbers to 2 decimal places: expects `42.00`.
#[test]
fn test_number_format_small() {
    let out = compile_and_run("<?php echo number_format(42, 2);");
    assert_eq!(out, "42.00");
}

/// Verifies `number_format(-1234.5, 1)` handles negative numbers: expects `-1,234.5`.
#[test]
fn test_number_format_negative() {
    let out = compile_and_run("<?php echo number_format(-1234.5, 1);");
    assert_eq!(out, "-1,234.5");
}

/// Verifies `number_format` with custom decimal `,` and thousands `.` separators (European style): expects `1.234.567,89`.
#[test]
fn test_number_format_custom_separators() {
    let out = compile_and_run(r#"<?php echo number_format(1234567.89, 2, ",", ".");"#);
    assert_eq!(out, "1.234.567,89");
}

/// Verifies `number_format` with empty string as thousands separator disables grouping: expects `1234567.89`.
#[test]
fn test_number_format_no_thousands() {
    let out = compile_and_run(r#"<?php echo number_format(1234567.89, 2, ".", "");"#);
    assert_eq!(out, "1234567.89");
}

/// Verifies `number_format` with a space as thousands separator: expects `1 234 567`.
#[test]
fn test_number_format_space_thousands() {
    let out = compile_and_run(r#"<?php echo number_format(1234567, 0, ".", " ");"#);
    assert_eq!(out, "1 234 567");
}

// --- Constants ---
