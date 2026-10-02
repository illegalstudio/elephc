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

/// Boxing the arm that KEPT an array or hash on its merge edge is a representation change, not a
/// rebinding, so the internal pointer `next`/`end` moved before the `if` survives the merge.
#[test]
fn test_branch_join_keeps_array_cursor_on_kept_container_arm() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
next($a);
if ($argc > 5) { $a = null; }
var_dump(current($a));
$h = ["x" => 1, "y" => 2, "z" => 3];
end($h);
if ($argc > 5) { $h = null; }
var_dump(key($h));
$f = function (int $n) { $h = ["a" => 1, "b" => 2, "c" => 3]; end($h); if ($n > 5) { $h = null; } prev($h); return key($h); };
var_dump($f(1));
$g = function (int $n) { $a = [10, 20, 30]; next($a); if ($n > 5) { $a = null; } else { echo "kept "; } return current($a); };
var_dump($g(1));
function arr_cursor(int $n) { $a = [10, 20, 30]; next($a); if ($n > 5) { $a = null; } echo current($a), " "; next($a); echo current($a), "\n"; return 0; }
arr_cursor(1);
"#,
    );
    assert_eq!(out, "int(20)\nstring(1) \"z\"\nstring(1) \"b\"\nkept int(20)\n20 30\n");
}

/// A cursor carried around a loop whose body joins the array against `null` keeps advancing:
/// before the fix every merge rewound it, so `while (current($l) !== false)` never ended.
#[test]
fn test_branch_join_keeps_loop_carried_array_cursor() {
    let out = compile_and_run(
        r#"<?php
$l = [1, 2, 3, 4];
$guard = 0;
while (($v = current($l)) !== false && $guard++ < 10) {
    echo $v, " ";
    if ($argc > 5) { $l = null; }
    next($l);
}
echo "\n";
$h = ["a" => 1, "b" => 2, "c" => 3];
end($h);
for ($i = 0; $i < 2; $i++) {
    echo key($h), " ";
    if ($argc > 5) { $h = null; }
    prev($h);
}
echo "\n";
"#,
    );
    assert_eq!(out, "1 2 3 4 \nc b \n");
}

/// An object, array, hash or callable local with a `null` arm compiles and reads back each arm's
/// value, including in named functions where DCE copies the tail into both arms and the `null`
/// arm reads the pointer slot through a `null` view.
#[test]
fn test_branch_join_null_arm_over_pointer_slot() {
    let out = compile_and_run(
        r#"<?php
function jo(int $n) { $o = new stdClass(); if ($n > 0) { $o = null; } var_dump($o); return 0; }
function ja(int $n) { $a = [1, 2]; if ($n > 0) { $a = null; } var_dump($a); return 0; }
function jh(int $n) { $a = ["k" => 1]; if ($n > 0) { $a = null; } else { $a["j"] = 2; } var_dump($a); return 0; }
function jc(int $n) { $f = fn() => 1; if ($n > 0) { $f = null; } var_dump($f === null); return 0; }
jo(1); jo(0); ja(1); ja(0); jh(1); jh(0); jc(1); jc(0);
$o = new stdClass(); if ($argc == 1) { $o = null; } var_dump($o);
$c = function () { return 1; }; if ($argc == 1) { $c = null; } var_dump($c);
"#,
    );
    assert_eq!(out, "NULL\nobject(stdClass)#1 (0) {\n}\nNULL\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\nNULL\narray(2) {\n  [\"k\"]=>\n  int(1)\n  [\"j\"]=>\n  int(2)\n}\nbool(true)\nbool(false)\nNULL\nNULL\n");
}

/// An assignment inside one arm of a ternary, `match`, `&&`, `||`, `?:` or `??` holds only on the
/// path that ran it. The arms used to share one flow environment, so `$c ? ($o = null) : 0` left
/// `$o` typed `null` below the merge and read the live object on the other path back as `NULL`
/// (or, over an object slot, refused to compile).
#[test]
fn test_lazy_expression_arm_assignment_stays_in_its_arm() {
    let out = compile_and_run(
        r#"<?php
function t(int $n) { $o = new stdClass(); $x = $n > 0 ? ($o = null) : 0; var_dump($o); return 0; }
function m(int $n) { $o = new stdClass(); $r = match(true) { $n > 0 => ($o = null), default => 0 }; var_dump($o); return 0; }
function l(int $n) { $o = new stdClass(); $n > 0 && ($o = null); var_dump($o); return 0; }
function o(int $n) { $x = null; $ok = $n > 0 || ($x = "fallback"); var_dump($x); return 0; }
function st(int $n) { $o = new stdClass(); $d = $n ?: ($o = null); var_dump($o); return 0; }
function co(?int $n) { $o = new stdClass(); $d = $n ?? ($o = null); var_dump($o); return 0; }
t(0); t(1); m(0); m(1); l(0); l(1); o(1); o(0); st(1); st(0); co(5); co($argc > 5 ? 1 : null);
$c = function (int $n) { $v = 1.5; $x = $n > 0 ? ($v = null) : 0; var_dump($v); return 0; };
$c(0); $c(1);
$s = function (int $n) { $o = new stdClass(); if ($n > 5) { $o = null; } $n > 0 && ($o = null); var_dump($o); return 0; };
$s(0); $s(1);
"#,
    );
    let object = "object(stdClass)#1 (0) {\n}\n";
    assert_eq!(
        out,
        format!(
            "{object}NULL\n{object}NULL\n{object}NULL\nNULL\nstring(8) \"fallback\"\n{object}NULL\n{object}NULL\nfloat(1.5)\nNULL\nobject(stdClass)#3 (0) {{\n}}\nNULL\n"
        )
    );
}

/// A local that enters a loop typed `null` while the checker still knows its earlier type
/// (`$o = new C; $o = null;`) is boxed at the loop head, so a read before the body's own
/// assignment sees the value the back edge carries in instead of `null` on every iteration.
#[test]
fn test_loop_entry_null_fact_reads_the_back_edge_value() {
    let out = compile_and_run(
        r#"<?php
function lo(int $n) { $o = new stdClass(); $o = null; for ($i = 0; $i < $n; $i++) { var_dump($o === null); if ($i >= 0) { $o = new stdClass(); } } return 0; }
function dw(int $n) { $o = new stdClass(); if ($n > 0) { $o = null; $i = 0; do { var_dump($o === null); if ($i > 0) { $o = new stdClass(); } $i++; } while ($i < 3); return 1; } return 0; }
function fe(array $xs) { $o = new stdClass(); $o = null; foreach ($xs as $x) { var_dump($o === null); if ($x > 0) { $o = new stdClass(); } } return 0; }
function lv(int $n) { $v = 5; $v = null; for ($i = 0; $i < $n; $i++) { var_dump($v); $v = $i; } return 0; }
lo(3); dw(1); fe([1, 2, 3]); lv(3);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nbool(false)\nbool(true)\nbool(true)\nbool(false)\nbool(true)\nbool(false)\nbool(false)\nNULL\nint(0)\nint(1)\n"
    );
}

/// A whole-value `Mixed` loop contract over an array boxes the SAME array at the loop head, so
/// the internal pointer `next`/`end` moved before the loop survives it, like the `if` join.
#[test]
fn test_loop_mixed_contract_keeps_array_cursor() {
    let out = compile_and_run(
        r#"<?php
function pick_mixed(int $i): mixed { return $i > 2 ? ["x" => 1] : null; }
function pick_union(int $i) { return $i > 5 ? [1, 2] : "s"; }
function walk(int $n, mixed $m) { $a = [10, 20, 30]; next($a); $i = 0; while ($i < 2) { echo current($a), " "; if ($n > 5) { $a = $m; } next($a); $i++; } echo "\n"; return 0; }
walk(1, "x");
$h = ["a" => 1, "b" => 2, "c" => 3];
end($h);
for ($i = 0; $i < 2; $i++) { echo key($h), " "; if ($argc > 5) { $h = pick_mixed($i); } prev($h); }
echo "\n";
$a = [10, 20, 30];
next($a);
for ($i = 0; $i < 2; $i++) { if ($argc > 5) { $a = pick_union($i); } }
var_dump(current($a));
"#,
    );
    assert_eq!(out, "20 30 \nc b \nint(20)\n");
}

/// The `null` view of a pointer slot is materialized wherever the `null` store provably reaches
/// the read: across an inner `if`, a loop, a call, or a `try`, not only in the same block.
#[test]
fn test_branch_join_null_arm_read_proven_across_blocks() {
    let out = compile_and_run(
        r#"<?php
function nested(int $n) { $o = new stdClass(); if ($n > 0) { $o = null; if ($n > 1) { echo "deep "; } var_dump($o); return 1; } var_dump($o); return 0; }
function loopn(int $n) { $o = new stdClass(); if ($n > 0) { $o = null; for ($i = 0; $i < $n; $i++) { echo $i; } echo "\n"; var_dump($o); return 1; } var_dump($o); return 0; }
function callb(int $n) { $o = new stdClass(); if ($n > 0) { $o = null; strlen("x" . $n); var_dump($o); return 1; } var_dump($o); return 0; }
function tr(int $n) { $o = new stdClass(); try { if ($n > 0) { $o = null; var_dump($o); return 1; } } catch (Exception $e) {} var_dump($o); return 0; }
nested(2); nested(0); loopn(2); callb(1); tr(1); tr(0);
"#,
    );
    let object = "object(stdClass)#1 (0) {\n}\n";
    assert_eq!(out, format!("deep NULL\n{object}01\nNULL\nNULL\nNULL\n{object}"));
}

/// `buffer<T>` and packed-class slots also keep their pointer storage across a `null` store,
/// so the `null` arm of a named function reads them through the same proven null view.
/// (elephc extensions: the expected output follows PHP's `null` semantics for the local.)
#[test]
fn test_branch_join_null_arm_over_buffer_and_packed_slots() {
    let out = compile_and_run(
        r#"<?php
packed class P { public int $x; }
function fb(int $n) { $b = buffer_new<int>(2); if ($n > 0) { $b = null; } echo $b === null ? "null" : "buf", "\n"; return 0; }
function fp(int $n) { $b = buffer_new<P>(1); $p = $b[0]; if ($n > 0) { $p = null; } echo $p === null ? "null" : "p", "\n"; return 0; }
fb(1); fb(0); fp(1); fp(0);
"#,
    );
    assert_eq!(out, "null\nbuf\nnull\np\n");
}

/// A `match` arm with several conditions is reached once per condition, and each edge carries
/// the facts its own condition left: `($v = null), ($v = 1) => …` enters the result with `$v`
/// null on the first edge and int on the second. The result block used to take the LAST
/// condition's fact on every edge, so the first condition's `null` read back as an int.
#[test]
fn test_match_condition_list_joins_each_condition_edge() {
    let out = compile_and_run(
        r#"<?php
function m($n) {
    $r = match ($n) { ($v = null), ($v = 1) => 10, default => 0 };
    var_dump($r, $v);
}
m(null);
m(1);
m(2);
function k($n) {
    $r = match ($n) { ($v = 1.5), ($v = 2) => "hit", default => "miss" };
    var_dump($r, $v);
}
k(1.5);
k(2);
k(3);
"#,
    );
    assert_eq!(
        out,
        "int(10)\nNULL\nint(10)\nint(1)\nint(0)\nint(1)\nstring(3) \"hit\"\nfloat(1.5)\nstring(3) \"hit\"\nint(2)\nstring(4) \"miss\"\nint(2)\n"
    );
}

/// A `switch` exit is a join: each `break` reaches it with the facts its own case left, and
/// each body is entered from the dispatch as well as by falling through. Lowering the bodies in
/// sequence left `$o` typed `null` after `case 1: $o = null; break;` on every path, so a read
/// after the switch refused to compile, and a loop that read `$o` before assigning it boxed the
/// stale `null` at its head and printed `N` for the object the other cases kept.
#[test]
fn test_switch_exit_and_bodies_join_every_edge() {
    let out = compile_and_run(
        r#"<?php
class P { public int $v = 7; }
function after(int $n): string {
    $o = new P();
    switch ($n) {
        case 1: $o = null; break;
        case 2: $n = 5; break;
        default: break;
    }
    return $o === null ? "N" : "O" . $o->v;
}
echo after(1), " ", after(2), " ", after(3), "\n";
function loop(int $n): string {
    $o = new P();
    switch ($n) {
        case 1: $o = null; break;
        case 2: $n = 5; break;
        default: break;
    }
    $out = "";
    for ($i = 0; $i < 2; $i++) {
        $out .= $o === null ? "N" : "O";
        $o = new P();
    }
    return $out;
}
echo loop(1), " ", loop(2), " ", loop(3), "\n";
function entry(int $n): string {
    $o = new P();
    switch ($n) {
        case 1: $o = null; break;
        case 2: return "O" . $o->v;
        default: break;
    }
    return $o === null ? "N" : "D";
}
echo entry(1), " ", entry(2), " ", entry(3), "\n";
function fall(int $n): string {
    $o = new P();
    switch ($n) {
        case 1: $o = null;
        case 2: $s = $o === null ? "N" : "O"; break;
        default: $s = "D";
    }
    return $s;
}
echo fall(1), " ", fall(2), " ", fall(3), "\n";
"#,
    );
    assert_eq!(out, "N O7 O7\nNO OO OO\nN O7 D\nN O D\n");
}

/// A `switch` whose cases leave a local in different representations reads it back after the
/// exit as the value of whichever case ran. The exit used to take the representation of the
/// case body lowered last, so the paths that skipped `$m = "s"` printed `string(0) ""`.
#[test]
fn test_switch_exit_joins_divergent_scalar_representations() {
    let out = compile_and_run(
        r#"<?php
function sw(int $n) {
    $m = null;
    switch ($n) {
        case 1: $m = "s" . $n; break;
        case 2: $n++; break;
    }
    var_dump($m);
    return 0;
}
sw(1); sw(2); sw(3);
$c = null;
switch ($argc) {
    case 5: $c = 1.5; break;
    case 6: break;
    default: $c = $argc;
}
var_dump($c);
"#,
    );
    assert_eq!(out, "string(2) \"s1\"\nNULL\nNULL\nint(1)\n");
}

/// A loop exit keeps its body's facts, so after `while (…) { $o = null; }` lowering types `$o`
/// `null` on the path that never entered the loop as well. The next loop's head must not read
/// the slot through that fact: it gives the local boxed storage instead of re-storing a box of
/// the `null` view, which printed `N` for the object the skipped loop left in place.
#[test]
fn test_loop_entry_box_ignores_stale_null_from_previous_loop_exit() {
    let out = compile_and_run(
        r#"<?php
class P { public int $v = 7; }
function wl(int $k): string {
    $o = new P();
    while ($k-- > 0) {
        $o = null;
    }
    $out = "";
    for ($i = 0; $i < 2; $i++) {
        $out .= $o === null ? "N" : "O";
        $o = new P();
    }
    return $out;
}
echo wl(0), " ", wl(1), "\n";
function fe(array $xs): string {
    $o = new P();
    foreach ($xs as $x) {
        if ($x > 1) { break; }
        $o = null;
    }
    $out = "";
    foreach ([1, 2] as $y) {
        $out .= $o === null ? "N" : "O";
        $o = new P();
    }
    return $out;
}
echo fe([]), " ", fe([1]), " ", fe([2]), "\n";
"#,
    );
    assert_eq!(out, "OO NO\nOO NO OO\n");
}

/// A `switch` case label may assign a local, so each body starts from the facts of the labels
/// that select it, not from those the LAST label left. `case ($x = null) === null:` followed by
/// `case ($x = 5) > 0:` read `$x` as an integer in the first body (printing the null sentinel).
/// A body reached by two labels that disagree joins them, and a body entered both by its own
/// label (`$o = null`) and by fall-through (`$o` still the object) joins those. The locals start
/// from non-literal values so AST constant propagation cannot fold them. Runs under
/// `--heap-debug` over a loop. Review follow-up for #771.
#[test]
fn test_switch_bodies_start_from_their_own_label_edges() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function s(int $n): string {
    $x = 1;
    $o = new stdClass();
    $out = str_repeat("-", $n > 100 ? 1 : 0);
    switch (true) {
        case ($x = null) === null && $n == 1:
        case ($x = 5) > 0 && $n == 2:
            $out .= var_export($x, true) . ",";
        case ($o = null) === null && $n == 3:
            $out .= ($o === null ? "o-null" : "o-obj") . ",";
            break;
        default:
            $out .= var_export($x, true) . "/" . ($o === null ? "o-null" : "o-obj");
    }
    return $out . "|" . var_export($x, true);
}
function u(int $n): string {
    $s = str_repeat("a", $n > 100 ? 2 : 1);
    switch ($n) {
        case ($s = null) ?? 1: return var_export($s, true);
        case ($s = "zz") ? 2 : 2: return $s;
        default: return "d" . $s;
    }
}
$r = "";
for ($i = 0; $i < 30 + $argc; $i++) {
    $r = s(1) . " " . s(2) . " " . s(3) . " " . s(4) . " " . u(1) . " " . u(2) . " " . u(3);
}
echo $r, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "NULL,o-obj,|NULL 5,o-obj,|5 o-null,|5 5/o-null|5 NULL zz dzz\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
