//! Purpose:
//! End-to-end tests for `ReflectionMethod`, `ReflectionFunction` and `ReflectionParameter`
//! `__toString()` in compiled code.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - All three answered wrongly before #1080: the two callables returned the EMPTY STRING (their
//!   `__string` slot was declared and never filled) and a parameter returned its bare NAME
//!   (`__toString` read `__name`).
//! - Every expected string here is real `LC_ALL=C php` 8.5.10 output with one line removed: PHP
//!   prints `  @@ <file> <line> - <line>` plus a blank line after the header, which a compiled
//!   binary cannot honestly answer — the source it was built from need not exist where it runs.
//!   The eval bridge omits the same line, but still differs in three places the compiled path
//!   gets right: the prototype marker, the empty body, and the union order (#1117).
//! - A union prints in PHP's type-mask order, not the declared one: `int|string` comes back as
//!   `string|int`. See `reflection_union_member_rank`.
//! - An internal callable prints `<internal>` where PHP names the extension (`<internal:Core>`).
//!   The extension is not carried in the metadata; `<user>` would contradict the same object's
//!   `isInternal()`, which answers true.

use super::*;

/// The reported shape: a method with parameters, defaults and a return type.
#[test]
fn test_reflection_method_to_string_renders_the_php_dump() {
    let out = compile_and_run(
        r#"<?php
class Demo {
    public function plain(int $a, string $b = "x"): bool { return true; }
}
echo (new ReflectionMethod('Demo', 'plain'))->__toString();
"#,
    );
    assert_eq!(
        out,
        "Method [ <user> public method plain ] {\n  \
         - Parameters [2] {\n    \
         Parameter #0 [ <required> int $a ]\n    \
         Parameter #1 [ <optional> string $b = 'x' ]\n  \
         }\n  \
         - Return [ bool ]\n}\n"
    );
}

/// Modifiers print in PHP's order — abstract, final, static, then visibility — and an override
/// carries the class it came from in the `<user, prototype C>` marker.
#[test]
fn test_reflection_method_to_string_renders_modifiers_and_prototype() {
    let out = compile_and_run(
        r#"<?php
interface Shape { public function area(): float; }

abstract class Base implements Shape {
    abstract public function area(): float;
    final protected static function tag(): string { return "b"; }
}
echo (new ReflectionMethod('Base', 'area'))->__toString();
echo (new ReflectionMethod('Base', 'tag'))->__toString();
"#,
    );
    assert_eq!(
        out,
        "Method [ <user, prototype Shape> abstract public method area ] {\n  \
         - Parameters [0] {\n  }\n  - Return [ float ]\n}\n\
         Method [ <user> final static protected method tag ] {\n  \
         - Parameters [0] {\n  }\n  - Return [ string ]\n}\n"
    );
}

/// PHP prints an EMPTY body when a callable has no parameters and no declared return type, and
/// prints the parameters block as soon as either half has something to say. Measured on 8.5.10
/// with `function f() {}` against `function f(): void {}`.
#[test]
fn test_reflection_method_to_string_empty_body_without_parameters_or_return() {
    let out = compile_and_run(
        r#"<?php
class D {
    public function noneNoRet() {}
    public function noneWithRet(): void {}
    public function oneNoRet($a) {}
}
echo (new ReflectionMethod('D', 'noneNoRet'))->__toString();
echo (new ReflectionMethod('D', 'noneWithRet'))->__toString();
echo (new ReflectionMethod('D', 'oneNoRet'))->__toString();
"#,
    );
    assert_eq!(
        out,
        "Method [ <user> public method noneNoRet ] {\n}\n\
         Method [ <user> public method noneWithRet ] {\n  \
         - Parameters [0] {\n  }\n  - Return [ void ]\n}\n\
         Method [ <user> public method oneNoRet ] {\n  \
         - Parameters [1] {\n    Parameter #0 [ <required> $a ]\n  }\n}\n"
    );
}

/// By-reference, variadic, nullable and union parameters, and the union's print order.
#[test]
fn test_reflection_method_to_string_renders_every_parameter_shape() {
    let out = compile_and_run(
        r#"<?php
class Base {}
class Demo {
    public function mixup(int|string $v, ?Base $b = null, int &$out = 0, ...$rest): void {}
}
echo (new ReflectionMethod('Demo', 'mixup'))->__toString();
"#,
    );
    assert_eq!(
        out,
        "Method [ <user> public method mixup ] {\n  \
         - Parameters [4] {\n    \
         Parameter #0 [ <required> string|int $v ]\n    \
         Parameter #1 [ <optional> ?Base $b = NULL ]\n    \
         Parameter #2 [ <optional> int &$out = 0 ]\n    \
         Parameter #3 [ <optional> ...$rest ]\n  \
         }\n  - Return [ void ]\n}\n"
    );
}

/// Defaults render as PHP's dump does: arrays spelled out, a float keeping its decimal point, and
/// a constant default printing the CONSTANT's name rather than its value.
#[test]
fn test_reflection_method_to_string_renders_defaults() {
    let out = compile_and_run(
        r#"<?php
const LIMIT = 7;

class E {
    const CEIL = 9;
    public function defs(
        array $empty = [],
        array $filled = [1, 2],
        array $keyed = ['a' => 1],
        float $round = 1.0,
        bool $yes = true,
        int $glob = LIMIT,
        int $own = self::CEIL,
        ?string $nul = null
    ) {}
}
echo (new ReflectionMethod('E', 'defs'))->__toString();
"#,
    );
    assert_eq!(
        out,
        "Method [ <user> public method defs ] {\n  \
         - Parameters [8] {\n    \
         Parameter #0 [ <optional> array $empty = [] ]\n    \
         Parameter #1 [ <optional> array $filled = [1, 2] ]\n    \
         Parameter #2 [ <optional> array $keyed = ['a' => 1] ]\n    \
         Parameter #3 [ <optional> float $round = 1.0 ]\n    \
         Parameter #4 [ <optional> bool $yes = true ]\n    \
         Parameter #5 [ <optional> int $glob = LIMIT ]\n    \
         Parameter #6 [ <optional> int $own = self::CEIL ]\n    \
         Parameter #7 [ <optional> ?string $nul = NULL ]\n  \
         }\n}\n"
    );
}

/// `ReflectionFunction::__toString()` carries the same body under a `Function [ ... ]` header.
#[test]
fn test_reflection_function_to_string_renders_the_php_dump() {
    let out = compile_and_run(
        r#"<?php
function freeFn(float $f, array $a = [], ?callable $c = null): ?string { return null; }
function bare() {}
echo (new ReflectionFunction('freeFn'))->__toString();
echo (new ReflectionFunction('bare'))->__toString();
"#,
    );
    assert_eq!(
        out,
        "Function [ <user> function freeFn ] {\n  \
         - Parameters [3] {\n    \
         Parameter #0 [ <required> float $f ]\n    \
         Parameter #1 [ <optional> array $a = [] ]\n    \
         Parameter #2 [ <optional> ?callable $c = NULL ]\n  \
         }\n  - Return [ ?string ]\n}\n\
         Function [ <user> function bare ] {\n}\n"
    );
}

/// A parameter renders on its own too. `__toString` read the `__name` slot before #1080, so this
/// answered `a` where PHP answers the whole `Parameter #0 [ ... ]` line.
#[test]
fn test_reflection_parameter_to_string_renders_the_php_line() {
    let out = compile_and_run(
        r#"<?php
class Demo {
    public function plain(int $a, string $b = "x", ...$rest): bool { return true; }
}
foreach ((new ReflectionMethod('Demo', 'plain'))->getParameters() as $p) {
    echo (string) $p, "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Parameter #0 [ <required> int $a ]\n\
         Parameter #1 [ <optional> string $b = 'x' ]\n\
         Parameter #2 [ <optional> ...$rest ]\n"
    );
}

/// The entries `ReflectionClass::getMethods()` hands back carry the same rendering as the ones
/// built by `new ReflectionMethod(...)` — they are populated by a different emitter.
#[test]
fn test_listed_method_to_string_matches_the_constructed_one() {
    let out = compile_and_run(
        r#"<?php
class Demo {
    public function only(int $a): bool { return true; }
}
$listed = (new ReflectionClass('Demo'))->getMethods()[0];
$built = new ReflectionMethod('Demo', 'only');
echo $listed->__toString() === $built->__toString() ? "same" : "differs", "\n";
echo $listed->__toString();
"#,
    );
    assert_eq!(
        out,
        "same\nMethod [ <user> public method only ] {\n  \
         - Parameters [1] {\n    Parameter #0 [ <required> int $a ]\n  \
         }\n  - Return [ bool ]\n}\n"
    );
}

/// The eval bridge already rendered these; the compiled path now agrees with it, which is what
/// keeps a program's answer the same whether the reflection runs before or after `eval()`.
#[test]
fn test_compiled_and_eval_method_to_string_agree() {
    let out = compile_and_run(
        r#"<?php
class Demo {
    public function plain(int $a, string $b = "x"): bool { return true; }
}
$compiled = (new ReflectionMethod('Demo', 'plain'))->__toString();
eval('$fromEval = (new ReflectionMethod("Demo", "plain"))->__toString();');
echo $compiled === $fromEval ? "same" : "differs", "\n";
"#,
    );
    assert_eq!(out, "same\n");
}

/// A default written as a GLOBAL constant has to carry its VALUE as well as its name: the name
/// alone left `isDefaultValueConstant()` true while `isDefaultValueAvailable()` was false and
/// `getDefaultValue()` threw — a combination PHP never produces.
#[test]
fn test_global_constant_default_is_available_and_named() {
    let out = compile_and_run(
        r#"<?php
const LIMIT = 7;

class E { const CEIL = 9; }

function f(int $n = LIMIT, int $m = E::CEIL) {}

foreach ((new ReflectionFunction('f'))->getParameters() as $p) {
    echo $p->getName(), ":", $p->isDefaultValueAvailable() ? "y" : "n";
    echo ":", $p->isDefaultValueConstant() ? "y" : "n";
    echo ":", $p->getDefaultValueConstantName();
    echo ":", $p->getDefaultValue(), "\n";
}
"#,
    );
    assert_eq!(out, "n:y:y:LIMIT:7\nm:y:y:E::CEIL:9\n");
}

/// A namespaced constant answers with the RESOLVED name, which is what PHP answers too — measured
/// on 8.5.10, where `namespace N; const LIMIT = 7;` gives `'N\LIMIT'`.
#[test]
fn test_namespaced_constant_default_keeps_its_resolved_name() {
    let out = compile_and_run(
        r#"<?php
namespace N;

const LIMIT = 7;

function f(int $n = LIMIT) {}

$p = (new \ReflectionFunction("N\\f"))->getParameters()[0];
echo $p->getDefaultValueConstantName(), "|", $p->getDefaultValue(), "\n";
echo (new \ReflectionFunction("N\\f"))->__toString();
"#,
    );
    assert_eq!(
        out,
        "N\\LIMIT|7\nFunction [ <user> function N\\f ] {\n  \
         - Parameters [1] {\n    Parameter #0 [ <optional> int $n = N\\LIMIT ]\n  }\n}\n"
    );
}

/// PHP exports the written AST for an object default, so the arguments stay, the class name is
/// fully qualified, and a call with NO arguments keeps its empty parentheses however many the
/// constructor would supply by default.
#[test]
fn test_object_default_renders_its_written_arguments() {
    let out = compile_and_run(
        r#"<?php
class Foo { public function __construct(public int $a = 1, public string $b = "a") {} }

function f($x = new Foo(1, "a"), $y = new Foo()) {}

echo (new ReflectionFunction('f'))->__toString();
"#,
    );
    assert_eq!(
        out,
        "Function [ <user> function f ] {\n  \
         - Parameters [2] {\n    \
         Parameter #0 [ <optional> $x = new \\Foo(1, 'a') ]\n    \
         Parameter #1 [ <optional> $y = new \\Foo() ]\n  }\n}\n"
    );
}

/// PHP switches a float default to its exponent form outside a narrow decimal range, where Rust's
/// own `to_string` expands every digit. Measured on 8.5.10.
#[test]
fn test_float_defaults_use_phps_exponent_form() {
    let out = compile_and_run(
        r#"<?php
function f(float $big = 1e100, float $small = 1e-5, float $mid = 1e15, float $plain = 1.5, float $round = 1.0) {}
echo (new ReflectionFunction('f'))->__toString();
"#,
    );
    assert_eq!(
        out,
        "Function [ <user> function f ] {\n  \
         - Parameters [5] {\n    \
         Parameter #0 [ <optional> float $big = 1.0E+100 ]\n    \
         Parameter #1 [ <optional> float $small = 1.0E-5 ]\n    \
         Parameter #2 [ <optional> float $mid = 1.0E+15 ]\n    \
         Parameter #3 [ <optional> float $plain = 1.5 ]\n    \
         Parameter #4 [ <optional> float $round = 1.0 ]\n  }\n}\n"
    );
}

/// `getDeclaringFunction()` builds its reflector from metadata that deliberately carries no
/// parameters — emitting them would emit a `ReflectionParameter` for each, and each of those emits
/// its own declaring function, which does not terminate. The DUMP is a string, so it is rendered
/// from the real member instead of from that metadata; without that it claimed `Parameters [0]`
/// for a method that has two.
#[test]
fn test_declaring_function_dump_carries_the_real_parameters() {
    let out = compile_and_run(
        r#"<?php
class Demo { public function plain(int $a, string $b): bool { return true; } }
function freeFn(int $x, string $y): void {}

$p = (new ReflectionMethod('Demo', 'plain'))->getParameters()[0];
echo $p->getDeclaringFunction()->__toString();
$q = (new ReflectionFunction('freeFn'))->getParameters()[0];
echo $q->getDeclaringFunction()->__toString();
"#,
    );
    assert_eq!(
        out,
        "Method [ <user> public method plain ] {\n  \
         - Parameters [2] {\n    \
         Parameter #0 [ <required> int $a ]\n    \
         Parameter #1 [ <required> string $b ]\n  }\n  - Return [ bool ]\n}\n\
         Function [ <user> function freeFn ] {\n  \
         - Parameters [2] {\n    \
         Parameter #0 [ <required> int $x ]\n    \
         Parameter #1 [ <required> string $y ]\n  }\n  - Return [ void ]\n}\n"
    );
}

/// An internal callable's dump must not claim `<user>` while the same object's `isInternal()`
/// answers true. PHP names the extension (`<internal:Core>` for `strlen`); the metadata reaching
/// the renderer does not carry it, so the tag stops at `<internal>`.
#[test]
fn test_internal_function_dump_says_internal() {
    let out = compile_and_run(
        r#"<?php
$r = new ReflectionFunction('strlen');
$s = (string) $r;
echo substr($s, 0, strpos($s, ']') + 1), "|", $r->isInternal() ? "y" : "n", "\n";
"#,
    );
    assert_eq!(out, "Function [ <internal> function strlen ]|y\n");
}

/// Verifies a supported callable BUILTIN's declaring dump keeps its parameters.
///
/// `function_by_name` only knows generated functions and closures, so `strlen` missed and the
/// dump fell back to the deliberately parameterless metadata the reflector carries to stay out of
/// the `ReflectionParameter` -> `getDeclaringFunction()` -> `ReflectionParameter` cycle. It then
/// claimed `Parameters [0]` for a function PHP reports one parameter for.
///
/// The two leading reads are not decoration. Stringifying this object in a program that does
/// nothing else SEGFAULTS, on this branch and at the merge-base alike (#1229) — a pre-existing
/// crash this fixture must not trip over while covering the parameters, so it is written in the
/// shape that runs.
#[test]
fn test_a_builtin_declaring_function_dump_keeps_its_parameters() {
    let out = compile_and_run(
        r#"<?php
$p = new ReflectionParameter('strlen', 'string');
echo $p->getName(), "|";
$f = $p->getDeclaringFunction();
echo $f->getName(), "|";
echo (string) $f;
"#,
    );
    assert_eq!(
        out,
        "string|strlen|Function [ <internal> function strlen ] {\n  \
         - Parameters [1] {\n    \
         Parameter #0 [ <required> string $string ]\n  }\n  - Return [ int ]\n}\n"
    );
}

/// Verifies the non-finite float defaults use PHP's spelling rather than Rust's.
///
/// `f64::to_string` gives `inf`, `-inf` and `NaN`; PHP 8.5.10 prints `INF`, `-INF` and `NAN`.
#[test]
fn test_non_finite_float_defaults_use_php_spelling() {
    let out = compile_and_run(
        r#"<?php
function nonFinite(float $x = INF, float $y = -INF, float $z = NAN) { return 0; }
echo (string) new ReflectionFunction('nonFinite');
"#,
    );
    assert_eq!(
        out,
        "Function [ <user> function nonFinite ] {\n  \
         - Parameters [3] {\n    \
         Parameter #0 [ <optional> float $x = INF ]\n    \
         Parameter #1 [ <optional> float $y = -INF ]\n    \
         Parameter #2 [ <optional> float $z = NAN ]\n  }\n}\n"
    );
}

/// Verifies a global constant this evaluator cannot fold does not fail the COMPILE.
///
/// `ReflectionConstantValue` has no array variant, so routing global constants through the
/// fallible evaluator made `const ITEMS = [1, 2]; function f($items = ITEMS) {}` a compile error
/// for the whole program. The metadata is still missing — PHP reports the array — but the program
/// builds, and the predicates then behave exactly as they do for a parameter with no default,
/// which is what PHP does in that state too.
#[test]
fn test_an_unfoldable_global_constant_default_still_compiles() {
    let out = compile_and_run(
        r#"<?php
const ITEMS = [1, 2];
const LIMIT = 7;
function withArray($x = ITEMS) { return 0; }
function withScalar($x = LIMIT) { return 0; }

$a = new ReflectionParameter('withArray', 0);
$s = new ReflectionParameter('withScalar', 0);
echo $a->isDefaultValueAvailable() ? "y" : "n";
echo $s->isDefaultValueAvailable() ? "y" : "n";
echo $s->isDefaultValueConstant() ? "y" : "n";
echo $s->getDefaultValue();
"#,
    );
    assert_eq!(out, "nyy7");
}
