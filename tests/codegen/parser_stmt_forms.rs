//! Purpose:
//! End-to-end coverage for statement-level parser constructs:
//! attributes on global constants, destructuring in a `while` condition, statements that
//! start with a literal, and `include`/`require` assigned to a static property.
//!
//! Called from:
//! - `cargo test --test codegen_tests parser_stmt_forms`.
//!
//! Key details:
//! - Each test asserts PHP 8.5's output for the same source.

use crate::support::*;

// --- Attributes on global constants (PHP 8.5) ---

#[test]
fn test_attribute_on_global_const_is_accepted() {
    let out = compile_and_run(
        r#"<?php
#[Deprecated(message: 'use UUID_TYPE_RANDOM instead')]
const UUID_TYPE_DCE = 4;

#[Deprecated(message: 'use UUID_TYPE_TIME instead')]
const UUID_TYPE_NAME = 1;

#[MyMarker]
#[Other(1, 2), Third]
const PLAIN = 'plain';

echo PLAIN, "\n";
echo defined('UUID_TYPE_DCE') ? "defined\n" : "missing\n";
"#,
    );
    assert_eq!(out, "plain\ndefined\n");
}

#[test]
fn test_attribute_on_namespaced_const_is_accepted() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
#[\Deprecated]
const OLD = 3;
#[Marker]
const NEW_ONE = 4;
echo NEW_ONE + 1, "\n";
"#,
    );
    assert_eq!(out, "5\n");
}

#[test]
fn test_const_list_without_attributes_still_works() {
    let out = compile_and_run("<?php const A = 7, B = A + 1; echo A + B, \"\\n\";");
    assert_eq!(out, "15\n");
}

// --- Destructuring in a `while` condition (CarbonInterval:1006) ---

#[test]
fn test_while_condition_bracket_destructuring() {
    let out = compile_and_run(
        r#"<?php
$parts = [[1, 'a', true], [2, 'b', false], [3, 'c', true]];
while ([$num, $letter, $flag] = array_shift($parts)) {
    echo $num, $letter, $flag ? 'T' : 'F', "\n";
}
var_dump($num, $letter);
echo count($parts), "\n";
"#,
    );
    assert_eq!(out, "1aT\n2bF\n3cT\nNULL\nNULL\n0\n");
}

#[test]
fn test_while_condition_keyed_and_list_destructuring() {
    let out = compile_and_run(
        r#"<?php
$pairs = [['k' => 'x', 'v' => 10], ['k' => 'y', 'v' => 20]];
while (['k' => $k, 'v' => $v] = array_shift($pairs)) {
    echo "$k=$v\n";
}
$rows = [[5, [6, 7]]];
while (list($p, list($q, $r)) = array_shift($rows)) {
    echo $p * $q + $r, "\n";
}
var_dump($k, $r);
"#,
    );
    assert_eq!(out, "x=10\ny=20\n37\nNULL\nNULL\n");
}

#[test]
fn test_while_condition_destructuring_continue_and_break() {
    let out = compile_and_run(
        r#"<?php
$items = [[1, 2], [3, 4], [5, 6], [7, 8]];
while ([$a, $b] = array_shift($items)) {
    if ($a === 3) {
        continue;
    }
    if ($a === 7) {
        break;
    }
    echo $a + $b, "\n";
}
echo $a, count($items), "\n";
"#,
    );
    assert_eq!(out, "3\n11\n70\n");
}

#[test]
fn test_while_plain_assignment_condition_still_works() {
    let out = compile_and_run(
        r#"<?php
$q = [3, 2, 1];
while ($x = array_shift($q)) {
    echo $x;
}
$i = 0;
while ($i < 3) { $i++; }
echo "|", $i, "\n";
"#,
    );
    assert_eq!(out, "321|3\n");
}

#[test]
fn test_while_condition_destructuring_in_static_method_with_switch() {
    let out = compile_and_run(
        r#"<?php
class Parser
{
    public static function total(string $spec): int
    {
        preg_match_all('/(\d+)\h*([a-z]+)/i', $spec, $parts, PREG_SET_ORDER);
        $sum = 0;
        while ([$part, $value, $unit] = array_shift($parts)) {
            switch ($unit) {
                case 'h':
                    $sum += (int) $value * 60;
                    break;
                default:
                    $sum += (int) $value;
                    break;
            }
        }
        return $sum;
    }
}
echo Parser::total('2h 5m 3s'), "\n";
"#,
    );
    assert_eq!(out, "128\n");
}

#[test]
fn test_while_condition_destructuring_alt_syntax() {
    let out = compile_and_run(
        "<?php $q = [[1, 2]]; while ([$a, $b] = array_shift($q)): echo $a + $b; endwhile; echo \"\\n\";",
    );
    assert_eq!(out, "3\n");
}

// --- Statements that start with a literal (polyfill-intl-normalizer Normalizer.php:243) ---

#[test]
fn test_statement_starting_with_int_literal_short_circuit_assignment() {
    let out = compile_and_run(
        r#"<?php
function adjust(int $T): int
{
    0 > $T && $T += 0x40;
    return $T;
}
echo adjust(-5), " ", adjust(3), "\n";
"#,
    );
    assert_eq!(out, "59 3\n");
}

#[test]
fn test_statements_starting_with_other_literals_and_unary_operators() {
    let out = compile_and_run(
        r#"<?php
$s = 'abc';
42;
'abc' === $s and print("str-lead\n");
1.5 < 2 || print("never\n");
null ?? print("null-lead\n");
true && print("true-lead\n");
false || print("false-lead\n");
-1 < 0 && print("neg-lead\n");
!empty($s) && print("bang-lead\n");
PHP_EOL === "\n" && print("const-lead\n");
__LINE__ > 0 && print("magic-lead\n");
echo "done\n";
"#,
    );
    assert_eq!(
        out,
        "str-lead\nnull-lead\ntrue-lead\nfalse-lead\nneg-lead\nbang-lead\nconst-lead\nmagic-lead\ndone\n"
    );
}

// --- `include`/`require` assigned to a static property (polyfill-intl-idn Idn.php:261) ---

#[test]
fn test_require_assigned_to_static_property() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
class Holder
{
    public static $data;
    public static $other;
    public static $list = [];

    public static function load(): void
    {
        self::$data = require __DIR__ . '/data.php';
        static::$other = include 'data.php';
        Holder::$list[] = require_once __DIR__ . '/once.php';
        self::$list[] = include __DIR__ . '/data.php';
    }
}
Holder::load();
echo Holder::$data['a'], Holder::$data['b'], "\n";
echo Holder::$other['b'], "\n";
echo Holder::$list[0], "|", Holder::$list[1]['a'], "\n";
"#,
            ),
            ("data.php", "<?php\nreturn ['a' => 'alpha', 'b' => 'beta'];\n"),
            ("once.php", "<?php\nreturn 'first';\n"),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        "alphabeta\nbeta\nfirst|alpha\n"
    );
}

#[test]
fn test_require_assigned_to_local_still_works() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php\n$v = require 'v.php';\necho $v * 2, \"\\n\";\n"),
            ("v.php", "<?php\nreturn 21;\n"),
        ],
        "main.php",
    );
    assert_eq!(out, "42\n");
}

// --- `static $a = …, $b = …;` (symfony/polyfill-uuid Uuid.php:432) ---

#[test]
fn test_static_var_declaration_list() {
    let out = compile_and_run(
        r#"<?php
function tick(int $step): string
{
    static $total = 100, $calls = 0, $label = 'n';
    $calls++;
    $total = $total + $step;
    return $label . $calls . '=' . $total;
}
echo tick(1), ' ', tick(5), ' ', tick(10), "\n";
function single(): int { static $n = 0; return ++$n; }
echo single(), single(), "\n";
"#,
    );
    assert_eq!(out, "n1=101 n2=106 n3=116\n12\n");
}
