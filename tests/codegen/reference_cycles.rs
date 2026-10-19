//! Purpose:
//! End-to-end tests for self-referencing arrays (`$a[] = &$a` and friends) and for references
//! to readonly properties, enum `name`/`value` included (SIGSEGVs found by
//! running the php-src phpt suite).
//!
//! Called from:
//! - `cargo test --test codegen_tests reference_cycle` through Rust's test harness.
//!
//! Key details:
//! - Every expected output was produced by php 8.5 on the same source.
//! - Cycles themselves are not collected (elephc has no array cycle collector), so only the
//!   acyclic programs run under `--heap-debug`.

use crate::support::*;

/// `$a[] = &$a` (gc_004): var_dump and print_r stop at the revisited table with PHP's
/// `*RECURSION*` marker instead of recursing until the stack overflows.
#[test]
fn test_reference_cycle_self_append_renders_recursion() {
    let out = compile_and_run(
        r#"<?php
$a = array();
$a[] =& $a;
var_dump($a);
print_r($a);
echo "\n", count($a), "\n";
"#,
    );
    assert_eq!(out, "array(1) {\n  [0]=>\n  *RECURSION*\n}\nArray\n(\n    [0] => Array\n *RECURSION*\n)\n\n1\n");
}

/// `$a[0][] = &$a[0]` (bug35163): the inner table renders once, behind its `&`, and each
/// self-reference inside it prints `*RECURSION*` without the `&`.
#[test]
fn test_reference_cycle_nested_element_renders_recursion() {
    let out = compile_and_run(
        r#"<?php
$a = array(array(1));
$a[0][] =& $a[0];
$a[0][] =& $a[0];
$a[0][0] = 2;
var_dump($a);
print_r($a);
echo "\n";
"#,
    );
    assert_eq!(out, "array(1) {\n  [0]=>\n  &array(3) {\n    [0]=>\n    int(2)\n    [1]=>\n    *RECURSION*\n    [2]=>\n    *RECURSION*\n  }\n}\nArray\n(\n    [0] => Array\n        (\n            [0] => 2\n            [1] => Array\n *RECURSION*\n            [2] => Array\n *RECURSION*\n        )\n\n)\n\n");
}

/// Two arrays referencing each other (print_r/var_dump_array_circular) and a hash holding a
/// reference to itself.
#[test]
fn test_reference_cycle_mutual_and_hash_render_recursion() {
    let out = compile_and_run(
        r#"<?php
$a1 = [];
$a2 = [&$a1];
$a1[] =& $a2;
print_r($a2);
var_dump($a2);
$h = ['k' => 'v'];
$h['self'] = &$h;
var_dump($h);
"#,
    );
    assert_eq!(out, "Array\n(\n    [0] => Array\n        (\n            [0] => Array\n *RECURSION*\n        )\n\n)\narray(1) {\n  [0]=>\n  &array(1) {\n    [0]=>\n    *RECURSION*\n  }\n}\narray(2) {\n  [\"k\"]=>\n  string(1) \"v\"\n  [\"self\"]=>\n  *RECURSION*\n}\n");
}

/// bug69376: copying a table whose singleton reference points back at that same table keeps the
/// reference shared, as php's `zend_array_dup` does, so a write through the copy reaches the
/// original; an ordinary singleton reference still separates.
#[test]
fn test_reference_cycle_copy_keeps_self_reference_shared() {
    let out = compile_and_run(
        r#"<?php
function &test() {
    $var = array();
    $var[] =& $var;
    return $var;
}
$a = test();
$b = $a;
$b[0] = 123;
print_r($a);
print_r($b);
$array = array();
$array[] = &$array;
$c = $array;
unset($array);
$d = $c;
$d[0] = 'w';
var_dump($c, $d);
$x = 1;
$l = [];
$l[] = &$x;
$m = $l;
$m[0] = 5;
echo $x, " ", $l[0], "\n";
"#,
    );
    assert_eq!(out, "Array\n(\n    [0] => 123\n)\nArray\n(\n    [0] => 123\n)\narray(1) {\n  [0]=>\n  &string(1) \"w\"\n}\narray(1) {\n  [0]=>\n  &string(1) \"w\"\n}\n5 5\n");
}

/// The recursion guard tracks only the tables on the current walk path: 300 nested arrays still
/// render in full, and sibling copies of one table each render their body.
#[test]
fn test_reference_cycle_guard_keeps_deep_and_sibling_tables() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [];
for ($i = 0; $i < 300; $i++) { $a = [$a]; }
ob_start();
var_dump($a);
$s = ob_get_clean();
echo substr_count($s, "array(1)"), " ", substr_count($s, "RECURSION"), "\n";
ob_start(); print_r($a); $s = ob_get_clean();
echo substr_count($s, "Array"), " ", substr_count($s, "RECURSION"), "\n";
$b = [1];
$c = [$b, $b, ['x' => $b]];
var_dump($c);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "300 0\n301 0\narray(3) {\n  [0]=>\n  array(1) {\n    [0]=>\n    int(1)\n  }\n  [1]=>\n  array(1) {\n    [0]=>\n    int(1)\n  }\n  [2]=>\n  array(1) {\n    [\"x\"]=>\n    array(1) {\n      [0]=>\n      int(1)\n    }\n  }\n}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Every reference to an enum's `value`/`name` throws php's readonly Error, and the singleton
/// stays intact (no-write-properties-through-foreach-reference, -references, no-return-
/// properties-by-ref used to SIGSEGV on the next read).
#[test]
fn test_enum_readonly_reference_forms_throw() {
    let out = compile_and_run(
        r#"<?php
enum Foo: int { case Bar = 0; }
function &byRef() { $bar = Foo::Bar; return $bar->value; }
function attempt(callable $f) {
    try { $f(); echo "no error\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
}
attempt(function () { $bar = Foo::Bar; foreach ([1] as &$bar->value) {} });
attempt(function () { $bar = Foo::Bar; $value = &$bar->value; $value = 1; });
attempt(function () { $value = &byRef(); $value = 1; });
attempt(function () { $bar = Foo::Bar; foreach ([1] as &$bar->name) {} });
var_dump(Foo::Bar->value, Foo::Bar->name, Foo::Bar);
"#,
    );
    assert_eq!(out, "Cannot indirectly modify readonly property Foo::$value\nCannot indirectly modify readonly property Foo::$value\nCannot indirectly modify readonly property Foo::$value\nCannot indirectly modify readonly property Foo::$name\nint(0)\nstring(3) \"Bar\"\nenum(Foo::Bar)\n");
}

/// A by-reference foreach over a readonly property (or an element of one) throws instead of
/// writing through the slot, and references to readonly properties never re-lay the slot out as
/// a reference cell (an uninitialized one still dumps as `uninitialized(int)`).
#[test]
fn test_readonly_property_by_reference_foreach_and_references_throw() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class R {
    public readonly int $late;
    public function __construct(public readonly array $a = [1, 2], public readonly int $i = 3) {}
    public function own() {
        try { foreach ($this->a as &$v) { $v++; } } catch (Error $e) { echo $e->getMessage(), "\n"; }
        try { $r = &$this->late; } catch (Error $e) { echo $e->getMessage(), "\n"; }
    }
}
function attempt(callable $f) {
    try { $f(); echo "no error\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
}
$o = new R();
attempt(function () use ($o) { foreach ($o->a as &$v) { $v++; } });
attempt(function () use ($o) { foreach ($o->a as $k => &$v) { $v = $k; } });
attempt(function () use ($o) { foreach ([5] as &$o->i) {} });
attempt(function () use ($o) { $r = &$o->i; $r = 7; });
attempt(function () use ($o) { $r = &$o->a[0]; $r = 7; });
$o->own();
var_dump($o);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Cannot indirectly modify readonly property R::$a\nCannot indirectly modify readonly property R::$a\nCannot indirectly modify readonly property R::$i\nCannot indirectly modify readonly property R::$i\nCannot indirectly modify readonly property R::$a\nCannot indirectly modify readonly property R::$a\nCannot indirectly modify readonly property R::$late\nobject(R)#1 (2) {\n  [\"late\"]=>\n  uninitialized(int)\n  [\"a\"]=>\n  array(2) {\n    [0]=>\n    int(1)\n    [1]=>\n    int(2)\n  }\n  [\"i\"]=>\n  int(3)\n}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `json_encode()` of a self-referencing array records JSON_ERROR_RECURSION and returns false
/// (`null` in its place under JSON_PARTIAL_OUTPUT_ON_ERROR, a JsonException under
/// JSON_THROW_ON_ERROR), and `serialize()` closes the cycle with php's `R:<index>;`. Both used to
/// recurse until they overran the concat buffer.
#[test]
fn test_reference_cycle_json_encode_and_serialize_terminate() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[] = &$a;
var_dump(json_encode($a));
echo json_last_error(), " ", json_last_error_msg(), "\n";
var_dump(json_encode($a, JSON_PARTIAL_OUTPUT_ON_ERROR));
try { json_encode($a, JSON_THROW_ON_ERROR); } catch (JsonException $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo serialize($a), "\n";
$h = ['k' => 'v'];
$h['self'] = &$h;
echo serialize($h), "\n";
var_dump(json_encode(['a' => [1, 2], 'b' => ['c' => true], 'h' => $h], JSON_PARTIAL_OUTPUT_ON_ERROR));
var_dump(json_encode(['a' => [1, 2], 'b' => ['c' => true]]));
echo json_last_error(), "\n";
"#,
    );
    assert_eq!(out, "bool(false)\n6 Recursion detected\nstring(6) \"[null]\"\nJsonException: Recursion detected\na:1:{i:0;a:1:{i:0;R:2;}}\na:2:{s:1:\"k\";s:1:\"v\";s:4:\"self\";a:2:{s:1:\"k\";s:1:\"v\";s:4:\"self\";R:3;}}\nstring(52) \"{\"a\":[1,2],\"b\":{\"c\":true},\"h\":{\"k\":\"v\",\"self\":null}}\"\nstring(26) \"{\"a\":[1,2],\"b\":{\"c\":true}}\"\n0\n");
}

/// `===` between two distinct self-referencing arrays throws php's catchable "Nesting level too
/// deep - recursive dependency?" (it used to recurse until the stack overflowed); the guard is
/// cleared by the throw, so later comparisons are unaffected.
#[test]
fn test_reference_cycle_strict_equality_throws_nesting_error() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[] = &$a;
$b = [];
$b[] = &$b;
var_dump($a === $a);
try { var_dump($a === $b); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$x = [1, [2, [3]]];
$y = [1, [2, [3]]];
var_dump($x === $y, $x === [1, [2, [4]]], [] === [], [1] === [1, 2]);
try { var_dump($a === $b); } catch (Error $e) { echo $e->getMessage(), "\n"; }
var_dump($x === $y);
"#,
    );
    assert_eq!(out, "bool(true)\nError: Nesting level too deep - recursive dependency?\nbool(true)\nbool(false)\nbool(true)\nbool(false)\nNesting level too deep - recursive dependency?\nbool(true)\n");
}

/// A loop whose body makes a local's entries referenceable (by-reference foreach, `$r = &$b[$i]`,
/// `$h[$k] = &$z`) carries the hash layout around the back edge, so the next iteration does not
/// run packed-array ops on it.
#[test]
fn test_reference_cycle_loop_carried_reference_storage() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1, 2, 3];
for ($i = 0; $i < 3; $i++) { foreach ($a as &$v) { $v++; } unset($v); }
echo implode(",", $a), "\n";
$b = [];
for ($i = 0; $i < 3; $i++) { $b[$i] = $i; $r = &$b[$i]; $r *= 10; unset($r); }
echo implode(",", $b), "\n";
$h = ['x' => 1];
$k = 0;
while ($k < 2) { $z = $k; $h["k$k"] = &$z; unset($z); $k++; }
echo implode(",", $h), "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "4,5,6\n0,10,20\n1,0,1\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// gc_012's shape: a thousand elements that each reference themselves, built in a loop (it used
/// to SIGBUS on the second iteration).
#[test]
fn test_reference_cycle_self_referencing_elements_in_a_loop() {
    let out = compile_and_run(
        r#"<?php
$a = array();
for ($i = 0; $i < 1000; $i++) {
    $a[$i] = array(array());
    $a[$i][0] = & $a[$i];
}
echo count($a), " ", count($a[999]), "\n";
var_dump($a[3]);
"#,
    );
    assert_eq!(out, "1000 1\narray(1) {\n  [0]=>\n  *RECURSION*\n}\n");
}

/// An ACYCLIC array deeper than the guard's storage is not a cycle. `===` on two
/// 2049-level nestings is true (it used to throw "Nesting level too deep" once the 2048-entry
/// stack filled), and `print_r` of a 2100-level nesting renders every level instead of a false
/// `*RECURSION*` at level 2048.
#[test]
fn test_reference_cycle_guard_full_stack_is_not_a_revisit() {
    let out = compile_and_run(
        r#"<?php
$a = []; $b = [];
for ($i = 0; $i < 2049; $i++) { $a = [$a]; $b = [$b]; }
var_dump($a === $b);
$c = [];
for ($i = 0; $i < 2049; $i++) { $c = [$c]; }
$c[0][0] = 1;
var_dump($a === $c);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\n");
    let out = compile_and_run(
        r#"<?php
$a = []; for ($i = 0; $i < 2100; $i++) { $a = [$a]; } print_r($a);
"#,
    );
    assert_eq!(out.matches("RECURSION").count(), 0);
    assert_eq!(out.matches("Array\n").count(), 2101);
}
