//! Purpose:
//! Verifies semantic profile boundaries in eval execution.
//!
//! Called from:
//! - Focused Magician interpreter unit tests.
//!
//! Key details:
//! - Uses the fake runtime to observe failures without loading native libraries.

use super::super::*;
use super::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Runtime source cannot bypass the strict boundary through the interpreter API.
#[test]
fn strict_profile_dynamic_code_blocks_nested_eval_and_include() {
    for source in [b"eval('return 1;');".as_slice(), b"include '/must-not-read-strict-source.php';"] {
        let program = parse_fragment(source).unwrap();
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::DynamicCode, true)).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values),
            Err(EvalStatus::UncaughtThrowable));
        assert!(values.output.is_empty());
    }
}

/// An external host cannot execute with a profile whose guarantees are still unimplemented.
#[test]
fn strict_profile_pending_guarantees_cannot_execute_through_interpreter_api() {
    let program = parse_fragment(b"echo 'must not execute';").unwrap();
    let mut context = ElephcEvalContext::new();
    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)).unwrap();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values),
        Err(EvalStatus::UnsupportedConstruct));
    assert!(values.output.is_empty());
}

/// Runs the same boolean rules through both execution engines with explicit cleanup.
#[test]
fn strict_profile_bools_walker_and_bytecode_require_boolean_conditions() {
    for source in [
        "if (1) { echo 'bad'; }", "echo !1;", "echo 1 ?: false;",
        "echo true && 1;", "echo true xor 1;", "while (1) { break; }",
    ] {
        for walker in [true, false] {
            let program = parse_fragment(source.as_bytes()).unwrap();
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)).unwrap();
            let mut scope = ElephcEvalScope::new();
            let mut values = FakeOps::default();
            let result = if walker {
                execute_program_statements(&program, &mut context, &mut scope, &mut values).map(|_| ())
            } else {
                execute_program_with_context(&mut context, &program, &mut scope, &mut values).map(|_| ())
            };
            assert_eq!(result, Err(EvalStatus::UncaughtThrowable), "{source}, walker={walker}");
            assert!(values.output.is_empty(), "{source}");
            assert!(context.take_pending_throw().is_some());
        }
    }
}

/// Explicit casts, boolean short circuits, and callback-free data filtering remain usable.
#[test]
fn strict_profile_bools_preserves_explicit_casts_and_boolean_callbacks() {
    let program = parse_fragment(b"if ((bool) 1) { echo 'B'; } echo !false; echo false && 1; echo count(array_filter([0, 1, 2]));").unwrap();
    let mut context = ElephcEvalContext::new();
    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)).unwrap();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    assert_eq!(values.output, "B12");
}

/// Eval callback predicates reject nonboolean results through the ordinary Throwable channel.
#[test]
fn strict_profile_bools_callback_results_are_checked_in_eval() {
    for source in [
        "function strict_eval_predicate($value) { return $value; } array_filter([1], 'strict_eval_predicate');",
        "function strict_eval_predicate() { return 1; } iterator_apply($it, 'strict_eval_predicate');",
    ] {
        let program = parse_fragment(source.as_bytes()).unwrap();
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let iterator = values.alloc(FakeValue::Iterator { len: 1, position: 0 });
        scope.set("it", iterator, crate::scope::ScopeCellOwnership::Borrowed);
        assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values), Err(EvalStatus::UncaughtThrowable));
        assert!(context.take_pending_throw().is_some());
    }
}
