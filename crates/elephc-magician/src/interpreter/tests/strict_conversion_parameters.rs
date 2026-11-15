//! Purpose:
//! Checks noncoercive scalar parameter binding in both eval engines.
//!
//! Called from:
//! - Focused interpreter tests with explicit owner accounting.
//!
//! Key details:
//! - Binding failures preserve caller results and transfer a catchable TypeError owner.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Exact declarations accept their own kinds and reject scalar conversion without leaking arguments.
#[test]
fn strict_conversion_parameters_eval_engines_exact_members_and_owners() {
    for vm in [false, true] {
        for (declaration, argument, accepted) in [
            ("float", "2.0", true), ("float", "2", false), ("int", "2", true),
            ("int", "'2'", false), ("string", "'text'", true), ("string", "2", false),
            ("bool", "false", true), ("bool", "1", false), ("int|string", "'2'", true),
            ("int|string", "2.0", false), ("?int", "null", true),
            ("stdClass|float", "2.0", true), ("stdClass|float", "2", false),
            ("stdClass|float", "$nominal", true), ("?stdClass", "null", true),
            ("array|float", "$container", true), ("array|float", "2", false),
            ("iterable|float", "$container", true), ("iterable|float", "2", false),
            ("callable|float", "'exactCallback'", true), ("callable|float", "2", false),
            ("callable|float", "'missing_callback'", false),
            ("callable", "'exactCallback'", true), ("callable", "'missing_callback'", false),
        ] {
            let declaration = format!("function exactParameter({declaration} $v) {{ return $v; }}");
            let source = format!("{} $result = 23; $result = exactParameter({argument});",
                if vm { "if (false) { function exactParameter($v) {} }" } else { "" });
            // Container and object construction stay in the walker; both callers consume the same owners.
            let declaration = format!("{declaration} function exactCallback() {{}} $nominal = new stdClass(); $container = [1];");
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            staged_numeric_execution(&declaration, false, &mut context, &mut scope, &mut values).unwrap();
            let outcome = staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values);
            if accepted { assert!(outcome.is_ok(), "{source}, vm={vm}"); }
            else {
                assert!(matches!(outcome, Err(EvalStatus::UncaughtThrowable)), "{source}, vm={vm}");
                assert_eq!(values.get(scope.visible_cell("result").unwrap()), FakeValue::Int(23));
                let error = context.take_pending_throw().expect("binding error owner");
                assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
            }
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}, vm={vm}: {:?}", values.values[id]); }
        }
    }
}


/// Builtin scalar declarations reject implicit conversion through every supported call route.
#[test]
fn strict_conversion_parameters_builtin_scalar_contracts() {
    for source in ["echo strlen(7);", "echo strlen(string: 7);",
        "echo call_user_func('strlen', 7);", "echo strlen(...[7]);", "echo STRLEN(7);", "echo strlen(null);",
        "echo substr('abc', '1');", "echo str_repeat('a', 2.0);", "echo trim('text', null);"] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        let result = staged_numeric_execution(source, false, &mut context, &mut scope, &mut values);
        assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}: {:?} output={}", result.as_ref().err(), values.output);
        let error = context.take_pending_throw().expect("builtin conversion TypeError owner");
        assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
    }
}

/// Exact builtin values, mixed input APIs and explicit conversions preserve their contracts.
#[test]
fn strict_conversion_parameters_builtin_accepts_exact_and_explicit_inputs() {
    for (source, expected) in [
        ("echo strlen('12');", "2"), ("echo strlen(strval(7));", "1"),
        ("echo intval('7');", "7"), ("echo implode('', ['a', 'b']);", "ab"),
        ("echo substr('abc', 1);", "bc"), ("echo str_repeat('a', 2);", "aa"),
    ] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        staged_numeric_execution(source, false, &mut context, &mut scope, &mut values)
            .unwrap_or_else(|status| panic!("{source}: {status:?}"));
        assert_eq!(values.output, expected, "{source}");
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
    }
}

/// VM callers and Stringable values meet the guard before a hook or later caller effect runs.
#[test]
fn strict_conversion_parameters_builtin_vm_caller_and_stringable_owners() {
    for (setup, source, vm) in [
        ("function strictBuiltinInput($value) { echo strlen($value); echo 'X'; }",
            "if (false) { function strictBuiltinInput($value) {} } strictBuiltinInput(7);", true),
        ("class StrictBuiltinStringable { function __toString(): string { echo 'S'; return 'text'; } } \
            $value = new StrictBuiltinStringable();", "echo strlen($value);", false),
    ] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        staged_numeric_execution(setup, false, &mut context, &mut scope, &mut values).unwrap();
        let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
        assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}: {:?}", result.as_ref().err());
        assert!(values.output.is_empty(), "{source}: {}", values.output);
        let error = context.take_pending_throw().expect("builtin conversion TypeError owner");
        assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
    }
}
