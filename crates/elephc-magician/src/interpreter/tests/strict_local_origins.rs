//! Purpose:
//! Checks eval-created binding origins and producer openness across both execution engines.
//!
//! Called from:
//! - Focused Magician interpreter unit tests.
//!
//! Key details:
//! - Failed stores preserve the original value and balance every owned runtime cell.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Walker and VM reject a binding-kind change before later effects or operand retirement.
#[test]
fn strict_eval_local_origins_preserve_first_values_in_both_engines() {
    for vm in [false, true] {
        for (source, expected) in [
            ("$value = 1; $value = 'bad'; echo 'wrong';", FakeValue::Int(1)),
            ("$value = 2.5; $value = 3; echo 'wrong';", FakeValue::Float(2.5)),
            ("$value = true; $value = 0; echo 'wrong';", FakeValue::Bool(true)),
            ("$value = null; $value = 1; echo 'wrong';", FakeValue::Null),
            ("$value = ($nested = 1); $value = 'bad';", FakeValue::Int(1)),
        ] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            assert!(matches!(staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values),
                Err(EvalStatus::UncaughtThrowable)), "{source}, vm={vm}");
            assert_eq!(values.get(scope.visible_cell("value").unwrap()), expected, "{source}, vm={vm}");
            assert!(values.output.is_empty(), "{source}");
            let thrown = context.take_pending_throw().unwrap();
            assert!(values.object_is_a(thrown, "TypeError", false).unwrap());
            values.release(thrown).unwrap();
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
        }
    }
}

/// Imported values and untyped function producers remain open while ordinary value copies stay typed.
#[test]
fn strict_eval_local_origins_preserve_open_producers_and_independent_copies() {
    for vm in [false, true] {
        for source in [
            "$copy = $input; $copy = 1; $copy = 'open'; return $copy;",
            "function openOrigin($input) { return $input; } $copy = openOrigin(1); $copy = 'open'; return $copy;",
            "$first = ($second = 1); $first = 2; return $second;",
            "$value = $input > 0 ? 1 : 2.5; $value = 2.5; return $value;",
        ] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let input = values.float(2.5).unwrap();
            scope.set("input", input, ScopeCellOwnership::Owned);
            let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values).unwrap();
            let EvalControl::Return(value) = result else { panic!("{source}"); };
            let expected = if source.contains("$second") { FakeValue::Int(1) }
                else if source.contains("? 1 : 2.5") { FakeValue::Float(2.5) }
                else { FakeValue::String("open".into()) };
            assert_eq!(values.get(value), expected, "{source}, vm={vm}");
            values.release(value).unwrap();
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
        }
    }
}

/// Logical native source metadata constrains copies independently from the current snapshot shape.
#[test]
fn strict_eval_local_native_producer_contracts_are_independent_from_storage() {
    for vm in [false, true] {
        for source in ["$copy = $input; $copy = 'bad';",
            "if(false) {function strlen($text) {}} $copy = strlen('hi'); $copy = 'bad';"] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let input = values.int(1).unwrap();
            scope.set("input", input, ScopeCellOwnership::Owned);
            scope.set_native_binding_type("input".into(), EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false));
            assert!(matches!(staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values),
                Err(EvalStatus::UncaughtThrowable)), "{source}, vm={vm}");
            let expected = if source.contains("strlen") { 2 } else { 1 };
            assert_eq!(values.get(scope.visible_cell("copy").unwrap()), FakeValue::Int(expected));
            values.release(context.take_pending_throw().unwrap()).unwrap();
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
        }
    }
}
