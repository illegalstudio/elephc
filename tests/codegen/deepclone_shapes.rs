//! Purpose:
//! Regression tests for the PHP shapes Symfony's DeepClone polyfill runs on: forward `goto` out of
//! nested blocks, runtime-named first-class callables and property references, element references
//! into boxed containers, reflection through boxed reflectors, variadic and closure methods on
//! boxed receivers, spreads that bind by name, the try/catch type join, and stdClass rendering.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected output is `php -n` 8.5's own output for the same fixture.
//! - Fixtures that still leave blocks behind for a known, pre-existing reason assert output only;
//!   the others assert a clean heap.

use crate::support::*;

/// DeepClone's `prepare()` jumps forward to `prepare_value:` and `handle_value:` from ifs nested
/// up to three deep and from inside an inner `foreach`, all within one outer loop body.
#[test]
fn test_forward_gotos_leave_nested_ifs_and_an_inner_loop() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f($items) { $out = []; foreach ($items as $k => $v) { if (is_int($v)) { if ($v > 10) { $v = "big"; goto handle; } goto prep; } foreach ([1, 2] as $j) { if ($v === "skip" && $j === 2) { goto handle; } } $v = strtoupper($v); prep: $v = "[$v]"; handle: $out[$k] = $v; } return $out; } var_dump(f([1, 20, "a", "skip"]));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"array(4) {
  [0]=>
  string(3) "[1]"
  [1]=>
  string(3) "big"
  [2]=>
  string(3) "[A]"
  [3]=>
  string(4) "skip"
}
"#
    );
}

/// A forward goto at function level skips to its label; code between runs only when the jump is
/// not taken.
#[test]
fn test_a_forward_goto_skips_statements_at_function_level() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f($n) { if ($n > 1) { goto big; } echo "small\n"; goto done; big: echo "big\n"; done: echo "end $n\n"; } f(1); f(2);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"small
end 1
big
end 2
"#
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// The segmentation wraps statements in `do { } while (false)` blocks; a `continue`/`break`
/// written in the loop body still targets the loop, not the wrapper.
#[test]
fn test_continue_and_break_still_target_the_right_loop_after_goto_segmentation() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f() { $s = ""; foreach ([1, 2, 3, 4] as $v) { if ($v === 2) { continue; } if ($v === 4) { break; } if ($v === 3) { goto tail; } $s .= "a$v"; tail: $s .= "t$v"; } echo $s, "\n"; } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"a1t1t3
"#
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// `$obj->$name(...)` and `$class::$name(...)` build closures over the method chosen at run time
/// (they did not parse).
#[test]
fn test_a_runtime_named_method_first_class_callable() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public $p = 3; function m($x) { return $x * $this->p; } static function s($x) { return $x + 1; } } function f() { $o = new C; $n = "m"; $g = $o->$n(...); $c = "C"; $s = "s"; $h = $c::$s(...); echo $g(2), " ", $h(2), "\n"; } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"6 3
"#
    );
}

/// `$t = &$c[$k]` where `$c` is an untyped parameter binds into the array the box holds.
#[test]
fn test_an_element_reference_into_a_boxed_container() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f($c) { $k = "a"; $t = &$c[$k]; $t = 5; $t2 = &$c["b"]; $t2 = 6; return $c; } var_dump(f(["a" => 1]));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"array(2) {
  ["a"]=>
  int(5)
  ["b"]=>
  int(6)
}
"#
    );
}

/// A by-reference foreach over an untyped parameter that stores `&$value` elsewhere shares the
/// element (it crashed on an alias of raw storage).
#[test]
fn test_a_by_reference_foreach_over_a_boxed_parameter() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f($vars) { $scoped = []; foreach ($vars as $name => &$value) { $scoped[$name] = &$value; } unset($value); $scoped["x"] = 100; var_dump($vars); } f(["x" => 1, "y" => 2]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"array(2) {
  ["x"]=>
  &int(100)
  ["y"]=>
  &int(2)
}
"#
    );
}

/// `$obj->$name = &$x` makes the property and the variable one reference.
#[test]
fn test_a_runtime_named_property_bound_by_reference() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f() { $o = new stdClass; $x = 1; $n = "p"; $o->$n = &$x; $x = 2; echo $o->p, "\n"; $o->p = 3; echo $x, "\n"; } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"2
3
"#
    );
}

/// A `use (&$acc)` capture of a local that has no value yet is typed from what the closure stores
/// into it.
#[test]
fn test_a_closure_capture_by_reference_of_an_unset_local() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f() { $fn = function ($v) use (&$acc) { $acc[] = $v; }; $fn(1); $fn(2); return $acc; } var_dump(f());
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
"#
    );
}

/// `spl_object_id()` accepts a closure, and the id is stable for the same closure.
#[test]
fn test_spl_object_id_of_a_closure() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f() { $g = function () { return 1; }; $h = $g; var_dump(spl_object_id($g) === spl_object_id($h), is_int(spl_object_id($g))); } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"bool(true)
bool(true)
"#
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// `isCloneable()` and `isIterable()` answered false for every compiled class (the flags word
/// never carried them).
#[test]
fn test_reflection_reports_cloneable_and_iterable() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class U {} class NoClone { private function __clone() {} } class It implements IteratorAggregate { function getIterator(): Iterator { return new ArrayIterator([]); } } abstract class A {} function f() { foreach (["stdClass", "U", "NoClone", "It", "A"] as $c) { $r = new ReflectionClass($c); echo $c, " ", var_export($r->isCloneable(), true), " ", var_export($r->isIterable(), true), "\n"; } } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"stdClass true false
U true false
NoClone false false
It true true
A false false
"#
    );
}

/// A reflector kept in an array reaches the method bodies: `newInstanceWithoutConstructor()`
/// returned null, `newInstance()` and `newInstanceArgs()` fataled or dropped their arguments.
#[test]
fn test_reflection_construction_through_a_boxed_reflector() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public function __construct(public $a = 0, public $b = "x") {} } function f() { $r = [new ReflectionClass("P")][0]; var_dump($r->newInstanceWithoutConstructor(), $r->newInstance(5, "y"), $r->newInstanceArgs([7]), $r->newInstanceArgs(["b" => "named"])); } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"object(P)#2 (2) {
  ["a"]=>
  NULL
  ["b"]=>
  NULL
}
object(P)#3 (2) {
  ["a"]=>
  int(5)
  ["b"]=>
  string(1) "y"
}
object(P)#4 (2) {
  ["a"]=>
  int(7)
  ["b"]=>
  string(1) "x"
}
object(P)#5 (2) {
  ["a"]=>
  int(0)
  ["b"]=>
  string(5) "named"
}
"#
    );
}

/// `newInstanceArgs($args)` and `newInstance(...$args)` with an array only known at run time (a
/// compile-time "unsupported" fatal before).
#[test]
fn test_reflection_construction_with_run_time_arguments() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public function __construct(public $a = 0, public $b = "x") {} } function g(ReflectionClass $r, array $args) { return [$r->newInstanceArgs($args), $r->newInstance(...$args)]; } function f() { $r = new ReflectionClass("P"); echo json_encode([g($r, [1, 2]), g($r, ["b" => "n"])]), "\n"; } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"[[{"a":1,"b":2},{"a":1,"b":2}],[{"a":0,"b":"n"},{"a":0,"b":"n"}]]
"#
    );
}

/// DeepClone's cache shape: `self::$reflectors[$c] ??= ...` then a method call on the cached
/// reflector.
#[test]
fn test_a_reflector_kept_in_a_static_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class H { public static array $reflectors = []; static function make($class) { self::$reflectors[$class] ??= new ReflectionClass($class); return self::$reflectors[$class]->newInstanceWithoutConstructor(); } } function f() { $o = H::make("stdClass"); $o->k = 1; var_dump($o); } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"object(stdClass)#2 (1) {
  ["k"]=>
  int(1)
}
"#
    );
}

/// Built without its constructor, an untyped promoted property is null, as any untyped property
/// is (it dumped `uninitialized(mixed)`).
#[test]
fn test_an_untyped_promoted_property_is_born_null() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public function __construct(public $a = 0, public int $b = 1) {} } function f() { $o = (new ReflectionClass("P"))->newInstanceWithoutConstructor(); print_r($o); echo "\n"; var_dump(isset($o->a), array_key_exists("a", get_object_vars($o))); } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"P Object
(
    [a] => 
)

bool(false)
bool(true)
"#
    );
}

/// A variadic method called on a receiver only known as `mixed` (it fataled "on null": the
/// class-id dispatch cannot gather a variadic tail).
#[test]
fn test_a_variadic_method_through_a_boxed_receiver() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class V { function m($x = "d", ...$a) { return $x . "|" . count($a) . ":" . implode(",", $a); } } class W { function m($x = "w") { return "W$x"; } } function t($o) { return [$o->m(), $o->m(1, 2, 3), $o->m(x: "n"), $o->m(...[4, 5])]; } function f() { var_dump(t(new V), t(new W)); } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"array(4) {
  [0]=>
  string(4) "d|0:"
  [1]=>
  string(7) "1|2:2,3"
  [2]=>
  string(4) "n|0:"
  [3]=>
  string(5) "4|1:5"
}
array(4) {
  [0]=>
  string(2) "Ww"
  [1]=>
  string(2) "W1"
  [2]=>
  string(2) "Wn"
  [3]=>
  string(2) "W4"
}
"#
    );
}

/// `bindTo()`, `Closure::bind()` and `__invoke()` on a closure read back from a static property
/// or a `??=` (the eval bridge refused them).
#[test]
fn test_closure_rebinding_through_a_boxed_closure() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Point { public $x; protected $y; private $z; } class H { public static $one; public static $hs = []; } function f() { $m = H::$hs["k"] ??= static function ($o, $n, $v) { $o->$n = $v; }; $p = new Point; $b = $m->bindTo(null, "Point"); $b($p, "z", 9); H::$one = static function ($o) { return $o + 4; }; $g = Closure::bind(H::$one, null, "H"); var_dump($p, $g(1), H::$one->__invoke(2)); } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"object(Point)#2 (3) {
  ["x"]=>
  NULL
  ["y":protected]=>
  NULL
  ["z":"Point":private]=>
  int(9)
}
int(5)
int(6)
"#
    );
}

/// A spread binds each key: an integer to its position, a string to the parameter of that name,
/// in the same unpack, for every source spelling.
#[test]
fn test_a_spread_binds_names_and_positions_per_key() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public function __construct(public $a = 0, public $b = "x") {} } function g($a = 0, $b = "x") { return "$a/$b"; } function viaf(array $xs) { return g(...$xs); } function vian(array $xs) { return new P(...$xs); } function viad($c, array $xs) { return new $c(...$xs); } function viau($c, $xs) { return new $c(...$xs); } function f() { echo viaf([1, 2]), " ", viaf(["b" => "n"]), " ", viaf([5, "b" => "m"]), "\n"; echo json_encode([vian(["b" => "n"]), viad("P", ["b" => "n"]), viau("P", ["b" => "n"]), viau("P", [3, 4])]), "\n"; } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"1/2 0/n 5/m
[{"a":0,"b":"n"},{"a":0,"b":"n"},{"a":0,"b":"n"},{"a":3,"b":4}]
"#
    );
}

/// A variadic parameter that received a named argument (typed `iterable`) spreads into `new
/// $class(...)` by name and position.
#[test]
fn test_a_variadic_parameter_spread_into_a_dynamic_new() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public function __construct(public $a = 0, public $b = "x") {} } class R { public $n = "P"; function k(...$args) { return new ($this->n)(...$args); } } function w($c, ...$args) { return new $c(...$args); } function f() { echo json_encode([w("P", b: "n"), (new R)->k(b: "n"), (new R)->k(4, 5)]), "\n"; } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"[{"a":0,"b":"n"},{"a":0,"b":"n"},{"a":4,"b":5}]
"#
    );
}

/// After `try { $p = <object>; } catch (E) { $p = "O:"; }` the variable reads the object on the
/// try path (it read the catch branch's string, so the object came out as "").
#[test]
fn test_a_variable_assigned_in_both_try_and_catch_keeps_both_types() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function mk($c) { return $c === "x" ? throw new RuntimeException("no") : (new ReflectionClass($c))->newInstanceWithoutConstructor(); } function f($c) { try { $proto = mk($c); } catch (RuntimeException) { $proto = "O:"; } echo is_object($proto) ? get_class($proto) : $proto, "\n"; } f("stdClass"); f("x");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"stdClass
O:
"#
    );
}

/// A hint-less function or method whose returns are an array and a string used to infer `string`
/// and convert the array ("Array" plus a warning) -- Twig's `first`/`last` filters over `slice()`
/// among them.
#[test]
fn test_a_function_returning_an_array_or_a_string_returns_the_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function sl($item) { if (is_array($item)) { return array_slice($item, 0, 1); } return substr((string) $item, 0, 1); } class C { public static function sl($item) { if (is_array($item)) { return [1]; } return "s"; } public static function first($item) { $e = self::sl($item); return is_string($e) ? $e : current($e); } } function f() { var_dump(sl([5, 6]), sl("xy"), C::sl([]), C::first([]), C::first("q")); } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"array(1) {
  [0]=>
  int(5)
}
string(1) "x"
array(1) {
  [0]=>
  int(1)
}
int(1)
string(1) "s"
"#
    );
}

/// The element type of an untyped variadic joins over every call; it was fixed by the calls seen
/// first, so `v(); v("s")` and `v(1); v("s")` read each string's pointer as an integer.
#[test]
fn test_a_variadic_element_type_joins_every_call() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function v(...$r) { var_dump($r); } function w(...$r) { var_dump($r); } function f() { v(); v("y", "z"); w(1); w("s"); } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"array(0) {
}
array(2) {
  [0]=>
  string(1) "y"
  [1]=>
  string(1) "z"
}
array(1) {
  [0]=>
  int(1)
}
array(1) {
  [0]=>
  string(1) "s"
}
"#
    );
}

/// `f($fixed, $extra, ...$rest)` packs the extras and the spread into the variadic tail (the call
/// reached codegen with more operands than parameters) -- polyfill-mbstring forwards `$var,
/// ...$vars` this way.
#[test]
fn test_a_spread_after_plain_arguments_fills_the_variadic() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class M { public static function c($to, ...$vars) { return $to . count($vars) . implode(",", $vars); } } function c2($to, ...$vars) { return $to . count($vars) . implode(",", $vars); } function f(array $rest) { $a = "x"; echo M::c("T", $a, ...$rest), " ", c2("T", $a, ...$rest), " ", c2("U", ...$rest), "\n"; } f([]); f(["y", 2]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"T1x T1x U0
T3x,y,2 T3x,y,2 U2y,2
"#
    );
}

/// A parameter typed `array|string|null`, narrowed by `is_array()` and mapped, is written to an
/// untyped static list (the store did not compile) -- polyfill-mbstring's `mb_detect_order()`.
#[test]
fn test_a_narrowed_list_written_to_a_static_property() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class M { private static $list = ["ASCII", "UTF-8"]; public static function order($l = null) { if (null === $l) { return self::$list; } if (!is_array($l)) { $l = array_map("trim", explode(",", $l)); } $l = array_map("strtoupper", $l); self::$list = $l; return true; } } function order(array|string|null $e = null): array|bool { return M::order($e); } function f() { var_dump(order(), order("a, b"), order(), order(["x"]), order()); } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"array(2) {
  [0]=>
  string(5) "ASCII"
  [1]=>
  string(5) "UTF-8"
}
bool(true)
array(2) {
  [0]=>
  string(1) "A"
  [1]=>
  string(1) "B"
}
bool(true)
array(1) {
  [0]=>
  string(1) "X"
}
"#
    );
}

/// A stdClass keeps its properties in a hash; `var_dump` and `print_r` rendered every stdClass
/// empty, whatever the spelling that built it.
#[test]
fn test_var_dump_and_print_r_render_stdclass_properties() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f() { $o = new stdClass; $o->x = 1; $o->list = [1, "a"]; $n = "dyn"; $o->$n = true; $m = [$o][0]; $c = clone $m; $c->y = 2.5; var_dump($o, $c); print_r($o); echo "\n"; $e = new stdClass; var_dump($e); print_r($e); echo "\n"; } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"object(stdClass)#1 (3) {
  ["x"]=>
  int(1)
  ["list"]=>
  array(2) {
    [0]=>
    int(1)
    [1]=>
    string(1) "a"
  }
  ["dyn"]=>
  bool(true)
}
object(stdClass)#2 (4) {
  ["x"]=>
  int(1)
  ["list"]=>
  array(2) {
    [0]=>
    int(1)
    [1]=>
    string(1) "a"
  }
  ["dyn"]=>
  bool(true)
  ["y"]=>
  float(2.5)
}
stdClass Object
(
    [x] => 1
    [list] => Array
        (
            [0] => 1
            [1] => a
        )

    [dyn] => 1
)

object(stdClass)#3 (0) {
}
stdClass Object
(
)

"#
    );
}

/// A stdClass from `newInstanceWithoutConstructor()` gets its property hash (a first write
/// faulted on the null hash).
#[test]
fn test_a_stdclass_built_without_its_constructor_holds_properties() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f() { $o = (new ReflectionClass("stdClass"))->newInstanceWithoutConstructor(); $o->y = 2; echo $o->y, json_encode($o), count(get_object_vars($o)), "\n"; } f();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"2{"y":2}1
"#
    );
}

/// A closure rebound to a scope that is only a string at run time sees that scope's private
/// members: `bindTo`, `Closure::bind` with a variable scope, a literal closure bound to a variable
/// scope, and a scope passed through a parameter. A subclass scope cannot see its parent's private.
/// This is DeepClone's `getHydrator()` shape (`$baseHydrator->bindTo(null, $class)`). Output only:
/// a runtime-named property write leaks its key string independent of scope (pre-existing).
#[test]
fn test_closure_bound_to_a_runtime_class_name_scope_reaches_private_members() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class D { private bool $a = false; public function a() { return $this->a; } }
class E extends D {}
$h = static function ($props, $obj) { foreach ($props as $n => $v) { $obj->$n = $v; } };
foreach (['D', 'E'] as $i => $cls) {
    $b = $i === 0 ? $h->bindTo(null, $cls) : \Closure::bind($h, null, $cls);
    $o = new D;
    try { $b(['a' => true], $o); var_dump($o->a()); } catch (Error $e) { echo $e->getMessage(), "\n"; }
}
$name = 'D'; $lit2 = \Closure::bind(static function ($o) { $o->a = true; }, null, $name); $o = new D; $lit2($o); var_dump($o->a());
class Holder { public static function hydrator(string $cls) { $base = static function ($props, $o) { foreach ($props as $n => $v) { $o->$n = $v; } }; return $base->bindTo(null, $cls); } }
$o = new D; (Holder::hydrator('D'))(['a' => true], $o); var_dump($o->a());
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(true)\nCannot access private property D::$a\nbool(true)\nbool(true)\n"
    );
}

/// The runtime scope decides private AND protected visibility exactly like a method of that class:
/// an ancestor scope reaches a descendant object's protected and its own private members, a
/// descendant scope reaches only the protected ones, an unrelated scope neither, and an object given
/// as the scope names its class. A closure with no runtime scope keeps its lexical one.
#[test]
fn test_runtime_closure_scope_decides_protected_and_private_visibility() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { protected int $p = 1; private string $s = "ps"; public function g() { return $this->p . $this->s; } }
class C extends P { private string $own = "o"; public function own() { return $this->own; } }
class U {}
$set = static function ($obj, $n, $v) { $obj->$n = $v; return $obj; };
$get = static function ($obj, $n) { return $obj->$n; };
foreach ([['P', 'C', ['p', 's']], ['C', 'C', ['p']], ['U', 'C', ['p']], ['P', 'P', ['p', 's']], ['C', 'P', ['p', 's']]] as [$scope, $cls, $names]) {
    $s = \Closure::bind($set, null, $scope);
    $g = \Closure::bind($get, null, $scope);
    foreach ($names as $n) {
        $o = new $cls;
        try { $s($o, $n, $n === 'p' ? 7 : 'x'); $r = "ok " . $o->g(); } catch (Error $e) { $r = $e->getMessage(); }
        echo "$scope/$cls set $n: $r\n";
        try { $r = $g($o, $n); } catch (Error $e) { $r = $e->getMessage(); }
        echo "$scope/$cls get $n: $r\n";
    }
}
$s = $set->bindTo(null, new C);
$o = new C;
try { $s($o, 'own', 'q'); echo $o->own(), "\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { $set(new C, 'own', 'q'); echo "unbound ok\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
class W { private $z = 3; public function mk() { return function ($o, $n) { return $o->$n; }; } }
$f = (new W)->mk();
echo $f(new W, 'z'), "\n";
try { echo $f(new C, 'own'), "\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
$rebound = $f->bindTo(null, 'C');
echo $rebound(new C, 'own'), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"P/C set p: ok 7ps
P/C get p: 7
P/C set s: ok 1x
P/C get s: x
C/C set p: ok 7ps
C/C get p: 7
U/C set p: Cannot access protected property C::$p
U/C get p: Cannot access protected property C::$p
P/P set p: ok 7ps
P/P get p: 7
P/P set s: ok 1x
P/P get s: x
C/P set p: ok 7ps
C/P get p: 7
C/P set s: Cannot access private property P::$s
C/P get s: Cannot access private property P::$s
q
Cannot access private property C::$own
3
Cannot access private property C::$own
o
"#
    );
}
