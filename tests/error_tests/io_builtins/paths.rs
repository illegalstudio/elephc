//! Purpose:
//! Integration or regression tests for diagnostic coverage of I/O builtin paths, including rewind wrong args, ftell wrong args, and fseek wrong args.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

#[test]
fn test_error_rewind_wrong_args() {
    expect_error("<?php rewind();", "rewind() takes exactly 1 argument");
}

#[test]
fn test_error_ftell_wrong_args() {
    expect_error("<?php ftell();", "ftell() takes exactly 1 argument");
}

#[test]
fn test_error_fseek_wrong_args() {
    expect_error("<?php fseek(1);", "fseek() takes 2 or 3 arguments");
}

#[test]
fn test_error_file_wrong_args() {
    expect_error("<?php file();", "file() takes exactly 1 argument");
}

#[test]
fn test_error_readline_wrong_args() {
    expect_error(
        r#"<?php readline(1, 2);"#,
        "readline() takes 0 or 1 arguments",
    );
}

#[test]
fn test_error_fgetcsv_wrong_args() {
    expect_error("<?php fgetcsv();", "fgetcsv() takes 1 to 3 arguments");
}

#[test]
fn test_error_fputcsv_wrong_args() {
    expect_error("<?php fputcsv(1);", "fputcsv() takes 2 to 4 arguments");
}

#[test]
fn test_error_dirname_wrong_args() {
    expect_error("<?php dirname();", "dirname() takes 1 or 2 arguments");
}

#[test]
fn test_error_basename_wrong_args() {
    expect_error("<?php basename();", "basename() takes 1 or 2 arguments");
}

#[test]
fn test_error_dirname_rejects_static_levels_below_one() {
    expect_error(
        r#"<?php dirname("/tmp/file", 0);"#,
        "dirname() levels must be greater than or equal to 1",
    );
}

#[test]
fn test_error_fnmatch_wrong_args() {
    expect_error("<?php fnmatch(\"*.txt\");", "fnmatch() takes 2 or 3 arguments");
}

#[test]
fn test_error_fnmatch_rejects_non_int_flags() {
    expect_error(
        r#"<?php fnmatch("*.TXT", "file.txt", "casefold");"#,
        "fnmatch() flags must be int",
    );
}

#[test]
fn test_error_pathinfo_rejects_non_int_flags() {
    expect_error(
        r#"<?php
$flag = "extension";
echo pathinfo("foo.txt", $flag);
"#,
        "pathinfo() flag must be int",
    );
}

#[test]
fn test_error_realpath_wrong_args() {
    expect_error("<?php realpath();", "realpath() takes exactly 1 argument");
}

#[test]
fn test_error_touch_rejects_invalid_timestamp_args() {
    expect_error(
        r#"<?php touch("file.txt", "now");"#,
        "touch() timestamp arguments must be int or null",
    );
    expect_error(
        r#"<?php touch("file.txt", null, 1000);"#,
        "touch() mtime cannot be null when atime is provided",
    );
    expect_error(
        r#"<?php
$mtime = null;
touch("file.txt", $mtime, 1000);
"#,
        "touch() mtime cannot be null when atime is provided",
    );
}

