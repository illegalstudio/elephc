//! Purpose:
//! End-to-end regressions for lowering decisions that php takes from the RUN-TIME value rather
//! than from the static type: `Closure::bind()` scopes, callbacks resolved on the object's class,
//! `??` over a property a subclass declares, `foreach` over a subclass that is traversable,
//! internal pointers on non-local places, unpacked integer keys and closed-world `extract()`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each expected text is `php -n`'s output for the same program (php 8.5.10).

use crate::support::*;

/// Only a closure LITERAL is lowered in `Closure::bind()`'s scope: a property read or a static
/// call passed as the closure belongs to the class it is written in.
#[test]
fn test_closure_bind_lowers_a_non_literal_closure_in_the_enclosing_scope() {
    let out = compile_and_run(
        r#"<?php
class B { private $v = 'b-private'; }
class A {
    private $fn;
    function __construct() { $this->fn = function () { return $this->v; }; }
    private static function factory() { return function () { return 'factory'; }; }
    function rebind($o) { $c = \Closure::bind($this->fn, $o, B::class); return $c(); }
    function rebind2() { $c = \Closure::bind(self::factory(), null, B::class); return $c(); }
}
$a = new A();
echo $a->rebind(new B()), "\n";
echo $a->rebind2(), "\n";
"#,
    );
    assert_eq!(out, "b-private\nfactory\n");
}

/// A `[$object, 'method']` callback is resolved on the object's run-time class, which may widen a
/// protected method; an inaccessible one still throws php's `TypeError`, after the arguments ran.
#[test]
fn test_invoke_only_callback_is_resolved_on_the_run_time_class() {
    let out = compile_and_run(
        r#"<?php
class Box { protected function cmp($a, $b) { return $a <=> $b; } }
class OpenBox extends Box { public function cmp($a, $b) { return $a <=> $b; } }
function s(Box $b, array $x) { usort($x, [$b, 'cmp']); return $x; }
echo implode(',', s(new OpenBox(), [3, 1, 2])), "\n";
function effect() { echo "effect\n"; return [2, 1]; }
class Hidden { protected function h($a, $b) { return 0; } }
$h = new Hidden();
try { $r = effect(); usort($r, [$h, 'h']); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { array_map([$h, 'h'], effect()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "1,2,3\neffect\nusort(): Argument #2 ($callback) must be a valid callback, cannot access protected method Hidden::h()\neffect\narray_map(): Argument #1 ($callback) must be a valid callback or null, cannot access protected method Hidden::h()\n"
    );
}

/// `$o->prop ?? $default` is not folded to the default when a class inheriting the receiver's
/// static class declares the property or answers it through `__get()`.
#[test]
fn test_null_coalesce_reads_a_property_only_a_subclass_supplies() {
    let out = compile_and_run(
        r#"<?php
class Base { function label() { return $this->label ?? 'none'; } }
class Child extends Base { public $label = 'kid'; }
echo (new Child())->label(), "\n";
echo (new Base())->label(), "\n";
function f(Base $o) { return $o->extra ?? 'd'; }
class Magic extends Base { function __isset($n) { return true; } function __get($n) { return "magic-$n"; } }
echo f(new Magic()), "\n";
echo f(new Base()), "\n";
"#,
    );
    assert_eq!(out, "kid\nnone\nmagic-extra\nd\n");
}

/// The internal pointer of a stable non-local place (`$cfg['items']`, `$box->items`,
/// `Box::$queue`) is shared by the seeks and reads of one frame.
#[test]
fn test_internal_pointer_of_a_non_local_place_persists_in_the_frame() {
    let out = compile_and_run(
        r#"<?php
function lastKey(array $cfg) { end($cfg['items']); return key($cfg['items']); }
echo lastKey(['items' => ['a' => 1, 'b' => 2, 'c' => 3]]), "\n";
class Box { public $items = [10, 20, 30]; public static $queue = [1, 2, 3]; }
$box = new Box();
next($box->items); echo current($box->items), "\n";
reset(Box::$queue);
while (($v = next(Box::$queue)) !== false) { echo $v, ";"; }
echo "\n";
"#,
    );
    assert_eq!(out, "c\n20\n2;3;\n");
}

/// A by-value `foreach` over an object declared as a non-traversable class or an interface walks
/// the iterator when the run-time object is traversable, and the visible properties otherwise.
#[test]
fn test_foreach_over_a_declared_type_follows_the_run_time_object() {
    let out = compile_and_run(
        r#"<?php
class Bag { public $pub = 'p'; private $priv = 'q'; }
class Coll extends Bag implements IteratorAggregate { private array $items = [1, 2]; function getIterator(): Iterator { return new ArrayIterator($this->items); } }
function walk(Bag $b) { foreach ($b as $k => $v) echo "$k=$v\n"; }
walk(new Coll());
walk(new Bag());
function cnt(Countable $c) { foreach ($c as $v) echo "v=$v\n"; }
cnt(new ArrayObject([7, 8]));
"#,
    );
    assert_eq!(out, "0=1\n1=2\npub=p\nv=7\nv=8\n");
}

/// Unpacking binds the integer-keyed elements to the positions in iteration order, whatever
/// their key values.
#[test]
fn test_spread_binds_integer_keys_in_iteration_order() {
    let out = compile_and_run(
        r#"<?php
function f($a, $b = PHP_EOL) { return "$a|$b"; }
$args = array_filter(['', 'x', 'y']);
echo f(...$args), "\n";
function g($a, $b) { return "$a|$b"; }
$m = [5 => 'p', 9 => 'q'];
echo g(...$m), "\n";
"#,
    );
    assert_eq!(out, "x|y\np|q\n");
}
