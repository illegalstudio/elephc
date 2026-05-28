//! Purpose:
//! Integration or regression tests for diagnostic coverage of I/O builtin streams, including var dump wrong args, print r wrong args, and fopen wrong args.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies var_dump() produces correct error when called with no arguments.
#[test]
fn test_error_var_dump_wrong_args() {
    expect_error("<?php var_dump();", "var_dump() takes exactly 1 argument");
}

/// Verifies print_r() produces correct error when called with no arguments.
#[test]
fn test_error_print_r_wrong_args() {
    expect_error("<?php print_r();", "print_r() takes exactly 1 argument");
}

/// Verifies fopen() produces correct error when called with only one argument.
#[test]
fn test_error_fopen_wrong_args() {
    expect_error(
        r#"<?php fopen("file");"#,
        "fopen() takes exactly 2 arguments",
    );
}

/// Verifies fclose() produces correct error when called with no arguments.
#[test]
fn test_error_fclose_wrong_args() {
    expect_error("<?php fclose();", "fclose() takes exactly 1 argument");
}

/// Verifies fclose() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fclose_requires_resource_handle() {
    expect_error("<?php fclose(1);", "fclose() expects resource, got int");
}

/// Verifies fread() produces correct error when called with only one argument.
#[test]
fn test_error_fread_wrong_args() {
    expect_error("<?php fread(1);", "fread() takes exactly 2 arguments");
}

/// Verifies fread() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fread_requires_resource_handle() {
    expect_error("<?php fread(1, 1);", "fread() expects resource, got int");
}

/// Verifies fwrite() produces correct error when called with only one argument.
#[test]
fn test_error_fwrite_wrong_args() {
    expect_error("<?php fwrite(1);", "fwrite() takes exactly 2 arguments");
}

/// Verifies fwrite() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fwrite_requires_resource_handle() {
    expect_error(
        r#"<?php fwrite(1, "x");"#,
        "fwrite() expects resource, got int",
    );
}

/// Verifies fgets() produces correct error when called with no arguments.
#[test]
fn test_error_fgets_wrong_args() {
    expect_error("<?php fgets();", "fgets() takes exactly 1 argument");
}

/// Verifies fgets() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fgets_requires_resource_handle() {
    expect_error("<?php fgets(1);", "fgets() expects resource, got int");
}

/// Verifies fgetc() produces correct error when called with no arguments.
#[test]
fn test_error_fgetc_wrong_args() {
    expect_error("<?php fgetc();", "fgetc() takes exactly 1 argument");
}

/// Verifies fgetc() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fgetc_requires_resource_handle() {
    expect_error("<?php fgetc(1);", "fgetc() expects resource, got int");
}

/// Verifies fpassthru() produces correct error when called with no arguments.
#[test]
fn test_error_fpassthru_wrong_args() {
    expect_error("<?php fpassthru();", "fpassthru() takes exactly 1 argument");
}

/// Verifies fpassthru() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fpassthru_requires_resource_handle() {
    expect_error("<?php fpassthru(1);", "fpassthru() expects resource, got int");
}

/// Verifies flock() produces correct error when called with only STDIN (1 argument, requires 2 or 3).
#[test]
fn test_error_flock_wrong_args() {
    expect_error("<?php flock(STDIN);", "flock() takes 2 or 3 arguments");
}

/// Verifies flock() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_flock_requires_resource_handle() {
    expect_error("<?php flock(1, LOCK_EX);", "flock() expects resource, got int");
}

/// Verifies flock() produces correct error when the operation argument is a string instead of int.
#[test]
fn test_error_flock_rejects_non_int_operation() {
    expect_error(
        r#"<?php flock(STDIN, "exclusive");"#,
        "flock() operation must be int",
    );
}

/// Verifies flock() produces correct error when $would_block is not passed a variable.
#[test]
fn test_error_flock_would_block_requires_variable() {
    expect_error(
        r#"<?php flock(STDIN, LOCK_EX, 0);"#,
        "flock() parameter $would_block must be passed a variable",
    );
}

/// Verifies tmpfile() produces correct error when called with an argument.
#[test]
fn test_error_tmpfile_wrong_args() {
    expect_error("<?php tmpfile(1);", "tmpfile() takes no arguments");
}

/// Verifies tmpfile() produces correct error when called with a non-empty spread argument.
#[test]
fn test_error_tmpfile_rejects_nonempty_static_spread() {
    expect_error("<?php tmpfile(...[1]);", "tmpfile() takes no arguments");
}

/// Verifies a function with string return type annotation produces an error when returning fgetc() which can return false.
#[test]
fn test_error_fgetc_false_return_rejects_string_return_type() {
    expect_error(
        r#"<?php
function read_char(): string {
    return fgetc(STDIN);
}
"#,
        "Function 'read_char' return type expects Str, got Union([Str, Bool])",
    );
}

/// Verifies feof() produces correct error when called with no arguments.
#[test]
fn test_error_feof_wrong_args() {
    expect_error("<?php feof();", "feof() takes exactly 1 argument");
}

/// Verifies fstat() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fstat_requires_resource_handle() {
    expect_error("<?php fstat(-1);", "fstat() expects resource, got int");
}

/// Verifies ftruncate(), fsync(), fflush(), and fdatasync() produce correct errors when called with wrong argument count.
#[test]
fn test_error_stream_modify_builtins_wrong_args() {
    for (source, message) in [
        ("<?php ftruncate(1);", "ftruncate() takes exactly 2 arguments"),
        ("<?php fsync();", "fsync() takes exactly 1 argument"),
        ("<?php fflush();", "fflush() takes exactly 1 argument"),
        ("<?php fdatasync();", "fdatasync() takes exactly 1 argument"),
    ] {
        expect_error(source, message);
    }
}

/// Verifies ftruncate(), fsync(), fflush(), and fdatasync() produce correct errors when passed an int instead of a resource.
#[test]
fn test_error_stream_modify_builtins_require_resource_handle() {
    for (source, message) in [
        ("<?php ftruncate(1, 0);", "ftruncate() expects resource, got int"),
        ("<?php fsync(1);", "fsync() expects resource, got int"),
        ("<?php fflush(1);", "fflush() expects resource, got int"),
        ("<?php fdatasync(1);", "fdatasync() expects resource, got int"),
    ] {
        expect_error(source, message);
    }
}
