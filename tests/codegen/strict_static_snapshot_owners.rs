//! Purpose:
//! Checks retaining Reflection static/capture snapshots before stronger static origin integration.
//!
//! Called from:
//! - Focused strict static snapshot ownership regressions.
//!
//! Key details:
//! - Repeated snapshots retain the source values while retiring temporary keys and value leases.

use crate::support::*;

/// Static and closure snapshots preserve their values and retire every temporary owner.
#[test]
fn test_strict_static_snapshot_temporary_owners_bundle() {
    let source = include_str!("../fixtures/strict/reflection_static_snapshot_owners.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(source).capture(&flags);
            assert_eq!(String::from_utf8_lossy(&output.stdout), "7:11|7:11|7:11|", "{flags:?}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
