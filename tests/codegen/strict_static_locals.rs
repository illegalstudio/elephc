//! Purpose:
//! Checks persistent interpreted static bindings and Reflection snapshot membership.
//!
//! Called from:
//! - Focused strict static-local end-to-end regressions.
//!
//! Key details:
//! - Both optimizer modes cover initialization failures, aliases and detached replacements.

use crate::support::*;

/// Static origins remain stable while detached locals and independent snapshots stay writable.
#[test]
fn test_strict_static_local_origins_and_lifetimes_bundle() {
    let source = include_str!("../fixtures/strict/static_local_origins.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(source).capture(&flags);
            let expected = if strict {
                "STinteger:1|STinteger:1|UWdouble|Mdouble|RTinteger|Pinteger:1|I2|A2|ATinteger|Ninteger:1|Ninteger:1|C9:1|Xold|Xold|F2|Z2:2|B1:3.5|B1:3.5|G1|G1|Jdouble:2.5|Dinteger:1|Dinteger:1|"
            } else {
                "SWdouble:2.5|SWdouble:3.5|UWdouble|Mdouble|RWdouble|Pinteger:1|I2|A2|AWdouble|Ninteger:1|Ninteger:1|C9:1|Xold|Xold|F2|Z2:2|B1:3.5|B1:3.5|G1|G1|Jdouble:2.5|Dinteger:1|Dinteger:1|"
            };
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
