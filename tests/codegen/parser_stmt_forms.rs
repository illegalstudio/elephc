//! Purpose:
//! End-to-end coverage for statement-level parser constructs:
//! attributes on global constants, destructuring in a `while` condition, statements that
//! start with a literal or prefix operator, and `static $a, $b` declaration lists.
//!
//! Called from:
//! - `cargo test --test codegen_tests parser_stmt_forms`.
//!
//! Key details:
//! - Each test asserts PHP 8.5's output for the same source.

use crate::support::*;

// --- Attributes on global constants (PHP 8.5) ---

/// Verifies that attributes (single, stacked and grouped) before a global `const` are parsed
/// and ignored, and that the constants stay defined.
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

/// Verifies that attributes before a namespaced `const` are accepted, including a fully
/// qualified `#[\Deprecated]`.
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

/// Control: already passes before this change; guards the unchanged form.
#[test]
fn test_const_list_without_attributes_still_works() {
    let out = compile_and_run("<?php const A = 7, B = A + 1; echo A + B, \"\\n\";");
    assert_eq!(out, "15\n");
}

// --- Destructuring in a `while` condition (CarbonInterval:1006) ---

/// Verifies `while ([$a, $b, $c] = array_shift($q))`: the body sees each unpacked row, and the
/// final failed unpack leaves the targets `null` as in PHP.
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

/// Verifies keyed (`['k' => $k]`) and nested `list()` patterns in a `while` condition, including
/// the `null` targets left by the terminating iteration.
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

/// Verifies that `continue` re-runs the destructuring head and `break` leaves the loop with the
/// last unpacked values, in a `while` whose condition destructures.
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

/// Control: already passes before this change; guards the unchanged form.
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

/// Verifies a destructuring `while` condition inside a static method whose body uses `switch`
/// with `break`, which must only leave the `switch`, not the loop.
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

/// Verifies a destructuring condition in the alternative `while (...): ... endwhile;` syntax.
#[test]
fn test_while_condition_destructuring_alt_syntax() {
    let out = compile_and_run(
        "<?php $q = [[1, 2]]; while ([$a, $b] = array_shift($q)): echo $a + $b; endwhile; echo \"\\n\";",
    );
    assert_eq!(out, "3\n");
}

// --- Statements that start with a literal (polyfill-intl-normalizer Normalizer.php:243) ---

/// Verifies a statement that starts with an integer literal (`0 > $T && $T += 0x40;`) parses as
/// an expression statement whose short-circuited assignment runs only when the guard holds.
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

/// Verifies expression statements that start with string, float, `null`, `true`/`false`
/// literals, unary `-`/`!`, constants and magic constants.
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

// --- `static $a = …, $b = …;` (symfony/polyfill-uuid Uuid.php:432) ---

/// Verifies `static $a = ..., $b = ..., $c = ...;` declares every listed static local with its
/// initializer and keeps their values across calls.
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

// --- Loop exit analysis when the loop can break before its `return` ---

/// Regression: a destructuring `while` whose body returns must still fall through to the code
/// after the loop when the first unpack fails. The rewritten `while (true)` head breaks before
/// the body's `return`, so the loop is not terminal and the trailing `return 99` must survive.
#[test]
fn test_while_condition_destructuring_body_return_keeps_fallback_return() {
    let out = compile_and_run(
        r#"<?php
function first(array $q): int { while ([$a] = array_shift($q)) { return $a; } return 99; }
function firstPositive(array $q): int {
    while ([$a] = array_shift($q)) {
        if ($a > 0) {
            return $a;
        }
    }
    return 77;
}
echo first([]), " ", first([[5]]), " ", firstPositive([[0], [0]]), "\n";
"#,
    );
    assert_eq!(out, "99 5 77\n");
}

/// Regression: `while (true)`, `for (;;)` and `do ... while (true)` bodies that may `break`
/// before their `return` are not terminal, so the `return` after the loop is kept.
#[test]
fn test_infinite_loop_that_breaks_before_return_keeps_fallback_return() {
    let out = compile_and_run(
        r#"<?php
function w(array $q): int { while (true) { $x = array_shift($q); if (!$x) { break; } return $x; } return 99; }
function f(array $q): int { for (;;) { $x = array_shift($q); if (!$x) { break; } return $x; } return 98; }
function d(array $q): int { do { $x = array_shift($q); if (!$x) { break; } return $x; } while (true); return 97; }
echo w([]), " ", w([4]), " ", f([]), " ", d([]), " ", d([3]), "\n";
"#,
    );
    assert_eq!(out, "99 4 98 97 3\n");
}
