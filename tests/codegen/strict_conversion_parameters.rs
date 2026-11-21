//! Purpose:
//! Exercises exact scalar parameter boundaries and caller evaluation order natively.
//!
//! Called from:
//! - Focused staged codegen integration tests.
//!
//! Key details:
//! - Both optimizer settings must preserve catchable failures and balanced owners.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Dynamic builtin inputs reach exact guards before ABI coercion or Stringable hooks.
#[test]
fn test_strict_conversion_builtin_original_dynamic_inputs_bundle() {
    let source = include_str!("../fixtures/strict/builtin_value_inputs.php");
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true), optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "TTTTTNPCEOEMJ|1A|abcbc", "opt={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Calls guard original scalar values after evaluating later arguments and before entering the callee.
#[test]
fn test_strict_conversion_parameters_native_order_kinds_and_owners() {
    let source = include_str!("../fixtures/strict/conversion_parameters.php");
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true), optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "F:4.25ISbETNDSBEMLVCUPifnQ:23", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}
