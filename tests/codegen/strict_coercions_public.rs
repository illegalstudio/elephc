//! Purpose:
//! Verifies public exact value contracts without changing independently selected guarantees.
//!
//! Called from:
//! - Focused CLI admission tests after native eval property/reference parity.
//!
//! Key details:
//! - Source evidence and runtime value guards are tested through the real compiler pipeline.
//! - Shared fixtures also serve staged optimizer and ownership controls.

use crate::support::*;

/// Public parameter, return, builtin and callback contracts retain original values and owners.
#[test]
fn test_strict_coercions_public_native_contracts_bundle() {
    for (source, expected) in [
        (include_str!("../fixtures/strict/conversion_parameters.php"), "F:4.25ISbETNDSBEMLVCUPifnQ:23"),
        (include_str!("../fixtures/strict/conversion_returns.php"), "FSb:4.25:-INF:owned-|NSBOR2ZTCUAM1fnQ:1:23"),
        (include_str!("../fixtures/strict/builtin_value_inputs.php"), "TTTTTNPCEOEMJ|1A|abcbc"),
    ] {
        for optimization in ["--ir-opt=off", "--ir-opt=on"] {
            let output = StrictFixture::new(source).capture(&["--strict-coercions", optimization, "--heap-debug"]);
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Explicit casts remain intentional conversions and known invalid boundaries fail before folding.
#[test]
fn test_strict_coercions_public_source_contracts_and_explicit_conversions() {
    for source in [
        "<?php declare(strict_types=0); function exactFloat(float $value): void {} exactFloat(1);",
        "<?php if (false) { strlen(7); }",
        "<?php str_repeat('a', 2.0);",
        "<?php function exactResult(): float { return 1; }",
        "<?php class ExactDefault { public float $value = 1; }",
    ] {
        StrictFixture::new(source).assert_error(&["--strict-coercions"], "strict-coercions");
    }
    assert_eq!(StrictFixture::new("<?php function convertedFloat(float $value): float { return $value; } echo convertedFloat((float) 1), '|', strval(7), '|', 7 . 'x';")
        .run(&["--strict-coercions"]), "1|7|7x");
}

/// The shared example makes a float conversion explicit and demonstrates a rejected integer input.
#[test]
fn test_strict_coercions_public_example_combines_independent_profiles() {
    let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/strict-semantics/main.php")).unwrap();
    let output = StrictFixture::new(&source).capture(&["--strict-coercions", "--strict-dynamic-code",
        "--strict-bools", "--strict-arithmetic", "--strict-overflow", "--heap-debug"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), concat!(
        "Verified at compilation\nAccepted a boolean\nAccepted a boolean\nCondition requires bool\n",
        "Explicit conversion\nAttempts: 3\nRatio: 1.5\nTimeout: 30\nSettings timeout: 30\n",
        "Displayed timeout: 30\nDivision result: double\nArithmetic requires numeric values\n",
        "Integer overflow was rejected\nRate requires float\nRate: 2.5\n"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
        "{}", String::from_utf8_lossy(&output.stderr));
}
