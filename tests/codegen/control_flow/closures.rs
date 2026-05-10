//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow closures, including chained closure call.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_chained_closure_call() {
    let out = compile_and_run(
        "<?php $f = function() { return function() { return 99; }; }; echo $f()();",
    );
    assert_eq!(out, "99");
}

// --- do...while ---
