//! Purpose:
//! Verifies argument leases in Magician's shared-runtime builtin dispatcher.
//!
//! Called from:
//! - The Magician interpreter unit-test harness.
//!
//! Key details:
//! - FakeOps counts cell owners on successful calls and injected cleanup failures.
//! - Native heap-debug fixtures separately cover the generated runtime ABI.

use super::super::*;
use super::support::*;

/// Shared-runtime calls retire temporary inputs and preserve borrowed scope storage.
#[test]
fn shared_runtime_calls_balance_operand_owners() {
    for (source, expected) in [
        ("return array_key_exists('missing', []);", FakeValue::Bool(false)),
        ("return strrev(strrev('owned'));", FakeValue::String("owned".into())),
        ("$key = 'borrowed'; return array_key_exists($key, []);", FakeValue::Bool(false)),
        ("$key = 'borrowed'; return array_key_exists($key, ($key = []));", FakeValue::Bool(false)),
        ("return intval('17');", FakeValue::Int(17)),
        ("return intval('21', 16);", FakeValue::Int(33)),
        ("return round(2.6, 0);", FakeValue::Float(3.0)),
    ] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let program = parse_fragment(source.as_bytes()).unwrap();
        let returned = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
        assert_eq!(values.get(returned), expected, "{source}");
        if let Some(key) = scope.visible_cell("key") {
            assert_eq!(values.cell_owners[&(key.as_ptr() as usize)], 1, "{source}");
        }
        assert_eq!(values.cell_owners[&(returned.as_ptr() as usize)], 1, "{source}");
        values.release(returned).unwrap();
        for cell in scope.drain_owned_cells() { values.release(cell).unwrap(); }
        for (id, count) in &values.cell_owners {
            assert_eq!(*count, 0, "{source}: {:?}", values.values[id]);
        }
    }
}

/// Operand evaluation and runtime-hook failures retire all previously evaluated arguments.
#[test]
fn shared_runtime_call_failures_release_evaluated_operands() {
    for source in [
        "return array_key_exists('released', missingSharedOperand());",
        "return array_key_exists('released', 0);",
    ] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let program = parse_fragment(source.as_bytes()).unwrap();
        assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values),
            Err(EvalStatus::UnsupportedConstruct), "{source}");
        for (id, count) in &values.cell_owners {
            assert_eq!(*count, 0, "{source}: {:?}", values.values[id]);
        }
    }
}

/// Cleanup failures discard a successful result but do not override a dispatch failure.
#[test]
fn shared_runtime_operand_cleanup_preserves_primary_errors() {
    for (source, expected) in [
        ("return array_key_exists('released', []);", EvalStatus::UncaughtThrowable),
        ("return array_key_exists('released', 0);", EvalStatus::UnsupportedConstruct),
    ] {
        let mut values = FakeOps { fail_release_call: Some(0), ..FakeOps::default() };
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let program = parse_fragment(source.as_bytes()).unwrap();
        assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values),
            Err(expected), "{source}");
        for (id, count) in &values.cell_owners {
            assert_eq!(*count, 0, "{source}: {:?}", values.values[id]);
        }
    }
}
