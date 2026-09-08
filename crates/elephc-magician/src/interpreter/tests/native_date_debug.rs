//! Purpose:
//! Verifies debug-property ownership and native visibility decoding.
//!
//! Called from:
//! - Magician's interpreter unit-test harness.
//!
//! Key details:
//! - Snapshot owners are released; borrowed scope properties remain live.

use super::super::*;
use super::support::*;

/// User debug hooks preserve integer keys and visibility-mangled property names in both renderers.
#[test]
fn debug_hook_keys_and_visibility() {
    let program = parse_fragment(br#"class DebugKeys {
        public function __debugInfo(): array {
            return [7 => "seven", "\0*\0guarded" => 2, "\0DebugKeys\0hidden" => 3];
        }
    }
    $value = new DebugKeys(); var_dump($value); print_r($value);"#).unwrap();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).unwrap();
    assert!(values.output.contains("[7]=>"), "{}", values.output);
    assert!(values.output.contains("[\"guarded\":protected]=>"), "{}", values.output);
    assert!(values.output.contains("[\"hidden\":\"DebugKeys\":private]=>"), "{}", values.output);
    assert!(values.output.contains("[hidden:DebugKeys:private] => 3"), "{}", values.output);
}

/// Releasing a property snapshot consumes its acquired cells but not borrowed object properties.
#[test]
fn native_date_debug_property_ownership() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let owned = values.string("temporary").unwrap();
    let borrowed = values.string("borrowed").unwrap();
    let properties = vec![(owned, true), (borrowed, false)].into_iter().map(|(value, owned_value)| {
        EvalDebugObjectProperty {
            name: "field".into(),
            visibility: EvalDebugPropertyVisibility { kind: EvalDebugPropertyVisibilityKind::Public },
            value, owned_value, is_reference: false, numeric_name: false,
        }
    }).collect();
    release_debug_properties(properties, &mut context, &mut values).unwrap();
    assert_eq!(values.releases, vec![owned]);
}

/// Debug visibility uses bridge flags and excludes non-instance storage and virtual getters.
#[test]
fn native_date_debug_property_visibility() {
    use crate::interpreter::reflection::native_property_debug_visibility;
    assert_eq!(native_property_debug_visibility(1), None);
    assert_eq!(native_property_debug_visibility(1024), None);
    assert_eq!(native_property_debug_visibility(2), Some(EvalVisibility::Public));
    assert_eq!(native_property_debug_visibility(4), Some(EvalVisibility::Protected));
    assert_eq!(native_property_debug_visibility(8 | 64), Some(EvalVisibility::Private));
}
