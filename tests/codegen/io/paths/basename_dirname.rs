//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O, paths basename and dirname builtins, including basename simple, path builtins are case insensitive, and path builtins fall back to global namespace.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_basename_simple() {
    let out = compile_and_run(r#"<?php echo basename("/etc/passwd");"#);
    assert_eq!(out, "passwd");
}

#[test]
fn test_path_builtins_are_case_insensitive() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("anchor.txt", "");
echo BASENAME("/etc/passwd") . "|";
echo DIRNAME("/etc/passwd") . "|";
echo FNMATCH("*.txt", "report.txt") ? "match" : "miss";
echo "|";
echo PATHINFO("/var/log/syslog.log", PATHINFO_EXTENSION) . "|";
$resolved = REALPATH("anchor.txt");
echo $resolved !== false ? "real" : "false";
"#,
    );
    assert_eq!(out, "passwd|/etc|match|log|real");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_path_builtins_fall_back_to_global_namespace() {
    let out = compile_and_run(
        r#"<?php
namespace App;
echo basename("/etc/passwd") . "|";
echo dirname("/etc/passwd") . "|";
echo fnmatch("*.txt", "report.txt") ? "match" : "miss";
echo "|";
echo pathinfo("/var/log/syslog.log", PATHINFO_FILENAME);
"#,
    );
    assert_eq!(out, "passwd|/etc|match|syslog");
}

#[test]
fn test_basename_no_separator() {
    let out = compile_and_run(r#"<?php echo basename("foo");"#);
    assert_eq!(out, "foo");
}

#[test]
fn test_basename_trailing_slash() {
    let out = compile_and_run(r#"<?php echo basename("/usr/local/bin/");"#);
    assert_eq!(out, "bin");
}

#[test]
fn test_basename_multiple_trailing_slashes() {
    let out = compile_and_run(r#"<?php echo basename("/usr///");"#);
    assert_eq!(out, "usr");
}

#[test]
fn test_basename_root_only() {
    let out = compile_and_run(r#"<?php var_dump(basename("/"));"#);
    assert!(out.starts_with("string(0)"), "got: {}", out);
}

#[test]
fn test_basename_with_suffix() {
    let out = compile_and_run(r#"<?php echo basename("/var/log/syslog.log", ".log");"#);
    assert_eq!(out, "syslog");
}

#[test]
fn test_basename_suffix_no_match() {
    let out = compile_and_run(r#"<?php echo basename("foo.tar.gz", ".bz2");"#);
    assert_eq!(out, "foo.tar.gz");
}

#[test]
fn test_basename_suffix_equals_name() {
    // PHP keeps the basename when stripping the suffix would empty it.
    let out = compile_and_run(r#"<?php echo basename("foo", "foo");"#);
    assert_eq!(out, "foo");
}

#[test]
fn test_dirname_simple() {
    let out = compile_and_run(r#"<?php echo dirname("/etc/passwd");"#);
    assert_eq!(out, "/etc");
}

#[test]
fn test_dirname_trailing_slash() {
    let out = compile_and_run(r#"<?php echo dirname("/etc/passwd/");"#);
    assert_eq!(out, "/etc");
}

#[test]
fn test_dirname_no_separator() {
    let out = compile_and_run(r#"<?php echo dirname("foo");"#);
    assert_eq!(out, ".");
}

#[test]
fn test_dirname_root_child() {
    let out = compile_and_run(r#"<?php echo dirname("/foo");"#);
    assert_eq!(out, "/");
}

#[test]
fn test_dirname_root_only() {
    let out = compile_and_run(r#"<?php echo dirname("/");"#);
    assert_eq!(out, "/");
}

#[test]
fn test_dirname_collapses_redundant_slashes() {
    let out = compile_and_run(r#"<?php echo dirname("/usr///local///bin");"#);
    assert_eq!(out, "/usr///local");
}

#[test]
fn test_dirname_dot() {
    let out = compile_and_run(r#"<?php echo dirname(".");"#);
    assert_eq!(out, ".");
}

#[test]
fn test_dirname_levels() {
    let out = compile_and_run(r#"<?php echo dirname("/usr/local/bin/tool", 2);"#);
    assert_eq!(out, "/usr/local");
}

#[test]
fn test_dirname_levels_past_root_stays_root() {
    let out = compile_and_run(r#"<?php echo dirname("/usr", 3);"#);
    assert_eq!(out, "/");
}
