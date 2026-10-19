//! Purpose:
//! Diagnostic coverage for the `static` local shapes elephc refuses at compile time
//! instead of miscompiling them (references to the local itself, by-reference arguments,
//! by-reference `foreach` and `unset`).
//!
//! Called from:
//! - `cargo test` through Rust's test harness (`tests/error_tests.rs`).
//!
//! Key details:
//! - Supported forms are covered end to end in `tests/codegen/static_ref.rs`.

use super::*;

/// Verifies `$r = &$s` and `$s = &$v` on a static local are refused (they crashed or kept two copies).
#[test]
fn test_reference_to_static_local_is_refused() {
    expect_error(
        "<?php function f() { static $x = []; $r = &$x; $r['q'] = 1; return $x; } f();",
        "A reference to the static local $x",
    );
    expect_error(
        "<?php function f() { static $n = 0; $v = 1; $n = &$v; return $n; } f();",
        "A reference to the static local $n",
    );
}

/// Verifies a static local bound into an array element by reference is refused.
#[test]
fn test_static_local_bound_into_element_by_reference_is_refused() {
    expect_error(
        "<?php function f() { static $s = 1; $a = []; $a['k'] = &$s; return $a; } f();",
        "Binding the static local $s into a property or array element by reference",
    );
}

/// Verifies a boxed static local passed to a user by-reference parameter is refused (the
/// callee's writes were silently dropped).
#[test]
fn test_boxed_static_local_by_reference_argument_is_refused() {
    expect_error(
        "<?php function g(&$a) { $a[] = 9; } function f() { static $c = []; g($c); return count($c); } f();",
        "passing the static local $c by reference is not supported yet",
    );
}

/// Verifies `foreach ($static as &$v)` is refused (the element writes were dropped).
#[test]
fn test_by_reference_foreach_over_static_local_is_refused() {
    expect_error(
        "<?php function f($x) { static $c = []; foreach ($c as &$v) { $v++; } unset($v); $c[] = $x; return $c; } f(1);",
        "foreach by reference over the static local $c",
    );
}

/// Verifies `unset($static)` is refused (the static lost its value for the next call).
#[test]
fn test_unset_of_static_local_is_refused() {
    expect_error(
        "<?php function f() { static $n; $n = ($n ?? 0) + 1; if ($n > 1) { unset($n); } return $n; } f();",
        "unset() of the static local $n",
    );
}
