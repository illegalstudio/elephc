//! Purpose:
//! Regression tests for value-based keys in persistent eval array references.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through the interpreter test harness.
//!
//! Key details:
//! - Aliases must keep the evaluated key after variables or temporary cells change.
//! - Copies, nested elements and mutating calls must share the original storage target.

use super::super::*;
use super::support::*;

/// Rebinding the source index does not move an already established element reference.
#[test]
fn reference_key_snapshot_survives_index_rebinding() {
    let program = parse_fragment(br#"
$values = [10, 20]; $key = 0;
$refs = [&$values[$key]]; $key = 1;
$refs[0] = 30;
return $values[0] . ":" . $values[1];
"#).expect("parse reference fixture");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute reference fixture");
    assert_eq!(values.get(result), FakeValue::String("30:20".into()));
}

/// Persistent keys preserve PHP's distinction between canonical and leading-zero strings.
#[test]
fn reference_key_snapshot_preserves_string_normalization() {
    let program = parse_fragment(br#"
$values = ["00" => 10, 0 => 20]; $key = "00";
$refs = [&$values[$key]]; $key = "0";
$refs[0] = 30;
return $values["00"] . ":" . $values[0];
"#).expect("parse reference fixture");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute reference fixture");
    assert_eq!(values.get(result), FakeValue::String("30:20".into()));
}

/// A computed key runs once and its reference remains writable after copying the alias array.
#[test]
fn reference_key_snapshot_copies_keep_computed_target() {
    let program = parse_fragment(br#"
function offset() { echo "K"; return 0; }
$values = [10]; $refs = [&$values[offset()]];
$copy = $refs; unset($refs); $copy[0] = 30;
return $values[0];
"#).expect("parse reference fixture");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute reference fixture");
    assert_eq!(values.output, "K");
    assert_eq!(values.get(result), FakeValue::Int(30));
}

/// Nested targets retain their independently evaluated integer keys through writes.
#[test]
fn reference_key_snapshot_nested_elements_write_back() {
    let program = parse_fragment(br#"
$values = [[10, 20]]; $key = 0;
$refs = [&$values[$key][$key]]; $key = 1;
$refs[0] = 30;
return $values[0][0] . ":" . $values[0][1];
"#).expect("parse reference fixture");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute reference fixture");
    assert_eq!(values.get(result), FakeValue::String("30:20".into()));
}

/// User-defined by-reference parameters use the same stable array-element target.
#[test]
fn reference_key_snapshot_user_function_writes_back() {
    let program = parse_fragment(br#"
function update(&$value) { $value = 30; }
$values = [10]; update($values[0]); return $values[0];
"#).expect("parse reference fixture");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute reference fixture");
    assert_eq!(values.get(result), FakeValue::Int(30));
}

/// Builtin mutation of a nested array publishes through the stable target.
#[test]
fn reference_key_snapshot_mutating_builtin_writes_back() {
    let program = parse_fragment(br#"
$values = [[3, 1, 2]]; sort($values[0]); return implode(",", $values[0]);
"#).expect("parse reference fixture");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute reference fixture");
    assert_eq!(values.get(result), FakeValue::String("1,2,3".into()));
}
