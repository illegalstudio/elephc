//! Purpose:
//! Regression tests for a by-reference `foreach` whose source is a function call, both the
//! by-value result (php bug #67633) and a by-reference-returning callee.
//!
//! Called from:
//! - `cargo test` through the `codegen_tests` harness via `crate::support`.
//!
//! Key details:
//! - A by-value call result is a shared copy: the by-reference loop must iterate a private
//!   array and leave the caller's array untouched. Converting the unboxed `mixed` payload
//!   without acquiring a reference first consumed the `mixed` cell's owner, so the caller's
//!   array was freed and the following ownership release double-freed it (issue #1790).
//! - A by-reference-returning callee hands the loop its own cell, so the loop mutates the
//!   array the caller's variable aliases.

use crate::support::*;

/// A by-reference loop over a by-value call result iterates a copy; the caller's array is intact.
#[test]
fn test_by_ref_foreach_over_by_value_call_result_keeps_the_caller_array() {
    let out = compile_and_run(
        r#"<?php
function id($x) { return $x; }
$array = ['a', 'b', 'c'];
foreach (id($array) as &$v) { $v .= 'q'; }
var_dump($array);
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(1) \"b\"\n  [2]=>\n  string(1) \"c\"\n}\n"
    );
}

/// The same holds when the by-value result is an associative array.
#[test]
fn test_by_ref_foreach_over_by_value_assoc_call_result_keeps_the_caller_array() {
    let out = compile_and_run(
        r#"<?php
function id($x) { return $x; }
$array = ['k' => 'v', 'j' => 'w'];
foreach (id($array) as &$v) { $v .= 'q'; }
echo $array['k'], '|', $array['j'];
"#,
    );
    assert_eq!(out, "v|w");
}

/// The caller's array also survives when the by-value result is bound to a local first.
#[test]
fn test_by_value_call_result_assignment_keeps_the_caller_array() {
    let out = compile_and_run(
        r#"<?php
function id($x) { return $x; }
$array = ['a', 'b', 'c'];
$copy = id($array);
echo implode(',', $array), '|', implode(',', $copy);
"#,
    );
    assert_eq!(out, "a,b,c|a,b,c");
}

/// A by-reference-returning callee lets the loop mutate the array the caller aliases.
#[test]
fn test_by_ref_foreach_over_reference_returning_call_mutates_the_referenced_array() {
    let out = compile_and_run(
        r#"<?php
function &ref_id(&$x) { return $x; }
$array = ['a', 'b', 'c'];
foreach (ref_id($array) as &$v) { $v .= 'q'; }
unset($v);
echo implode(',', $array);
"#,
    );
    assert_eq!(out, "aq,bq,cq");
}

/// The full php-src `bug67633.phpt` sequence: the by-value loop copies, the reference loop writes.
#[test]
fn test_bug67633_sequence() {
    let out = compile_and_run(
        r#"<?php
function id($x) { return $x; }
function &ref_id(&$x) { return $x; }
$c = 'c';
$array = ['a', 'b', $c];
foreach (id($array) as &$v) { $v .= 'q'; }
echo implode(',', $array), '|';
foreach (ref_id($array) as &$v) { $v .= 'q'; }
unset($v);
echo implode(',', $array);
"#,
    );
    assert_eq!(out, "a,b,c|aq,bq,cq");
}
