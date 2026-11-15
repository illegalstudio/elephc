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
