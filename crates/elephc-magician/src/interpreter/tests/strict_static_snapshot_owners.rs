//! Purpose:
//! Accounts for owned Reflection static/capture snapshots before static binding changes.
//!
//! Called from:
//! - Focused interpreter metadata snapshot ownership regressions.
//!
//! Key details:
//! - The retaining result arrays own entries after temporary key and value leases retire.

use super::super::*;
use super::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Repeated static and captured-variable maps preserve source values without retaining temporaries.
#[test]
fn strict_static_snapshot_eval_temporary_keys_values_and_owners() {
    for strict in [false, true] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        assert!(context.set_global_scope(&mut scope));
        let setup = [
            "function owned_static_value(): int {static $value=7;return $value;} \
             function make_owned_capture(int $value): callable {return static function() use($value): int {return $value;};}",
            "$callback=make_owned_capture(11);",
            "$staticReflection=new ReflectionFunction(\"owned_static_value\");",
            "$captureReflection=new ReflectionFunction($callback);",
        ];
        let reads = ["$staticSnapshot=$staticReflection->getStaticVariables();",
            "$captureSnapshot=$captureReflection->getClosureUsedVariables();",
            "echo $staticSnapshot[\"value\"],\":\",$captureSnapshot[\"value\"],\"|\";"];
        for (stage, source) in setup.into_iter().chain(reads.into_iter().cycle().take(9)).enumerate() {
            let program = crate::parser::parse_fragment_with_semantics(source.as_bytes(), context.semantic_profile()).unwrap();
            let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
                .unwrap_or_else(|status| panic!("stage={stage}, source={source}, strict={strict}, output={}: {status:?}", values.output));
            values.release(result).unwrap();
        }
        assert_eq!(values.output, "7:11|7:11|7:11|", "strict={strict}");
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners {
            assert_eq!(*owners, 0, "strict={strict}: {:?}", values.values[id]);
        }
    }
}

/// A reference wrapper must not change the object key used by synthetic Reflection metadata.
#[test]
fn strict_static_snapshot_fake_reference_preserves_object_identity() {
    let mut values = FakeOps::default();
    let object = values.new_object("stdClass").unwrap();
    let identity = values.object_identity(object).unwrap();
    let reference = values.reference_new(object).unwrap();
    assert_eq!(values.object_identity(reference).unwrap(), identity);
    assert_eq!(values.php_object_handle(reference).unwrap(), identity);
    values.release(object).unwrap();values.release(reference).unwrap();
    assert!(values.cell_owners.values().all(|owners| *owners == 0));
}
