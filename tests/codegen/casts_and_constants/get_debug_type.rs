//! Purpose:
//! Codegen tests for PHP's `get_debug_type()`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected output was measured on PHP 8.5, including the anonymous-class spellings
//!   (`class@anonymous`, `Parent@anonymous`, `Interface@anonymous`) and closed resources.

use super::*;

/// Scalars, arrays, named objects, enums, generators and closures use php-src's zend type names,
/// whether the type is static, nullable, a union, or only known from a boxed array element.
#[test]
fn test_get_debug_type_names_scalars_arrays_and_objects() {
    let out = compile_and_run(
        r##"<?php
namespace App;
enum Suit: string { case Hearts = 'h'; }
class Point {}
function gen() { yield 1; }
function nullable(?int $x): string { return get_debug_type($x); }
function union(int|string|null $x): string { return get_debug_type($x); }
echo get_debug_type(null), " ", get_debug_type(true), " ", get_debug_type(false), " ", get_debug_type(42), " ",
    get_debug_type(1.5), " ", get_debug_type("s"), " ", get_debug_type([1, 2]), " ", get_debug_type(["k" => 1]), "\n";
echo get_debug_type(new Point()), " ", get_debug_type(new \stdClass()), " ", get_debug_type(new \ArrayObject([])), " ",
    get_debug_type(Suit::Hearts), " ", get_debug_type(gen()), "\n";
echo get_debug_type(function () {}), " ", get_debug_type(fn() => 1), " ", get_debug_type(strlen(...)), "\n";
echo nullable(1), " ", nullable(null), " ", union(1), " ", union("x"), " ", union(null), "\n";
$mixed = [null, true, 7, 2.5, "x", [], new Point(), fn() => 0, Suit::Hearts];
foreach ($mixed as $value) {
    echo get_debug_type($value), ",";
}
echo "\n";
echo get_debug_type(unserialize('O:7:"Missing":0:{}')), "\n";
"##,
    );
    assert_eq!(out, "null bool bool int float string array array\nApp\\Point stdClass ArrayObject App\\Suit Generator\nClosure Closure Closure\nint null int string null\nnull,bool,int,float,string,array,App\\Point,Closure,App\\Suit,\n__PHP_Incomplete_Class\n");
}

/// Anonymous classes are named from their parent, else their first declared interface, else
/// `class`; the implicit `Stringable` a `__toString()` method adds does not count.
#[test]
fn test_get_debug_type_names_anonymous_classes_like_php() {
    let out = compile_and_run(
        r##"<?php
namespace App;
interface Shape {}
interface Named extends \Stringable {}
abstract class Base {}
$values = [
    new class {},
    new class extends Base {},
    new class implements Shape {},
    new class implements Shape, \Countable { public function count(): int { return 0; } },
    new class extends Base implements Shape {},
    new class extends \ArrayObject {},
    new class { public function __toString(): string { return ""; } },
    new class implements \Stringable { public function __toString(): string { return ""; } },
    new class implements Named { public function __toString(): string { return ""; } },
];
foreach ($values as $value) {
    echo get_debug_type($value), "\n";
}
$point = new class extends Base {};
echo get_debug_type($point), "\n";
"##,
    );
    assert_eq!(out, "class@anonymous\nApp\\Base@anonymous\nApp\\Shape@anonymous\nApp\\Shape@anonymous\nApp\\Base@anonymous\nArrayObject@anonymous\nclass@anonymous\nStringable@anonymous\nApp\\Named@anonymous\nApp\\Base@anonymous\n");
}

/// Resources report their type in parentheses, and `resource (closed)` once closed, boxed or not.
#[test]
fn test_get_debug_type_names_open_and_closed_resources() {
    let out = compile_and_run(
        r##"<?php
$fp = fopen('php://memory', 'w+');
$filter = stream_filter_append($fp, 'string.rot13');
$boxed = [$fp, 1];
echo get_debug_type($fp), "|", get_debug_type($filter), "|", get_debug_type($boxed[0]), "|", get_debug_type(STDIN), "\n";
fclose($fp);
echo get_debug_type($fp), "|", get_debug_type($boxed[0]), "\n";
"##,
    );
    assert_eq!(out, "resource (stream)|resource (stream filter)|resource (stream)|resource (stream)\nresource (closed)|resource (closed)\n");
}

/// `get_debug_type()` is an internal function: eval reaches it (with compiled and eval-declared
/// anonymous classes crossing the boundary both ways), as do string callables, first-class
/// callables, `array_map()`, namespaced and case-insensitive calls, and Reflection.
#[test]
fn test_get_debug_type_is_an_internal_function() {
    let out = compile_and_run(
        r##"<?php
namespace App;
interface Shape {}
class Base {}
$native = [new class {}, new class extends Base {}, new class implements Shape {}, fn() => 1, 3, null];
eval('foreach ($native as $v) { echo get_debug_type($v), ","; } echo "\n";');
$evaluated = eval('return [new class {}, new class extends \App\Base {}, new class implements \App\Shape {}, new \App\Base(), function () {}, 1.0, "x", [1], false, STDIN];');
foreach ($evaluated as $v) {
    echo get_debug_type($v), ",";
}
echo "\n";
$f = 'get_debug_type';
$g = get_debug_type(...);
echo $f(1), " ", call_user_func('get_debug_type', 2.5), " ", $g(null), " ", GET_DEBUG_TYPE([]), " ", \get_debug_type(true), "\n";
echo implode(",", array_map('get_debug_type', [1, "a", null, new Base(), [], 1.5])), "\n";
echo (new \ReflectionFunction('get_debug_type'))->isInternal() ? 1 : 0,
    in_array('get_debug_type', get_defined_functions()['internal'], true) ? 1 : 0,
    function_exists('get_debug_type') ? 1 : 0, "\n";
"##,
    );
    assert_eq!(out, "class@anonymous,App\\Base@anonymous,App\\Shape@anonymous,Closure,int,null,\nclass@anonymous,App\\Base@anonymous,App\\Shape@anonymous,App\\Base,Closure,float,string,array,bool,resource (stream),\nint float null array bool\nint,string,null,App\\Base,array,float\n111\n");
}

/// An `iterable` parameter holds either an array or a `Traversable` object behind one
/// type-erased pointer: objects report their class name, not `array`.
#[test]
fn test_get_debug_type_names_objects_passed_through_iterable() {
    let out = compile_and_run(
        r##"<?php
function gen() { yield 1; }
function name(iterable $items): string { return get_debug_type($items); }
echo name(gen()), "|", name(new ArrayIterator([1])), "|", name([1]), "|", name(["k" => 1]), "\n";
"##,
    );
    assert_eq!(out, "Generator|ArrayIterator|array|array\n");
}

/// An incomplete object created by compiled `unserialize()` keeps its
/// `__PHP_Incomplete_Class` name when eval code inspects it, for `get_debug_type()` and
/// `get_class()` alike, instead of failing the eval call.
#[test]
fn test_get_debug_type_names_incomplete_objects_inside_eval() {
    let out = compile_and_run(
        r##"<?php
$o = unserialize('O:7:"Missing":0:{}');
eval('echo get_debug_type($o), "|", get_class($o), "\n";');
"##,
    );
    assert_eq!(out, "__PHP_Incomplete_Class|__PHP_Incomplete_Class\n");
}

/// A callback name known only at run time reaches `get_debug_type()` through its runtime
/// wrapper: `array_map()`, a variable call and `call_user_func()`.
#[test]
fn test_get_debug_type_runtime_callback_name() {
    let out = compile_and_run(
        r#"<?php
$f = $argc > 5 ? "gettype" : "get_debug_type";
echo implode(",", array_map($f, [1, "x", 1.5, null, [], new stdClass()])), "\n";
echo $f(true), ",", call_user_func($f, 2.0), "\n";
"#,
    );
    assert_eq!(out, "int,string,float,null,array,stdClass\nbool,float\n");
}
