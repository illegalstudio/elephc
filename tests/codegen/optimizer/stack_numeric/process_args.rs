//! Purpose:
//! Checks retirement of implicit process arguments before frame-numeric overwrites.
//!
//! Called from:
//! - The focused optimizer codegen suite.
//!
//! Key details:
//! - Runtime seeds keep checked arithmetic live in both IR modes.
//! - PHP and Elephc both print the rounded float after integer overflow.

use super::*;

/// The OS argc box is released before a frame-backed numeric seed replaces it.
#[test]
fn test_stack_numeric_process_argc_seed_retires_implicit_heap_owner() {
    let source = r#"<?php
$argc = strlen($argv[0]);
$argc += PHP_INT_MAX;
echo (float)$argc;
"#;
    for optimized in [false, true] {
        let (stdout, _) = run(source, optimized);
        assert_eq!(stdout, "9.2233720368548E+18");
    }
}
