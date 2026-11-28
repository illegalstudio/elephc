//! Purpose:
//! Preserves eval parameter and inferred closure binding contracts across caller engines.
//!
//! Called from:
//! - Focused strict local binding origin regressions.
//!
//! Key details:
//! - Typed callee bodies use the walker fallback; their callers cover walker and bytecode.
//! - Mixed/untyped inputs stay open and all failed and successful owners are retired.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Parameter local contracts follow declarations rather than incoming values or scalar coercion.
#[test]
fn strict_parameter_locals_eval_declarations_and_caller_owners() {
    for vm in [false, true] {
        for strict in [false, true] {
            for (declared, input, next, tag, strict_accepts) in [
                ("int", "1", "2.5", EVAL_TAG_FLOAT, false),
                ("int", "'2'", "3", EVAL_TAG_INT, true),
                ("float", "1", "2", EVAL_TAG_INT, false),
                ("float", "1", "2.5", EVAL_TAG_FLOAT, true),
                ("string", "'old'", "1", EVAL_TAG_INT, false),
                ("bool", "false", "1", EVAL_TAG_INT, false),
                ("false", "false", "false", EVAL_TAG_BOOL, true),
                ("false", "false", "true", EVAL_TAG_BOOL, false),
                ("int|float", "1", "2.5", EVAL_TAG_FLOAT, true),
                ("?int", "null", "1", EVAL_TAG_INT, true),
                ("?int", "null", "2.5", EVAL_TAG_FLOAT, false),
                ("mixed", "1", "2.5", EVAL_TAG_FLOAT, true),
                ("", "1", "2.5", EVAL_TAG_FLOAT, true),
                ("callable", "'parameterCallback'", "1", EVAL_TAG_INT, false),
                ("callable", "'parameterCallback'", "'missingCallback'", EVAL_TAG_STRING, false),
                ("callable", "'parameterCallback'", "'PARAMETERCALLBACK'", EVAL_TAG_STRING, true),
                ("iterable", "$container", "1", EVAL_TAG_INT, false),
                ("array", "$container", "1", EVAL_TAG_INT, false),
            ] {
                let mut context = ElephcEvalContext::new();
                context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict)).unwrap();
                let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                let setup = format!("function parameterCallback(): int {{return 1;}} \
                    function stableParameter({declared} $value,mixed $next) {{$value=$next;return $value;}} $container=[1];");
                staged_numeric_execution(&setup, false, &mut context, &mut scope, &mut values).unwrap();
                let shape = if vm { "if(false){function stableParameter($value,$next){}}" } else { "" };
                let source = format!("{shape} return stableParameter({input},{next});");
                let result = staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values);
                if !strict || strict_accepts {
                    let Ok(EvalControl::Return(value)) = result else { panic!("{source}, {declared}, strict={strict}, vm={vm}: {:?}", result.as_ref().err()); };
                    assert_eq!(values.type_tag(value).unwrap(), tag, "{source}, {declared}, strict={strict}, vm={vm}");
                    values.release(value).unwrap();
                } else {
                    assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}, {declared}, vm={vm}: {:?}", result.as_ref().err());
                    let error = context.take_pending_throw().expect("parameter-local failure owner");
                    assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
                }
                for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
                for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}, {declared}, strict={strict}, vm={vm}: {:?}", values.values[id]); }
            }
        }
    }
}

/// Inferred closures retain callable capability rather than the signature of their first value.
#[test]
fn strict_parameter_locals_inferred_closure_capability_and_owners() {
    for vm in [false, true] {
        for strict in [false, true] {
            for (source, accepts) in [("$value=1;", false), ("$value='missingCallback';", false),
                ("$value='closureReplacement';", true), ("$value='CLOSUREREPLACEMENT';", true)] {
                let mut context = ElephcEvalContext::new();
                context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict)).unwrap();
                let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                staged_numeric_execution("function closureReplacement(): int {return 1;} \
                    $value=static function(int $number): int {return $number;};", false,
                    &mut context, &mut scope, &mut values).unwrap();
                let before = values.get(scope.visible_cell("value").unwrap());
                let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
                if !strict || accepts { result.unwrap_or_else(|status| panic!("{source}, vm={vm}: {status:?}")); }
                else {
                    assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}, vm={vm}: {:?}", result.as_ref().err());
                    assert_eq!(values.get(scope.visible_cell("value").unwrap()), before);
                    let error = context.take_pending_throw().expect("inferred callable failure owner");
                    assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
                }
                for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
                for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}, strict={strict}, vm={vm}: {:?}", values.values[id]); }
            }
        }
    }
}
