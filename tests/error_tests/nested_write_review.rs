//! Purpose:
//! Keeps nested write-context conversion scoped away from ordinary reads and selector expressions.
//!
//! Called from:
//! - The diagnostic integration suite.
//!
//! Key details:
//! - False array elements may become write parents, not readable containers.

use super::*;

/// Ordinary reads through a statically false array element keep their existing diagnostic.
#[test]
fn test_nested_write_eleventh_review_false_read_rejected() {
    expect_error("<?php $items = [false]; echo $items[0][0];", "Cannot index non-array");
}

/// A selector remains a read even while its containing target permits false conversion.
#[test]
fn test_nested_write_eleventh_review_false_selector_rejected() {
    expect_error("<?php $items = [false]; $key = [false]; $items[0][$key[0][0]][1] = 9;",
        "Cannot index non-array");
}

/// Write-parent recursion does not hide an undefined selector variable.
#[test]
fn test_nested_write_eleventh_review_undefined_selector_rejected() {
    expect_error("<?php $items = [false]; $items[0][$missing][1] = 9;", "Undefined variable: $missing");
}

/// A false probe stays on the ordinary read/probe validation path.
#[test]
fn test_nested_write_eleventh_review_false_probe_rejected() {
    expect_error("<?php $items = [false]; echo isset($items[0][0]);", "Cannot index non-array");
}
