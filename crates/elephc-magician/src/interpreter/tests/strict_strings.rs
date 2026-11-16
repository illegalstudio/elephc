//! Purpose:
//! Verifies strict string boundaries and explicit formatting in both eval engines.
//!
//! Called from:
//! - Focused staged strict-string interpreter verification.
//!
//! Key details:
//! - Source and value operands share a profile while every fake owner must retire.
//! - Container construction stays in the walker; the selected engine executes each boundary.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Native update adapters inspect original operands before the first mutating callback.
#[test]
fn strict_string_eval_native_property_update_precedes_mutation() {
    for selected in [false, true] {
        for operation in [EvalBinOp::Concat, EvalBinOp::Add] {
            for current in [FakeValue::Object(Vec::new()), FakeValue::Null, FakeValue::String("a".into())] {
                for right in [FakeValue::Int(7), FakeValue::String("x".into())] {
                    let rejected = selected && operation == EvalBinOp::Concat
                        && (!matches!(current, FakeValue::String(_)) || !matches!(right, FakeValue::String(_)));
                    let mut context = ElephcEvalContext::new();
                    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, selected)).unwrap();
                    let mut values = FakeOps::default();
                    let current = values.alloc(current.clone());
                    let object = values.alloc(FakeValue::Object(vec![("__xml_update_operand".into(), current)]));
                    values.release(current).unwrap();
                    values.object_classes.insert(object.as_ptr() as usize, "SimpleXMLElement".into());
                    let right = values.alloc(right);
                    let result = runtime_ops::simplexml_properties::update(&mut values,
                        object, "child", operation, right, false, &mut context);
                    if rejected {
                        assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)));
                        let error = context.take_pending_throw().expect("native concat TypeError owner");
                        assert!(values.object_is_a(error, "TypeError", false).unwrap());
                        values.release(error).unwrap();
                        assert_eq!(values.output, "G");
                    } else {
                        let result = result.unwrap().expect("native update result");
                        assert_eq!(values.get(result), FakeValue::Int(900));
                        values.release(result).unwrap();
                        assert_eq!(values.output, if selected && operation == EvalBinOp::Concat { "GW" } else { "W" });
                    }
                    values.release(right).unwrap(); values.release(object).unwrap();
                    for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
                    for (id, owners) in &values.cell_owners {
                        assert_eq!(*owners, 0, "strict={selected} op={operation:?}: {:?}", values.values[id]);
                    }
                }
            }
        }
    }
}

/// A throwing Stringable hook transfers one Throwable and retires output/interpolation operands.
#[test]
fn strict_string_eval_throwing_formatters_balance_both_engines() {
    for vm in [false, true] {
        for source in ["echo $text;", "print $text;", "return \"[$text]\";"] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            staged_numeric_execution("class EvalThrowingText { function __toString(): string { throw new DomainException('format'); } } \
                $text = new EvalThrowingText();", false, &mut context, &mut scope, &mut values).unwrap();
            let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
            assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}, vm={vm}: {:?}", result.as_ref().err());
            let error = context.take_pending_throw().expect("formatter Throwable owner");
            assert!(values.object_is_a(error, "DomainException", false).unwrap()); values.release(error).unwrap();
            assert!(values.output.is_empty(), "{source}, vm={vm}");
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}, vm={vm}: {:?}", values.values[id]); }
        }
    }
}

/// Failed compound stores preserve locals, aliases, entries and properties at their original value.
#[test]
fn strict_string_eval_compound_reference_and_property_writes_are_atomic() {
    for (source, read) in [
        ("$number .= 'x';", "return $number;"),
        ("$alias =& $number; $alias .= 'x';", "return $number;"),
        ("$container[0] .= 'x';", "return $container[0];"),
        ("$holder->value .= 'x';", "return $holder->value;"),
    ] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        staged_numeric_execution("class StringWriteHolder { public mixed $value = 7; } \
            $holder = new StringWriteHolder(); $number = 7; $container = [7];", false,
            &mut context, &mut scope, &mut values).unwrap();
        let result = staged_numeric_execution(source, false, &mut context, &mut scope, &mut values);
        assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}: {:?}", result.as_ref().err());
        let error = context.take_pending_throw().expect("compound TypeError owner");
        assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
        let EvalControl::Return(value) = staged_numeric_execution(read, false,
            &mut context, &mut scope, &mut values).unwrap() else { panic!("read after failure"); };
        assert_eq!(values.get(value), FakeValue::Int(7), "{source}"); values.release(value).unwrap();
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
    }
}

/// Explicit conversions and Stringable formatting keep their own contracts and balanced owners.
#[test]
fn strict_string_eval_explicit_formatting_and_stringable_controls() {
    for vm in [false, true] {
        for selected in [false, true] {
            for (source, expected, output) in [
                ("return (string)$container;", Some(FakeValue::String("Array".into())), ""),
                ("return $text . 'x';", if selected { None } else { Some(FakeValue::String("vx".into())) },
                    if selected { "" } else { "H" }),
                ("return \"[$text]\";", Some(FakeValue::String("[v]".into())), "H"),
                ("echo $text; return 0;", Some(FakeValue::Int(0)), "Hv"),
            ] {
                let mut context = ElephcEvalContext::new();
                context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, selected)).unwrap();
                let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                staged_numeric_execution("class EvalStringText { function __toString(): string { echo 'H'; return 'v'; } } \
                    $text = new EvalStringText(); $container = [1];", false, &mut context, &mut scope, &mut values).unwrap();
                let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
                if let Some(expected) = expected {
                    let EvalControl::Return(value) = result.unwrap_or_else(|status|
                        panic!("{source}, strict={selected}, vm={vm}: {status:?}")) else { panic!("return value"); };
                    assert_eq!(values.get(value), expected, "{source}, strict={selected}, vm={vm}");
                    values.release(value).unwrap();
                } else {
                    assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}: {:?}", result.as_ref().err());
                    let error = context.take_pending_throw().expect("string operand TypeError");
                    assert!(values.object_is_a(error, "TypeError", false).unwrap());
                    values.release(error).unwrap();
                }
                assert_eq!(values.output, output, "{source}, strict={selected}, vm={vm}");
                for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
                for (id, owners) in &values.cell_owners {
                    assert_eq!(*owners, 0, "{source}, strict={selected}, vm={vm}: {:?}", values.values[id]);
                }
            }
        }
    }
}

/// Concatenation rejects original kinds while output and interpolation keep explicit formatting.
#[test]
fn strict_string_eval_engines_boundaries_and_owners() {
    let mut failures = Vec::new();
    for vm in [false, true] {
        for (source, expected, output) in [
            ("return 1 . 'x';", None, ""),
            ("return 'x' . false;", None, ""),
            ("return $number . 'x';", None, ""),
            ("return $container . 'x';", None, ""),
            ("$number .= 'x';", None, ""),
            ("return $number . (print 'R');", None, "R"),
            ("echo $container;", None, ""),
            ("print $container;", None, ""),
            ("return \"[$container]\";", None, ""),
            ("return 'a' . 'b';", Some(FakeValue::String("ab".into())), ""),
            ("$text = 'a'; $text .= 'b'; return $text;", Some(FakeValue::String("ab".into())), ""),
            ("return \"[$number]\";", Some(FakeValue::String("[7]".into())), ""),
            ("return 'x' . (string)$number;", Some(FakeValue::String("x7".into())), ""),
            ("echo $number; return print false;", Some(FakeValue::Int(1)), "7"),
        ] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            staged_numeric_execution("$number = 7; $container = [1];", false,
                &mut context, &mut scope, &mut values).unwrap();
            let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
            match (expected, result) {
                (Some(expected), Ok(EvalControl::Return(value))) => {
                    if values.get(value) != expected {
                        failures.push(format!("{source}, vm={vm}: {:?}", values.get(value)));
                    }
                    values.release(value).unwrap();
                }
                (None, Err(EvalStatus::UncaughtThrowable)) => {
                    let error = context.take_pending_throw().expect("string TypeError owner");
                    assert!(values.object_is_a(error, "TypeError", false).unwrap());
                    values.release(error).unwrap();
                }
                (_, result) => {
                    failures.push(format!("{source}, vm={vm}: unexpected result {:?}", result.as_ref().err()));
                    if let Ok(EvalControl::Return(value)) = result { values.release(value).unwrap(); }
                }
            }
            if values.output != output { failures.push(format!("{source}, vm={vm}: output {:?}", values.output)); }
            if let Some(error) = context.take_pending_throw() { values.release(error).unwrap(); }
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners {
                if *owners != 0 { failures.push(format!("{source}, vm={vm}: {owners} owner(s) for {:?}", values.values[id])); }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
