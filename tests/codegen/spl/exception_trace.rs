//! Purpose:
//! Integration tests for php's `Stack trace:` block, and for `Throwable::getTraceAsString()`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A trace that is SHORT is not an approximation. `#0 {main}` where php names a frame asserts the
//!   stack was empty, which is a WRONG answer rather than a missing one — so nothing is printed
//!   unless the frame list is known complete, and these tests pin both sides of that: what is
//!   printed, and what deliberately is not.
//! - An exception raised inside a builtin class is reported with the CALL as its frame `#0` —
//!   MEASURED on `php -n` 8.5.6, `(new SplFileInfo("nope.txt"))->getSize()` reports
//!   `#0 file(N): SplFileInfo->getSize()` then `#1 {main}`, and a `new` reports
//!   `DirectoryIterator->__construct('missing-dir')`, arguments included.
//! - The completeness proof travels ON the throwable, because by report time the site that built
//!   it is gone; a global consulted then answers for whatever was constructed last.

use crate::support::*;

/// Verifies the frame a builtin-class METHOD call contributes, and the tail that follows it.
#[test]
fn an_exception_from_a_builtin_method_names_the_call_as_frame_zero() {
    let out = compile_and_run_capture(
        r#"<?php
$info = new SplFileInfo("nope.txt");
$info->getSize();
"#,
    );
    assert_eq!(out.exit_code, Some(255));
    // The block lands in the PROGRAM stream: its lines carry no `Warning:`-style kind prefix, so
    // the harness's diagnostic split leaves them where php wrote them.
    let report = &out.stdout;
    assert!(report.contains("Stack trace:"), "no trace block: {report}");
    assert!(
        report.contains("SplFileInfo->getSize()"),
        "frame #0 must name the call: {report}"
    );
    assert!(report.contains("#1 {main}"), "{report}");
    assert!(
        report.contains("thrown in") && report.contains("on line 3"),
        "the tail belongs to the thrown exception: {report}"
    );
}

/// Verifies a CONSTRUCTOR frame, with the argument php renders inside it.
#[test]
fn a_builtin_constructor_frame_carries_its_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
new DirectoryIterator("missing-dir");
"#,
    );
    assert_eq!(out.exit_code, Some(255));
    assert!(
        out.stdout
            .contains("DirectoryIterator->__construct('missing-dir')"),
        "{}",
        out.stdout
    );
}

/// Verifies `getTraceAsString()` on an exception built where nothing is above it.
///
/// php answers `#0 {main}` — never the empty string — and the frames are newline-SEPARATED, so
/// the sentinel ends the text without one.
#[test]
fn get_trace_as_string_answers_main_for_a_frameless_exception() {
    let out = compile_and_run_capture(
        r#"<?php
$e = new RuntimeException("top level");
var_dump($e->getTraceAsString(), $e->getTrace());
var_dump($e->getTraceAsString() === $e->getTraceAsString());
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string(9) \"#0 {main}\"\narray(0) {\n}\nbool(true)\n"
    );
}

/// Verifies an exception built inside a USER function says nothing rather than something short.
///
/// php reports `#0 file(N): f()` then `#1 {main}`. elephc cannot walk to `f()`'s frame, so it
/// answers the empty string — a missing answer, not a wrong one. Asking twice must not change it,
/// and asking must not disturb the frames of anything else.
#[test]
fn a_trace_that_cannot_be_proven_complete_stays_empty() {
    let out = compile_and_run_capture(
        r#"<?php
function f(): Throwable { return new LogicException("in f"); }
$inner = f();
var_dump($inner->getTraceAsString());
$top = new RuntimeException("top");
var_dump($top->getTraceAsString());
var_dump($inner->getTraceAsString());
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string(0) \"\"\nstring(9) \"#0 {main}\"\nstring(0) \"\"\n"
    );
}
