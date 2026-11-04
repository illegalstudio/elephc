//! Purpose:
//! Verifies that the PHP 8.6 `Io\Poll` surface is absent under earlier profiles.
//!
//! Called from:
//! - `cargo test --test error_tests io_poll_php86`.
//!
//! Key details:
//! - Positive Io\Poll behavior is covered by `tests/codegen/io/io_poll_php86.rs` with
//!   `--php-version=8.6`; this module checks the version gate on the default 8.5 profile.

use super::*;

/// `Io\Poll\Context` is undeclared before PHP 8.6.
#[test]
fn test_io_poll_requires_php_86_profile() {
    expect_error(
        "<?php $context = new Io\\Poll\\Context();",
        "Undefined class: Io\\Poll\\Context",
    );
}
