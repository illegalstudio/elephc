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

/// Rejects reference assignment to a property the class does not declare.
#[test]
fn dynamic_property_reference_is_rejected() {
    expect_error(
        "<?php $o = new stdClass(); $o->p = &$value;",
        "Reference assignment to a dynamic property is not supported",
    );
}

/// Rejects a reference element that targets PHP's global symbol table.
#[test]
fn globals_reference_element_is_rejected() {
    expect_error(
        "<?php $refs = [&$GLOBALS];",
        "$GLOBALS can only be modified using the $GLOBALS[$name] = $value syntax",
    );
}

/// Rejects reference appends until the writer can allocate the new element key and cell.
#[test]
fn reference_append_is_rejected() {
    expect_error(
        "<?php $array = []; $value = 1; $array[] = &$value;",
        "Reference append (`$array[] = &$value`) is not supported",
    );
    expect_error(
        "<?php $array = [[1]]; $value = 1; $array[0][] = &$value;",
        "Reference append (`$array[] = &$value`) is not supported",
    );
}
