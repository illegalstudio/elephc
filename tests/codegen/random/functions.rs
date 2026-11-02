//! Purpose:
//! Integration tests for the global `random` extension functions (`mt_srand`, `srand`,
//! `rand`, `mt_rand`, `mt_getrandmax`, `getrandmax`, `random_bytes`, `random_int`,
//! `lcg_value`) ported from php-src `ext/random/tests/01_functions`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixed seeds make every assertion deterministic and cross-checkable against php-src.
//! - Deprecation notices for `MT_RAND_PHP`/`lcg_value` go to stderr, so these fixtures
//!   assert stdout only.

use crate::support::*;

/// `mt_getrandmax()`/`getrandmax()` both report PHP's 31-bit MT19937 maximum.
#[test]
fn test_random_getrandmax_is_31_bit() {
    let out = compile_and_run(
        r#"<?php
var_dump(mt_getrandmax());
var_dump(getrandmax());
"#,
    );
    assert_eq!(out, "int(2147483647)\nint(2147483647)\n");
}

/// Reproduces php-src's MT19937 reference stream for `mt_srand(12345678)`.
#[test]
fn test_random_mt_rand_mt19937_sequence() {
    let out = compile_and_run(
        r#"<?php
mt_srand(12345678);
for ($i = 0; $i < 16; $i++) {
    echo mt_rand(), "\n";
}
$x = 0;
for ($i = 0; $i < 1024; $i++) {
    $x ^= mt_rand();
}
echo $x, "\n";
"#,
    );
    assert_eq!(
        out,
        "527860569\n1711027313\n1280820687\n688176834\n770499160\n412773096\n\
813703253\n898651287\n52508912\n757323740\n511765911\n274407457\n833082629\n\
1923803667\n1461450755\n1301698200\n1612214270\n"
    );
}

/// Reproduces php-src's historical `MT_RAND_PHP` biased stream, including its bug.
#[test]
fn test_random_mt_rand_php_mode_sequence() {
    let out = compile_and_run(
        r#"<?php
mt_srand(12345678, MT_RAND_PHP);
for ($i = 0; $i < 16; $i++) {
    echo mt_rand(), "\n";
}
$x = 0;
for ($i = 0; $i < 1024; $i++) {
    $x ^= mt_rand();
}
echo $x, "\n";
"#,
    );
    assert_eq!(
        out,
        "1614640687\n1711027313\n857485497\n688176834\n1386682158\n412773096\n\
813703253\n898651287\n2087374214\n1382556330\n1640700129\n1863374167\n\
1324097651\n1923803667\n676334965\n853386222\n1571178311\n"
    );
}

/// `MT_RAND_PHP` keeps php-src's double-based bad-scaling formula for ranged draws.
#[test]
fn test_random_mt_rand_php_mode_bad_scaling() {
    let out = compile_and_run(
        r#"<?php
mt_srand(0, MT_RAND_PHP);
var_dump(mt_rand(0, 999999999));
var_dump(mt_rand(0, 999));
"#,
    );
    assert_eq!(out, "int(448865905)\nint(592)\n");
}

/// An unknown seed mode is silently clamped to `MT_RAND_MT19937`, like php-src.
#[test]
fn test_random_mt_srand_unknown_mode_clamps() {
    let out = compile_and_run(
        r#"<?php
mt_srand(1, 0);
var_dump(mt_rand());
mt_srand(1, 1);
var_dump(mt_rand());
mt_srand(1, 2);
var_dump(mt_rand());
mt_srand(1, 256);
var_dump(mt_rand());
"#,
    );
    assert_eq!(
        out,
        "int(895547922)\nint(1244335972)\nint(895547922)\nint(895547922)\n"
    );
}

/// `rand()` swaps inverted bounds instead of rejecting them.
#[test]
fn test_random_rand_swaps_inverted_bounds() {
    let out = compile_and_run(
        r#"<?php
$min = 10;
$max = 20;
$correct = rand($min, $max);
$incorrect = rand($max, $min);
var_dump($correct >= $min, $correct <= $max);
var_dump($incorrect >= $min, $incorrect <= $max);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\n"
    );
}

/// `mt_rand()` rejects inverted bounds with php-src's exact `ValueError` wording.
#[test]
fn test_random_mt_rand_rejects_inverted_bounds() {
    let out = compile_and_run(
        r#"<?php
mt_srand(1234);
var_dump(mt_rand(3, 8) >= 3);
try {
    mt_rand(8, 3);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
echo "Done.\n";
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nmt_rand(): Argument #2 ($max) must be greater than or equal to argument #1 ($min)\nDone.\n"
    );
}

/// Zero-argument and ranged draws come from the same seeded MT19937 stream.
#[test]
fn test_random_mt_rand_seeded_ranged_draws() {
    let out = compile_and_run(
        r#"<?php
mt_srand(1234);
echo mt_rand(), "\n";
echo mt_rand(0, 999), "\n";
echo mt_rand(10, 100), "\n";
$x = 0;
for ($i = 0; $i < 100; $i++) {
    $x ^= mt_rand();
}
echo $x, "\n";
mt_srand(1234, MT_RAND_PHP);
echo mt_rand(), "\n";
echo mt_rand(0, 999), "\n";
"#,
    );
    assert_eq!(out, "411284887\n171\n94\n357572096\n1741177057\n497\n");
}

/// Zero-argument `mt_rand()` stays inside PHP's `[0, 2147483647]` range.
#[test]
fn test_random_mt_rand_zero_arg_range() {
    let out = compile_and_run(
        r#"<?php
mt_srand(42);
$ok = true;
for ($i = 0; $i < 1000; $i++) {
    $v = mt_rand();
    if ($v < 0 || $v > mt_getrandmax()) {
        $ok = false;
    }
}
echo $ok ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// `random_bytes()` returns exactly the requested number of bytes.
#[test]
fn test_random_bytes_lengths() {
    let out = compile_and_run(
        r#"<?php
foreach ([16, 10, 257] as $length) {
    $bytes = random_bytes($length);
    echo strlen($bytes), " ", strlen(bin2hex($bytes)), "\n";
}
"#,
    );
    assert_eq!(out, "16 32\n10 20\n257 514\n");
}

/// `random_int()` honours its inclusive bounds and the `min == max` shortcut.
#[test]
fn test_random_int_bounds_and_fixed_point() {
    let out = compile_and_run(
        r#"<?php
$ok = true;
for ($i = 0; $i < 1000; $i++) {
    $v = random_int(-10, 10);
    if (!is_int($v) || $v < -10 || $v > 10) {
        $ok = false;
    }
}
var_dump($ok);
var_dump(random_int(42, 42));
var_dump(random_int(PHP_INT_MIN, PHP_INT_MAX) >= PHP_INT_MIN);
"#,
    );
    assert_eq!(out, "bool(true)\nint(42)\nbool(true)\n");
}

/// `random_int()` rejects an inverted range with php-src's exact message.
#[test]
fn test_random_int_rejects_inverted_bounds() {
    let out = compile_and_run(
        r#"<?php
try {
    random_int(42, 0);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "random_int(): Argument #1 ($min) must be less than or equal to argument #2 ($max)\n"
    );
}

/// `lcg_value()` stays in `[0, 1)` and advances its combined LCG.
#[test]
fn test_random_lcg_value_unit_interval() {
    let out = compile_and_run(
        r#"<?php
$ok = true;
$previous = -1.0;
$changed = false;
for ($i = 0; $i < 100; $i++) {
    $v = lcg_value();
    if ($v < 0.0 || $v >= 1.0) {
        $ok = false;
    }
    if ($v !== $previous) {
        $changed = true;
    }
    $previous = $v;
}
var_dump($ok, $changed);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

/// `array_rand()` consumes the seeded global MT19937 stream.
#[test]
fn test_random_array_rand_uses_seeded_mt_stream() {
    let out = compile_and_run(
        r#"<?php
mt_srand(1234);
$array = range(1, 10);
var_dump(array_rand($array));
var_dump(array_rand($array));
"#,
    );
    assert_eq!(out, "int(5)\nint(1)\n");
}


/// `random_bytes()` rejects a non-positive length with php-src's `ValueError` message.
#[test]
fn test_random_bytes_rejects_non_positive_length() {
    let out = compile_and_run(
        r#"<?php
foreach ([0, -1] as $length) {
    try {
        random_bytes($length);
    } catch (ValueError $e) {
        echo $e->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        "random_bytes(): Argument #1 ($length) must be greater than 0\n\
random_bytes(): Argument #1 ($length) must be greater than 0\n"
    );
}

/// `array_rand()` supports associative arrays and the multi-key form on the seeded stream.
#[test]
fn test_random_array_rand_assoc_and_multi() {
    let out = compile_and_run(
        r#"<?php
mt_srand(1234);
$list = range(1, 10);
$map = ["foo" => 0, "bar" => 1, "baz" => 2];
var_dump(array_rand($map));
var_dump(array_rand($map, 2));
var_dump(array_rand($list));
var_dump(array_rand($list, 2));
"#,
    );
    assert_eq!(
        out,
        "string(3) \"foo\"\n\
array(2) {\n  [0]=>\n  string(3) \"bar\"\n  [1]=>\n  string(3) \"baz\"\n}\n\
int(6)\n\
array(2) {\n  [0]=>\n  int(5)\n  [1]=>\n  int(6)\n}\n"
    );
}

/// `array_rand()` validates the array and the requested key count like php-src.
#[test]
fn test_random_array_rand_errors() {
    let out = compile_and_run(
        r#"<?php
try {
    $empty = array_fill(0, 0, 0);
    array_rand($empty);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
foreach ([0, 4] as $num) {
    try {
        array_rand([1, 2, 3], $num);
    } catch (ValueError $e) {
        echo $e->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        "array_rand(): Argument #1 ($array) must not be empty\n\
array_rand(): Argument #2 ($num) must be between 1 and the number of elements in argument #1 ($array)\n\
array_rand(): Argument #2 ($num) must be between 1 and the number of elements in argument #1 ($array)\n"
    );
}

/// `array_rand()` and `Randomizer::pickArrayKeys()` consume the same Mt19937 stream.
#[test]
fn test_random_array_rand_matches_pick_array_keys() {
    let out = compile_and_run(
        r#"<?php
$map = ["foo" => 0, "bar" => 1, "baz" => 2];
$list = range(1, 10);

mt_srand(1234);
$mapOne = array_rand($map, 1);
$mapTwo = array_rand($map, 2);
$listOne = array_rand($list, 1);
$listTwo = array_rand($list, 2);

function pick(Random\Randomizer $r, array $array, int $num): array {
    return $r->pickArrayKeys($array, $num);
}
$r = new Random\Randomizer(new Random\Engine\Mt19937(1234));
[$mapPickOne] = pick($r, $map, 1);
$mapPickTwo = pick($r, $map, 2);
[$listPickOne] = pick($r, $list, 1);
$listPickTwo = pick($r, $list, 2);

var_dump(
    $mapOne === $mapPickOne,
    $mapTwo === $mapPickTwo,
    $listOne === $listPickOne,
    $listTwo === $listPickTwo,
);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\n"
    );
}


/// The random constants are predefined, so a program that only reads them needs no prelude.
#[test]
fn test_random_constants_are_always_available() {
    let out = compile_and_run(
        r#"<?php
var_dump(MT_RAND_MT19937, MT_RAND_PHP);
"#,
    );
    assert_eq!(out, "int(0)\nint(1)\n");
}

/// `shuffle()` consumes the seeded global Mt19937 stream on every target.
#[test]
fn test_random_shuffle_uses_seeded_mt_stream() {
    let out = compile_and_run(
        r#"<?php
mt_srand(1234);
$array = range(1, 5);
shuffle($array);
var_dump($array);
"#,
    );
    assert_eq!(
        out,
        "array(5) {\n  [0]=>\n  int(3)\n  [1]=>\n  int(2)\n  [2]=>\n  int(5)\n  [3]=>\n  int(4)\n  [4]=>\n  int(1)\n}\n"
    );
}

/// First-class `array_rand(...)` callables use the multi-key prelude implementation.
#[test]
fn test_random_array_rand_first_class_callable() {
    let out = compile_and_run(
        r#"<?php
$pick = array_rand(...);
$list = range(1, 10);
var_dump(is_int($pick($list)));
var_dump(count($pick($list, 2)));
"#,
    );
    assert_eq!(out, "bool(true)\nint(2)\n");
}


/// Dynamic calls through a function-name string still inject the prelude.
#[test]
fn test_random_dynamic_function_name_injects_prelude() {
    let out = compile_and_run(
        r#"<?php
$bytes = "random_bytes";
echo strlen($bytes(16)), "\n";
$max = "mt_getrandmax";
var_dump($max());
"#,
    );
    assert_eq!(out, "16\nint(2147483647)\n");
}

/// Reading `MT_RAND_PHP` emits PHP 8.3's constant-level deprecation at run time.
#[test]
fn test_random_mt_rand_php_constant_read_deprecation() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo MT_RAND_PHP, "\n";
"#,
    );
    assert_eq!(out.stdout, "1\n");
    assert!(
        out.stderr.contains(
            "Constant MT_RAND_PHP is deprecated since 8.3, as it uses a biased non-standard variant of Mt19937"
        ),
        "{}",
        out.stderr
    );
    // The diagnostic belongs to the read, not to a synthetic prelude helper span.
    assert!(
        out.stderr.contains("on line 2"),
        "the deprecation must report the read's own line: {}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("on line 1000"),
        "the deprecation leaked a synthetic span: {}",
        out.stderr
    );
}

/// `MT_RAND_PHP` reads in `switch` cases and `match` arms deprecate where they are written.
#[test]
fn test_random_mt_rand_php_switch_and_match_reads() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
switch (1) {
    case MT_RAND_PHP:
        echo "switch\n";
        break;
}
echo match (1) {
    MT_RAND_PHP => "match\n",
    default => "other\n",
};
"#,
    );
    assert_eq!(out.stdout, "switch\nmatch\n");
    assert!(out.stderr.contains("on line 3"), "{}", out.stderr);
    assert!(out.stderr.contains("on line 8"), "{}", out.stderr);
    assert!(!out.stderr.contains("on line 1000"), "{}", out.stderr);
}

/// Notices raised inside prelude-implemented functions report the user's call site.
#[test]
fn test_random_prelude_notices_report_the_call_site() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $level, string $message, string $file, int $line): bool {
    echo basename($file), ":", $line, " ", $message, "\n";
    return true;
});
lcg_value();
mt_srand(0, MT_RAND_PHP);
new Random\Engine\Mt19937(0, MT_RAND_PHP);
trigger_error("user", E_USER_NOTICE);
function inner(): void {
    trigger_error("inner", E_USER_NOTICE);
}
inner();
"#,
    );
    assert_eq!(
        out,
        "test.php:6 Function lcg_value() is deprecated since 8.4, use \\Random\\Randomizer::getFloat() instead\n\
test.php:7 Constant MT_RAND_PHP is deprecated since 8.3, as it uses a biased non-standard variant of Mt19937\n\
test.php:7 The MT_RAND_PHP variant of Mt19937 is deprecated\n\
test.php:8 Constant MT_RAND_PHP is deprecated since 8.3, as it uses a biased non-standard variant of Mt19937\n\
test.php:8 The MT_RAND_PHP variant of Mt19937 is deprecated\n\
test.php:9 user\n\
test.php:11 inner\n"
    );
}

/// Passing `MT_RAND_PHP` as a mode emits the constant deprecation and the variant notice.
#[test]
fn test_random_mt_rand_php_mode_argument_deprecations() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
mt_srand(0, MT_RAND_PHP);
var_dump(mt_rand(0, 999999999));
"#,
    );
    assert_eq!(out.stdout, "int(448865905)\n");
    assert!(
        out.stderr
            .contains("Constant MT_RAND_PHP is deprecated since 8.3"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr
            .contains("The MT_RAND_PHP variant of Mt19937 is deprecated"),
        "{}",
        out.stderr
    );
}

/// The literal `1` selects the mode without claiming a deprecated-constant read.
#[test]
fn test_random_mt_rand_php_literal_mode_has_no_constant_deprecation() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
mt_srand(0, 1);
var_dump(mt_rand(0, 999999999));
"#,
    );
    assert_eq!(out.stdout, "int(448865905)\n");
    assert!(
        !out.stderr.contains("Constant MT_RAND_PHP is deprecated"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr
            .contains("The MT_RAND_PHP variant of Mt19937 is deprecated"),
        "{}",
        out.stderr
    );
}

/// `MT_RAND_PHP` remains usable inside a constant expression (no function-call lowering).
#[test]
fn test_random_mt_rand_php_constant_expression() {
    let out = compile_and_run(
        r#"<?php
const RANDOM_MODE = MT_RAND_PHP;
var_dump(RANDOM_MODE);
"#,
    );
    assert_eq!(out, "int(1)\n");
}

/// `random_bytes()` reports its argument count at run time like PHP, including dynamic calls.
#[test]
fn test_random_bytes_runtime_arity_errors() {
    let out = compile_and_run(
        r#"<?php
foreach ([
    fn() => random_bytes(),
    fn() => random_bytes(1, 2),
    function () { $f = "random_bytes"; return $f(); },
    function () { $f = "random_bytes"; return $f(1, 2); },
    function () { $f = "random_bytes"; return $f(1, 2, 3); },
    function () { $f = random_bytes(...); return $f(); },
    fn() => call_user_func("random_bytes"),
    fn() => call_user_func_array("random_bytes", [1, 2]),
] as $case) {
    try {
        $case();
        echo "no error\n";
    } catch (ArgumentCountError $e) {
        echo $e->getMessage(), "\n";
    }
}
$f = "random_bytes";
var_dump(strlen($f(16)));
"#,
    );
    assert_eq!(
        out,
        "random_bytes() expects exactly 1 argument, 0 given\n\
random_bytes() expects exactly 1 argument, 2 given\n\
random_bytes() expects exactly 1 argument, 0 given\n\
random_bytes() expects exactly 1 argument, 2 given\n\
random_bytes() expects exactly 1 argument, 3 given\n\
random_bytes() expects exactly 1 argument, 0 given\n\
random_bytes() expects exactly 1 argument, 0 given\n\
random_bytes() expects exactly 1 argument, 2 given\n\
int(16)\n"
    );
}

/// `random_bytes()` rejects unknown named parameters with PHP's `Error`, before arity.
#[test]
fn test_random_bytes_rejects_unknown_named_parameters() {
    let out = compile_and_run(
        r#"<?php
foreach ([
    fn() => random_bytes(foo: 1),
    fn() => random_bytes(16, foo: 1),
    function () { $f = "random_bytes"; return $f(foo: 1); },
] as $case) {
    try {
        $case();
        echo "no error\n";
    } catch (Error $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
var_dump(strlen(random_bytes(length: 16)));
var_dump(strlen(random_bytes(...[16])));
"#,
    );
    assert_eq!(
        out,
        "Error: Unknown named parameter $foo\n\
Error: Unknown named parameter $foo\n\
Error: Unknown named parameter $foo\n\
int(16)\n\
int(16)\n"
    );
}

/// A statically empty unpack keeps a valid one-argument `random_bytes()` call valid.
#[test]
fn test_random_bytes_accepts_empty_unpack() {
    let out = compile_and_run(
        r#"<?php
var_dump(strlen(random_bytes(16, ...[])));
"#,
    );
    assert_eq!(out, "int(16)\n");
}

/// A namespaced user function named `random_bytes` is not the PHP builtin.
#[test]
fn test_random_bytes_namespaced_user_function_is_not_rewritten() {
    let out = compile_and_run(
        r#"<?php
namespace App {
    function random_bytes(int $first, int $second): string {
        return "user:" . $first . ":" . $second;
    }
}
namespace {
    echo \App\random_bytes(1, 2), "\n";
}
"#,
    );
    assert_eq!(out, "user:1:2\n");
}
