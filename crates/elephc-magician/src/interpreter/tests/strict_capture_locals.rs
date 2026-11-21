//! Purpose:
//! Verifies immutable producer origins on escaping by-value closure captures.
//!
//! Called from:
//! - Focused strict capture tests with walker and bytecode callers.
//!
//! Key details:
//! - Declared alternatives survive capture without narrowing mixed or retaining extra PHP owners.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Escaping captures preserve declared kinds across both caller engines and every cleanup path.
#[test]
fn strict_capture_locals_eval_origins_alternatives_and_owners() {
    for vm in [false, true] {
        for strict in [false, true] {
            for (declared, initial, next, tag, accepts) in [
                ("int", "1", "2.5", EVAL_TAG_FLOAT, false),
                ("int", "1", "2", EVAL_TAG_INT, true),
                ("int|float", "1", "2.5", EVAL_TAG_FLOAT, true),
                ("?int", "null", "2", EVAL_TAG_INT, true),
                ("?int", "null", "2.5", EVAL_TAG_FLOAT, false),
                ("mixed", "1", "2.5", EVAL_TAG_FLOAT, true),
                ("", "1", "2.5", EVAL_TAG_FLOAT, true),
                ("string", "'old'", "1", EVAL_TAG_INT, false),
                ("bool", "false", "1", EVAL_TAG_INT, false),
                ("array", "[1]", "1", EVAL_TAG_INT, false),
                ("array", "[1]", "[2.5]", EVAL_TAG_ARRAY, true),
                ("iterable", "[1]", "1", EVAL_TAG_INT, false),
                ("callable", "'captureOriginal'", "'missingCapture'", EVAL_TAG_STRING, false),
                ("callable", "'captureOriginal'", "'CAPTUREREPLACEMENT'", EVAL_TAG_STRING, true),
            ] {
                let mut context = ElephcEvalContext::new();
                context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict)).unwrap();
                let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                assert!(context.set_global_scope(&mut scope));
                let setup = format!("function captureOriginal(): int {{return 1;}} function captureReplacement(): int {{return 2;}} \
                    function makeStableCapture({declared} $value): callable {{ \
                        return static function(mixed $next) use($value) {{$value=$next;return $value;}}; }} \
                    function invokeStableCapture(mixed $next) {{global $write;return $write($next);}} \
                    $initial={initial};$candidate={next};$write=makeStableCapture($initial);");
                staged_numeric_execution(&setup, false, &mut context, &mut scope, &mut values).unwrap();
                let before = values.get(scope.visible_cell("initial").unwrap());
                let source = if vm { "if(false){function invokeStableCapture($next){}} return invokeStableCapture($candidate);" }
                    else { "return $write($candidate);" };
                let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
                if !strict || accepts {
                    let Ok(EvalControl::Return(value)) = result else { panic!("{declared}, {initial}, {next}, strict={strict}, vm={vm}: {:?}", result.as_ref().err()); };
                    assert_eq!(values.type_tag(value).unwrap(), tag, "{declared}, {initial}, {next}, strict={strict}, vm={vm}");
                    values.release(value).unwrap();
                } else {
                    assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{declared}, {initial}, {next}, vm={vm}: {:?}", result.as_ref().err());
                    let error = context.take_pending_throw().expect("captured binding TypeError owner");
                    assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
                }
                assert_eq!(values.get(scope.visible_cell("initial").unwrap()), before);
                for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
                for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{declared}, {initial}, {next}, strict={strict}, vm={vm}: {:?}", values.values[id]); }
            }
        }
    }
}
