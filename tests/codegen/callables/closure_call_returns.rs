//! Purpose:
//! Integration tests for a closure whose body is `return <user function call>;`. The EIR
//! re-derivation of such a closure's return type consulted only the builtin call-type map, so a
//! USER callee fell through to the syntactic `Int` default and the call's real result — a string,
//! a float, an array — was read back through an int slot with no diagnostic.
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
