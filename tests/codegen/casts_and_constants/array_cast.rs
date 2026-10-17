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
//! - `test_array_cast_as_a_literal_element_follows_php` places the cast inside indexed and
//!   associative literals, including with the EIR optimizer off and under `--heap-debug`.
//! - `test_array_cast_of_unpredictable_sources_inside_literals_follows_php` covers sources
//!   whose syntactic type is `int` but whose lowered type is not, in both literal shapes.
//! - `test_array_cast_of_float_arithmetic_inside_literals_follows_php` checks `%` and the
//!   other arithmetic operators on a runtime float parameter.

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

/// `(array)` used as an element of an indexed or associative literal.
///
/// The element type used to come from `infer_expr_type_syntactic`, which answered `int` for an
/// `(array)` cast. Lowering then cast the array to its truthiness, so `[(array) "x"]` ran as
/// `[1]` and `[(array) null]` as `[0]`.
const ARRAY_CAST_LITERAL_ELEMENTS: &str = r#"<?php
class Box { public int $n = 4; public string $s = "z"; }
function as_mixed(int $i): mixed {
    return $i % 2 === 0 ? "m" : [$i, $i + 1];
}
$s = $argc > 5 ? "long" : "x";
$n = $argc > 5 ? 9 : 5;
$nothing = null;
$arr = [1, 2];
$obj = new Box();

$str = [(array) "x", (array) $s];
echo count($str), ":", count($str[0]), ":", $str[0][0], ":", count($str[1]), ":", $str[1][0], "\n";
$none = [(array) null, (array) $nothing];
echo count($none), ":", count($none[0]), ":", count($none[1]), "\n";
$num = [(array) 5, (array) $n];
echo count($num), ":", $num[0][0], ":", $num[1][0], "\n";
$id = [(array) $arr];
echo count($id), ":", count($id[0]), ":", $id[0][0], ",", $id[0][1], "\n";
$boxed = [(array) $obj];
echo count($boxed), ":", count($boxed[0]), ":", $boxed[0]["n"], ",", $boxed[0]["s"], "\n";
$mix = [(array) as_mixed(0), (array) as_mixed(1)];
echo json_encode($mix), "\n";
$nest = [(array) "x", (array) null];
echo json_encode($nest), "\n";

$smap = ["k" => (array) "x"];
echo count($smap), ":", count($smap["k"]), ":", $smap["k"][0], "\n";
$nmap = ["k" => (array) null];
echo count($nmap), ":", count($nmap["k"]), "\n";
$imap = ["k" => (array) 1];
echo count($imap), ":", $imap["k"][0], "\n";
$amap = ["k" => (array) $arr];
echo count($amap["k"]), ":", $amap["k"][0], ",", $amap["k"][1], "\n";
$omap = ["k" => (array) $obj];
echo $omap["k"]["n"], ",", $omap["k"]["s"], "\n";
$mmap = ["k" => (array) as_mixed(1)];
echo json_encode($mmap), "\n";

$rows = [];
$maps = [];
for ($i = 0; $i < 3; $i++) {
    $rows[] = [(array) "x", (array) null];
    $maps[] = ["k" => (array) ($i + 1)];
}
echo json_encode($rows), "\n";
echo json_encode($maps), "\n";

$sum = 0;
for ($i = 0; $i < 40; $i++) {
    $row = [(array) "v", (array) null, (array) $i, (array) [$i]];
    $map = [
        "s" => (array) "v",
        "n" => (array) null,
        "i" => (array) $i,
        "a" => (array) [$i],
        "o" => (array) $obj,
        "m" => (array) as_mixed($i),
    ];
    $sum += count($row) + count($row[0]) + count($row[1]) + count($row[2]) + $row[2][0];
    $sum += count($row[3]) + $row[3][0];
    $sum += strlen($map["s"][0]) + count($map["n"]) + $map["i"][0] + count($map["a"]) + $map["a"][0];
    $sum += $map["o"]["n"] + count($map["m"]);
}
echo $sum, "\n";
"#;

/// Reference output for [`ARRAY_CAST_LITERAL_ELEMENTS`].
const ARRAY_CAST_LITERAL_ELEMENTS_OUT: &str = "\
2:1:x:1:x
2:0:0
2:5:5
1:2:1,2
1:2:4,z
[[\"m\"],[1,2]]
[[\"x\"],[]]
1:1:x
1:0
1:1
2:1,2
4,z
{\"k\":[1,2]}
[[[\"x\"],[]],[[\"x\"],[]],[[\"x\"],[]]]
[{\"k\":[1]},{\"k\":[2]},{\"k\":[3]}]
3700
";

/// Asserts stdout and a clean `--heap-debug` summary for the literal-element fixture.
fn assert_clean_literal_elements(out: ProgramOutput) {
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, ARRAY_CAST_LITERAL_ELEMENTS_OUT, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies an `(array)` cast stored in an indexed or associative literal keeps the array.
#[test]
fn test_array_cast_as_a_literal_element_follows_php() {
    assert_eq!(
        compile_and_run(ARRAY_CAST_LITERAL_ELEMENTS),
        ARRAY_CAST_LITERAL_ELEMENTS_OUT
    );
}

/// Verifies the literal-element cast still lowers to an array with the EIR optimizer off.
#[test]
fn test_array_cast_as_a_literal_element_follows_php_without_ir_opt() {
    let out = without_ir_opt(|| compile_and_run(ARRAY_CAST_LITERAL_ELEMENTS));
    assert_eq!(out, ARRAY_CAST_LITERAL_ELEMENTS_OUT);
}

/// Verifies repeated literal-element casts release their arrays under `--heap-debug`.
#[test]
fn test_array_cast_as_a_literal_element_leaves_a_clean_heap() {
    assert_clean_literal_elements(compile_and_run_with_heap_debug(ARRAY_CAST_LITERAL_ELEMENTS));
}

/// Verifies the heap-debug literal-element run with the EIR optimizer off.
#[test]
fn test_array_cast_as_a_literal_element_leaves_a_clean_heap_without_ir_opt() {
    let out = without_ir_opt(|| compile_and_run_with_heap_debug(ARRAY_CAST_LITERAL_ELEMENTS));
    assert_clean_literal_elements(out);
}

/// `(array)` of sources whose syntactic type is `int` but whose lowered type is not.
///
/// Indexed and associative literals both store the cast. `foreach`, `json_encode`, `count`,
/// and a key read must match PHP 8.5. The sources are runtime values so folding cannot
/// replace the cast before lowering classifies it.
const ARRAY_CAST_UNPREDICTABLE_SOURCES: &str = r#"<?php
class Box { public string $s = "hello"; public int $n = 7; }
function show($label, $row, $key) {
    echo $label, " json=", json_encode($row), "\n";
    echo $label, " count=", count($row), "\n";
    echo $label, " key=", json_encode($row[$key]), "\n";
    echo $label, " foreach=";
    foreach ($row as $k => $v) {
        echo json_encode($k), ":", json_encode($v), ";";
    }
    echo "\n";
}
$obj = new Box();
if ($argc > 5) { $obj = new Box(); }
$other = new Box();
$arr = ["s" => "from-array"];
if ($argc > 5) { $arr = ["s" => "other"]; }
$n = $argc > 5 ? 4 : 5;
$text = $argc > 5 ? "no" : "hi";
$f = $argc > 5 ? 2.5 : 1.5;
$b = $argc > 5 ? true : false;
$num = $argc > 5 ? "8" : "9";
$cls = $argc > 5 ? "Nope" : "Box";
$prop = $argc > 5 ? "n" : "s";
$fn = function (): string { return "zz"; };
class Holder { public static string $label = "L"; }

$idx = [(array)(object)$obj];
show("obj-i", $idx[0], "s");
$map = ["k" => (array)(object)$obj];
show("obj-a", $map["k"], "s");
$idx = [(array)(object)$arr];
show("arr-i", $idx[0], "s");
$map = ["k" => (array)(object)$arr];
show("arr-a", $map["k"], "s");
$idx = [(array)(object)$n];
show("int-i", $idx[0], "scalar");
$map = ["k" => (array)(object)$n];
show("int-a", $map["k"], "scalar");
$idx = [(array)(object)$text];
show("str-i", $idx[0], "scalar");
$map = ["k" => (array)(object)$text];
show("str-a", $map["k"], "scalar");
$idx = [(array)(array)(object)$obj];
show("nest-i", $idx[0], "s");
$map = ["k" => (array)(array)(object)$obj];
show("nest-a", $map["k"], "s");
$idx = [(array)(clone $obj)];
show("clone-i", $idx[0], "s");
$map = ["k" => (array)(clone $obj)];
show("clone-a", $map["k"], "s");
$idx = [(array)($tmp = $obj)];
show("assign-i", $idx[0], "s");
$map = ["k" => (array)($tmp = $obj)];
show("assign-a", $map["k"], "s");
$idx = [(array)(@$obj)];
show("suppress-i", $idx[0], "s");
$map = ["k" => (array)(@$obj)];
show("suppress-a", $map["k"], "s");
$idx = [(array)(-$f)];
show("neg-i", $idx[0], 0);
$map = ["k" => (array)(-$f)];
show("neg-a", $map["k"], 0);
$idx = [(array)(!$b)];
show("not-i", $idx[0], 0);
$map = ["k" => (array)(!$b)];
show("not-a", $map["k"], 0);
$idx = [(array)($f + 0)];
show("add0-i", $idx[0], 0);
$map = ["k" => (array)($f + 0)];
show("add0-a", $map["k"], 0);
$idx = [(array)($obj ?? $other)];
show("coalesce-i", $idx[0], "s");
$map = ["k" => (array)($obj ?? $other)];
show("coalesce-a", $map["k"], "s");
$idx = [(array)($text ?: "fallback")];
show("short-i", $idx[0], 0);
$map = ["k" => (array)($text ?: "fallback")];
show("short-a", $map["k"], 0);
$pick = $argc > 5 ? 0 : 1;
$idx = [(array)(match ($pick) { 1 => $obj, default => $other })];
show("match-i", $idx[0], "s");
$map = ["k" => (array)(match ($pick) { 1 => $obj, default => $other })];
show("match-a", $map["k"], "s");
$upf1 = $f; $upf2 = $f; $downf1 = $f; $downf2 = $f;
$idx = [(array)(++$upf1)];
show("fpreinc-i", $idx[0], 0);
$map = ["k" => (array)(++$upf2)];
show("fpreinc-a", $map["k"], 0);
$idx = [(array)(--$downf1)];
show("fpredec-i", $idx[0], 0);
$map = ["k" => (array)(--$downf2)];
show("fpredec-a", $map["k"], 0);
$ups1 = $num; $ups2 = $num; $downs1 = $num; $downs2 = $num;
$idx = [(array)(++$ups1)];
show("spreinc-i", $idx[0], 0);
$map = ["k" => (array)(++$ups2)];
show("spreinc-a", $map["k"], 0);
$idx = [(array)(--$downs1)];
show("spredec-i", $idx[0], 0);
$map = ["k" => (array)(--$downs2)];
show("spredec-a", $map["k"], 0);
$idx = [(array)(new $cls())];
show("dynnew-i", $idx[0], "s");
$map = ["k" => (array)(new $cls())];
show("dynnew-a", $map["k"], "s");
$idx = [(array)(Holder::$label)];
show("static-i", $idx[0], 0);
$map = ["k" => (array)(Holder::$label)];
show("static-a", $map["k"], 0);
$idx = [(array)($obj->$prop)];
show("dynprop-i", $idx[0], 0);
$map = ["k" => (array)($obj->$prop)];
show("dynprop-a", $map["k"], 0);
$idx = [(array)$fn()];
show("closure-i", $idx[0], 0);
$map = ["k" => (array)$fn()];
show("closure-a", $map["k"], 0);
$idx = [(array)((int)$num)];
show("castint-i", $idx[0], 0);
$map = ["k" => (array)((int)$num)];
show("castint-a", $map["k"], 0);
$idx = [(array)((float)$n)];
show("castfloat-i", $idx[0], 0);
$map = ["k" => (array)((float)$n)];
show("castfloat-a", $map["k"], 0);
$idx = [(array)((string)$n)];
show("caststr-i", $idx[0], 0);
$map = ["k" => (array)((string)$n)];
show("caststr-a", $map["k"], 0);
$idx = [(array)((bool)$n)];
show("castbool-i", $idx[0], 0);
$map = ["k" => (array)((bool)$n)];
show("castbool-a", $map["k"], 0);
"#;

/// Reference output for [`ARRAY_CAST_UNPREDICTABLE_SOURCES`].
const ARRAY_CAST_UNPREDICTABLE_SOURCES_OUT: &str = r#"obj-i json={"s":"hello","n":7}
obj-i count=2
obj-i key="hello"
obj-i foreach="s":"hello";"n":7;
obj-a json={"s":"hello","n":7}
obj-a count=2
obj-a key="hello"
obj-a foreach="s":"hello";"n":7;
arr-i json={"s":"from-array"}
arr-i count=1
arr-i key="from-array"
arr-i foreach="s":"from-array";
arr-a json={"s":"from-array"}
arr-a count=1
arr-a key="from-array"
arr-a foreach="s":"from-array";
int-i json={"scalar":5}
int-i count=1
int-i key=5
int-i foreach="scalar":5;
int-a json={"scalar":5}
int-a count=1
int-a key=5
int-a foreach="scalar":5;
str-i json={"scalar":"hi"}
str-i count=1
str-i key="hi"
str-i foreach="scalar":"hi";
str-a json={"scalar":"hi"}
str-a count=1
str-a key="hi"
str-a foreach="scalar":"hi";
nest-i json={"s":"hello","n":7}
nest-i count=2
nest-i key="hello"
nest-i foreach="s":"hello";"n":7;
nest-a json={"s":"hello","n":7}
nest-a count=2
nest-a key="hello"
nest-a foreach="s":"hello";"n":7;
clone-i json={"s":"hello","n":7}
clone-i count=2
clone-i key="hello"
clone-i foreach="s":"hello";"n":7;
clone-a json={"s":"hello","n":7}
clone-a count=2
clone-a key="hello"
clone-a foreach="s":"hello";"n":7;
assign-i json={"s":"hello","n":7}
assign-i count=2
assign-i key="hello"
assign-i foreach="s":"hello";"n":7;
assign-a json={"s":"hello","n":7}
assign-a count=2
assign-a key="hello"
assign-a foreach="s":"hello";"n":7;
suppress-i json={"s":"hello","n":7}
suppress-i count=2
suppress-i key="hello"
suppress-i foreach="s":"hello";"n":7;
suppress-a json={"s":"hello","n":7}
suppress-a count=2
suppress-a key="hello"
suppress-a foreach="s":"hello";"n":7;
neg-i json=[-1.5]
neg-i count=1
neg-i key=-1.5
neg-i foreach=0:-1.5;
neg-a json=[-1.5]
neg-a count=1
neg-a key=-1.5
neg-a foreach=0:-1.5;
not-i json=[true]
not-i count=1
not-i key=true
not-i foreach=0:true;
not-a json=[true]
not-a count=1
not-a key=true
not-a foreach=0:true;
add0-i json=[1.5]
add0-i count=1
add0-i key=1.5
add0-i foreach=0:1.5;
add0-a json=[1.5]
add0-a count=1
add0-a key=1.5
add0-a foreach=0:1.5;
coalesce-i json={"s":"hello","n":7}
coalesce-i count=2
coalesce-i key="hello"
coalesce-i foreach="s":"hello";"n":7;
coalesce-a json={"s":"hello","n":7}
coalesce-a count=2
coalesce-a key="hello"
coalesce-a foreach="s":"hello";"n":7;
short-i json=["hi"]
short-i count=1
short-i key="hi"
short-i foreach=0:"hi";
short-a json=["hi"]
short-a count=1
short-a key="hi"
short-a foreach=0:"hi";
match-i json={"s":"hello","n":7}
match-i count=2
match-i key="hello"
match-i foreach="s":"hello";"n":7;
match-a json={"s":"hello","n":7}
match-a count=2
match-a key="hello"
match-a foreach="s":"hello";"n":7;
fpreinc-i json=[2.5]
fpreinc-i count=1
fpreinc-i key=2.5
fpreinc-i foreach=0:2.5;
fpreinc-a json=[2.5]
fpreinc-a count=1
fpreinc-a key=2.5
fpreinc-a foreach=0:2.5;
fpredec-i json=[0.5]
fpredec-i count=1
fpredec-i key=0.5
fpredec-i foreach=0:0.5;
fpredec-a json=[0.5]
fpredec-a count=1
fpredec-a key=0.5
fpredec-a foreach=0:0.5;
spreinc-i json=[10]
spreinc-i count=1
spreinc-i key=10
spreinc-i foreach=0:10;
spreinc-a json=[10]
spreinc-a count=1
spreinc-a key=10
spreinc-a foreach=0:10;
spredec-i json=[8]
spredec-i count=1
spredec-i key=8
spredec-i foreach=0:8;
spredec-a json=[8]
spredec-a count=1
spredec-a key=8
spredec-a foreach=0:8;
dynnew-i json={"s":"hello","n":7}
dynnew-i count=2
dynnew-i key="hello"
dynnew-i foreach="s":"hello";"n":7;
dynnew-a json={"s":"hello","n":7}
dynnew-a count=2
dynnew-a key="hello"
dynnew-a foreach="s":"hello";"n":7;
static-i json=["L"]
static-i count=1
static-i key="L"
static-i foreach=0:"L";
static-a json=["L"]
static-a count=1
static-a key="L"
static-a foreach=0:"L";
dynprop-i json=["hello"]
dynprop-i count=1
dynprop-i key="hello"
dynprop-i foreach=0:"hello";
dynprop-a json=["hello"]
dynprop-a count=1
dynprop-a key="hello"
dynprop-a foreach=0:"hello";
closure-i json=["zz"]
closure-i count=1
closure-i key="zz"
closure-i foreach=0:"zz";
closure-a json=["zz"]
closure-a count=1
closure-a key="zz"
closure-a foreach=0:"zz";
castint-i json=[9]
castint-i count=1
castint-i key=9
castint-i foreach=0:9;
castint-a json=[9]
castint-a count=1
castint-a key=9
castint-a foreach=0:9;
castfloat-i json=[5]
castfloat-i count=1
castfloat-i key=5
castfloat-i foreach=0:5;
castfloat-a json=[5]
castfloat-a count=1
castfloat-a key=5
castfloat-a foreach=0:5;
caststr-i json=["5"]
caststr-i count=1
caststr-i key="5"
caststr-i foreach=0:"5";
caststr-a json=["5"]
caststr-a count=1
caststr-a key="5"
caststr-a foreach=0:"5";
castbool-i json=[true]
castbool-i count=1
castbool-i key=true
castbool-i foreach=0:true;
castbool-a json=[true]
castbool-a count=1
castbool-a key=true
castbool-a foreach=0:true;
"#;

/// Asserts stdout and a clean `--heap-debug` summary for the unpredictable-source fixture.
fn assert_clean_unpredictable_sources(out: ProgramOutput) {
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, ARRAY_CAST_UNPREDICTABLE_SOURCES_OUT, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies `(array)` of object casts, clones, calls, and arithmetic inside literals.
#[test]
fn test_array_cast_of_unpredictable_sources_inside_literals_follows_php() {
    assert_eq!(
        compile_and_run(ARRAY_CAST_UNPREDICTABLE_SOURCES),
        ARRAY_CAST_UNPREDICTABLE_SOURCES_OUT
    );
}

/// Verifies the same sources with the EIR optimizer off.
#[test]
fn test_array_cast_of_unpredictable_sources_inside_literals_follows_php_without_ir_opt() {
    let out = without_ir_opt(|| compile_and_run(ARRAY_CAST_UNPREDICTABLE_SOURCES));
    assert_eq!(out, ARRAY_CAST_UNPREDICTABLE_SOURCES_OUT);
}

/// Verifies the unpredictable-source fixture releases every array under `--heap-debug`.
#[test]
fn test_array_cast_of_unpredictable_sources_inside_literals_leaves_a_clean_heap() {
    assert_clean_unpredictable_sources(compile_and_run_with_heap_debug(
        ARRAY_CAST_UNPREDICTABLE_SOURCES,
    ));
}

/// Verifies the heap-debug run of the unpredictable-source fixture with the optimizer off.
#[test]
fn test_array_cast_of_unpredictable_sources_inside_literals_leaves_a_clean_heap_without_ir_opt() {
    let out = without_ir_opt(|| compile_and_run_with_heap_debug(ARRAY_CAST_UNPREDICTABLE_SOURCES));
    assert_clean_unpredictable_sources(out);
}

/// `(array)` of arithmetic on a runtime float, in indexed and associative literals.
///
/// `%` of a float parameter is an int: the lowering coerces before the float path.
/// `/`, `**`, `+`, `-`, and `*` stay floats. Shifts and `&`, `|`, `^` are always ints;
/// a float reaches them through `mixed`, which the checker accepts and the lowering
/// coerces. `~` requires an int, so the float is cast with `(int)` first.
const ARRAY_CAST_FLOAT_ARITHMETIC: &str = r#"<?php
function show($label, $row) {
    echo $label, " json=", json_encode($row), "\n";
    echo $label, " v=", $row[0], "\n";
    echo $label, " count=", count($row), "\n";
    echo $label, " foreach=";
    foreach ($row as $k => $v) {
        echo json_encode($k), ":", json_encode($v), ";";
    }
    echo "\n";
}
function arith(float $x) {
    $idx = [(array)($x % 2)];
    show("mod-i", $idx[0]);
    $map = ["k" => (array)($x % 2)];
    show("mod-a", $map["k"]);
    $idx = [(array)($x / 2)];
    show("div-i", $idx[0]);
    $map = ["k" => (array)($x / 2)];
    show("div-a", $map["k"]);
    $idx = [(array)($x ** 2)];
    show("pow-i", $idx[0]);
    $map = ["k" => (array)($x ** 2)];
    show("pow-a", $map["k"]);
    $idx = [(array)($x + 0.25)];
    show("add-i", $idx[0]);
    $map = ["k" => (array)($x + 0.25)];
    show("add-a", $map["k"]);
    $idx = [(array)($x - 0.75)];
    show("sub-i", $idx[0]);
    $map = ["k" => (array)($x - 0.75)];
    show("sub-a", $map["k"]);
    $idx = [(array)($x * 2)];
    show("mul-i", $idx[0]);
    $map = ["k" => (array)($x * 2)];
    show("mul-a", $map["k"]);
}
function bits(mixed $n) {
    $idx = [(array)($n << 1)];
    show("shl-i", $idx[0]);
    $map = ["k" => (array)($n << 1)];
    show("shl-a", $map["k"]);
    $idx = [(array)($n >> 1)];
    show("shr-i", $idx[0]);
    $map = ["k" => (array)($n >> 1)];
    show("shr-a", $map["k"]);
    $idx = [(array)($n & 3)];
    show("and-i", $idx[0]);
    $map = ["k" => (array)($n & 3)];
    show("and-a", $map["k"]);
    $idx = [(array)($n | 2)];
    show("or-i", $idx[0]);
    $map = ["k" => (array)($n | 2)];
    show("or-a", $map["k"]);
    $idx = [(array)($n ^ 1)];
    show("xor-i", $idx[0]);
    $map = ["k" => (array)($n ^ 1)];
    show("xor-a", $map["k"]);
}
function complement(float $x) {
    $n = (int)$x;
    $idx = [(array)(~$n)];
    show("not-i", $idx[0]);
    $map = ["k" => (array)(~$n)];
    show("not-a", $map["k"]);
}
$x = $argc > 5 ? 1.25 : 5.75;
arith($x);
bits($x);
complement($x);
"#;

/// Reference output for [`ARRAY_CAST_FLOAT_ARITHMETIC`].
const ARRAY_CAST_FLOAT_ARITHMETIC_OUT: &str = r#"mod-i json=[1]
mod-i v=1
mod-i count=1
mod-i foreach=0:1;
mod-a json=[1]
mod-a v=1
mod-a count=1
mod-a foreach=0:1;
div-i json=[2.875]
div-i v=2.875
div-i count=1
div-i foreach=0:2.875;
div-a json=[2.875]
div-a v=2.875
div-a count=1
div-a foreach=0:2.875;
pow-i json=[33.0625]
pow-i v=33.0625
pow-i count=1
pow-i foreach=0:33.0625;
pow-a json=[33.0625]
pow-a v=33.0625
pow-a count=1
pow-a foreach=0:33.0625;
add-i json=[6]
add-i v=6
add-i count=1
add-i foreach=0:6;
add-a json=[6]
add-a v=6
add-a count=1
add-a foreach=0:6;
sub-i json=[5]
sub-i v=5
sub-i count=1
sub-i foreach=0:5;
sub-a json=[5]
sub-a v=5
sub-a count=1
sub-a foreach=0:5;
mul-i json=[11.5]
mul-i v=11.5
mul-i count=1
mul-i foreach=0:11.5;
mul-a json=[11.5]
mul-a v=11.5
mul-a count=1
mul-a foreach=0:11.5;
shl-i json=[10]
shl-i v=10
shl-i count=1
shl-i foreach=0:10;
shl-a json=[10]
shl-a v=10
shl-a count=1
shl-a foreach=0:10;
shr-i json=[2]
shr-i v=2
shr-i count=1
shr-i foreach=0:2;
shr-a json=[2]
shr-a v=2
shr-a count=1
shr-a foreach=0:2;
and-i json=[1]
and-i v=1
and-i count=1
and-i foreach=0:1;
and-a json=[1]
and-a v=1
and-a count=1
and-a foreach=0:1;
or-i json=[7]
or-i v=7
or-i count=1
or-i foreach=0:7;
or-a json=[7]
or-a v=7
or-a count=1
or-a foreach=0:7;
xor-i json=[4]
xor-i v=4
xor-i count=1
xor-i foreach=0:4;
xor-a json=[4]
xor-a v=4
xor-a count=1
xor-a foreach=0:4;
not-i json=[-6]
not-i v=-6
not-i count=1
not-i foreach=0:-6;
not-a json=[-6]
not-a v=-6
not-a count=1
not-a foreach=0:-6;
"#;

/// Asserts stdout and a clean `--heap-debug` summary for the float-arithmetic fixture.
fn assert_clean_float_arithmetic(out: ProgramOutput) {
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, ARRAY_CAST_FLOAT_ARITHMETIC_OUT, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies float `%`, shifts, bitwise ops, `/`, and `**` inside array literals.
#[test]
fn test_array_cast_of_float_arithmetic_inside_literals_follows_php() {
    assert_eq!(
        compile_and_run(ARRAY_CAST_FLOAT_ARITHMETIC),
        ARRAY_CAST_FLOAT_ARITHMETIC_OUT
    );
}

/// Verifies the float-arithmetic literal casts with the EIR optimizer off.
#[test]
fn test_array_cast_of_float_arithmetic_inside_literals_follows_php_without_ir_opt() {
    let out = without_ir_opt(|| compile_and_run(ARRAY_CAST_FLOAT_ARITHMETIC));
    assert_eq!(out, ARRAY_CAST_FLOAT_ARITHMETIC_OUT);
}

/// Verifies the float-arithmetic fixture releases every array under `--heap-debug`.
#[test]
fn test_array_cast_of_float_arithmetic_inside_literals_leaves_a_clean_heap() {
    assert_clean_float_arithmetic(compile_and_run_with_heap_debug(ARRAY_CAST_FLOAT_ARITHMETIC));
}

/// Verifies the heap-debug float-arithmetic run with the EIR optimizer off.
#[test]
fn test_array_cast_of_float_arithmetic_inside_literals_leaves_a_clean_heap_without_ir_opt() {
    let out = without_ir_opt(|| compile_and_run_with_heap_debug(ARRAY_CAST_FLOAT_ARITHMETIC));
    assert_clean_float_arithmetic(out);
}
