//! Purpose:
//! Integration or regression tests for diagnostic coverage of I/O builtin ownership and globals, including file ownership builtins reject invalid principals, umask wrong args, and global missing var.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies ownership builtins reject invalid principal types.
/// `chmod()` requires an integer mode; owner/group builtins require int or string principals.
/// Uses `expect_error()` to assert the correct diagnostic message for each case.
#[test]
fn test_error_file_ownership_builtins_reject_invalid_principals() {
    expect_error(
        r#"<?php chmod("file.txt", "0644");"#,
        "chmod() mode must be int",
    );
    expect_error(
        r#"<?php chown("file.txt", null);"#,
        "chown() owner/group must be int or string",
    );
    expect_error(
        r#"<?php chgrp("file.txt", null);"#,
        "chgrp() owner/group must be int or string",
    );
    expect_error(
        r#"<?php lchown("file.txt", null);"#,
        "lchown() owner/group must be int or string",
    );
    expect_error(
        r#"<?php lchgrp("file.txt", null);"#,
        "lchgrp() owner/group must be int or string",
    );
}

/// Verifies lchown()/lchgrp() reject the wrong number of arguments.
#[test]
fn test_error_lchown_lchgrp_wrong_args() {
    expect_error("<?php lchown(\"file.txt\");", "lchown() takes exactly 2 arguments");
    expect_error("<?php lchgrp(\"file.txt\");", "lchgrp() takes exactly 2 arguments");
}

/// Verifies that `umask()` rejects calls with more than 1 argument.
/// `umask()` accepts 0 or 1 arguments; extra positional arguments must be rejected.
#[test]
fn test_error_umask_wrong_args() {
    expect_error("<?php umask(1, 2);", "umask() takes 0 or 1 arguments");
}

// --- v0.6: switch/match/array errors ---

/// Verifies that the `global` keyword produces an error when no variable follows it.
/// The parser must emit "Expected variable after 'global'" for `global ;`.
#[test]
fn test_error_global_missing_var() {
    expect_error("<?php global ;", "Expected variable after 'global'");
}
