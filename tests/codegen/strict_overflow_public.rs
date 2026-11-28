//! Purpose:
//! Verifies public checked-overflow admission across native compilation and runtime source.
//!
//! Called from:
//! - Focused codegen tests for independent and combined numeric profiles.
//!
//! Key details:
//! - Failed writes retain their source storage, and repeated eval uses the same immutable profile.

use crate::support::*;

/// Public entry points contain overflow, preserve destinations and compose with arithmetic independently.
#[test]
fn test_strict_overflow_public_cli_native_eval_bundle() {
    let source = include_str!("../fixtures/strict/overflow_public.php");
    for arithmetic in [false, true] {
        for optimization in ["--ir-opt=off", "--ir-opt=on"] {
            let mut flags = vec!["--strict-overflow", optimization, "--heap-debug"];
            if arithmetic { flags.push("--strict-arithmetic"); }
            let output = StrictFixture::new(source).capture(&flags);
            let middle = if arithmetic { "D|Q|F" } else { "3|0.5|I" };
            assert_eq!(String::from_utf8_lossy(&output.stdout),
                format!("ASMPNBRUKCKTKI|{middle}|UKABRUKABR"), "arithmetic={arithmetic}, {optimization}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "arithmetic={arithmetic}, {optimization}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Integer spellings and proven arithmetic fail before folding while explicit floats remain IEEE values.
#[test]
fn test_strict_overflow_public_source_evidence_and_float_control() {
    for source in ["<?php echo 9223372036854775808;", "<?php echo PHP_INT_MAX + 1;",
        "<?php echo -(-9223372036854775808);", "<?php if (false) { echo 2 ** 63; }"] {
        StrictFixture::new(source).assert_error(&["--strict-overflow"], "strict-overflow");
    }
    assert_eq!(StrictFixture::new("<?php echo is_float(1e999), ':', is_float(1.0e308 * 1.0e308), ':', -9223372036854775808;")
        .run(&["--strict-overflow"]), "1:1:-9223372036854775808");
    assert_eq!(StrictFixture::new("<?php echo is_float(PHP_INT_MAX + 1) ? 'F' : 'X';")
        .run(&["--no-strict-overflow"]), "F");
}

/// Generic RHS failures preserve late-owned destination arrays without selecting any strict flag.
#[test]
fn test_array_write_pending_receiver_native_ownership_bundle() {
    let source = include_str!("../fixtures/strict/array_write_pending_owner.php");
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&[optimization, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "CE|1", "{optimization}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
