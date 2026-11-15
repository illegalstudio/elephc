//! Purpose:
//! Verifies exact declaration membership for eval-owned property writes and references.
//!
//! Called from:
//! - Staged Coercions regressions for the walker and bytecode consumers.
//!
//! Key details:
//! - VM callers use the ordinary walker fallback for unsupported PropertySet bodies.
//! - Rejected candidates leave the original property and initialization state intact.
//! - Static, instance and pending Throwable owners are retired explicitly.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Instance, static and Reflection writes share exact scalar, union and structural membership.
#[test]
fn strict_conversion_properties_eval_engines_exact_values_and_owners() {
    for (declaration, initial, replacement, accepted) in [
        ("float", "1.5", "7", false), ("float", "1.5", "7.0", true),
        ("int", "1", "'2'", false), ("int", "1", "2", true),
        ("bool", "false", "1", false), ("bool", "false", "true", true),
        ("string", "'kept'", "7", false), ("string", "'kept'", "'next'", true),
        ("int|string", "'7'", "7", true), ("int|string", "'7'", "2.5", false),
        ("?int", "null", "3", true), ("?int", "null", "3.5", false),
        ("array|float", "[]", "2", false), ("array|float", "[]", "[2]", true),
        ("array|float", "[]", "3.5", true),
        ("iterable", "[]", "1", false), ("iterable", "[]", "[2]", true),
        ("false", "false", "true", false), ("false", "false", "false", true),
    ] { verify_property_case(declaration, initial, replacement, accepted); }
}

/// Lexical self/parent contracts and nominal members do not depend on the writing caller's scope.
#[test]
fn strict_conversion_properties_eval_lexical_nominal_contracts_and_owners() {
    for (declaration, replacement, accepted) in [
        ("?self", "$object", true), ("?self", "new ExactEvalBase()", false),
        ("?parent", "new ExactEvalBase()", true), ("?parent", "new stdClass()", false),
        ("?ExactEvalBase", "$object", true), ("?ExactEvalBase", "new stdClass()", false),
        ("?object", "new stdClass()", true), ("?object", "7", false),
    ] { verify_property_case(declaration, "null", replacement, accepted); }
}

/// The fake Reflection factory transfers metadata fields under the ordinary permissive profile too.
#[test]
fn strict_conversion_properties_permissive_reflection_owner_control() {
    let source = r#"class PropertyOwnerControl { public int $value = 1; }
$class = new ReflectionClass("PropertyOwnerControl");
$property = new ReflectionProperty("PropertyOwnerControl", "value");
$object = new PropertyOwnerControl(); $property->setValue($object, 2);
return $object->value;
"#;
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
    let Ok(EvalControl::Return(result)) = staged_numeric_execution(source, false,
        &mut context, &mut scope, &mut values) else { panic!("permissive Reflection control"); };
    assert_eq!(values.get(result), FakeValue::Int(2)); values.release(result).unwrap();
    clean_property_fixture(&mut context, &mut scope, &mut values, "permissive Reflection control");
}

/// Held aliases and by-reference callees refuse a kind change before later callee effects.
#[test]
fn strict_conversion_properties_reference_assignment_points_and_owners() {
    for vm in [false, true] {
        for is_static in [false, true] {
            for parameter in [false, true] {
                let place = if is_static { "ReferenceEvalProperty::$shared" } else { "$object->value" };
                let setup = format!("class ReferenceEvalProperty {{ public float $value = 1.5; \
                    public static float $shared = 1.5; }} $object = new ReferenceEvalProperty(); \
                    $alias = 1.5; {place} =& $alias; function assignExactPropertyRef(mixed &$value) {{ \
                    echo 'B'; $value = 7; echo 'A'; }}");
                let setup = if vm && parameter {
                    format!("{setup} function exactPropertyRefProxy($object) {{ assignExactPropertyRef({place}); }}")
                } else { setup };
                let source = if parameter {
                    if vm { "if (false) { function exactPropertyRefProxy($object) {} } exactPropertyRefProxy($object);" }
                    else { "assignExactPropertyRef($alias);" }
                } else { "$alias = 7;" };
                let mut context = ElephcEvalContext::new();
                context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
                let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                staged_numeric_execution(&setup, false, &mut context, &mut scope, &mut values).unwrap();
                let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
                assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)),
                    "vm={vm} static={is_static} parameter={parameter}: {:?}", result.as_ref().err());
                assert_eq!(values.output, if parameter { "B" } else { "" },
                    "vm={vm} static={is_static} parameter={parameter}");
                let error = context.take_pending_throw().expect("reference TypeError owner");
                assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
                for probe in [format!("return {place};"), "return $alias;".to_string()] {
                    let Ok(EvalControl::Return(current)) = staged_numeric_execution(&probe, false,
                        &mut context, &mut scope, &mut values) else { panic!("reference value probe"); };
                    assert_eq!(values.get(current), FakeValue::Float(1.5)); values.release(current).unwrap();
                }
                clean_property_fixture(&mut context, &mut scope, &mut values, "held property reference");
            }
        }
    }
}

/// Captured references retain property constraints after the temporary parameter scope ends.
#[test]
fn strict_conversion_properties_reference_capture_escape_and_owners() {
    for is_static in [false, true] {
        let place = if is_static { "EscapedEvalProperty::$shared" } else { "$object->value" };
        let setup = format!("class EscapedEvalProperty {{ public float $value = 1.5; \
            public static float $shared = 1.5; }} $object = new EscapedEvalProperty(); \
            $alias = 1.5; {place} =& $alias; \
            function escapeExactPropertyRef(mixed &$value): mixed {{ \
                return function (mixed $next) use (&$value) {{ $value = $next; }}; }} \
            $escaped = escapeExactPropertyRef($alias);");
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        if let Err(status) = staged_numeric_execution(&setup, false, &mut context, &mut scope, &mut values) {
            let message = context.take_pending_throw().and_then(|error| match values.get(error) {
                FakeValue::Object(fields) => fields.into_iter().find(|(name, _)| name == "message")
                    .map(|(_, value)| values.get(value)),
                _ => None,
            });
            panic!("escaped setup static={is_static}: {status:?} {message:?}");
        }
        let result = staged_numeric_execution("$escaped(7);", false, &mut context, &mut scope, &mut values);
        assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "static={is_static}: {:?}", result.as_ref().err());
        let error = context.take_pending_throw().expect("escaped reference TypeError owner");
        assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
        for probe in [format!("return {place};"), "return $alias;".to_string()] {
            let Ok(EvalControl::Return(current)) = staged_numeric_execution(&probe, false,
                &mut context, &mut scope, &mut values) else { panic!("escaped reference value probe"); };
            assert_eq!(values.get(current), FakeValue::Float(1.5)); values.release(current).unwrap();
        }
        clean_property_fixture(&mut context, &mut scope, &mut values, "escaped property reference");
    }
}

/// Unset, rebind and final-object metadata retirement release only the departed property source.
#[test]
fn strict_conversion_properties_reference_sources_retire_with_bindings() {
    for removal in ["rebind", "unset", "retire"] {
        let setup = "class RetiredEvalProperty { public float $value = 1.5; } \
            $object = new RetiredEvalProperty(); $alias = 1.5; $object->value =& $alias;";
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        staged_numeric_execution(setup, false, &mut context, &mut scope, &mut values).unwrap();
        let source = scope.visible_cell("alias").unwrap();
        assert_eq!(context.property_reference_types(source).len(), 1, "{removal}");
        match removal {
            "rebind" => {
                staged_numeric_execution("$other = 2.5; $object->value =& $other;", false,
                    &mut context, &mut scope, &mut values).unwrap();
                assert_eq!(context.property_reference_types(scope.visible_cell("other").unwrap()).len(), 1);
            }
            "unset" => { staged_numeric_execution("unset($object->value);", false,
                &mut context, &mut scope, &mut values).unwrap(); }
            _ => {
                let identity = values.object_identity(scope.visible_cell("object").unwrap()).unwrap();
                staged_numeric_execution("unset($object);", false, &mut context, &mut scope, &mut values).unwrap();
                // Fake release has no native finalizer; invoke the same metadata retirement callback explicitly.
                context.forget_dynamic_object(identity);
            }
        }
        assert!(context.property_reference_types(source).is_empty(), "{removal}");
        staged_numeric_execution("$alias = 'released';", false, &mut context, &mut scope, &mut values).unwrap();
        assert_eq!(values.get(scope.visible_cell("alias").unwrap()), FakeValue::String("released".into()));
        clean_property_fixture(&mut context, &mut scope, &mut values, removal);
    }
}

/// Hidden parent-private slots keep their original contracts when a child shadows the name.
#[test]
fn strict_conversion_properties_reference_private_shadow_contracts() {
    let setup = "class PrivateReferenceBase { private float $value = 1.5; \
        public function bind(mixed &$source) { $this->value =& $source; } \
        public function read(): mixed { return $this->value; } } \
        class PrivateReferenceChild extends PrivateReferenceBase { public string $value = 'child'; } \
        $object = new PrivateReferenceChild(); $alias = 1.5; $object->bind($alias);";
    let mut context = ElephcEvalContext::new();
    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
    let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
    staged_numeric_execution(setup, false, &mut context, &mut scope, &mut values).unwrap();
    staged_numeric_execution("$alias = 2.5;", false, &mut context, &mut scope, &mut values).unwrap();
    let result = staged_numeric_execution("$alias = 'bad';", false, &mut context, &mut scope, &mut values);
    assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)));
    let error = context.take_pending_throw().unwrap(); values.release(error).unwrap();
    staged_numeric_execution("echo $object->read(), ':', $object->value;", false,
        &mut context, &mut scope, &mut values).unwrap();
    assert_eq!(values.output, "2.5:child");
    clean_property_fixture(&mut context, &mut scope, &mut values, "private declaration shadow");
}

/// Every live source constrains a shared cell, while removing one source preserves the others.
#[test]
fn strict_conversion_properties_reference_intersection_of_live_sources() {
    let setup = "class SharedReferenceSources { public float $number = 1.5; \
        public int|float $numeric = 1; public mixed $open = null; } \
        $object = new SharedReferenceSources(); $alias = 1.5; \
        $object->number =& $alias; $object->numeric =& $alias; $object->open =& $alias;";
    let mut context = ElephcEvalContext::new();
    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
    let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
    staged_numeric_execution(setup, false, &mut context, &mut scope, &mut values).unwrap();
    let result = staged_numeric_execution("$alias = 7;", false, &mut context, &mut scope, &mut values);
    assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)));
    let error = context.take_pending_throw().unwrap(); values.release(error).unwrap();
    staged_numeric_execution("$alias = 2.5; unset($object->number); $alias = 7; \
        echo gettype($object->numeric), ':', $object->numeric, ':', $object->open;", false,
        &mut context, &mut scope, &mut values).unwrap();
    assert_eq!(values.output, "integer:7:7");
    clean_property_fixture(&mut context, &mut scope, &mut values, "shared property sources");
}

/// Executes an original candidate through each ordinary and reflected property write surface.
fn verify_property_case(declaration: &str, initial: &str, replacement: &str, accepted: bool) {
    for vm in [false, true] {
        for (is_static, reflection) in [(false, false), (true, false), (false, true), (true, true)] {
            let setup = format!("class ExactEvalBase {{}} class ExactEvalProperty extends ExactEvalBase {{ \
                public {declaration} $value = {initial}; public static {declaration} $shared = {initial}; \
                }} $object = new ExactEvalProperty(); $next = {replacement};");
            let place = if is_static { "ExactEvalProperty::$shared" } else { "$object->value" };
            let write = match (reflection, is_static) {
                (true, true) => r#"$reflection = new ReflectionClass("ExactEvalProperty");
                    $reflection->setStaticPropertyValue("shared", $next);"#.to_string(),
                (true, false) => r#"$reflection = new ReflectionProperty("ExactEvalProperty", "value");
                    $reflection->setValue($object, $next);"#.to_string(),
                _ => format!("{place} = $next;"),
            };
            let setup = if vm { format!("{setup} function exactPropertyWrite($object, $next) {{ {write} }}") }
                else { setup };
            let source = if vm {
                "if (false) { function exactPropertyWrite($object, $next) {} } exactPropertyWrite($object, $next);".to_string()
            } else { write };
            let probe = format!("return {place};");
            let label = format!("{declaration} <- {replacement}, static={is_static}, reflection={reflection}, vm={vm}");
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            staged_numeric_execution(&setup, false, &mut context, &mut scope, &mut values).unwrap();
            let Ok(EvalControl::Return(before)) = staged_numeric_execution(&probe, false,
                &mut context, &mut scope, &mut values) else { panic!("initial property: {label}"); };
            let original = values.get(before); values.release(before).unwrap();
            let result = staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values);
            if accepted { assert!(result.is_ok(), "{label}: {:?}", result.as_ref().err()); }
            else {
                assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{label}: {:?}", result.as_ref().err());
                let error = context.take_pending_throw().expect("property TypeError owner");
                assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
            }
            let Ok(EvalControl::Return(current)) = staged_numeric_execution(&probe, false,
                &mut context, &mut scope, &mut values) else { panic!("stored property: {label}"); };
            if accepted { assert_eq!(values.get(current), values.get(scope.visible_cell("next").unwrap()), "{label}"); }
            else { assert_eq!(values.get(current), original, "{label}"); }
            values.release(current).unwrap();
            clean_property_fixture(&mut context, &mut scope, &mut values, &label);
        }
    }
}

/// Balances both activation owners and persistent statics before checking the fake native heap.
fn clean_property_fixture(context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut FakeOps, label: &str,
) {
    for cell in scope.drain_owned_cells() { values.release(cell).unwrap(); }
    for cell in context.take_persistent_cell_owners() { values.release(cell).unwrap(); }
    for (id, owners) in &values.cell_owners {
        assert_eq!(*owners, 0, "{label}: {:?}", values.values[id]);
    }
}
