//! Purpose:
//! Integration tests for a closure whose body is `return <call>;`. The EIR re-derivation of such a
//! closure's return type consulted only the builtin call-type map, so a USER callee fell through
//! to the syntactic `Int` default and the call's real result (a string, a float, an array) was
//! read back through an int slot with no diagnostic. Method, static-method, captured-closure,
//! expression-call and pipe callees took the same fallback until issues #1269 and #1272.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout.
//! - Expected values are real `LC_ALL=C php` 8.5 output for the same fixtures.
//! - Several fixtures wrap the call in `strlen()`/`count()`, which refuse to lower against a
//!   wrongly-typed operand — so they pin the checker's TYPE, not only the runtime bytes.

use crate::support::*;

/// The reported shape: a closure returning a call to a `: string` function printed `0`.
#[test]
fn test_closure_returning_a_user_call_keeps_the_callee_return_type() {
    let out = compile_and_run(
        r#"<?php
function tag(int $n): string { return "t" . $n; }
$f = function ($v) { return tag($v); };
echo $f(1);
"#,
    );
    assert_eq!(out, "t1");
}

/// `strlen()` over the same call pins the checker's type rather than the bytes.
///
/// Before the fix this did not merely print the wrong value — it refused to compile with
/// "strlen cannot lower checked operand type Int", which is the clearest statement that the
/// call's result really was typed `int`.
#[test]
fn test_closure_call_result_is_typed_string_not_just_printed_as_one() {
    let out = compile_and_run(
        r#"<?php
function tag(int $n): string { return "t" . $n; }
$f = function ($v) { return tag($v); };
echo strlen($f(1)), "|", strtoupper($f(2));
"#,
    );
    assert_eq!(out, "2|T2");
}

/// Arrow bodies, zero-parameter closures and typed parameters take the same path.
///
/// The issue's table showed all three failing, which is what ruled out arity and parameter
/// typing as the cause and pointed at the closure's own return type.
#[test]
fn test_arrow_zero_parameter_and_typed_parameter_closures_all_carry_the_return_type() {
    let out = compile_and_run(
        r#"<?php
function tag(int $n): string { return "t" . $n; }
$arrow = fn ($v) => tag($v);
$none = function () { return tag(3); };
$typed = function (int $v) { return tag($v); };
echo $arrow(1), "|", $none(), "|", $typed(2);
"#,
    );
    assert_eq!(out, "t1|t3|t2");
}

/// A callee with NO declared return type is resolved from its inferred signature.
///
/// This is the row that rules out "read the callee's hint": there is no hint to read, and the
/// answer still has to be the inferred `string` rather than the syntactic `int`.
#[test]
fn test_closure_call_to_an_unhinted_callee_uses_its_inferred_return_type() {
    let out = compile_and_run(
        r#"<?php
function untyped($n) { return "u" . $n; }
$f = function ($v) { return untyped($v); };
echo $f(3), "|", strlen($f(3));
"#,
    );
    assert_eq!(out, "u3|2");
}

/// Non-string return types are carried too: float and array.
///
/// The syntactic default was `int` for everything, so every non-int callee was mistyped, not
/// only string ones. `count()` on the array result pins that side the way `strlen()` pins the
/// string side.
#[test]
fn test_closure_call_carries_float_and_array_return_types() {
    let out = compile_and_run(
        r#"<?php
function flt(int $n): float { return $n + 0.5; }
function arr(int $n): array { return [$n, $n + 1]; }
$f = function ($v) { return flt($v); };
$g = function ($v) { return arr($v); };
echo $f(2), "|", count($g(5)), $g(5)[1];
"#,
    );
    assert_eq!(out, "2.5|26");
}

/// A CONTAINER return is normalized the way ordinary call lowering normalizes it.
///
/// Raised in review. The callee's untyped by-value parameter arrives as a boxed Mixed, so a
/// container built out of it has Mixed elements whatever the signature's inferred element type
/// says — which is exactly what `eir_user_function_return_type` encodes. Copying the raw
/// signature type instead stamped a narrower element type on the closure's contract, and the
/// caller then read the boxed element with the wrong layout: `$b[0]` came back as its own
/// pointer printed as an integer (`int(4363925416)`) instead of the string.
///
/// Worth pinning precisely because the pre-fix behaviour was a hard compile error rather than a
/// wrong answer, so the first version of this change traded a refusal for a silent miscompile.
#[test]
fn test_closure_call_returning_a_container_normalizes_its_element_type() {
    let out = compile_and_run(
        r#"<?php
function values($v) { return [$v]; }
function assoc($v) { return ["k" => $v]; }
$fs = function ($v) { return values($v); };
$ha = function ($v) { return assoc($v); };
$b = $fs("hello");
$e = $ha("zz");
$seen = "";
foreach ($b as $x) { $seen .= $x; }
echo count($b), $b[0], "|", $seen, "|", count($e), $e["k"];
"#,
    );
    assert_eq!(out, "1hello|hello|1zz");
}

/// The same normalization keeps a FLOAT element readable as a float, not as a pointer.
///
/// `var_dump` is the discriminating consumer here: the value printed through `echo` can look
/// plausible while the runtime tag is wrong, and the pre-fix output was `int(4363925608)`.
#[test]
fn test_closure_call_container_keeps_a_float_elements_runtime_tag() {
    let out = compile_and_run(
        r#"<?php
function values($v) { return [$v]; }
$ff = function ($v) { return values($v); };
$c = $ff(2.5);
var_dump($c[0]);
"#,
    );
    assert_eq!(out, "float(2.5)\n");
}

/// An object return survives, and its method can be called through the closure's result.
#[test]
fn test_closure_call_carries_an_object_return_type() {
    let out = compile_and_run(
        r#"<?php
class Box { public function __construct(public int $n) {} public function label(): string { return "b" . $this->n; } }
function make(int $n): Box { return new Box($n); }
$f = function ($v) { return make($v); };
echo $f(7)->label(), "|", $f(7)->n;
"#,
    );
    assert_eq!(out, "b7|7");
}

/// The shapes that already worked must keep working.
///
/// A declared `: string` skips the inference entirely, a local gives the checker a typed
/// binding to read back, a non-call body never reaches the fallback, and an int-valued call is
/// the one case the old default happened to get right.
#[test]
fn test_the_previously_working_closure_shapes_are_unchanged() {
    let out = compile_and_run(
        r#"<?php
function tag(int $n): string { return "t" . $n; }
$declared = function ($v): string { return tag($v); };
$viaLocal = function ($v) { $s = tag($v); return $s; };
$noCall = function ($v) { return "d" . $v; };
$intResult = function ($v) { return strlen(tag($v)); };
echo $declared(2), "|", $viaLocal(4), "|", $noCall(4), "|", $intResult(1);
"#,
    );
    assert_eq!(out, "t2|t4|d4|2");
}

/// A closure returning an ARRAY LITERAL whose elements are user calls resolves each element.
///
/// The element types run through the same helper as the bare return, so an element that is a
/// user call was stamped `int` and read a string payload back as an integer — the array-literal
/// counterpart of the reported bug.
#[test]
fn test_closure_returning_an_array_literal_of_user_calls_types_its_elements() {
    let out = compile_and_run(
        r#"<?php
function tag(int $n): string { return "t" . $n; }
$f = function ($v) { return [tag($v), tag($v + 1)]; };
$r = $f(1);
echo $r[0], "|", $r[1], "|", strlen($r[1]);
"#,
    );
    assert_eq!(out, "t1|t2|2");
}

/// A closure returning an instance or static METHOD call keeps the method's return type.
///
/// Only free functions were resolved, so `$o->greet()`, `Greeter::shout()`, an arrow body, a
/// typed parameter receiver, a chained receiver and `$this->greet()` inside a method all fell to
/// the syntactic `Int` default and printed `0`; the float came back as `int(1)` and the array
/// failed to compile (issue #1269). The nullsafe call hands back its boxed chain result. Reference
/// PHP 8.5 output.
#[test]
fn test_closure_returning_a_method_call_keeps_the_method_return_type() {
    let out = compile_and_run(
        r#"<?php
class Greeter {
    public function greet(string $who): string { return "hi " . $who; }
    public static function shout(string $who): string { return strtoupper($who); }
    public function half(int $n): float { return $n / 2; }
    public function pair(string $a): array { return [$a, $a . $a]; }
    public function me(): Greeter { return $this; }
    public function run(): string {
        $viaThis = function (string $w) { return $this->greet($w); };
        return $viaThis("this");
    }
}
$g = new Greeter();
$instance = function (string $w) use ($g) { return $g->greet($w); };
$static = function (string $w) { return Greeter::shout($w); };
$arrow = fn(string $w) => $g->greet($w);
$param = function (Greeter $h) { return $h->greet("param"); };
$chain = fn(string $w) => $g->me()->greet($w);
$float = fn(int $n) => $g->half($n);
$array = function () use ($g) { return $g->pair("x"); };
$maybe = fn(?Greeter $h) => $h?->greet("n");
echo $instance("bob"), "|", $static("amy"), "|", $arrow("kim"), "|", $param($g), "|", $chain("ann"), "|", $g->run(), "\n";
echo strlen($instance("bob")), "|", implode(",", $array()), "|", count($array()), "\n";
var_dump($float(3), $maybe($g), $maybe(null));
"#,
    );
    assert_eq!(
        out,
        "hi bob|AMY|hi kim|hi param|hi ann|hi this\n6|x,xx|2\nfloat(1.5)\nstring(4) \"hi n\"\nNULL\n"
    );
}

/// A closure returning an EXTERN call keeps the C declaration's return type.
///
/// Extern functions are published in the same signature map as user functions, so this shape
/// already resolved through the user-function lookup; pinned so that path keeps covering it
/// (issue #1272). The message text is libc's, so it is compared against a direct call.
#[test]
fn test_closure_returning_an_extern_call_keeps_the_extern_return_type() {
    let out = compile_and_run(
        r#"<?php
extern function strerror(int $errnum): string;
extern function atof(string $s): float;
$message = function (int $code) { return strerror($code); };
$parse = fn(string $s) => atof($s);
echo $message(2) === strerror(2) ? "same" : "diff", "|", strlen($message(2)) > 0 ? "text" : "empty", "|";
var_dump($parse("2.5"));
"#,
    );
    assert_eq!(out, "same|text|float(2.5)\n");
}

/// A closure returning a call whose callee is only known at runtime returns the boxed result.
///
/// A captured closure, an expression call and a pipe all took the syntactic `Int` default, so a
/// string result printed `0` (issue #1272). The body hands back a boxed dispatch result, which is
/// now what the closure signature carries. Reference PHP 8.5 output.
#[test]
fn test_closure_returning_a_runtime_dispatched_call_boxes_the_result() {
    let out = compile_and_run(
        r#"<?php
function wrap(string $s): string { return "{" . $s . "}"; }
$inner = function (string $w): string { return "<" . $w . ">"; };
$viaCapture = function (string $w) use ($inner) { return $inner($w); };
$maker = fn() => fn(string $s): string => "[" . $s . "]";
$viaExprCall = fn(string $s) => ($maker())($s);
$viaPipe = fn(string $s) => $s |> wrap(...);
echo $viaCapture("a"), $viaExprCall("b"), $viaPipe("c"), "|", strlen($viaCapture("xy"));
"#,
    );
    assert_eq!(out, "<a>[b]{c}|4");
}

/// The checker accepts a string consumer on an element of a closure-returned container.
///
/// Lowering stamps `return values($v);` with the callee's boxed-element container, while the
/// checker typed the element from the untyped parameter's `int` placeholder and refused
/// `strlen($f("hello")[0])` with `strlen() argument must be string` (issue #1270). Reference
/// PHP 8.5 prints `5|ABC|3`.
#[test]
fn test_checker_accepts_string_consumers_on_a_closure_returned_container_element() {
    let out = compile_and_run(
        r#"<?php
function values($v) { return [$v]; }
function pairs($k, $v) { return [$k => $v]; }
$f = function ($v) { return values($v); };
$g = fn($v) => values($v);
$h = fn($k, $v) => pairs($k, $v);
echo strlen($f("hello")[0]), "|", strtoupper($g("abc")[0]), "|", strlen($h("a", "xyz")["a"]);
"#,
    );
    assert_eq!(out, "5|ABC|3");
}

/// The checker widens a container INSIDE a union the same way: a callee returning `[$v]` or
/// `null` makes the closure `?array<mixed>`, so its element reaches a `string` parameter.
///
/// Only a bare array return was widened at first, so this union kept the parameter
/// placeholder's element type and `take($m("hello")[0])` was refused with `Function 'take'
/// parameter $s expects Str, got Union([Int, Void])`, while lowering already boxed the whole
/// result (raised in review). `strlen()` accepts any union, which is why the `string` parameter
/// is the consumer that pins it. Reference PHP 8.5 prints `[hello]|[xyz]|5` and the dump.
#[test]
fn test_checker_widens_a_nullable_container_returned_through_a_closure() {
    let out = compile_and_run(
        r#"<?php
function maybe_values($v) { if ($v === null) { return null; } return [$v]; }
function maybe_pairs($k, $v) { if ($v === null) { return null; } return [$k => $v]; }
function take(string $s): string { return "[" . $s . "]"; }
$m = function ($v) { return maybe_values($v); };
$p = fn($k, $v) => maybe_pairs($k, $v);
echo take($m("hello")[0]), "|", take($p("a", "xyz")["a"]), "|", strlen($m("hello")[0]), "\n";
var_dump($m("q"));
"#,
    );
    assert_eq!(
        out,
        "[hello]|[xyz]|5\narray(1) {\n  [0]=>\n  string(1) \"q\"\n}\n"
    );
}
