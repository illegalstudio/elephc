//! Purpose:
//! Verifies captured producer contracts across closure lifetimes and native eval re-entry.
//!
//! Called from:
//! - Focused strict captured-local codegen regressions.
//!
//! Key details:
//! - One opaque-eval bundle covers escaping, nested and rebound captures without repeated catalog builds.
//! - Array-kind and resource contracts preserve their existing owner and subtype representations.

use crate::support::*;

/// Captures preserve declared alternatives, mixed openness, fresh bindings and PHP owners.
#[test]
fn test_strict_capture_locals_native_lifetimes_and_owners_bundle() {
    let source = include_str!("../fixtures/strict/eval_capture_bindings.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(source).capture(&flags);
            let expected = if strict {
                concat!("Tinteger|Wdouble|Mdouble|Winteger|TNULL|Tinteger|Ndouble|Tinteger|",
                    "Tinteger|Tinteger|Tstring|Tarray|Warray|Tstring|Wstring|Tresource|Wboolean|",
                    "TCaptureNominalFactory|TCaptureNominalFactory|")
            } else {
                concat!("Wdouble|Wdouble|Mdouble|Winteger|Wdouble|Wdouble|Ndouble|Wdouble|",
                    "Wdouble|Wdouble|Winteger|Winteger|Warray|Wstring|Wstring|Winteger|Wboolean|",
                    "WstdClass|WstdClass|")
            };
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}: {}",
                String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
