//! Purpose:
//! Regression coverage for Reflection constant arithmetic and resolved default fallbacks.
//!
//! Called from:
//! - `codegen_tests` through the `oop` module.
//!
//! Key details:
//! - Supported constant expressions retain their values without executing constructors on dump.
//! - Operator spelling uses the documented evaluated-value fallback, unlike PHP's AST export.
//! - Numeric constant metadata preserves PHP types, overflow promotion, and power rounding.

use super::*;

/// Declaring-function dumps retain source defaults whose evaluation raises a deferred Error.
#[test]
fn test_declaring_function_dump_preserves_lazy_source_defaults() {
    let out = compile_and_run(r#"<?php
function lazyOwner($x = new self()) {}
$function = new ReflectionFunction('lazyOwner');
$parameter = $function->getParameters()[0];
echo $function, $parameter->getDeclaringFunction();
try { $parameter->getDefaultValue(); }
catch (Error $error) { echo get_class($error), ':', $error->getMessage(), "\n"; }
"#);
    let dump = "Function [ <user> function lazyOwner ] {\n\n  \
        - Parameters [1] {\n    \
        Parameter #0 [ <optional> $x = new self() ]\n  }\n}\n";
    assert_eq!(out, format!("{dump}{dump}Error:Cannot access \"self\" when no class scope is active\n"));
}

/// Reflected constant arithmetic preserves exact integers and promotes overflow like PHP.
#[test]
fn test_reflection_constant_arithmetic_preserves_php_numeric_results() {
    let out = compile_and_run(r#"<?php
const LIMIT = PHP_INT_MAX;
const LOWER = PHP_INT_MIN;
const EXPONENT = 63;
const HUGE = 4294967296;
const BASE = 3;
class Values {
    const MAXIMUM = LIMIT;
    const TWO = 2;
    const THREE = 3;
    const ADD = LIMIT + 1;
    const SCOPED_ADD = self::MAXIMUM + 1;
    const SUB = LOWER - 1;
    const MUL = LIMIT * 2;
    const MOD = LOWER % -1;
    const POW = 2 ** EXPONENT;
    const PRECISE_POW = BASE ** 41;
    const HUGE_POW = 2 ** HUGE;
    const NEGATIVE_POW = (-2) ** LIMIT;
    const IDENTITY_POW = 1 ** HUGE;
    const NEGATIVE_IDENTITY = (-1) ** LIMIT;
    const SAFE_POW = self::TWO ** self::THREE;
    const EXACT_DIV = 6 / self::THREE;
    const FRACTIONAL_DIV = 7 / self::TWO;
    const MIN_DIV = LOWER / -1;
    const SAFE_ADD = self::THREE + self::TWO;
    const SAFE_SUB = self::TWO - self::THREE;
    const SAFE_MUL = self::TWO * self::THREE;
}
$class = new ReflectionClass('Values');
foreach ([
    'ADD' => 9223372036854775808.0, 'SCOPED_ADD' => 9223372036854775808.0,
    'SUB' => -9223372036854775808.0, 'MUL' => 18446744073709551616.0,
    'POW' => 9223372036854775808.0, 'PRECISE_POW' => 3.647299637717079e19,
    'HUGE_POW' => INF, 'NEGATIVE_POW' => -INF, 'FRACTIONAL_DIV' => 3.5,
    'MIN_DIV' => 9223372036854775808.0,
] as $name => $expected) {
    $value = $class->getConstant($name);
    echo $name, is_float($value) && $value === $expected ? ":F\n" : ":bad\n";
}
foreach ([
    'MOD' => 0, 'IDENTITY_POW' => 1, 'NEGATIVE_IDENTITY' => -1, 'SAFE_POW' => 8,
    'EXACT_DIV' => 2, 'SAFE_ADD' => 5, 'SAFE_SUB' => -1, 'SAFE_MUL' => 6,
] as $name => $expected) {
    $value = $class->getConstant($name);
    echo $name, is_int($value) && $value === $expected ? ":I\n" : ":bad\n";
}
"#);
    assert_eq!(out, "ADD:F\nSCOPED_ADD:F\nSUB:F\nMUL:F\nPOW:F\nPRECISE_POW:F\n\
        HUGE_POW:F\nNEGATIVE_POW:F\nFRACTIONAL_DIV:F\nMIN_DIV:F\nMOD:I\nIDENTITY_POW:I\n\
        NEGATIVE_IDENTITY:I\nSAFE_POW:I\nEXACT_DIV:I\nSAFE_ADD:I\nSAFE_SUB:I\nSAFE_MUL:I\n");
}

/// Negating the minimum integer through a constant promotes to float without compiler overflow.
#[test]
fn test_reflection_dump_negated_minimum_constant() {
    let out = compile_and_run(r#"<?php
class Values { const Y = PHP_INT_MIN; const X = -self::Y; }
class Foo { public function __construct(public float $x) { echo "ctor\n"; } }
function f($a = Values::X, Foo $b = new Foo(Values::X)) {}
$a = new ReflectionParameter('f', 0);
$b = new ReflectionParameter('f', 1);
echo $a, "\n", $b, "\n";
echo $a->getDefaultValue() === 9223372036854775808.0 ? "float\n" : "wrong\n";
$o = $b->getDefaultValue();
echo $o->x === 9223372036854775808.0 ? "float\n" : "wrong\n";
"#);
    assert_eq!(out, "Parameter #0 [ <optional> $a = Values::X ]\n\
        Parameter #1 [ <optional> Foo $b = new \\Foo(Values::X) ]\nfloat\nctor\nfloat\n");
}

/// Nested array and constructor expressions retain folded defaults and omit unwritten args.
#[test]
fn test_reflection_dump_resolved_compound_default_fallbacks() {
    // PHP retains LIMIT - 1 in these dumps; this subset intentionally exports the resolved value.
    let out = compile_and_run_capture(r#"<?php
const LIMIT = 7;
class Foo {
    public function __construct(public int $x, public int $y = 9) { echo "ctor\n"; }
}


function f($a = [LIMIT - 1, [LIMIT - 2, 1 + 2]], Foo $b = new Foo(LIMIT - 1)) {}
$a = new ReflectionParameter('f', 0);
$b = new ReflectionParameter('f', 1);
echo $a, "\n", $b, "\n";
echo $a->isDefaultValueAvailable() ? "yes\n" : "no\n";
echo $b->isDefaultValueAvailable() ? "yes\n" : "no\n";
$v = $a->getDefaultValue();
echo $v[0], ':', $v[1][0], ':', $v[1][1], "\n";
$o = $b->getDefaultValue();
echo $o->x, ':', $o->y, "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Parameter #0 [ <optional> $a = [6, [5, 3]] ]\n\
        Parameter #1 [ <optional> Foo $b = new \\Foo(6) ]\nyes\nyes\n6:5:3\nctor\n6:9\n");
}

/// Named constructor slots normalize independently of written argument order and dump spelling.
#[test]
fn test_reflection_dump_named_compound_default_fallbacks() {
    // The documented fallback evaluates LIMIT arithmetic while retaining each written name.
    let out = compile_and_run_capture(r#"<?php
const LIMIT = 7;
class Foo {
    public function __construct(public int $x = 4, public int $y = 9) { echo "ctor\n"; }
}
function f(Foo $a = new Foo(y: LIMIT - 1)) {}
function g(Foo $b = new Foo(y: LIMIT - 1, x: LIMIT - 2)) {}
function h(Foo $c = new Foo(LIMIT - 2, y: LIMIT - 1)) {}
function i(Foo $d = new Foo(x: LIMIT - 1)) {}
$a = new ReflectionParameter('f', 0);
$b = new ReflectionParameter('g', 0);
$c = new ReflectionParameter('h', 0);
$d = new ReflectionParameter('i', 0);
echo $a, "\n", $b, "\n", $c, "\n", $d, "\n";
echo $a->isDefaultValueAvailable(), $b->isDefaultValueAvailable(),
    $c->isDefaultValueAvailable(), $d->isDefaultValueAvailable(), "\n";
$first = $a->getDefaultValue(); echo $first->x, ':', $first->y, "\n";
$second = $b->getDefaultValue(); echo $second->x, ':', $second->y, "\n";
$third = $c->getDefaultValue(); echo $third->x, ':', $third->y, "\n";
$fourth = $d->getDefaultValue(); echo $fourth->x, ':', $fourth->y, "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Parameter #0 [ <optional> Foo $a = new \\Foo(y: 6) ]\n\
        Parameter #0 [ <optional> Foo $b = new \\Foo(y: 6, x: 5) ]\n\
        Parameter #0 [ <optional> Foo $c = new \\Foo(5, y: 6) ]\n\
        Parameter #0 [ <optional> Foo $d = new \\Foo(x: 6) ]\n\
        1111\nctor\n4:6\nctor\n5:6\nctor\n5:6\nctor\n6:9\n");
}
