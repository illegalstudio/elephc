//! Purpose:
//! Regression tests for issue #771: a local whose `if` arms leave different representations
//! (a string against `null`, an int against a float, an object against `null`, ...) must be read
//! back after the merge as the value of whichever arm ran.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The fixtures keep the `if` away from a trailing `return` or put it in a closure or the top
//!   level, because DCE's tail-sinking hid the bug in named functions by copying the tail into
//!   both arms. Expected output was produced by PHP 8.5.
//! - Conditions read `$argc` or a parameter so the AST optimizer cannot fold the branch away.

use super::*;

/// Issue #771 repro: a by-value capture assigned `null` and then a string in one arm keeps the
/// string after the `if` merge instead of reading the slot back as `null`.
#[test]
fn test_closure_local_retype_null_then_string_in_branch_by_value_capture() {
    let out = compile_and_run(
        r#"<?php
$m = "k" . $argc;
$f = function (int $n) use ($m) { $m = null; if ($n > 1) { $m = "s" . $n; } return $m; };
var_dump($f(2));
var_dump($f(1));
"#,
    );
    assert_eq!(out, "string(2) \"s2\"\nNULL\n");
}

/// The same merge when the joined name is a closure parameter rather than a capture.
#[test]
fn test_closure_local_retype_null_then_string_in_branch_parameter() {
    let out = compile_and_run(
        r#"<?php
$p = function (int $n, string $m) { $m = null; if ($n > 1) { $m = "s" . $n; } return $m; };
var_dump($p(3, "a"));
var_dump($p(0, "a"));
"#,
    );
    assert_eq!(out, "string(2) \"s3\"\nNULL\n");
}

/// The same merge for a plain closure local that was first copied from a capture.
#[test]
fn test_closure_local_retype_null_then_string_in_branch_plain_local() {
    let out = compile_and_run(
        r#"<?php
$m2 = "c" . $argc;
$l = function (int $n) use ($m2) { $x = $m2; $x = null; if ($n > 1) { $x = "s" . $n; } return $x; };
var_dump($l(4));
var_dump($l(1));
"#,
    );
    assert_eq!(out, "string(2) \"s4\"\nNULL\n");
}

/// An arrow function capturing a local that an `if` joined in its enclosing closure sees the
/// value of whichever arm ran.
#[test]
fn test_closure_local_retype_joined_local_captured_by_arrow_fn() {
    let out = compile_and_run(
        r#"<?php
$ar = function (int $n) {
    $w = null;
    if ($n > 1) { $w = "w" . $n; }
    $get = fn() => $w;
    return $get();
};
var_dump($ar(5));
var_dump($ar(0));
"#,
    );
    assert_eq!(out, "string(2) \"w5\"\nNULL\n");
}

/// An `if`/`elseif` chain that leaves an int, a float or a string in one closure local returns
/// each arm's value with its own type.
#[test]
fn test_closure_local_retype_int_float_string_arms_join() {
    let out = compile_and_run(
        r#"<?php
$r = function (int $n) {
    $v = 1;
    if ($n === 1) { $v = 2.5; } elseif ($n === 2) { $v = "str" . $n; }
    return $v;
};
var_dump($r(0));
var_dump($r(1));
var_dump($r(2));
"#,
    );
    assert_eq!(out, "int(1)\nfloat(2.5)\nstring(4) \"str2\"\n");
}

/// Arms whose storage shares one scalar word (`false`/int, `null`/int) or holds `null` as a zero
/// pointer (object, array) are boxed on the merge edge, so neither path reads the other's shape.
#[test]
fn test_closure_local_retype_scalar_word_and_pointer_arms_join() {
    let out = compile_and_run(
        r#"<?php
$b = function (int $n) { $v = false; if ($n > 0) { $v = 10 * $n; } return $v; };
var_dump($b(0));
var_dump($b(3));
$d = function (int $n) { $v = null; if ($n > 0) { $v = $n; } return $v; };
var_dump($d(0));
var_dump($d(4));
$o = function (int $n) { $v = null; if ($n > 0) { $v = new ArrayObject([$n]); } return $v === null ? "none" : count($v); };
var_dump($o(0));
var_dump($o(2));
$a = function (int $n) { $v = null; if ($n > 0) { $v = [$n, $n + 1]; } return $v; };
var_dump($a(0));
var_dump($a(7));
"#,
    );
    assert_eq!(out, "bool(false)\nint(30)\nNULL\nint(4)\nstring(4) \"none\"\nint(1)\nNULL\narray(2) {\n  [0]=>\n  int(7)\n  [1]=>\n  int(8)\n}\n");
}

/// Both an inner closure and the closure that calls it join a `null`-or-string local correctly.
#[test]
fn test_closure_local_retype_nested_closure_branch_join() {
    let out = compile_and_run(
        r#"<?php
$outer = function (int $n) {
    $inner = function (int $k) { $s = null; if ($k > 1) { $s = "in" . $k; } return $s; };
    $t = null;
    if ($n > 0) { $t = $inner($n + 1); }
    return [$t, $inner(0)];
};
var_dump($outer(1));
var_dump($outer(0));
"#,
    );
    assert_eq!(out, "array(2) {\n  [0]=>\n  string(3) \"in2\"\n  [1]=>\n  NULL\n}\narray(2) {\n  [0]=>\n  NULL\n  [1]=>\n  NULL\n}\n");
}

/// Closures created inside instance and static methods join a `null`-or-string local correctly.
#[test]
fn test_closure_local_retype_method_scoped_closure_branch_join() {
    let out = compile_and_run(
        r#"<?php
class Box {
    private string $tag = "box";
    public function make(): Closure {
        return function (int $n) { $s = null; if ($n > 1) { $s = $this->tag . $n; } return $s; };
    }
    public static function smake(): Closure {
        return static function (int $n) { $s = null; if ($n > 1) { $s = "st" . $n; } return $s; };
    }
}
$bx = (new Box())->make();
var_dump($bx(9));
var_dump($bx(1));
$sx = Box::smake();
var_dump($sx(9));
var_dump($sx(1));
"#,
    );
    assert_eq!(out, "string(4) \"box9\"\nNULL\nstring(3) \"st9\"\nNULL\n");
}

/// A `null`-or-string local joined inside `for` and `foreach` bodies carries the right value
/// across iterations and out of the loop.
#[test]
fn test_closure_local_retype_branch_join_inside_loops() {
    let out = compile_and_run(
        r#"<?php
$loop = function (int $n) {
    $last = null;
    for ($i = 0; $i < $n; $i++) {
        if ($i % 2 === 1) { $last = "odd" . $i; }
    }
    return $last;
};
var_dump($loop(0));
var_dump($loop(1));
var_dump($loop(6));
$scan = function (array $xs) {
    $found = null;
    foreach ($xs as $x) {
        if ($x > 10) { $found = "big" . $x; }
        echo $found ?? "-", " ";
    }
    echo "\n";
    return $found;
};
var_dump($scan([1, 20, 3, 40]));
"#,
    );
    assert_eq!(out, "NULL\nNULL\nstring(4) \"odd5\"\n- big20 big20 big40 \nstring(5) \"big40\"\n");
}

/// The top-level program joins divergent `if` arms the same way: before #771 was fixed every one
/// of these printed the fall-through arm's view (`NULL`, `int(2)`, `string(0) ""`).
#[test]
fn test_local_retype_branch_join_top_level_program() {
    let out = compile_and_run(
        r#"<?php
$a = null; if ($argc == 1) { $a = "t" . $argc; } var_dump($a);
$b = 1; if ($argc == 1) { $b = 2.5; } var_dump($b);
$c = "s" . $argc; if ($argc == 1) { $c = null; } var_dump($c);
$d = null; if ($argc == 1) { $d = 7; } var_dump($d);
$e = null; if ($argc == 1) { $e = [1, 2]; } var_dump($e);
$f = null; if ($argc == 1) { $f = 1.5; } var_dump($f);
$g = null; if ($argc == 1) { $g = new stdClass(); } var_dump($g);
$i = null; if ($argc == 1) { $i = true; } var_dump($i);
$j = null; if ($argc == 2) { $j = "never"; } var_dump($j);
"#,
    );
    assert_eq!(out, "string(2) \"t1\"\nfloat(2.5)\nNULL\nint(7)\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\nfloat(1.5)\nobject(stdClass)#1 (0) {\n}\nbool(true)\nNULL\n");
}

/// A named function whose `if` is not followed directly by `return` (so DCE cannot sink the tail
/// into both arms) joins the local correctly too.
#[test]
fn test_local_retype_branch_join_named_function_without_tail_return() {
    let out = compile_and_run(
        r#"<?php
function nf(int $n) { $m = null; if ($n > 1) { $m = "s" . $n; } echo gettype($m), ":", $m ?? "null", "\n"; return 0; }
nf(2);
nf(1);
"#,
    );
    assert_eq!(out, "string:s2\nNULL:null\n");
}
