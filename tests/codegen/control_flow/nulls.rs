//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow nulls, including ternary null is falsy.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests that `null` in a ternary condition is treated as falsy, matching PHP semantics.
/// Verifies that `$x = null; echo $x ? "yes" : "no";` outputs `"no"`.
#[test]
fn test_ternary_null_is_falsy() {
    let out = compile_and_run("<?php $x = null; echo $x ? \"yes\" : \"no\";");
    assert_eq!(out, "no");
}
