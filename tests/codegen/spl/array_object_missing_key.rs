//! Purpose:
//! Integration tests for READING a key an `ArrayObject` or `ArrayIterator` does not have.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - php raises `Undefined array key <k>` for it, exactly as it does for a plain array, and
//!   answers NULL. elephc answered NULL in SILENCE: the value was right, the diagnostic missing.
//!   MEASURED on `php -n` 8.5.6 across both classes and both spellings — `offsetGet($k)` and
//!   `$o[$k]` — which is six warnings, none of which elephc raised.
//! - The key's RENDERING is part of the message: an int bare, a string quoted. Reading out of an
//!   empty array is php's own way of producing it, so both come from the runtime path a plain
//!   array already uses rather than from a second formatter.
//! - The LINE is the caller's. A synthesized body publishes no span of its own, so it inherits
//!   whatever the call site published — which is why the call into a synthesized class publishes
//!   its line whether or not the effect refinement believed the call could warn.

use crate::support::*;

/// Verifies both classes and both spellings warn, with php's key rendering and the caller's line.
#[test]
fn reading_an_absent_key_warns_like_a_plain_array() {
    let out = compile_and_run_capture(
        r#"<?php
$a = new ArrayObject([1, 2, 3]);
var_dump($a->offsetGet(99));
var_dump($a[99]);
var_dump($a->offsetExists(99));
$h = new ArrayObject(["k" => 1]);
var_dump($h->offsetGet("nope"));
var_dump($h["nope"]);
$it = new ArrayIterator([1]);
var_dump($it->offsetGet(7));
var_dump($it[7]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let warnings: Vec<&str> = out
        .located_diagnostics
        .lines()
        .filter(|line| line.starts_with("Warning: Undefined array key"))
        .collect();
    assert_eq!(warnings.len(), 6, "got {}", out.located_diagnostics);
    for (warning, expected) in warnings.iter().zip([
        ("99", 3),
        ("99", 4),
        ("\"nope\"", 7),
        ("\"nope\"", 8),
        ("7", 10),
        ("7", 11),
    ]) {
        assert!(
            warning.contains(&format!("Undefined array key {} ", expected.0))
                && warning.ends_with(&format!(" on line {}", expected.1)),
            "expected key {} at line {}, got {warning}",
            expected.0,
            expected.1
        );
    }
}

/// Verifies a key that IS present stays silent, and that `offsetExists` never warns.
///
/// The miss path reads out of an empty array to borrow php's diagnostic; a hit must not go
/// anywhere near it, and neither must the question `offsetExists()` exists to answer.
#[test]
fn a_present_key_and_an_existence_check_stay_silent() {
    let out = compile_and_run_capture(
        r#"<?php
$a = new ArrayObject(["k" => 1, 5 => "v"]);
var_dump($a->offsetGet("k"), $a[5], $a->offsetExists("k"), $a->offsetExists("gone"));
foreach ($a as $key => $value) { var_dump($key, $value); }
var_dump(count($a), $a->getArrayCopy());
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert!(
        !out.located_diagnostics.contains("Undefined array key"),
        "unexpected warning: {}",
        out.located_diagnostics
    );
}
