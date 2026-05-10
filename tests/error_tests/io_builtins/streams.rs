//! Purpose:
//! Integration or regression tests for diagnostic coverage of I/O builtin streams, including var dump wrong args, print r wrong args, and fopen wrong args.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

#[test]
fn test_error_var_dump_wrong_args() {
    expect_error("<?php var_dump();", "var_dump() takes exactly 1 argument");
}

#[test]
fn test_error_print_r_wrong_args() {
    expect_error("<?php print_r();", "print_r() takes exactly 1 argument");
}

#[test]
fn test_error_fopen_wrong_args() {
    expect_error(
        r#"<?php fopen("file");"#,
        "fopen() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_fclose_wrong_args() {
    expect_error("<?php fclose();", "fclose() takes exactly 1 argument");
}

#[test]
fn test_error_fclose_requires_resource_handle() {
    expect_error("<?php fclose(1);", "fclose() expects resource, got int");
}

#[test]
fn test_error_fread_wrong_args() {
    expect_error("<?php fread(1);", "fread() takes exactly 2 arguments");
}

#[test]
fn test_error_fread_requires_resource_handle() {
    expect_error("<?php fread(1, 1);", "fread() expects resource, got int");
}

#[test]
fn test_error_fwrite_wrong_args() {
    expect_error("<?php fwrite(1);", "fwrite() takes exactly 2 arguments");
}

#[test]
fn test_error_fwrite_requires_resource_handle() {
    expect_error(
        r#"<?php fwrite(1, "x");"#,
        "fwrite() expects resource, got int",
    );
}

#[test]
fn test_error_fgets_wrong_args() {
    expect_error("<?php fgets();", "fgets() takes exactly 1 argument");
}

#[test]
fn test_error_fgets_requires_resource_handle() {
    expect_error("<?php fgets(1);", "fgets() expects resource, got int");
}

#[test]
fn test_error_feof_wrong_args() {
    expect_error("<?php feof();", "feof() takes exactly 1 argument");
}

#[test]
fn test_error_fstat_requires_resource_handle() {
    expect_error("<?php fstat(-1);", "fstat() expects resource, got int");
}

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

