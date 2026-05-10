//! Purpose:
//! Integration or regression tests for diagnostic coverage of I/O builtin filesystem, including file get contents wrong args, file get contents false return rejects string return type, and file put contents wrong args.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

#[test]
fn test_error_file_get_contents_wrong_args() {
    expect_error(
        "<?php file_get_contents();",
        "file_get_contents() takes exactly 1 argument",
    );
}

#[test]
fn test_error_file_get_contents_false_return_rejects_string_return_type() {
    expect_error(
        r#"<?php
function read_file(): string {
    return file_get_contents("missing.txt");
}
"#,
        "Function 'read_file' return type expects Str, got Union([Str, Bool])",
    );
}

#[test]
fn test_error_file_put_contents_wrong_args() {
    expect_error(
        r#"<?php file_put_contents("x");"#,
        "file_put_contents() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_file_exists_wrong_args() {
    expect_error(
        "<?php file_exists();",
        "file_exists() takes exactly 1 argument",
    );
}

#[test]
fn test_error_mkdir_wrong_args() {
    expect_error("<?php mkdir();", "mkdir() takes exactly 1 argument");
}

#[test]
fn test_error_copy_wrong_args() {
    expect_error(r#"<?php copy("x");"#, "copy() takes exactly 2 arguments");
}

#[test]
fn test_error_rename_wrong_args() {
    expect_error(
        r#"<?php rename("x");"#,
        "rename() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_getcwd_wrong_args() {
    expect_error("<?php getcwd(1);", "getcwd() takes no arguments");
}

#[test]
fn test_error_scandir_wrong_args() {
    expect_error("<?php scandir();", "scandir() takes exactly 1 argument");
}

#[test]
fn test_error_tempnam_wrong_args() {
    expect_error(
        r#"<?php tempnam("x");"#,
        "tempnam() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_is_file_wrong_args() {
    expect_error("<?php is_file();", "is_file() takes exactly 1 argument");
}

#[test]
fn test_error_is_dir_wrong_args() {
    expect_error("<?php is_dir();", "is_dir() takes exactly 1 argument");
}

#[test]
fn test_error_is_readable_wrong_args() {
    expect_error(
        "<?php is_readable();",
        "is_readable() takes exactly 1 argument",
    );
}

#[test]
fn test_error_is_writable_wrong_args() {
    expect_error(
        "<?php is_writable();",
        "is_writable() takes exactly 1 argument",
    );
}

#[test]
fn test_error_filesize_wrong_args() {
    expect_error("<?php filesize();", "filesize() takes exactly 1 argument");
}

#[test]
fn test_error_filemtime_wrong_args() {
    expect_error("<?php filemtime();", "filemtime() takes exactly 1 argument");
}

#[test]
fn test_error_extended_stat_builtins_wrong_args() {
    for (source, message) in [
        ("<?php fileatime();", "fileatime() takes exactly 1 argument"),
        ("<?php filectime();", "filectime() takes exactly 1 argument"),
        ("<?php fileperms();", "fileperms() takes exactly 1 argument"),
        ("<?php fileowner();", "fileowner() takes exactly 1 argument"),
        ("<?php filegroup();", "filegroup() takes exactly 1 argument"),
        ("<?php fileinode();", "fileinode() takes exactly 1 argument"),
        ("<?php filetype();", "filetype() takes exactly 1 argument"),
        ("<?php is_executable();", "is_executable() takes exactly 1 argument"),
        ("<?php is_link();", "is_link() takes exactly 1 argument"),
        ("<?php is_writeable();", "is_writeable() takes exactly 1 argument"),
        ("<?php stat();", "stat() takes exactly 1 argument"),
        ("<?php lstat();", "lstat() takes exactly 1 argument"),
        ("<?php fstat();", "fstat() takes exactly 1 argument"),
        (
            "<?php clearstatcache(false, \"a\", \"extra\");",
            "clearstatcache() takes at most 2 arguments",
        ),
    ] {
        expect_error(source, message);
    }
}

#[test]
fn test_error_unlink_wrong_args() {
    expect_error("<?php unlink();", "unlink() takes exactly 1 argument");
}

#[test]
fn test_error_rmdir_wrong_args() {
    expect_error("<?php rmdir();", "rmdir() takes exactly 1 argument");
}

#[test]
fn test_error_chdir_wrong_args() {
    expect_error("<?php chdir();", "chdir() takes exactly 1 argument");
}

#[test]
fn test_error_glob_wrong_args() {
    expect_error("<?php glob();", "glob() takes exactly 1 argument");
}

#[test]
fn test_error_sys_get_temp_dir_wrong_args() {
    expect_error(
        "<?php sys_get_temp_dir(1);",
        "sys_get_temp_dir() takes no arguments",
    );
}

