//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O, paths realpath and pathinfo builtins, including realpath existing file, realpath strips redundant segments, and realpath missing returns false.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use super::*;

#[test]
fn test_realpath_existing_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("anchor.txt", "");
$resolved = realpath("anchor.txt");
echo $resolved !== false ? "ok" : "empty";
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_realpath_strips_redundant_segments() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("anchor.txt", "");
$resolved = realpath("./anchor.txt");
$direct = realpath("anchor.txt");
echo $resolved === $direct ? "match" : "differ";
"#,
    );
    assert_eq!(out, "match");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_realpath_missing_returns_false() {
    let out = compile_and_run(
        r#"<?php
$value = realpath("/definitely/does/not/exist/anywhere/12345");
echo $value === false ? "false" : "string";
"#,
    );
    assert_eq!(out, "false");
}

#[test]
fn test_realpath_direct_echo_does_not_crash() {
    // Regression: realpath used to return Union(Str, Bool) without boxing the
    // result as a Mixed cell, so directly echoing the value crashed because
    // the codegen pipeline expects boxed values for union-typed expressions.
    // This test verifies that the success path can be echoed directly.
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("anchor.txt", "");
$resolved = realpath("anchor.txt");
echo $resolved !== false ? "ok" : "fail";
echo "|";
if ($resolved !== false) {
    echo $resolved;
}
"#,
    );
    assert!(out.starts_with("ok|"), "got: {}", out);
    assert!(out.ends_with("anchor.txt"), "got: {}", out);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_pathinfo_dirname() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("/var/log/syslog.log", PATHINFO_DIRNAME);"#,
    );
    assert_eq!(out, "/var/log");
}

#[test]
fn test_pathinfo_basename() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("/var/log/syslog.log", PATHINFO_BASENAME);"#,
    );
    assert_eq!(out, "syslog.log");
}

#[test]
fn test_pathinfo_extension() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("/var/log/syslog.log", PATHINFO_EXTENSION);"#,
    );
    assert_eq!(out, "log");
}

#[test]
fn test_pathinfo_extension_multiple_dots() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("archive.tar.gz", PATHINFO_EXTENSION);"#,
    );
    assert_eq!(out, "gz");
}

#[test]
fn test_pathinfo_extension_no_dot() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("/etc/hosts", PATHINFO_EXTENSION);"#,
    );
    assert_eq!(out, "");
}

#[test]
fn test_pathinfo_extension_dotfile() {
    let out = compile_and_run(
        r#"<?php echo pathinfo(".bashrc", PATHINFO_EXTENSION);"#,
    );
    assert_eq!(out, "bashrc");
}

#[test]
fn test_pathinfo_filename() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("/var/log/syslog.log", PATHINFO_FILENAME);"#,
    );
    assert_eq!(out, "syslog");
}

#[test]
fn test_pathinfo_filename_multiple_dots() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("archive.tar.gz", PATHINFO_FILENAME);"#,
    );
    assert_eq!(out, "archive.tar");
}

#[test]
fn test_pathinfo_filename_no_dot() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("/etc/hosts", PATHINFO_FILENAME);"#,
    );
    assert_eq!(out, "hosts");
}

#[test]
fn test_pathinfo_filename_dotfile() {
    let out = compile_and_run(
        r#"<?php echo pathinfo(".bashrc", PATHINFO_FILENAME);"#,
    );
    assert_eq!(out, "");
}

#[test]
fn test_pathinfo_array_full() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("/var/log/syslog.log");
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, "/var/log|syslog.log|log|syslog");
}

#[test]
fn test_pathinfo_array_with_pathinfo_all_flag() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("/var/log/syslog.log", PATHINFO_ALL);
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, "/var/log|syslog.log|log|syslog");
}

#[test]
fn test_pathinfo_array_with_literal_all_flag() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("foo.txt", 15);
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, ".|foo.txt|txt|foo");
}

#[test]
fn test_pathinfo_array_no_extension_omits_key() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("/etc/hosts");
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["filename"] . "|";
echo array_key_exists("extension", $info) ? "yes" : "no";
"#,
    );
    assert_eq!(out, "/etc|hosts|hosts|no");
}

#[test]
fn test_pathinfo_array_dotfile_includes_extension() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo(".bashrc");
echo $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"] . "|";
echo array_key_exists("extension", $info) ? "yes" : "no";
"#,
    );
    assert_eq!(out, ".bashrc|bashrc||yes");
}

#[test]
fn test_pathinfo_array_multiple_dots() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("archive.tar.gz");
echo $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, "archive.tar.gz|gz|archive.tar");
}

#[test]
fn test_pathinfo_array_relative_path() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("foo.txt");
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, ".|foo.txt|txt|foo");
}

#[test]
fn test_pathinfo_array_empty_path_omits_dirname() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("");
echo (array_key_exists("dirname", $info) ? "yes" : "no") . "|";
echo $info["basename"] . "|" . $info["filename"] . "|";
echo pathinfo("", PATHINFO_DIRNAME);
"#,
    );
    assert_eq!(out, "no|||");
}

#[test]
fn test_pathinfo_array_trailing_dot_keeps_empty_extension_key() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("file.");
echo $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"] . "|";
echo array_key_exists("extension", $info) ? "yes" : "no";
"#,
    );
    assert_eq!(out, "file.||file|yes");
}

#[test]
fn test_pathinfo_bitmask_component_priority() {
    let out = compile_and_run(
        r#"<?php
echo pathinfo("/a/b.php", PATHINFO_DIRNAME | PATHINFO_EXTENSION) . "|";
echo pathinfo("/a/b.php", PATHINFO_BASENAME | PATHINFO_FILENAME) . "|";
echo pathinfo("/a/b.php", PATHINFO_EXTENSION | PATHINFO_FILENAME) . "|";
echo pathinfo("/a/b.php", 0);
"#,
    );
    assert_eq!(out, "/a|b.php|php|");
}

#[test]
fn test_pathinfo_all_bitmask_expression_returns_array() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("foo.txt", PATHINFO_DIRNAME | PATHINFO_BASENAME | PATHINFO_EXTENSION | PATHINFO_FILENAME);
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, ".|foo.txt|txt|foo");
}

#[test]
fn test_pathinfo_dynamic_component_flag_returns_string() {
    let out = compile_and_run(
        r#"<?php
$flag = PATHINFO_EXTENSION;
echo pathinfo("archive.tar.gz", $flag);
"#,
    );
    assert_eq!(out, "gz");
}

#[test]
fn test_pathinfo_dynamic_all_flag_returns_array() {
    let out = compile_and_run(
        r#"<?php
$flag = PATHINFO_ALL;
$info = pathinfo("/var/log/syslog.log", $flag);
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, "/var/log|syslog.log|log|syslog");
}

#[test]
fn test_pathinfo_dynamic_all_bitmask_returns_array() {
    let out = compile_and_run(
        r#"<?php
$flag = PATHINFO_DIRNAME | PATHINFO_BASENAME | PATHINFO_EXTENSION | PATHINFO_FILENAME;
$info = pathinfo("foo.txt", $flag);
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, ".|foo.txt|txt|foo");
}

#[test]
fn test_pathinfo_dynamic_runtime_shape_can_change() {
    let out = compile_and_run(
        r#"<?php
$flag = PATHINFO_EXTENSION;
$component = pathinfo("foo.txt", $flag);
echo $component . "|";
$flag = PATHINFO_ALL;
$info = pathinfo("foo.txt", $flag);
echo $info["basename"] . "|" . $info["extension"];
"#,
    );
    assert_eq!(out, "txt|foo.txt|txt");
}

#[test]
fn test_pathinfo_dynamic_all_inside_function_returns_array() {
    let out = compile_and_run(
        r#"<?php
function dynamic_basename(int $flag) {
    $info = pathinfo("foo.txt", $flag);
    return $info["basename"];
}
echo dynamic_basename(PATHINFO_ALL);
"#,
    );
    assert_eq!(out, "foo.txt");
}

#[test]
fn test_pathinfo_dynamic_zero_flag_returns_empty_string() {
    let out = compile_and_run(
        r#"<?php
$flag = 0;
echo "[" . pathinfo("foo.txt", $flag) . "]";
"#,
    );
    assert_eq!(out, "[]");
}

#[test]
fn test_dirname_dynamic_invalid_levels_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
$levels = 0;
echo dirname("/usr/local/bin", $levels);
"#,
    );
    assert!(
        err.contains("dirname(): Argument #2 ($levels) must be greater than or equal to 1"),
        "unexpected stderr: {}",
        err
    );
}
