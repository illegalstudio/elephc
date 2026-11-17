//! Purpose:
//! Verifies runtime parameter and inferred callable binding origins in native programs.
//!
//! Called from:
//! - Focused strict local codegen and runtime plugin example tests.
//!
//! Key details:
//! - One opaque-eval bundle covers parameters, aliases, methods and variadic collectors.
//! - Both optimizer settings preserve rejected values and balanced heap ownership.

use crate::support::*;

/// Runtime declarations retain original parameter alternatives while mixed/untyped stay open.
#[test]
fn test_strict_parameter_locals_native_eval_and_owners_bundle() {
    let source = include_str!("../fixtures/strict/eval_parameter_bindings.php");
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(source).capture(&flags);
            let expected = if strict {
                concat!("Tinteger|Winteger|Tdouble|Wdouble|TNULL|Winteger|Wdouble|double|double|",
                    "Tstring|Tstring|Wstring|Tarray|Warray|Tarray|Warray|Wstring11|Tobject2|",
                    "Tinteger|string|Tinteger|Tinteger|Tinteger|TRuntimeBindingMethods|TRuntimeBindingMethods|")
            } else {
                concat!("Wdouble|Winteger|Winteger|Wdouble|Wdouble|Winteger|Wdouble|double|double|",
                    "Winteger|Wstring|Wstring|Winteger|Warray|Winteger|Warray|Wstring11|Wstring|",
                    "Wdouble|string|Wdouble|Wdouble|Wdouble|WstdClass|WstdClass|")
            };
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}: {}",
                String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// The plugin example preserves parameter/capture contracts and replaces an inferred closure by name.
#[test]
fn test_strict_parameter_locals_runtime_plugin_example() {
    let output = StrictFixture::new(include_str!("../../examples/strict-eval-locals/main.php"))
        .capture(&["--strict-locals", "--heap-debug"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), concat!(
        "The plugin quantity kept its integer value\nPlugin quantity: 2\nQuantity: 3\nThe captured quantity kept its integer value\n",
        "Plugin total: 3\nThe static total kept its integer value\nPlugin total: 3\n"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
        "{}", String::from_utf8_lossy(&output.stderr));
}
