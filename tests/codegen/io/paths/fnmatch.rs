//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O, paths fnmatch path matching, including fnmatch literal match, fnmatch accepts zero flags argument, and fnmatch pathname flag keeps slash special.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

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
fn test_fnmatch_pathname_flag_keeps_slash_special() {
    let out = compile_and_run(
        r#"<?php
echo fnmatch("*.txt", "dir/file.txt") ? "y" : "n";
echo "|";
echo fnmatch("*.txt", "dir/file.txt", FNM_PATHNAME) ? "y" : "n";
"#,
    );
    assert_eq!(out, "y|n");
}

#[test]
fn test_fnmatch_period_flag_blocks_leading_dot_wildcard() {
    let out = compile_and_run(
        r#"<?php
echo fnmatch("*", ".env") ? "y" : "n";
echo "|";
echo fnmatch("*", ".env", FNM_PERIOD) ? "y" : "n";
"#,
    );
    assert_eq!(out, "y|n");
}

#[test]
fn test_fnmatch_casefold_flag_matches_case_insensitively() {
    let out = compile_and_run(
        r#"<?php
echo fnmatch("*.TXT", "file.txt") ? "y" : "n";
echo "|";
echo fnmatch("*.TXT", "file.txt", FNM_CASEFOLD) ? "y" : "n";
"#,
    );
    assert_eq!(out, "n|y");
}

#[test]
fn test_fnmatch_noescape_flag_treats_backslash_as_literal() {
    let out = compile_and_run(
        r#"<?php
echo fnmatch('a\\*b', 'a*b') ? "y" : "n";
echo "|";
echo fnmatch('a\\*b', 'a*b', FNM_NOESCAPE) ? "y" : "n";
echo "|";
echo fnmatch('a\\*b', 'a\\xxb', FNM_NOESCAPE) ? "y" : "n";
"#,
    );
    assert_eq!(out, "y|n|y");
}

#[test]
fn test_fnmatch_combined_runtime_flags() {
    let out = compile_and_run(
        r#"<?php
$flags = FNM_PATHNAME | FNM_CASEFOLD;
echo fnmatch("*.TXT", "dir/file.txt", $flags) ? "y" : "n";
echo "|";
echo fnmatch("dir/*.TXT", "dir/file.txt", $flags) ? "y" : "n";
"#,
    );
    assert_eq!(out, "n|y");
}

#[test]
fn test_fnmatch_combined_constant_flags() {
    let out = compile_and_run(
        r#"<?php
echo fnmatch("*/*.txt", ".hidden/file.txt", FNM_PATHNAME | FNM_PERIOD) ? "y" : "n";
echo "|";
echo fnmatch("*/*.txt", "visible/file.txt", FNM_PATHNAME | FNM_PERIOD) ? "y" : "n";
"#,
    );
    assert_eq!(out, "n|y");
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
