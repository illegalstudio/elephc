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

/// Verifies indexed and associative literals of a nullable scalar static property box the
/// element instead of stamping the literal with the inline `TaggedScalar` representation:
/// the associative form panicked in `hash_value_type_tag`, and the indexed form read every
/// non-null payload as `NULL`. Covers `null`, `0` and a non-zero value in both literal shapes.
#[test]
fn test_array_literals_with_nullable_static_property_elements() {
    let out = compile_and_run(
        r#"<?php
class C {
    public static ?int $n5 = 5;
    public static ?int $n0 = 0;
    public static ?int $nn = null;
    public static ?bool $bf = false;
    public static ?string $bs = "s";
}
var_dump([C::$n5]);
var_dump([C::$n0]);
var_dump([C::$nn]);
var_dump(["k" => C::$n5]);
var_dump(["k" => C::$n0]);
var_dump(["k" => C::$nn]);
var_dump([C::$bf, C::$bs]);
var_dump(["b" => C::$bf, "s" => C::$bs]);
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [0]=>\n  int(5)\n}\n\
         array(1) {\n  [0]=>\n  int(0)\n}\n\
         array(1) {\n  [0]=>\n  NULL\n}\n\
         array(1) {\n  [\"k\"]=>\n  int(5)\n}\n\
         array(1) {\n  [\"k\"]=>\n  int(0)\n}\n\
         array(1) {\n  [\"k\"]=>\n  NULL\n}\n\
         array(2) {\n  [0]=>\n  bool(false)\n  [1]=>\n  string(1) \"s\"\n}\n\
         array(2) {\n  [\"b\"]=>\n  bool(false)\n  [\"s\"]=>\n  string(1) \"s\"\n}\n"
    );
}

/// Verifies the boxing applies to every nullable element source, not just a static property:
/// a nullable local, an instance property, and a `?int`-returning method each used to stamp
/// the inline `TaggedScalar` representation, so indexed reads printed `NULL` and associative
/// literals panicked in `hash_value_type_tag`.
#[test]
fn test_array_literals_with_nullable_scalar_locals_properties_and_calls() {
    let out = compile_and_run(
        r#"<?php
function f(?int $x): void { var_dump([$x]); var_dump(["k" => $x]); }
f(5);
f(null);
class O {
    public ?int $np = 5;
    public ?int $nn = null;
    public ?bool $bp = false;
    public ?string $sp = "s";
    public function g(): ?int { return 5; }
}
$o = new O();
var_dump([$o->np, $o->nn, $o->bp, $o->sp]);
var_dump(["k" => $o->np, "n" => $o->nn, "b" => $o->bp, "s" => $o->sp]);
var_dump([$o->g()]);
var_dump(["k" => $o->g()]);
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [0]=>\n  int(5)\n}\n\
         array(1) {\n  [\"k\"]=>\n  int(5)\n}\n\
         array(1) {\n  [0]=>\n  NULL\n}\n\
         array(1) {\n  [\"k\"]=>\n  NULL\n}\n\
         array(4) {\n  [0]=>\n  int(5)\n  [1]=>\n  NULL\n  [2]=>\n  bool(false)\n  [3]=>\n  string(1) \"s\"\n}\n\
         array(4) {\n  [\"k\"]=>\n  int(5)\n  [\"n\"]=>\n  NULL\n  [\"b\"]=>\n  bool(false)\n  [\"s\"]=>\n  string(1) \"s\"\n}\n\
         array(1) {\n  [0]=>\n  int(5)\n}\n\
         array(1) {\n  [\"k\"]=>\n  int(5)\n}\n"
    );
}

/// Verifies a class-constant array or scalar and an enum case merge from their declaration.
/// The syntactic fallback answered `Int`, so two array constants failed to lower, a scalar
/// pair printed `string(2) "42"`, and an enum-case pair cast the chosen case object to string.
#[test]
fn test_branch_merges_over_class_constants_and_enum_cases() {
    let out = compile_and_run(
        r#"<?php
const GA = ["g", "h"];
class C {
    const A = ["a", "b"];
    const B = ["c"];
    const N = 42;
    const S = "hi";
    public static function pick(int $n): array { return $n > 0 ? self::A : GA; }
    public static function pickStatic(int $n): array { return $n > 0 ? static::A : GA; }
}
enum Suit { case Hearts; case Spades; }
echo implode(",", $argc > 0 ? C::A : C::B), "\n";
echo implode(",", match ($argc) { 1 => C::A, default => C::B }), "\n";
var_dump($argc > 0 ? C::N : C::S);
var_dump($argc > 0 ? Suit::Hearts : Suit::Spades);
var_dump(match ($argc) { 1 => Suit::Hearts, default => Suit::Spades });
echo implode(",", C::pick($argc)), "|", implode(",", C::pickStatic($argc)), "\n";
echo implode(",", $argc > 0 ? GA : GA), "\n";
"#,
    );
    assert_eq!(
        out,
        "a,b\na,b\nint(42)\nenum(Suit::Hearts)\nenum(Suit::Hearts)\n\
         a,b|a,b\ng,h\n"
    );
}

/// Verifies `?:` keeps a null default: the merge temp used the value's scalar slot, so the
/// null sentinel was read back as `int(9223372036854775806)` instead of `NULL`.
#[test]
fn test_short_ternary_with_a_null_default_stays_nullable() {
    let out = compile_and_run(
        r#"<?php
class C {
    public static int $zero = 0;
    public static int $i = 7;
    public static ?int $n = null;
}
$x = 0;
var_dump($x ?: null);
$y = 5;
var_dump($y ?: null);
$s = "";
var_dump($s ?: null);
$t = "hi";
var_dump($t ?: null);
$b = false;
var_dump($b ?: null);
$on = true;
var_dump($on ?: null);
var_dump(C::$zero ?: null);
var_dump(C::$i ?: null);
var_dump(C::$n ?: null);
"#,
    );
    assert_eq!(
        out,
        "NULL\nint(5)\nNULL\nstring(2) \"hi\"\nNULL\nbool(true)\nNULL\nint(7)\nNULL\n"
    );
}

/// Verifies `??` keeps each taken arm's tag now that an `int`/`bool` join is boxed, and that
/// a nullable static property remains distinguishable from a truthy value.
#[test]
fn test_null_coalesce_keeps_int_and_bool_tags() {
    let out = compile_and_run(
        r#"<?php
class C {
    public static int $i = 7;
    public static bool $flag = false;
    public static ?int $n = null;
}
var_dump(C::$i ?? C::$flag);
var_dump(C::$flag ?? C::$i);
var_dump(C::$n ?? C::$flag);
$x = 5;
$b = false;
var_dump($x ?? true);
var_dump($b ?? $x);
var_dump($argc > 0 ? C::$n : C::$i);
var_dump($argc > 0 ? C::$n : null);
$n = null;
$n ??= 5;
var_dump($n);
$m = 7;
$m ??= 9;
var_dump($m);
C::$n ??= 5;
var_dump(C::$n);
"#,
    );
    assert_eq!(
        out,
        "int(7)\nbool(false)\nbool(false)\nint(5)\nbool(false)\nNULL\nNULL\nint(5)\nint(7)\nint(5)\n"
    );
}

/// Verifies nullable static-property arms keep `null`, `0` and `false` through ternary,
/// `match` and `??` merges, where a narrower merge temp could read a tagged payload as zero
/// or lose the null.
#[test]
fn test_nullable_static_property_arms_through_branch_merges() {
    let out = compile_and_run(
        r#"<?php
class C {
    public static ?int $n5 = 5;
    public static ?int $nn = null;
    public static ?bool $bf = false;
    public static ?string $bs = "s";
}
var_dump($argc > 0 ? C::$n5 : C::$nn);
var_dump($argc > 5 ? C::$n5 : C::$nn);
var_dump(match ($argc) { 1 => C::$n5, default => C::$nn });
var_dump(C::$nn ?? C::$n5);
var_dump(C::$n5 ?? C::$nn);
var_dump($argc > 0 ? C::$bf : C::$bs);
var_dump($argc > 0 ? C::$bs : C::$bf);
"#,
    );
    assert_eq!(
        out,
        "int(5)\nNULL\nint(5)\nint(5)\nint(5)\nbool(false)\nstring(1) \"s\"\n"
    );
}

/// Verifies nullable scalar merges release every boxed temp in a loop, leaving a clean heap.
#[test]
fn test_nullable_scalar_merges_in_a_loop_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public static ?int $n5 = 5; public static ?int $nn = null; }
$total = 0;
for ($i = 0; $i < 20 + $argc; $i++) {
    $n = $i % 2 ? C::$n5 : C::$nn;
    $m = match ($i % 2) { 0 => C::$nn, default => C::$n5 };
    $total += ($n ?? 0) + ($m ?? 0);
}
echo $total;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "100");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies an `int` and a `bool` static-property join keeps each arm's runtime tag in both
/// argument orders and across ternary, `?:` and `match`: letting one side win labelled a taken
/// `int(7)` as `bool(true)` and a taken `false` as `int(0)`.
#[test]
fn test_int_and_bool_static_arms_keep_their_own_tags() {
    let out = compile_and_run(
        r#"<?php
class C {
    public static int $i = 7;
    public static bool $flag = false;
    public static bool $on = true;
}
var_dump($argc > 0 ? C::$i : C::$flag);
var_dump($argc > 0 ? C::$flag : C::$i);
var_dump($argc > 0 ? C::$on : C::$i);
var_dump($argc > 0 ? C::$i : C::$on);
var_dump($argc > 5 ? C::$i : C::$flag);
var_dump($argc > 5 ? C::$flag : C::$i);
var_dump(match ($argc) { 1 => C::$i, default => C::$flag });
var_dump(match ($argc) { 1 => C::$flag, default => C::$i });
var_dump(C::$flag ?: C::$i);
var_dump(C::$i ?: C::$flag);
"#,
    );
    assert_eq!(
        out,
        "int(7)\nbool(false)\nbool(true)\nint(7)\nbool(false)\nint(7)\nint(7)\n\
         bool(false)\nint(7)\nint(7)\n"
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
echo implode(",", $argc > 0 ? $c->$mn() : $c->$mn()), "\n";
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
    assert_eq!(out, "a,b\nx,y\na,b\nx,y\n5\n8\n");
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

/// Verifies a coalesce expression inside a literal or a ternary arm is typed by its lowered
/// branch result, not the syntactic `Int` fallback: an array-valued `??` was cast to its
/// element count in both positions.
#[test]
fn test_coalesce_expressions_keep_arrays_in_literals_and_merges() {
    let out = compile_and_run(
        r#"<?php
$a = ["x"];
$b = ["y"];
var_dump([$a ?? $b]);
$r = $argc > 0 ? ($a ?? $b) : ($b ?? $a);
var_dump($r);
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [0]=>\n  array(1) {\n    [0]=>\n    string(1) \"x\"\n  }\n}\n\
         array(1) {\n  [0]=>\n  string(1) \"x\"\n}\n"
    );
}

/// Verifies literal arms of a ternary and `match` are typed by the literal typers, not the
/// syntactic `Int` guess: string-local elements were stored as `array<int>` and read back as
/// pointer garbage.
#[test]
fn test_array_literal_arms_keep_their_element_types() {
    let out = compile_and_run(
        r#"<?php
$s = "hello"; $t = "world";
var_dump($argc > 0 ? [$s] : [$t]);
$r = match ($argc) { 1 => ["k" => $s], default => ["k" => $t] };
var_dump($r["k"]);
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [0]=>\n  string(5) \"hello\"\n}\nstring(5) \"hello\"\n"
    );
}

/// Verifies an element read through a receiver with no single class (a union) is boxed rather
/// than stamped `int`: `$u->ys()` returned an array that was read back as `int(2)`.
#[test]
fn test_array_literal_call_elements_without_a_single_receiver_class() {
    let out = compile_and_run(
        r#"<?php
class A1 { public function ys(): array { return ["a1", "b1"]; } }
class B1 { public function ys(): array { return ["b1", "b2"]; } }
class M { public static function __callStatic($n, $a): array { return ["static", $n]; } }
$u = $argc > 0 ? new A1() : new B1();
var_dump([$u->ys()]);
var_dump([M::items()]);
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [0]=>\n  array(2) {\n    [0]=>\n    string(2) \"a1\"\n    [1]=>\n    string(2) \"b1\"\n  }\n}\n\
         array(1) {\n  [0]=>\n  array(2) {\n    [0]=>\n    string(6) \"static\"\n    [1]=>\n    string(5) \"items\"\n  }\n}\n"
    );
}

/// Verifies a class constant aliased to another class constant (`const A = self::B`) keeps the
/// aliased value's type: the initializer is a `ScopedConstantAccess`, which the syntactic
/// guess types as `string`, so an array alias was cast with `(string)`.
#[test]
fn test_class_constant_alias_keeps_the_aliased_type() {
    let out = compile_and_run(
        r#"<?php
class C {
    const B = ["x", "y"];
    const A = self::B;
}
var_dump($argc > 0 ? C::A : C::B);
var_dump([C::A]);
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  string(1) \"x\"\n  [1]=>\n  string(1) \"y\"\n}\n\
         array(1) {\n  [0]=>\n  array(2) {\n    [0]=>\n    string(1) \"x\"\n    [1]=>\n    string(1) \"y\"\n  }\n}\n"
    );
}

/// Verifies a missing indexed read merged against a present array stays `NULL` instead of the
/// null-container sentinel being dereferenced as an array header.
#[test]
fn test_missing_array_read_merge_keeps_null() {
    let out = compile_and_run_capture(
        r#"<?php
$rows = [[1, 2]];
$r = $argc > 0 ? $rows[5] : $rows[0];
var_dump($r);
$m = match ($argc) { 1 => $rows[5], default => $rows[0] };
var_dump($m);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\nNULL\n");
    assert!(out.stderr.contains("Undefined array key"), "{}", out.stderr);
}

/// Verifies nullable-scalar literal elements and merges release every boxed cell in a loop,
/// including a `?string` payload that needs a refcounted boxing, not just register-sized ints.
#[test]
fn test_nullable_scalar_literals_in_a_loop_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C {
    public static ?int $n5 = 5;
    public static ?int $nn = null;
    public static ?string $s = "x";
}
$total = 0;
for ($i = 0; $i < 20 + $argc; $i++) {
    $a = [C::$n5, C::$nn];
    $h = ["k" => C::$n5, "n" => C::$nn, "s" => C::$s];
    $p = $i % 2 ? C::$s : "y";
    $total += count($a) + count($h) + strlen($p);
}
echo $total;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "126");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a resource element keeps `PhpType::Resource` storage through the central boxing
/// boundary instead of being collapsed to `Int` or boxed as `Mixed`.
#[test]
fn test_resource_array_element_keeps_its_storage_type() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$r = [fopen("php://memory", "r")];
var_dump(is_resource($r[0]));
fclose($r[0]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies array elements of a declared `array<?int>` parameter box their tagged scalar reads
/// in both literal shapes and through a copied local.
#[test]
fn test_nullable_int_array_parameter_elements_are_boxed() {
    let out = compile_and_run(
        r#"<?php
function f(array<?int> $a): void {
    var_dump([$a[0], $a[1]]);
    var_dump(["k" => $a[0]]);
    $b = [$a[0], $a[1]];
    var_dump($b);
    var_dump($b[0], $b[1]);
    var_dump([...$a]);
}
f([11, null]);
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  int(11)\n  [1]=>\n  NULL\n}\n\
         array(1) {\n  [\"k\"]=>\n  int(11)\n}\n\
         array(2) {\n  [0]=>\n  int(11)\n  [1]=>\n  NULL\n}\nint(11)\nNULL\n\
         array(2) {\n  [0]=>\n  int(11)\n  [1]=>\n  NULL\n}\n"
    );
}

/// Verifies an inherited constant initializer binds `self`/`parent` to the class that DECLARED
/// the constant, not to the subclass the access names, and that global array constants and
/// global aliases keep their container type in a literal element.
#[test]
fn test_class_constant_alias_resolves_against_the_declaring_class() {
    let out = compile_and_run(
        r#"<?php
const GA = ["g", "h"];
const GB = GA;
class C { const B = ["x"]; const A = self::B; }
class D extends C { const B = [1, 2]; }
class P { const B = ["p"]; }
class Q extends P { const A = parent::B; }
interface I { const X = ["i"]; const Y = 42; }
class RI { const X = self::Y; const Y = ["r"]; }
class DI extends RI implements I {}
class G { const X = 1; }
class P2 extends G { const B = parent::X; }
class C2 extends P2 { const X = ["c"]; const A = self::B; }
var_dump([D::A]);
var_dump([Q::A]);
var_dump([GA]);
var_dump([GB]);
var_dump([DI::X]);
var_dump([C2::A]);
var_dump(C2::A);
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [0]=>\n  array(1) {\n    [0]=>\n    string(1) \"x\"\n  }\n}\n\
         array(1) {\n  [0]=>\n  array(1) {\n    [0]=>\n    string(1) \"p\"\n  }\n}\n\
         array(1) {\n  [0]=>\n  array(2) {\n    [0]=>\n    string(1) \"g\"\n    [1]=>\n    string(1) \"h\"\n  }\n}\n\
         array(1) {\n  [0]=>\n  array(2) {\n    [0]=>\n    string(1) \"g\"\n    [1]=>\n    string(1) \"h\"\n  }\n}\n\
         array(1) {\n  [0]=>\n  array(1) {\n    [0]=>\n    string(1) \"r\"\n  }\n}\n\
         array(1) {\n  [0]=>\n  int(1)\n}\n\
         int(1)\n"
    );
}

/// Verifies a class-constant alias written with a namespaced relative class name still types
/// the element from the array it aliases, even when a global class shares the short name.
#[test]
fn test_namespaced_class_constant_alias_keeps_the_aliased_type() {
    let out = compile_and_run(
        r#"<?php
class Bar { const B = 42; }
namespace Foo {
    class Bar { const B = ["a", "b"]; }
    class C { const A = Bar::B; }
}
namespace {
    var_dump([\Foo\C::A]);
}
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [0]=>\n  array(2) {\n    [0]=>\n    string(1) \"a\"\n    [1]=>\n    string(1) \"b\"\n  }\n}\n"
    );
}

/// Verifies `?? null` on a nullable scalar or string keeps the null default: the value's
/// nullability must stay in the merge temp, or the null sentinel is read as an integer or an
/// empty string.
#[test]
fn test_null_coalesce_with_a_null_default_stays_null() {
    let out = compile_and_run(
        r#"<?php
function f(?int $i, ?string $s): void {
    var_dump($i ?? null);
    var_dump($s ?? null);
}
f(null, null);
f(5, "x");
"#,
    );
    assert_eq!(out, "NULL\nNULL\nint(5)\nstring(1) \"x\"\n");
}

/// Verifies nullable `int` and `bool` arms keep the taken arm's tag in both orders: the pair
/// used to share one representation and read `NULL` as an integer or `false` as `int(0)`.
#[test]
fn test_nullable_int_and_bool_arms_keep_their_own_tags() {
    let out = compile_and_run(
        r#"<?php
function f(?int $n, ?bool $b, bool $c): void {
    var_dump($c ? $n : $b);
    var_dump($c ? $b : $n);
}
f(null, false, true);
f(null, false, false);
f(5, false, true);
f(5, false, false);
f(null, true, true);
f(null, true, false);
"#,
    );
    assert_eq!(
        out,
        "NULL\nbool(false)\nbool(false)\nNULL\nint(5)\nbool(false)\n\
         bool(false)\nint(5)\nNULL\nbool(true)\nbool(true)\nNULL\n"
    );
}

/// Verifies `?C ?? new C()` and `?array ?? [...]` release every boxed value in a loop,
/// covering the object `MixedUnbox` arm and the typed-container unbox path under heap debug.
#[test]
fn test_nullable_object_and_array_coalesce_in_a_loop_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public int $v = 7; }
function object_loop(?C $o): int {
    $total = 0;
    for ($i = 0; $i < 20; $i++) {
        $x = $o ?? new C();
        $total += $x->v;
    }
    return $total;
}
function array_loop(?array $a): int {
    $total = 0;
    for ($i = 0; $i < 20; $i++) {
        $r = $a ?? [4, 5];
        $total += $r[0];
    }
    return $total;
}
echo object_loop(null), ":", object_loop(new C()), ":", array_loop(null), ":", array_loop([1, 2]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "140:140:80:20");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}
