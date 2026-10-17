//! Purpose:
//! Regression tests for branch merges and array literals whose arms or elements are static
//! properties, calls, callable invocations, nullsafe or dynamic member accesses, `new $cls()`,
//! `clone`, assignments or pipes, typed from their declarations rather than their syntax, or
//! as `Mixed` when no declaration is readable before lowering (issue #1501).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every fixture reads `$argc` so the ternary, `?:` and `match` survive AST constant folding.
//! - The syntactic fallback answered `Int` for every one of these shapes, which sized the
//!   merge temp as an integer: an array arm became its element count, a string or object arm
//!   became `0`, and a literal element was stamped as an integer. Expected output is from php.

use super::*;

/// Verifies a ternary over two static array properties yields the chosen array, at top level
/// and inside a function, rather than the array's element count (the shape of issue #1501).
#[test]
fn test_ternary_over_static_array_properties_keeps_the_array() {
    let out = compile_and_run(
        r#"<?php
class C { public static array $strs = ["a", "b"]; public static array $more = ["c"]; }
$x = $argc > 0 ? C::$strs : C::$more;
var_dump($x);
function g(int $n) { $y = $n > 0 ? C::$strs : C::$more; var_dump($y); }
g($argc);
g(0);
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(1) \"b\"\n}\n\
         array(2) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(1) \"b\"\n}\n\
         array(1) {\n  [0]=>\n  string(1) \"c\"\n}\n"
    );
}

/// Verifies ternaries over static string, object, int/float and untyped properties keep each
/// arm's value: strings and objects were read back as `0`, and a float arm was truncated.
#[test]
fn test_ternary_over_static_scalar_object_and_untyped_properties() {
    let out = compile_and_run(
        r#"<?php
class O { public int $v = 5; }
class C {
    public static string $s1 = "hello";
    public static string $s2 = "world";
    public static ?O $o1 = null;
    public static ?O $o2 = null;
    public static int $i = 7;
    public static float $f = 1.5;
    public static $untyped = ["u"];
}
C::$o1 = new O();
$t = new O();
$t->v = 9;
C::$o2 = $t;
echo $argc > 0 ? C::$s1 : C::$s2, "\n";
echo strlen($argc > 5 ? C::$s1 : C::$s2), "\n";
echo ($argc > 0 ? C::$o1 : C::$o2)->v, "\n";
echo ($argc > 5 ? C::$o1 : C::$o2)->v, "\n";
var_dump($argc > 0 ? C::$i : C::$f);
var_dump($argc > 5 ? C::$i : C::$f);
var_dump($argc > 0 ? C::$untyped : C::$s1);
"#,
    );
    assert_eq!(
        out,
        "hello\n5\n5\n9\nint(7)\nfloat(1.5)\narray(1) {\n  [0]=>\n  string(1) \"u\"\n}\n"
    );
}

/// Verifies `?:`, `match`, `self::`/`static::` arms, a `foreach` source and a copied result
/// over static array properties, and that writing the copy leaves the property untouched.
#[test]
fn test_short_ternary_match_and_foreach_over_static_array_properties() {
    let out = compile_and_run(
        r#"<?php
class C {
    public static array $strs = ["a", "b"];
    public static array $more = ["c"];
    public static string $s = "hello";
    public static function pick(int $n): array { return $n > 0 ? self::$strs : static::$more; }
}
echo implode(",", C::$strs ?: C::$more), "\n";
echo implode(",", match ($argc) { 1 => C::$strs, default => C::$more }), "\n";
echo match ($argc) { 1 => C::$s, default => "other" }, "\n";
echo implode(",", C::pick($argc)), "|", implode(",", C::pick(0)), "\n";
foreach ($argc > 0 ? C::$strs : C::$more as $v) {
    echo $v, ";";
}
echo "\n";
$y = $argc > 0 ? C::$strs : C::$more;
$y[0] = "changed";
echo count($y), " ", C::$strs[0], " ", $y[0], "\n";
"#,
    );
    assert_eq!(out, "a,b\na,b\nhello\na,b|c\na;b;\n2 a changed\n");
}

/// Verifies indexed and associative array literals type a static-property element from its
/// declaration: an array element was stamped `int` and printed as its count, and the
/// associative form failed to compile with an unsupported `hash_set` value type.
#[test]
fn test_array_literals_with_static_property_elements() {
    let out = compile_and_run(
        r#"<?php
class C {
    public static array $strs = ["a", "b"];
    public static string $s = "hello";
    public static float $f = 1.5;
    public static $untyped = ["u"];
}
var_dump([C::$strs, C::$s, C::$f]);
var_dump(["k" => C::$strs, "s" => C::$s, "f" => C::$f, "u" => C::$untyped]);
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  array(2) {\n    [0]=>\n    string(1) \"a\"\n    [1]=>\n    string(1) \"b\"\n  }\n  [1]=>\n  string(5) \"hello\"\n  [2]=>\n  float(1.5)\n}\n\
         array(4) {\n  [\"k\"]=>\n  array(2) {\n    [0]=>\n    string(1) \"a\"\n    [1]=>\n    string(1) \"b\"\n  }\n  [\"s\"]=>\n  string(5) \"hello\"\n  [\"f\"]=>\n  float(1.5)\n  [\"u\"]=>\n  array(1) {\n    [0]=>\n    string(1) \"u\"\n  }\n}\n"
    );
}

/// Verifies ternary arms that are function, method, static-method and builtin calls keep the
/// declared or inferred array result instead of being cast to `int(1)`.
#[test]
fn test_ternary_over_call_arms_keeps_array_results() {
    let out = compile_and_run(
        r#"<?php
function f() { return ["f1", "f2"]; }
class D {
    public static function a(): array { return ["da"]; }
    public function m() { return ["mm"]; }
}
$d = new D();
echo implode(",", $argc > 0 ? f() : f()), "\n";
echo implode(",", $argc > 0 ? $d->m() : $d->m()), "\n";
echo implode(",", $argc > 0 ? D::a() : f()), "\n";
echo implode(",", $argc > 5 ? D::a() : $d->m()), "\n";
echo implode(",", $argc > 0 ? explode(",", "x,y") : []), "\n";
"#,
    );
    assert_eq!(out, "f1,f2\nmm\nda\nmm\nx,y\n");
}

/// Verifies static-property and call arms in a loop release every merged value: the heap is
/// clean at exit, so no arm leaks the reference its merge temp acquired.
#[test]
fn test_static_property_and_call_arms_in_a_loop_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class O { public $v = 5; }
class C {
    public static array $strs = ["a", "b"];
    public static array $more = ["c"];
    public static string $s1 = "hello";
    public static string $s2 = "world";
    public static ?O $o1 = null;
    public static ?O $o2 = null;
}
function f() { return ["f1", "f2"]; }
class D { public function m() { return ["mm"]; } }
C::$o1 = new O();
C::$o2 = new O();
$d = new D();
$total = 0;
for ($i = 0; $i < 20 + $argc; $i++) {
    $a = $i % 2 ? C::$strs : C::$more;
    $b = $i % 3 ? C::$s1 : C::$s2;
    $o = $i % 2 ? C::$o1 : C::$o2;
    $c = C::$strs ?: C::$more;
    $m = match ($i % 3) { 0 => C::$strs, 1 => C::$more, default => ["lit"] };
    $l = [C::$strs, C::$s1, "k" => C::$o1];
    $g = $i % 2 ? f() : $d->m();
    $total += count($a) + strlen($b) + $o->v + count($c) + count($m) + count($l) + count($g);
}
echo $total;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "405");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies ternary arms that are nullsafe method calls or nullsafe property reads keep the
/// array or string they produce; both arms were typed `Int` and printed a count or `0`.
#[test]
fn test_ternary_over_nullsafe_method_and_property_arms() {
    let out = compile_and_run(
        r#"<?php
class R {
    public array $p = ["p1", "p2"];
    public string $s = "str";
    public function items(): array { return ["a", "b"]; }
    public function more() { return ["c"]; }
}
$d = $argc > 0 ? new R() : null;
echo implode(",", $argc > 0 ? $d?->items() : $d?->more()), "\n";
echo implode(",", $argc > 5 ? $d?->items() : $d?->more()), "\n";
echo implode(",", $argc > 0 ? $d?->p : $d?->p), "\n";
echo $argc > 0 ? $d?->s : $d?->s, "\n";
"#,
    );
    assert_eq!(out, "a,b\nc\np1,p2\nstr\n");
}

/// Verifies closure, arrow-function, first-class-callable and string-callable invocations in
/// ternary, `?:` and `match` arms keep their array results instead of `int(<count>)`.
#[test]
fn test_ternary_short_ternary_and_match_over_callable_invocations() {
    let out = compile_and_run(
        r#"<?php
function mk(): array { return ["m", "k"]; }
$f = function (): array { return ["a", "b"]; };
$g = fn() => ["c"];
$e = explode(...);
$m = mk(...);
$n = "mk";
echo implode(",", $argc > 0 ? $f() : $g()), "\n";
echo implode(",", $argc > 5 ? $f() : $g()), "\n";
echo implode(",", $argc > 0 ? $e(",", "x,y") : $e(",", "z")), "\n";
echo implode(",", $argc > 0 ? $m() : $m()), "\n";
echo implode(",", $argc > 0 ? $n() : $n()), "\n";
echo implode(",", $argc > 0 ? (fn() => ["p"])() : (fn() => ["q"])()), "\n";
echo implode(",", $f() ?: $g()), "\n";
echo implode(",", match ($argc) { 1 => $f(), default => $g() }), "\n";
"#,
    );
    assert_eq!(out, "a,b\nc\nx,y\nm,k\nm,k\np\na,b\na,b\n");
}

/// Verifies method and property arms on an untyped receiver, a union receiver, and a
/// `__call`/`__callStatic` redispatch are merged as `Mixed` (their lowering's own answer)
/// rather than cast to the syntactic `Int`.
#[test]
fn test_ternary_over_method_arms_without_a_single_receiver_class() {
    let out = compile_and_run(
        r#"<?php
class C {
    public $p = ["p1", "p2"];
    public function xs() { return ["a", "b"]; }
}
class M {
    public function __call($n, $a): array { return ["call", $n]; }
    public static function __callStatic($n, $a): array { return ["static", $n]; }
}
class A1 { public function ys(): array { return ["a1"]; } }
class B1 { public function ys(): array { return ["b1", "b2"]; } }
function pick($o, int $n) {
    echo implode(",", $n > 0 ? $o->xs() : $o->xs()), "|";
    echo implode(",", $n > 0 ? $o->p : $o->p), "\n";
}
pick(new C(), $argc);
$m = new M();
echo implode(",", $argc > 0 ? $m->items() : $m->entries()), "\n";
echo implode(",", $argc > 0 ? M::items() : M::entries()), "\n";
$u = $argc > 0 ? new A1() : new B1();
echo implode(",", $argc > 0 ? $u->ys() : $u->ys()), "\n";
"#,
    );
    assert_eq!(out, "a,b|p1,p2\ncall,items\nstatic,items\na1\n");
}

/// Verifies dynamic property reads, nullsafe dynamic property and method calls, `new $cls()`
/// and `clone` arms keep their values: arrays printed counts and objects failed to compile.
#[test]
fn test_ternary_over_dynamic_members_dynamic_new_and_clone() {
    let out = compile_and_run(
        r#"<?php
class O {
    public int $v = 5;
    public array $p = ["a", "b"];
    public function xs(): array { return ["x", "y"]; }
}
$c = new O();
$z = $argc > 0 ? new O() : null;
$pn = "p";
$mn = "xs";
echo implode(",", $argc > 0 ? $c->$pn : $c->$pn), "\n";
echo implode(",", $argc > 0 ? $z?->$pn : $z?->$pn), "\n";
echo implode(",", $argc > 0 ? $z?->$mn() : $z?->$mn()), "\n";
$cls = "O";
$a = $argc > 0 ? new $cls() : new $cls();
echo $a->v, "\n";
$q = new O();
$q->v = 8;
$b = $argc > 0 ? clone $q : clone $c;
echo $b->v, "\n";
"#,
    );
    assert_eq!(out, "a,b\na,b\nx,y\n5\n8\n");
}

/// Verifies assignment, pipe, and subscript arms whose container type the merge cannot read
/// (a static property or call result) keep their arrays, including inside an array literal.
#[test]
fn test_ternary_over_assignment_pipe_and_unresolved_access_arms() {
    let out = compile_and_run(
        r#"<?php
class C { public static array $m = ["k" => ["a", "b"], "j" => ["c"]]; }
function f() { return [["x", "y"], ["z"]]; }
function wrap(string $s): array { return [$s, $s]; }
echo implode(",", $argc > 0 ? ($a = ["a1", "a2"]) : ($b = ["b1"])), "\n";
echo implode(",", $argc > 0 ? ("w" |> wrap(...)) : ("v" |> wrap(...))), "\n";
echo implode(",", $argc > 0 ? C::$m["k"] : C::$m["j"]), "\n";
echo implode(",", $argc > 0 ? f()[0] : f()[1]), "\n";
$g = fn() => ["g1", "g2"];
var_dump([$argc > 0 ? $g() : $g()]);
"#,
    );
    assert_eq!(out, "a1,a2\nw,w\na,b\nx,y\narray(1) {\n  [0]=>\n  array(2) {\n    [0]=>\n    string(2) \"g1\"\n    [1]=>\n    string(2) \"g2\"\n  }\n}\n");
}

/// Verifies an instance-method arm returning `static` is typed with the receiver class, as the
/// call's own lowering binds it, so members declared only on the subclass stay readable.
#[test]
fn test_ternary_over_late_static_method_arms_binds_the_receiver() {
    let out = compile_and_run(
        r#"<?php
class A { public function make(): static { return new static(); } }
class B extends A { public int $extra = 7; public function only(): int { return 1; } }
$b = new B();
$x = $argc > 0 ? $b->make() : $b->make();
echo $x->extra, " ", $x->only(), " ", get_class($x), "\n";
"#,
    );
    assert_eq!(out, "7 1 B\n");
}

/// Verifies the nullsafe, callable, `__call`, dynamic-member, `clone`, untyped-receiver and
/// assignment arms release every merged value in a loop, leaving a clean heap at exit.
#[test]
fn test_mixed_merged_arms_in_a_loop_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class R {
    public array $p = ["p1", "p2"];
    public function items(): array { return ["a", "b"]; }
}
class M { public function __call($n, $a): array { return [$n]; } }
function pick($o, int $n) { return $n > 0 ? $o->items() : $o->items(); }
$d = $argc > 0 ? new R() : null;
$m = new M();
$f = fn() => ["f1", "f2", "f3"];
$pn = "p";
$total = 0;
for ($i = 0; $i < 20 + $argc; $i++) {
    $a = $i % 2 ? $d?->items() : $d?->p;
    $b = $i % 3 ? $f() : $f();
    $c = $i % 2 ? $m->one() : $m->two();
    $e = $i % 2 ? $d->$pn : $d->$pn;
    $k = $i % 2 ? clone $d : clone $d;
    $g = pick($d, $i % 2);
    $h = $i % 2 ? ($t = ["t"]) : ($t = ["u", "v"]);
    $total += count($a) + count($b) + count($c) + count($e) + count($k->p) + count($g) + count($h);
}
echo $total;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "284");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a `?->` chain arm is merged as the boxed `Mixed` its lowering produces even when the
/// method's inferred return is a concrete `array<int>` and the receiver is never null: typing
/// the merge temp from the declaration stored the box into a raw-array slot, so `count()` and
/// element reads saw the box's bytes. Covers `?:`, ternary and `match` arms, a chain continuing
/// after `?->`, and an index on the chain, in a loop under `--heap-debug`.
#[test]
fn test_nullsafe_chain_arm_with_inferred_array_return_merges_as_mixed() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box {
    public function items() { return [1, 2, 3]; }
    public function more() { return [9, 8]; }
    public function none() { return []; }
    public function map() { return ["a" => 1, "b" => 2]; }
    public function self(): Box { return $this; }
}
$o = new Box();
$out = "";
for ($i = 0; $i < 20 + $argc; $i++) {
    $a = $o?->items() ?: $o?->more();
    $b = $o?->none() ?: $o?->more();
    $c = $i % 2 ? $o?->self()->items() : [0];
    $d = $argc > 0 ? $o?->map() : ["z" => 0];
    $e = match ($argc) { 1 => $o?->items()[2], default => 0 };
    $f = $o?->self()?->more() ?: [];
    $out = count($a) . $a[1] . count($b) . $b[0] . count($c) . count($d) . $d["b"] . $e . count($f) . $f[1];
}
echo $out, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "3229122328\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}
