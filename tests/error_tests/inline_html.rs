//! Purpose:
//! Diagnostic coverage for inline HTML shapes PHP itself rejects.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (`tests/error_tests.rs`).
//!
//! Key details:
//! - Inline HTML lexes as `echo "text";`, so output between `switch (x):` and its first `case`
//!   is a syntax error exactly as in PHP, and an `endif` with no open block stays an error.

use super::*;

/// Verifies output (even whitespace) between `switch (x):` and the first case is refused.
#[test]
fn test_error_inline_html_before_first_case() {
    expect_error(
        "<?php switch (1): ?> x <?php case 1: break; endswitch;",
        "Expected 'case' or 'default' inside switch",
    );
}

/// Verifies an `endif` closing nothing is still refused after a close tag.
#[test]
fn test_error_endif_without_open_block_after_html() {
    expect_error(
        "<?php ?>text<?php endif;",
        "there is no open alternative-syntax block",
    );
}
