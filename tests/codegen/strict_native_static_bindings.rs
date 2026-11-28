//! Purpose:
//! Verifies native static activation binding and displaced-owner exception ordering.
//!
//! Called from:
//! - Focused static declaration end-to-end regressions.
//!
//! Key details:
//! - Both optimizer modes preserve prior aliases and failed initializer state.

use crate::support::*;

/// Native static declarations select persistent storage only on their reached execution path.
#[test]
fn test_strict_native_static_activation_bindings_and_owners_bundle() {
    let source = include_str!("../fixtures/strict/native_static_bindings.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(source).capture(&flags);
            let dynamic = if strict { "Tinteger:1|Tinteger:1|" } else { "Wdouble:2.5|Wdouble:3.5|" };
            let expected = format!("seed:local|retained:local|9:1:9|old|old|integer:1|integer:1|2|2|{dynamic}\
                double:2.5|string:new|string:local|integer:1|local|1:2|2:2|kept|kept|7|7|2:2|");
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Detaching a static name preserves its aliases and gives a fresh local its own contract.
#[test]
fn test_strict_native_static_detachment_and_reference_contracts_bundle() {
    let source = include_str!("../fixtures/strict/native_static_detachment.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(source).capture(&flags);
            let expected = if strict {
                "integer:1|Wdouble:3.5:1|integer:1|Tdouble:2.5:1|Tinteger:1|Tinteger:1|\
                Tinteger:1|Tinteger:1|Tstring:9|Tstring:9|seed|9|"
            } else {
                "integer:1|Wdouble:3.5:1|integer:1|Wstring:fresh:1|Wdouble:2.5|Wdouble:3.5|\
                Wdouble:2.5|Wdouble:3.5|Winteger:10|Winteger:11|seed|9|"
            };
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
