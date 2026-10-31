//! Purpose:
//! Checks diagnostics for unsupported by-reference array-literal element shapes.
//!
//! Called from:
//! - The `error_tests` integration-test root through Rust's test harness.
//!
//! Key details:
//! - Lowering materializes a reference element by rebinding a local to the new entry's cell, so
//!   only variables are accepted today; other lvalues fail before code generation.
//! - A bare reference after a spread has no compile-time integer key to promote.

use super::*;

/// Rejects a temporary expression as a reference element.
#[test]
fn reference_array_element_requires_a_variable() {
    expect_error(
        "<?php $refs = [&1];",
        "Reference array elements must reference a variable",
    );
    expect_error(
        "<?php $refs = [&$items[0]];",
        "Reference array elements must reference a variable",
    );
}

/// Rejects a bare reference after a spread until its runtime key can be promoted.
#[test]
fn bare_reference_after_spread_is_rejected() {
    expect_error(
        "<?php $refs = [...$items, &$value];",
        "Reference array elements after a spread are not supported",
    );
}
