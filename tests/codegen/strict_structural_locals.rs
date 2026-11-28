//! Purpose:
//! Verifies callable and iterable local contracts across native stores and aliases.
//!
//! Called from:
//! - Focused strict local codegen regressions.
//!
//! Key details:
//! - Failure preserves the old value, source effects run once, and descriptor owners balance.

use crate::support::*;

/// Both optimizer settings preserve capability membership without enabling scalar coercion checks.
#[test]
fn test_strict_structural_locals_membership_aliases_and_owners() {
    let source = include_str!("../fixtures/strict/structural_local_bindings.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(source).capture(&flags);
            let expected = if strict {
                "T2|T2|W11|OT2|T1|W23|W4|T2|T2|W11|T1|W2|W11|W31|T2|W41|T2|T2|W11|W4|T2|T2|W41|"
            } else {
                "W|W|W11|OW|W|W23|W4|W|W|W11|W|W2|W11|W31|W|W41|W|W|W11|W4|W|W|W41|"
            };
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}: {}",
                String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
