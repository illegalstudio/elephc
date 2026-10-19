//! Purpose:
//! End-to-end tests for references to a NEW array element (`&$a[]`, `$a[] = &$x`,
//! `foreach ($a as &$d[])`) and for references bound through a runtime property name
//! (`$o->{$k} = &$v`, `$o->{$k}['x'] = &$v`, `$o->{$k}[] = &$v`).
//!
//! Called from:
//! - `cargo test --test codegen_tests reference_append` through Rust's test harness.
//!
//! Key details:
//! - Every expected output was produced by php 8.5 on the same source (deprecations go to
//!   stderr in both, so only stdout is compared).
//! - The loop test runs under `--heap-debug` and must report a clean heap.

use crate::support::*;

/// `$r = &$a[]` and `$r = &$a['k'][]`: the new element is created as null and aliased.
#[test]
fn test_reference_new_element_local_and_nested() {
    let out = compile_and_run(
        r#"<?php
// $r = &$a[] creates a null element and binds $r to it.
$a = [1, 2];
$r = &$a[];
var_dump($a);
$r = 'x';
var_dump($a);
$a2 = ['k' => [5]];
$s = &$a2['k'][];
$s = 6;
$t = &$a2['n'][];
$t = 7;
var_dump($a2);
$h = ['z' => 1, 10 => 2];
$q = &$h[];
$q = 'eleven';
var_dump($h);
"#,
    );
    assert_eq!(
        out,
        r#"array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  &NULL
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  &string(1) "x"
}
array(2) {
  ["k"]=>
  array(2) {
    [0]=>
    int(5)
    [1]=>
    &int(6)
  }
  ["n"]=>
  array(1) {
    [0]=>
    &int(7)
  }
}
array(3) {
  ["z"]=>
  int(1)
  [10]=>
  int(2)
  [11]=>
  &string(6) "eleven"
}
"#
    );
}

/// nette `Arrays::associate`: `$x = &$x[]` walks down and creates one level per step.
#[test]
fn test_reference_new_element_walk_creates_levels() {
    let out = compile_and_run(
        r#"<?php
// nette Arrays::associate shape: $x = &$x[] walks down and creates new levels.
function root(bool $object): mixed { return $object ? new stdClass : []; }
$res = root(false);
foreach ([['a' => 1], ['a' => 2]] as $row) {
    $x = &$res;
    $x = &$x[];
    $x = $row['a'];
    unset($x);
}
var_dump($res);
$tree = root(false);
$x = &$tree;
for ($i = 0; $i < 3; $i++) {
    $x = &$x[];
}
$x = 'leaf';
unset($x);
var_dump($tree);
"#,
    );
    assert_eq!(
        out,
        r#"array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
array(1) {
  [0]=>
  array(1) {
    [0]=>
    array(1) {
      [0]=>
      string(4) "leaf"
    }
  }
}
"#
    );
}

/// symfony `EventDispatcher::optimizeListeners`: `$c = &$this->optimized[$e][]` in a loop.
#[test]
fn test_reference_new_element_through_property() {
    let out = compile_and_run(
        r#"<?php
// symfony EventDispatcher::optimizeListeners shape.
class D {
    public array $optimized = [];
    public $loose;
    public function opt(string $e, array $ls): array {
        $this->optimized[$e] = [];
        foreach ($ls as $l) {
            $closure = &$this->optimized[$e][];
            $closure = strtoupper($l);
        }
        $r = &$this->loose[];
        $r = 'auto';
        return $this->optimized;
    }
}
$d = new D();
var_dump($d->opt('ev', ['a', 'b']));
var_dump($d->loose);
"#,
    );
    assert_eq!(
        out,
        r#"array(1) {
  ["ev"]=>
  array(2) {
    [0]=>
    string(1) "A"
    [1]=>
    string(1) "B"
  }
}
array(1) {
  [0]=>
  string(4) "auto"
}
"#
    );
}

/// `$a[] = &$x`, `$m['k'][] = &$undefined` and `$obj->items[] = &$y`.
#[test]
fn test_reference_append_target() {
    let out = compile_and_run(
        r#"<?php
// $a[] = &$x: the new element is bound to a variable (debugbar, sebastian).
$x = 1;
$a = [];
$a[] = &$x;
$a[] = &$x;
$x = 2;
var_dump($a);
$m = ['k' => []];
$m['k'][] = &$undefinedYet;
$undefinedYet = 'now';
var_dump($m);
class Bag { public array $items = []; }
$b = new Bag();
$y = 'y0';
$b->items[] = &$y;
$y = 'y1';
var_dump($b->items);
"#,
    );
    assert_eq!(
        out,
        r#"array(2) {
  [0]=>
  &int(2)
  [1]=>
  &int(2)
}
array(1) {
  ["k"]=>
  array(1) {
    [0]=>
    &string(3) "now"
  }
}
array(1) {
  [0]=>
  &string(2) "y1"
}
"#
    );
}

/// `foreach ($a as $c[])` and `foreach ($a as &$d[])`.
#[test]
fn test_reference_foreach_into_new_element() {
    let out = compile_and_run(
        r#"<?php
// $a[] as a foreach target, by value and by reference.
$c = [];
foreach ([1, 2, 3] as $c[]) {}
var_dump($c);
$d = [];
$src = [[1], [2]];
foreach ($src as &$d[]) {}
$d[0][] = 9;
var_dump($src);
"#,
    );
    assert_eq!(
        out,
        r#"array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
}
array(2) {
  [0]=>
  &array(2) {
    [0]=>
    int(1)
    [1]=>
    int(9)
  }
  [1]=>
  &array(1) {
    [0]=>
    int(2)
  }
}
"#
    );
}

/// `$o->{$k} = &$v` / `$o->$k = &$v` on declared, dynamic and stdClass properties, and the element forms.
#[test]
fn test_reference_dynamic_property_bind() {
    let out = compile_and_run(
        r#"<?php
// $o->{$k} = &$v and $o->$k = &$v (laravel serializable-closure Native.php).
class P { public $decl = 0; public array $arr = []; }
$o = new P();
$v = 1;
$k = 'decl';
$o->{$k} = &$v;
$v = 2;
var_dump($o->decl);
$k2 = 'dyn';
$w = 'w0';
$o->$k2 = &$w;
$w = 'w1';
var_dump($o->{$k2});
$s = new stdClass();
foreach (['a', 'b'] as $key) {
    $s->{$key} = &$v;
}
$v = 3;
var_dump($s);
$n = 'arr';
$o->{$n}[] = &$v;
$o->{$n}['x'] = &$w;
$v = 4; $w = 'w2';
var_dump($o->arr);
"#,
    );
    assert_eq!(
        out,
        r#"int(2)
string(2) "w1"
object(stdClass)#2 (2) {
  ["a"]=>
  &int(3)
  ["b"]=>
  &int(3)
}
array(2) {
  [0]=>
  &int(4)
  ["x"]=>
  &string(2) "w2"
}
"#
    );
}

/// A runtime-named reference keeps PHP's visibility and readonly `Error`s and reaches subclass slots.
#[test]
fn test_reference_dynamic_property_access_errors() {
    let out = compile_and_run(
        r#"<?php
class P { public $u = 1; private $priv = 0; public readonly array $ro; public int $t = 0; public function __construct() { $this->ro = []; } }
class Q extends P { public $sub = 's'; }
function bind(P $o, string $k, &$v) {
    try { $o->{$k} = &$v; echo "bound $k\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
}
function bindElement(P $o, string $k, &$v) {
    try { $o->{$k}['x'] = &$v; echo "bound element $k\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
}
$x = 5;
$p = new P();
bind($p, 'u', $x);
bind($p, 'priv', $x);
bind($p, 'ro', $x);
bindElement($p, 'ro', $x);
$q = new Q();
bind($q, 'sub', $x);
$x = 6;
var_dump($p->u, $q->sub);
"#,
    );
    assert_eq!(
        out,
        r#"bound u
Error: Cannot access private property P::$priv
Error: Cannot indirectly modify readonly property P::$ro
Error: Cannot indirectly modify readonly property P::$ro
bound sub
int(6)
int(6)
"#
    );
}

/// laravel serializable-closure `Native::mapPointers` on a receiver of unknown class narrowed to stdClass.
#[test]
fn test_reference_dynamic_property_mixed_receiver() {
    let out = compile_and_run(
        r#"<?php
class Holder { public $closure = 'c0'; }
function mapPointers($data, Holder $h, array $keys) {
    if ($data instanceof \stdClass) {
        foreach ($keys as $key) {
            $data->{$key} = &$h->closure;
        }
    }
}
$h = new Holder();
$d = new stdClass();
$d->a = 'self';
$d->b = 'x';
mapPointers($d, $h, ['a', 'n']);
$h->closure = 'c1';
var_dump($d);
echo json_encode($d), "\n";
$d->a = 'w';
echo $h->closure, " ", $d->n, "\n";
$c = clone $d;
$h->closure = 'c2';
echo $c->a, "\n";
unset($d->a);
echo $h->closure, " ", count((array) $d), "\n";
"#,
    );
    assert_eq!(
        out,
        r#"object(stdClass)#2 (3) {
  ["a"]=>
  &string(2) "c1"
  ["b"]=>
  string(1) "x"
  ["n"]=>
  &string(2) "c1"
}
{"a":"c1","b":"x","n":"c1"}
w w
c2
c2 2
"#
    );
}

/// Every form in a 2000-iteration loop: the heap stays clean (no reference cell, key string or
/// container copy outlives its iteration).
#[test]
fn test_reference_append_forms_do_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class D { public array $optimized = []; public $loose; }
$total = 0;
for ($i = 0; $i < 2000; $i++) {
    $a = [1];
    $r = &$a[];
    $r = $i;
    $tree = (function (): mixed { return []; })();
    $x = &$tree;
    $x = &$x[];
    $x = &$x[];
    $x = 'leaf';
    unset($x);
    $d = new D();
    $d->optimized['e'] = [];
    $c = &$d->optimized['e'][];
    $c = 'l';
    $y = $i;
    $a[] = &$y;
    $s = new stdClass();
    $k = 'p' . ($i % 3);
    $s->{$k} = &$y;
    $s->{$k . 'l'}[] = &$y;
    $n = 'loose';
    $d->{$n}['x'] = &$y;
    $total += count($a) + count($d->optimized['e']) + count((array) $s) + count($tree);
    unset($r, $c, $y);
}
echo $total, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "14000\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$this->p[$k][] = $v` on a property container released its probe read: before, every nested
/// property append leaked the outer container (five blocks per iteration here).
#[test]
fn test_nested_property_append_releases_container_probe() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class D { public array $o = []; public $u = []; public static array $s = []; }
$d = new D();
for ($i = 0; $i < 100; $i++) {
    $d->o = ['e' => []];
    $d->o['e'][] = 's';
    $d->u = ['e' => []];
    $d->u['e'][] = 's';
    $d->u['n'][] = $i;
    D::$s = ['e' => []];
    D::$s['e'][] = 's';
}
echo count($d->o['e']), count($d->u), count(D::$s['e']), "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "121\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$a = []; $a[] = null;` gives the list a boxed element instead of an unsupported `Void` slot.
#[test]
fn test_push_null_into_empty_list() {
    let out = compile_and_run(
        r#"<?php
function f() { $r = []; $r[] = null; $r[] = 2; return $r; }
$g = [];
$g[] = null;
var_dump(f(), $g);
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  NULL\n  [1]=>\n  int(2)\n}\narray(1) {\n  [0]=>\n  NULL\n}\n"
    );
}

/// `$r = &$o->{$k}` / `$r = &$o->$k` on declared, dynamic and stdClass properties; a missing one is created as null.
#[test]
fn test_reference_dynamic_property_source() {
    let out = compile_and_run(
        r#"<?php
// $r = &$o->{$k} and $r = &$o->$k on declared, dynamic and stdClass properties.
class P { public $decl = 1; }
$o = new P();
$k = 'decl';
$r = &$o->{$k};
$r = 2;
var_dump($o->decl);
$s = new stdClass();
$s->a = 'a0';
$n = 'a';
$t = &$s->$n;
$t = 'a1';
$m = 'missing';
$u = &$s->{$m};
var_dump($u);
$u = 'created';
var_dump($s);
$d = 'dyn';
$w = &$o->{$d};
$w = 'w';
var_dump($o->{$d});
"#,
    );
    assert_eq!(
        out,
        r#"int(2)
NULL
object(stdClass)#2 (2) {
  ["a"]=>
  &string(2) "a1"
  ["missing"]=>
  &string(7) "created"
}
string(1) "w"
"#
    );
}

/// nette `Arrays::associate`: `$x = &$x->{$k}` walks and creates properties, and `$r = &$o->{$k}[$i]`.
#[test]
fn test_reference_dynamic_property_source_walk() {
    let out = compile_and_run(
        r#"<?php
// nette Arrays::associate: $x = &$x->{$k} walks and creates properties, and $r = &$o->{$k}[$i].
$res = new stdClass();
foreach ([['id' => 'a', 'v' => 1], ['id' => 'b', 'v' => 2]] as $row) {
    $x = &$res;
    $x = &$x->{$row['id']};
    if ($x === null) {
        $x = new stdClass();
    }
    $x = &$x->{'value'};
    $x = $row['v'];
    unset($x);
}
var_dump($res);
$s = new stdClass();
$s->list = [10, 20];
$k = 'list';
$e = &$s->{$k}[1];
$e = 21;
$f = &$s->{$k}['new'];
$f = 'n';
unset($e, $f);
var_dump($s);
"#,
    );
    assert_eq!(
        out,
        r#"object(stdClass)#1 (2) {
  ["a"]=>
  object(stdClass)#2 (1) {
    ["value"]=>
    int(1)
  }
  ["b"]=>
  object(stdClass)#3 (1) {
    ["value"]=>
    int(2)
  }
}
object(stdClass)#4 (1) {
  ["list"]=>
  array(3) {
    [0]=>
    int(10)
    [1]=>
    int(21)
    ["new"]=>
    string(1) "n"
  }
}
"#
    );
}

/// A runtime-named reference source to a readonly property throws PHP's Error.
#[test]
fn test_reference_dynamic_property_source_readonly() {
    let out = compile_and_run(
        r#"<?php
// A reference to a readonly property through a dynamic name throws Error.
final class R {
    public function __construct(public readonly array $v = [1]) {}
    public function take(string $k) {
        try { $r = &$this->{$k}; echo "taken\n"; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
        try { $r = &$this->{$k}[0]; echo "taken\n"; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
    }
}
(new R())->take('v');
"#,
    );
    assert_eq!(
        out,
        r#"Error: Cannot indirectly modify readonly property R::$v
Error: Cannot indirectly modify readonly property R::$v
"#
    );
}

/// Every runtime-named source form in a 2000-iteration loop leaves a clean heap.
#[test]
fn test_reference_dynamic_property_source_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public $decl = 1; }
$total = 0;
for ($i = 0; $i < 2000; $i++) {
    $o = new P();
    $k = 'decl';
    $r = &$o->{$k};
    $r = $i;
    $s = new stdClass();
    $n = 'p' . ($i % 3);
    $t = &$s->{$n};
    $t = 'x';
    $s->l = [1, 2];
    $e = &$s->{'l'}[1];
    $e = $i;
    $res = new stdClass();
    $x = &$res;
    $x = &$x->{'a' . $i};
    $x = new stdClass();
    $x = &$x->{'v'};
    $x = $i;
    unset($x, $r, $t, $e);
    $total += $o->decl + $s->l[1] + count((array) $res);
}
echo $total, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "4000000\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An element reference (or a by-reference foreach) through a PHP variable that aliases a list
/// by reference binds when the list's elements are mixed. The TYPED-list shape
/// (`$x = &$res; $y = &$x[0];` with `$res = [1]`) used to crash and is now refused at lowering;
/// `ir_lower::tests::reference_alias_promotion` pins that refusal.
#[test]
fn test_reference_element_through_alias_of_mixed_list() {
    let out = compile_and_run(
        r#"<?php
$res = [1, 'a'];
$x = &$res;
$y = &$x[0];
$y = 5;
foreach ($x as &$v) { $v = $v . '!'; }
unset($v);
echo implode(",", $res), "\n";
"#,
    );
    assert_eq!(out, "5!,a!\n");
}

/// A runtime-named reference that PHP would route to `__get`/`__set` (an undeclared or
/// inaccessible name on a class with magic accessors) throws an explicit elephc `Error` instead
/// of binding a dynamic property; a declared accessible property still binds.
#[test]
fn test_reference_dynamic_property_refuses_magic_accessors() {
    let out = compile_and_run(
        r#"<?php
class M {
    public $d = 'd';
    private $hid = 'H';
    function __get($n) { return 'G'; }
    function __set($n, $v) {}
}
class X extends stdClass { function __get($n) { return 'G'; } }
function bind($o, string $k, &$v) {
    try { $o->{$k} = &$v; echo "bound $k\n"; } catch (Error $e) { echo substr($e->getMessage(), 0, 60), "\n"; }
}
function take(M $o, string $k) {
    try { $r = &$o->{$k}; echo "taken $k\n"; } catch (Error $e) { echo substr($e->getMessage(), 0, 60), "\n"; }
}
$v = 1;
$m = new M();
take($m, 'd');
take($m, 'zz');
take($m, 'hid');
bind($m, 'zz', $v);
bind(new X(), 'zz', $v);
$s = new stdClass();
bind($s, 'a', $v);
"#,
    );
    assert_eq!(
        out,
        r#"taken d
elephc: a reference through a runtime property name (`$obj->
elephc: a reference through a runtime property name (`$obj->
elephc: a reference bound through a runtime property name (`
elephc: a reference bound through a runtime property name (`
bound a
"#
    );
}

/// `&$c->t[]` and `$c->t[] = &$x` on an uninitialized typed `array` property raise PHP's
/// uninitialized-property `Error` (a reference slot keeps its marker inside the cell).
#[test]
fn test_reference_new_element_of_uninitialized_typed_property() {
    let out = compile_and_run(
        r#"<?php
class C { public array $t; }
$c = new C;
try { $r = &$c->t[]; $r = 1; } catch (Error $e) { echo $e->getMessage(), "\n"; }
$x = 1;
try { $c->t[] = &$x; } catch (Error $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "Typed property C::$t must not be accessed before initialization\nTyped property C::$t must not be accessed before initialization\n"
    );
}
