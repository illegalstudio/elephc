//! Purpose:
//! Checks the owners of property names synthesized for eval magic methods.
//!
//! Called from:
//! - The Magician interpreter unit-test harness.
//!
//! Key details:
//! - Method binders borrow caller operands on successful and throwing calls.
//! - Name results returned by a getter must survive retirement of the argument owner.

use super::super::*;
use super::support::*;

/// Physical scalar fields return their stored type while accessor updates return computed values.
#[test]
fn property_update_expression_results_follow_storage_coercion() {
    let source = br#"
class StorageResultBox {
    public int $number = 1;
    public string $text = '3';
    public float $decimal = 1.0;
    public int $hook { get { echo 'get|'; return 1; } set(int|float $value) { echo 'set|'; } }
}
$box = new StorageResultBox();
$result = ($box->number += 1.0); echo gettype($result), ':', $result, ':', $box->number, '|';
$result = ($box->text += 2); echo gettype($result), ':', $result, ':', $box->text, '|';
$result = ++$box->text; echo gettype($result), ':', $result, ':', $box->text, '|';
$result = ($box->decimal += 0.5); echo gettype($result), ':', $result, '|';
$result = ($box->hook += 0.5); echo gettype($result), ':', $result, '|';
try { $box->number .= 'x'; } catch (TypeError $error) { echo 'invalid:', $box->number, '|'; }
unset($box, $result, $error);
"#;
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let program = parse_fragment(source).unwrap();
    let returned = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    eval_release_value(&mut context, &mut values, returned).unwrap();
    assert_eq!(values.output, "integer:2:2|string:5:5|string:6:6|double:1.5|get|set|double:1.5|invalid:2|");
}

/// Property expression values survive writeback and retire after normal and throwing setters.
#[test]
fn property_update_expression_results_keep_balanced_owners() {
    let source = br#"
class ExpressionValueBox {
    public int $number = 1;
    public string $text = 'a';
    public function receiver(): self { echo 'r|'; return $this; }
}
class ExpressionThrowingSetter {
    public function __get(string $name): mixed { return 'a'; }
    public function __set(string $name, mixed $value): void { throw new RuntimeException('setter'); }
}
$box = new ExpressionValueBox(); $name = 'number';
echo ($box->receiver()->number += 3), '|';
echo ++$box->{$name}, '|', $box->{$name}++, '|', $box->number, '|';
echo ($box->text .= 'b'), ':', $box->text, '|';
$throwing = new ExpressionThrowingSetter();
try { echo ($throwing->missing .= 'b'); }
catch (RuntimeException $error) { echo 'caught|'; }
unset($box, $name, $throwing, $error);
"#;
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let program = parse_fragment(source).unwrap();
    let returned = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    eval_release_value(&mut context, &mut values, returned).unwrap();
    assert_eq!(values.output, "r|4|5|5|6|ab:ab|caught|");
    for (id, value) in &values.values {
        if matches!(value, FakeValue::String(text) if text == "ab") {
            assert_eq!(values.cell_owners[id], 0, "computed result {id} escaped: {value:?}");
        }
    }
}

/// Fresh magic names retire on success and failure without consuming an aliased getter result.
#[test]
fn magic_property_name_owners_cover_success_and_failure() {
    for (method, parameters, operation, result) in [
        ("__get", "string $name", "$object->missing", "return $name;"),
        ("__set", "string $name, mixed $value", "$object->missing = 7", "return;"),
        ("__isset", "string $name", "isset($object->missing)", "return true;"),
    ] {
        for fail in [false, true] {
            let body = if fail { "missingMagicOwnerBody();" } else { result };
            let source = format!(
                "class MagicNameOwner {{ public function {method}({parameters}) {{ {body} }} }} \
                 $object = new MagicNameOwner(); {operation}; unset($object);"
            );
            let mut values = FakeOps::default();
            let mut context = ElephcEvalContext::new();
            let mut scope = ElephcEvalScope::new();
            let program = parse_fragment(source.as_bytes()).unwrap();
            let outcome = execute_program_outcome_with_context(
                &mut context, &program, &mut scope, &mut values,
            );
            assert!(outcome.is_ok(), "{source}");
            assert_eq!(matches!(outcome.unwrap(), EvalOutcome::Throwable(_)), fail, "{source}");
            let names: Vec<_> = values.values.iter().filter_map(|(id, value)| {
                matches!(value, FakeValue::String(name) if name == "missing").then_some(*id)
            }).collect();
            assert!(!names.is_empty(), "{source}");
            for name in names {
                assert_eq!(values.cell_owners[&name], 0, "{source}: name={name}");
            }
        }
    }
}

/// Magic instance and static method names and argument arrays keep no caller leases after dispatch.
#[test]
fn synthesized_magic_call_arguments_are_retired() {
    for (modifier, method, call) in [
        ("", "__call", "$object->missingOwnerMethod(7)"),
        ("static", "__callStatic", "MagicCallOwner::missingOwnerMethod(7)"),
    ] {
        let source = format!("class MagicCallOwner {{ public {modifier} function {method}($name, $args) \
            {{ return $name; }} }} $object = new MagicCallOwner(); {call}; unset($object);");
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let program = parse_fragment(source.as_bytes()).unwrap();
        execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
        for (id, value) in &values.values {
            if matches!(value, FakeValue::String(name) if name == "missingOwnerMethod")
                || matches!(value, FakeValue::Array(_) | FakeValue::Int(0)) {
                assert_eq!(values.cell_owners[id], 0, "{method}: {value:?}");
            }
        }
    }
}

/// Stream hooks consume synthesized path, mode and length arguments after normal calls.
#[test]
fn synthesized_user_stream_arguments_are_retired() {
    let template = r#"
class OwnerStreamWrapper {
    public function stream_open($path, $mode, $options, &$opened): bool { return true; }
    public function stream_read($length): string { return ''; }
    public function stream_eof(): bool { return true; }
    public function stream_close(): void {}
}
stream_wrapper_register('ownerstream', 'OwnerStreamWrapper');
$handle = fopen('ownerstream://value', 'r');
fread($handle, 3); fclose($handle); unset($handle);
"#;
    for fail in [false, true] {
        let source = if fail { template.replace("return '';", "missingStreamOwnerBody();") }
            else { template.to_string() };
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let program = parse_fragment(source.as_bytes()).unwrap();
        let outcome = execute_program_outcome_with_context(
            &mut context, &program, &mut scope, &mut values,
        ).unwrap();
        assert_eq!(matches!(outcome, EvalOutcome::Throwable(_)), fail);
        for (id, value) in &values.values {
            if matches!(value, FakeValue::String(name) if name == "ownerstream://value" || name == "r") {
                assert_eq!(values.cell_owners[id], 0, "{value:?}");
            }
        }
    }
}
