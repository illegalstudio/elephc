//! Purpose:
//! Runs php-src 8.6's `ext/standard/tests/math/clamp.phpt` as a codegen fixture.
//!
//! Called from:
//! - `cargo test --test codegen_tests clamp_php86`.
//!
//! Key details:
//! - The PHP source and expected output are transcribed from the vendored
//!   `tests/php_src/ext/standard/tests/math/clamp.phpt`; only two static-checker aids are
//!   added and neither changes stdout: `assert(...)` becomes an equivalent conditional
//!   throw (elephc has no `assert` builtin), and `check_clamp_result(...)::class` is split
//!   into a local plus an `instanceof Throwable` guard (elephc refuses `::class` on mixed).
//! - php-src's `run-tests.php` trims both sides before comparing, so this test does too.

use crate::support::*;

/// Returns the fixture's PHP source.
fn source() -> &'static str {
    r#"<?php

function make_clamp_fcc() {
    return clamp(...);
}

function check_clamp_result($value, $min, $max) {
    $flf = clamp($value, $min, $max);
    $dyn = make_clamp_fcc()($value, $min, $max);
    if (!($flf === $dyn || (is_nan($flf) && is_nan($dyn)))) {
        throw new \Exception('Direct and first-class clamp() disagree');
    }

    return $flf;
}

function check_clamp_exception($value, $min, $max) {
    try {
        var_dump(clamp($value, $min, $max));
    } catch (Throwable $error) {
        echo $error::class, ': ', $error->getMessage(), "\n";
    }

    try {
        var_dump(make_clamp_fcc()($value, $min, $max));
    } catch (Throwable $error) {
        echo $error::class, ': ', $error->getMessage(), "\n";
    }
}

var_dump(check_clamp_result(2, 1, 3));
var_dump(check_clamp_result(0, 1, 3));
var_dump(check_clamp_result(6, 1, 3));
var_dump(check_clamp_result(2, 1.3, 3.4));
var_dump(check_clamp_result(2.5, 1, 3));
var_dump(check_clamp_result(2.5, 1.3, 3.4));
var_dump(check_clamp_result(0, 1.3, 3.4));
var_dump(check_clamp_result(M_PI, -INF, INF));
var_dump(check_clamp_result(NAN, 4, 6));
var_dump(check_clamp_result("a", "c", "g"));
var_dump(check_clamp_result("d", "c", "g"));
echo check_clamp_result('2025-08-01', '2025-08-15', '2025-09-15'), "\n";
echo check_clamp_result('2025-08-20', '2025-08-15', '2025-09-15'), "\n";
echo check_clamp_result(new \DateTimeImmutable('2025-08-01'), new \DateTimeImmutable('2025-08-15'), new \DateTimeImmutable('2025-09-15'))->format('Y-m-d'), "\n";
echo check_clamp_result(new \DateTimeImmutable('2025-08-20'), new \DateTimeImmutable('2025-08-15'), new \DateTimeImmutable('2025-09-15'))->format('Y-m-d'), "\n";
var_dump(check_clamp_result(null, -1, 1));
var_dump(check_clamp_result(null, 1, 3));
var_dump(check_clamp_result(null, -3, -1));
var_dump(check_clamp_result(-9999, null, 10));
var_dump(check_clamp_result(12, null, 10));

$a = new \InvalidArgumentException('a');
$b = new \RuntimeException('b');
$c = new \LogicException('c');
$r1 = check_clamp_result($a, $b, $c);
if (!$r1 instanceof \Throwable) { throw new \Error('unreachable'); }
echo $r1::class, "\n";
$r2 = check_clamp_result($b, $a, $c);
if (!$r2 instanceof \Throwable) { throw new \Error('unreachable'); }
echo $r2::class, "\n";
$r3 = check_clamp_result($c, $a, $b);
if (!$r3 instanceof \Throwable) { throw new \Error('unreachable'); }
echo $r3::class, "\n";

check_clamp_exception(4, NAN, 6);
check_clamp_exception(7, 6, NAN);
check_clamp_exception(1, 3, 2);
check_clamp_exception(-9999, 5, null);
check_clamp_exception(12, -5, null);

"#
}

/// Returns the fixture's exact `--EXPECT--` output.
fn expected() -> &'static str {
    r#"int(2)
int(1)
int(3)
int(2)
float(2.5)
float(2.5)
float(1.3)
float(3.141592653589793)
float(NAN)
string(1) "c"
string(1) "d"
2025-08-15
2025-08-20
2025-08-15
2025-08-20
int(-1)
int(1)
int(-3)
int(-9999)
int(10)
InvalidArgumentException
RuntimeException
LogicException
ValueError: clamp(): Argument #2 ($min) must not be NAN
ValueError: clamp(): Argument #2 ($min) must not be NAN
ValueError: clamp(): Argument #3 ($max) must not be NAN
ValueError: clamp(): Argument #3 ($max) must not be NAN
ValueError: clamp(): Argument #2 ($min) must be smaller than or equal to argument #3 ($max)
ValueError: clamp(): Argument #2 ($min) must be smaller than or equal to argument #3 ($max)
ValueError: clamp(): Argument #2 ($min) must be smaller than or equal to argument #3 ($max)
ValueError: clamp(): Argument #2 ($min) must be smaller than or equal to argument #3 ($max)
ValueError: clamp(): Argument #2 ($min) must be smaller than or equal to argument #3 ($max)
ValueError: clamp(): Argument #2 ($min) must be smaller than or equal to argument #3 ($max)
"#
}

/// Compiles and runs the php-src clamp corpus and compares the trimmed stdout.
#[test]
fn test_clamp_php86_php_src_fixture() {
    let out = compile_and_run_with_php_version(source(), elephc::php_version::PhpVersion::Php86);
    assert_eq!(out.trim(), expected().trim());
}

/// A union-typed numeric argument keeps the typed runtime lowering and PHP's result types.
///
/// This guards the routing rule: only a `mixed` result goes through the prelude helper, so an
/// `int|float` argument family must not be handed to a helper that returns a boxed cell.
#[test]
fn test_clamp_php86_union_numeric_operands() {
    let source = r#"<?php
function f(int|float $x, int|float $min, int|float $max): int|float {
    return clamp($x, $min, $max);
}
var_dump(f(5, 0, 3), f(5.5, 0.0, 3.0), f(5.0, 0, 3), f(1, 2, 3));
"#;
    assert_eq!(
        compile_and_run(source),
        "int(3)\nfloat(3)\nint(3)\nint(2)\n"
    );
}
