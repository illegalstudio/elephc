//! Purpose:
//! Regression tests for the maximum-size bounds reference PHP enforces on `array_fill()`'s
//! `$count` and on `range()`'s element count — the ones it reports as a catchable `ValueError`
//! rather than as an allocation fatal.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected string in this file is verbatim `LC_ALL=C php` 8.4.20 output for the same
//!   fixture, including `range()`'s message, which interpolates the NORMALIZED interval: the
//!   smaller endpoint as `start=`, the larger as `end=`, and `abs($step)` as `step=`.
//! - The two bounds are deliberately different, because php-src's are: `array_fill()` is checked
//!   against `INT_MAX` (`2147483647` builds, `2147483648` throws) while `range()` is checked
//!   against the maximum array size (`1073741823` elements build, `1073741824` throw). Both
//!   boundaries are asserted from the rejected side here and from the accepted side by the
//!   heap-exhaustion fixtures in `arrays::allocation_guards`.
//! - The accepted side of each boundary is never executed for its result: reference PHP accepts
//!   it and then fails on memory, and so does elephc, but the attempt would ask this process for
//!   an 8 GiB payload.
//! - `range()`'s three `$step` `ValueError`s are checked before the size one in php-src, so the
//!   fixtures that would trip both assert the step wording.

use crate::support::*;

/// php-src's verbatim `ValueError` message for an `array_fill()` `$count` past `INT_MAX`.
const FILL_COUNT_TOO_LARGE: &str = "ValueError: array_fill(): Argument #2 ($count) is too large";

/// php-src's verbatim `ValueError` message for a negative `array_fill()` `$count`.
const FILL_COUNT_NEGATIVE: &str =
    "ValueError: array_fill(): Argument #2 ($count) must be greater than or equal to 0";

/// Regression: an `array_fill()` `$count` past `INT_MAX` must raise reference PHP's catchable
/// `ValueError` instead of the process's uncatchable heap fatal.
///
/// Before the guard every one of these counts reached the fill helper, whose allocation guard
/// reported `heap memory exhausted` and exited — memory-safe, but invisible to
/// `catch (ValueError $e)`. The negative count is asserted alongside so the two `$count` guards
/// keep reporting php-src's two different messages.
#[test]
fn test_array_fill_oversized_count_throws_catchable_value_error() {
    let out = compile_and_run(
        r#"<?php
foreach ([2147483648, 3000000000, 4294967296, PHP_INT_MAX, 0x2000000000000004, -1] as $n) {
    try {
        $r = array_fill(0, $n, 0);
        echo "no throw ", count($r), "\n";
    } catch (ValueError $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
echo "after\n";
"#,
    );
    assert_eq!(
        out,
        format!("{FILL_COUNT_TOO_LARGE}\n").repeat(5) + FILL_COUNT_NEGATIVE + "\nafter\n"
    );
}

/// Regression: the keyed and string fill helpers are guarded by the same bound, so a non-zero
/// `$start` (which routes the count into `__rt_hash_new` as a bucket capacity) and a string value
/// (whose helper takes `$count` in the first ABI argument register) both throw as well.
#[test]
fn test_array_fill_oversized_count_throws_on_every_fill_helper() {
    let out = compile_and_run(
        r#"<?php
foreach ([2147483648, 0x0400000000000004] as $n) {
    try {
        $r = array_fill(5, $n, 0);
        echo "no throw ", count($r), "\n";
    } catch (ValueError $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
    try {
        $s = array_fill(0, $n, "x");
        echo "no throw ", count($s), "\n";
    } catch (ValueError $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
echo "after\n";
"#,
    );
    assert_eq!(out, format!("{FILL_COUNT_TOO_LARGE}\n").repeat(4) + "after\n");
}

/// Verifies an uncaught oversized `$count` terminates with the PHP-shaped uncaught diagnostic
/// naming `ValueError`, not with the allocator's heap fatal.
#[test]
fn test_array_fill_oversized_count_uncaught_is_fatal() {
    let err =
        compile_and_run_expect_failure("<?php $a = array_fill(0, 3000000000, 0); echo count($a);");
    assert!(err.contains(&format!("Uncaught {FILL_COUNT_TOO_LARGE}")), "{}", err);
}

/// Regression: a `range()` whose element count exceeds the maximum array size must raise
/// reference PHP's catchable `ValueError`, interpolating the normalized interval exactly as
/// php-src does.
///
/// The endpoints are printed smallest-first and the step as its magnitude, so a descending
/// `range(3000000000, 1)` reports the same text as the ascending `range(1, 3000000000)`.
#[test]
fn test_range_oversized_span_throws_catchable_value_error() {
    let out = compile_and_run(
        r#"<?php
try { $a = range(1, 3000000000); echo "no throw\n"; } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { $b = range(3000000000, 1); echo "no throw\n"; } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { $c = range(0, 1073741823); echo "no throw\n"; } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { $d = range(0, 4294967292, 4); echo "no throw\n"; } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { $f = range(3000000000, 1, -2); echo "no throw\n"; } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { $g = range(PHP_INT_MIN, PHP_INT_MAX, 2); echo "no throw\n"; } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "after\n";
"#,
    );
    assert_eq!(
        out,
        r#"ValueError: The supplied range exceeds the maximum array size: start=1 end=3000000000 step=1
ValueError: The supplied range exceeds the maximum array size: start=1 end=3000000000 step=1
ValueError: The supplied range exceeds the maximum array size: start=0 end=1073741823 step=1
ValueError: The supplied range exceeds the maximum array size: start=0 end=4294967292 step=4
ValueError: The supplied range exceeds the maximum array size: start=1 end=3000000000 step=2
ValueError: The supplied range exceeds the maximum array size: start=-9223372036854775808 end=9223372036854775807 step=2
after
"#
    );
}

/// Verifies an uncaught oversized `range()` terminates with the PHP-shaped uncaught diagnostic
/// naming `ValueError` and carrying the interpolated interval.
///
/// The message is built at runtime, so this also pins the dynamic-message throw path: before it
/// existed, a throwable whose text is only known at runtime reported the unwinder's generic
/// `uncaught exception` line instead of naming its class.
#[test]
fn test_range_oversized_span_uncaught_is_fatal() {
    let err = compile_and_run_expect_failure("<?php $a = range(1, 3000000000); echo count($a);");
    assert!(
        err.contains(
            "Uncaught ValueError: The supplied range exceeds the maximum array size: start=1 end=3000000000 step=1"
        ),
        "{}",
        err
    );
}

/// Regression: php-src checks `range()`'s three `$step` `ValueError`s before it sizes the result,
/// so an oversized range with a bad step must still report the step wording.
///
/// The ordinary stepped ranges on the last line are the positive control for the same guard
/// sequence: adding the size check must not narrow the shapes `range()` already accepted.
#[test]
fn test_range_step_value_errors_still_precede_the_size_error() {
    let out = compile_and_run(
        r#"<?php
$z = 0; $n = -1; $w = 10;
try { range(1, 3000000000, $z); echo "no throw\n"; } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { range(1, 3000000000, $n); echo "no throw\n"; } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { range(1, 3, $w); echo "no throw\n"; } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
echo implode(",", range(1, 9, 2)), "|", implode(",", range(5, 1, -2)), "|", implode(",", range(7, 7, 100)), "\n";
"#,
    );
    assert_eq!(
        out,
        r#"range(): Argument #3 ($step) cannot be 0
range(): Argument #3 ($step) must be greater than 0 for increasing ranges
range(): Argument #3 ($step) must be less than the range spanned by argument #1 ($start) and argument #2 ($end)
1,3,5,7,9|5,3,1|7
"#
    );
}

/// Regression: the widest possible interval spans `2^64 - 1`, which every step magnitude fits, so
/// PHP rejects it for its SIZE and not for its step.
///
/// The step-magnitude guard used to take a signed absolute of `end - start`, which wraps to `1`
/// for `PHP_INT_MIN`..`PHP_INT_MAX` and made every step past `1` look wider than the interval.
/// php-src reads that subtraction as unsigned, so the guard now orders the endpoints first and
/// compares against the unsigned width.
#[test]
fn test_range_widest_interval_reports_the_size_error_not_the_step_error() {
    let out = compile_and_run(
        r#"<?php
foreach ([2, 4294967296] as $s) {
    try { range(PHP_INT_MIN, PHP_INT_MAX, $s); echo "no throw\n"; } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(
        out,
        r#"The supplied range exceeds the maximum array size: start=-9223372036854775808 end=9223372036854775807 step=2
The supplied range exceeds the maximum array size: start=-9223372036854775808 end=9223372036854775807 step=4294967296
"#
    );
}

/// Positive control: ordinary fills and ranges — indexed, keyed, string-valued, ascending,
/// descending, stepped and degenerate — keep producing exactly what reference PHP produces.
#[test]
fn test_ordinary_fills_and_ranges_still_work() {
    let out = compile_and_run(
        r#"<?php
$a = array_fill(0, 4, 7);
echo count($a), ":", implode(",", $a), "|";
$b = array_fill(0, 3, "x");
echo count($b), ":", implode(",", $b), "|";
foreach (array_fill(5, 3, 1) as $k => $v) { echo "$k=$v,"; }
echo "|", implode(",", range(1, 5)), "|", implode(",", range(5, 1)), "|", implode(",", range(-3, 3, 2)), "|", implode(",", range(1073741823, 1073741823)), "\n";
"#,
    );
    assert_eq!(
        out,
        "4:7,7,7,7|3:x,x,x|5=1,6=1,7=1,|1,2,3,4,5|5,4,3,2,1|-3,-1,1,3|1073741823\n"
    );
}

/// Regression guard for the accepted side of the `range()` boundary and for the clean
/// heap-exhaustion path: `range(-1073741822, 0)` asks for exactly `1073741823` elements, the most
/// reference PHP will build, so the size guard must let it through to the allocator — which then
/// reports heap exhaustion, the condition PHP has no `ValueError` for.
///
/// One element more (`range(-1073741823, 0)`) is the `ValueError` asserted above, so the pair pins
/// the boundary from both sides without ever completing an 8 GiB allocation.
#[test]
fn test_range_largest_accepted_span_still_reaches_heap_exhaustion() {
    let err = compile_and_run_expect_failure("<?php $a = range(-1073741822, 0); echo count($a);");
    assert!(err.contains("Allowed memory size of"), "{}", err);
}

/// Regression guard for the accepted side of the `array_fill()` boundary: `INT_MAX` itself is
/// accepted by reference PHP (which then fails on memory) and must stay accepted here, reaching
/// the allocator's heap fatal rather than the new `ValueError`.
#[test]
fn test_array_fill_largest_accepted_count_still_reaches_heap_exhaustion() {
    let err =
        compile_and_run_expect_failure("<?php $a = array_fill(0, 2147483647, 7); echo count($a);");
    assert!(err.contains("Allowed memory size of"), "{}", err);
}
