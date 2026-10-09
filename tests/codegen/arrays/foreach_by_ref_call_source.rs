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

/// A by-reference `foreach` source adopts the callee's cell for every call kind `lower_ref_assign`
/// accepts, not just a direct function call: an instance method, a static method, `parent::`, a
/// closure variable, and an immediately-invoked closure literal. Each must mutate the caller's
/// array, where before they all iterated a detached copy (review follow-up for #1790).
#[test]
fn test_by_ref_foreach_over_every_reference_returning_call_kind() {
    let out = compile_and_run(
        r#"<?php
class C {
    public function &ref_id(&$x) { return $x; }
    public static function &sref(&$x) { return $x; }
}
class D extends C {
    public function parent_ref(&$x) {
        foreach (parent::ref_id($x) as &$v) { $v .= 'q'; }
        unset($v);
    }
}
$c = new C();
$a = ['a', 'b', 'c'];
foreach ($c->ref_id($a) as &$v) { $v .= 'q'; }
unset($v);
echo implode(',', $a), '|';
$b = ['a', 'b', 'c'];
foreach (C::sref($b) as &$v) { $v .= 'q'; }
unset($v);
echo implode(',', $b), '|';
$e = ['a', 'b', 'c'];
(new D())->parent_ref($e);
echo implode(',', $e), '|';
$f = ['a', 'b', 'c'];
$fn = function &(&$x) { return $x; };
foreach ($fn($f) as &$v) { $v .= 'q'; }
unset($v);
echo implode(',', $f), '|';
$g = ['a', 'b', 'c'];
foreach ((function &(&$x) { return $x; })($g) as &$v) { $v .= 'q'; }
unset($v);
echo implode(',', $g);
"#,
    );
    assert_eq!(out, "aq,bq,cq|aq,bq,cq|aq,bq,cq|aq,bq,cq|aq,bq,cq");
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
