//! Purpose:
//! Regression tests for PHP's implicit array creation on the first offset write.
//!
//! Called from:
//! - `cargo test` through the arrays integration suite.
//!
//! Key details:
//! - An undefined variable used as `$var[$key] = $value` or `$var[] = $value` becomes an
//!   empty indexed array with no undefined-variable diagnostic.
//! - A loop body must not reset an array created by an earlier iteration; the runtime
//!   guard returns the bound container and retains it for the write.

use crate::support::*;

/// A loop's first write creates the array and later iterations append to the same storage.
#[test]
fn test_undefined_array_write_autovivifies_in_a_loop() {
    let out = compile_and_run(
        r#"<?php
foreach ([1, 2, 3] as $key => $value) { $result[$key] = $value * 10; }
var_dump($result);
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  int(10)\n  [1]=>\n  int(20)\n  [2]=>\n  int(30)\n}\n"
    );
}

/// An undefined append target starts as an indexed array and keeps every appended value.
#[test]
fn test_undefined_array_append_autovivifies_in_a_loop() {
    let out = compile_and_run(
        r#"<?php
foreach ([1, 2, 3] as $value) { $result[] = $value; }
echo implode(",", $result);
"#,
    );
    assert_eq!(out, "1,2,3");
}

/// The key expression is evaluated before the new array is published, so it sees the old
/// (undefined) variable exactly as PHP does.
#[test]
fn test_undefined_array_write_uses_the_original_key_value() {
    let out = compile_and_run(
        r#"<?php
foreach ([1, 2, 3] as $key => $value) { $result[$key] = $key; }
var_dump($result);
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  int(0)\n  [1]=>\n  int(1)\n  [2]=>\n  int(2)\n}\n"
    );
}
/// A string-keyed first write creates a hash and keeps the keys from later iterations.
#[test]
fn test_undefined_associative_array_write_autovivifies_in_a_loop() {
    let out = compile_and_run(
        r#"<?php
foreach ([1, 2, 3] as $value) { $result["k" . $value] = $value * 10; }
var_dump($result);
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [\"k1\"]=>\n  int(10)\n  [\"k2\"]=>\n  int(20)\n  [\"k3\"]=>\n  int(30)\n}\n"
    );
}
