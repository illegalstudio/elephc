//! Purpose:
//! Verifies Reflection conversion retirement preserves PHP static-binding identity.
//!
//! Called from:
//! - Focused Casts admission audits through native lowering and the real eval bridge.
//!
//! Key details:
//! - PHP distinguishes an existing reference from one introduced by an argument destructor.
//! - Successful conversion still publishes when temporary retirement throws.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Keeps native declarations and original references while moving only invocation into Magician.
fn opaque_invocation(source: &str) -> String {
    let (setup, body) = source.split_once("// invoke cases").unwrap();
    let mut body = body.to_string();
    for index in 0..4 {
        body = body.replace(&format!("setStaticPropertyValue(\"text\", new ReflectionBindingText{index}())"),
            &format!("setStaticPropertyValue(value: new ReflectionBindingText{index}(), name: \"text\")"));
    }
    let body = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("{setup}\n$fragment = $argc > 0 ? '{body}' : ''; eval($fragment);")
}

/// Native and opaque Reflection preserve captured targets through rebinding and throwing destructors.
#[test]
fn test_strict_cast_reflection_binding_identity_and_retirement_bundle() {
    let original = include_str!("../fixtures/strict/reflection_binding_identity.php");
    let expected = concat!(
        "SDold:second|old:second:new\n",
        "SDold:second|new:second:second\n",
        "SDold:secondT|old:second:new\n",
        "SDold:secondT|new:second:second\n");
    for opaque in [false, true] {
        let source = if opaque { opaque_invocation(original) } else { original.to_string() };
        for optimized in [false, true] {
            let fixture = StrictFixture::new(&source);
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
            let output = fixture.capture_staged_eval_runtime(profile, optimized);
            assert!(output.success, "opaque={opaque} opt={optimized}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, expected, "opaque={opaque} opt={optimized}: {}", output.stderr);
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"),
                "opaque={opaque} opt={optimized}: {}", output.stderr);
        }
    }
}

/// A physical static rebind detaches the old native alias and preserves both live source groups.
fn verify_checked_native_static_reference_rebinding(feature: StrictFeature, optimized: bool) {
    let source = r#"<?php
class StaticAliasOwner { public static string $text = 'old'; }
$first =& StaticAliasOwner::$text; $second = 'second';
$fragment = $argc > 0 ? 'StaticAliasOwner::$text =& $second;' : '';
eval($fragment);
echo $first, ':', $second, ':', StaticAliasOwner::$text, '|';
$first = 'first'; echo $first, ':', $second, ':', StaticAliasOwner::$text, '|';
StaticAliasOwner::$text = 'third'; echo $first, ':', $second, ':', StaticAliasOwner::$text;
gc_collect_cycles();
"#;
    let output = StrictFixture::new(source).capture_staged_eval_runtime(
        SemanticProfile::PERMISSIVE.with(feature, true), optimized);
    assert!(output.success, "{feature:?} opt={optimized}: {} {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "old:second:second|first:second:second|first:third:third",
        "{feature:?} opt={optimized}: {}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Checks standalone casts with the optimizer disabled.
#[test]
fn test_static_reference_rebinding_strict_casts_unoptimized() {
    verify_checked_native_static_reference_rebinding(StrictFeature::Casts, false);
}

/// Checks standalone casts with the optimizer enabled.
#[test]
fn test_static_reference_rebinding_strict_casts_optimized() {
    verify_checked_native_static_reference_rebinding(StrictFeature::Casts, true);
}

/// Checks standalone coercions with the optimizer disabled.
#[test]
fn test_static_reference_rebinding_strict_coercions_unoptimized() {
    verify_checked_native_static_reference_rebinding(StrictFeature::Coercions, false);
}

/// Checks standalone coercions with the optimizer enabled.
#[test]
fn test_static_reference_rebinding_strict_coercions_optimized() {
    verify_checked_native_static_reference_rebinding(StrictFeature::Coercions, true);
}
