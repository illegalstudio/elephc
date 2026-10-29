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

/// Nested appends autovivify quietly and retain independent snapshots across array replacement.
#[test]
fn execute_nested_dimension_writes_preserve_cow() {
    let program = parse_fragment(br#"
        $rows = [];
        $rows['i'][] = 'first';
        $snapshot = $rows;
        $rows['i'][] = 'second';
        $rows['i'][0] .= '!';
        return count($rows['i']) . ':' . count($snapshot['i']) . ':' . $rows['i'][0] . ':' . $snapshot['i'][0];
    "#).expect("parse nested writes");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute nested writes");
    assert_eq!(values.get(result), FakeValue::String("2:1:first!:first".into()));
}

/// A computed object root runs once while nested arrays write back into its declared property.
#[test]
fn execute_computed_property_dimension_writes() {
    let program = parse_fragment(br#"
        class DimensionRegistry {
            public array $handlers = [];
            public int $calls = 0;
            public function registry(): object { $this->calls += 1; return $this; }
        }
        $registry = new DimensionRegistry();
        $registry->registry()->handlers['save'][] = 'first';
        $registry->registry()->handlers['save'][] = 'second';
        return count($registry->handlers['save']) . ':' . $registry->calls;
    "#).expect("parse computed property writes");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute computed property writes");
    assert_eq!(values.get(result), FakeValue::String("2:2".into()));
}

/// Unattached temporary array writes fail explicitly instead of losing an aliased replacement.
#[test]
fn execute_temporary_array_dimension_write_is_refused() {
    let program = parse_fragment(br#"
        function temporaryDimensionArray(): array { return []; }
        try { temporaryDimensionArray()[0] = 1; }
        catch (Error $error) { return $error->getMessage(); }
    "#).expect("parse temporary array write");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("catch the explicit refusal");
    assert_eq!(values.get(result), FakeValue::String(
        "Unsupported dimension write: temporary arrays have no attached storage".into()));
}

/// Dynamic static class and property operands preserve an attached nested write target.
#[test]
fn execute_dynamic_static_nested_dimension_writes() {
    let program = parse_fragment(br#"
        class DynamicDimensionStore { public static array $items = []; }
        $class = 'DynamicDimensionStore';
        $property = 'items';
        $class::$items['k'][] = 1;
        $class::${$property}['k'][] = 2;
        return count(DynamicDimensionStore::$items['k']);
    "#).expect("parse dynamic static writes");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute attached writes");
    assert_eq!(values.get(result), FakeValue::Int(2));
}

/// Writes through a null property receiver raise catchable operation-specific PHP Errors.
#[test]
fn execute_null_property_write_errors_are_catchable() {
    let program = parse_fragment(br#"
        $null = null; $result = '';
        try { $null->plain = 1; } catch (Error $error) { $result .= $error->getMessage() . '|'; }
        try { $null->nested->leaf = 1; } catch (Error $error) { $result .= $error->getMessage() . '|'; }
        try { $null->items[] = 1; } catch (Error $error) { $result .= $error->getMessage() . '|'; }
        try { $null->text .= 'a'; } catch (Error $error) { $result .= $error->getMessage() . '|'; }
        try { $null->number++; } catch (Error $error) { $result .= $error->getMessage() . '|'; }
        return $result;
    "#).expect("parse null property writes");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("catch null property Errors");
    assert_eq!(values.get(result), FakeValue::String(concat!(
        "Attempt to assign property \"plain\" on null|",
        "Attempt to modify property \"nested\" on null|",
        "Attempt to modify property \"items\" on null|",
        "Attempt to assign property \"text\" on null|",
        "Attempt to increment/decrement property \"number\" on null|",
    ).into()));
}

/// By-value overloaded array children emit a notice and never call offsetSet during writeback.
#[test]
fn execute_overloaded_dimension_copies_discard_writeback() {
    let program = parse_fragment(br#"
        class CopiedDimension implements ArrayAccess {
            public array $data = ['k' => ['x' => 1]];
            public int $sets = 0;
            public function offsetGet(mixed $key): mixed { return $this->data[$key]; }
            public function offsetSet(mixed $key, mixed $value): void { $this->sets += 1; $this->data[$key] = $value; }
            public function offsetExists(mixed $key): bool { return true; }
            public function offsetUnset(mixed $key): void {}
        }
        $copy = new CopiedDimension();
        $copy['k']['x'] = 2;
        $copy['k'][] = 3;
        $copy['k']['x'] += 4;
        return $copy->sets . ':' . $copy->data['k']['x'];
    "#).expect("parse overloaded copies");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute disposable copies");
    assert_eq!(values.get(result), FakeValue::String("0:1".into()));
    assert_eq!(values.warnings.len(), 3);
    assert!(values.warnings.iter().all(|message| message.contains(
        "Indirect modification of overloaded element of CopiedDimension has no effect")));
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
