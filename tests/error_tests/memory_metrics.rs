//! Purpose:
//! Verifies shared heap metric signatures and extension visibility diagnostics.
//!
//! Called from:
//! - The frontend error integration suite.
//!
//! Key details:
//! - PHP memory functions remain available in strict PHP mode; counters are extensions.

use super::*;

/// All five metric contracts reject surplus arguments before backend lowering.
#[test]
fn test_memory_metrics_arity_errors() {
    for (call, name) in [
        ("memory_get_usage(false, true)", "memory_get_usage"),
        ("memory_get_peak_usage(false, true)", "memory_get_peak_usage"),
        ("memory_reset_peak_usage(1)", "memory_reset_peak_usage"),
        ("memory_get_allocations(1)", "memory_get_allocations"),
        ("memory_get_frees(1)", "memory_get_frees"),
    ] {
        let error = check_source(&format!("<?php {call};")).expect_err("extra arguments must fail");
        assert!(error.contains(name) && error.contains("argument"), "{call}: {error}");
    }
}

/// The real_usage flag rejects arrays and uses its canonical PHP parameter name.
#[test]
fn test_memory_metrics_flag_errors() {
    for call in ["memory_get_usage([])", "memory_get_peak_usage(real_usage: [])", "memory_get_usage(real: true)"] {
        assert!(check_source(&format!("<?php {call};")).is_err(), "{call}");
    }
}

/// Strict PHP retains the standard APIs and hides the two allocation-counter extensions.
#[test]
fn test_memory_metrics_strict_visibility() {
    let _guard = elephc::strict_php::scoped_enable();
    assert!(check_source("<?php memory_get_usage(); memory_get_peak_usage(true); memory_reset_peak_usage();").is_ok());
    for name in ["memory_get_allocations", "memory_get_frees"] {
        let error = check_source(&format!("<?php {name}();")).expect_err("extension must be hidden");
        assert!(error.contains("disabled by --strict-php"), "{error}");
    }
}
