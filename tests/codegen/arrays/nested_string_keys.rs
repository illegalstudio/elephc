//! Purpose:
//! Regression coverage for nested writes whose intermediate key is NOT a string literal
//! (a variable, property, call or runtime-tagged key) and for the containers those writes go
//! through: locals, untyped and declared properties, static properties, static locals and
//! elements of other arrays.
//!
//! Called from:
//! - `cargo test --test codegen_tests arrays::nested_string_keys`.
//!
//! Key details:
//! - The parent fetch classified its key SYNTACTICALLY, and a plain variable key is
//!   syntactically `int`: `$c[$e]['k'] = 'x'` coerced `'a'` to `0` and printed `[{"k":"x"}]`.
//!   The key is now classified from its lowered value (string, integer or runtime-tagged).
//! - An untyped `public $listeners = []` inferred `array<never>`, a slot the nested-write
//!   lowering cannot write through, so Symfony's `EventDispatcher::addListener`
//!   (`$this->listeners[$eventName][$priority][] = $listener`) wrote into a copy and left the
//!   property empty. Such roots now widen to the boxed PHP array representation.
//! - Expected outputs are php 8.5's.

use crate::support::{
    compile_and_run, compile_and_run_with_heap_debug, compile_cli_file_with_flags_expect_failure,
};

/// A string-valued variable key keeps its own hash key at every level.
#[test]
fn test_nested_write_variable_string_key_keeps_key() {
    let out = compile_and_run(
        r#"<?php
$e = 'a'; $f = 'b'; $i = 3; $j = 0;
$c = []; $c[$e]['k'] = 'x'; echo json_encode($c), "\n";
$c[$e][$f] = 'y'; $c[$f][$i] = 1; echo json_encode($c), "\n";
$h = []; $h[$e][$f][$i][$j] = 'deep'; echo json_encode($h), "\n";
"#,
    );
    assert_eq!(
        out,
        "{\"a\":{\"k\":\"x\"}}\n{\"a\":{\"k\":\"x\",\"b\":\"y\"},\"b\":{\"3\":1}}\n{\"a\":{\"b\":{\"3\":[\"deep\"]}}}\n"
    );
}

/// Compound updates, appends and `??=` at the leaf of a variable-keyed chain.
#[test]
fn test_nested_write_variable_key_compound_leaf_updates() {
    let out = compile_and_run(
        r#"<?php
$e = 'a'; $f = 'b'; $i = 3;
$d = []; $d[$e][$i][$f] = 2; $d[$e][$i][$f] .= 'z'; echo json_encode($d), "\n";
$g = []; $g[$i][$e] = 1; $g[$i][$e] += 5; echo json_encode($g), "\n";
$m = []; $m[$e][] = 1; $m[$e][] = 2; $m[$f][$i][] = 3; echo json_encode($m), "\n";
$n = []; $n[$e]['q'] ??= 'first'; $n[$e]['q'] ??= 'second'; echo json_encode($n), "\n";
"#,
    );
    assert_eq!(
        out,
        "{\"a\":{\"3\":{\"b\":\"2z\"}}}\n{\"3\":{\"a\":6}}\n{\"a\":[1,2],\"b\":{\"3\":[3]}}\n{\"a\":{\"q\":\"first\"}}\n"
    );
}

/// `isset`, `array_key_exists` and nested `unset` observe and edit what the writes built.
#[test]
fn test_nested_write_variable_key_isset_and_unset() {
    let out = compile_and_run(
        r#"<?php
$e = 'a';
$c = []; $c[$e]['k'] = 'x'; $c[$e]['j'] = 'y';
var_dump(isset($c[$e]['k']), isset($c[$e]['zz']), array_key_exists($e, $c));
unset($c[$e]['k']); echo json_encode($c), "\n";
unset($c['missing']['k']); echo json_encode($c), "\n";
unset($c[$e]); echo json_encode($c), "\n";
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nbool(true)\n{\"a\":{\"j\":\"y\"}}\n{\"a\":{\"j\":\"y\"}}\n[]\n"
    );
}

/// Runtime-tagged keys (untyped parameters, foreach keys) and call keys.
#[test]
fn test_nested_write_mixed_and_call_keys() {
    let out = compile_and_run(
        r#"<?php
function build($k1, $k2) { $a = []; $a[$k1][$k2] = 1; $a[$k1][$k2] += 1; $a[$k1][] = 9; return $a; }
echo json_encode(build('s', 7)), json_encode(build(4, 'u')), json_encode(build('5', 'v')), "\n";
$r = [];
foreach (['x' => 1, 'y' => 2, 3 => 3] as $k => $v) { $r[$k]['v'] = $v; }
echo json_encode($r), "\n";
$t = []; $t[strtoupper('q')]['z'] = 1; echo json_encode($t), "\n";
"#,
    );
    assert_eq!(
        out,
        "{\"s\":{\"7\":2,\"8\":9}}{\"4\":{\"u\":2,\"0\":9}}{\"5\":{\"v\":2,\"0\":9}}\n{\"x\":{\"v\":1},\"y\":{\"v\":2},\"3\":{\"v\":3}}\n{\"Q\":{\"z\":1}}\n"
    );
}

/// An integer key past the end of a list creates that key only (no null padding).
#[test]
fn test_nested_write_integer_gap_key_promotes_instead_of_padding() {
    let out = compile_and_run(
        r#"<?php
$h = []; $h[5]['k'] = 1; $h[9]['z'] = 2; echo json_encode($h), count($h), "\n";
$l = []; $l[0]['k'] = 1; $l[1]['k'] = 2; echo json_encode($l), "\n";
$h2 = []; $h2[1][2][3][0] = 'deep'; echo json_encode($h2), "\n";
"#,
    );
    assert_eq!(
        out,
        "{\"5\":{\"k\":1},\"9\":{\"z\":2}}2\n[{\"k\":1},{\"k\":2}]\n{\"1\":{\"2\":{\"3\":[\"deep\"]}}}\n"
    );
}

/// Symfony `EventDispatcher::addListener` on an untyped `= []` property.
#[test]
fn test_nested_write_untyped_property_listener_map() {
    let out = compile_and_run(
        r#"<?php
class Ev {
    public $listeners = [];
    public array $typed = [];
    function listen(string $e, $l, int $prio) { $this->listeners[$e][$prio][] = $l; }
    function typed(string $e, $l, int $prio) { $this->typed[$e][$prio][] = $l; }
}
$ev = new Ev;
$ev->listen('a', 'x', 0); $ev->listen('a', 'y', 0); $ev->listen('b', 'z', -5);
$ev->typed('a', 'x', 10); $ev->typed('a', 'y', 0);
echo json_encode($ev->listeners), "\n", json_encode($ev->typed), "\n";
var_dump(isset($ev->listeners['a'][0]), isset($ev->listeners['c']));
unset($ev->listeners['a'][0]); echo json_encode($ev->listeners), "\n";
unset($ev->listeners['b']); echo json_encode($ev->listeners), "\n";
"#,
    );
    assert_eq!(
        out,
        "{\"a\":[[\"x\",\"y\"]],\"b\":{\"-5\":[\"z\"]}}\n{\"a\":{\"10\":[\"x\"],\"0\":[\"y\"]}}\nbool(true)\nbool(false)\n{\"a\":[],\"b\":{\"-5\":[\"z\"]}}\n{\"a\":[]}\n"
    );
}

/// The receiver of the property root may be an array element or a call result.
#[test]
fn test_nested_write_property_root_through_element_and_call_receivers() {
    let out = compile_and_run(
        r#"<?php
class E { public $p = []; function me() { return $this; } }
$e = [new E]; $e[0]->p['a']['c'] = 2; echo json_encode($e[0]->p), "\n";
$f = new E; $f->me()->p['a']['d'] = 3; echo json_encode($f->p), "\n";
"#,
    );
    assert_eq!(out, "{\"a\":{\"c\":2}}\n{\"a\":{\"d\":3}}\n");
}

/// Static properties and elements of other arrays as nested-write containers.
#[test]
fn test_nested_write_static_and_element_containers() {
    let out = compile_and_run(
        r#"<?php
class R {
    public static $map = [];
    public static array $tmap = [];
    static function add(string $e, int $p, $v) { static::$map[$e][$p][] = $v; self::$tmap[$e][$p] = $v; }
}
R::add('a', 1, 'x'); R::add('a', 1, 'y'); R::add('b', 2, 'z');
echo json_encode(R::$map), json_encode(R::$tmap), "\n";
class K { public $name = 'nm'; public $id = 4; }
$o = new K;
$outer = ['root' => []];
$outer['root'][$o->name][$o->id] = 'v';
$outer['root'][$o->name]['list'][] = 1;
echo json_encode($outer), "\n";
$w = [[]]; $z = 'z'; $w[0][$z]['k'] = 1; $w[0][$z]['k'] += 2; echo json_encode($w), "\n";
"#,
    );
    assert_eq!(
        out,
        "{\"a\":{\"1\":[\"x\",\"y\"]},\"b\":{\"2\":[\"z\"]}}{\"a\":{\"1\":\"y\"},\"b\":{\"2\":\"z\"}}\n{\"root\":{\"nm\":{\"4\":\"v\",\"list\":[1]}}}\n[{\"z\":{\"k\":3}}]\n"
    );
}

/// A static array property inside an array literal keeps its array value.
#[test]
fn test_static_array_property_inside_array_literal() {
    let out = compile_and_run(
        r#"<?php
class P { public static array $a = ['n' => 0]; public static $u = ['n' => 1]; }
echo json_encode([P::$a, P::$u]), "\n";
$x = [P::$a, 5]; echo json_encode($x), "\n";
echo json_encode(['k' => P::$a]), "\n";
"#,
    );
    assert_eq!(out, "[{\"n\":0},{\"n\":1}]\n[{\"n\":0},5]\n{\"k\":{\"n\":0}}\n");
}

/// Resetting an untyped static property that held an array or an object back to null.
#[test]
fn test_static_property_reset_to_null_after_array_or_object() {
    let out = compile_and_run(
        r#"<?php
class R { public static $u = null; public static $o = null; }
R::$u = ['a' => 1]; R::$o = new stdClass;
R::$u = null; R::$o = null; var_dump(R::$u, R::$o);
class S { public static $instance; static function get() { return static::$instance ??= new S; } }
S::get(); S::$instance = null; var_dump(S::$instance);
"#,
    );
    assert_eq!(out, "NULL\nNULL\nNULL\n");
}

/// The listener map pattern is leak-free under heap-debug.
#[test]
fn test_nested_write_listener_map_heap_debug_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class D {
    private $listeners = [];
    function add(string $e, $l, int $p) { $this->listeners[$e][$p][] = $l; }
    function drop(string $e) { unset($this->listeners[$e]); }
    function count(): int { return count($this->listeners); }
}
$d = new D;
for ($i = 0; $i < 300; $i++) {
    $d->add('ev' . ($i % 7), 'l' . $i, $i % 3);
    $k = 'k' . ($i % 5); $m = []; $m[$k][$i % 2][] = $i; $m[$k]['s'] = str_repeat('x', 3);
    unset($m[$k][0]);
}
$d->drop('ev1');
echo $d->count(), "\n";
"#,
    );
    assert_eq!(out.stdout, "6\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A nested write through a by-reference alias of a concretely typed container would land in a
/// copy, so it is refused instead of compiling a write that silently does nothing.
#[test]
fn test_nested_write_through_copy_is_refused() {
    let err = compile_cli_file_with_flags_expect_failure(
        r#"<?php
$m = ["a" => ["b" => 1]];
$r = &$m;
$r["a"]["c"] = 2;
echo json_encode($m);
"#,
        &[],
    );
    assert!(
        err.contains("Nested array write through this container is not supported"),
        "unexpected diagnostic: {err}"
    );
}

/// After a nested write promoted a statically indexed local to hash storage
/// at run time (gap key or boxed string key), `$a[$k][] = $v` read dense slots out of the hash
/// table — silently wrong groups or a SIGSEGV. The append now goes through the fetch-for-write
/// parent cell, which dispatches on the runtime container kind.

#[test]
fn test_nested_append_after_runtime_hash_promotion_gap_then_variable_int_key() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [];
$a[3]["k"] = 1;
$x = 0;
$a[$x][] = 2;
$a[4][] = 5;
var_dump($a);
"#,
    );
    assert_eq!(out.stdout, "array(3) {\n  [3]=>\n  array(1) {\n    [\"k\"]=>\n    int(1)\n  }\n  [0]=>\n  array(1) {\n    [0]=>\n    int(2)\n  }\n  [4]=>\n  array(1) {\n    [0]=>\n    int(5)\n  }\n}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

#[test]
fn test_nested_append_after_runtime_hash_promotion_grouping_by_id_then_literal() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function grp(array $ids) { $g = []; foreach ($ids as $id) { $g[$id]["n"] = $id; } $g[9][] = 1; return $g; }
var_dump(grp([5, 2]));
"#,
    );
    assert_eq!(out.stdout, "array(3) {\n  [5]=>\n  array(1) {\n    [\"n\"]=>\n    int(5)\n  }\n  [2]=>\n  array(1) {\n    [\"n\"]=>\n    int(2)\n  }\n  [9]=>\n  array(1) {\n    [0]=>\n    int(1)\n  }\n}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

#[test]
fn test_nested_append_after_runtime_hash_promotion_mixed_string_key_then_int() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [];
$m = json_decode("\"ev\"");
$a[$m]["k"] = 1;
$a[0][] = 2;
var_dump($a);
"#,
    );
    assert_eq!(out.stdout, "array(2) {\n  [\"ev\"]=>\n  array(1) {\n    [\"k\"]=>\n    int(1)\n  }\n  [0]=>\n  array(1) {\n    [0]=>\n    int(2)\n  }\n}\n");
    // `json_decode` of a scalar string leaks one block on its own (preexisting, also on main),
    // so this test checks stdout only.
}

#[test]
fn test_nested_append_after_runtime_hash_promotion_gap_then_lower_int_key() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [];
$a[5]["k"] = 1;
$a[2][] = "x";
var_dump($a);
"#,
    );
    assert_eq!(out.stdout, "array(2) {\n  [5]=>\n  array(1) {\n    [\"k\"]=>\n    int(1)\n  }\n  [2]=>\n  array(1) {\n    [0]=>\n    string(1) \"x\"\n  }\n}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

#[test]
fn test_nested_append_after_runtime_hash_promotion_gap_then_variable_key_past_end() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$g = [];
$j = 6;
$g[5]["k"] = 1;
$g[$j][] = 3;
var_dump($g);
"#,
    );
    assert_eq!(out.stdout, "array(2) {\n  [5]=>\n  array(1) {\n    [\"k\"]=>\n    int(1)\n  }\n  [6]=>\n  array(1) {\n    [0]=>\n    int(3)\n  }\n}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

#[test]
fn test_nested_append_after_runtime_hash_promotion_list_with_gap_append() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [[1]];
$a[5][] = "x";
var_dump($a);
"#,
    );
    assert_eq!(out.stdout, "array(2) {\n  [0]=>\n  array(1) {\n    [0]=>\n    int(1)\n  }\n  [5]=>\n  array(1) {\n    [0]=>\n    string(1) \"x\"\n  }\n}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// LIMIT (preexisting, also on the main branch): a nested write whose key has a side effect on
/// the right-hand side variable stores the OLD value — elephc reads `$i` before `$i++` runs,
/// where php evaluates the keys first and stores `1`. Kept ignored as a record of the limit.
#[test]
#[ignore = "known limit: nested-write RHS is evaluated before effectful keys"]
fn test_nested_write_effectful_key_runs_before_rhs() {
    let out = compile_and_run(
        r#"<?php
$m = json_decode("{\"a\":{}}", true);
$i = 0;
$m[$i++]["k"] = $i;
echo json_encode($m), "\n";
"#,
    );
    assert_eq!(out, "{\"a\":[],\"0\":{\"k\":1}}\n");
}
