//! Purpose:
//! Regression tests for issue #398: scalar-union builtin/user function returns
//! (e.g. `string|false`, `int|false`) collapsed `false` to `""`/`0` because
//! `wider_type` let `Str`/`Float` absorb `Bool`. The fix makes `Bool` widen to
//! `Mixed` (boxed tagged cell) so `false` preserves its tag through `return`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each test compiles inline PHP and asserts stdout matches PHP behavior.
//! - The return type infers to `Mixed`, not `Str`/`Int`, so `=== false` works.

use crate::support::compile_and_run;

/// User function returning `string|false` must preserve `false` as a bool-tagged
/// Mixed cell, not coerce it to `""`.
#[test]
fn test_user_function_string_or_false_return() {
    let out = compile_and_run(
        r#"<?php
function find(bool $ok) {
    if ($ok) { return "yes"; }
    return false;
}
$a = find(true);
$b = find(false);
var_dump($a === false);
var_dump($a);
var_dump($b === false);
var_dump($b);
"#,
    );
    assert_eq!(out, "bool(false)\nstring(3) \"yes\"\nbool(true)\nbool(false)\n");
}

/// User function returning `int|false` must preserve `false` as a bool-tagged
/// Mixed cell, not coerce it to `0`.
#[test]
fn test_user_function_int_or_false_return() {
    let out = compile_and_run(
        r#"<?php
function lookup(bool $ok) {
    if ($ok) { return 42; }
    return false;
}
$a = lookup(true);
$b = lookup(false);
var_dump($a === false);
var_dump($a);
var_dump($b === false);
var_dump($b);
"#,
    );
    assert_eq!(out, "bool(false)\nint(42)\nbool(true)\nbool(false)\n");
}

/// `image_type_to_extension` for an unknown type returns `false`, not `""`.
#[test]
fn test_image_type_to_extension_unknown_returns_false() {
    let out = compile_and_run(
        r#"<?php
$r = image_type_to_extension(IMAGETYPE_UNKNOWN);
var_dump($r === false);
"#,
    );
    assert_eq!(out, "bool(true)\n");
}