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
