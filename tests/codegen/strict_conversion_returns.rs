//! Purpose:
//! Exercises declared scalar return guards, callable dispatch and exceptional cleanup natively.
//!
//! Called from:
//! - Focused staged codegen regression tests.
//!
//! Key details:
//! - Same-frame catches, finally bodies and rejected temporary results remain observable.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Scalar and union return guards preserve values, catch scopes and owners under both optimizers.
#[test]
fn test_strict_conversion_returns_native_catches_callables_and_owners() {
    let source = include_str!("../fixtures/strict/conversion_returns.php");
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true), optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "FSb:4.25:-INF:owned-|NSBOR2ZTCUAM1fnQ:1:23", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}
