//! Purpose:
//! Regression tests for locals a join point may leave undefined: reads answer PHP null with the
//! "Undefined variable" warning instead of a stale or garbage slot value.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The warning goes to stderr like every elephc runtime warning; the tests that check it use
//!   `compile_and_run_capture`.

use crate::support::*;

/// Verifies a plain read of a local only one `if` arm or a loop body assigned yields null and
/// warns "Undefined variable $x in <file> on line N", as PHP does, instead of the previous call's
/// value or an empty string.
#[test]
fn test_maybe_undefined_plain_read_warns_and_yields_null() {
    let out = compile_and_run_capture(
        r#"<?php
function f($c) {
    if ($c) { $x = 5; }
    var_dump($x);
}
f(true);
f(false);
function h($n) {
    for ($i = 0; $i < $n; $i++) { $last = $i; }
    var_dump($last);
}
h(3);
h(0);
function k($c) {
    if ($c) { $s = "str"; }
    return $s;
}
var_dump(k(true));
var_dump(k(false));
"#,
    );
    assert_eq!(out.stdout, "int(5)\nNULL\nint(2)\nNULL\nstring(3) \"str\"\nNULL\n");
    for (name, line) in [("x", 4), ("last", 10), ("s", 16)] {
        let needle = format!("Warning: Undefined variable ${} in ", name);
        let at = out.stderr.find(&needle).unwrap_or_else(|| panic!("missing {needle:?} in {:?}", out.stderr));
        let rest = &out.stderr[at..];
        let line_end = rest.find('\n').unwrap_or(rest.len());
        assert!(
            rest[..line_end].ends_with(&format!(" on line {}", line)),
            "unexpected warning line: {:?}",
            &rest[..line_end]
        );
    }
    assert_eq!(out.stderr.matches("Undefined variable").count(), 3, "stderr: {:?}", out.stderr);
}

/// Verifies top-level locals only a branch or a loop that did not run assigned read as null
/// (with the warning) instead of whatever the uninitialized slot held.
#[test]
fn test_top_level_maybe_undefined_reads_answer_php_null() {
    let out = compile_and_run_capture(
        r#"<?php
if ($argc > 5) { $x = 1; }
var_dump($x);
var_dump(isset($x));
for ($i = 0; $i < 0; $i++) { $y = $i; }
echo $y ?? "none", "\n";
"#,
    );
    assert_eq!(out.stdout, "NULL\nbool(false)\nnone\n");
    assert!(
        out.stderr.contains("Warning: Undefined variable $x in "),
        "stderr: {:?}",
        out.stderr
    );
    assert_eq!(out.stderr.matches("Undefined variable").count(), 1, "stderr: {:?}", out.stderr);
}

/// Verifies a type guard that excludes null (`instanceof`, `is_array()`, `isset()`) defines a
/// possibly undefined local in the region it guards, so a typed parameter accepts the narrowed
/// read, and an undefined local fails the guard instead of passing a stale value.
#[test]
fn test_type_guard_defines_possibly_undefined_local() {
    let out = compile_and_run_capture(
        r#"<?php
class O {}
function g(O $x) { return "o"; }
function n(array $x) { return count($x); }
function t($c) {
    if ($c) { $s = new O; $a = [1, 2]; }
    if ($s instanceof O) { echo g($s); } else { echo "-"; }
    echo is_array($a) ? n($a) : "-";
    echo isset($s) ? "set" : "unset", "\n";
}
t(true);
t(false);
"#,
    );
    assert_eq!(out.stdout, "o2set\n--unset\n");
    assert_eq!(out.stderr.matches("Undefined variable").count(), 2, "stderr: {:?}", out.stderr);
}

/// Verifies `throw $e` on a Throwable local only one `if` arm assigned still compiles, throws the
/// object when it is defined, and otherwise warns and raises `Error("Can only throw objects")`.
#[test]
fn test_throw_maybe_undefined_throwable_local() {
    let out = compile_and_run_capture(
        r#"<?php
function c($c) {
    if ($c) { $e = new LogicException("maybe"); }
    try { throw $e; } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
}
c(true);
c(false);
function b() {
    try {
        try { throw new RuntimeException("inner"); }
        catch (RuntimeException $ex) { $saved = $ex; }
        finally { if (isset($saved)) { throw $saved; } }
    } catch (Exception $x) { echo "caught ", $x->getMessage(), "\n"; }
}
b();
"#,
    );
    assert_eq!(out.stdout, "LogicException: maybe\nError: Can only throw objects\ncaught inner\n");
    assert!(
        out.stderr.contains("Warning: Undefined variable $e in "),
        "stderr: {:?}",
        out.stderr
    );
}

/// Verifies a local some path leaves undefined reads as PHP null there (`isset`, `??`, `@`,
/// `empty`, after an `if`, a lazy edge, a loop that did not run and `unset`) instead of the value
/// a previous call left in the slot.
#[test]
fn test_maybe_undefined_local_reads_answer_php_null() {
    let out = compile_and_run(
        r#"<?php
        // A local some path leaves undefined reads as PHP null there.
        function f($v) { return $v; }
        function e01($a) { if ($a && ($r = f(5))) { echo "in\n"; } return $r ?? null; }
        var_dump(e01(true), e01(false));
        function e02($a) { if ($a && ($r = f(5))) { } var_dump(isset($r)); }
        e02(true); e02(false);
        function e03($a) { if ($a && ($r = f(5))) { } $r = $r ?? 0; var_dump($r); }
        e03(true); e03(false);
        function e08($a) { if ($a && ($r = f(5))) { } echo @$r, "|\n"; }
        e08(true); e08(false);
        function e14($a) { $a && ($r = f(5)); return $r ?? "d"; }
        var_dump(e14(true), e14(false));
        function g03($a) { if ($a) { $r = f(5); } return $r ?? null; }
        var_dump(g03(true), g03(false));
        function g05($a) { if ($a) { $r = f(5); } var_dump(isset($r), empty($r)); }
        g05(true); g05(false);
        function lp($n) { for ($i = 0; $i < $n; $i++) { $last = $i; } return isset($last) ? $last : "none"; }
        var_dump(lp(3), lp(0));
        function un($a) { $r = 1; if ($a) { unset($r); } return $r ?? "unset"; }
        var_dump(un(true), un(false));
        "#,
    );
    assert_eq!(out, "in\nint(5)\nNULL\nbool(true)\nbool(false)\nint(5)\nint(0)\n5|\n|\nint(5)\nstring(1) \"d\"\nint(5)\nNULL\nbool(true)\nbool(false)\nbool(false)\nbool(true)\nint(2)\nstring(4) \"none\"\nstring(5) \"unset\"\nint(1)\n");
}

/// Verifies closures capture a possibly undefined local with PHP's semantics: `use (&$r)`
/// creates it as null, `use ($r)` captures null (after a warning), and an arrow function keeps it
/// undefined in its body, defined-flag included.
#[test]
fn test_maybe_undefined_closure_captures() {
    let out = compile_and_run(
        r#"<?php
        // Closures capture a possibly undefined local with PHP's semantics.
        function h04($a) { if ($a) { $r = 5; } $c = function () use (&$r) { $r = 6; }; $c(); echo $r, "\n"; }
        h04(true); h04(false);
        function nw($a) { if ($a) { $r = 5; } $c = function () use (&$r) { }; $c(); var_dump($r, isset($r)); }
        nw(true); nw(false);
        function rd($a) { if ($a) { $r = "s"; } $c = function () use (&$r) { return $r ?? "u"; }; echo $c(), "\n"; var_dump($r); }
        rd(true); rd(false);
        function bv($a) { if ($a) { $r = "x"; } $g = @function () use ($r) { return [isset($r), $r ?? "u"]; }; var_dump($g()); }
        bv(true); bv(false);
        function h05($a) { if ($a) { $r = 5; } $c = fn() => $r ?? "u"; echo $c(), "\n"; }
        h05(true); h05(false);
        function ar($a) { if ($a) { $r = "s"; } $c = fn() => isset($r) ? "Y" : "N"; echo $c(), "\n"; }
        ar(true); ar(false);
        function nest($a) { if ($a) { $r = 7; } $c = fn() => fn() => $r ?? "nu"; echo ($c())(), "\n"; }
        nest(true); nest(false);
        "#,
    );
    assert_eq!(
        out,
        "6\n6\nint(5)\nbool(true)\nNULL\nbool(false)\ns\nstring(1) \"s\"\nu\nNULL\narray(2) {\n  [0]=>\n  bool(true)\n  [1]=>\n  string(1) \"x\"\n}\narray(2) {\n  [0]=>\n  bool(false)\n  [1]=>\n  string(1) \"u\"\n}\n5\nu\nY\nN\n7\nnu\n"
    );
}

/// Verifies the defined-flag follows `unset` in loops, `try`/`catch`/`finally`, `switch`
/// fallthrough and `match` arms.
#[test]
fn test_maybe_undefined_locals_across_unset_try_switch_match() {
    let out = compile_and_run(
        r#"<?php
        // The defined-flag follows unset in loops, try/catch/finally, switch and match.
        function v2() { foreach ([1, 2, 3] as $x) { if ($x == 2) { unset($z); } echo $z ?? "u", " "; $z = $x; } echo "\n"; }
        v2();
        function v3() { foreach ([1, 2, 3] as $x) { echo isset($z) ? "Y" : "N"; $z = "s$x"; if ($x == 1) { unset($z); } } echo "\n"; }
        v3();
        function v4() { $z = 1; while (true) { if ($z == 1) { unset($z); break; } } echo $z ?? "u", "\n"; }
        v4();
        function v5() { $z = "a"; for ($i = 0; $i < 3; $i++) { echo $z ?? "u", " "; unset($z); } echo "\n"; }
        v5();
        function t($a) { try { if ($a) throw new Exception("x"); $t = "tried"; } catch (Exception $e) { } echo $t ?? "u", "\n"; }
        t(true); t(false);
        function c($a) { try { if ($a) throw new Exception("x"); } catch (Exception $e) { } echo isset($e) ? "Y" : "N", "\n"; }
        c(true); c(false);
        function f($a) { try { if ($a) throw new Exception("x"); $t = 1; } catch (Exception $e) { $c = 2; } finally { $fin = 3; } echo $t ?? "u", $c ?? "u", $fin ?? "u", "\n"; }
        f(true); f(false);
        function g($a) { try { $t = "pre"; if ($a) throw new Exception("x"); $t2 = "post"; } catch (Exception $e) { echo $t ?? "u", $t2 ?? "u", "|"; } echo $t2 ?? "u", "\n"; }
        g(true); g(false);
        function r($a) { try { if ($a) { return "ret\n"; } $q = 1; } finally { echo $q ?? "u", "|"; } return "end\n"; }
        echo r(true); echo r(false);
        function sw($a) { switch ($a) { case 1: $s = "one"; break; default: break; } echo $s ?? "u", "\n"; }
        sw(1); sw(2);
        function ft($a) { switch ($a) { case 1: $s = "one"; case 2: $s2 = "two"; break; default: $d = "d"; } echo $s ?? "u", $s2 ?? "u", $d ?? "u", "\n"; }
        ft(1); ft(2); ft(3);
        function m($a) { $m = match ($a) { 1 => ($z = "one"), default => "d" }; echo $m, $z ?? "u", "\n"; }
        m(1); m(2);
        "#,
    );
    assert_eq!(
        out,
        "u u 2 \nNNY\nu\na u u \nu\ntried\nY\nN\nu23\n1u3\npreu|u\npost\nu|ret\n1|end\none\nu\nonetwou\nutwou\nuud\noneone\ndu\n"
    );
}

/// Verifies a local assigned in a lazy operand of a condition is defined where that edge leads:
/// the `if`/`while`/`for` body on the true edge of `&&`, the code after a diverging `if` on the
/// false edge of `||`, and the matching ternary arm.
#[test]
fn test_condition_edge_definitions_reach_the_guarded_region() {
    let out = compile_and_run(
        r#"<?php
        // A local assigned in a lazy operand of a condition is defined on the edge that implies it.
        class O { public $name = "N"; function m() { return $this; } }
        function f($v) { return $v; }
        function a($c) { if ($c && ($x = f(new O))) { return $x->name ?? false; } return "no"; }
        var_dump(a(true), a(false));
        function b($c) { if (!$c || !($y = f(5))) { return "early"; } return $y + 1; }
        var_dump(b(true), b(false));
        function c() { $i = 0; while ($i < 3 && ($r = f($i * 2)) !== null) { echo $r, ","; $i++; } echo "\n"; }
        c();
        function d($c) { if ($c && ($p = f("p")) && ($q = f("q"))) { return $p . $q; } elseif (!$c || ($z = f("z"))) { return "elif"; } return "else"; }
        var_dump(d(true), d(false));
        function e($c) { return ($c && ($t = f(7))) ? $t * 2 : -1; }
        var_dump(e(true), e(false));
        function g($c) { for ($i = 0; $i < 2 && ($v = f($i + 10)); $i++) { echo $v, ","; } echo "\n"; }
        g(1);
        "#,
    );
    assert_eq!(out, "string(1) \"N\"\nstring(2) \"no\"\nint(6)\nstring(5) \"early\"\n0,2,4,\nstring(2) \"pq\"\nstring(4) \"elif\"\nint(14)\nint(-1)\n10,11,\n");
}

/// Verifies nested `&&`/`||`, `!`, `elseif` false edges and ternary conditions define the
/// assigned local on the right edge (`is_callable($cb) && ! ($r = new R(...))->isAnonymous()`).
#[test]
fn test_condition_edge_definitions_nested_and_negated() {
    let out = compile_and_run(
        r#"<?php
        // Nested &&/||, `!`, elseif false edges and diverging `if`.
        class S { public $name = "Scope"; }
        class R { function __construct(public $anon) {} function isAnonymous() { return $this->anon; } function getClosureScopeClass() { return new S; } }
        class C {
            function get($cb) {
                if (is_callable($cb) && ! ($reflector = new R($cb === 'x'))->isAnonymous()) {
                    return $reflector->getClosureScopeClass()->name ?? false;
                }
                return "other";
            }
        }
        $c = new C; var_dump($c->get('strlen'), $c->get('x'), $c->get(5));
        function f($v) { return $v; }
        function n1($a, $b) { if ($a && ($b || ($z = f(3)))) { return isset($z) ? "z" : "noz"; } return "f"; }
        var_dump(n1(true, false), n1(true, true), n1(false, false));
        function n2($a) { if (!(!$a || !($w = f("w")))) { return $w; } return "-"; }
        var_dump(n2(1), n2(0));
        function n3($a) { if (!$a || ($y = f(9)) === null) { return "early"; } return $y * 2; }
        var_dump(n3(1), n3(0));
        function n4($a) { if ($a) { return "a"; } elseif (($q = f("q")) && ($r = f("r"))) { return $q . $r; } else { return "e"; } }
        var_dump(n4(0), n4(1));
        $top = true;
        if (!$top || !($g = f("g"))) { echo "no\n"; } else { echo $g, "\n"; }
        echo ($top && ($h = f(4))) ? $h + 1 : 0, "\n";
        "#,
    );
    assert_eq!(out, "string(5) \"Scope\"\nstring(5) \"other\"\nstring(5) \"other\"\nstring(1) \"z\"\nstring(3) \"noz\"\nstring(1) \"f\"\nstring(1) \"w\"\nstring(1) \"-\"\nint(18)\nstring(5) \"early\"\nstring(2) \"qr\"\nstring(1) \"a\"\ng\n5\n");
}

/// Control (already passes on main): a local defined by a condition edge can still be retyped
/// inside the guarded region like any other local.
#[test]
fn test_condition_edge_local_can_be_retyped_in_the_region() {
    let out = compile_and_run(
        r#"<?php
        // A local defined by a condition edge can be retyped in the guarded region.
        function f($v) { return $v; }
        function k2($a) { if ($a && ($r = f(5))) { $r .= "x"; echo $r, "\n"; } }
        k2(true);
        function k3($a) { if ($a && ($r = f(5))) { $r = "s"; echo $r, "\n"; } }
        k3(true);
        function k5($a) { $i = 0; while ($i++ < 2 && ($r = f(5))) { $r .= "x"; echo $r, "\n"; } }
        k5(true);
        function k6($a) { echo ($a && ($r = f(5))) ? ($r = "s") : "no", "\n"; }
        k6(true); k6(false);
        function v12($a) { if ($a && ($r = f(5))) { $r++; $r .= "x"; echo $r, "\n"; } }
        v12(true);
        "#,
    );
    assert_eq!(out, "5x\ns\n5x\n5x\ns\nno\n6x\n");
}

/// Control (already passes on main): `empty()`/`isset()` on a possibly undefined `ArrayAccess`
/// local keep the `offsetExists`-first route and answer like an undefined container when the
/// local was never assigned.
#[test]
fn test_maybe_undefined_array_access_receiver_probes() {
    let out = compile_and_run(
        r#"<?php
        class Box implements ArrayAccess {
            public function offsetExists(mixed $offset): bool { echo "E"; return $offset === "p"; }
            public function offsetGet(mixed $offset): mixed { echo "G"; return 0; }
            public function offsetSet(mixed $offset, mixed $value): void {}
            public function offsetUnset(mixed $offset): void {}
        }
        function t($a) {
            if ($a) { $b = new Box(); }
            echo empty($b["q"]) ? "|empty\n" : "|set\n";
            echo empty($b["p"]) ? "|empty\n" : "|set\n";
            echo isset($b["p"]) ? "|set\n" : "|unset\n";
        }
        t(true); t(false);
        "#,
    );
    assert_eq!(out, "E|empty\nEG|empty\nE|set\n|empty\n|empty\n|unset\n");
}

/// Verifies a loop body reads a local its `while` / `for` condition assigns on the true edge as
/// the assigned value: `while ($c && ($a = [1]))` binds `$a` before every iteration, so passing it
/// to an `array` parameter compiles and runs.
#[test]
fn test_loop_condition_true_edge_defines_the_body_local() {
    let out = compile_and_run_capture(
        r#"<?php
function take(array $a) { echo count($a), "\n"; }
function w($c) { while ($c && ($a = [1, 2])) { take($a); break; } }
function f($c) { for (; $c && ($b = [1, 2, 3]); ) { take($b); break; } }
w(true);
f(true);
w(false);
f(false);
echo "done\n";
"#,
    );
    assert_eq!(out.stdout, "2\n3\ndone\n");
    assert!(!out.stderr.contains("Undefined variable"), "stderr: {:?}", out.stderr);
}

/// Verifies an `elseif` condition that reads a local only an earlier clause's body assigned sees
/// it as undefined: PHP warns and compares null, so `elseif ($x > 3)` takes the `else`.
#[test]
fn test_elseif_condition_reads_earlier_body_local_as_undefined() {
    let out = compile_and_run_capture(
        r#"<?php
function t($c) {
    if ($c) { $x = 5; } elseif ($x > 3) { echo "wrong\n"; } else { echo "else\n"; }
}
t(true);
t(false);
t(false);
"#,
    );
    assert_eq!(out.stdout, "else\nelse\n");
    assert_eq!(out.stderr.matches("Undefined variable $x").count(), 2, "stderr: {:?}", out.stderr);
}

/// Verifies a `catch` body reads a local the `try` body unset before throwing as undefined, even
/// though a later statement of the `try` body assigns it again.
#[test]
fn test_catch_reads_local_unset_before_the_throw() {
    let out = compile_and_run_capture(
        r#"<?php
function t($c) {
    $x = 1;
    try {
        unset($x);
        if ($c) throw new Exception("e");
        $x = 2;
    } catch (Exception $e) {
        var_dump($x);
    }
    var_dump($x);
}
t(true);
t(false);
"#,
    );
    assert_eq!(out.stdout, "NULL\nNULL\nint(2)\n");
    assert_eq!(out.stderr.matches("Undefined variable $x").count(), 2, "stderr: {:?}", out.stderr);
}
