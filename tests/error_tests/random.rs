//! Purpose:
//! Diagnostic coverage for the `random` extension's prelude-provided procedural
//! functions and their arity rules.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `random_bytes`, `mt_srand`, `srand` and `lcg_value` are prelude functions, so an
//!   invalid argument count is a compile-time diagnostic rather than PHP's runtime
//!   `TypeError`. These assertions pin elephc's wording for that path.

use super::*;

/// `mt_srand()` accepts zero, one or two arguments.
#[test]
fn test_error_mt_srand_arity() {
    expect_error(
        "<?php mt_srand(1, 2, 3);",
        "Function 'mt_srand' expects 0 to 2 arguments, got 3",
    );
}

/// `lcg_value()` takes no arguments.
#[test]
fn test_error_lcg_value_arity() {
    expect_error(
        "<?php lcg_value(1);",
        "Function 'lcg_value' expects 0 arguments, got 1",
    );
}
