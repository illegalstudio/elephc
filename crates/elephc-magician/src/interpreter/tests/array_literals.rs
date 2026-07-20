//! Purpose:
//! Interpreter tests for EvalIR array literal key allocation and reads.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases focus on PHP array-key normalization during literal evaluation.

use super::super::*;
use super::support::*;

/// Verifies indexed array literals and reads execute through runtime hooks.
#[test]
fn execute_program_reads_indexed_array_literal() {
    let program = parse_fragment(br#"return ["a", "b"][1];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("b".to_string()));
}
/// Verifies legacy `array(...)` literals execute through the existing array runtime hooks.
#[test]
fn execute_program_reads_legacy_array_literal() {
    let program =
        parse_fragment(br#"return array("a", "b" => "bee",)[0];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("a".to_string()));
}
/// Verifies associative array literals and string-key reads execute through runtime hooks.
#[test]
fn execute_program_reads_assoc_array_literal() {
    let program =
        parse_fragment(br#"return ["name" => "Ada"]["name"];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("Ada".to_string()));
}
/// Verifies unkeyed assoc literal entries start at zero after string keys.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_string_key_starts_at_zero() {
    let program =
        parse_fragment(br#"return ["name" => "Ada", "Grace"][0];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
}
/// Verifies unkeyed assoc literal entries use one plus the largest integer key.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_positive_int_key() {
    let program =
        parse_fragment(br#"return [2 => "two", "tail"][3];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}
/// Verifies unkeyed assoc literal entries preserve PHP's negative-key rule.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_negative_int_key() {
    let program =
        parse_fragment(br#"return [-2 => "minus", "tail"][-1];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}
/// Verifies numeric string literal keys update the next automatic key.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_numeric_string_key() {
    let program =
        parse_fragment(br#"return ["2" => "two", "tail"][3];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}
/// Verifies leading-zero string literal keys do not update the automatic key.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_leading_zero_string_key() {
    let program =
        parse_fragment(br#"return ["02" => "two", "tail"][0];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}
/// Verifies null literal keys normalize to empty strings without advancing automatic keys.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_null_key() {
    let program =
        parse_fragment(br#"return [null => "empty", "tail"][0];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}
/// Verifies null literal keys are readable through the empty-string key.
#[test]
fn execute_program_assoc_array_literal_reads_null_key_as_empty_string() {
    let program = parse_fragment(br#"return [null => "empty"][""];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("empty".to_string()));
}
/// Verifies boolean literal keys update the next automatic key after integer normalization.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_bool_key() {
    let program =
        parse_fragment(br#"return [true => "yes", "tail"][2];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}
/// Verifies false literal keys update the next automatic key from zero.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_false_key() {
    let program =
        parse_fragment(br#"return [false => "no", "tail"][1];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}
/// Verifies float literal keys update the next automatic key after truncation.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_float_key() {
    let program =
        parse_fragment(br#"return [2.7 => "two", "tail"][3];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}
