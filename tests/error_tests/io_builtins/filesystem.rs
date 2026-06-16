//! Purpose:
//! Integration or regression tests for diagnostic coverage of I/O builtin filesystem, including file get contents wrong args, file get contents false return rejects string return type, and file put contents wrong args.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies `file_get_contents()` rejects zero arguments with arity error.
#[test]
fn test_error_file_get_contents_wrong_args() {
    expect_error(
        "<?php file_get_contents();",
        "file_get_contents() takes exactly 1 argument",
    );
}

/// Verifies `file_get_contents()` returning `false` is incompatible with declared `string` return type.
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

/// Verifies `hash_file()` rejects too few arguments with arity error.
#[test]
fn test_error_hash_file_wrong_args() {
    expect_error(
        r#"<?php hash_file("sha256");"#,
        "hash_file() takes 2 or 3 arguments",
    );
}

/// Verifies `readfile()` rejects zero arguments with arity error.
#[test]
fn test_error_readfile_wrong_args() {
    expect_error("<?php readfile();", "readfile() takes exactly 1 argument");
}

/// Verifies `readfile()` returning `false` is incompatible with declared `int` return type.
#[test]
fn test_error_readfile_false_return_rejects_int_return_type() {
    expect_error(
        r#"<?php
function dump_file(): int {
    return readfile("missing.txt");
}
"#,
        "Function 'dump_file' return type expects Int, got Union([Int, Bool])",
    );
}

/// Verifies `file_put_contents()` rejects one argument (requires 2) with arity error.
#[test]
fn test_error_file_put_contents_wrong_args() {
    expect_error(
        r#"<?php file_put_contents("x");"#,
        "file_put_contents() takes exactly 2 arguments",
    );
}

/// Verifies `file_exists()` rejects zero arguments with arity error.
#[test]
fn test_error_file_exists_wrong_args() {
    expect_error(
        "<?php file_exists();",
        "file_exists() takes exactly 1 argument",
    );
}

/// Verifies `mkdir()` rejects zero arguments with arity error.
#[test]
fn test_error_mkdir_wrong_args() {
    expect_error("<?php mkdir();", "mkdir() takes exactly 1 argument");
}

/// Verifies `copy()` rejects one argument (requires 2) with arity error.
#[test]
fn test_error_copy_wrong_args() {
    expect_error(r#"<?php copy("x");"#, "copy() takes exactly 2 arguments");
}

/// Verifies `link()` rejects one argument (requires 2) with arity error.
#[test]
fn test_error_link_wrong_args() {
    expect_error(r#"<?php link("x");"#, "link() takes exactly 2 arguments");
}

/// Verifies `symlink()` rejects one argument (requires 2) with arity error.
#[test]
fn test_error_symlink_wrong_args() {
    expect_error(
        r#"<?php symlink("target");"#,
        "symlink() takes exactly 2 arguments",
    );
}

/// Verifies `readlink()` rejects zero arguments with arity error.
#[test]
fn test_error_readlink_wrong_args() {
    expect_error("<?php readlink();", "readlink() takes exactly 1 argument");
}

/// Verifies `linkinfo()` rejects zero arguments with arity error.
#[test]
fn test_error_linkinfo_wrong_args() {
    expect_error("<?php linkinfo();", "linkinfo() takes exactly 1 argument");
}

/// Verifies `rename()` rejects one argument (requires 2) with arity error.
#[test]
fn test_error_rename_wrong_args() {
    expect_error(
        r#"<?php rename("x");"#,
        "rename() takes exactly 2 arguments",
    );
}

/// Verifies `getcwd()` rejects arguments with arity error.
#[test]
fn test_error_getcwd_wrong_args() {
    expect_error("<?php getcwd(1);", "getcwd() takes no arguments");
}

/// Verifies `scandir()` rejects zero arguments with arity error.
#[test]
fn test_error_scandir_wrong_args() {
    expect_error("<?php scandir();", "scandir() takes exactly 1 argument");
}

/// Verifies `tempnam()` rejects one argument (requires 2) with arity error.
#[test]
fn test_error_tempnam_wrong_args() {
    expect_error(
        r#"<?php tempnam("x");"#,
        "tempnam() takes exactly 2 arguments",
    );
}

/// Verifies `is_file()` rejects zero arguments with arity error.
#[test]
fn test_error_is_file_wrong_args() {
    expect_error("<?php is_file();", "is_file() takes exactly 1 argument");
}

/// Verifies `is_dir()` rejects zero arguments with arity error.
#[test]
fn test_error_is_dir_wrong_args() {
    expect_error("<?php is_dir();", "is_dir() takes exactly 1 argument");
}

/// Verifies `is_readable()` rejects zero arguments with arity error.
#[test]
fn test_error_is_readable_wrong_args() {
    expect_error(
        "<?php is_readable();",
        "is_readable() takes exactly 1 argument",
    );
}

/// Verifies `is_writable()` rejects zero arguments with arity error.
#[test]
fn test_error_is_writable_wrong_args() {
    expect_error(
        "<?php is_writable();",
        "is_writable() takes exactly 1 argument",
    );
}

/// Verifies `filesize()` rejects zero arguments with arity error.
#[test]
fn test_error_filesize_wrong_args() {
    expect_error("<?php filesize();", "filesize() takes exactly 1 argument");
}

/// Verifies `filemtime()` rejects zero arguments with arity error.
#[test]
fn test_error_filemtime_wrong_args() {
    expect_error("<?php filemtime();", "filemtime() takes exactly 1 argument");
}

/// Verifies arity errors for extended stat builtins: fileatime, filectime, fileperms,
/// fileowner, filegroup, fileinode, filetype, is_executable, is_link, is_writeable,
/// stat, lstat, fstat, and clearstatcache with too many args.
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

/// Verifies `unlink()` rejects zero arguments with arity error.
#[test]
fn test_error_unlink_wrong_args() {
    expect_error("<?php unlink();", "unlink() takes exactly 1 argument");
}

/// Verifies `rmdir()` rejects zero arguments with arity error.
#[test]
fn test_error_rmdir_wrong_args() {
    expect_error("<?php rmdir();", "rmdir() takes exactly 1 argument");
}

/// Verifies `chdir()` rejects zero arguments with arity error.
#[test]
fn test_error_chdir_wrong_args() {
    expect_error("<?php chdir();", "chdir() takes exactly 1 argument");
}

/// Verifies `glob()` rejects zero arguments with arity error.
#[test]
fn test_error_glob_wrong_args() {
    expect_error("<?php glob();", "glob() takes exactly 1 argument");
}

/// Verifies `sys_get_temp_dir()` rejects arguments with arity error.
#[test]
fn test_error_sys_get_temp_dir_wrong_args() {
    expect_error(
        "<?php sys_get_temp_dir(1);",
        "sys_get_temp_dir() takes no arguments",
    );
}

/// Verifies the invalid-call diagnostic for error disk free space wrong args.
#[test]
fn test_error_disk_free_space_wrong_args() {
    expect_error(
        "<?php disk_free_space();",
        "disk_free_space() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error disk total space wrong args.
#[test]
fn test_error_disk_total_space_wrong_args() {
    expect_error(
        "<?php disk_total_space();",
        "disk_total_space() takes exactly 1 argument",
    );
}
