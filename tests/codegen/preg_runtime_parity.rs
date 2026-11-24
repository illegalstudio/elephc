//! Purpose:
//! End-to-end regression tests for the compiled `preg_*` runtime against PHP 8.5:
//! delimiter parsing and malformed patterns (#2134), the `A`, `D`, `n`, `J` and `r`
//! modifiers (#2135), zero-length matches in the replace and split loops (#2006), an
//! empty runtime subject in `preg_replace()` (#2005), and the `preg_split()` limit under
//! `PREG_SPLIT_NO_EMPTY` (#2394).
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext preg_runtime_parity::` through the PCRE test module.
//!
//! Key details:
//! - Every expected string is PHP 8.5.12's output for the same program.
//! - Subjects that must not be folded at compile time are built from `$argc`.

use crate::support::*;

/// Any PHP delimiter, leading whitespace, bracket nesting and escaped closers parse like PHP.
#[test]
fn test_preg_delimiters_follow_php() {
    let out = compile_and_run(
        r##"<?php
$cb = fn($m) => "X";
echo var_export(preg_replace_callback("#a#", $cb, "ab"), true), "\n";
echo var_export(preg_replace_callback("~a~i", $cb, "Ab"), true), "\n";
echo var_export(preg_match("{a}", "ab"), true), "\n";
echo var_export(preg_match(" /a/", "a"), true), "\n";
echo var_export(preg_match("\t\n/a/", "a"), true), "\n";
echo var_export(preg_match("(a(b)c)", "abc", $m), true), " ", $m[1], "\n";
echo var_export(preg_match("/a\\/b/", "a/b"), true), "\n";
echo var_export(preg_match("//", "a"), true), "\n";
echo var_export(preg_match("/a/\n", "a"), true), "\n";
echo var_export(preg_match("/a/SX", "a"), true), "\n";
"##,
    );
    assert_eq!(out, "'Xb'\n'Xb'\n1\n1\n1\n1 b\n1\n1\n1\n1\n");
}

/// Patterns PHP rejects before compiling return each function's failure value.
#[test]
fn test_preg_malformed_patterns_fail_like_php() {
    let out = compile_and_run(
        r##"<?php
$cb = fn($m) => "X";
echo var_export(@preg_replace_callback("/a/b/i", $cb, "a/b a"), true), "\n";
echo var_export(@preg_replace_callback("a", $cb, "ab"), true), "\n";
echo var_export(@preg_replace_callback("", $cb, "ab"), true), "\n";
echo var_export(@preg_match("(a\\)", "a)"), true), "\n";
echo var_export(@preg_match("/a/ b", "a"), true), "\n";
echo var_export(@preg_match("   ", "a"), true), "\n";
echo var_export(@preg_match("\\a\\", "a"), true), "\n";
echo var_export(@preg_match_all("/a", "a"), true), "\n";
echo var_export(@preg_replace("#a", "b", "a"), true), "\n";
echo var_export(@preg_split("<a", "a"), true), "\n";
echo var_export(@preg_grep("1a1", ["a"]), true), "\n";
"##,
    );
    assert_eq!(
        out,
        "NULL\nNULL\nNULL\nfalse\nfalse\nfalse\nfalse\nfalse\nNULL\nfalse\nfalse\n"
    );
}

/// `A`, `D`, `n`, `J` and `r` map to PCRE2 compile options as in php-src.
///
/// `r` only changes caseless matching between ASCII and non-ASCII letters, which needs
/// PCRE2 10.43+; the test providers may be older, so only its acceptance is checked here.
#[test]
fn test_preg_modifiers_a_d_n_j_r() {
    let out = compile_and_run(
        r##"<?php
echo var_export(preg_match("/a/A", "ba"), true), "\n";
echo var_export(preg_match('/a$/D', "a\n"), true), "\n";
echo var_export(preg_match('/a$/', "a\n"), true), "\n";
preg_match("/(a)(b)/n", "ab", $m); echo count($m), "\n";
echo var_export(preg_match("/(?<n>a)|(?<n>b)/J", "b"), true), "\n";
echo var_export(preg_match("/k/ir", "K"), true), "\n";
echo var_export(preg_match_all("/a/A", "aaba"), true), "\n";
echo var_export(preg_replace("/a/A", "x", "aaba"), true), "\n";
echo implode("|", preg_split("/,/A", ",a,b")), "\n";
"##,
    );
    assert_eq!(out, "0\n0\n1\n1\n1\n1\n2\n'xxba'\n|a,b\n");
}

/// Zero-length matches keep every subject byte, probe the subject end, and see the
/// original subject for anchors and lookbehind.
#[test]
fn test_preg_replace_zero_length_matches() {
    let out = compile_and_run(
        r##"<?php
echo preg_replace("/x*/", "-", "abc"), "\n";
echo preg_replace("/\\b/", "|", "ab cd"), "\n";
echo preg_replace_callback("/x*/", fn($m) => "[]", ""), "\n";
echo preg_replace_callback("/x*/", fn($m) => "[]", "a"), "\n";
echo preg_replace("//u", "|", "àb"), "\n";
echo preg_replace_callback("//u", fn($m) => "|", "àb"), "\n";
echo preg_replace("/^a/", "x", "aaa"), "\n";
echo preg_replace_callback("/^a/", fn($m) => "x", "aaa"), "\n";
echo preg_replace("/(?<=a)b/", "X", "abab"), "\n";
echo preg_replace("/a|/", '[$0]', "bab"), "\n";
echo preg_replace_callback("/(a)|/", fn($m) => "[" . count($m) . "]", "bab"), "\n";
echo preg_replace("/(\\w)(\\d)?/", '<$2$1>', "a1b"), "\n";
"##,
    );
    assert_eq!(
        out,
        "-a-b-c-\n|ab| |cd|\n[]\n[]a[]\n|à|b|\n|à|b|\nxaa\nxaa\naXaX\n[]b[a][]b[]\n[1]b[2][1]b[1]\n<1a><b>\n"
    );
}

/// `preg_replace()` over an empty runtime subject returns an empty string and leaves the
/// concat buffer usable.
#[test]
fn test_preg_replace_empty_runtime_subject() {
    let out = compile_and_run(
        r##"<?php
$e = str_repeat("a", $argc - 1);
var_dump(preg_replace("/x/", "y", $e));
var_dump(preg_replace("/x/", "y", $e) . "|tail");
var_dump(preg_replace("/x/", "y", $e) === "");
var_dump(preg_replace("/a/", "y", "ba") . "|tail");
"##,
    );
    assert_eq!(
        out,
        "string(0) \"\"\nstring(5) \"|tail\"\nbool(true)\nstring(7) \"by|tail\"\n"
    );
}

/// `$limit` counts only the pieces `preg_split()` returns, and limits below -1 act like 1.
#[test]
fn test_preg_split_limit_counts_returned_pieces() {
    let out = compile_and_run(
        r##"<?php
echo implode("|", preg_split("/,/", ",a,b", 2, PREG_SPLIT_NO_EMPTY)), "\n";
echo implode("|", preg_split("/,/", ",,a,,b,c", 3, PREG_SPLIT_NO_EMPTY)), "\n";
echo implode("|", preg_split("/(,)/", ",a,,b", 3, PREG_SPLIT_NO_EMPTY | PREG_SPLIT_DELIM_CAPTURE)), "\n";
foreach (preg_split("/,/", ",a,b", 2, PREG_SPLIT_NO_EMPTY | PREG_SPLIT_OFFSET_CAPTURE) as $p) {
    echo $p[0], "@", $p[1], " ";
}
echo "\n";
echo implode("|", preg_split("/,/", "a,b,c", -2)), "\n";
echo implode("|", preg_split("/,/", "a,b,c", 1)), "\n";
echo implode("|", preg_split("/,/", "a,b,c", 0)), "\n";
echo implode("|", preg_split("/,/", "a,b,c", -1)), "\n";
"##,
    );
    assert_eq!(out, "a|b\na|b|c\n,|a|,|,|b\na@1 b@3 \na,b,c\na,b,c\na|b|c\na|b|c\n");
}

/// `preg_split()` steps over zero-length separators like PHP and reports absolute offsets.
#[test]
fn test_preg_split_zero_length_separators() {
    let out = compile_and_run(
        r##"<?php
echo implode("|", preg_split("//", "abc")), "\n";
echo implode("|", preg_split("/x*/", "ab")), "\n";
echo implode("|", preg_split("//u", "àb", -1, PREG_SPLIT_NO_EMPTY)), "\n";
echo implode("|", preg_split("/^a/", "aaa")), "\n";
foreach (preg_split("/(b)/", "abc", -1, PREG_SPLIT_DELIM_CAPTURE | PREG_SPLIT_OFFSET_CAPTURE) as $p) {
    echo $p[0], "@", $p[1], " ";
}
echo "\n";
"##,
    );
    assert_eq!(out, "|a|b|c|\n|a|b|\nà|b\n|aa\na@0 b@1 c@2 \n");
}
