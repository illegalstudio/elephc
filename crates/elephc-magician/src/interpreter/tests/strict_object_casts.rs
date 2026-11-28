//! Purpose:
//! Verifies original native numeric projections precede checked eval casts.
//!
//! Called from:
//! - Focused walker/VM regressions with reviewed native projection test doubles.
//!
//! Key details:
//! - Projected text has one independent owner and never replaces the original object field.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Stages a reviewed native object whose text would be narrowed by PHP's legacy numeric adapter.
fn object(values: &mut FakeOps, class: &str, source: FakeValue) -> RuntimeCellHandle {
    let text = values.alloc(source);
    let object = values.alloc(FakeValue::Object(vec![("__elephc_num".into(), text)]));
    values.release(text).unwrap();
    values.object_classes.insert(object.as_ptr() as usize, class.to_string());
    object
}

/// Both engines retain exact integers and full numeric strings from native projections.
#[test]
fn strict_object_casts_eval_original_projection_values_and_owners() {
    for vm in [false, true] {
        for class in ["GMP", "SimpleXMLElement", "SimpleXMLIterator"] {
            for (source, cast, expected) in [
                ("9223372036854775807", "int", FakeValue::Int(i64::MAX)),
                ("-9223372036854775808", "int", FakeValue::Int(i64::MIN)),
                ("1.5", "float", FakeValue::Float(1.5)),
                ("1e999", "float", FakeValue::Float(f64::INFINITY)),
            ] {
                let mut context = ElephcEvalContext::new();
                context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
                let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                let original = object(&mut values, class, FakeValue::String(source.into()));
                scope.set("original", original, ScopeCellOwnership::Owned);
                let expression = format!("return ({cast})$original;");
                let Ok(EvalControl::Return(result)) = staged_numeric_execution(&expression, vm, &mut context, &mut scope, &mut values)
                    else { panic!("{class} {expression}"); };
                assert_eq!(values.get(result), expected, "{class} {source} vm={vm}");
                values.release(result).unwrap();
                for owner in scope.drain_owned_cells() { values.release(owner).unwrap(); }
                for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{:?}", values.values[id]); }
            }
        }
    }
}

/// Oversized values, incomplete text and unreviewed objects fail before narrowed adapters can run.
#[test]
fn strict_object_casts_eval_projection_failures_preserve_original_owners() {
    for vm in [false, true] {
        for (class, source, expected) in [("GMP", FakeValue::String("9223372036854775808".into()), "ValueError"),
            ("GMP", FakeValue::String("-9223372036854775809".into()), "ValueError"),
            ("SimpleXMLElement", FakeValue::String("12tail".into()), "ValueError"),
            ("SimpleXMLIterator", FakeValue::String("".into()), "ValueError"),
            ("GMP", FakeValue::Int(7), "TypeError"),
            ("stdClass", FakeValue::String("7".into()), "TypeError")] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let original = object(&mut values, class, source); let before = values.get(original);
            scope.set("original", original, ScopeCellOwnership::Owned);
            assert!(matches!(staged_numeric_execution("return (int)$original;", vm, &mut context, &mut scope, &mut values),
                Err(EvalStatus::UncaughtThrowable)), "{class} vm={vm}");
            assert_eq!(values.get(original), before);
            let error = context.take_pending_throw().unwrap(); assert!(values.object_is_a(error, expected, false).unwrap());
            values.release(error).unwrap();
            for owner in scope.drain_owned_cells() { values.release(owner).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{:?}", values.values[id]); }
        }
    }
}
