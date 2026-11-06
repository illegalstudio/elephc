//! Purpose:
//! Regression tests for keys an associative-array spread contributes: a dynamic read, a read the
//! folder must not answer from the literal, and a spread nested inside a callable's argument array.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value is verbatim `LC_ALL=C php` output from PHP 8.4.20.
//! - A literal that mixes explicit keys with a spread is `ExprKind::ArrayLiteralMixed`, an
//!   ordered entry list; neither single-shape node can hold it. A pass that only special-cases
//!   `ArrayLiteralAssoc` must therefore leave the mixed node dynamic, which is what stops the
//!   effect analysis from calling a spread-supplied key a statically missing offset and the
//!   folder from answering an access from the literal entry the spread overwrites.

use crate::support::*;

/// Spreading arrays of arrays beside an explicit key preserves each nested array's payload.
#[test]
fn test_assoc_spread_preserves_nested_array_values() {
    let out = compile_and_run(r#"<?php
$rows = [...[[1], [2]], 'last' => [3]];
foreach ($rows as $row) { echo $row[0], "\n"; }
$source = ['first' => [4], 'second' => [5]];
$rows = [...$source, 'last' => [6]];
foreach ($rows as $row) { echo $row[0], "\n"; }
"#);
    assert_eq!(out, "1\n2\n3\n4\n5\n6\n");
}

/// Verifies a key contributed only by a spread is readable through a runtime key expression, and
/// that a key the literal writes itself still reads back unchanged.
#[test]
fn test_assoc_spread_supplies_a_dynamic_key_read() {
    let out = compile_and_run(
        r#"<?php
$extra = ["b" => "spread"];
$key = "b";
var_dump(["a" => 1, ...$extra][$key]);
var_dump(["a" => 1, ...$extra]["a"]);
"#,
    );
    assert_eq!(out, "string(6) \"spread\"\nint(1)\n");
}

/// Verifies a literal-key access is not folded to the pair the spread overwrites.
///
/// PHP merges the spread source in source order, so `["a" => 1, ...["a" => ...]]["a"]` is the
/// spread's value, not the `1` the earlier pair carries.
#[test]
fn test_assoc_spread_overwrites_an_earlier_literal_key() {
    let out = compile_and_run(
        r#"<?php
$extra = ["a" => "overwritten"];
var_dump(["a" => 1, ...$extra]["a"]);
"#,
    );
    assert_eq!(out, "string(11) \"overwritten\"\n");
}

/// Verifies a spread nested inside a callable's argument array still binds every named parameter.
///
/// The nested spread must stay dynamic: flattening the literal would push the null placeholder as
/// an argument and drop the source, losing `count` entirely.
#[test]
fn test_assoc_spread_inside_callable_argument_array() {
    let out = compile_and_run(
        r#"<?php
function label(string $name, int $count): string { return $name . ":" . $count; }
$rest = ["count" => 7];
var_dump(call_user_func_array("label", ["name" => "items", ...$rest]));
"#,
    );
    assert_eq!(out, "string(7) \"items:7\"\n");
}
