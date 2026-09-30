//! Purpose:
//! Integration or regression tests for diagnostic coverage of I/O builtin ownership and globals, including chmod mode rejection, umask wrong args, and global missing var.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies `chmod()` rejects a mode php itself rejects.
///
/// The four ownership builtins used to be asserted here too, for a `null` principal — and that
/// was a refusal of a program php RUNS. MEASURED on `php -n` 8.5.6, `chown($f, null)` deprecates
/// and then answers for uid 0; `chgrp($f, null)` answers `bool(true)`. Their behaviour is pinned
/// by `codegen::io::modify` now, where an answer can be compared instead of a diagnostic.
///
/// `"abc"` is the case that survives, because php refuses it as well —
/// `chmod(): Argument #2 ($permissions) must be of type int, string given` — so elephc's compile
/// refusal is the same verdict, earlier. So is `"12abc"`: MEASURED, a LEADING-numeric string
/// throws too. A NUMERIC string is a different matter — php coerces `chmod($f, "0644")` to mode
/// 644 — and elephc now coerces it as well, pinned by
/// `test_chmod_coerces_its_mode_the_way_php_does`.
#[test]
fn test_error_chmod_rejects_a_mode_php_rejects_too() {
    expect_error(
        r#"<?php chmod("file.txt", "abc");"#,
        "chmod() mode must be int",
    );
    expect_error(
        r#"<?php chmod("file.txt", "12abc");"#,
        "chmod() mode must be int",
    );
    expect_no_error(r#"<?php chmod("file.txt", "0644");"#);
    expect_no_error(r#"<?php chmod("file.txt", " 644");"#);
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
