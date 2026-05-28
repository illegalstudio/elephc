//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O, paths fnmatch path matching, including fnmatch literal match, fnmatch accepts zero flags argument, and fnmatch pathname flag keeps slash special.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies fnmatch returns true when pattern and filename are identical literals.
#[test]
fn test_fnmatch_literal_match() {
    let out = compile_and_run(r#"<?php echo fnmatch("file.txt", "file.txt") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies fnmatch accepts zero as the optional flags argument and still matches correctly.
#[test]
fn test_fnmatch_accepts_zero_flags_argument() {
    let out = compile_and_run(r#"<?php echo fnmatch("*.txt", "report.txt", 0) ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies FNM_PATHNAME flag causes slashes in the filename to be matched literally,
/// so "*.txt" does not match "dir/file.txt" but does match "file.txt".
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

/// Verifies FNM_PERIOD flag causes a leading dot in filename to not match a wildcard pattern,
/// so "*" without the flag matches ".env" but with FNM_PERIOD it does not.
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

/// Verifies FNM_CASEFOLD flag enables case-insensitive matching,
/// so "*.TXT" matches "file.txt" only when the flag is set.
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

/// Verifies FNM_NOESCAPE flag treats backslashes as literal characters rather than escape tokens,
/// so "a\\*b" matches "a*b" without the flag but not with the flag set.
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

/// Verifies fnmatch works when flags are computed at runtime via bitwise-OR of constants.
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

/// Verifies fnmatch works when multiple constants are combined via | at the call site.
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

/// Verifies fnmatch returns false when literal pattern does not match the filename.
#[test]
fn test_fnmatch_literal_mismatch() {
    let out = compile_and_run(r#"<?php echo fnmatch("file.txt", "file.png") ? "y" : "n";"#);
    assert_eq!(out, "n");
}

/// Verifies star at end of pattern matches any suffix.
#[test]
fn test_fnmatch_star_suffix() {
    let out = compile_and_run(r#"<?php echo fnmatch("*.txt", "report.txt") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies star at start of pattern matches any prefix.
#[test]
fn test_fnmatch_star_prefix() {
    let out = compile_and_run(r#"<?php echo fnmatch("doc-*", "doc-2026") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies star in the middle of a pattern matches any substring.
#[test]
fn test_fnmatch_star_middle() {
    let out = compile_and_run(r#"<?php echo fnmatch("a*z", "abcz") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies star matches an empty string when no characters remain to consume.
#[test]
fn test_fnmatch_star_empty() {
    let out = compile_and_run(r#"<?php echo fnmatch("a*z", "az") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies a pattern consisting of only a star matches any filename.
#[test]
fn test_fnmatch_star_only() {
    let out = compile_and_run(r#"<?php echo fnmatch("*", "anything") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies ? matches exactly one arbitrary character.
#[test]
fn test_fnmatch_question_mark() {
    let out = compile_and_run(r#"<?php echo fnmatch("?at", "cat") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies ? does not match when the filename has more than one character to cover.
#[test]
fn test_fnmatch_question_mark_mismatch_length() {
    let out = compile_and_run(r#"<?php echo fnmatch("?at", "chat") ? "y" : "n";"#);
    assert_eq!(out, "n");
}

/// Verifies character class matches any single character listed inside the brackets.
#[test]
fn test_fnmatch_class_set() {
    let out = compile_and_run(r#"<?php echo fnmatch("[abc]ello", "bello") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies character class with a range "a-z" matches any character in that range.
#[test]
fn test_fnmatch_class_range() {
    let out = compile_and_run(r#"<?php echo fnmatch("[a-z]oo", "foo") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies character class range "a-z" does not match uppercase letters outside the range.
#[test]
fn test_fnmatch_class_range_no_match() {
    let out = compile_and_run(r#"<?php echo fnmatch("[a-z]oo", "Foo") ? "y" : "n";"#);
    assert_eq!(out, "n");
}

/// Verifies negated character class [!abc] matches any character not listed.
#[test]
fn test_fnmatch_class_negated() {
    let out = compile_and_run(r#"<?php echo fnmatch("[!abc]oo", "doo") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies negated character class [!abc] does not match a character that is listed.
#[test]
fn test_fnmatch_class_negated_no_match() {
    let out = compile_and_run(r#"<?php echo fnmatch("[!abc]oo", "boo") ? "y" : "n";"#);
    assert_eq!(out, "n");
}

/// Verifies backslash in pattern escapes the following character, treating it as a literal.
#[test]
fn test_fnmatch_escape() {
    let out = compile_and_run(r#"<?php echo fnmatch("a\\*b", "a*b") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies fnmatch returns true when both pattern and filename are empty strings.
#[test]
fn test_fnmatch_empty_pattern_empty_filename() {
    let out = compile_and_run(r#"<?php echo fnmatch("", "") ? "y" : "n";"#);
    assert_eq!(out, "y");
}

/// Verifies star matches an empty string in the middle of a filename.
#[test]
fn test_fnmatch_star_matches_empty_string() {
    let out = compile_and_run(r#"<?php echo fnmatch("*", "") ? "y" : "n";"#);
    assert_eq!(out, "y");
}
