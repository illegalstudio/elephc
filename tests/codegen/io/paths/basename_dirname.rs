//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O, paths basename and dirname builtins, including basename simple, path builtins are case insensitive, and path builtins fall back to global namespace.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `basename()` extracts the filename from a simple absolute path.
/// Fixture: `/etc/passwd` → expects `passwd`.
#[test]
fn test_basename_simple() {
    let out = compile_and_run(r#"<?php echo basename("/etc/passwd");"#);
    assert_eq!(out, "passwd");
}

/// Verifies path builtins are callable with uppercase names (case insensitivity).
/// Covers BASENAME, DIRNAME, FNMATCH, PATHINFO with uppercase names vs lowercase PHP names.
/// Fixture: various uppercase builtin calls → validates correct output.
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

/// Verifies path builtins fall back to global namespace when called from within a namespace.
/// Fixture: `namespace App;` with unprefixed basename/dirname/fnmatch/pathinfo → global fallback.
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

/// Verifies `basename()` with no path separator returns the input as-is.
/// Fixture: `"foo"` → expects `"foo"`.
#[test]
fn test_basename_no_separator() {
    let out = compile_and_run(r#"<?php echo basename("foo");"#);
    assert_eq!(out, "foo");
}

/// Verifies `basename()` strips trailing slashes before extracting filename.
/// Fixture: `/usr/local/bin/` → expects `"bin"`.
#[test]
fn test_basename_trailing_slash() {
    let out = compile_and_run(r#"<?php echo basename("/usr/local/bin/");"#);
    assert_eq!(out, "bin");
}

/// Verifies `basename()` with multiple trailing slashes handled correctly.
/// Fixture: `/usr///` → expects `"usr"`. Regression for malformed slash sequences.
#[test]
fn test_basename_multiple_trailing_slashes() {
    let out = compile_and_run(r#"<?php echo basename("/usr///");"#);
    assert_eq!(out, "usr");
}

/// Verifies `basename()` on root-only path returns empty string.
/// Fixture: `"/"` → expects `string(0) ""`. Regression for edge-case root handling.
#[test]
fn test_basename_root_only() {
    let out = compile_and_run(r#"<?php var_dump(basename("/"));"#);
    assert!(out.starts_with("string(0)"), "got: {}", out);
}

/// Verifies `basename()` strips the suffix only when it matches the end of the basename.
/// Fixture: `/var/log/syslog.log` with suffix `.log` → expects `"syslog"`.
#[test]
fn test_basename_with_suffix() {
    let out = compile_and_run(r#"<?php echo basename("/var/log/syslog.log", ".log");"#);
    assert_eq!(out, "syslog");
}

/// Verifies `basename()` returns the full name when suffix does not match.
/// Fixture: `foo.tar.gz` with suffix `.bz2` → expects `"foo.tar.gz"` (no stripping).
#[test]
fn test_basename_suffix_no_match() {
    let out = compile_and_run(r#"<?php echo basename("foo.tar.gz", ".bz2");"#);
    assert_eq!(out, "foo.tar.gz");
}

/// Verifies `basename()` keeps the name when stripping the suffix would empty it.
/// Fixture: `foo` with suffix `foo` → expects `"foo"`. PHP preserves name when result would be empty.
#[test]
fn test_basename_suffix_equals_name() {
    let out = compile_and_run(r#"<?php echo basename("foo", "foo");"#);
    assert_eq!(out, "foo");
}

/// Verifies `dirname()` extracts the directory from a simple absolute path.
/// Fixture: `/etc/passwd` → expects `/etc`.
#[test]
fn test_dirname_simple() {
    let out = compile_and_run(r#"<?php echo dirname("/etc/passwd");"#);
    assert_eq!(out, "/etc");
}

/// Verifies `dirname()` normalizes trailing slashes before processing.
/// Fixture: `/etc/passwd/` → expects `/etc`.
#[test]
fn test_dirname_trailing_slash() {
    let out = compile_and_run(r#"<?php echo dirname("/etc/passwd/");"#);
    assert_eq!(out, "/etc");
}

/// Verifies `dirname()` returns `.` for paths with no separator.
/// Fixture: `foo` → expects `.`.
#[test]
fn test_dirname_no_separator() {
    let out = compile_and_run(r#"<?php echo dirname("foo");"#);
    assert_eq!(out, ".");
}

/// Verifies `dirname()` on a direct child of root returns `/`.
/// Fixture: `/foo` → expects `/`.
#[test]
fn test_dirname_root_child() {
    let out = compile_and_run(r#"<?php echo dirname("/foo");"#);
    assert_eq!(out, "/");
}

/// Verifies `dirname()` on root-only path returns `/`.
/// Fixture: `/` → expects `/`.
#[test]
fn test_dirname_root_only() {
    let out = compile_and_run(r#"<?php echo dirname("/");"#);
    assert_eq!(out, "/");
}

/// Verifies `dirname()` preserves redundant slashes in the resulting path.
/// Fixture: `/usr///local///bin` → expects `/usr///local`. Slash sequence preservation.
#[test]
fn test_dirname_collapses_redundant_slashes() {
    let out = compile_and_run(r#"<?php echo dirname("/usr///local///bin");"#);
    assert_eq!(out, "/usr///local");
}

/// Verifies `dirname()` on `.` returns `.`.
/// Fixture: `.` → expects `.`.
#[test]
fn test_dirname_dot() {
    let out = compile_and_run(r#"<?php echo dirname(".");"#);
    assert_eq!(out, ".");
}

/// Verifies `dirname()` with explicit `$level` parameter ascends multiple directory levels.
/// Fixture: `/usr/local/bin/tool` with level 2 → expects `/usr/local`.
#[test]
fn test_dirname_levels() {
    let out = compile_and_run(r#"<?php echo dirname("/usr/local/bin/tool", 2);"#);
    assert_eq!(out, "/usr/local");
}

/// Verifies `dirname()` with level parameter stops at root (does not go past root).
/// Fixture: `/usr` with level 3 → expects `/`. Prevents underflow beyond root.
#[test]
fn test_dirname_levels_past_root_stays_root() {
    let out = compile_and_run(r#"<?php echo dirname("/usr", 3);"#);
    assert_eq!(out, "/");
}
