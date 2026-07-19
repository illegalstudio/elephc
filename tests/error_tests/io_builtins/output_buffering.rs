//! Purpose:
//! Integration tests for diagnostic coverage of the output-buffering (`ob_*`)
//! builtins: arity errors and the unsupported-output-handler rejection.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages.
//! - `ob_start()` admits null, closure, first-class-callable, and string-name
//!   handlers; array-pair callables are a compile-time error.

use super::*;

/// Verifies `ob_get_contents()` rejects any arguments with an arity error.
#[test]
fn test_error_ob_get_contents_wrong_args() {
    expect_error(
        "<?php ob_get_contents(1);",
        "ob_get_contents() takes no arguments",
    );
}

/// Verifies `ob_get_clean()` rejects any arguments with an arity error.
#[test]
fn test_error_ob_get_clean_wrong_args() {
    expect_error("<?php ob_get_clean(true);", "ob_get_clean() takes no arguments");
}

/// Verifies `ob_get_level()` rejects any arguments with an arity error.
#[test]
fn test_error_ob_get_level_wrong_args() {
    expect_error("<?php ob_get_level(0);", "ob_get_level() takes no arguments");
}

/// Verifies `ob_end_flush()` rejects any arguments with an arity error.
#[test]
fn test_error_ob_end_flush_wrong_args() {
    expect_error("<?php ob_end_flush(1);", "ob_end_flush() takes no arguments");
}

/// Verifies `ob_start()` caps its arity at the three PHP parameters.
#[test]
fn test_error_ob_start_too_many_args() {
    expect_error(
        "<?php ob_start(null, 0, 112, 9);",
        "ob_start() takes at most 3 arguments",
    );
}

/// Verifies `ob_start()` rejects an array-pair output-handler callback.
#[test]
fn test_error_ob_start_array_callback_unsupported() {
    expect_error(
        r#"<?php class C { public function m($b, $p) { return $b; } } ob_start([new C(), "m"]);"#,
        "ob_start() array output-handler callbacks are not supported; use a closure or function name",
    );
}

/// Verifies `ob_implicit_flush()` caps its arity at one argument.
#[test]
fn test_error_ob_implicit_flush_too_many_args() {
    expect_error(
        "<?php ob_implicit_flush(true, 1);",
        "ob_implicit_flush() takes at most 1 argument",
    );
}

/// Verifies `ob_get_status()` caps its arity at one argument.
#[test]
fn test_error_ob_get_status_too_many_args() {
    expect_error(
        "<?php ob_get_status(true, 1);",
        "ob_get_status() takes at most 1 argument",
    );
}
