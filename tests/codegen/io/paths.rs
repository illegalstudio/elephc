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

#[test]
fn test_fnmatch_literal_match() {
    let out = compile_and_run(r#"<?php echo fnmatch("file.txt", "file.txt") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_fnmatch_accepts_zero_flags_argument() {
    let out = compile_and_run(r#"<?php echo fnmatch("*.txt", "report.txt", 0) ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_fnmatch_literal_mismatch() {
    let out = compile_and_run(r#"<?php echo fnmatch("file.txt", "file.png") ? "y" : "n";"#);
    assert_eq!(out, "n");
}

#[test]
fn test_fnmatch_star_suffix() {
    let out = compile_and_run(r#"<?php echo fnmatch("*.txt", "report.txt") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_fnmatch_star_prefix() {
    let out = compile_and_run(r#"<?php echo fnmatch("doc-*", "doc-2026") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_fnmatch_star_middle() {
    let out = compile_and_run(r#"<?php echo fnmatch("a*z", "abcz") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_fnmatch_star_empty() {
    let out = compile_and_run(r#"<?php echo fnmatch("a*z", "az") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_fnmatch_star_only() {
    let out = compile_and_run(r#"<?php echo fnmatch("*", "anything") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_fnmatch_question_mark() {
    let out = compile_and_run(r#"<?php echo fnmatch("?at", "cat") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_fnmatch_question_mark_mismatch_length() {
    let out = compile_and_run(r#"<?php echo fnmatch("?at", "chat") ? "y" : "n";"#);
    assert_eq!(out, "n");
}

#[test]
fn test_fnmatch_class_set() {
    let out = compile_and_run(r#"<?php echo fnmatch("[abc]ello", "bello") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_fnmatch_class_range() {
    let out = compile_and_run(r#"<?php echo fnmatch("[a-z]oo", "foo") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_fnmatch_class_range_no_match() {
    let out = compile_and_run(r#"<?php echo fnmatch("[a-z]oo", "Foo") ? "y" : "n";"#);
    assert_eq!(out, "n");
}

#[test]
fn test_fnmatch_class_negated() {
    let out = compile_and_run(r#"<?php echo fnmatch("[!abc]oo", "doo") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_fnmatch_class_negated_no_match() {
    let out = compile_and_run(r#"<?php echo fnmatch("[!abc]oo", "boo") ? "y" : "n";"#);
    assert_eq!(out, "n");
}

#[test]
fn test_fnmatch_escape() {
    let out = compile_and_run(r#"<?php echo fnmatch("a\\*b", "a*b") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_fnmatch_empty_pattern_empty_filename() {
    let out = compile_and_run(r#"<?php echo fnmatch("", "") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

#[test]
fn test_fnmatch_star_matches_empty_string() {
    let out = compile_and_run(r#"<?php echo fnmatch("*", "") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

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
