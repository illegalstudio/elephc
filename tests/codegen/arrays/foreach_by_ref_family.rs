//! Purpose:
//! End-to-end regression tests for the by-reference `foreach` source family
//! (issues #1803, #1805, #1807, #1809, #1810) and the `var_dump` reference
//! marker (#1802). Each test pins the exact PHP 8.5.10 output recorded in the
//! issue, cross-checked with `php` at authoring time.
//!
//! Called from:
//! - `cargo test` through the `codegen_tests` harness via `crate::support`.
//!
//! Key details:
//! - #1803 iterates through a direct `$ref =& $array` alias; #1805 nests two
//!   by-reference walks over the same array; #1807 deletes the tail key before
//!   appending; #1809 appends into an instance property during the walk;
//!   #1810 replaces the whole source mid-loop; #1802 pins the `&` marker
//!   `var_dump` prints for live reference entries on the full-array dump (PHP
//!   only marks the entry when dumping the container itself, not on a scalar
//!   element read, and never in `print_r`/`var_export`).
//! - The foreach-side marker pins (which entries stay referenced after a
//!   by-reference walk) follow php's reference-set dissolution on loop rebind
//!   and `unset()`; they are tracked with the remaining #1802 work and are
//!   intentionally not pinned here until that lands.
//! - All fixtures use bounded bodies, so a wrong implementation cannot turn a
//!   failing pin into a divergent or non-terminating binary.

use crate::support::*;

/// #1803: the loop promotes the array through `$ref`'s aliasing cell and both
/// names observe the mutated storage afterwards.
#[test]
fn by_ref_foreach_over_a_reference_alias_mutates_the_aliased_array() {
    let out = compile_and_run(
        r#"<?php
$array = ['a', 'b', 'c'];
$ref =& $array;
foreach ($ref as &$v) { $v .= 'q'; }
unset($v);
echo implode(",", $array);
"#,
    );
    assert_eq!(out, "aq,bq,cq");
}

/// #1805: two by-reference walks over the same array stay independent and the
/// outer loop's element stays valid across the inner walk.
#[test]
fn nested_by_ref_foreach_over_the_same_array_visits_every_pair() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
foreach ($a as &$x) {
    foreach ($a as &$y) {
        echo "$x-$y\n";
    }
}
unset($x);
unset($y);
"#,
    );
    assert_eq!(out, "1-1\n1-2\n2-1\n2-2\n");
}

/// #1807: the tail anchor must survive the tail key's deletion, so an append
/// after `unset($tail)` is still visited.
#[test]
fn by_ref_foreach_visits_an_append_after_the_tail_key_is_unset() {
    let out = compile_and_run(
        r#"<?php
$a = [1];
foreach ($a as &$v) {
    echo "$v\n";
    unset($a[0]);
    $a[1] = 2;
}
unset($v);
"#,
    );
    assert_eq!(out, "1\n2\n");
}

/// #1807, string keys: the deletion + append shape through persisted keys.
#[test]
fn by_ref_foreach_visits_an_assoc_append_after_the_tail_key_is_unset() {
    let out = compile_and_run(
        r#"<?php
$a = ["x" => 1];
foreach ($a as $k => &$v) {
    echo "$k=$v\n";
    unset($a["x"]);
    $a["y"] = 2;
}
unset($v);
"#,
    );
    assert_eq!(out, "x=1\ny=2\n");
}

/// #1809: an append through the property receiver during the walk is visited.
#[test]
fn by_ref_foreach_over_an_instance_property_visits_the_appended_tail() {
    let out = compile_and_run(
        r#"<?php
class C { public array $a = [1]; }
$o = new C();
foreach ($o->a as &$v) {
    echo "$v\n";
    $o->a[1] = 2;
}
unset($v);
"#,
    );
    assert_eq!(out, "1\n2\n");
}

/// #1809, bounded mid-loop append inside a longer property source.
#[test]
fn by_ref_foreach_over_an_instance_property_visits_a_mid_loop_append() {
    let out = compile_and_run(
        r#"<?php
class D { public array $a = [1, 2, 3]; }
$d = new D();
$seen = [];
foreach ($d->a as &$v) {
    $seen[] = "v$v";
    if (count($seen) >= 4) { break; }
    $d->a[] = 4;
}
unset($v);
echo implode("|", $seen), " final=", implode(",", $d->a);
"#,
    );
    assert_eq!(out, "v1|v2|v3|v4 final=1,2,3,4,4,4");
}

/// #1810: replacing the whole source mid-loop restarts the walk on the new
/// array's head instead of resuming into it by key identity.
#[test]
fn by_ref_foreach_restarts_from_the_head_when_the_source_is_reassigned() {
    let out = compile_and_run(
        r#"<?php
$a = [1];
$g = 0;
foreach ($a as &$v) {
    $g++;
    if ($g > 3) { break; }
    $a = [8, 9];
}
echo implode(",", $a), "|", $v;
unset($v);
echo "|", implode(",", $a);
"#,
    );
    assert_eq!(out, "8,9|8|8,9");
}

/// #1802: `var_dump` of the whole array prefixes live reference entries with `&`.
///
/// The direct element alias is the membership shape whose lifetime elephc
/// already models exactly: the entry joins the reference set at `$c =& $b[0]`
/// and stays referenced after that, so php's marker shows on `[0]` only.
#[test]
fn var_dump_marks_a_directly_aliased_element_with_ampersand() {
    let out = compile_and_run(
        r#"<?php
$b = [1, 2];
$c =& $b[0];
var_dump($b);
"#,
    );
    assert!(out.contains("[0]=>\n  &int(1)"), "missing & marker: {out}");
    assert!(out.contains("[1]=>\n  int(2)"), "plain entries must stay unmarked: {out}");
}

/// #1802: an entry that joins a reference set keeps the marker while a second
/// alias holds the cell, and php prints it before the type on the value line.
#[test]
fn var_dump_keeps_the_marker_while_the_reference_set_has_two_members() {
    let out = compile_and_run(
        r#"<?php
$b = [1, 2];
$c =& $b[0];
$d =& $c;
var_dump($b);
"#,
    );
    assert!(out.contains("[0]=>\n  &int(1)"), "missing & marker: {out}");
    assert!(out.contains("[1]=>\n  int(2)"), "plain entries must stay unmarked: {out}");
}

/// #1802: after a by-reference walk, php marks the entry the loop variable is
/// still bound to (its cell keeps a live member); earlier entries were released
/// by the rebinding walk and print without the marker.
#[test]
fn var_dump_marks_only_the_still_bound_entry_after_a_by_ref_foreach() {
    let out = compile_and_run(
        r#"<?php
$a = ['a', 'b', 'c'];
foreach ($a as &$v) { $v .= 'q'; }
var_dump($a);
"#,
    );
    assert!(out.contains("[0]=>\n  string(2) \"aq\""), "released entries must not print the marker: {out}");
    assert!(out.contains("[1]=>\n  string(2) \"bq\""), "released entries must not print the marker: {out}");
    assert!(out.contains("[2]=>\n  &string(2) \"cq\""), "the still-bound entry must print the marker: {out}");
}

/// #1802: `unset()` of the loop variable dissolves php's last reference set
/// member, so the whole array dumps without any marker afterwards.
#[test]
fn var_dump_prints_no_marker_after_the_loop_variable_is_unset() {
    let out = compile_and_run(
        r#"<?php
$overrides = ["u" => 1];
foreach ($overrides as &$value) {}
unset($value);
var_dump($overrides);
echo $overrides["u"];
"#,
    );
    assert!(!out.contains("&"), "dissolved entries print plain values: {out}");
    assert!(out.contains("[\"u\"]=>\n  int(1)"), "the value must survive: {out}");
    assert_eq!(out.ends_with("int(1)\n}\n1"), true, "the read after the dump stays the plain value: {out}");
}

/// #1802: PHP does not print the marker in `print_r` or `var_export`.
#[test]
fn print_r_and_var_export_do_not_mark_reference_entries() {
    let out = compile_and_run(
        r#"<?php
$b = [1, 2];
$c =& $b[0];
print_r($b);
var_export($b);
echo "\n";
"#,
    );
    assert!(!out.contains("&"), "print_r/var_export must not mark references: {out}");
}