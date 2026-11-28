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
        "echo substr('abc', '1');", "echo str_repeat('a', 2.0);", "echo trim('text', null);",
        "echo nl2br('text', 1);"] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        let result = staged_numeric_execution(source, false, &mut context, &mut scope, &mut values);
        assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}: {:?} output={}", result.as_ref().err(), values.output);
        let error = context.take_pending_throw().expect("builtin conversion TypeError owner");
        assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
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
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
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
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
    }
}

/// Direct source-sensitive builtins reject input conversion before touching reference outputs.
#[test]
fn strict_conversion_parameters_builtin_source_reference_contracts() {
    for (setup, source, binding) in [
        ("$array = ['b' => 2, 'a' => 1];", "ksort($array, '1'); echo 'X';", "array"),
        ("$array = ['b' => 2, 'a' => 1]; $sort = 'ksort';", "$sort(array: $array, flags: '1'); echo 'X';", "array"),
        ("$value = 1.5;", "settype($value, 7); echo 'X';", "value"),
        ("$name = 'kept';", "is_callable('strlen', 1, $name); echo 'X';", "name"),
        ("$array = [1, 2, 3];", "array_splice($array, 1.0); echo 'X';", "array"),
        ("$matches = ['kept'];", "preg_match(7, 'text', $matches); echo 'X';", "matches"),
        ("$tag = 'kept';", "openssl_encrypt(7, 'aes-128-gcm', 'pass', 0, 'iv', $tag); echo 'X';", "tag"),
        ("$errno = 0; $message = 'kept';", "fsockopen(7, 80, $errno, $message); echo 'X';", "message"),
        ("$blocked = false;", "flock(null, '2', $blocked); echo 'X';", "blocked"),
    ] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        staged_numeric_execution(setup, false, &mut context, &mut scope, &mut values).unwrap();
        let before = values.get(scope.visible_cell(binding).unwrap());
        let result = staged_numeric_execution(source, false, &mut context, &mut scope, &mut values);
        assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}: {:?}", result.as_ref().err());
        assert!(values.output.is_empty(), "{source}: {}", values.output);
        assert_eq!(values.get(scope.visible_cell(binding).unwrap()), before, "{source}");
        let error = context.take_pending_throw().expect("source builtin TypeError owner");
        assert!(values.object_is_a(error, "TypeError", false).unwrap(), "{source}"); values.release(error).unwrap();
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
    }
}

/// Valid source reference calls preserve explicit casts, output targets and array mutations.
#[test]
fn strict_conversion_parameters_builtin_source_reference_exact_inputs() {
    for (source, expected) in [
        ("$array = ['b' => 2, 'a' => 1]; ksort($array, SORT_STRING); echo implode('', array_keys($array));", "ab"),
        ("$array = ['b' => 2, 'a' => 1]; $sort = 'ksort'; $sort($array, SORT_STRING); echo implode('', array_keys($array));", "ab"),
        ("$value = 1.5; settype($value, 'int'); echo gettype($value), ':', $value;", "integer:1"),
        ("$name = 'kept'; is_callable('strlen', false, $name); echo $name;", "strlen"),
        ("$array = [1, 2, 3]; array_splice($array, 1, 1, [9]); echo implode('', $array);", "193"),
        ("$array = ['a' => 1, 8 => 2, 'b' => 3]; $removed = array_splice($array, 1, 1, ['ignored' => 9]); \
            echo implode('', array_keys($array)), ':', implode('', $array), ':', implode('', $removed);", "a0b:193:2"),
    ] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        staged_numeric_execution(source, false, &mut context, &mut scope, &mut values)
            .unwrap_or_else(|status| panic!("{source}: {status:?}"));
        assert_eq!(values.output, expected, "{source}");
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
    }
}

/// A declaration that rejects a freshly produced output retains neither the candidate nor its receiver.
#[test]
fn strict_conversion_parameters_builtin_rejected_output_owners() {
    let mut context = ElephcEvalContext::new();
    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
    let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
    let source = "class StrictBuiltinOutput { public bool $name = false; } \
        $holder = new StrictBuiltinOutput(); is_callable('strlen', false, $holder->name); echo 'X';";
    assert!(matches!(staged_numeric_execution(source, false, &mut context, &mut scope, &mut values),
        Err(EvalStatus::UncaughtThrowable)));
    assert!(values.output.is_empty());
    let error = context.take_pending_throw().expect("output property TypeError");
    assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
    for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
    for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "rejected output: {:?}", values.values[id]); }
}

/// Failed entry conversion retires every lease collected before sort rejects its input.
#[test]
fn strict_conversion_parameters_builtin_sort_failure_owners() {
    for name in ["sort", "natsort"] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        let source = format!("$array = [1, 'text']; {name}($array);");
        assert!(matches!(staged_numeric_execution(&source, false, &mut context, &mut scope, &mut values),
            Err(EvalStatus::RuntimeFatal)));
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{name}: {:?}", values.values[id]); }
    }
}

/// Source input effects run once before type rejection and reference consumers read the live place.
#[test]
fn strict_conversion_parameters_builtin_source_effect_order_and_live_values() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
    let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
    assert!(context.set_global_scope(&mut scope));
    staged_numeric_execution("function strictSourceFlags() { echo 'F'; return '1'; } \
        $array = ['b' => 2, 'a' => 1];", false, &mut context, &mut scope, &mut values).unwrap();
    let before = values.get(scope.visible_cell("array").unwrap());
    let result = staged_numeric_execution("ksort(flags: strictSourceFlags(), array: $array); echo 'X';", false,
        &mut context, &mut scope, &mut values);
    assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)));
    assert_eq!(values.output, "F"); assert_eq!(values.get(scope.visible_cell("array").unwrap()), before);
    let error = context.take_pending_throw().expect("named source argument error"); values.release(error).unwrap();
    values.output.clear();
    staged_numeric_execution("class StrictSourceLiveInput { public mixed $value = 1.5; \
        function typeName(): string { $this->value = '7'; echo 'F'; return 'int'; } } \
        $holder = new StrictSourceLiveInput(); settype($holder->value, $holder->typeName()); \
        echo $holder->value;", false, &mut context, &mut scope, &mut values).unwrap();
    assert_eq!(values.output, "F7");
    values.output.clear();
    staged_numeric_execution("class StrictSourceDetachedInput { public mixed $value = 1.5; \
        function typeName(): string { unset($this->value); $this->value = '9'; echo 'F'; return 'int'; } } \
        $detached = new StrictSourceDetachedInput(); settype($detached->value, $detached->typeName()); \
        echo gettype($detached->value), ':', $detached->value;", false, &mut context, &mut scope, &mut values).unwrap();
    assert_eq!(values.output, "Fstring:9");
    values.output.clear();
    staged_numeric_execution("class StrictSourceStaticInput { public static mixed $value = 1.5; \
        static function typeName(): string { self::$value = '7'; echo 'F'; return 'int'; } } \
        settype(StrictSourceStaticInput::$value, StrictSourceStaticInput::typeName()); \
        echo StrictSourceStaticInput::$value;", false, &mut context, &mut scope, &mut values).unwrap();
    assert_eq!(values.output, "F7");
    values.output.clear();
    staged_numeric_execution("$live = 1.5; function strictSourceLocalName() { global $live; $live = '7'; echo 'F'; return 'int'; } \
        settype($live, strictSourceLocalName()); echo $live;", false, &mut context, &mut scope, &mut values).unwrap();
    assert_eq!(values.output, "F7");
    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
    for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
    for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "live source input: {:?}", values.values[id]); }
}
