//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of PHP's `(array)` cast over
//! every source kind: an array is returned unchanged, `null` becomes `[]`, a scalar becomes
//! `[0 => value]`, and an object projects to its visibility-mangled property map.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout.
//! - Expected output is verbatim reference PHP 8.5 output for the same programs (issue #707).
//! - Runtime values come from `$argc` or from function parameters so constant folding cannot
//!   decide the cast before lowering sees it.

use super::*;

/// The issue #707 reproduction: a `mixed` return that holds an array casts back to it.
#[test]
fn test_array_cast_of_mixed_return_holding_an_array() {
    let out = compile_and_run(
        r#"<?php
function f(): mixed {
    return ["a", "b"];
}
$a = (array) f();
echo $a[0], "|", $a[1];
"#,
    );
    assert_eq!(out, "a|b");
}

/// Literal sources: a packed list, a string, `null`, every scalar kind, and an associative
/// array. Each of the non-`mixed` shapes used to die in the backend with
/// `cast to EIR type Heap(Array)`.
#[test]
fn test_array_cast_of_literals_follows_php() {
    let out = compile_and_run(
        r#"<?php
$list = (array) [1, 2];
echo count($list), ":", $list[0], ",", $list[1], "\n";
$str = (array) "x";
echo count($str), ":", $str[0], "\n";
$none = (array) null;
echo count($none), "\n";
var_dump((array) 5, (array) 1.5, (array) true, (array) false);
var_dump((array) ["k" => "v", "n" => 2]);
"#,
    );
    assert_eq!(
        out,
        "2:1,2\n1:x\n0\narray(1) {\n  [0]=>\n  int(5)\n}\narray(1) {\n  [0]=>\n  float(1.5)\n}\narray(1) {\n  [0]=>\n  bool(true)\n}\narray(1) {\n  [0]=>\n  bool(false)\n}\narray(2) {\n  [\"k\"]=>\n  string(1) \"v\"\n  [\"n\"]=>\n  int(2)\n}\n"
    );
}

/// Locals whose values are only known at run time, including the heterogeneous and nullable
/// ones the checker widens to a boxed representation.
#[test]
fn test_array_cast_of_runtime_locals_follows_php() {
    let out = compile_and_run(
        r#"<?php
$n = $argc > 5 ? 9 : 7;
$s = $argc > 5 ? "long" : "short";
$f = $argc > 5 ? 2.5 : 3.5;
$b = $argc > 5;
$list = [$n, $n + 1];
$map = ["a" => $s, "b" => $n];
$nothing = null;
echo json_encode((array) $n), json_encode((array) $s), json_encode((array) $f), "\n";
var_dump((array) $b);
echo json_encode((array) $list), json_encode((array) $map), json_encode((array) $nothing), "\n";
$either = $argc > 5 ? "str" : 42;
var_dump((array) $either);
$maybe_list = $argc > 5 ? null : [1, 2, 3];
echo json_encode((array) $maybe_list), "\n";
$maybe_none = $argc > 5 ? [1] : null;
echo json_encode((array) $maybe_none), "\n";
$maybe_int = $argc > 5 ? 1 : null;
echo json_encode((array) $maybe_int), "\n";
"#,
    );
    assert_eq!(
        out,
        "[7][\"short\"][3.5]\narray(1) {\n  [0]=>\n  bool(false)\n}\n[7,8]{\"a\":\"short\",\"b\":7}[]\narray(1) {\n  [0]=>\n  int(42)\n}\n[1,2,3]\n[]\n[]\n"
    );
}

/// Every parameter type returns through an `array` declaration. The `mixed`, union and
/// nullable shapes used to be typed `mixed` and rejected by the `: array` return check.
#[test]
fn test_array_cast_of_parameters_returns_php_arrays() {
    let out = compile_and_run(
        r#"<?php
function from_mixed(mixed $v): array { return (array) $v; }
function from_union(int|string $v): array { return (array) $v; }
function from_nullable_array(?array $v): array { return (array) $v; }
function from_array(array $v): array { return (array) $v; }
function from_int(int $v): array { return (array) $v; }
function from_string(string $v): array { return (array) $v; }
function from_float(float $v): array { return (array) $v; }
function from_bool(bool $v): array { return (array) $v; }
function from_nullable_int(?int $v): array { return (array) $v; }
function from_nullable_string(?string $v): array { return (array) $v; }
function from_untyped($v) { return (array) $v; }
echo json_encode([from_mixed(1), from_mixed("s"), from_mixed(null), from_mixed([1, 2])]), "\n";
echo json_encode([from_mixed(["x" => 1]), from_mixed(1.5), from_mixed(false)]), "\n";
echo json_encode([from_union(3), from_union("q")]), "\n";
echo json_encode([from_nullable_array(null), from_nullable_array([4, 5]), from_nullable_array(["k" => "v"])]), "\n";
echo json_encode([from_array([7]), from_array(["a" => 1]), from_array([])]), "\n";
echo json_encode([from_int(8), from_string("t"), from_float(0.25), from_bool(true)]), "\n";
echo json_encode([from_nullable_int(null), from_nullable_int(6), from_nullable_string(null), from_nullable_string("z")]), "\n";
echo json_encode([from_untyped(1), from_untyped("u"), from_untyped(null), from_untyped([1])]), "\n";
"#,
    );
    assert_eq!(
        out,
        "[[1],[\"s\"],[],[1,2]]\n[{\"x\":1},[1.5],[false]]\n[[3],[\"q\"]]\n[[],[4,5],{\"k\":\"v\"}]\n[[7],{\"a\":1},[]]\n[[8],[\"t\"],[0.25],[true]]\n[[],[6],[],[\"z\"]]\n[[1],[\"u\"],[],[1]]\n"
    );
}

/// The cast as an operand: under `count()`, as a `foreach` source, as `array_merge()`
/// arguments, over function returns, and inside loops.
#[test]
fn test_array_cast_nested_in_calls_and_loops() {
    let out = compile_and_run(
        r#"<?php
function shape(int $i) { return $i % 2 ? "odd$i" : [$i, $i * 2]; }
function numbers(): array { return [1, 2, 3]; }
function word(): string { return "s"; }
echo count((array) numbers()), count((array) word()), count((array) null), count((array) $argc), "\n";
for ($i = 0; $i < 4; $i++) {
    $row = (array) shape($i);
    echo count($row), ":", implode(",", $row), "\n";
}
foreach ((array) "one" as $k => $v) { echo "$k=>$v\n"; }
foreach ((array) numbers() as $k => $v) { echo "$k=>$v\n"; }
foreach ((array) null as $v) { echo "never\n"; }
echo json_encode(array_merge((array) 0, (array) [1, 2])), "\n";
function merged(mixed $a, mixed $b): array { return array_merge((array) $a, (array) $b); }
echo json_encode(merged("a", [1, 2])), json_encode(merged(null, ["k" => "v"])), json_encode(merged(1.5, true)), "\n";
$acc = [];
for ($i = 0; $i < 3; $i++) {
    $acc = array_merge($acc, (array) $i);
}
echo json_encode($acc), "\n";
foreach ([1, "two", null, [3, 4], 5.5] as $x) {
    echo json_encode((array) $x), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "3101\n2:0,0\n1:odd1\n2:2,4\n1:odd3\n0=>one\n0=>1\n1=>2\n2=>3\n[0,1,2]\n[\"a\",1,2]{\"k\":\"v\"}[1.5,true]\n[0,1,2]\n[1]\n[\"two\"]\n[]\n[3,4]\n[5.5]\n"
    );
}

/// Property, nullable-property, and static-property sources inside methods, ternary arms, a
/// double cast, and writes to the result that must not reach the source array.
#[test]
fn test_array_cast_of_members_and_writes_to_the_result() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public array $items = [];
    public $raw;
    public ?int $maybe = null;
    public static $shared = "st";
    public function __construct($raw) { $this->raw = $raw; }
    public function load(): void { $this->items = (array) $this->raw; }
    public function asList(): array { return (array) $this->raw; }
    public function maybeList(): array { return (array) $this->maybe; }
    public static function sharedList(): array { return (array) self::$shared; }
}
$bag = new Bag($argc > 5 ? [9] : "raw");
$bag->load();
echo json_encode($bag->items), json_encode($bag->asList()), json_encode($bag->maybeList()), "\n";
$bag->maybe = 4;
echo json_encode($bag->maybeList()), json_encode(Bag::sharedList()), "\n";
$cond = $argc > 5;
$picked = $cond ? (array) "s" : [];
echo json_encode($picked), json_encode($cond ? [1] : (array) 7), json_encode((array) (array) $argc), "\n";
$copy = (array) $bag->items;
$copy[] = "more";
echo count($bag->items), count($copy), "\n";
$w = (array) "x";
$w[] = "y";
echo implode(",", $w), "\n";
$e = (array) null;
$e[] = 1;
$e["k"] = 2;
echo json_encode($e), "\n";
"#,
    );
    assert_eq!(
        out,
        "[\"raw\"][\"raw\"][]\n[4][\"st\"]\n[][7][1]\n12\nx,y\n{\"0\":1,\"k\":2}\n"
    );
}

/// Object sources keep PHP's mangled keys whether the object is typed or reaches the cast
/// through `mixed`, and the remaining runtime-typed shapes (`?Class`, a closure, `iterable`,
/// `int|float`) take the tag-dispatch path.
#[test]
fn test_array_cast_of_objects_and_runtime_typed_sources() {
    let out = compile_and_run(
        r#"<?php
class Base { public $a = 1; protected $b = 2; private $c = 3; }
class Child extends Base { public $d = "d"; }
foreach ((array) new Base() as $k => $v) { echo json_encode($k), "=", $v, "\n"; }
function project(mixed $x) { return (array) $x; }
foreach (project(new Child()) as $k => $v) { echo json_encode($k), "=", $v, "\n"; }
function maybe_base(bool $b): ?Base { return $b ? new Base() : null; }
echo count((array) maybe_base(true)), count((array) maybe_base(false)), "\n";
$fn = function () { return 1; };
echo count((array) $fn), "\n";
function each_of(iterable $v): array { return (array) $v; }
echo json_encode(each_of([1, 2])), "\n";
function int_or_float(bool $b): int|float { return $b ? 1 : 2.5; }
echo json_encode((array) int_or_float(true)), json_encode((array) int_or_float(false)), "\n";
"#,
    );
    assert_eq!(
        out,
        "\"a\"=1\n\"\\u0000*\\u0000b\"=2\n\"\\u0000Base\\u0000c\"=3\n\"a\"=1\n\"\\u0000*\\u0000b\"=2\n\"\\u0000Base\\u0000c\"=3\n\"d\"=d\n30\n1\n[1,2]\n[1][2.5]\n"
    );
}

/// `(array)` on a resource wraps the resource itself, not its numeric handle: the element is still
/// a stream that `is_resource()`, `get_resource_type()` and `fwrite()` accept, both from a
/// statically typed resource and from one held in a `mixed` value (the runtime-dispatch path).
/// Review follow-up for #707.
#[test]
fn test_array_cast_of_a_resource_keeps_the_resource() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://memory", "r+");
$a = (array) $h;
var_dump(count($a), is_resource($a[0]), get_resource_type($a[0]));
fwrite($a[0], "hi");
rewind($h);
echo fread($h, 2), "\n";
$m = $argc > 5 ? 1 : $h;
$b = (array) $m;
var_dump(count($b), is_resource($b[0]));
fclose($h);
"#,
    );
    assert_eq!(out, "int(1)\nbool(true)\nstring(6) \"stream\"\nhi\nint(1)\nbool(true)\n");
}
