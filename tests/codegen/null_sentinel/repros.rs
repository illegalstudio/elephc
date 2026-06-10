//! Purpose:
//! Regression repros for the null-sentinel collision (`0x7fff_ffff_ffff_fffe`): the integer
//! `9223372036854775806` (= `PHP_INT_MAX - 1`) must behave as a real int, never as `null`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - PHP-cross-checked expectations (PHP 8.4); the fixture value equals the null sentinel bit
//!   pattern, deliberately distinct from the uninitialized-property sentinel (`PHP_INT_MAX - 2`).

use super::*;

/// Verifies the integer PHP_INT_MAX-1 (== the null sentinel bit pattern) echoes as itself
/// instead of being suppressed as null.
#[ignore = "TODO(null-sentinel): misread as null under NullRepr::Sentinel - un-ignore in Phase 2"]
#[test]
fn test_int_at_null_sentinel_echoes_as_integer() {
    let out = compile_and_run("<?php echo 9223372036854775806;");
    assert_eq!(out, "9223372036854775806");
}

/// Verifies the sentinel-valued integer survives a variable round-trip through echo and
/// var_dump instead of printing empty / NULL.
#[ignore = "TODO(null-sentinel): misread as null under NullRepr::Sentinel - un-ignore in Phase 2"]
#[test]
fn test_int_at_null_sentinel_var_roundtrips() {
    let out =
        compile_and_run("<?php $x = 9223372036854775806; echo $x, \"|\"; var_dump($x);");
    assert_eq!(out, "9223372036854775806|int(9223372036854775806)\n");
}

/// Verifies the sentinel-valued integer read back from an array prints and var_dumps as
/// int(...), not NULL — the array element store/load path must not misread it as a miss.
#[ignore = "TODO(null-sentinel): misread as null under NullRepr::Sentinel - un-ignore in Phase 2"]
#[test]
fn test_int_at_null_sentinel_in_array_roundtrips() {
    let out = compile_and_run(
        "<?php $a = [9223372036854775806]; echo $a[0], \"|\"; var_dump($a[0]);",
    );
    assert_eq!(out, "9223372036854775806|int(9223372036854775806)\n");
}

/// Verifies a nullable-int function returning the sentinel-valued integer var_dumps as
/// int(...), not NULL — the ?int return channel must distinguish the value from null.
#[ignore = "TODO(null-sentinel): misread as null under NullRepr::Sentinel - un-ignore in Phase 2"]
#[test]
fn test_int_at_null_sentinel_nullable_return_is_not_null() {
    let out = compile_and_run(
        "<?php function f(): ?int { return 9223372036854775806; } var_dump(f());",
    );
    assert_eq!(out, "int(9223372036854775806)\n");
}

/// Verifies is_null() rejects the sentinel-valued integer: a real int is never null.
#[ignore = "TODO(null-sentinel): misread as null under NullRepr::Sentinel - un-ignore in Phase 2"]
#[test]
fn test_int_at_null_sentinel_is_not_null() {
    let out = compile_and_run("<?php $x = 9223372036854775806; var_dump(is_null($x));");
    assert_eq!(out, "bool(false)\n");
}

/// Verifies ?? does not treat the sentinel-valued integer as null: the left operand wins.
#[ignore = "TODO(null-sentinel): misread as null under NullRepr::Sentinel - un-ignore in Phase 2"]
#[test]
fn test_int_at_null_sentinel_null_coalesce_keeps_value() {
    let out = compile_and_run("<?php $x = 9223372036854775806; echo $x ?? 0;");
    assert_eq!(out, "9223372036854775806");
}

/// Verifies isset() reports true for an array element holding the sentinel-valued integer.
/// Uses a ternary echo because var_dump(isset(...)) has a separate, unrelated formatting
/// quirk (prints int(1) instead of bool(true)).
#[test]
fn test_int_at_null_sentinel_isset_is_true() {
    let out = compile_and_run(
        "<?php $a = [9223372036854775806]; echo isset($a[0]) ? \"set\" : \"unset\";",
    );
    assert_eq!(out, "set");
}
