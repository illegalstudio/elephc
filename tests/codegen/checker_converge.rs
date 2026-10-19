//! Purpose:
//! End-to-end coverage for the checker's fixpoints widening by a lattice JOIN
//! (`crate::types::checker::lattice`), so they converge on large codebases, and untyped
//! parameters join every call-site type, closures included.
//!
//! Called from:
//! - The native codegen suite (`tests/codegen/mod.rs`).
//!
//! Key details:
//! - The expected output was produced by PHP 8.5 from the same source.

use crate::support::*;

/// `__callStatic`'s declared `array $parameters` seen with an int, a float and an int again joins to `array<mixed>` instead of re-adopting `array<int>` (Carbon).
#[test]
fn test_checker_converge_magic_callstatic_int_float_widen() {
    let out = compile_and_run(r#"<?php
// Repro: __callStatic's declared `array $parameters` seen with int and float.
class CI
{
    public static function __callStatic(string $method, array $parameters)
    {
        return $method . ":" . $parameters[0];
    }
}
function later() { return CI::secs(3); }
echo CI::days(1), "\n";
echo CI::hours(1.5), "\n";
echo CI::mins(2), " ", later(), "\n";
"#);
    assert_eq!(out, "days:1\nhours:1.5\nmins:2 secs:3\n");
}

/// An untyped property filled by constructor calls with a parent and a child class joins to the parent, and scalars of different tags join to mixed (HttpResponseException).
#[test]
fn test_checker_converge_ctor_untyped_property_joins_parent_and_child() {
    let out = compile_and_run(r#"<?php
// Repro: an untyped property filled from constructor calls with parent and child.
class R { public function name() { return "R"; } }
class IR extends R { public function name() { return "IR"; } }
class HRE
{
    protected $response;
    public function __construct(R $response) { $this->response = $response; }
    public function getResponse() { return $this->response; }
}
function make() { return new HRE(new IR()); }
$a = new HRE(new R());
echo $a->getResponse()->name(), " ", make()->getResponse()->name(), "\n";
class A { public $v; public function __construct($v) { $this->v = $v; } }
class B extends A {}
class C extends A {}
echo gettype((new A(1))->v), " ", gettype((new B("s"))->v), " ", gettype((new C(1.5))->v), "\n";
"#);
    assert_eq!(out, "R IR\ninteger string double\n");
}

/// Laravel's `value()` called with closures and with scalars keeps every value.
#[test]
fn test_untyped_param_value_helper_closure_and_scalars() {
    let out = compile_and_run(r#"<?php
// Laravel Collections/helpers.php, literal.
function value($value, ...$args)
{
    return $value instanceof Closure ? $value(...$args) : $value;
}
echo value(fn () => 5), " ", value(5), " ", value('x'), "\n";
echo value(fn ($a, $b) => $a + $b, 2, 3), " ", value(1.5), " ", value(true) ? "t" : "f", "\n";
$o = new ArrayObject([1, 2]);
echo get_class(value($o)), " ", count(value(fn () => [1, 2, 3])), "\n";
var_dump(value(null));
"#);
    assert_eq!(out, "5 5 x\n5 1.5 t\nArrayObject 3\nNULL\n");
}

/// Laravel's `tap()` called with and without a callback: the untyped `$callback = null` is `callable|null`.
#[test]
fn test_untyped_param_tap_helper_callable_or_null() {
    let out = compile_and_run(r#"<?php
class HigherOrderTapProxy
{
    public $target;
    public function __construct($target) { $this->target = $target; }
}
// Laravel Support/helpers.php, literal.
function tap($value, $callback = null)
{
    if (is_null($callback)) {
        return new HigherOrderTapProxy($value);
    }

    $callback($value);

    return $value;
}
echo tap(5, function ($v) { echo "saw ", $v, "\n"; }), "\n";
echo tap('t', fn ($v) => null), "\n";
echo tap(2.5, fn ($v) => null), "\n";
echo get_class(tap(7)), " ", tap(7)->target, "\n";
"#);
    assert_eq!(out, "saw 5\n5\nt\n2.5\nHigherOrderTapProxy 7\n");
}

/// Laravel's `with()` with and without a callback.
#[test]
fn test_untyped_param_with_helper_nullable_callable() {
    let out = compile_and_run(r#"<?php
// Laravel Support/helpers.php, literal.
function with($value, ?callable $callback = null)
{
    return is_null($callback) ? $value : $callback($value);
}
echo with(5), " ", with('s'), " ", with(3, fn ($v) => $v * 2), " ", with('a', fn ($v) => strtoupper($v)), "\n";
echo with(1.25), " ", count(with([1, 2])), " ", with(2.5, fn ($v) => $v * 2), "\n";
"#);
    assert_eq!(out, "5 s 6 A\n1.25 2 5\n");
}

/// A reduced `data_get()` over arrays, objects and closure defaults.
#[test]
fn test_untyped_param_data_get_helper_mixed_targets() {
    let out = compile_and_run(r#"<?php
function value($value, ...$args) { return $value instanceof Closure ? $value(...$args) : $value; }
// Laravel data_get, reduced to the array/object/default paths (no wildcard, no Collection).
function data_get($target, $key, $default = null)
{
    if (is_null($key)) {
        return $target;
    }
    $key = is_array($key) ? $key : explode('.', $key);
    foreach ($key as $segment) {
        if (is_array($target) && array_key_exists($segment, $target)) {
            $target = $target[$segment];
        } elseif (is_object($target) && isset($target->{$segment})) {
            $target = $target->{$segment};
        } else {
            return value($default);
        }
    }
    return $target;
}
$data = ['a' => ['b' => 5, 'c' => 'str', 'f' => 1.5], 'o' => (object) ['p' => 'prop']];
echo data_get($data, 'a.b'), " ", data_get($data, 'a.c'), " ", data_get($data, 'a.f'), "\n";
echo data_get($data, 'o.p'), " ", data_get($data, 'x.y', 'dflt'), " ", data_get($data, 'x', fn () => 'lazy'), "\n";
echo data_get($data, ['a', 'b']), " ", count(data_get($data, null)), "\n";
"#);
    assert_eq!(out, "5 str 1.5\nprop dflt lazy\n5 2\n");
}

/// Helpers that receive mixed and return it unchanged keep each value's tag.
#[test]
fn test_untyped_param_identity_helpers_keep_tags() {
    let out = compile_and_run(r#"<?php
// Helpers that receive mixed and return the same value.
function id(mixed $v): mixed { return $v; }
function pass($v) { return $v; }
function first_or($v, $d = null) { return $v ?? $d; }
echo id(1), id('a'), id(2.5), " ", pass(3), pass('b'), pass(1.5), "\n";
echo first_or(null, 'd'), first_or('x'), first_or(4), "\n";
var_dump(pass(true), id(null), pass([1])[0]);
"#);
    assert_eq!(out, "1a2.5 3b1.5\ndx4\nbool(true)\nNULL\nint(1)\n");
}

/// `(string)` over a `Stringable` (or user interface) parameter calls the runtime class's `__toString`.
#[test]
fn test_stringable_cast_dispatches_runtime_tostring() {
    let out = compile_and_run(r#"<?php
class N implements Stringable { public function __toString(): string { return "n!"; } }
class M { public function __toString(): string { return "m!"; } }
function s(Stringable $x) { return (string) $x; }
function t(Stringable $x) { return "[" . $x . "]"; }
echo s(new N()), " ", t(new N()), " ", s(new M()), "\n";
interface Named { public function __toString(): string; }
class P implements Named { public function __toString(): string { return "p!"; } }
function u(Named $x): string { return (string) $x; }
echo u(new P()), "\n";
"#);
    assert_eq!(out, "n! [n!] m!\np!\n");
}

/// An untyped method variadic called with a string, an int, a float and a bool keeps each
/// value: the collector element is the lattice join (`mixed`), never `string`.
#[test]
fn test_checker_converge_untyped_method_variadic_mixed_scalars() {
    let out = compile_and_run(r#"<?php
class Cat { public static function cat(...$xs) { $s = ""; foreach ($xs as $x) { $s .= var_export($x, true) . ","; } return $s; } }
echo Cat::cat('x', 1, 2.5, true), "\n";
echo Cat::cat(3), "\n";
"#);
    assert_eq!(out, "'x',1,2.5,true,\n3,\n");
}

/// Sibling classes joined in an untyped function, instance-method and static-method parameter
/// form a union, so a method and a property only the children declare still type-check (the common
/// ancestor `A` rejected `$x->m()`).
#[test]
fn test_checker_converge_sibling_classes_join_to_union_in_params() {
    let out = compile_and_run(r#"<?php
class A { public $tag = "a"; }
class B extends A { public $only = "ob"; function m() { return "B::m"; } }
class C extends A { public $only = "oc"; function m() { return "C::m"; } }
function f($x) { return $x->m() . "/" . $x->only . "/" . $x->tag; }
echo f(new B()), "\n";
echo f(new C()), "\n";
class K { function g($x) { return $x->m(); } static function s($x) { return $x->m(); } }
$k = new K();
echo $k->g(new B()), $k->g(new C()), K::s(new C()), K::s(new B()), "\n";
function take(A $a) { return get_class($a); }
function pass($x) { return take($x); }
echo pass(new B()), pass(new C()), "\n";
"#);
    assert_eq!(out, "B::m/ob/a\nC::m/oc/a\nB::mC::mC::mB::m\nBC\n");
}

/// Sibling classes filling an untyped property from the constructor join to a union.
#[test]
fn test_checker_converge_sibling_classes_join_to_union_in_ctor_property() {
    let out = compile_and_run(r#"<?php
class A { }
class B extends A { function m() { return "B::m"; } }
class C extends A { function m() { return "C::m"; } }
class H { public $p; function __construct($p) { $this->p = $p; } function run() { return $this->p->m(); } }
echo (new H(new B()))->run(), " ", (new H(new C()))->run(), "\n";
"#);
    assert_eq!(out, "B::m C::m\n");
}

/// A method whose sixty untyped parameters each widen in a different pass is monotone, so the
/// oscillation detector must let it converge (counting changes per signature
/// raised an internal error at the 49th pass).
#[test]
fn test_checker_converge_many_params_widening_one_per_pass_is_not_an_oscillation() {
    let n = 60;
    let params: Vec<String> = (0..n).map(|i| format!("$p{i}")).collect();
    let body: Vec<String> = (0..n).map(|i| format!("gettype($p{i})[0]")).collect();
    let mut src = format!(
        "<?php\nclass Hub {{ function m({}) {{ return {}; }} }}\n",
        params.join(", "),
        body.join(" . ")
    );
    for i in 0..n {
        let v = if i + 1 < n {
            format!("return (new C{}())->v();", i + 1)
        } else {
            "return \"s\";".to_string()
        };
        let args: Vec<String> = (0..n)
            .map(|j| if j == i { "$this->v()".to_string() } else { "1".to_string() })
            .collect();
        src.push_str(&format!(
            "class C{i} {{ function v() {{ {v} }} function use() {{ return (new Hub())->m({}); }} }}\n",
            args.join(", ")
        ));
    }
    for i in 0..n {
        src.push_str(&format!("echo (new C{i}())->use(), \"\\n\";\n"));
    }
    let out = compile_and_run(&src);
    let expected: String = (0..n)
        .map(|i| {
            let line: String = (0..n).map(|j| if j == i { 's' } else { 'i' }).collect();
            format!("{line}\n")
        })
        .collect();
    assert_eq!(out, expected);
}

/// A parent|child join (`e(new P1); e(mkp(): P0)`) keeps the child's own method callable.
#[test]
fn test_checker_converge_parent_child_join_keeps_child_method() {
    let out = compile_and_run(r#"<?php
class P0 { function base() { return "b"; } }
class P1 extends P0 { function extra() { return "e1"; } }
function e($o) { return $o->extra(); }
function mkp(): P0 { return new P1(); }
echo e(new P1()), e(mkp()), "\n";
"#);
    assert_eq!(out, "e1e1\n");
}

/// When the runtime object of a parent|child join is the parent, the child-only method throws PHP's catchable `Error`.
#[test]
fn test_checker_converge_parent_child_join_throws_php_error_on_parent() {
    let out = compile_and_run(r#"<?php
class P0 { function base() { return "b"; } }
class P1 extends P0 { function extra() { return "e1"; } }
function e($o) { try { return $o->extra(); } catch (Error $x) { return "err"; } }
function mkp(): P0 { return new P0(); }
echo e(new P1()), e(mkp()), "\n";
"#);
    assert_eq!(out, "e1err\n");
}

/// An untyped property filled with an interface and its sub-interface keeps the sub-interface method callable (`MessageInterface::getStatusCode`).
#[test]
fn test_checker_converge_interface_parent_child_ctor_property_join() {
    let out = compile_and_run(r#"<?php
interface Msg { function body(); }
interface Resp extends Msg { function code(); }
class R implements Resp { function body() { return "B"; } function code() { return 200; } }
function asMsg(): Msg { return new R(); }
class H { protected $r; function __construct($r) { $this->r = $r; } function s() { return $this->r->code(); } }
echo (new H(asMsg()))->s(), " ", (new H(new R()))->s(), "\n";
"#);
    assert_eq!(out, "200 200\n");
}

/// `match (true)` arms see their `instanceof` condition's narrowing, so an untyped function returning `$v->value` / `$v->name` over a joined enum union keeps a string return (Laravel `enum_value()`).
#[test]
fn test_checker_converge_match_true_instanceof_narrows_joined_union_arms() {
    let out = compile_and_run(r#"<?php
enum Suit: string { case Hearts = 'H'; }
enum Pure { case One; }
class A { public $n = 'a'; } class B { public $m = 'b'; }
function ev($value, $default = null) {
    return match (true) {
        $value instanceof BackedEnum => $value->value,
        $value instanceof UnitEnum => $value->name,
        default => $value ?? $default,
    };
}
function an(A|B $v) { return match (true) { $v instanceof A => $v->n, default => 'z' }; }
echo ev(Suit::Hearts), ' ', ev(Pure::One), ' ', ev(null, 'd'), "\n";
var_dump(an(new A), an(new B));
"#);
    assert_eq!(out, "H One d\nstring(1) \"a\"\nstring(1) \"z\"\n");
}
