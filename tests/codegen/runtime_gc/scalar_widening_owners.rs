//! Purpose:
//! Checks scalar owners overwritten before later frame-wide eval boxing.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - A future eval determines the physical storage used by preceding scalar stores.
//! - Early overwrites and unset must retire those boxed values in both optimizer modes.

use crate::support::*;

/// Scalar stores and unset retire old boxes even when opaque eval occurs later in source order.
#[test]
fn test_future_eval_boxing_releases_earlier_scalar_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$flag = $argc > 0;
$number = 13;
$number = 17;
$fraction = 1.25;
$fraction = 2.5;
echo (int)$flag, ':', $number, ':', $fraction, '|';
unset($flag, $number, $fraction);
$source = $argc > 0 ? 'echo (int)isset($flag), (int)isset($number), (int)isset($fraction);' : '';
eval($source);
unset($source);
"#);
    assert!(out.success, "stdout={:?}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:17:2.5|000", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Scalar retirement before eval boxing also holds without EIR optimization.
#[test]
fn test_future_eval_boxing_releases_earlier_scalar_owners_no_ir_opt() {
    without_ir_opt(test_future_eval_boxing_releases_earlier_scalar_owners);
}
