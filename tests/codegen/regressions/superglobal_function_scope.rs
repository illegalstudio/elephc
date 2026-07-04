//! Purpose:
//! Regression tests for superglobal (`$_GET`/`$_POST`/...) writes from inside
//! function and closure scopes. The root fix makes `LoweringContext::local_type`
//! superglobal-aware so the fixed `AssocArray{Str, Mixed}` slot contract is
//! honored in every scope, including indexed→hash conversion of array literals
//! before they are stored into the global Hash slot.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and stdout is compared.
//! - All fixtures run under the `--web` request-superglobal model.

use super::*;

/// Verifies that assigning an empty array then a string key inside a function
/// produces a readable string-keyed entry in the global slot.
#[test]
fn test_superglobal_fn_assign_string_keys() {
    let out = compile_and_run(
        r#"<?php
function fill(): void { $_GET = []; $_GET['a'] = 'written'; }
fill();
echo $_GET['a'] ?? 'empty';
"#,
    );
    assert_eq!(out, "written");
}

/// Verifies that an associative array literal assigned to a superglobal inside
/// a function preserves both string keys through the indexed→hash conversion.
#[test]
fn test_superglobal_fn_assign_assoc_literal() {
    let out = compile_and_run(
        r#"<?php
function fill(): void { $_GET = ['a' => 'x', 'b' => 'y']; }
fill();
echo ($_GET['a'] ?? '?') . ($_GET['b'] ?? '?');
"#,
    );
    assert_eq!(out, "xy");
}

/// Verifies that repeated calls to a function writing the superglobal correctly
/// overwrite the shared slot and that intermediate reads observe the live value.
#[test]
fn test_superglobal_fn_repeated_calls() {
    let out = compile_and_run(
        r#"<?php
function fill(string $v): void { $_GET = []; $_GET['k'] = $v; }
fill('one');
$r1 = $_GET['k'] ?? '?';
fill('two');
echo $r1 . '|' . ($_GET['k'] ?? '?');
"#,
    );
    assert_eq!(out, "one|two");
}

/// Verifies that a closure writing a superglobal uses the same global-storage
/// path as named functions.
#[test]
fn test_superglobal_write_from_closure() {
    let out = compile_and_run(
        r#"<?php
$f = function (): void { $_GET = []; $_GET['c'] = 'closure'; };
$f();
echo $_GET['c'] ?? 'empty';
"#,
    );
    assert_eq!(out, "closure");
}

/// Control: writing a superglobal element from a function after a top-level
/// initialization already works today; this guards against regressions in the
/// element-store path while the literal-store path is being fixed.
#[test]
fn test_superglobal_element_write_from_fn_after_toplevel_init() {
    let out = compile_and_run(
        r#"<?php
$_GET = [];
function put(): void { $_GET['b'] = 'hello'; }
put();
echo $_GET['b'] ?? 'empty';
"#,
    );
    assert_eq!(out, "hello");
}

/// Verifies that growing a superglobal with many string keys from inside a
/// function does not corrupt the Hash and that `count()` reflects the growth.
#[test]
fn test_superglobal_fn_many_keys_growth() {
    let out = compile_and_run(
        r#"<?php
function fill(): void {
    $_GET = [];
    for ($i = 0; $i < 30; $i++) { $_GET['p' . $i] = (string)$i; }
}
fill();
echo count($_GET) . '|' . ($_GET['p29'] ?? '?');
"#,
    );
    assert_eq!(out, "30|29");
}
