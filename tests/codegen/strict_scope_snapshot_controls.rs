//! Purpose:
//! Verifies activation snapshot retirement preserves dynamic bindings and escaped owners.
//!
//! Called from:
//! - Focused native eval ownership regression controls.
//!
//! Key details:
//! - Later eval entries retain interpreter-only locals while native unsets drop copied leases.
//! - Explicit destructor ordering distinguishes a valid retained owner from stale scope copies.

use crate::support::*;

/// Interpreter-created object bindings survive retirement and release at their own unset.
#[test]
fn test_eval_activation_snapshot_retirement_preserves_dynamic_owners_bundle() {
    let source = r#"<?php
class EvalSnapshotOwner {
    public function __toString(): string { return 'value'; }
    public function __destruct() { echo 'D'; }
}
function evalSnapshotActivation(int $selector, bool $reference): void {
    $object = new EvalSnapshotOwner();
    $source = $selector > 0 ? '$dynamic = $object; $only = "scope";' : '';
    if ($reference) { $source = $selector > 0 ? '$dynamic =& $object; $only = "scope";' : ''; }
    eval($source);
    unset($object); echo 'A|';
    $read = $selector > 0 ? 'echo $only, ":", $dynamic, "|"; unset($dynamic, $only);' : '';
    eval($read);
    echo 'B';
}
evalSnapshotActivation($argc, false); echo '|'; evalSnapshotActivation($argc, true);
gc_collect_cycles();
"#;
    for optimized in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(
            elephc_semantics::SemanticProfile::PERMISSIVE, optimized);
        assert!(output.success, "opt={optimized}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "A|scope:value|DB|A|scope:value|DB", "opt={optimized}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    }
}
