//! Purpose:
//! Regression tests for value-producing branch merges (`?:` ternary, short `?:`, `??`, `match`)
//! whose branches have DIFFERENT representations: a branch the pre-lowering type estimate could
//! not name (a property read through a `mixed` or `T|false` receiver, a constant, a static
//! property, a negated float, ...) used to be coerced into the other branch's slot type.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture prints what PHP 8.5 prints; the homogeneous fixtures at the end pin the
//!   pre-existing behaviour of int/int, string/string and object/object merges.

use super::*;

/// Shared declarations: a `mixed` receiver and an `R|false` receiver over the same class.
const PRELUDE: &str = r#"<?php
class R { public $n = "str"; public $i = 5; public static $s = "st"; const CI = 7; }
function gr(): mixed { return new R(); }
function uf(int $k): R|false { return $k > 9 ? false : new R(); }
$t = count($argv) < 9; $f = !$t; $rm = gr(); $ru = uf(1);
"#;

fn run(body: &str) -> String {
    compile_and_run(&format!("{PRELUDE}{body}"))
}

/// `$c ? null : $r->n` over a `mixed` `$r` printed the string payload's address as an int.
#[test]
fn test_ternary_null_or_property_of_mixed_receiver() {
    let out = run(r#"var_dump($f ? null : $rm->n); var_dump($t ? null : $rm->n);"#);
    assert_eq!(out, "string(3) \"str\"\nNULL\n");
}

/// `$r === false ? false : $r->n` over `M|false` printed `int(0)` for a string property.
#[test]
fn test_ternary_false_guard_or_property_of_union_receiver() {
    let out = compile_and_run(
        r#"<?php
class M { public string $n = "M"; public function dup(): M|false { return $this; } }
class S extends M { public function dup(): M|false { return false; } }
function go(M $m) { $r = $m->dup(); var_dump($r === false ? false : $r->n); }
go(new M()); go(new S());
"#,
    );
    assert_eq!(out, "string(1) \"M\"\nbool(false)\n");
}

/// A `static|false` method narrowed by `=== false` (the LSP-family shape).
#[test]
fn test_ternary_static_false_receiver_property() {
    let out = compile_and_run(
        r#"<?php
class M { public $n = "M"; public function dup(): static|false { return false; } }
final class A extends M { public function dup(): static { return $this; } }
function go(M $m) { $r = $m->dup(); var_dump($r === false ? false : $r->n); }
go(new M()); go(new A());
"#,
    );
    assert_eq!(out, "bool(false)\nstring(1) \"M\"\n");
}

/// Two property reads with different runtime types through one `mixed` receiver.
#[test]
fn test_ternary_two_properties_of_mixed_receiver() {
    let out = run(r#"var_dump($t ? $rm->n : $rm->i, $f ? $rm->n : $rm->i);"#);
    assert_eq!(out, "string(3) \"str\"\nint(5)\n");
}

/// Short ternary: the default arm is a property of a `T|false` receiver.
#[test]
fn test_short_ternary_property_of_union_receiver() {
    let out = run(r#"var_dump(0 ?: $ru->n, $rm->i ?: true, $ru->n ?: null);"#);
    assert_eq!(out, "string(3) \"str\"\nint(5)\nstring(3) \"str\"\n");
}

/// Null coalesce: a non-null left operand keeps its own type next to a boxed default.
#[test]
fn test_null_coalesce_mixed_property_default() {
    let out = run(r#"var_dump(true ?? $rm->n, $rm->n ?? 1, $rm?->n ?? null);"#);
    assert_eq!(out, "bool(true)\nstring(3) \"str\"\nstring(3) \"str\"\n");
}

/// `match` arms share the merge: a mixed-receiver property next to an `int` and a `false` arm.
#[test]
fn test_match_arms_mixed_property_int_false() {
    let out = run(
        r#"foreach ([0, 1, 2] as $k) {
    var_dump(match ($k) { 0 => 2, 1 => false, default => $rm->n });
}"#,
    );
    assert_eq!(out, "int(2)\nbool(false)\nstring(3) \"str\"\n");
}

/// `int` and `bool` share a register but not a PHP type; neither may relabel the other.
#[test]
fn test_ternary_int_bool_branches_keep_their_types() {
    let out = run(
        r#"var_dump($t ? 42 : false, $f ? 42 : false, $t ? true : 42, $f ? true : 42);
var_dump(match ($t) { true => 1, default => false });"#,
    );
    assert_eq!(out, "int(42)\nbool(false)\nbool(true)\nint(42)\nint(1)\n");
}

/// Constants, class constants, static properties, negation and `!` beside null/int/false.
#[test]
fn test_ternary_unestimated_expression_kinds() {
    let out = compile_and_run(
        r#"<?php
const CS = "cs";
class K { const CI = 7; public static $s = "st"; }
$t = count($argv) < 9; $fl = 2.5;
var_dump($t ? CS : 42, $t ? K::CI : "s", $t ? K::$s : null, $t ? -$fl : 1, $t ? !$t : 3);
"#,
    );
    assert_eq!(
        out,
        "string(2) \"cs\"\nint(7)\nstring(2) \"st\"\nfloat(-2.5)\nbool(false)\n"
    );
}

/// The merged value in every value context: assignment, return, argument, echo, `===`, and
/// string interpolation.
#[test]
fn test_ternary_union_value_contexts() {
    let out = run(
        r#"function fr($t, $rm) { return $t ? $rm->n : null; }
function fa($x) { var_dump($x); }
$v = $f ? false : $rm->n;
var_dump($v, fr($t, $rm));
fa($t ? $ru->n : 1);
echo $t ? $rm->n : 0, "\n";
var_dump(($t ? $rm->n : null) === "str");
$w = $t ? $ru->n : false;
echo "[$w]\n";
"#,
    );
    assert_eq!(
        out,
        "string(3) \"str\"\nstring(3) \"str\"\nstring(3) \"str\"\nstr\nbool(true)\n[str]\n"
    );
}

/// Merges used as array-literal elements (indexed and associative) keep each arm's value,
/// including a nullable merge, which an array slot holds boxed.
#[test]
fn test_ternary_union_array_literal_elements() {
    let out = run(
        r#"$x = [$t ? $rm->n : null, $t ? 7 : null, $f ?: null, $rm->i ?? "d"];
$y = ["k" => match ($t) { true => $ru->n, default => 0 }, "j" => $f ? 1 : null];
echo json_encode($x), json_encode($y), "\n";
"#,
    );
    assert_eq!(out, "[\"str\",7,null,5]{\"k\":\"str\",\"j\":null}\n");
}

/// Pre-existing behaviour: homogeneous int/int, string/string and object/object merges.
#[test]
fn test_ternary_homogeneous_branches_unchanged() {
    let out = compile_and_run(
        r#"<?php
class P { public function __construct(public int $v) {} }
function pick(bool $c, int $a, int $b): int { return $c ? $a : $b; }
function name(bool $c, string $a, string $b): string { return $c ? $a : $b; }
function obj(bool $c, P $a, P $b): P { return $c ? $a : $b; }
function sum(int $n): int { $s = 0; for ($i = 0; $i < $n; $i++) { $s += $i % 2 ? $i : -$i; } return $s; }
$t = count($argv) < 9;
var_dump(pick($t, 1, 2), pick(!$t, 1, 2), name($t, "a", "b"), obj(!$t, new P(1), new P(2))->v);
var_dump(match ($t) { true => "x", default => "y" }, ($t ? 3 : 0) ?: 9, sum(10));
"#,
    );
    assert_eq!(
        out,
        "int(1)\nint(2)\nstring(1) \"a\"\nint(2)\nstring(1) \"x\"\nint(3)\nint(5)\n"
    );
}

/// Shared declarations for the nullable-object merges: `?X` values are boxed `mixed` cells.
const NULLABLE_OBJECTS: &str = r#"<?php
class X { public $n; function __construct($n) { $this->n = $n; } }
function mo(int $n): ?X { return $n ? new X($n) : null; }
$o = new X(1); $x = new X(2);
"#;

fn run_nullable(body: &str) -> String {
    compile_and_run(&format!("{NULLABLE_OBJECTS}{body}"))
}

/// `?X ?: X` into a local: the non-null arm used to be narrowed to an object slot while its
/// value was still the boxed cell, so `$r->n` read the box pointer.
#[test]
fn test_short_ternary_nullable_object_value_and_object_default() {
    let out = run_nullable(
        r#"function a1(?X $p, X $d) { $r = $p ?: $d; return $r->n; }
function k4(?X $p, X $d) { return ($p ?: $d)->n; }
function k2(bool $c, ?X $p, X $d) { $r = $c ? ($p ?: $d) : $d; return $r->n; }
var_dump(a1($x, $o), a1(null, $o), k4($x, $o), k4(null, $o), k2(true, $x, $o), k2(true, null, $o));
"#,
    );
    assert_eq!(out, "int(2)\nint(1)\nint(2)\nint(1)\nint(2)\nint(1)\n");
}

/// `?X ?? X` (wrong in main too) and both operators as array-literal elements.
#[test]
fn test_null_coalesce_nullable_object_value_and_object_default() {
    let out = run_nullable(
        r#"function a2(?X $p, X $d) { $r = $p ?? $d; return $r->n; }
function px(?X $p, X $d) { return [($p ?: $d)->n, ($p ?? $d)->n]; }
var_dump(a2($x, $o), a2(null, $o));
echo json_encode(px($x, $o)), json_encode(px(null, $o)), "\n";
"#,
    );
    assert_eq!(out, "int(2)\nint(1)\n[2,2][1,1]\n");
}

/// A `?X` call result as the value arm keeps the right object (it printed another class's
/// object and then crashed).
#[test]
fn test_nullable_object_call_result_merges() {
    let out = run_nullable(
        r#"$w = mo(2) ?: $o; $w2 = mo(0) ?: $o; $q = mo(2) ?? $o; $q2 = mo(0) ?? $o;
var_dump($w->n, $w2->n, $q->n, $q2->n, get_class($w), $q instanceof X);
foreach ([0, 2] as $k) { echo (mo($k) ?: $o)->n, (mo($k) ?? $o)->n, "\n"; }
"#,
    );
    assert_eq!(out, "int(2)\nint(1)\nint(2)\nint(1)\nstring(1) \"X\"\nbool(true)\n11\n22\n");
}

/// A `match` with no matching arm and no `default` throws a catchable `UnhandledMatchError`
/// that renders the subject as PHP does.
#[test]
fn test_match_without_default_throws_catchable_unhandled_match_error() {
    let out = compile_and_run(
        r#"<?php
class Foo {}
function m($x) {
    try { return match ($x) { 1 => "a", 2 => 2 }; }
    catch (\UnhandledMatchError $e) { return $e->getMessage(); }
}
var_dump(m(1), m(2), m(3), m("s"), m(1.5), m(true), m(null), m([1]), m(new Foo()));
"#,
    );
    assert_eq!(
        out,
        "string(1) \"a\"\nint(2)\nstring(22) \"Unhandled match case 3\"\n\
         string(24) \"Unhandled match case 's'\"\nstring(24) \"Unhandled match case 1.5\"\n\
         string(25) \"Unhandled match case true\"\nstring(25) \"Unhandled match case NULL\"\n\
         string(34) \"Unhandled match case of type array\"\n\
         string(32) \"Unhandled match case of type Foo\"\n"
    );
}

/// The subject held for the `UnhandledMatchError` message is released when an arm matches, so a
/// destructor runs where PHP runs it, and the heap stays balanced.
#[test]
fn test_match_without_default_releases_subject_on_match() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class D { function __construct(public int $k) {} function __destruct() { echo "d{$this->k}\n"; } }
function f() {
    $o = new D(9); $s = match ($o) { $o => "same" }; echo $s, "\n"; unset($o); echo "after\n";
}
f(); echo "end\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "same\nd9\nafter\nend\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Truthiness of a nullable int (a tagged scalar pair) in `?:`, a ternary condition, `(bool)`,
/// `if` and `!`: it was dispatched as a boxed cell and crashed.
#[test]
fn test_nullable_int_truthiness_in_short_ternary_and_conditions() {
    let out = compile_and_run(
        r#"<?php
class H { public ?int $pi = null; }
function c1(?int $p) { return $p ?: 5; }
function c2(?int $p) { return $p ? "t" : "f"; }
function c3(?int $p) { return (bool) $p; }
function c4(?int $p) { if ($p) { return "yes"; } return "no"; }
var_dump(c1(3), c1(null), c1(0), c2(3), c2(null), c2(0), c3(3), c3(null), c3(0), c4(-1), c4(null));
$h = new H(); var_dump($h->pi ?: 5, !$h->pi); $h->pi = 7; var_dump($h->pi ?: 5, !$h->pi);
"#,
    );
    assert_eq!(
        out,
        "int(3)\nint(5)\nint(5)\nstring(1) \"t\"\nstring(1) \"f\"\nstring(1) \"f\"\nbool(true)\n\
         bool(false)\nbool(false)\nstring(3) \"yes\"\nstring(2) \"no\"\nint(5)\nbool(true)\nint(7)\n\
         bool(false)\n"
    );
}

/// Shared `UnhandledMatchError` message probe over a `mixed` and a typed subject.
const UNHANDLED_MESSAGES: &str = r#"<?php
enum Suit { case Hearts; case Spades; }
enum BS: string { case A = "a"; case B = "b"; }
function m($x) { try { return match ($x) { -999 => 1 }; } catch (\UnhandledMatchError $e) { return $e->getMessage(); } }
"#;

fn run_unhandled(body: &str) -> String {
    compile_and_run(&format!("{UNHANDLED_MESSAGES}{body}"))
}

/// An enum case subject renders as `Class::Case`, as PHP does, not `of type Class`.
#[test]
fn test_unhandled_match_message_renders_enum_case() {
    let out = run_unhandled(
        r#"function te(Suit $x) { try { return match ($x) { Suit::Spades => 1 }; } catch (\UnhandledMatchError $e) { return $e->getMessage(); } }
echo m(Suit::Hearts), "\n", m(BS::A), "\n", te(Suit::Hearts), "\n";
"#,
    );
    assert_eq!(
        out,
        "Unhandled match case Suit::Hearts\nUnhandled match case BS::A\nUnhandled match case Suit::Hearts\n"
    );
}

/// A float subject renders like PHP's precision-14 form with `.0` on integral values.
#[test]
fn test_unhandled_match_message_renders_floats_like_php() {
    let out = run_unhandled(
        r#"function tf(float $x) { try { return match ($x) { 9.5 => 1 }; } catch (\UnhandledMatchError $e) { return $e->getMessage(); } }
foreach ([1.0, -0.0, 1.5, 1e15, 0.30000000000000004, INF, NAN] as $v) { echo m($v), "|"; }
echo tf(-2.0), "|", tf(0.1), "\n";
"#,
    );
    assert_eq!(
        out,
        "Unhandled match case 1.0|Unhandled match case -0.0|Unhandled match case 1.5|\
         Unhandled match case 1.0E+15|Unhandled match case 0.3|Unhandled match case INF|\
         Unhandled match case NAN|Unhandled match case -2.0|Unhandled match case 0.1\n"
    );
}

/// A string subject is quoted, truncated to 15 bytes plus `...`, with `\\` and control
/// escapes applied.
#[test]
fn test_unhandled_match_message_truncates_long_strings() {
    let out = run_unhandled(
        r#"function ts(string $x) { try { return match ($x) { "z" => 1 }; } catch (\UnhandledMatchError $e) { return $e->getMessage(); } }
foreach (["123456789012345", "1234567890123456", "a very long string here", "a'b", "", "ab\\c\n"] as $v) { echo m($v), "|"; }
echo ts("a very long string here"), "\n";
"#,
    );
    assert_eq!(
        out,
        "Unhandled match case '123456789012345'|Unhandled match case '123456789012345...'|\
         Unhandled match case 'a very long str...'|Unhandled match case 'a'b'|\
         Unhandled match case ''|Unhandled match case 'ab\\\\c\\n'|\
         Unhandled match case 'a very long str...'\n"
    );
}
