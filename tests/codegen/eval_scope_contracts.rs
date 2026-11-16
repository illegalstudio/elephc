//! Purpose:
//! Verifies that native eval snapshots carry contracts only for existing object bindings.
//!
//! Called from:
//! - Focused eval scope and strict local binding regression checks.
//!
//! Key details:
//! - Runtime-selected source preserves the original permissive catch-name collision.
//! - Both optimizer modes must retain catch behavior and balanced scope owners.

use crate::support::*;

/// A future native catch type cannot constrain an earlier opaque eval catch using that name.
#[test]
fn test_eval_native_unbound_catch_contract_scope_bundle() {
    let source = include_str!("../fixtures/strict/catch_scope_contract.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&[mode, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "TVN", "{mode}: {}",
            String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
