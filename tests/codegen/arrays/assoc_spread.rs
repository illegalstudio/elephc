//! Purpose:
//! Regression tests for keys an associative-array spread contributes: a dynamic read, a read the
//! folder must not answer from the literal, and a spread nested inside a callable's argument array.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value is verbatim `LC_ALL=C php` output from PHP 8.4.20.
//! - A spread has no pair of its own in `ExprKind::ArrayLiteralAssoc`: it is carried as a pair
//!   whose KEY is the `ExprKind::Spread` and whose value is an inert null placeholder, detected
//!   through `parser::ast::assoc_spread_source`. A consumer that walks pairs generically reads
//!   that placeholder as an ordinary entry and concludes the key set is complete, which made the
//!   effect analysis call a spread-supplied key a statically missing offset and let the folder
//!   answer an access from the literal entry the spread overwrites.

use crate::support::*;

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
