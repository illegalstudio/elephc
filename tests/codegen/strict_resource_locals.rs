//! Purpose:
//! Verifies resource binding origins across native, interpreted and reference stores.
//!
//! Called from:
//! - Focused resource local contract end-to-end tests.
//!
//! Key details:
//! - One opaque-eval bundle avoids repeatedly lowering the builtin catalog.
//! - Both optimizers preserve rejected values, false alternatives, subtypes and owners.

use crate::support::*;

/// Resource producers, aliases, independent copies and eval use the same value-kind contract.
#[test]
fn test_strict_resource_locals_native_eval_and_representation_bundle() {
    let source = include_str!("../fixtures/strict/resource_local_bindings.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(source).capture(&flags);
            let expected = if strict {
                concat!("Tresource|Wboolean|Tresource|Tresource|Wboolean|Tresource|T|Wboolean|Tresource|Tresource|",
                    "Tresource|Wboolean|Tresource|",
                    "Wresource:stream filter|Wresource:stream filter|Tresource:stream-context|Tresource:stream-context|",
                    "Tresource:resource|Wboolean:resource|Nstring:resource|")
            } else {
                concat!("Winteger|Wboolean|Wboolean|Winteger|Wboolean|Wboolean|Winteger|Wboolean|Winteger|Winteger|",
                    "Winteger|Wboolean|Wboolean|",
                    "Wresource:stream filter|Wresource:stream filter|Winteger|Winteger|",
                    "Winteger:resource|Wboolean:resource|Nstring:resource|")
            };
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}: {}",
                String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
