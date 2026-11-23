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

/// Rejects reference assignment to a property slot, including an undeclared one.
#[test]
fn dynamic_property_reference_is_rejected() {
    expect_error(
        "<?php $o = new stdClass(); $o->p = &$value;",
        "Reference assignment to a property is not supported",
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

/// The reverse property direction stays rejected until the writer can replace property storage.
#[test]
fn property_reference_assignment_is_rejected() {
    expect_error(
        "<?php class Box { public mixed $value; } $box = new Box(); $value = 1; $box->value = &$value;",
        "Reference assignment to a property is not supported",
    );
}

/// Static property storage has the same two-step limitation as instance properties.
#[test]
fn static_property_reference_assignment_is_rejected() {
    expect_error(
        "<?php class Box { public static mixed $value; } $value = 1; Box::$value = &$value;",
        "Reference assignment to a static property is not supported",
    );
}

/// A nested element needs a temporary intermediate cell the two-step lowering cannot bind.
#[test]
fn nested_array_element_reference_assignment_is_rejected() {
    expect_error(
        "<?php $array = [[1]]; $value = 1; $array[0][0] = &$value;",
        "Reference assignment to a nested array element is not supported",
    );
}

/// A property array element cannot expose its entry's storage to the reference writer.
#[test]
fn property_array_element_reference_assignment_is_rejected() {
    expect_error(
        "<?php class Box { public array $items = [1]; } $box = new Box(); $value = 1; $box->items[0] = &$value;",
        "Reference assignment to a property array element is not supported",
    );
}

/// A temporary array base has no variable storage for the reference writer to replace.
#[test]
fn temporary_array_reference_assignment_is_rejected() {
    expect_error(
        "<?php function items(): array { return [1]; } $value = 1; items()[0] = &$value;",
        "Reference assignment to an array element requires a variable array",
    );
}

/// A `$GLOBALS` element has no writable reference-cell storage in the array writer.
#[test]
fn globals_reference_assignment_is_rejected() {
    expect_error(
        "<?php $value = 1; $GLOBALS[\"k\"] = &$value;",
        "Reference assignment to a $GLOBALS element is not supported",
    );
}

/// A string offset is not reference storage in PHP.
#[test]
fn string_offset_reference_assignment_is_rejected() {
    expect_error(
        "<?php $text = \"abc\"; $value = \"Z\"; $text[0] = &$value;",
        "Cannot create references to/from string offsets",
    );
}

/// A scoped receiver is parsed before the diagnostic, so the property shape is named.
#[test]
fn scoped_receiver_property_reference_assignment_is_rejected() {
    expect_error(
        "<?php class Factory { public static function make(): object { return new stdClass; } } $value = 1; Factory::make()->slot = &$value;",
        "Reference assignment to a property is not supported",
    );
}
