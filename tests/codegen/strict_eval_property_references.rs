//! Purpose:
//! Verifies native stores honor eval-owned property declarations at the assignment point.
//!
//! Called from:
//! - Focused strict codegen tests using the real Magician bridge and canonical publisher.
//!
//! Key details:
//! - Opaque eval exercises independently supported runtime Coercions without CLI admission.
//! - Callee effects and heap checks distinguish early rejection from deferred writeback.

use crate::support::*;

/// Eval-owned property contracts reject native by-reference writes before later callee effects.
#[test]
fn test_strict_eval_property_native_reference_assignment_point_bundle() {
    let source = r#"<?php
function strictNativePropertyWrite(mixed &$value): void { echo "B"; $value = 7; echo "A"; }
$source = $argc > 0 ? '
class StrictEvalPropertyRef { public float $value = 1.5; }
$alias = 1.5; $object = new StrictEvalPropertyRef(); $object->value =& $alias;
try { strictNativePropertyWrite($alias); } catch (TypeError $error) { echo "T"; }
echo gettype($alias), ":", $alias, ":", $object->value;
' : '';
eval($source);
"#;
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Coercions, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "BTdouble:1.5:1.5", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Shared live declarations, native catches and nested eval use the same pre-publication guard.
#[test]
fn test_strict_eval_property_native_reference_lifecycle_bundle() {
    let source = include_str!("../fixtures/strict/eval_property_native_references.php");
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Coercions, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "BT1.5:BCE1.5:T1.5:BA2.5:2.5:BAinteger:7|BT1.5:1.5|BAinteger:7", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}
