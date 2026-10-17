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

/// An overflowing implicit key still evaluates its value before raising the insertion error.
#[test]
fn execute_program_array_max_key_literal_preserves_value_effects() {
    let program = parse_fragment(br#"
function mark() { echo "mark:"; return 2; }
try { $a = [PHP_INT_MAX => 1, mark()]; }
catch (Error $error) { echo $error->getMessage(); }
"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "mark:Cannot add element to the array as the next element is already occupied");
    assert_eq!(values.get(result), FakeValue::Null);
}

/// An exception from the element value takes precedence over implicit-key exhaustion.
#[test]
fn execute_program_array_max_key_literal_preserves_value_exception() {
    let program = parse_fragment(br#"
function mark() { echo "mark:"; throw new Exception("value"); }
try { $a = [PHP_INT_MAX => 1, mark()]; }
catch (Throwable $error) { echo get_class($error), ":", $error->getMessage(); }
"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "mark:Exception:value");
    assert_eq!(values.get(result), FakeValue::Null);
}

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
///
/// The default eval profile is PHP 8.5, which continues from the negative key.
#[test]
fn execute_program_assoc_array_literal_unkeyed_after_negative_int_key() {
    let program =
        parse_fragment(br#"return [-2 => "minus", "tail"][-1];"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}
/// Verifies a PHP 8.3 eval literal continues implicit keys from a negative key.
///
/// Covers the leading negative key, a later explicit non-negative key, and an
/// implicit element after the cursor has already reached 0.
#[test]
fn execute_program_assoc_array_literal_negative_key_continues_on_php_83() {
    let _profile = crate::eval_php_profile::scoped_profile(80_300);
    let program = parse_fragment(
        br#"return [-5 => "a", "b"][-4]
            . [-5 => "a", "b", 3 => "c", "d"][-4]
            . [-5 => "a", "b", 3 => "c", "d"][4]
            . [-5 => "a", 0 => "z", "c"][1];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("bbdc".to_string()));
}
/// Verifies a PHP 8.2 eval literal also continues implicit keys from a negative key.
///
/// The same three shapes as the 8.3 fixture. A literal is a fresh hash, so
/// `[-5 => "a", "b"]` stores `"b"` at -4 on 8.2 as well. A later explicit key 3
/// places the following element at 4, and an explicit 0 places the next element at 1.
#[test]
fn execute_program_assoc_array_literal_negative_key_continues_on_php_82() {
    let _profile = crate::eval_php_profile::scoped_profile(80_200);
    let program = parse_fragment(
        br#"return [-5 => "a", "b"][-4]
            . [-5 => "a", "b", 3 => "c", "d"][-4]
            . [-5 => "a", "b", 3 => "c", "d"][4]
            . [-5 => "a", 0 => "z", "c"][1];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("bbdc".to_string()));
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
