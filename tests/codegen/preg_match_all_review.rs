//! Purpose:
//! Regressions for regex iteration context and capture-output reference cells.
//!
//! Called from:
//! - The system codegen integration tests.
//!
//! Key details:
//! - Iteration must retain anchors, boundaries, lookbehind and absolute offsets.
//! - Repeated capture writes retire old output owners without replacing reference cells.

use crate::support::*;

/// Named output arguments update matches rather than the argument at source position two.
#[test]
fn preg_match_all_oct9_reordered_named_matches() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$flags = PREG_SET_ORDER;
echo preg_match_all(pattern: '/([a-z])/', subject: 'ab', flags: $flags, matches: $all),
    ':', $all[1][1], ':', $flags + 1, '|';
$previous = 42;
echo preg_match_all(pattern: '/([a-z])/', flags: $flags, subject: 'ab', matches: $previous),
    ':', $previous[0][1], ':', $flags + 1, '|';
echo preg_match(matches: $single, pattern: '/(a)/', subject: 'a'), ':', $single[1];
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:b:3|2:a:3|1:a");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Conflicting order flags fail closed without a capture matrix on both execution routes.
#[test]
fn preg_match_all_oct9_conflicting_eval_order() {
    let out = compile_and_run_with_regex(r#"<?php
echo preg_match_all('/(a)/', 'a', $native, PREG_PATTERN_ORDER | PREG_SET_ORDER), ':', count($native), '|';
$source = 'echo preg_match_all("/(a)/", "a", $matches, PREG_PATTERN_ORDER | PREG_SET_ORDER), ":", count($matches);'
    . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "0:0|0:0");
}

/// Keeps the original subject visible for later alternatives in both count and capture modes.
#[test]
fn preg_match_all_review_retains_prefix_context() {
    let out = compile_and_run(r#"<?php
echo preg_match_all('/a|^b/', 'ab', $anchor), ':', count($anchor[0]), '|';
echo preg_match_all('/a|^b/', 'ab'), '|';
echo preg_match_all('/a|\Ab/', 'ab', $absolute), ':', count($absolute[0]), '|';
echo preg_match_all('/a|\bb/', 'ab', $boundary), ':', count($boundary[0]), '|';
echo preg_match_all('/a|(?<=a)b/', 'ab', $behind, PREG_OFFSET_CAPTURE), ':',
    $behind[0][1][0], '@', $behind[0][1][1], '|';
echo preg_match_all('/a|(?<=a)b/', 'ab'), '|';
echo preg_match_all('/a|^b/m', "a\nb", $lines), ':', $lines[0][1];
"#);
    assert_eq!(out, "1:1|1|1:1|1:1|2:b@1|2|2:b");
}

/// Replaces a typed by-reference output while preserving aliases and match counts.
#[test]
fn preg_match_all_review_writes_through_array_parameter() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function collect(array &$matches): int {
    return preg_match_all('/([a-z])([0-9])/', 'a1 b2', $matches, PREG_SET_ORDER);
}
$matches = [str_repeat('x', 24)];
for ($i = 0; $i < 8; $i++) {
    echo collect($matches), ':', $matches[1][0], ':', $matches[1][2], '|';
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:b2:2|".repeat(8));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Supports Mixed by-reference output storage and named/case-folded builtin calls.
#[test]
fn preg_match_all_review_writes_through_mixed_parameter() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function collect(&$matches): int {
    return \PrEg_MaTcH_AlL(pattern: '/([a-z])/', subject: 'ab', matches: $matches,
        flags: PREG_SET_ORDER | PREG_OFFSET_CAPTURE);
}
$matches = [str_repeat('x', 24)];
for ($i = 0; $i < 8; $i++) {
    echo collect($matches), ':', $matches[1][1][0], '@', $matches[1][1][1], '|';
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:b@1|".repeat(8));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Local aliases retain their shared cell across successful, empty and invalid capture writes.
#[test]
fn preg_match_all_review_preserves_local_aliases() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$matches = [str_repeat('x', 24), 1];
$alias =& $matches;
for ($i = 0; $i < 8; $i++) {
    echo preg_match_all('/(a)?(b)/', 'b', $alias, PREG_UNMATCHED_AS_NULL), ':',
        ($matches[1][0] === null ? 'n' : 'bad'), ':', $matches[2][0], '|';
}
echo preg_match_all('/x/', 'b', $alias), ':', count($matches[0]), '|';
echo preg_match_all('/(/', 'b', $alias), ':', count($matches);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("{}0:0|0:0", "1:n:b|".repeat(8)));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Conditional aliases work in both raw and promoted representations.
#[test]
fn preg_match_all_review_preserves_conditional_aliases() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function collect(bool $bind): void {
    $matches = [str_repeat('x', 24), 1];
    $alias = [str_repeat('y', 24), 1];
    if ($bind) { $alias =& $matches; }
    echo preg_match_all('/(b)/', 'b', $alias), ':', $alias[1][0], ':',
        ($bind ? $matches[1][0] : $matches[0]), '|';
}
collect(false); collect(true);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("1:b:{}|1:b:b|", "x".repeat(24)));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native-backed eval iteration uses the same context-preserving opaque shim.
#[test]
fn preg_match_all_review_eval_retains_prefix_context() {
    let out = compile_and_run_with_regex(r#"<?php
$source = 'echo preg_match_all("/a|^b/", "ab", $a), ":", count($a[0]), "|";
echo preg_match_all("/a|(?<=a)b/", "ab", $b, PREG_OFFSET_CAPTURE), ":", $b[0][1][0], "@", $b[0][1][1];'
    . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "1:1|2:b@1");
}

/// Global iteration includes terminal empty matches and advances on UTF-8 or CRLF boundaries.
#[test]
fn preg_match_all_review_empty_match_progression() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function collect_case(string $pattern, string $subject, array &$matches): void {
    echo preg_match_all($pattern, $subject), ':',
        preg_match_all($pattern, $subject, $matches, PREG_OFFSET_CAPTURE), ':';
    foreach ($matches[0] as $match) { echo $match[1], ',', strlen($match[0]), ';'; }
    echo '|';
}
$matches = [str_repeat('x', 24), 1];
collect_case('/\b/', 'a', $matches); collect_case('//', 'ab', $matches); collect_case('/^/', '', $matches);
collect_case('/(?=b)/', 'ab', $matches); collect_case('/(?:|a)/', 'a', $matches);
collect_case('//u', 'éé', $matches); collect_case('/(*UTF)/', 'éé', $matches); collect_case('/(*CRLF)/', "\r\n", $matches);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:2:0,0;1,0;|3:3:0,0;1,0;2,0;|1:1:0,0;|1:1:1,0;|3:3:0,0;0,1;1,0;|3:3:0,0;2,0;4,0;|3:3:0,0;2,0;4,0;|2:2:0,0;2,0;|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Set-order capture rows preserve terminal UTF-8 offsets across repeated reference writes.
#[test]
fn preg_match_all_review_empty_set_order_reference() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function collect(array &$matches): int {
    return preg_match_all('//u', 'éé', $matches, PREG_SET_ORDER | PREG_OFFSET_CAPTURE);
}
$matches = [];
for ($i = 0; $i < 8; $i++) {
    echo collect($matches), ':', count($matches), ':';
    foreach ($matches as $row) { echo $row[0][1], ','; }
    echo '|';
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3:3:0,2,4,|".repeat(8));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Only omitted matches, including named omissions before flags, select count-only execution.
#[test]
fn preg_match_all_review_omitted_named_matches() {
    let out = compile_and_run(r#"<?php
echo \PrEg_MaTcH_AlL(subject: 'ab', flags: PREG_SET_ORDER, pattern: '//'), '|';
echo preg_match_all(subject: 'ab', pattern: '//');
"#);
    assert_eq!(out, "3|3");
}

/// Variable syntax backed by global or static storage cannot discard capture output.
#[test]
fn preg_match_all_review_rejects_global_and_static_destinations() {
    for source in [
        r#"<?php $matches = []; function collect(): void { global $matches; echo preg_match_all('/a/', 'a', $matches); } collect();"#,
        r#"<?php function collect(): void { static $matches = []; echo preg_match_all('/a/', 'a', $matches); } collect();"#,
    ] {
        let error = compile_cli_file_with_flags_expect_failure(source, &[]);
        assert!(error.contains("preg_match_all(): non-local $matches destinations are not supported"), "{error}");
    }
}

/// Native-backed dynamic eval uses the same terminal-empty and UTF-8 progression rules.
#[test]
fn preg_match_all_review_eval_empty_match_progression() {
    let out = compile_and_run_with_regex(r#"<?php
$source = 'echo preg_match_all("//u", "éé", $matches, PREG_SET_ORDER | PREG_OFFSET_CAPTURE), ":", $matches[2][0][1], "|"; echo preg_match_all("/^/", "");'
    . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "3:4|1");
}
