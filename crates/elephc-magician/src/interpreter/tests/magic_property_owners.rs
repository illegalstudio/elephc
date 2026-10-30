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
