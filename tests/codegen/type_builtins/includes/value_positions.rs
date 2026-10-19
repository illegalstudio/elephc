//! Purpose:
//! Integration tests for `include`/`require` used as an expression in any position: a call
//! argument, an operand, an array element, a static-property value, `??=`, a ternary branch.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `crate::resolver::value_include_hoist` binds each include to a hidden local in front of the
//!   statement, so these pin both the value and the caller-scope semantics of the hoisted form.
//! - Expected outputs were measured with PHP 8.5.

use super::*;

/// Carbon's lang-file shape: `return array_replace_recursive(require __DIR__.'/en.php', [...]);`
/// inside an included file, with the nested `__DIR__` resolved against that file.
#[test]
fn test_require_as_call_argument_with_dir() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php $fr = require 'lang/fr.php'; echo $fr['a'], ' ', $fr['b'], ' ', $fr['n']['y'];",
            ),
            (
                "lang/en.php",
                "<?php echo '[en]'; return ['a' => 'A-en', 'b' => 'B-en', 'n' => ['x' => 1, 'y' => 2]];",
            ),
            (
                "lang/fr.php",
                "<?php return array_replace_recursive(require __DIR__.'/en.php', ['b' => 'B-fr', 'n' => ['y' => 20]]);",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "[en]A-en B-fr 20");
}

/// An include as an operand, a parenthesized operand, an array element and a negated value;
/// a file without `return` contributes `1`.
#[test]
fn test_include_as_operand_and_array_element() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php $x = 10 + require 'num.php'; echo $x, '|';
echo (require 'num.php') . '!|';
echo 1 + include 'noreturn.php', '|';
$arr = [require 'num.php', 'k' => include 'num.php'];
echo $arr[0] + $arr['k'], '|', -require 'num.php';",
            ),
            ("num.php", "<?php return 32;"),
            ("noreturn.php", "<?php echo '[nr]';"),
        ],
        "main.php",
    );
    assert_eq!(out, "42|32!|[nr]2|64|-32");
}

/// The included file of a hoisted include runs in the caller's scope, in a function as well.
#[test]
fn test_include_expression_shares_caller_scope() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php $a = 5; echo 'v=' . (require 'scope.php'), '|', $b, '|';
function inner(): int { $a = 100; $r = max(require 'scope.php', 0); return $r + $b; }
echo inner();",
            ),
            ("scope.php", "<?php $b = $a * 2; return $b + 1;"),
        ],
        "main.php",
    );
    assert_eq!(out, "v=11|10|401");
}

/// `??=` with an include value runs the file only while the target is null, for a static
/// property (Carbon `Language::$languagesNames`) and a local.
#[test]
fn test_null_coalesce_assign_include() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php class Language { protected static $names;
    public static function names() { static::$names ??= require 'table.php'; return static::$names; } }
echo Language::names()['t'], Language::names()['t'], '|';
$local = null; $local ??= require 'num.php'; $local ??= require 'table.php'; echo $local;",
            ),
            ("table.php", "<?php echo '[t]'; return ['t' => 7];"),
            ("num.php", "<?php return 32;"),
        ],
        "main.php",
    );
    assert_eq!(out, "[t]77|32");
}

/// An include on the lazy side of `?:`, `&&` and `||` runs only when PHP evaluates that side.
#[test]
fn test_include_in_short_circuit_operands() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php $e = ''; $a = $e ?: require 'num.php'; $b = 'set' ?: require 'num.php';
$t = true && require 'num.php'; $f = false && require 'num.php'; $o = true || require 'num.php';
echo $a, '|', $b, '|'; var_dump($t, $f, $o);",
            ),
            ("num.php", "<?php echo '[n]'; return 32;"),
        ],
        "main.php",
    );
    assert_eq!(out, "[n][n]32|set|bool(true)\nbool(false)\nbool(true)\n");
}

/// An include in a ternary branch runs only when that branch is taken.
#[test]
fn test_include_in_ternary_branch() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function pick(bool $c) { return $c ? require 'num.php' : require 'table.php'; }
echo pick(true), '|', pick(false)['t'], '|';
$c = false; $v = $c ? require 'num.php' : 'skip'; echo $v;",
            ),
            ("table.php", "<?php echo '[t]'; return ['t' => 7];"),
            ("num.php", "<?php echo '[n]'; return 32;"),
        ],
        "main.php",
    );
    assert_eq!(out, "[n]32|[t]7|skip");
}

/// A repeat `_once` include evaluates to `true`, in any position.
#[test]
fn test_once_include_value_is_true_on_repeat() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php var_dump(require_once 'once.php'); var_dump(require_once 'once.php');
var_dump(include_once 'once.php'); $x = require_once 'once.php'; var_dump($x);",
            ),
            ("once.php", "<?php echo '[once]'; return 'first';"),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        "[once]string(5) \"first\"\nbool(true)\nbool(true)\nbool(true)\n"
    );
}

/// A static property assigned an include value, plain and compound.
#[test]
fn test_static_property_include_value() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php class K { public static $s = 'x'; public static $a = []; }
K::$s .= include 'str.php'; K::$a[] = require 'str.php'; K::$a[0] .= require 'str.php';
echo K::$s, '|', K::$a[0];",
            ),
            ("str.php", "<?php return 'S';"),
        ],
        "main.php",
    );
    assert_eq!(out, "xS|SS");
}

/// Operands PHP has already evaluated when an include of the same statement
/// runs keep the value they had then, even when the included file writes them: an array
/// element (`INIT_ARRAY`), a static property (`FETCH_STATIC_PROP_R`), a dimension read, a
/// method receiver and a dynamic callee are snapshotted in front of the include, while a
/// variable operand of a binary operator is read when the operator runs, after it.
#[test]
fn test_include_keeps_operands_evaluated_before_it() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php class K { public static int $p = 1; }
class O { function __construct(public string $n) {} function m($v) { return $this->n . \"->m($v)\"; } }
$x = 1; $arr = [$x, require 'setx.php']; echo implode(',', $arr), '|';
$x = 1; echo $x + require 'setx.php', '|';
echo K::$p + require 'setk.php', '|';
$a = [1]; $a[0] = $a[0] + require 'seta.php'; echo $a[0], '|';
$a = [1]; $a[0] += require 'seta.php'; echo $a[0], '|';
$o = new O('first'); $fn = function ($v) { return \"fn1($v)\"; };
echo $o->m(require 'seto.php'), '|';
$o = new O('first'); $fn = function ($v) { return \"fn1($v)\"; };
echo $fn(require 'seto.php');",
            ),
            ("setx.php", "<?php $x = 2; return 10;"),
            ("setk.php", "<?php K::$p = 100; return 10;"),
            ("seta.php", "<?php $a[0] = 100; return 10;"),
            (
                "seto.php",
                "<?php $o = new O('second'); $fn = function ($v) { return \"fn2($v)\"; }; return 5;",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "1,10|12|11|11|110|first->m(5)|fn1(5)");
}

/// An effectful operand before an include is materialized in order instead of
/// being refused: `f(g(), require F, h())` runs `g`, the file, then `h`.
#[test]
fn test_include_after_effectful_operand_keeps_order() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function g() { echo 'g '; return 1; } function h() { echo 'h '; return 3; }
function f($a, $b, $c) { echo \"f $a $b $c\"; }
f(g(), require 'ret5.php', h());",
            ),
            ("ret5.php", "<?php echo '[ret5] '; return 5;"),
        ],
        "main.php",
    );
    assert_eq!(out, "g [ret5] h f 1 5 3");
}

/// An array literal fetches a variable KEY when it adds the element, after the
/// element's value, so a key the included value writes is the new one; a computed key runs
/// before the value; an earlier element and a later include keep PHP's order too.
#[test]
fn test_array_literal_variable_key_is_read_after_its_value() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function k($s) { echo \"[k$s]\"; return \"k$s\"; }
$x = 'oldx'; $i = 1;
echo json_encode(['a' => 0, $x => require 'f.php']), '|';
echo json_encode([$i => require 'f.php']), '|';
$x = 'oldx'; echo json_encode([k(1) => require 'f.php', $x => k(2)]), '|';
$x = 'oldx'; echo json_encode([$x => 'first', 'z' => require 'f.php']), '|';
$x = 'oldx'; echo json_encode([$x => require 'f.php', 'b' => require 'g.php']);",
            ),
            ("f.php", "<?php echo '[f]'; $x = 'NEWX'; $i = 9; return 'V';"),
            ("g.php", "<?php $x = 'LATER'; return 'W';"),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        "[f]{\"a\":0,\"NEWX\":\"V\"}|[f]{\"9\":\"V\"}|[k1][f][k2]{\"k1\":\"V\",\"NEWX\":\"k2\"}|[f]{\"oldx\":\"first\",\"z\":\"V\"}|[f]{\"NEWX\":\"V\",\"b\":\"W\"}"
    );
}
