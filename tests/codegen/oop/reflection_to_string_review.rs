//! Purpose:
//! Regression coverage for PHP Reflection dump details and eval-created reflector casts.
//!
//! Called from:
//! - `codegen_tests` through the `oop` module.
//!
//! Key details:
//! - Fixtures use PHP 8.5.11/8.5.12 oracles, excluding the source location line.
//! - Direct objects and member listings must share the same formatting.

use super::*;

/// Overrides report the overwritten declaration and the root prototype independently.
#[test]
fn test_reflection_dump_override_origins() {
    let out = compile_and_run(r#"<?php
class A { public function run() {} private function hidden() {} }
class B extends A { public function run() {} public function hidden() {} }
class C extends B { public function run() {} }
class D extends B {}
interface I { public function run(); }
class E extends A implements I { public function run() {} }
echo new ReflectionMethod('B', 'run');
echo new ReflectionMethod('C', 'run');
echo new ReflectionMethod('D', 'run');
echo new ReflectionMethod('B', 'hidden');
echo new ReflectionMethod('E', 'run');
$r = new ReflectionClass('C');
foreach ($r->getMethods() as $m) { if ($m->getName() === 'run') echo $m; }
"#);
    assert_eq!(out, "Method [ <user, overwrites A, prototype A> public method run ] {\n}\n\
        Method [ <user, overwrites B, prototype A> public method run ] {\n}\n\
        Method [ <user, inherits B, prototype A> public method run ] {\n}\n\
        Method [ <user> public method hidden ] {\n}\n\
        Method [ <user, overwrites A, prototype I> public method run ] {\n}\n\
        Method [ <user, overwrites B, prototype A> public method run ] {\n}\n");
}

/// Interface prototypes follow direct clause order and retain the parent's linked contracts.
#[test]
fn test_reflection_dump_interface_link_order() {
    let out = compile_and_run(r#"<?php
interface I1 { public function run(); }
interface I2 { public function run(); }
interface I3 { public function run(); }
interface IP extends I1 { public function run(); }
interface IK extends I1 {}
class A implements I1, I2 { public function run() {} }
class H implements I1, IP { public function run() {} }
class P12 implements I1, I2 { public function run() {} }
class C extends P12 { public function run() {} }
class Cadd extends P12 implements I3 { public function run() {} }
class Cinherits extends P12 implements I3 {}
class Cdeep extends Cinherits {}
class Crev extends P12 implements I2, I1 { public function run() {} }
class Jsingle implements IP { public function run() {} }
class Ksingle implements IK { public function run() {} }
class LastDirect implements IP, I1 { public function run() {} }
echo new ReflectionMethod('A', 'run');
echo new ReflectionMethod('H', 'run');
echo new ReflectionMethod('C', 'run');
echo new ReflectionMethod('Cadd', 'run');
echo new ReflectionMethod('Cinherits', 'run');
echo new ReflectionMethod('Cdeep', 'run');
echo new ReflectionMethod('Crev', 'run');
echo new ReflectionMethod('Jsingle', 'run');
echo new ReflectionMethod('Ksingle', 'run');
echo new ReflectionMethod('LastDirect', 'run');
"#);
    assert_eq!(out, "Method [ <user, prototype I2> public method run ] {\n}\n\
        Method [ <user, prototype IP> public method run ] {\n}\n\
        Method [ <user, overwrites P12, prototype I2> public method run ] {\n}\n\
        Method [ <user, overwrites P12, prototype I3> public method run ] {\n}\n\
        Method [ <user, inherits P12, prototype I3> public method run ] {\n}\n\
        Method [ <user, inherits P12, prototype I3> public method run ] {\n}\n\
        Method [ <user, overwrites P12, prototype I2> public method run ] {\n}\n\
        Method [ <user, prototype IP> public method run ] {\n}\n\
        Method [ <user, prototype I1> public method run ] {\n}\n\
        Method [ <user, prototype I1> public method run ] {\n}\n");
}

/// Float defaults round to 14 digits before choosing the decimal or exponential form.
#[test]
fn test_reflection_dump_float_precision() {
    let out = compile_and_run(r#"<?php
function f(float $a = 0.30000000000000004, float $b = 1e14,
    float $c = 1e13, float $d = 99999999999999.9, float $e = -0.0) {}
foreach ((new ReflectionFunction('f'))->getParameters() as $p) echo $p, "\n";
"#);
    assert_eq!(out, "Parameter #0 [ <optional> float $a = 0.3 ]\n\
        Parameter #1 [ <optional> float $b = 1.0E+14 ]\n\
        Parameter #2 [ <optional> float $c = 10000000000000.0 ]\n\
        Parameter #3 [ <optional> float $d = 1.0E+14 ]\n\
        Parameter #4 [ <optional> float $e = -0.0 ]\n");
}

/// Strings escape control bytes, backslashes and UTF-8 bytes without escaping quote marks.
#[test]
fn test_reflection_dump_string_escapes() {
    let out = compile_and_run(r#"<?php
function f(string $s = "\n\r\t\v\f\e\x00\x7F\xFF\\'é", array $a = ["a\nb" => "c\td"]) {}
foreach ((new ReflectionFunction('f'))->getParameters() as $p) echo $p, "\n";
"#);
    assert_eq!(out, "Parameter #0 [ <optional> string $s = '\\n\\r\\t\\v\\f\\e\\x00\\x7F\\xFF\\\\'\\xC3\\xA9' ]\n\
        Parameter #1 [ <optional> array $a = ['a\\nb' => 'c\\td'] ]\n");
}

/// Iterable aliases expand in nullable parameters and return types as PHP unions.
#[test]
fn test_reflection_dump_iterable_alias() {
    let out = compile_and_run(r#"<?php
function f(iterable $a = [], ?iterable $b = null): iterable { return []; }
echo new ReflectionFunction('f');
"#);
    assert_eq!(out, "Function [ <user> function f ] {\n\n  - Parameters [2] {\n\
        \x20   Parameter #0 [ <optional> Traversable|array $a = [] ]\n\
        \x20   Parameter #1 [ <optional> Traversable|array|null $b = NULL ]\n  }\n\
        \x20 - Return [ Traversable|array ]\n}\n");
}

/// Native constant defaults retain both their PHP-visible names and their resolved values.
#[test]
fn test_reflection_dump_native_constant_defaults() {
    let out = compile_and_run(r#"<?php
function f(int $flags = ENT_QUOTES) {}
$p = new ReflectionParameter('f', 'flags');
echo $p, "\n", (int)$p->isDefaultValueConstant(), " ",
    (int)$p->isDefaultValueAvailable(), " ", $p->getDefaultValueConstantName(), " ", $p->getDefaultValue();
"#);
    assert_eq!(out, "Parameter #0 [ <optional> int $flags = ENT_QUOTES ]\n1 1 ENT_QUOTES 3");
}

/// Deprecated attributes and tentative internal return types get distinct PHP labels.
#[test]
fn test_reflection_dump_deprecated_and_tentative_labels() {
    let out = compile_and_run(r#"<?php
#[Deprecated] function oldFunction() {}
class D { #[Deprecated] public function oldMethod() {} }
echo new ReflectionFunction('oldFunction');
echo new ReflectionMethod('D', 'oldMethod');
echo new ReflectionMethod('Iterator', 'current');
echo new ReflectionMethod('Countable', 'count');
echo new ReflectionMethod('ReflectionFunction', 'getName');
"#);
    assert_eq!(out, "Function [ <user, deprecated> function oldFunction ] {\n}\n\
        Method [ <user, deprecated> public method oldMethod ] {\n}\n\
        Method [ <internal:Core> abstract public method current ] {\n\n  - Parameters [0] {\n  }\n  - Tentative return [ mixed ]\n}\n\
        Method [ <internal:Core> abstract public method count ] {\n\n  - Parameters [0] {\n  }\n  - Tentative return [ int ]\n}\n\
        Method [ <internal:Reflection, inherits ReflectionFunctionAbstract> public method getName ] {\n\n  - Parameters [0] {\n  }\n  - Tentative return [ string ]\n}\n");
}

/// Eval-created reflectors keep their dumps for direct casts and casts after eval returns.
#[test]
fn test_reflection_dump_eval_created_casts() {
    let out = compile_and_run(r#"<?php
$source = $argc > 0 ? 'function dumpEval(int $a): void {} return new ReflectionFunction("dumpEval");' : '';
$r = eval($source);
echo $r;
$source = $argc > 0 ? 'class DumpEval { public function run(int $a): void {} }
    $m = new ReflectionMethod("DumpEval", "run"); echo $m;
    $p = new ReflectionParameter(["DumpEval", "run"], 0); echo $p, "\n";' : '';
eval($source);
"#);
    assert_eq!(out, "Function [ <user> function dumpEval ] {\n\n  - Parameters [1] {\n\
        \x20   Parameter #0 [ <required> int $a ]\n  }\n  - Return [ void ]\n}\n\
        Method [ <user> public method run ] {\n\n  - Parameters [1] {\n\
        \x20   Parameter #0 [ <required> int $a ]\n  }\n  - Return [ void ]\n}\n\
        Parameter #0 [ <required> int $a ]\n");
}

/// Constructors distinguish ordinary replacement from an abstract constructor prototype.
#[test]
fn test_reflection_dump_constructor_origins() {
    let out = compile_and_run(r#"<?php
class A { public function __construct() {} }
class B extends A { public function __construct() {} }
abstract class P { abstract public function __construct(); }
class Q extends P { public function __construct() {} }
class D extends B {}
class E extends Q {}
echo new ReflectionMethod('B', '__construct');
echo new ReflectionMethod('Q', '__construct');
echo new ReflectionMethod('D', '__construct');
echo new ReflectionMethod('E', '__construct');
"#);
    assert_eq!(out, "Method [ <user, overwrites A, ctor> public method __construct ] {\n}\n\
        Method [ <user, overwrites P, prototype P, ctor> public method __construct ] {\n}\n\
        Method [ <user, inherits B, ctor> public method __construct ] {\n}\n\
        Method [ <user, inherits Q, prototype P, ctor> public method __construct ] {\n}\n");
}

/// Constructor overrides preserve inherited contracts in both the prototype API and dump.
#[test]
fn test_reflection_constructor_overrides_keep_inherited_contracts() {
    let out = compile_and_run(r#"<?php
interface Contract { public function __construct(); }
class ParentCtor implements Contract { public function __construct() {} }
class ChildCtor extends ParentCtor { public function __construct() {} }
class GrandchildCtor extends ChildCtor { public function __construct() {} }
abstract class AbstractContract implements Contract { abstract public function __construct(); }
class ImplementsAbstract extends AbstractContract { public function __construct() {} }
abstract class AbstractCtor { abstract public function __construct(); }
class ConcreteCtor extends AbstractCtor { public function __construct() {} }
class ReplacedCtor extends ConcreteCtor { public function __construct() {} }
$method = new ReflectionMethod('ChildCtor', '__construct');
echo $method->hasPrototype() ? 'yes:' : 'no:';
if ($method->hasPrototype()) { echo $method->getPrototype()->getDeclaringClass()->getName(); }
echo "|", $method;
$method = new ReflectionMethod('GrandchildCtor', '__construct');
echo $method->hasPrototype() ? 'yes:' : 'no:';
if ($method->hasPrototype()) { echo $method->getPrototype()->getDeclaringClass()->getName(); }
echo "|", $method;
$method = new ReflectionMethod('ImplementsAbstract', '__construct');
echo $method->hasPrototype() ? 'yes:' : 'no:';
if ($method->hasPrototype()) { echo $method->getPrototype()->getDeclaringClass()->getName(); }
echo "|", $method;
$method = new ReflectionMethod('ReplacedCtor', '__construct');
echo $method->hasPrototype() ? 'yes:' : 'no:';
if ($method->hasPrototype()) { echo $method->getPrototype()->getDeclaringClass()->getName(); }
echo "|", $method;
"#);
    assert_eq!(out, "yes:Contract|Method [ <user, overwrites ParentCtor, prototype Contract, ctor> public method __construct ] {\n}\n\
        yes:Contract|Method [ <user, overwrites ChildCtor, prototype Contract, ctor> public method __construct ] {\n}\n\
        yes:Contract|Method [ <user, overwrites AbstractContract, prototype Contract, ctor> public method __construct ] {\n}\n\
        yes:AbstractCtor|Method [ <user, overwrites ConcreteCtor, prototype AbstractCtor, ctor> public method __construct ] {\n}\n");
}

/// Multiple overrides expose the root contract directly, including inherited and static methods.
#[test]
fn test_reflection_method_prototypes_reach_the_root_contract() {
    let out = compile_and_run(r#"<?php
class A { public function run() {} public static function s() {} }
class B extends A { public function run() {} public static function s() {} }
class C extends B { public function run() {} public static function s() {} }
class D extends C {}
interface I { public function run(); }
class P implements I { public function run() {} }
class Q extends P { public function run() {} }
class R extends Q {}
$method = new ReflectionMethod('C', 'run'); $prototype = $method->getPrototype();
echo $prototype->getDeclaringClass()->getName(), ":", $prototype->getName(), ":", $prototype->hasPrototype() ? 'Y' : 'N', "|", $method;
$method = new ReflectionMethod('C', 's'); $prototype = $method->getPrototype();
echo $prototype->getDeclaringClass()->getName(), ":", $prototype->getName(), ":", $prototype->hasPrototype() ? 'Y' : 'N', "|", $method;
$method = new ReflectionMethod('D', 'run'); $prototype = $method->getPrototype();
echo $prototype->getDeclaringClass()->getName(), ":", $prototype->getName(), ":", $prototype->hasPrototype() ? 'Y' : 'N', "|", $method;
$method = new ReflectionMethod('Q', 'run'); $prototype = $method->getPrototype();
echo $prototype->getDeclaringClass()->getName(), ":", $prototype->getName(), ":", $prototype->hasPrototype() ? 'Y' : 'N', "|", $method;
$method = new ReflectionMethod('R', 'run'); $prototype = $method->getPrototype();
echo $prototype->getDeclaringClass()->getName(), ":", $prototype->getName(), ":", $prototype->hasPrototype() ? 'Y' : 'N', "|", $method;
"#);
    assert_eq!(out, "A:run:N|Method [ <user, overwrites B, prototype A> public method run ] {\n}\n\
        A:s:N|Method [ <user, overwrites B, prototype A> static public method s ] {\n}\n\
        A:run:N|Method [ <user, inherits C, prototype A> public method run ] {\n}\n\
        I:run:N|Method [ <user, overwrites P, prototype I> public method run ] {\n}\n\
        I:run:N|Method [ <user, inherits Q, prototype I> public method run ] {\n}\n");
}

/// Prototype casts use their declaring scope while ordinary inherited views retain their labels.
#[test]
fn test_reflection_prototype_dumps_drop_intermediate_inherited_labels() {
    let out = compile_and_run(r#"<?php
class A { public function run() {} }
class B extends A {}
class C extends B { public function run() {} }
abstract class AA { abstract public function __construct(); }
abstract class BB extends AA {}
class CC extends BB { public function __construct() {} }
interface I { public function run(); }
interface J extends I {}
class D implements J { public function run() {} }
echo (new ReflectionMethod('C', 'run'))->getPrototype();
echo (new ReflectionMethod('CC', '__construct'))->getPrototype();
echo (new ReflectionMethod('D', 'run'))->getPrototype();
echo new ReflectionMethod('B', 'run');
echo new ReflectionMethod('BB', '__construct');
echo new ReflectionMethod('J', 'run');
"#);
    assert_eq!(out, "Method [ <user> public method run ] {\n}\n\
        Method [ <user, ctor> abstract public method __construct ] {\n}\n\
        Method [ <user> abstract public method run ] {\n}\n\
        Method [ <user, inherits A> public method run ] {\n}\n\
        Method [ <user, inherits AA, ctor> abstract public method __construct ] {\n}\n\
        Method [ <user, inherits I> abstract public method run ] {\n}\n");
}

/// AST-exported object arguments use double-quoted escapes for control characters.
#[test]
fn test_reflection_dump_object_string_escapes() {
    let out = compile_and_run(r#"<?php
class Box { public function __construct(public string $s) {} }
function f(Box $b = new Box("\n\t\\'é")) {}
echo new ReflectionParameter('f', 0);
"#);
    assert_eq!(out, "Parameter #0 [ <optional> Box $b = new \\Box(\"\\n\\t\\\\'é\") ]");
}

/// AST-exported bytes survive snapshots without corrupting genuine private-use parameter names.
#[test]
fn test_reflection_dump_object_binary_strings() {
    let source = r#"<?php
class Box { public function __construct(public string $s) {} }
function f(Box $c = new Box("\xFF"), Box $d = new Box("\n\xFF"),
    Box $PRIVATE_PARAMETER = new Box("é")) {}
foreach ((new ReflectionFunction('f'))->getParameters() as $p) echo bin2hex((string)$p), "\n";
echo strpos((string)new ReflectionFunction('f'), "\xFF") !== false ? "function-bytes\n" : "bad";
class Holder { public function accept(Box $v = new Box("\xFF")) {} }
echo strpos((string)new ReflectionMethod('Holder', 'accept'), "\xFF") !== false ? "method-bytes\n" : "bad";
foreach ((new ReflectionClass('Holder'))->getMethods() as $m) {
    if ($m->getName() === 'accept') echo strpos((string)$m, "\xFF") !== false ? "listed-bytes\n" : "bad";
}
"#.replace("PRIVATE_PARAMETER", "\u{e000}");
    let out = compile_and_run(&source);
    let parameters: [&[u8]; 3] = [
        b"Parameter #0 [ <optional> Box $c = new \\Box('\xff') ]",
        b"Parameter #1 [ <optional> Box $d = new \\Box(\"\\n\xff\") ]",
        b"Parameter #2 [ <optional> Box $\xee\x80\x80 = new \\Box('\xc3\xa9') ]",
    ];
    let mut expected = String::new();
    for parameter in parameters {
        expected.extend(parameter.iter().map(|byte| format!("{byte:02x}")));
        expected.push('\n');
    }
    expected.push_str("function-bytes\nmethod-bytes\nlisted-bytes\n");
    assert_eq!(out, expected);
}

/// Lazy defaults keep their written dump without running constructors.
#[test]
fn test_reflection_dump_deferred_array_defaults() {
    let out = compile_and_run(r#"<?php
class Box { public function __construct() { echo "constructed"; } }
function f(array $items = [new Box()]) {}
echo new ReflectionParameter('f', 0);
"#);
    assert_eq!(out, "Parameter #0 [ <optional> array $items = [new \\Box()] ]");
}

/// Evaluated arrays compact packed keys recursively and apply PHP's last-wins key normalization.
#[test]
fn test_reflection_dump_folded_array_defaults() {
    let out = compile_and_run(r#"<?php
function f(array $a = [0 => 1, 1 => 2], array $b = [false => 2],
    array $c = [0 => 1, 0 => 2], array $d = [[0 => 1]],
    array $e = [1.0 => 4, false => 5], array $f = [true => 1, 8, "2" => 3, 9],
    array $g = [9223372036854775807 => 1]) {}
foreach ((new ReflectionFunction('f'))->getParameters() as $p) echo $p, "\n";
"#);
    assert_eq!(out, "Parameter #0 [ <optional> array $a = [1, 2] ]\n\
        Parameter #1 [ <optional> array $b = [2] ]\n\
        Parameter #2 [ <optional> array $c = [2] ]\n\
        Parameter #3 [ <optional> array $d = [[1]] ]\n\
        Parameter #4 [ <optional> array $e = [1 => 4, 0 => 5] ]\n\
        Parameter #5 [ <optional> array $f = [1 => 1, 2 => 3, 3 => 9] ]\n\
        Parameter #6 [ <optional> array $g = [9223372036854775807 => 1] ]\n");
}

/// Null and fractional keys preserve the written array, including its other keys and nested forms.
#[test]
fn test_reflection_dump_unfolded_array_defaults() {
    let out = compile_and_run(r#"<?php
function f(array $a = [null => 3], array $b = [1.5 => 4], array $c = [null => 3, 1],
    array $d = [null => 3, "5" => 1, true => 2, false => 3, 1.5 => 4],
    array $e = [[null => 3], "5" => 1], array $f = [null => null]) {}
foreach ((new ReflectionFunction('f'))->getParameters() as $p) echo $p, "\n";
"#);
    assert_eq!(out, "Parameter #0 [ <optional> array $a = [null => 3] ]\n\
        Parameter #1 [ <optional> array $b = [1.5 => 4] ]\n\
        Parameter #2 [ <optional> array $c = [null => 3, 1] ]\n\
        Parameter #3 [ <optional> array $d = [null => 3, '5' => 1, true => 2, false => 3, 1.5 => 4] ]\n\
        Parameter #4 [ <optional> array $e = [[null => 3], '5' => 1] ]\n\
        Parameter #5 [ <optional> array $f = [null => null] ]\n");
}

/// Folded arrays inside object AST exports keep keys, while nonfoldable arrays keep written keys.
#[test]
fn test_reflection_dump_object_array_defaults() {
    let out = compile_and_run(r#"<?php
class Foo { public function __construct(public array $a) { echo 'constructed'; } }
function f(Foo $a = new Foo([0 => 1, 1 => 2]), Foo $b = new Foo([[0 => 1]]),
    Foo $c = new Foo([false => 2, "5" => 1, null => 3, 1.5 => 4, 0 => 1, 1 => 2]),
    Foo $d = new Foo([true => 1, 1.0 => 4, "05" => 2])) {}
foreach ((new ReflectionFunction('f'))->getParameters() as $p) echo $p, "\n";
"#);
    assert_eq!(out, "Parameter #0 [ <optional> Foo $a = new \\Foo([0 => 1, 1 => 2]) ]\n\
        Parameter #1 [ <optional> Foo $b = new \\Foo([0 => [0 => 1]]) ]\n\
        Parameter #2 [ <optional> Foo $c = new \\Foo([false => 2, '5' => 1, null => 3, 1.5 => 4, 0 => 1, 1 => 2]) ]\n\
        Parameter #3 [ <optional> Foo $d = new \\Foo([1 => 4, '05' => 2]) ]\n");
}

/// Written defaults keep builtin and unresolved constant names without evaluating them.
#[test]
fn test_reflection_dump_written_constant_names() {
    let out = compile_and_run(r#"<?php
class Foo { public function __construct(public mixed $a = null) { echo 'constructed'; } }
function f($a = [1, E_WARNING, PHP_VERSION], Foo $b = new Foo(E_WARNING),
    Foo $c = new Foo([PHP_VERSION => 1]), Foo $d = new Foo(MISSING_DUMP_CONSTANT),
    $e = [MISSING_DUMP_CONSTANT]) {}
foreach ((new ReflectionFunction('f'))->getParameters() as $p) echo $p, "\n";
"#);
    assert_eq!(out, "Parameter #0 [ <optional> $a = [1, E_WARNING, PHP_VERSION] ]\n\
        Parameter #1 [ <optional> Foo $b = new \\Foo(E_WARNING) ]\n\
        Parameter #2 [ <optional> Foo $c = new \\Foo([PHP_VERSION => 1]) ]\n\
        Parameter #3 [ <optional> Foo $d = new \\Foo(MISSING_DUMP_CONSTANT) ]\n\
        Parameter #4 [ <optional> $e = [MISSING_DUMP_CONSTANT] ]\n");
}

/// String keys use PHP byte identity across Unicode, byte escapes and class-name literals.
#[test]
fn test_reflection_dump_array_byte_key_identity() {
    let out = compile_and_run(r#"<?php
class Foo { public function __construct(public array $a) {} }
class ÉKey {}
function f(array $a = ["é" => 1, "\xC3\xA9" => 2],
    Foo $b = new Foo(["é" => 1, "\xC3\xA9" => 2]),
    array $c = [null => 1, "é" => 2, "\xC3\xA9" => 3],
    array $d = [ÉKey::class => 1, "\xC3\x89Key" => 2]) {}
foreach ((new ReflectionFunction('f'))->getParameters() as $p) echo $p, "\n";
"#);
    assert_eq!(out, "Parameter #0 [ <optional> array $a = ['\\xC3\\xA9' => 2] ]\n\
        Parameter #1 [ <optional> Foo $b = new \\Foo(['é' => 2]) ]\n\
        Parameter #2 [ <optional> array $c = [null => 1, 'é' => 2, 'é' => 3] ]\n\
        Parameter #3 [ <optional> array $d = ['\\xC3\\x89Key' => 2] ]\n");
}

/// Array-valued scoped constants retain their names across parameter, function and method dumps.
#[test]
fn test_reflection_dump_scoped_array_constant_defaults() {
    let out = compile_and_run(r#"<?php
class Items { const ITEMS = [1, 2]; }
interface ItemContract { const ITEMS = [3]; }
trait ItemTrait { public const ITEMS = [4]; }
class TraitItems { use ItemTrait; }
function f($a = Items::ITEMS, $b = ItemContract::ITEMS, $c = TraitItems::ITEMS) {}
class Methods {
    public function run($a = Items::ITEMS, $b = ItemContract::ITEMS, $c = TraitItems::ITEMS) {}
}
echo new ReflectionParameter('f', 0), "\n";
echo new ReflectionFunction('f');
echo new ReflectionMethod('Methods', 'run');
"#);
    assert_eq!(out, "Parameter #0 [ <optional> $a = Items::ITEMS ]\n\
        Function [ <user> function f ] {\n\n  - Parameters [3] {\n\
        \x20   Parameter #0 [ <optional> $a = Items::ITEMS ]\n\
        \x20   Parameter #1 [ <optional> $b = ItemContract::ITEMS ]\n\
        \x20   Parameter #2 [ <optional> $c = TraitItems::ITEMS ]\n  }\n}\n\
        Method [ <user> public method run ] {\n\n  - Parameters [3] {\n\
        \x20   Parameter #0 [ <optional> $a = Items::ITEMS ]\n\
        \x20   Parameter #1 [ <optional> $b = ItemContract::ITEMS ]\n\
        \x20   Parameter #2 [ <optional> $c = TraitItems::ITEMS ]\n  }\n}\n");
}

/// Scoped array constants inside object defaults print without executing constructors.
#[test]
fn test_reflection_dump_scoped_array_constants_in_object_defaults() {
    let out = compile_and_run(r#"<?php
class Items { const ITEMS = [1, 2]; }
interface ItemContract { const ITEMS = [3]; }
trait ItemTrait { public const ITEMS = [4]; }
class TraitItems { use ItemTrait; }
class Foo { public function __construct(public array $a = []) { echo 'constructed'; } }
function f(Foo $a = new Foo(Items::ITEMS), Foo $b = new Foo(ItemContract::ITEMS),
    Foo $c = new Foo(TraitItems::ITEMS)) {}
foreach ((new ReflectionFunction('f'))->getParameters() as $p) echo $p, "\n";
"#);
    assert_eq!(out, "Parameter #0 [ <optional> Foo $a = new \\Foo(Items::ITEMS) ]\n\
        Parameter #1 [ <optional> Foo $b = new \\Foo(ItemContract::ITEMS) ]\n\
        Parameter #2 [ <optional> Foo $c = new \\Foo(TraitItems::ITEMS) ]\n");
}

/// Inherited defaults fold relative class names in their declaration's scope.
#[test]
fn test_reflection_dump_inherited_default_class_scope() {
    let out = compile_and_run(r#"<?php
class BaseP { const NAME = 'base'; }
class ParentC extends BaseP {
    public function f($a = [self::class, self::NAME]) {}
    public function h($a = [parent::class]) {}
}
class ChildC extends ParentC {}
class Grand extends ChildC {}
echo (new ReflectionMethod('ChildC', 'f'))->getParameters()[0], "\n";
echo (new ReflectionMethod('Grand', 'h'))->getParameters()[0], "\n";
"#);
    assert_eq!(out, "Parameter #0 [ <optional> $a = ['ParentC', self::NAME] ]\n\
        Parameter #0 [ <optional> $a = ['BaseP'] ]\n");
}

/// Trait source syntax survives composition and aliases while evaluated values bind to the class.
#[test]
fn test_reflection_dump_trait_default_class_scope() {
    let out = compile_and_run(r#"<?php
trait Tself {
    public function f($a = [self::class]) {}
    public function g($a = self::class) {}
}
trait Outer { use Tself; }
class Cuse { use Outer { g as aliasG; } }
class ChildUse extends Cuse {}
class Own { use Tself; public function g($a = self::class) {} }
echo (new ReflectionMethod('Tself', 'f'))->getParameters()[0], "\n";
echo (new ReflectionMethod('Cuse', 'f'))->getParameters()[0], "\n";
$p = (new ReflectionMethod('ChildUse', 'aliasG'))->getParameters()[0];
echo $p, "\n", $p->getDefaultValue(), "\n";
echo (new ReflectionMethod('Own', 'g'))->getParameters()[0], "\n";
"#);
    assert_eq!(out, "Parameter #0 [ <optional> $a = [self::class] ]\n\
        Parameter #0 [ <optional> $a = [self::class] ]\n\
        Parameter #0 [ <optional> $a = self::class ]\nCuse\n\
        Parameter #0 [ <optional> $a = 'Own' ]\n");
}

/// Overflowing implicit keys stay written in dumps and never execute object constructors.
#[test]
fn test_reflection_dump_array_implicit_after_max_key() {
    let out = compile_and_run(r#"<?php
class Box { public function __construct(public array $a) { echo 'constructed'; } }
function f($a = [9223372036854775807 => 'a', 'b'],
    Box $b = new Box([9223372036854775807 => 1, 2]),
    $c = [9223372036854775807 => 'a', 9223372036854775807 => 'b', 'c'],
    $d = [1, 9223372036854775807 => 2, 3],
    $e = [9223372036854775806 => 1, 2]) {}
foreach ((new ReflectionFunction('f'))->getParameters() as $p) echo $p, "\n";
"#);
    assert_eq!(out, "Parameter #0 [ <optional> $a = [9223372036854775807 => 'a', 'b'] ]\n\
        Parameter #1 [ <optional> Box $b = new \\Box([9223372036854775807 => 1, 2]) ]\n\
        Parameter #2 [ <optional> $c = [9223372036854775807 => 'a', 9223372036854775807 => 'b', 'c'] ]\n\
        Parameter #3 [ <optional> $d = [1, 9223372036854775807 => 2, 3] ]\n\
        Parameter #4 [ <optional> $e = [9223372036854775806 => 1, 9223372036854775807 => 2] ]\n");
}

/// Standalone, union and nullable false types survive parameter and return metadata conversion.
#[test]
fn test_reflection_dump_false_type_metadata() {
    let out = compile_and_run(r#"<?php
function bare(): false { return false; }
function flagged(int|false $value = false): int|false { return $value; }
function wide(string|false|null $value = null): string|false|null { return $value; }
class Methods { public function run(?false $value = null): ?false { return $value; } }
echo new ReflectionFunction('bare');
echo new ReflectionFunction('flagged');
echo new ReflectionFunction('wide');
echo new ReflectionMethod('Methods', 'run');
"#);
    assert_eq!(out, "Function [ <user> function bare ] {\n\n  - Parameters [0] {\n  }\n\
        \x20 - Return [ false ]\n}\n\
        Function [ <user> function flagged ] {\n\n  - Parameters [1] {\n\
        \x20   Parameter #0 [ <optional> int|false $value = false ]\n  }\n\
        \x20 - Return [ int|false ]\n}\n\
        Function [ <user> function wide ] {\n\n  - Parameters [1] {\n\
        \x20   Parameter #0 [ <optional> string|false|null $value = NULL ]\n  }\n\
        \x20 - Return [ string|false|null ]\n}\n\
        Method [ <user> public method run ] {\n\n  - Parameters [1] {\n\
        \x20   Parameter #0 [ <optional> ?false $value = NULL ]\n  }\n\
        \x20 - Return [ ?false ]\n}\n");
}

/// Generic object names remain builtin through direct, nullable and union metadata.
#[test]
fn test_reflection_dump_generic_object_type_metadata() {
    let out = compile_and_run(r#"<?php
function f(object $a, ?object $b, object|int $c): object|int { return $c; }
echo new ReflectionFunction('f');
foreach ((new ReflectionFunction('f'))->getParameters() as $p) {
    $type = $p->getType();
    if ($type instanceof ReflectionNamedType) echo $type->getName(), ':', $type->isBuiltin(), "\n";
}
"#);
    assert_eq!(out, "Function [ <user> function f ] {\n\n  - Parameters [3] {\n\
        \x20   Parameter #0 [ <required> object $a ]\n\
        \x20   Parameter #1 [ <required> ?object $b ]\n\
        \x20   Parameter #2 [ <required> object|int $c ]\n  }\n\
        \x20 - Return [ object|int ]\n}\nobject:1\nobject:1\n");
}
