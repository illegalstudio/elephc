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

/// The write receiver is reloaded after operand evaluation, so an assignment inside the value
/// expression is not overwritten by the pre-evaluation snapshot (`$a[0] = count($a = [...])`).
#[test]
fn test_array_write_reloads_receiver_after_value_assignment() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[0] = count($a = [10, 20]);
echo $a[0], "|", $a[1];
"#,
    );
    assert_eq!(out, "2|20");
}

/// The first offset write re-reads the receiver after operand evaluation, so a value or key
/// expression that binds it is not overwritten by the pre-evaluation auto-vivified container.
#[test]
fn test_first_array_write_keeps_receiver_reassigned_by_value() {
    let out = compile_and_run(
        r#"<?php
$a[0] = count($a = [10, 20]);
echo $a[0], "|", $a[1];
"#,
    );
    assert_eq!(out, "2|20");
}

/// A key expression that binds the receiver is visible to the store, for a first write and
/// for an already-bound receiver.
#[test]
fn test_array_write_keeps_receiver_reassigned_by_key() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$a[($a = [10, 20]) ? 0 : 0] = 5;
echo $a[0], "|", $a[1], "\n";
$b[($b = [30, 40]) ? 0 : 0] = 7;
echo $b[0], "|", $b[1];
"#,
    );
    assert_eq!(out, "5|20\n7|40");
}

/// The same receiver reload also applies to a first reference write whose key binds it.
#[test]
fn test_first_reference_write_keeps_receiver_reassigned_by_key() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$r[($r = [10, 20]) ? 0 : 0] = &$x;
$r[0] = 9;
echo $x, "|", $r[1];
"#,
    );
    assert_eq!(out, "9|20");
}

/// A killed binding can be rebound to a packed array by the operands. The writer follows the
/// current container kind instead of the hash selected for the still-unbound slot.
#[test]
fn test_killed_binding_first_write_follows_reassigned_container_kind() {
    let out = compile_and_run(
        r#"<?php
$a = null;
$a[0] = count($a = [10, 20]);
echo $a[0], "|", $a[1];
"#,
    );
    assert_eq!(out, "2|20");
}
