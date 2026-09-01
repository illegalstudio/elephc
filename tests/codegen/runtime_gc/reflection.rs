//! Purpose:
//! Regression coverage for compact Reflection owners used by uninitialized date objects.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Constructorless-only reflectors must fit the default heap and retain php-src diagnostics.

use crate::support::compile_and_run_capture;

/// Verifies constructorless-only DateTime reflectors stay within the default 8 MiB heap.
#[test]
fn test_constructorless_only_reflection_class_uses_compact_owner() {
    let out = compile_and_run_capture(
        r#"<?php
$reflection = new ReflectionClass(DateTime::class);
$mutable = $reflection->newInstanceWithoutConstructor();
$immutable = (new ReflectionClass(DateTimeImmutable::class))->newInstanceWithoutConstructor();
$period = (new ReflectionClass(DatePeriod::class))->newInstanceWithoutConstructor();
$interval = (new ReflectionClass(DateInterval::class))->newInstanceWithoutConstructor();
echo get_class($mutable) . "|" . get_class($immutable) . "|" . get_class($period) . "|" . get_class($interval);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "DateTime|DateTimeImmutable|DatePeriod|DateInterval"
    );
}

/// Verifies uninitialized DateTime inputs expose php-src's DatePeriod diagnostics.
#[test]
fn test_reflection_uninitialized_dateperiod_inputs_match_php_src() {
    let out = compile_and_run_capture(
        r#"<?php
$now = new DateTimeImmutable();
$simpleInterval = new DateInterval("P2D");

$date = (new ReflectionClass(DateTime::class))->newInstanceWithoutConstructor();
try {
    new DatePeriod($date, new DateInterval('P1D'), 2);
} catch (Error $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}

$date = (new ReflectionClass(DateTime::class))->newInstanceWithoutConstructor();
try {
    new DatePeriod($now, new DateInterval('P1D'), $date);
} catch (Error $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}

$date = (new ReflectionClass(DateTime::class))->newInstanceWithoutConstructor();
$dateperiod = (new ReflectionClass(DatePeriod::class))->newInstanceWithoutConstructor();
$dateinterval = (new ReflectionClass(DateInterval::class))->newInstanceWithoutConstructor();
try {
    $dateperiod->__unserialize(['start' => $date]);
} catch (Error $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}

try {
    $dateperiod->__unserialize(['start' => $now, 'end' => $date]);
} catch (Error $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}

try {
    $dateperiod->__unserialize(['start' => $now, 'end' => $now, 'current' => $date]);
} catch (Error $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}

try {
    $dateperiod->__unserialize(['start' => $now, 'end' => $now, 'current' => $now, 'interval' => $dateinterval]);
} catch (Error $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}

try {
    $dateperiod->__unserialize([
        'start' => $now, 'end' => $now, 'current' => $now, 'interval' => $simpleInterval,
        'recurrences' => 2, 'include_start_date' => true, 'include_end_date' => true,
    ]);
    echo "DatePeriod::__unserialize: SUCCESS\n";
} catch (Error $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}
echo "OK\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "DateObjectError: Object of type DateTimeInterface has not been correctly initialized by calling parent::__construct() in its constructor\n",
            "DateObjectError: Object of type DateTimeInterface has not been correctly initialized by calling parent::__construct() in its constructor\n",
            "Error: Invalid serialization data for DatePeriod object\n",
            "Error: Invalid serialization data for DatePeriod object\n",
            "Error: Invalid serialization data for DatePeriod object\n",
            "Error: Invalid serialization data for DatePeriod object\n",
            "DatePeriod::__unserialize: SUCCESS\n",
            "OK\n",
        )
    );
}
