//! Purpose:
//! Verifies independent copied local origins across pure and bridge-backed eval scopes.
//!
//! Called from:
//! - Focused end-to-end codegen test selection.
//!
//! Key details:
//! - Both optimizers preserve exact origins without changing PHP value-copy ownership.

use crate::support::*;

/// Independent literal-produced copies retain origins across propagation, aliases and element changes.
#[test]
fn test_strict_literal_eval_independent_native_copy_origins() {
    let source = include_str!("../fixtures/strict/literal_eval_native_copy_origin.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            for bridge in [false, true] {
                let source = if bridge { format!("{source}\n$other=1; eval('unset($other);');") }
                    else { source.to_string() };
                let mut flags = vec![mode, "--heap-debug"];
                if strict { flags.push("--strict-locals"); }
                if !bridge { flags.push("--strict-dynamic-code"); }
                let output = StrictFixture::new(&source).capture(&flags);
                let expected = if strict { concat!("Tdouble:2.5|T2.5:3.5:3.5|Tstring:before|TNULL|",
                    "Tarray:1:ok|integer:1|T2.5:2.5:2.5|Tinteger:1|Tinteger:1|") }
                    else { concat!("Winteger:1|W2.5:3.5:1|Winteger:1|Winteger|",
                        "Warray:1:ok|integer:1|W2.5:1:1|Wdouble:3.5|Wdouble:3.5|") };
                assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}: {}",
                    String::from_utf8_lossy(&output.stderr));
                assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                    "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}
