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


/// Saturation preserves the final free integer key before a later implicit insertion fails.
#[test]
fn execute_program_array_literal_fills_last_free_max_key() {
    let program = parse_fragment(br#"
$a = [PHP_INT_MAX - 1 => "a", "b"];
echo $a[PHP_INT_MAX - 1], ":", $a[PHP_INT_MAX];
"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "a:b");
    assert_eq!(values.get(result), FakeValue::Null);
}


/// Creates the private spread expression produced by normalized native default metadata.
fn normalized_default_spread_array(initial: i64, spread: Vec<EvalArrayElement>) -> EvalExpr {
    use crate::eval_ir::{EvalCallArg, EvalConst};
    EvalExpr::Array(vec![
        EvalArrayElement::KeyValue {
            key: EvalExpr::Const(EvalConst::Int(initial)),
            value: EvalExpr::Const(EvalConst::String("a".into())),
        },
        EvalArrayElement::Value(EvalExpr::Call {
            name: "\0elephc_native_default_array_spread".into(),
            args: vec![EvalCallArg::positional(EvalExpr::Array(spread))],
        }),
    ])
}

/// Normalized integer spreads fill the last free key; empty/string spreads require no slot.
#[test]
fn execute_program_native_default_spread_preserves_last_free_key() {
    use crate::eval_ir::{EvalConst, EvalProgram, EvalStmt};
    let value = || EvalArrayElement::Value(EvalExpr::Const(EvalConst::String("b".into())));
    let keyed = EvalArrayElement::KeyValue {
        key: EvalExpr::Const(EvalConst::String("tag".into())),
        value: EvalExpr::Const(EvalConst::String("b".into())),
    };
    for (initial, spread, expected_len, last_key) in [
        (i64::MAX - 1, vec![value()], 2, Some(EvalConst::Int(i64::MAX))),
        (i64::MAX, vec![keyed], 2, Some(EvalConst::String("tag".into()))),
        (i64::MAX, vec![], 1, None),
    ] {
        let program = EvalProgram::new(0, vec![EvalStmt::Return(Some(
            normalized_default_spread_array(initial, spread),
        ))]);
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let array = execute_program(&program, &mut scope, &mut values).expect("construct native default");
        assert_eq!(values.array_len(array).unwrap(), expected_len);
        let key = values.int(initial).unwrap();
        let first = values.array_get(array, key).unwrap();
        assert_eq!(values.get(first), FakeValue::String("a".into()));
        if let Some(key) = last_key {
            let key = match key {
                EvalConst::Int(value) => values.int(value).unwrap(),
                EvalConst::String(value) => values.string(&value).unwrap(),
                _ => unreachable!(),
            };
            let last = values.array_get(array, key).unwrap();
            assert_eq!(values.get(last), FakeValue::String("b".into()));
        }
    }
}

/// Both an already occupied maximum and a second spread entry raise PHP's catchable Error.
#[test]
fn execute_program_native_default_spread_rejects_occupied_max_key() {
    use crate::eval_ir::{EvalConst, EvalProgram, EvalStmt};
    for (initial, count) in [(i64::MAX, 1), (i64::MAX - 1, 2)] {
        let spread = (0..count).map(|_| EvalArrayElement::Value(
            EvalExpr::Const(EvalConst::String("b".into())),
        )).collect();
        let expression = normalized_default_spread_array(initial, spread);
        let mut statements = parse_fragment(br#"try { return 0; } catch (Error $error) { return $error->getMessage(); }"#)
            .expect("parse catch wrapper").into_statements();
        let EvalStmt::Try { body, .. } = &mut statements[0] else { panic!("try wrapper"); };
        body[0] = EvalStmt::Return(Some(expression));
        let program = EvalProgram::new(0, statements);
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let result = execute_program(&program, &mut scope, &mut values).expect("catch insertion error");
        assert_eq!(values.get(result), FakeValue::String(
            "Cannot add element to the array as the next element is already occupied".into(),
        ));
    }
}
