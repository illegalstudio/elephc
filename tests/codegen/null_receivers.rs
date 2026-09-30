//! Purpose:
//! Integration tests for PHP's semantics when a READ goes through `null`: a property read and
//! an array offset each warn and answer `null`, and the program keeps running.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - elephc used to REFUSE both — `Property access requires an object or typed pointer` and
//!   `Cannot index non-array` — which are the last two members of the family
//!   `tests/codegen/undefined_variables.rs` opened: PHP answers a read it cannot perform with a
//!   warning and `null` rather than refusing the program.
//! - Whether the receiver is null is a RUN-TIME fact, so the diagnostic is raised by EIR
//!   lowering and the checker only types the read as null. Same split, same reason.
//! - The warnings travel on the DIAGNOSTIC stream, which is php's stdout.
//! - Every expectation below was measured on `php -n` 8.5.6.

use crate::support::*;

/// Verifies reading a property on null warns and answers null instead of refusing.
///
/// MEASURED: `$o = null; var_dump($o->name);` prints
/// `Warning: Attempt to read property "name" on null` and then `NULL`, exit 0. The property
/// NAME is in the message, so the warning identifies which read it was.
#[test]
fn test_a_property_read_on_null_warns_and_answers_null() {
    let out = compile_and_run_capture("<?php $o = null; var_dump($o->name);\n");
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\n");
    assert_eq!(
        out.diagnostics,
        "Warning: Attempt to read property \"name\" on null\n"
    );
}

/// Verifies reading an array offset on null warns and answers null instead of refusing.
///
/// The twin of the property read, and PHP words it differently — `Trying to access array offset
/// on null` — which is why both spellings are pinned rather than one standing for the other.
/// A string key and an integer key give the same message.
#[test]
fn test_an_array_offset_on_null_warns_and_answers_null() {
    for source in [
        "<?php $o = null; var_dump($o[0]);\n",
        "<?php $o = null; var_dump($o[\"k\"]);\n",
    ] {
        let out = compile_and_run_capture(source);
        assert!(out.success, "program failed: {} ({source})", out.stderr);
        assert_eq!(out.stdout, "NULL\n", "for {source}");
        assert_eq!(
            out.diagnostics,
            "Warning: Trying to access array offset on null\n",
            "for {source}",
        );
    }
}

/// Verifies an UNDEFINED variable used as a receiver raises BOTH warnings, in order.
///
/// This is where the two families meet: PHP raises `Undefined variable $x` for the read of the
/// name and then the receiver diagnostic for what was done with the null it answered. Getting
/// only one of them would mean either the read or the access had been silently tolerated.
#[test]
fn test_an_undefined_receiver_raises_both_warnings() {
    let property = compile_and_run_capture("<?php var_dump($x->name);\n");
    assert!(property.success, "program failed: {}", property.stderr);
    assert_eq!(property.stdout, "NULL\n");
    assert_eq!(
        property.diagnostics,
        "Warning: Undefined variable $x\n\
         Warning: Attempt to read property \"name\" on null\n"
    );

    let offset = compile_and_run_capture("<?php var_dump($x[0]);\n");
    assert!(offset.success, "program failed: {}", offset.stderr);
    assert_eq!(offset.stdout, "NULL\n");
    assert_eq!(
        offset.diagnostics,
        "Warning: Undefined variable $x\n\
         Warning: Trying to access array offset on null\n"
    );
}

/// Verifies the null probes stay SILENT for a read through a null receiver.
///
/// `isset($x->p)`, `empty($x->p)` and `$x->p ?? "d"` answer without raising anything — MEASURED
/// on `php -n` 8.5.6, which prints `bool(false)`, `bool(true)` and `"d"` and nothing else, even
/// though `$x` was never assigned and the property read goes through null.
///
/// Both halves had to be handled and each failed differently: the read reached `prop_get` with a
/// null receiver, which the backend refuses outright, and the receiver was lowered outside the
/// probe so it warned about `$x`. An empty diagnostic stream is what says both are fixed.
#[test]
fn test_the_null_probes_are_silent_through_a_null_receiver() {
    let out = compile_and_run_capture(
        "<?php var_dump(isset($x->p), empty($x->p), $x->p ?? \"d\", isset($x[0]), $x[0] ?? \"e\");\n",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(true)\nstring(1) \"d\"\nbool(false)\nstring(1) \"e\"\n"
    );
    assert_eq!(out.diagnostics, "");
}

/// Verifies `?->` keeps its own contract: it reaches through null WITHOUT the warning.
///
/// The nullsafe operator exists to make exactly this read silent, so it must not pick up the
/// diagnostic its ordinary spelling now raises. Both forms appear here so the pair is the test:
/// one warns, the other does not, and both answer NULL.
#[test]
fn test_the_nullsafe_operator_reaches_through_null_in_silence() {
    let out = compile_and_run_capture("<?php $o = null; var_dump($o?->name);\n");
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\n");
    assert_eq!(out.diagnostics, "");
}

/// Verifies the INDEX of an offset read through null is still evaluated.
///
/// PHP evaluates the index before it discovers the base is null, so a diagnostic owed by the
/// index is raised too. Skipping the index would be the easy way to implement the null base and
/// would silently drop that.
#[test]
fn test_the_index_of_a_null_offset_read_is_still_evaluated() {
    let out = compile_and_run_capture("<?php $o = null; var_dump($o[$missing]);\n");
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\n");
    assert!(
        out.diagnostics.contains("Warning: Undefined variable $missing"),
        "the index is an ordinary read and must warn, got {:?}",
        out.diagnostics
    );
    assert!(
        out.diagnostics
            .contains("Warning: Trying to access array offset on null"),
        "the null base must still warn, got {:?}",
        out.diagnostics
    );
}
